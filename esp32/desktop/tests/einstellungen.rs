//! P3-Nachweis (PC): Einstellungen per Touch – jede Seite bedient, Werte in
//! `/sys/settings/esp32os.ini` gespeichert und nach einem nachgestellten
//! Neustart (`konfig::laden`) wieder da. Hintergrund: Verlauf und Bild aus
//! Datei (Dateidialog per Touch), Desktop danach fotografiert.

use core::sync::atomic::Ordering::Relaxed;
use desktop::einstellungen::{KH, KW, kachel_lage};
use desktop::gesten::Gesten;
use desktop::konfig::{self, Hintergrund};
use kern::{KernAlloc, sys};
use std::path::{Path, PathBuf};
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
    std::thread::sleep(Duration::from_millis(250));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
    px
}

fn ini(wurzel: &Path) -> String {
    std::fs::read_to_string(wurzel.join("settings/esp32os.ini")).unwrap_or_default()
}

// Fenster (0, 0), Arbeitsbereich ab (5, 48)
fn kachel(i: usize) -> (i32, i32) {
    let (x, y) = kachel_lage(i);
    (5 + x + KW / 2, 48 + y + KH / 2)
}
const ZURUECK: (i32, i32) = (5 + 8 + 28, 48 + 6 + 21);
const MINUS: (i32, i32) = (5 + 20 + 32, 48 + 96 + 24);

#[test]
fn einstellungen_per_touch_und_nach_neustart() {
    let wurzel: PathBuf = std::env::temp_dir().join("esp32os_p3_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    // Ersteinrichtung schon erledigt (eigener Test: ersteinrichtung.rs)
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    let bilder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bilder");
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    kern::datei::einhaengen("/sd0/1", &bilder);
    // Speicherplatz kommt am Board aus FatFs; hier feste Werte
    kern::datei::speicher_info_setzen(|_| Some((1024 << 20, 300 << 20)));
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    kern::starte_name("einstellungen").unwrap();
    bis(|| sys::fenster_liste().iter().any(|f| f.titel == "Settings"), "Einstellungen offen");
    foto("p3_start");
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // Anzeige: Helligkeit 8 → + + → 10, − → 9
    let (x, y) = kachel(0);
    f.tippen(x, y);
    let plus_hell = (5 + 100 + 10 * 40 + 16 + 32, 48 + 96 + 24);
    f.tippen(plus_hell.0, plus_hell.1);
    f.tippen(plus_hell.0, plus_hell.1);
    bis(|| konfig::HELLIGKEIT.load(Relaxed) == 10, "Helligkeit 10");
    f.tippen(MINUS.0, MINUS.1);
    bis(|| konfig::HELLIGKEIT.load(Relaxed) == 9, "Helligkeit 9");
    foto("p3_anzeige");
    assert!(ini(&wurzel).contains("brightness=9"));
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Ton: Lautstärke 6 → 5, Probeton angefordert
    let (x, y) = kachel(1);
    f.tippen(x, y);
    konfig::TON_PROBE.store(false, Relaxed);
    f.tippen(MINUS.0, MINUS.1);
    bis(|| konfig::LAUTSTAERKE.load(Relaxed) == 5, "Lautstärke 5");
    assert!(konfig::TON_PROBE.load(Relaxed), "Probeton");
    assert!(ini(&wurzel).contains("volume=5"));
    foto("p3_ton");
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Akku: ohne Messung klarer Hinweis
    let (x, y) = kachel(2);
    f.tippen(x, y);
    foto("p3_akku");
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Datum/Zeit: Jahr + 1, Uhr stellen
    let jahr = |u: i64| kern::zeit::datum_aus_tagen(kern::zeit::lokal(u).div_euclid(86_400)).0;
    let vorher = jahr(kern::zeit::unix());
    let (x, y) = kachel(3);
    f.tippen(x, y);
    f.tippen(5 + 10 + 2 * 120 + 50, 48 + 78 + 21);
    foto("p3_zeit");
    f.tippen(5 + 10 + 110, 48 + 204 + 22);
    bis(|| jahr(kern::zeit::unix()) == vorher + 1, "Jahr gestellt");
    let gesichert: i64 = ini(&wurzel).lines().find_map(|l| l.strip_prefix("clock=")).unwrap().trim().parse().unwrap();
    assert!((gesichert - kern::zeit::unix()).abs() <= 2, "Uhrzeit gesichert");
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Touch: Touchpad-Modus an, Layout US
    let (x, y) = kachel(4);
    f.tippen(x, y);
    f.tippen(5 + 20 + 210, 48 + 66 + 24);
    bis(sys::eingabe::touchpad, "Touchpad an");
    // im Touchpad-Modus klickt Tippen am Zeiger – erst wieder aus (Zeiger
    // liegt über dem Knopf, siehe touchpad.rs), dann das Layout
    sys::eingabe::touchpad_setzen(false);
    assert!(ini(&wurzel).contains("touchpad=1"));
    f.tippen(5 + 20 + 210, 48 + 126 + 24);
    bis(|| ini(&wurzel).contains("keyboard=us"), "Layout US");
    foto("p3_touch");
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Hintergrund: grüner Verlauf, dann Bild aus Datei (Dateidialog)
    let (x, y) = kachel(5);
    f.tippen(x, y);
    f.tippen(5 + 8 + 154 + 73, 48 + 110);
    bis(|| konfig::hintergrund() == Hintergrund::Verlauf(1), "Verlauf grün");
    foto("p3_hintergrund");
    f.tippen(5 + 8 + 150, 48 + 176 + 24);
    bis(|| sys::fenster_liste().iter().any(|f| f.titel == "Open file"), "Dateidialog");
    // Dialog (100, 10), Arbeitsbereich +5/+48, Zeile 32: animation, farben, foto …
    f.tippen(105 + 100, 58 + 36 + 2 * 32 + 16);
    bis(|| konfig::hintergrund() == Hintergrund::Bild("/sd0/1/foto.jpg".into()), "Bild als Hintergrund");
    assert!(wurzel.join("settings/wallpaper.rgb").exists());
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Speicher
    let (x, y) = kachel(6);
    f.tippen(x, y);
    foto("p3_speicher");
    f.tippen(ZURUECK.0, ZURUECK.1);

    // Sprache: Deutsch
    let (x, y) = kachel(7);
    f.tippen(x, y);
    f.tippen(5 + 240 + 100, 48 + 70 + 24);
    bis(|| kern::sprache::aktuell() == kern::sprache::Sprache::Deutsch, "Deutsch");
    bis(|| sys::fenster_liste().iter().any(|f| f.titel == "Einstellungen"), "Titel deutsch");
    foto("p3_sprache_de");

    // Einstellungen schliessen → Desktop mit Bild-Hintergrund (deutsch)
    f.tippen(640 - 5 - 30, 24);
    bis(|| !sys::fenster_liste().iter().any(|f| f.titel == "Einstellungen"), "geschlossen");
    let px = foto("p3_desktop_bild");
    assert!(kern::liste().iter().any(|p| p.name == "@icon"), "@icon läuft noch");
    // Pixel rechts oben im Desktop = vorbereitetes Hintergrundbild
    let soll = desktop::konfig::hintergrund_lesen(640, 360).unwrap();
    for (x, y) in [(600usize, 30usize), (400, 200), (20, 300)] {
        let s = kern::grafik::von565(kern::grafik::zu565(soll.px[y * 640 + x]));
        assert_eq!(px[y * 640 + x], s, "Hintergrundbild bei ({x}, {y})");
    }
    assert!(std::fs::metadata(wurzel.join("settings/wallpaper.rgb")).unwrap().len() < 120_000, "klein genug für die interne Partition");

    // „Neustart“: Werte zurücksetzen und aus der ini laden
    konfig::HELLIGKEIT.store(3, Relaxed);
    konfig::LAUTSTAERKE.store(10, Relaxed);
    kern::sprache::setzen(kern::sprache::Sprache::Englisch);
    desktop::tastatur::layout_setzen(desktop::tastatur::Layout::Ch);
    konfig::laden();
    assert_eq!(konfig::HELLIGKEIT.load(Relaxed), 9);
    assert_eq!(konfig::LAUTSTAERKE.load(Relaxed), 5);
    assert_eq!(kern::sprache::aktuell(), kern::sprache::Sprache::Deutsch);
    assert_eq!(desktop::tastatur::layout(), desktop::tastatur::Layout::Us);
    assert!(sys::eingabe::touchpad(), "Touchpad-Modus aus der ini");
    sys::eingabe::touchpad_setzen(false);
    assert_eq!(konfig::hintergrund(), Hintergrund::Bild("/sd0/1/foto.jpg".into()));
}
