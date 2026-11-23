//! A5-Nachweis (PC): Musikplayer aus dem Startmenü, MP3 aus
//! `/sd0/1/ESP32-OS/Musik` (eigene Testtöne, `werkzeuge/test_mp3.py`).
//! Ein Test-Faden spielt die Rolle des Audio-Fadens der Firmware (holt den
//! Strom in Echtzeit ab) und prüft, dass wirklich die Töne der Dateien
//! ankommen: Titel 1 beginnt mit ~382 Hz, Titel 2 mit ~1916 Hz. Pause hält
//! den Ton an, Weiter spielt an derselben Stelle weiter, nach dem Ende von
//! Titel 1 kommt Titel 2, „nächster“/„zurück“ wechseln.

use desktop::gesten::Gesten;
use desktop::musik::{self, FX, FY, knopf_mitte, titel_mitte, zustand};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(sekunden: u64, mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(sekunden), "Zeitüberschreitung: {was}; Zustand {:?}; Programme {:?}; Abstürze {:?}", zustand(), kern::liste().iter().map(|p| (p.name, p.heap_belegt, p.heap_groesse)).collect::<Vec<_>>(), kern::neue_abstuerze().iter().map(|(n, e)| format!("{n}: {e:?}")).collect::<Vec<_>>());
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

/// Abgeholte Ausgabe (links), mit Zeitpunkt jedes Blocks.
type Mitschnitt = Arc<Mutex<Vec<i16>>>;

/// Audio-Faden: alle 11,6 ms 256 Frames abholen (wie die Firmware).
fn audio_faden(m: Mitschnitt) {
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut bloecke = 0u64;
        loop {
            let mut out = [0i16; 512];
            kern::ton::dazumischen(&mut out);
            m.lock().unwrap().extend(out.iter().step_by(2));
            bloecke += 1;
            let soll = Duration::from_micros(bloecke * 256 * 1_000_000 / u64::from(kern::ton::RATE));
            if let Some(w) = soll.checked_sub(start.elapsed()) {
                std::thread::sleep(w);
            }
        }
    });
}

/// RMS und Frequenz (Nulldurchgänge) der letzten `ms` des Mitschnitts.
fn messen(m: &Mitschnitt, ms: u32) -> (f64, f64) {
    let v = m.lock().unwrap();
    let n = (kern::ton::RATE * ms / 1000) as usize;
    let s = &v[v.len().saturating_sub(n)..];
    let rms = (s.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / s.len().max(1) as f64).sqrt();
    let null = s.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
    (rms, null as f64 / 2.0 / (s.len() as f64 / f64::from(kern::ton::RATE)))
}

#[test]
fn musik_spielt_pause_weiter_naechster() {
    let wurzel = std::env::temp_dir().join("esp32os_a5_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    let sys_dir = wurzel.join("sys");
    let sd = wurzel.join("sd");
    std::fs::create_dir_all(sys_dir.join("settings")).unwrap();
    std::fs::write(sys_dir.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    let musik_dir = sd.join("ESP32-OS/Musik");
    std::fs::create_dir_all(&musik_dir).unwrap();
    let daten = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/daten");
    for n in ["Testton 1.mp3", "Testton 2.mp3"] {
        std::fs::copy(daten.join(n), musik_dir.join(n)).unwrap();
    }
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &sys_dir);
    kern::datei::einhaengen("/rd/1", &sys_dir);
    kern::datei::einhaengen("/sd0/1", &sd);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    let m: Mitschnitt = Arc::new(Mutex::new(Vec::new()));
    audio_faden(m.clone());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));

    let mut f = Finger { g: Gesten::default(), t: 1000 };
    f.tippen(40, 340);
    bis(10, || sys::fenster_liste().iter().any(|w| w.titel == "ESP32-OS"), "Startmenü");
    let (x, y) = desktop::shell::menue_eintrag_mitte("musik").unwrap();
    f.tippen(x, y);
    bis(10, || sys::fenster_liste().iter().any(|w| w.titel == "Music"), "Musik offen");
    // Titel 1 antippen: Ton ~382 Hz kommt an
    let (x, y) = titel_mitte(0);
    f.tippen(FX + 5 + x, FY + 48 + y);
    bis(10, || zustand().0 == "Testton 1.mp3" && zustand().1, "Titel 1 spielt");
    std::thread::sleep(Duration::from_millis(350));
    let (rms, hz) = messen(&m, 250);
    assert!(rms > 1000.0 && (340.0..430.0).contains(&hz), "Titel 1: RMS {rms:.0}, {hz:.0} Hz");
    bis(10, || zustand().2 >= 1, "Position 0:01");
    foto("a5_spielt");

    // Pause: Ton weg, Position bleibt
    f.knopf(musik::K_SPIELEN);
    bis(10, || !zustand().1, "Pause");
    std::thread::sleep(Duration::from_millis(400));
    let (rms, _) = messen(&m, 300);
    assert!(rms < 1.0, "Pause ist still: RMS {rms:.1}");
    let pos = zustand().2;
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(zustand().2, pos, "Position steht in der Pause");
    foto("a5_pause");
    // Weiter: gleiche Stelle, Ton wieder da
    f.knopf(musik::K_SPIELEN);
    bis(10, || zustand().1, "Weiter");
    std::thread::sleep(Duration::from_millis(400));
    let (rms, _) = messen(&m, 250);
    assert!(rms > 1000.0, "weiter: RMS {rms:.0}");
    assert!(zustand().2 >= pos && zustand().2 <= pos + 1, "weiter an derselben Stelle: {pos} → {}", zustand().2);

    // Ende von Titel 1 (8 s) → Titel 2 (~1916 Hz)
    bis(15, || zustand().0 == "Testton 2.mp3", "automatisch Titel 2");
    std::thread::sleep(Duration::from_millis(400));
    let (rms, hz) = messen(&m, 250);
    assert!(rms > 1000.0 && (1700.0..2150.0).contains(&hz), "Titel 2: RMS {rms:.0}, {hz:.0} Hz");
    // zurück (unter 3 s): Titel 1; nächster: Titel 2
    f.knopf(musik::K_ZURUECK);
    bis(10, || zustand().0 == "Testton 1.mp3" && zustand().1, "zurück → Titel 1");
    f.knopf(musik::K_NAECHSTER);
    bis(10, || zustand().0 == "Testton 2.mp3" && zustand().1, "nächster → Titel 2");
    // letzter Titel zu Ende: Wiedergabe hört auf
    bis(12, || !zustand().1, "Ende der Liste");
    std::thread::sleep(Duration::from_millis(300));
    let (rms, _) = messen(&m, 200);
    assert!(rms < 1.0, "danach still: RMS {rms:.1}");
}
