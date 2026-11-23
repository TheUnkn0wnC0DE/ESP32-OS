//! A1-Nachweis (PC): Notizen per Touch. Vorhandene Notiz antippen → Text
//! geladen; „Neu“, Text antippen → Bildschirmtastatur, „Hallo Welt“,
//! Eingabe, „Zeile 2“ über die Tasten der Bildschirmtastatur, „Speichern“ →
//! Datei mit genau diesem Inhalt im Notizen-Ordner (eingehängter Ordner =
//! SD-Karte).

use desktop::gesten::Gesten;
use desktop::notizen::{FX, FY, LISTE_X, LISTE_Y, NEU, SPEICHERN, TEXT_X, TEXT_Y, ZEILE_H};
use desktop::tastatur::{HOEHE, sondertaste, tastenmitte};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}; Fenster: {:?}", titel());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn titel() -> Vec<String> {
    sys::fenster_liste().iter().map(|f| f.titel.clone()).collect()
}

fn hat(t: &str) -> bool {
    titel().iter().any(|x| x == t)
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
        std::thread::sleep(Duration::from_millis(120));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
}

/// Arbeitsbereich des Fensters (Rand 5, Kopf 48).
const AX: i32 = FX + 5;
const AY: i32 = FY + 48;

/// Oberkante der Bildschirmtastatur – wie `tastatur::dienst` sie legt.
fn tastatur_oben() -> i32 {
    let (h, unten) = (360, 360 - desktop::shell::LEISTE_H - HOEHE);
    match sys::tastatur_soll().1 {
        Some(f) if f.y1 > unten && f.y0 >= HOEHE => 0,
        Some(f) if f.y1 > unten => (f.y1 + 2).min(h - HOEHE),
        _ => unten,
    }
}

fn tippe_text(f: &mut Finger, text: &str) {
    for c in text.chars() {
        let oben = tastatur_oben();
        if c == '\n' {
            let (x, y) = sondertaste("enter", 640);
            f.tippen(x, oben + y);
            continue;
        }
        let (ebene, x, y) = tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        if ebene == "gross" {
            let (sx, sy) = sondertaste("shift", 640);
            f.tippen(sx, oben + sy);
        }
        f.tippen(x, oben + y);
        // Tastatur folgt der Zeile der Schreibmarke
        std::thread::sleep(Duration::from_millis(120));
    }
}

#[test]
fn notizen_oeffnen_bearbeiten_speichern() {
    let wurzel = std::env::temp_dir().join("esp32os_a1_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("sys/settings")).unwrap();
    std::fs::write(wurzel.join("sys/settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    std::fs::create_dir_all(wurzel.join("sd/Notizen")).unwrap();
    std::fs::write(wurzel.join("sd/Notizen/Einkauf.txt"), "Milch\nBrot").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", wurzel.join("sys"));
    kern::datei::einhaengen("/rd/1", wurzel.join("sys"));
    kern::datei::einhaengen("/sd0/1", wurzel.join("sd"));
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    assert!(kern::datei::starten("/sys/notizen", "/sd0/1/Notizen") > 0);
    bis(|| hat("Notes - new"), "Notizen offen");
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // vorhandene Notiz öffnen
    f.tippen(AX + LISTE_X + 40, AY + LISTE_Y + ZEILE_H / 2);
    bis(|| hat("Notes - Einkauf.txt"), "Notiz geöffnet");
    foto("a1_geoeffnet");

    // neue Notiz, Text antippen → Tastatur
    f.tippen(AX + NEU.0 + NEU.1 / 2, AY + 20);
    bis(|| hat("Notes - new"), "neue Notiz");
    f.tippen(AX + TEXT_X + 200, AY + TEXT_Y + 60);
    bis(|| sys::tastatur_soll().0, "Tastatur bei Fokus");
    std::thread::sleep(Duration::from_millis(300));
    tippe_text(&mut f, "Hallo Welt\nZeile 2");
    bis(|| hat("Notes - new *"), "Text geändert");
    foto("a1_getippt");

    // speichern
    f.tippen(AX + SPEICHERN.0 + SPEICHERN.1 / 2, AY + 20);
    bis(|| hat("Notes - Hallo Welt.txt"), "gespeichert");
    let inhalt = std::fs::read_to_string(wurzel.join("sd/Notizen/Hallo Welt.txt")).expect("Datei auf der SD-Karte");
    assert_eq!(inhalt, "Hallo Welt\nZeile 2");
    assert_eq!(std::fs::read_to_string(wurzel.join("sd/Notizen/Einkauf.txt")).unwrap(), "Milch\nBrot", "andere Notiz unverändert");
    assert!(!sys::tastatur_soll().0, "Tastatur nach dem Speichern zu");
    foto("a1_gespeichert");
}
