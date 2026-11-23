//! A2-Nachweis (PC): Rechner per Touch – „12 × 3,5 =“ ergibt 42.

use desktop::gesten::Gesten;
use desktop::rechner::{FX, FY, anzeige_zuletzt, tastenmitte};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}; Anzeige {:?}", anzeige_zuletzt());
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
        std::thread::sleep(Duration::from_millis(120));
    }
}

/// Arbeitsbereich des Fensters (Rand 5, Kopf 48).
const AX: i32 = FX + 5;
const AY: i32 = FY + 48;

#[test]
fn rechner_12_mal_3_5() {
    let wurzel = std::env::temp_dir().join("esp32os_a2_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    kern::starte_name("rechner").unwrap();
    bis(|| anzeige_zuletzt() == "0", "Rechner offen");
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    for c in "12*3,5=".chars() {
        let (x, y) = tastenmitte(c).unwrap();
        f.tippen(AX + x, AY + y);
    }
    bis(|| anzeige_zuletzt() == "42", "12 × 3,5 = 42");
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join("a2_rechner.png"), spatz::image::encode_png(w, h, &px)).unwrap();
}
