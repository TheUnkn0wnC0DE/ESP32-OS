//! Fehlerbericht 24.11. (Board, v1.0.2): Browser offen, Startknopf („Home“)
//! antippen → ganzes System stürzt ab („@menu“, illegal instruction).
//! Nachstellung am PC: Browser offen (lädt), Startmenü oft öffnen und
//! schliessen, Einträge antippen – kein Absturz, Menü und Browser bleiben.
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

const START: (i32, i32) = (40, 340);
// Titelleiste des Browsers: das Hauptmenü (seit „Wetter“ drei Spalten)
// reicht von y = 29 bis zur Taskleiste über die ganze Breite
const FREI: (i32, i32) = (300, 15);

fn titel() -> Vec<String> {
    sys::fenster_liste().into_iter().map(|f| f.titel).collect()
}

#[test]
fn startmenue_bei_offenem_browser() {
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2000));
    let _ = kern::neue_abstuerze();
    let mut g = Gesten::default();
    let mut t = 5000u64;
    let mut tippen = |x: i32, y: i32| {
        g.schritt(t, [Some((x, y)), None]);
        t += 100;
        g.schritt(t, [None, None]);
        for _ in 0..8 {
            t += 20;
            g.schritt(t, [None, None]);
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    // Browser über das Startmenü
    tippen(START.0, START.1);
    std::thread::sleep(Duration::from_millis(600));
    let (x, y) = desktop::shell::menue_eintrag_mitte("browser").expect("Browser im Menü");
    tippen(x, y);
    std::thread::sleep(Duration::from_millis(4000));
    let vorher = titel();
    assert!(kern::liste().iter().any(|p| p.name == "browser"), "Browser läuft: {vorher:?}");
    for runde in 0..15 {
        tippen(START.0, START.1);
        std::thread::sleep(Duration::from_millis(400));
        assert!(kern::liste().iter().any(|p| p.name == "@menu"), "Runde {runde}: Menü offen; Fenster {:?}, Abstürze {:?}", titel(), kern::neue_abstuerze());
        if runde % 3 == 2 {
            // Ordner „Mehr Programme“ öffnen (zweites Menü)
            let (x, y) = desktop::shell::menue_eintrag_mitte("@mehr").expect("Mehr");
            tippen(x, y);
            std::thread::sleep(Duration::from_millis(400));
        }
        tippen(FREI.0, FREI.1);
        std::thread::sleep(Duration::from_millis(400));
        let a = kern::neue_abstuerze();
        assert!(a.is_empty(), "Runde {runde}: Abstürze {a:?}");
        assert!(kern::liste().iter().any(|p| p.name == "browser"), "Runde {runde}: Browser läuft noch");
    }
    // Wie am Board: System-Heap fast voll (der Browser hat den PSRAM). Der
    // Fensterpuffer des Menüs passt nicht → nur das Menü endet („Speicher
    // voll“), Browser und Desktop laufen weiter, nichts bricht ab.
    use std::sync::atomic::Ordering::Relaxed;
    kern::grafik::TEST_SYSTEM_FREI.store(kern::grafik::RESERVE + 64 * 1024, Relaxed);
    tippen(START.0, START.1);
    std::thread::sleep(Duration::from_millis(800));
    kern::aufraeumen();
    let a = kern::neue_abstuerze();
    assert!(a.iter().any(|(n, e)| *n == "@menu" && *e == kern::Ende::SpeicherVoll), "Menü endet mit Speicher voll: {a:?}");
    assert!(!kern::liste().iter().any(|p| p.name == "@menu"), "Menü beendet");
    assert!(kern::liste().iter().any(|p| p.name == "browser"), "Browser läuft weiter");
    // Speicher wieder da: Menü geht wieder
    kern::grafik::TEST_SYSTEM_FREI.store(usize::MAX, Relaxed);
    tippen(START.0, START.1);
    std::thread::sleep(Duration::from_millis(600));
    assert!(kern::liste().iter().any(|p| p.name == "@menu"), "Menü öffnet wieder");
    assert!(kern::neue_abstuerze().is_empty());
}
