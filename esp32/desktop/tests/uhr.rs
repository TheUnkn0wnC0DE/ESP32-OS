//! A3-Nachweis (PC): Uhr per Touch aus dem Startmenü – Uhrzeit, Wecker
//! stellen (gespeichert), Timer 10 s ablaufen lassen (klingelt, Signalton,
//! „Stopp“), Wecker klingelt bei geschlossener Uhr (die Taskleiste startet
//! sie).

use core::sync::atomic::Ordering::Relaxed;
use desktop::gesten::Gesten;
use desktop::uhr::{self, FX, FY, anzeige_zuletzt, knopf_mitte};
use kern::{KernAlloc, sys};
use konsole::Spiel;
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(sekunden: u64, mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(sekunden), "Zeitüberschreitung: {was}; Anzeige {:?}", anzeige_zuletzt());
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

    /// Knopf der Uhr (Arbeitsbereich: Rand 5, Kopf 48).
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

fn uhr_laeuft() -> bool {
    kern::liste().iter().any(|p| p.name == "uhr")
}

/// Uhr auf die lokale Zeit `h:m:s` am 23.11.2026 stellen.
fn uhr_stellen(h: i64, m: i64, s: i64) {
    let tag = kern::zeit::tage_aus_datum(2026, 11, 23) * 86_400;
    let versatz = kern::zeit::lokal(tag) - tag;
    kern::zeit::unix_setzen(tag + h * 3600 + m * 60 + s - versatz);
}

#[test]
fn uhr_wecker_und_timer() {
    let wurzel = std::env::temp_dir().join("esp32os_a3_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    uhr_stellen(6, 30, 0);
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));

    // Uhr über das Startmenü
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    f.tippen(40, 340);
    bis(10, || sys::fenster_liste().iter().any(|w| w.titel == "ESP32-OS"), "Startmenü");
    let (x, y) = desktop::shell::menue_eintrag_mitte("uhr").unwrap();
    f.tippen(x, y);
    bis(10, || anzeige_zuletzt().starts_with("06:30:"), "Uhrzeit 06:30");
    foto("a3_uhr");

    // Wecker: Stunde +/−, Minute +/−, an (gespeichert)
    f.knopf(uhr::K_REITER + 1);
    bis(10, || anzeige_zuletzt() == "07:00", "Wecker-Reiter");
    f.knopf(uhr::K_STUNDE_PLUS);
    bis(10, || anzeige_zuletzt() == "08:00", "Stunde +");
    f.knopf(uhr::K_STUNDE_MINUS);
    f.knopf(uhr::K_MINUTE_MINUS);
    bis(10, || anzeige_zuletzt() == "07:59", "Stunde −, Minute −");
    f.knopf(uhr::K_MINUTE_PLUS);
    f.knopf(uhr::K_WECKER_AN);
    bis(10, || desktop::konfig::WECKER.load(Relaxed) == 7 * 60 | 1 << 16, "Wecker 07:00 an");
    let ini = std::fs::read_to_string(wurzel.join("settings/esp32os.ini")).unwrap();
    assert!(ini.contains("alarm=07:00") && ini.contains("alarm_on=1"), "gespeichert: {ini}");
    assert_eq!(anzeige_zuletzt(), "07:00");
    foto("a3_wecker");

    // Timer: 5 min → −1 min ×5 → +10 s → Start → klingelt nach 10 s
    f.knopf(uhr::K_REITER + 2);
    bis(10, || anzeige_zuletzt() == "05:00", "Timer-Reiter");
    for _ in 0..5 {
        f.knopf(uhr::K_T_MIN_MINUS);
    }
    f.knopf(uhr::K_T_SEK_PLUS);
    bis(10, || anzeige_zuletzt() == "00:10", "Timer 00:10");
    desktop::konfig::SIGNAL.store(0, Relaxed);
    let start = Instant::now();
    f.knopf(uhr::K_T_START);
    bis(10, || anzeige_zuletzt() == "00:05", "Timer läuft");
    foto("a3_timer_laeuft");
    bis(15, || uhr::KLINGELN.load(Relaxed) == 2, "Timer abgelaufen");
    let dauer = start.elapsed();
    assert!(dauer >= Duration::from_millis(9_500) && dauer < Duration::from_millis(11_500), "Timer 10 s: {dauer:?}");
    bis(10, || anzeige_zuletzt() == "Timer done", "Anzeige klingelt");
    bis(10, || desktop::konfig::SIGNAL.load(Relaxed) >= 2, "Signalton jede Sekunde");
    foto("a3_timer_klingelt");
    f.knopf(uhr::K_STOPP);
    bis(10, || uhr::KLINGELN.load(Relaxed) == 0 && anzeige_zuletzt() == "00:10", "Stopp → Timer wieder auf 00:10");

    // Wecker bei geschlossener Uhr: Fenster schliessen (Knopf 1)
    let schliessen = kern::kern_abschnitt(|| {
        kern::fenster::mit_welt(|w| {
            let s = w.fenster_liste().into_iter().find(|f| f.titel == "Clock").unwrap().slot;
            let fe = w.slots[s].as_ref().unwrap().fenster.as_ref().unwrap();
            let k = fe.knoepfe.iter().find(|k| k.id == 1).unwrap();
            (fe.x + k.x + k.w / 2, fe.y + k.y + k.h / 2)
        })
    });
    f.tippen(schliessen.0, schliessen.1);
    bis(10, || !uhr_laeuft(), "Uhr geschlossen");
    uhr_stellen(6, 59, 58);
    bis(10, uhr_laeuft, "Taskleiste startet die Uhr zum Wecken");
    bis(10, || uhr::KLINGELN.load(Relaxed) == 1 && anzeige_zuletzt() == "Alarm 07:00", "Wecker klingelt");
    foto("a3_wecker_klingelt");
    // klingelt: die Firmware hält den Bildschirm an (weckt aus Schoner/Ruhe)
    let mut d = desktop::Desktop::neu(|| 0);
    assert!(d.wach_halten(), "Bildschirm an, solange es klingelt");
    f.knopf(uhr::K_STOPP);
    bis(10, || uhr::KLINGELN.load(Relaxed) == 0 && anzeige_zuletzt() == "07:00", "Stopp → Wecker-Reiter");
    assert!(!d.wach_halten(), "nach Stopp darf der Bildschirm wieder aus");
    // klingelt in derselben Minute nicht noch einmal
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(uhr::KLINGELN.load(Relaxed), 0);
}
