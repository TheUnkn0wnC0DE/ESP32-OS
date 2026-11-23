//! P5-Nachweis (PC-Teil): Energie-Seite per Touch (Bildschirm aus nach 30 s,
//! Ruhezustand nach 1 min), Werte gespeichert und nach „Neustart“ wieder da,
//! der Desktop meldet sie der Firmware (`Spiel`); „Ruhezustand“ im
//! Startmenü; Akkustand in der Taskleiste, sobald die Firmware ihn misst.
//! Das Abschalten/Aufwachen selbst prüft der Board-Lauf.

use core::sync::atomic::Ordering::Relaxed;
use desktop::einstellungen::{KH, KW, kachel_lage};
use desktop::gesten::Gesten;
use desktop::konfig;
use kern::{KernAlloc, sys};
use konsole::Spiel;
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

fn foto(name: &str) -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
    px
}

fn ini(w: &Path) -> String {
    std::fs::read_to_string(w.join("settings/esp32os.ini")).unwrap_or_default()
}

#[test]
fn energie_ruhezustand_akku() {
    let wurzel = std::env::temp_dir().join("esp32os_p5_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let mut d = desktop::Desktop::neu(|| 0);
    assert_eq!(d.bildschirm_aus_s(), Some(120), "Standard: 2 min");
    assert_eq!(d.ruhe_nach_s(), Some(300), "Standard: 5 min");
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // Energie-Seite: Bildschirm aus nach 30 s, Ruhezustand nach 1 min
    kern::starte_name("einstellungen").unwrap();
    bis(|| sys::fenster_liste().iter().any(|f| f.titel == "Settings"), "Einstellungen");
    let (x, y) = kachel_lage(8);
    f.tippen(5 + x + KW / 2, 48 + y + KH / 2);
    f.tippen(5 + 10 + 48, 48 + 82 + 22);
    f.tippen(5 + 10 + 48, 48 + 164 + 22);
    bis(|| ini(&wurzel).contains("screen_off=30") && ini(&wurzel).contains("sleep_after=60"), "gespeichert");
    foto("p5_energie");
    assert_eq!(d.bildschirm_aus_s(), Some(30));
    assert_eq!(d.ruhe_nach_s(), Some(60));
    // „Nie“ = keine Zeit
    f.tippen(5 + 10 + 5 * 102 + 48, 48 + 82 + 22);
    bis(|| d.bildschirm_aus_s().is_none(), "nie");
    f.tippen(5 + 10 + 48, 48 + 82 + 22);
    bis(|| d.bildschirm_aus_s() == Some(30), "wieder 30 s");
    f.tippen(640 - 5 - 30, 24);
    bis(|| !sys::fenster_liste().iter().any(|f| f.titel == "Settings"), "zu");

    // „Neustart“: Werte aus der ini
    konfig::BILDSCHIRM_AUS.store(999, Relaxed);
    konfig::RUHE_NACH.store(999, Relaxed);
    konfig::laden();
    assert_eq!((d.bildschirm_aus_s(), d.ruhe_nach_s()), (Some(30), Some(60)));

    // Startmenü → „Sleep“: die Firmware erhält einmal „Ruhe jetzt“
    f.tippen(40, 360 - 20);
    bis(|| sys::fenster_liste().iter().any(|f| f.titel == "ESP32-OS"), "Startmenü");
    foto("p5_menue");
    let (x, y) = desktop::shell::menue_eintrag_mitte("@ruhe").unwrap();
    f.tippen(x, y);
    bis(|| konfig::RUHE_JETZT.load(Relaxed), "Ruhe gewünscht");
    assert!(d.ruhe_jetzt(), "einmal true");
    assert!(!d.ruhe_jetzt(), "danach false");

    // Akkustand in der Taskleiste (Firmware mit Spannungsteiler)
    let ohne = foto("p5_leiste_ohne_akku");
    d.akku(Some(76));
    std::thread::sleep(Duration::from_millis(800));
    let mit = foto("p5_leiste_akku");
    // Taskleiste ohne die Uhr (rechte 60 px), sonst stört ein Minutenwechsel
    let leiste = |px: &[u32]| px[(360 - 48) * 640..].iter().enumerate().filter(|(i, _)| i % 640 < 580).map(|(_, p)| *p).collect::<Vec<_>>();
    assert_ne!(leiste(&ohne), leiste(&mit), "Akkuanzeige erscheint");
    d.akku(None);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(leiste(&foto("p5_leiste_wieder_ohne")), leiste(&ohne), "ohne Messung keine Anzeige");
}
