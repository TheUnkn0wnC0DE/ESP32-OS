//! V2-Nachweis am Board (Test-Build `ESP32OS_STROMAUSFALL=1`): Stromausfall
//! während des Speicherns der Einstellungen. Bei jedem Start wird das
//! Speichern eines Testwerts an Schritt k (1 … `libini::SCHRITTE`)
//! abgebrochen – der Prüf-Haken startet das Board mitten im Ablauf neu, so
//! wie ein Stromausfall das Schreiben abbricht (der FAT-Zwischenspeicher wird
//! dabei nicht mehr geschrieben). Nach jedem Neustart prüft der Test, dass
//! die Sektion „system“ unverändert ist (Prüfsumme vom ersten Start, im NVS)
//! und der Testwert ganz oder gar nicht geschrieben wurde.

use crate::store::Nvs;
use esp_idf_svc::sys;
use konsole::start::Speicher;
use std::sync::atomic::{AtomicUsize, Ordering};

/// NVS-Schlüssel je Testdurchgang (`ESP32OS_STROMAUSFALL=<Durchgang>`), damit
/// sich ein Durchgang neu starten lässt.
const DURCHGANG: &str = match option_env!("ESP32OS_STROMAUSFALL") {
    Some(d) => d,
    None => "1",
};

fn schluessel() -> String {
    format!("stromtest{DURCHGANG}")
}
const REFERENZ: &str = "stromref";

static ABBRUCH: AtomicUsize = AtomicUsize::new(0);

fn haken(n: usize) {
    if n == ABBRUCH.load(Ordering::SeqCst) {
        log::warn!("Stromausfall-Test: Neustart nach Schritt {n} des Speicherns");
        // SAFETY: gewollter Neustart (Test-Build).
        unsafe { sys::esp_restart() };
    }
}

/// Sektion „system“ ohne die Uhrzeit (wird nebenher gesichert) als Text.
fn system_stand(datei: &str) -> String {
    let mut e = kolibri::libini::eintraege(datei, "system");
    e.retain(|(k, _)| !k.eq_ignore_ascii_case("clock"));
    e.sort();
    e.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(";")
}

pub fn lauf(datei: &str) {
    let Ok(nvs) = Nvs::open() else {
        log::error!("Stromausfall-Test: NVS fehlt");
        return;
    };
    let n = nvs.lesen(&schluessel()).and_then(|b| b.first().copied()).unwrap_or(0) as usize;
    let stand = system_stand(datei);
    if n == 0 {
        nvs.schreiben(REFERENZ, stand.as_bytes());
        log::warn!("Stromausfall-Test Durchgang {DURCHGANG}: Start");
        log::warn!("Stromausfall-Test: Referenz {} Einträge: {stand}", stand.split(';').count());
    } else {
        let referenz = nvs.lesen(REFERENZ).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let test = kolibri::libini::lesen(datei, "test", &schluessel());
        // Testwert: Jeder Versuch wurde abgebrochen. Bis Schritt 4 gilt die
        // alte Datei (kein Testwert), ab Schritt 5 die fertige neue (n).
        let erwartet = (n >= 5).then(|| n.to_string());
        let ok_test = test == erwartet;
        log::warn!(
            "Stromausfall-Test: nach Abbruch bei Schritt {n}: Einstellungen {}, Testwert {:?} ({})",
            if stand == referenz { "unverändert" } else { "VERÄNDERT" },
            test,
            if ok_test { "wie erwartet" } else { "FALSCH" }
        );
        if stand != referenz {
            log::error!("Stromausfall-Test: erwartet {referenz}, gelesen {stand}");
        }
    }
    if n >= kolibri::libini::SCHRITTE {
        log::warn!("Stromausfall-Test fertig: alle {} Schritte unterbrochen", kolibri::libini::SCHRITTE);
        return;
    }
    nvs.schreiben(&schluessel(), &[(n + 1) as u8]);
    ABBRUCH.store(n + 1, Ordering::SeqCst);
    kolibri::libini::stromausfall_haken(haken);
    let ok = kolibri::libini::schreiben(datei, "test", &schluessel(), &(n + 1).to_string());
    // Nur erreicht, wenn der Haken nicht ausgelöst hat (sollte nicht sein)
    log::error!("Stromausfall-Test: Schritt {} nicht erreicht (speichern {ok})", n + 1);
}
