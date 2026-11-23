//! A4-Nachweis (PC): Kalender aus dem Startmenü, Monatswechsel per
//! Wischen (links = nächster, rechts = voriger Monat, je Bewegung genau
//! einer) und über die Knöpfe; der Monatsname springt auf heute.

use desktop::gesten::Gesten;
use desktop::kalender::{self, FX, FY, knopf_mitte, monat_zuletzt};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}; Monat {:?}", monat_zuletzt());
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
        std::thread::sleep(Duration::from_millis(150));
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
        // länger als die Pause, nach der eine neue Wischbewegung zählt
        std::thread::sleep(Duration::from_millis(500));
    }

    fn knopf(&mut self, k: u32) {
        let (x, y) = knopf_mitte(k);
        self.tippen(FX + 5 + x, FY + 48 + y);
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

#[test]
fn kalender_wischen_und_knoepfe() {
    let wurzel = std::env::temp_dir().join("esp32os_a4_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    // 23.11.2026, 12:00 lokal
    let tag = kern::zeit::tage_aus_datum(2026, 11, 23) * 86_400;
    kern::zeit::unix_setzen(tag + 12 * 3600 - (kern::zeit::lokal(tag) - tag));
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));

    let mut f = Finger { g: Gesten::default(), t: 1000 };
    f.tippen(40, 340);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "ESP32-OS"), "Startmenü");
    let (x, y) = desktop::shell::menue_eintrag_mitte("kalender").unwrap();
    f.tippen(x, y);
    bis(|| monat_zuletzt() == (2026, 11), "November 2026");
    foto("a4_november");

    // Mitte des Rasters, 150 px nach links: nächster Monat (nur einer)
    let mitte = (FX + 5 + 195, FY + 48 + 150);
    f.wischen(mitte, (mitte.0 - 150, mitte.1));
    bis(|| monat_zuletzt() == (2026, 12), "Wischen links → Dezember");
    foto("a4_dezember");
    f.wischen(mitte, (mitte.0 + 150, mitte.1));
    bis(|| monat_zuletzt() == (2026, 11), "Wischen rechts → November");
    f.wischen(mitte, (mitte.0 + 150, mitte.1));
    bis(|| monat_zuletzt() == (2026, 10), "Wischen rechts → Oktober");
    // nach oben wischen: nächster Monat
    f.wischen(mitte, (mitte.0, mitte.1 - 120));
    bis(|| monat_zuletzt() == (2026, 11), "Wischen oben → November");
    // Knöpfe: > > (Januar 2027), Monatsname → heute
    f.knopf(kalender::K_VOR);
    f.knopf(kalender::K_VOR);
    bis(|| monat_zuletzt() == (2027, 1), "> > → Januar 2027");
    foto("a4_januar");
    f.knopf(kalender::K_ZURUECK);
    bis(|| monat_zuletzt() == (2026, 12), "< → Dezember");
    f.knopf(kalender::K_HEUTE);
    bis(|| monat_zuletzt() == (2026, 11), "Monatsname → heute");
}
