//! P1b-Nachweis (PC): Gespeicherte Einstellungen (`/sys/settings/esp32os.ini`,
//! wie nach einem Neustart) schalten die Oberfläche auf Deutsch und die
//! Bildschirmtastatur auf US. Getippt wird über die US-Belegung; die Taste
//! „US“/„CH“ schaltet um und speichert das Layout.

use desktop::dateimanager::{LEISTE, SEITE_W};
use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, Layout, layout, sondertaste, tastenmitte};
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
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(250));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
}

fn da(titel: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel == titel)
}

#[test]
fn deutsch_und_us_tastatur_aus_den_einstellungen() {
    let wurzel = std::env::temp_dir().join("esp32os_p1b_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nlanguage=de\nkeyboard=us\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    // @start liest die Einstellungen und startet Symbole, Taskleiste, Tastatur
    kern::starte_name("@start").unwrap();
    bis(|| kern::sprache::aktuell() == kern::sprache::Sprache::Deutsch, "Sprache aus der ini");
    assert_eq!(layout(), Layout::Us, "Layout aus der ini");
    std::thread::sleep(Duration::from_millis(2500));
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // Startmenü auf Deutsch
    f.tippen(40, 360 - 20);
    bis(|| da("ESP32-OS"), "Startmenü");
    foto("p1b_de_menue");
    f.tippen(600, 100);

    // Dateimanager auf Deutsch (nur /sys eingehängt → Gerät „System“)
    kern::starte_name("dateimanager").unwrap();
    bis(|| da("Dateimanager - /sys"), "Dateimanager deutsch");
    foto("p1b_de_dateimanager");

    // „Neuer Ordner“ → Dialog mit Bildschirmtastatur (US-Belegung)
    f.tippen(5 + SEITE_W + 4 + 86 + 65, 48 + 2 + 21);
    bis(|| sys::tastatur_soll().0, "Tastatur");
    std::thread::sleep(Duration::from_millis(300));
    let y_unten = 360 - desktop::shell::LEISTE_H - HOEHE;
    let oben = match sys::tastatur_soll().1 {
        Some(c) if c.y1 > y_unten && c.y0 >= HOEHE => 0,
        Some(c) if c.y1 > y_unten => (c.y1 + 2).min(360 - HOEHE),
        _ => y_unten,
    };
    // „y“ und „z“ liegen auf US anders als auf CH
    for c in "yz".chars() {
        let (ebene, x, y) = tastenmitte(c, 640).unwrap();
        assert_eq!(ebene, "normal");
        f.tippen(x, oben + y);
    }
    foto("p1b_tastatur_us");
    // Layout-Taste: US → CH, gespeichert
    let (lx, ly) = sondertaste("layout", 640);
    f.tippen(lx, oben + ly);
    bis(|| layout() == Layout::Ch, "Layout CH");
    let ini = std::fs::read_to_string(wurzel.join("settings/esp32os.ini")).unwrap();
    assert!(ini.contains("keyboard=ch") && ini.contains("language=de"), "gespeichert: {ini}");
    // OK → Ordner „yz“ (über die US-Belegung getippt)
    f.tippen(20 + 5 + 380 + 45, 48 + 3 + 21);
    bis(|| wurzel.join("yz").is_dir(), "Ordner yz angelegt");
    let _ = LEISTE;
}
