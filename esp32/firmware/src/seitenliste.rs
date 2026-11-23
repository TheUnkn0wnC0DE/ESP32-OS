//! Entwicklung (Browser-Messung BR1/BR2): Seitenliste am Board abarbeiten.
//! Build mit `ESP32OS_SEITENLISTE="url1 url2 …"` (Leerzeichen getrennt) und
//! `ESP32OS_SEITENLISTE_S` (Sekunden je Seite, Standard 60). Nach der
//! WLAN-Verbindung startet je Adresse ein frischer Browser (eigener
//! Datenordner, keine Daten des Nutzers), nach der Zeit wird er beendet –
//! so beeinflussen schwere Seiten die folgenden nicht. Die erste Seite
//! beginnt [`ERSTE_S`] s nach dem Einschalten, Seite i also bei
//! ERSTE_S + i × Zeit (für die Fotozeiten). Log je Seite: „SEITE i/n: url“
//! und am Ende Heap des Browsers.

use std::time::Duration;

/// Start der ersten Seite (s seit dem Einschalten).
pub const ERSTE_S: u64 = 20;

pub fn starten() {
    let Some(liste) = option_env!("ESP32OS_SEITENLISTE") else { return };
    let sek: u64 = option_env!("ESP32OS_SEITENLISTE_S").and_then(|s| s.parse().ok()).unwrap_or(60);
    if !crate::update::psram_faden(c"seitenliste", 8 * 1024, move || ablaufen(liste, sek)) {
        log::error!("SEITE: Faden nicht gestartet");
    }
}

/// Entwicklung (BR1): Heap-Tracing der Systemallokationen über Seite 2–3;
/// danach die noch belegten internen Blöcke mit Aufrufern ausgeben.
#[cfg(esp32os_heap_trace)]
mod spur {
    unsafe extern "C" {
        fn heap_trace_init_standalone(puffer: *mut u8, anzahl: usize) -> i32;
        fn heap_trace_start(modus: u32) -> i32;
        fn heap_trace_stop() -> i32;
        fn heap_trace_get_count() -> usize;
        fn heap_trace_get(index: usize, eintrag: *mut Eintrag) -> i32;
    }
    /// heap_trace_record_t (ESP-IDF 5.5, Stapeltiefe 0, Standalone).
    #[repr(C)]
    struct Eintrag {
        ccount: u32,
        adresse: *const u8,
        groesse: usize,
        frei: bool,
        _naechster: *const u8,
        _voriger: *const u8,
    }
    const ANZAHL: usize = 1500;
    pub fn starten() {
        // SAFETY: Puffer im PSRAM, gross genug für ANZAHL Einträge (≤ 96 Byte).
        unsafe {
            let p = esp_idf_svc::sys::heap_caps_malloc(ANZAHL * 96, esp_idf_svc::sys::MALLOC_CAP_SPIRAM) as *mut u8;
            let a = heap_trace_init_standalone(p, ANZAHL);
            let b = heap_trace_start(1); // HEAP_TRACE_LEAKS
            log::warn!("HEAPSPUR: gestartet ({a}, {b})");
        }
    }
    /// Noch belegte interne Blöcke nach Inhalt gruppiert (Anfang als Text).
    pub fn ausgeben() {
        use std::collections::HashMap;
        let mut gruppen: HashMap<String, (usize, usize)> = HashMap::new();
        // SAFETY: Tracing gestoppt; die Blöcke sind noch belegt (nur lesen).
        unsafe {
            heap_trace_stop();
            for i in 0..heap_trace_get_count() {
                let mut e = Eintrag { ccount: 0, adresse: core::ptr::null(), groesse: 0, frei: false, _naechster: core::ptr::null(), _voriger: core::ptr::null() };
                if heap_trace_get(i, &mut e) != 0 || e.adresse.is_null() || e.frei {
                    continue;
                }
                let a = e.adresse as usize;
                if !(0x4FF0_0000..0x4FFC_0000).contains(&a) {
                    continue; // nur interner RAM
                }
                let n = e.groesse.min(40);
                let text: String = core::slice::from_raw_parts(e.adresse, n)
                    .iter()
                    .map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' })
                    .collect();
                let g = gruppen.entry(format!("{} B: {text}", e.groesse)).or_default();
                g.0 += 1;
                g.1 += e.groesse;
            }
        }
        let mut v: Vec<_> = gruppen.into_iter().collect();
        v.sort_by(|a, b| b.1.1.cmp(&a.1.1));
        let summe: usize = v.iter().map(|x| x.1.1).sum();
        log::warn!("HEAPSPUR: {} Gruppen, {summe} Byte intern noch belegt", v.len());
        for (k, (anz, bytes)) in v.iter().take(60) {
            log::warn!("HEAPSPUR {bytes:6} B {anz:4}x  {k}");
        }
    }
}

fn browser_pid() -> Option<u32> {
    kern::liste().into_iter().find(|p| p.name == "browser").map(|p| p.pid)
}

fn ablaufen(liste: &'static str, sek: u64) {
    let seiten: Vec<&str> = liste.split_whitespace().collect();
    let n = seiten.len();
    for (i, url) in seiten.iter().enumerate() {
        // feste Startzeit je Seite
        let start_us = (ERSTE_S + i as u64 * sek) * 1_000_000;
        let jetzt = crate::hw::now_us() as u64;
        if start_us > jetzt {
            std::thread::sleep(Duration::from_micros(start_us - jetzt));
        }
        // ein noch laufender Browser (vorige Seite) muss weg sein
        for _ in 0..50 {
            if browser_pid().is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        #[cfg(esp32os_heap_trace)]
        if i == 1 {
            spur::starten();
        }
        log::info!("SEITE {}/{n}: {url}", i + 1);
        let r = kern::datei::starten("/sys/browser", &format!("--daten=/daten/netsurf_test {url}"));
        if r < 0 {
            log::error!("SEITE {}/{n}: Browser nicht gestartet ({r})", i + 1);
            continue;
        }
        let ende_us = start_us + (sek - 3) * 1_000_000;
        let jetzt = crate::hw::now_us() as u64;
        if ende_us > jetzt {
            std::thread::sleep(Duration::from_micros(ende_us - jetzt));
        }
        match kern::liste().into_iter().find(|p| p.name == "browser") {
            Some(p) => {
                log::info!(
                    "SEITE {}/{n} Ende: Browser-Heap belegt {} KB, Spitze {} KB von {} KB",
                    i + 1,
                    p.heap_belegt / 1024,
                    p.heap_spitze / 1024,
                    p.heap_groesse / 1024
                );
                let z = kern::zwangsbeenden(p.pid, 2000);
                std::thread::sleep(Duration::from_secs(2));
                // SAFETY: reine Abfrage des Heaps.
                let intern = unsafe { esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_INTERNAL) } / 1024;
                log::info!("SEITE {}/{n}: Browser beendet ({z:?}), danach {intern} KB interner RAM frei", i + 1);
            }
            None => log::warn!("SEITE {}/{n} Ende: Browser läuft nicht mehr (beendet oder abgestürzt)", i + 1),
        }
    }
    #[cfg(esp32os_heap_trace)]
    {
        std::thread::sleep(Duration::from_secs(15));
        spur::ausgeben();
    }
    log::info!("SEITE: Liste fertig ({n} Seiten)");
}
