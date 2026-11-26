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

/// Dateien, die übernommene KolibriOS-Programme in /sys erwarten (einmal
/// anlegen bzw. erneuern, wenn der Inhalt nicht stimmt): „Bild-Demo“
/// (libimg_demo) liest logo.png – ohne sie endete das Programm sofort
/// (Stabilitätsrunde 24.11.). Verglichen wird der ganze Inhalt, nicht nur
/// die Grösse: am Board (27.11.) hatte die Datei die richtige Grösse, liess
/// sich aber nur bis 4096 Byte lesen (ein Cluster) – die Bild-Demo stürzte
/// bei jedem Start ab.
fn sys_dateien_ablegen() {
    const DATEIEN: &[(&str, &[u8])] = &[("logo.png", include_bytes!("../../kolibri/daten/logo.png"))];
    for (n, d) in DATEIEN {
        let p = format!("/daten/{n}");
        let ist = std::fs::read(&p);
        if ist.as_deref().is_ok_and(|i| i == *d) {
            continue;
        }
        match &ist {
            Ok(i) => log::warn!("/sys/{n}: Inhalt weicht ab ({} von {} Byte lesbar) – neu geschrieben", i.len(), d.len()),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => log::warn!("/sys/{n} nicht lesbar ({e}) – neu geschrieben"),
            Err(_) => {}
        }
        // erst löschen: eine beschädigte Clusterkette wird so freigegeben
        let _ = std::fs::remove_file(&p);
        match std::fs::write(&p, d) {
            Ok(()) if std::fs::read(&p).is_ok_and(|i| i == *d) => log::info!("/sys/{n} angelegt ({} Byte)", d.len()),
            Ok(()) => log::warn!("/sys/{n} geschrieben, aber beim Nachlesen falsch"),
            Err(e) => log::warn!("/sys/{n} nicht angelegt: {e}"),
        }
    }
}

pub fn einhaengen() {
    kern::datei::speicher_info_setzen(speicherplatz);
    if crate::daten::einbinden() {
        kern::datei::einhaengen("/sys", "/daten");
        kern::datei::einhaengen("/rd/1", "/daten");
        sys_dateien_ablegen();
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

/// Entwicklungs-Build `ESP32OS_TESTNOTIZEN_LOESCHEN` (Q1 Fund 5, OK des
/// Nutzers 25.11.): die Test-Notizen früherer Board-Läufe in
/// `ESP32-OS/Notizen` löschen. Gelöscht wird nur eine Datei, deren Name mit
/// „qasdfghjkl1234567890qwer“ beginnt (vom Autopiloten getippter Titel, auch
/// „… (Kopie).txt“) und deren Inhalt diese Folge enthält und sonst nur aus
/// Kleinbuchstaben, Ziffern und Leerraum besteht. Alle anderen Notizen
/// bleiben; von ihnen steht nur Name und Grösse im Log.
pub fn testnotizen_loeschen() {
    const ORDNER: &str = "/sdcard/ESP32-OS/Notizen";
    const FOLGE: &str = "qasdfghjkl1234567890qwer";
    fn testnotiz(name: &str, t: &str) -> bool {
        name.starts_with(FOLGE) && t.contains(FOLGE) && t.chars().all(|c| c.is_whitespace() || c.is_ascii_lowercase() || c.is_ascii_digit())
    }
    let Ok(d) = std::fs::read_dir(ORDNER) else {
        log::warn!("Test-Notizen: Ordner {ORDNER} fehlt");
        return;
    };
    let mut namen: Vec<(String, u64)> = d.flatten().filter_map(|e| Some((e.file_name().to_string_lossy().into_owned(), e.metadata().ok().filter(|m| m.is_file())?.len()))).collect();
    namen.sort();
    log::warn!("Test-Notizen – vorher {} Dateien in {ORDNER}", namen.len());
    for (n, laenge) in &namen {
        let p = format!("{ORDNER}/{n}");
        let inhalt = if *laenge <= 4096 { std::fs::read_to_string(&p).unwrap_or_default() } else { String::new() };
        if testnotiz(n, &inhalt) {
            log::warn!("  {n} ({laenge} Byte, Inhalt {:?}): gelöscht {:?}", inhalt.trim(), std::fs::remove_file(&p).map_err(|e| e.to_string()));
        } else {
            log::warn!("  {n} ({laenge} Byte): bleibt");
        }
    }
    let nachher = std::fs::read_dir(ORDNER).map(|d| d.count()).unwrap_or(0);
    log::warn!("Test-Notizen – nachher {nachher} Dateien");
}

/// Entwicklungs-Builds: eigene Testdateien in einen Testordner
/// `ESP32-OS/<ordner>` der SD-Karte legen – nur in den eigenen Ordner von
/// ESP32-OS, vorhandene gleich grosse Dateien bleiben.
#[cfg(any(esp32os_musik_test, esp32os_pdf_test))]
fn testdaten_ablegen(was: &str, ordner: &str, dateien: &[(&str, &[u8])]) {
    if std::fs::metadata("/sdcard/ESP32-OS").is_err() {
        log::warn!("{was}: kein Ordner ESP32-OS auf der SD-Karte");
        return;
    }
    let ordner = format!("/sdcard/ESP32-OS/{ordner}");
    let _ = std::fs::create_dir_all(&ordner);
    for (n, d) in dateien {
        let p = format!("{ordner}/{n}");
        if std::fs::metadata(&p).is_ok_and(|m| m.len() == d.len() as u64) {
            log::warn!("{was}: {n} schon da");
            continue;
        }
        log::warn!("{was}: {n} ({} Byte): {:?}", d.len(), std::fs::write(&p, d).map_err(|e| e.to_string()));
    }
}

/// Entwicklungs-Build (A5): die eigenen Testtöne (`werkzeuge/test_mp3.py`)
/// nach `ESP32-OS/Musik_Test`.
#[cfg(esp32os_musik_test)]
pub fn musik_testdaten() {
    testdaten_ablegen("Musik-Test", "Musik_Test", &[
        ("Testton 1.mp3", include_bytes!("../../desktop/tests/daten/Testton 1.mp3")),
        ("Testton 2.mp3", include_bytes!("../../desktop/tests/daten/Testton 2.mp3")),
    ]);
}

/// Entwicklungs-Build (Z1): die Test-PDFs des PDF-Betrachters
/// (`desktop/tests/pdf`) nach `ESP32-OS/PDF_Test`.
#[cfg(esp32os_pdf_test)]
pub fn pdf_testdaten() {
    testdaten_ablegen("PDF-Test", "PDF_Test", &[
        ("bericht.pdf", include_bytes!("../../desktop/tests/pdf/bericht.pdf")),
        ("diagramm.pdf", include_bytes!("../../desktop/tests/pdf/diagramm.pdf")),
    ]);
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
