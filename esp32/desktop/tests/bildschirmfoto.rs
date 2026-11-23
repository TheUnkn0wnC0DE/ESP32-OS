//! P6-Nachweis (PC): Bildschirmfoto per langem Druck auf die Uhr der
//! Taskleiste und über das Startmenü – PNG im Ordner Screenshots der
//! SD-Karte, gelesen und mit dem Bildschirm verglichen.

use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(8), "Zeitüberschreitung: {was}; Fenster: {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn halten(&mut self, x: i32, y: i32, ms: u64) {
        let mut d = 0;
        while d <= ms {
            self.g.schritt(self.t + d, [Some((x, y)), None]);
            d += 50;
            std::thread::sleep(Duration::from_millis(5));
        }
        self.t += ms + 50;
        self.g.schritt(self.t, [None, None]);
        std::thread::sleep(Duration::from_millis(60));
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        self.t += 600;
        std::thread::sleep(Duration::from_millis(220));
    }
    fn tippen(&mut self, x: i32, y: i32) {
        self.halten(x, y, 80);
    }
}

fn dateien(o: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(o).map(|d| d.map(|e| e.unwrap().path()).collect()).unwrap_or_default();
    v.sort();
    v
}

fn pruefen(datei: &Path) {
    let d = std::fs::read(datei).unwrap();
    let (_, v) = kolibri::dekoder::dekodieren(&d).expect("PNG lesbar");
    assert_eq!((v[0].w, v[0].h), (640, 360));
    // Desktop oben rechts und die Taskleiste links sind im Bild wie am Schirm
    let (w, _, px) = sys::foto();
    for (x, y) in [(600usize, 40usize), (20, 340)] {
        assert_eq!(v[0].px[y * w + x] & 0xFF_FFFF, px[y * w + x] & 0xFF_FFFF, "Pixel ({x}, {y})");
    }
}

#[test]
fn bildschirmfoto_per_uhr_und_menue() {
    let wurzel = std::env::temp_dir().join("esp32os_p6_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sd0/1", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    for p in ["@icon", "@taskbar"] {
        kern::starte_name(p).unwrap();
    }
    std::thread::sleep(Duration::from_millis(800));
    let ordner = wurzel.join("Screenshots");
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // langer Druck auf die Uhr (rechts in der Taskleiste)
    f.halten(640 - 30, 360 - 24, 700);
    bis(|| dateien(&ordner).len() == 1, "Bildschirmfoto über die Uhr");
    pruefen(&dateien(&ordner)[0]);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Screenshot"), "Meldung");
    let n = std::fs::metadata(&dateien(&ordner)[0]).unwrap().len();
    assert!(n < 60_000, "PNG klein: {n} Bytes");
    // Meldung schliessen (OK), dann über das Startmenü
    let ok = sys::fenster_liste().into_iter().find(|w| w.titel == "Screenshot").unwrap();
    let (mx, my, mw, mh) = kern::kern_abschnitt(|| kern::fenster::mit_welt(|w| w.slots[ok.slot].as_ref().and_then(|s| s.fenster.as_ref()).map(|f| (f.x, f.y, f.w, f.h)).unwrap()));
    f.tippen(mx + mw - 10 - 55 - 10, my + mh - 50);
    bis(|| !sys::fenster_liste().iter().any(|w| w.titel == "Screenshot"), "Meldung zu");
    f.tippen(40, 360 - 20);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "ESP32-OS"), "Startmenü");
    let (x, y) = desktop::shell::menue_eintrag_mitte("@screenshot").unwrap();
    f.tippen(x, y);
    bis(|| dateien(&ordner).len() == 2, "Bildschirmfoto über das Menü");
    let zweites = dateien(&ordner)[1].clone();
    pruefen(&zweites);
    // Das Menü ist auf dem Foto nicht mehr zu sehen (vor der Aufnahme geschlossen)
    let d = std::fs::read(&zweites).unwrap();
    let (_, v) = kolibri::dekoder::dekodieren(&d).unwrap();
    assert!(v[0].px[(my as usize + 60) * 640 + 100] & 0xFF_FFFF != 0xF4F4F4, "Menü nicht im Bild");
    let ziel = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet/p6_screenshot.png");
    std::fs::copy(&zweites, ziel).unwrap();
}
