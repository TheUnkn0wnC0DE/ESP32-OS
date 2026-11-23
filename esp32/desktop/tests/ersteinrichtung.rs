//! P4-Nachweis (PC): Ohne gespeicherte Einstellungen startet `@start` die
//! Ersteinrichtung. Per Touch: Deutsch, US-Tastatur, WLAN (W3: Netz wählen,
//! Passwort über die Bildschirmtastatur, verbunden und gemerkt; Treiber
//! simuliert), Jahr + 1, Uhr stellen, Fertig → Werte in der ini, `setup=1`,
//! danach kein Assistent mehr.

use desktop::ersteinrichtung::weiter_lage;
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}; Fenster: {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn tippen(&mut self, x: i32, y: i32) {
        let mut d = 0;
        while d <= 80 {
            self.g.schritt(self.t + d, [Some((x, y)), None]);
            d += 50;
            std::thread::sleep(Duration::from_millis(5));
        }
        self.t += 130;
        self.g.schritt(self.t, [None, None]);
        std::thread::sleep(Duration::from_millis(60));
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        self.t += 600;
        std::thread::sleep(Duration::from_millis(220));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(250));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn da(titel: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel == titel)
}

fn netz(ssid: &str, rssi: i32, s: kern::wlan::Sicherheit, ax: bool) -> kern::wlan::Netz {
    kern::wlan::Netz { ssid: ssid.into(), rssi, kanal: 1, sicherheit: s, ax }
}

static SIM: std::sync::OnceLock<&'static kern::wlan::Simulation> = std::sync::OnceLock::new();

fn sim_suchen() -> u32 {
    SIM.get().map_or(0, |s| s.suchen_zahl())
}

/// Reicht an die Simulation weiter (sie bleibt für die Prüfungen greifbar).
struct Weiter(&'static kern::wlan::Simulation);
impl kern::wlan::Treiber for Weiter {
    fn zustand(&self) -> kern::wlan::Zustand {
        self.0.zustand()
    }
    fn suchen(&self) -> Result<Vec<kern::wlan::Netz>, i32> {
        self.0.suchen()
    }
    fn verbinden(&self, s: &str, p: &str) -> Result<(), i32> {
        self.0.verbinden(s, p)
    }
    fn trennen(&self) {
        self.0.trennen()
    }
    fn laden(&self) -> Vec<(String, String)> {
        self.0.laden()
    }
    fn sichern(&self, n: &[(String, String)]) -> bool {
        self.0.sichern(n)
    }
}

/// Text über die Bildschirmtastatur tippen (Kleinbuchstaben).
fn tippe(f: &mut Finger, text: &str) {
    use desktop::tastatur::{HOEHE, tastenmitte};
    bis(|| sys::tastatur_soll().0, "Tastatur erscheint");
    std::thread::sleep(Duration::from_millis(300));
    let y_unten = 360 - desktop::shell::LEISTE_H - HOEHE;
    let oben = match sys::tastatur_soll().1 {
        Some(c) if c.y1 > y_unten && c.y0 >= HOEHE => 0,
        Some(c) if c.y1 > y_unten => (c.y1 + 2).min(360 - HOEHE),
        _ => y_unten,
    };
    for c in text.chars() {
        let (ebene, x, y) = tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        assert_eq!(ebene, "normal", "{c}");
        f.tippen(x, oben + y);
    }
}

#[test]
fn ersteinrichtung_per_touch() {
    let wurzel = std::env::temp_dir().join("esp32os_p4_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    // WLAN wie am Board (simuliert), „Firmware“-Faden mit wache()
    let sim: &'static kern::wlan::Simulation = Box::leak(Box::new(kern::wlan::Simulation::neu(vec![
        (netz("Heimnetz", -50, kern::wlan::Sicherheit::Wpa2Wpa3, true), "sommerwind".into()),
        (netz("Gast", -66, kern::wlan::Sicherheit::Offen, false), String::new()),
    ])));
    SIM.set(sim).ok();
    assert!(kern::wlan::treiber_setzen(Box::new(Weiter(sim))));
    std::thread::spawn(|| {
        let t0 = Instant::now();
        loop {
            kern::wlan::wache(t0.elapsed().as_millis() as u64);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    assert!(desktop::konfig::einrichtung_noetig(), "frisches System braucht die Einrichtung");
    kern::starte_name("@start").unwrap();
    bis(|| da("Welcome to ESP32-OS"), "Assistent erscheint beim ersten Start");
    std::thread::sleep(Duration::from_millis(300));
    foto("p4_sprache");
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    let (wx, wy, ww, wh) = weiter_lage();
    let weiter = (5 + wx + ww / 2, 48 + wy + wh / 2);

    // 1 Sprache: Deutsch
    f.tippen(5 + 240 + 100, 48 + 100 + 28);
    bis(|| da("Willkommen bei ESP32-OS"), "Deutsch");
    f.tippen(weiter.0, weiter.1);
    // 2 Tastatur: US
    std::thread::sleep(Duration::from_millis(200));
    f.tippen(5 + 300 + 100, 48 + 100 + 28);
    bis(|| desktop::tastatur::layout() == desktop::tastatur::Layout::Us, "US");
    foto("p4_tastatur");
    f.tippen(weiter.0, weiter.1);
    // 3 WLAN (W3): Netz wählen, Passwort über die Bildschirmtastatur
    bis(|| sim_suchen() >= 1, "WLAN-Schritt sucht Netze");
    std::thread::sleep(Duration::from_millis(400));
    foto("p4_wlan");
    f.tippen(5 + 8 + 200, 48 + 54 + 22 + 17);
    bis(|| da("Passwort für Heimnetz"), "Passwort-Dialog");
    tippe(&mut f, "sommerwind");
    f.tippen(20 + 5 + 380 + 45, 48 + 3 + 21);
    bis(|| kern::wlan::zustand().verbunden, "verbunden");
    bis(|| kern::wlan::bekannte() == ["Heimnetz"], "gemerkt");
    std::thread::sleep(Duration::from_millis(600));
    foto("p4_wlan_verbunden");
    f.tippen(weiter.0, weiter.1);
    // 4 Uhr: Jahr + 1, „Uhr stellen“
    let jahr = |u: i64| kern::zeit::datum_aus_tagen(kern::zeit::lokal(u).div_euclid(86_400)).0;
    let vorher = jahr(kern::zeit::unix());
    std::thread::sleep(Duration::from_millis(200));
    f.tippen(5 + 250 + 50, 48 + 78 + 21);
    foto("p4_uhr");
    f.tippen(weiter.0, weiter.1);
    bis(|| jahr(kern::zeit::unix()) == vorher + 1, "Uhr gestellt");
    // 5 Fertig
    foto("p4_fertig");
    f.tippen(weiter.0, weiter.1);
    bis(|| !da("Willkommen bei ESP32-OS"), "Assistent geschlossen");
    let ini = std::fs::read_to_string(wurzel.join("settings/esp32os.ini")).unwrap();
    for k in ["setup=1", "language=de", "keyboard=us", "clock="] {
        assert!(ini.contains(k), "{k} in {ini}");
    }
    assert!(!desktop::konfig::einrichtung_noetig(), "nach dem Abschluss kein Assistent mehr");
    foto("p4_desktop");
}
