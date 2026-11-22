//! K7-Nachweis libini (Rust-Schnittstelle; C-Exporte rufen dieselben Funktionen).

use kern::KernAlloc;
use kolibri::libini as ini;

#[global_allocator]
static A: KernAlloc = KernAlloc;

#[test]
fn lesen_schreiben_aufzaehlen() {
    let dir = std::env::temp_dir().join(format!("esp32os_ini_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.ini"), "; Kommentar\r\n[Anzeige]\r\nHelligkeit=7\r\n\r\n[WLAN]\r\nSSID = Heim\r\n").unwrap();
    kern::init(&kern::PC, &[]);
    kern::datei::einhaengen("/sys", &dir);
    let f = "/sys/t.ini";
    assert_eq!(ini::zahl(f, "anzeige", "HELLIGKEIT", 0), 7);
    assert_eq!(ini::lesen(f, "WLAN", "ssid").as_deref(), Some("Heim"));
    assert!(ini::zahl_setzen(f, "Anzeige", "Helligkeit", 9));
    assert!(ini::schreiben(f, "Anzeige", "Modus", "dunkel"));
    assert!(ini::farbe_setzen(f, "Farben", "Hintergrund", 0x3A6EA5));
    assert_eq!(ini::farbe(f, "Farben", "Hintergrund", 0), 0x3A6EA5);
    assert_eq!(ini::zahl(f, "Anzeige", "Helligkeit", 0), 9);
    assert_eq!(ini::sektionen(f), ["Anzeige", "WLAN", "Farben"]);
    assert_eq!(ini::eintraege(f, "Anzeige"), [("Helligkeit".into(), "9".into()), ("Modus".into(), "dunkel".into())]);
    let text = std::fs::read_to_string(dir.join("t.ini")).unwrap();
    assert!(text.starts_with("; Kommentar"), "Kommentar bleibt: {text}");
    assert!(ini::sektion_loeschen(f, "WLAN"));
    assert_eq!(ini::lesen(f, "WLAN", "SSID"), None);
    assert_eq!(ini::zahl(f, "Fehlt", "x", -3), -3);
    let _ = std::fs::remove_dir_all(&dir);
}

/// V2 (Auftrag 13.7): Stromausfall an jeder Stelle des Speicherns
/// nachgestellt (der Prüf-Haken bricht per Panik ab, so wie ein Neustart den
/// Ablauf abbricht). Danach sind alle alten Werte da, der neue Wert ist ganz
/// oder gar nicht da, und das nächste Speichern verliert nichts. Dazu:
/// beschädigte Datei → Sicherungskopie gilt; nur Beschädigtes → nichts wird
/// überschrieben; vier Schreiber gleichzeitig → kein Wert geht verloren.
#[test]
fn absturzsicher_speichern() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static ZIEL: AtomicUsize = AtomicUsize::new(0);
    thread_local! {
        // nur dieser Testfaden (der Haken ist global, andere Tests laufen parallel)
        static AKTIV: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    fn haken(n: usize) {
        if AKTIV.with(|a| a.get()) && n == ZIEL.load(Ordering::SeqCst) {
            panic!("Stromausfall nach Schritt {n}");
        }
    }
    let dir = std::env::temp_dir().join(format!("esp32os_ini_strom_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    kern::init(&kern::PC, &[]);
    kern::datei::einhaengen("/strom", &dir);
    ini::stromausfall_haken(haken);
    AKTIV.with(|a| a.set(true));
    let f = "/strom/e.ini";
    let neu_anfangen = || {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            std::fs::remove_file(e.path()).unwrap();
        }
        assert!(ini::schreiben(f, "system", "language", "de"));
        assert!(ini::schreiben(f, "system", "brightness", "10"));
    };
    for k in 1..=ini::SCHRITTE {
        neu_anfangen();
        ZIEL.store(k, Ordering::SeqCst);
        let r = std::panic::catch_unwind(|| ini::schreiben(f, "system", "volume", "6"));
        ZIEL.store(0, Ordering::SeqCst);
        assert!(r.is_err(), "Schritt {k}: abgebrochen");
        assert_eq!(ini::lesen(f, "system", "language").as_deref(), Some("de"), "Schritt {k}");
        assert_eq!(ini::zahl(f, "system", "brightness", 0), 10, "Schritt {k}");
        let vol = ini::lesen(f, "system", "volume");
        // bis Schritt 4 gilt die alte Datei, ab 5 die fertige neue
        assert_eq!(vol.as_deref(), if k >= 5 { Some("6") } else { None }, "Schritt {k}");
        assert!(ini::schreiben(f, "system", "touchpad", "0"), "Schritt {k}: weiter speichern");
        assert_eq!(ini::lesen(f, "system", "language").as_deref(), Some("de"), "Schritt {k} danach");
        assert_eq!(ini::zahl(f, "system", "brightness", 0), 10, "Schritt {k} danach");
        assert_eq!(ini::zahl(f, "system", "touchpad", 9), 0, "Schritt {k} danach");
        assert_eq!(ini::lesen(f, "system", "volume"), vol, "Schritt {k}: nichts verloren");
        assert!(!dir.join("e.ini.neu").exists(), "Schritt {k}: aufgeräumt");
        assert!(dir.join("e.ini.bak").exists(), "Schritt {k}: Sicherungskopie");
    }
    // beschädigte Datei (ein Byte verändert) → Sicherungskopie gilt
    neu_anfangen();
    assert!(ini::schreiben(f, "system", "volume", "6"));
    let mut b = std::fs::read(dir.join("e.ini")).unwrap();
    let i = b.iter().position(|&c| c == b'd').unwrap();
    b[i] = b'x';
    std::fs::write(dir.join("e.ini"), &b).unwrap();
    assert_eq!(ini::lesen(f, "system", "language").as_deref(), Some("de"), "aus .bak");
    assert_eq!(ini::lesen(f, "system", "volume"), None, ".bak ist die Fassung davor");
    assert!(ini::schreiben(f, "system", "volume", "7"));
    assert_eq!(ini::lesen(f, "system", "language").as_deref(), Some("de"));
    assert_eq!(ini::zahl(f, "system", "volume", 0), 7);
    // nur Beschädigtes: nicht überschreiben
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        std::fs::remove_file(e.path()).unwrap();
    }
    std::fs::write(dir.join("e.ini"), "[system]\r\nlanguage=de\r\n; esp32os-pruefsumme=00000000 5\r\n").unwrap();
    assert!(!ini::schreiben(f, "system", "volume", "1"), "Beschädigtes nicht überschreiben");
    assert!(std::fs::read_to_string(dir.join("e.ini")).unwrap().contains("language=de"));
    // ältere Datei ohne Prüfsumme gilt und bekommt beim Speichern eine
    std::fs::write(dir.join("e.ini"), "[system]\r\nlanguage=de\r\n").unwrap();
    assert!(ini::schreiben(f, "system", "volume", "1"));
    assert!(std::fs::read_to_string(dir.join("e.ini")).unwrap().contains("esp32os-pruefsumme="));
    assert_eq!(ini::lesen(f, "system", "language").as_deref(), Some("de"));
    // vier Schreiber gleichzeitig (Einstellungen + Uhr-Sicherung …)
    let faeden: Vec<_> = (0..4)
        .map(|t| {
            std::thread::spawn(move || {
                for n in 0..40 {
                    assert!(ini::zahl_setzen(f, "parallel", &format!("t{t}"), n));
                }
            })
        })
        .collect();
    for h in faeden {
        h.join().unwrap();
    }
    for t in 0..4 {
        assert_eq!(ini::zahl(f, "parallel", &format!("t{t}"), -1), 39, "Schreiber {t}");
    }
    assert_eq!(ini::lesen(f, "system", "language").as_deref(), Some("de"), "nichts verloren");
}
