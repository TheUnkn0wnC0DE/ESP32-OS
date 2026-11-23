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

    /// Wischen von a nach b in 200 ms (10 Schritte), dann loslassen.
    fn wischen(&mut self, a: (i32, i32), b: (i32, i32)) {
        for i in 0..=10 {
            let p = (a.0 + (b.0 - a.0) * i / 10, a.1 + (b.1 - a.1) * i / 10);
            self.g.schritt(self.t, [Some(p), None]);
            self.t += 20;
            std::thread::sleep(Duration::from_millis(15));
        }
        self.g.schritt(self.t, [None, None]);
        self.t += 800;
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
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
    // A-Review: zu grosse Datei (300 KB) und mehr Notizen als Listenzeilen
    let gross = "x".repeat(99) + "\n";
    std::fs::write(wurzel.join("sd/Notizen/Gross.txt"), gross.repeat(3000)).unwrap();
    for n in ["Z1", "Z2", "Z3"] {
        std::fs::write(wurzel.join(format!("sd/Notizen/{n}.txt")), n).unwrap();
    }
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

    // A-Review: Ungespeichertes erst beim zweiten Antippen von „Neu“ weg
    f.tippen(AX + TEXT_X + 200, AY + TEXT_Y + 60);
    bis(|| sys::tastatur_soll().0, "Tastatur bei Fokus");
    std::thread::sleep(Duration::from_millis(300));
    tippe_text(&mut f, "!");
    bis(|| hat("Notes - Hallo Welt.txt *"), "geändert");
    f.tippen(AX + NEU.0 + NEU.1 / 2, AY + 20);
    std::thread::sleep(Duration::from_millis(300));
    assert!(hat("Notes - Hallo Welt.txt *"), "erstes Antippen verwirft nicht: {:?}", titel());
    f.tippen(AX + NEU.0 + NEU.1 / 2, AY + 20);
    bis(|| hat("Notes - new"), "zweites Antippen verwirft");
    assert_eq!(std::fs::read_to_string(wurzel.join("sd/Notizen/Hallo Welt.txt")).unwrap(), "Hallo Welt\nZeile 2", "Datei unverändert");

    // A-Review: Datei über 64 KB nur lesen – Tippen ändert nichts,
    // „Speichern“ schreibt sie nicht gekürzt zurück
    // Liste: Einkauf, Gross, Hallo Welt, Z1 (4 Zeilen sichtbar)
    f.tippen(AX + LISTE_X + 40, AY + LISTE_Y + ZEILE_H + ZEILE_H / 2);
    bis(|| hat("Notes - Gross.txt"), "grosse Datei geöffnet");
    f.tippen(AX + TEXT_X + 200, AY + TEXT_Y + 60);
    bis(|| sys::tastatur_soll().0, "Tastatur bei Fokus");
    std::thread::sleep(Duration::from_millis(300));
    tippe_text(&mut f, "a");
    std::thread::sleep(Duration::from_millis(300));
    assert!(hat("Notes - Gross.txt"), "nur lesen, nicht geändert: {:?}", titel());
    f.tippen(AX + SPEICHERN.0 + SPEICHERN.1 / 2, AY + 20);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(std::fs::metadata(wurzel.join("sd/Notizen/Gross.txt")).unwrap().len(), 300_000, "grosse Datei unverändert");

    // A-Review: Wischen blättert die Liste (6 Notizen, 4 Zeilen) – danach ist
    // die vierte Zeile „Z3“
    let liste = (AX + LISTE_X + 60, AY + LISTE_Y + 2 * ZEILE_H);
    f.wischen((liste.0, liste.1 + 60), (liste.0, liste.1 - 60));
    f.tippen(AX + LISTE_X + 40, AY + LISTE_Y + 3 * ZEILE_H + ZEILE_H / 2);
    bis(|| hat("Notes - Z3.txt"), "nach dem Wischen: Z3 in der vierten Zeile");
}
