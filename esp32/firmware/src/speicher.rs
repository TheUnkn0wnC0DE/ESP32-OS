//! ESP32-OS: Speicher für das Dateisystem des Kerns (K6) einhängen:
//! interne FAT-Partition `daten` (/daten) als `/sys` und `/rd/1`,
//! microSD-Karte (/sdcard) als `/sd0/1`.

use core::ffi::c_char;

unsafe extern "C" {
    fn esp32os_sd_einbinden(ort: *const c_char, mit_ldo: i32) -> i32;
    fn esp32os_sd_mb() -> u32;
}

/// Grösse der eingehängten SD-Karte in MB (0 = keine).
pub fn sd_mb() -> u32 {
    // SAFETY: reine Abfrage.
    unsafe { esp32os_sd_mb() }
}

/// Speicherplatz (Gesamt, frei) eines Host-Pfads („/sdcard“, „/daten“).
fn speicherplatz(host: &std::path::Path) -> Option<(u64, u64)> {
    unsafe extern "C" {
        fn esp32os_fat_info(ort: *const core::ffi::c_char, gesamt: *mut u64, frei: *mut u64) -> i32;
    }
    let ort = std::ffi::CString::new(host.to_str()?).ok()?;
    let (mut g, mut f) = (0u64, 0u64);
    // SAFETY: gültige C-Zeichenkette, Ausgaben auf lokale Variablen.
    (unsafe { esp32os_fat_info(ort.as_ptr(), &mut g, &mut f) } == 0).then_some((g, f))
}

/// Nur die SD-Karte unter `/sdcard` einbinden (auch für Doom/Quake ohne
/// Kern). 0 = ok, sonst ESP-Fehlercode. Mehrfacher Aufruf schadet nicht.
pub fn sd_einbinden() -> i32 {
    use std::sync::atomic::{AtomicI32, Ordering};
    static ERGEBNIS: AtomicI32 = AtomicI32::new(-1);
    let alt = ERGEBNIS.load(Ordering::Relaxed);
    if alt != -1 {
        return alt;
    }
    // Erst mit LDO-Versorgung (P4-Referenzschaltung), sonst ohne.
    // SAFETY: einmal beim Start, Pfad lebt statisch.
    let mut r = unsafe { esp32os_sd_einbinden(c"/sdcard".as_ptr(), 1) };
    if r != 0 {
        log::warn!("SD-Karte mit LDO 4: Fehler 0x{r:x} – zweiter Versuch ohne LDO");
        // SAFETY: wie oben.
        r = unsafe { esp32os_sd_einbinden(c"/sdcard".as_ptr(), 0) };
    }
    ERGEBNIS.store(r, Ordering::Relaxed);
    r
}

pub fn einhaengen() {
    kern::datei::speicher_info_setzen(speicherplatz);
    if crate::daten::einbinden() {
        kern::datei::einhaengen("/sys", "/daten");
        kern::datei::einhaengen("/rd/1", "/daten");
    }
    let r = sd_einbinden();
    if r == 0 {
        kern::datei::einhaengen("/sd0/1", "/sdcard");
        log::info!("SD-Karte eingehängt: {} MB als /sd0/1", sd_mb());
    } else {
        log::warn!("Keine SD-Karte (Fehler 0x{r:x}) – /sd0/1 fehlt");
    }
}

/// V3 (Auftrag 13.6, Test-Build `ESP32OS_SD_AUFRAEUMEN=1`): die SD-Testreste
/// löschen – genau der leere Ordner `z`, `autoboot (Kopie).txt` und die
/// Bildschirmfotos (PNG) in `/Screenshots`. Verzeichnisse vorher und nachher
/// ins Log. Sonst wird nichts angefasst.
pub fn netsurf_testreste_loeschen() {
    // Entwicklungs-Build `ESP32OS_NETSURF_RESTE` (B1, 22.11.): Der Browser
    // schrieb beim ersten Board-Versuch seine Ressourcen nach /daten/netsurf
    // (Kern /sys/netsurf); heute liest er sie aus dem Abbild (/nsres).
    // Gelöscht werden nur Dateien mit genau diesen Namen, dann leere Ordner;
    // alles andere bleibt und steht im Log.
    const OBEN: &[&str] = &[
        "adblock.css", "credits.html", "default.css", "favicon.png", "internal.css", "licence.html", "netsurf.png", "quirks.css",
        "welcome.html", "stand",
    ];
    fn erwartet(rel: &str) -> bool {
        match rel.split_once('/') {
            None => OBEN.contains(&rel),
            Some(("de" | "en", n)) => n == "welcome.html" || n == "Messages",
            Some(("icons", n)) => !n.contains('/') && n.ends_with(".png"),
            _ => false,
        }
    }
    fn dateien(pfad: &str, rel: &str, v: &mut Vec<(String, u64)>) {
        let Ok(d) = std::fs::read_dir(pfad) else { return };
        for e in d.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let r = if rel.is_empty() { n.clone() } else { format!("{rel}/{n}") };
            match e.metadata() {
                Ok(m) if m.is_dir() => dateien(&format!("{pfad}/{n}"), &r, v),
                Ok(m) => v.push((r, m.len())),
                Err(_) => {}
            }
        }
    }
    let wurzel = "/daten/netsurf";
    let mut v = Vec::new();
    dateien(wurzel, "", &mut v);
    v.sort();
    let summe: u64 = v.iter().map(|e| e.1).sum();
    log::warn!("NetSurf-Testreste – vorher {} Dateien, {summe} Byte: {:?}", v.len(), v.iter().map(|e| e.0.as_str()).collect::<Vec<_>>());
    for (rel, _) in &v {
        if erwartet(rel) {
            if let Err(e) = std::fs::remove_file(format!("{wurzel}/{rel}")) {
                log::warn!("  {rel}: {e}");
            }
        } else {
            log::warn!("  {rel}: unerwartet – bleibt");
        }
    }
    for d in ["icons", "de", "en", ""] {
        let p = if d.is_empty() { wurzel.to_string() } else { format!("{wurzel}/{d}") };
        if std::fs::read_dir(&p).is_ok_and(|mut x| x.next().is_none()) {
            log::warn!("  Ordner {p}: {:?}", std::fs::remove_dir(&p).map_err(|e| e.to_string()));
        }
    }
    let mut nachher = Vec::new();
    dateien(wurzel, "", &mut nachher);
    log::warn!("NetSurf-Testreste – nachher {} Dateien, Ordner vorhanden: {}", nachher.len(), std::fs::metadata(wurzel).is_ok());
}

pub fn sd_testreste_loeschen() {
    fn liste(pfad: &str) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(pfad)
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }
    let wurzel = "/sdcard";
    let fotos = "/sdcard/Screenshots";
    log::warn!("SD aufräumen – vorher: {:?}", liste(wurzel));
    log::warn!("SD aufräumen – vorher Screenshots: {:?}", liste(fotos));
    let z = format!("{wurzel}/z");
    match std::fs::read_dir(&z).map(|d| d.count()) {
        Ok(0) => log::warn!("  z (leerer Ordner): {:?}", std::fs::remove_dir(&z).map_err(|e| e.to_string())),
        Ok(n) => log::warn!("  z enthält {n} Einträge – nicht gelöscht"),
        Err(_) => log::warn!("  z nicht vorhanden"),
    }
    let kopie = format!("{wurzel}/autoboot (Kopie).txt");
    log::warn!("  autoboot (Kopie).txt: {:?}", std::fs::remove_file(&kopie).map_err(|e| e.to_string()));
    for n in liste(fotos) {
        if n.to_ascii_lowercase().ends_with(".png") {
            log::warn!("  Screenshots/{n}: {:?}", std::fs::remove_file(format!("{fotos}/{n}")).map_err(|e| e.to_string()));
        }
    }
    log::warn!("SD aufräumen – nachher: {:?}", liste(wurzel));
    log::warn!("SD aufräumen – nachher Screenshots: {:?}", liste(fotos));
}
