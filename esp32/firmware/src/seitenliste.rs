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
    log::info!("SEITE: Liste fertig ({n} Seiten)");
}
