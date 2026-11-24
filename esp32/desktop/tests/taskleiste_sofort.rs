//! S1: Fenster und Eintrag in der Taskleiste erscheinen (und verschwinden)
//! praktisch im selben Bild – die Taskleiste wird per Ereignis
//! aktualisiert statt alle 500 ms abzufragen. Gemessen: Zeit vom Erscheinen
//! des Fensters bis zur Änderung der Taskleiste im Bild.
use kern::{KernAlloc, sys};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Pixel der Taskleiste (unterste Zeilen, ohne Uhr rechts).
fn leiste() -> Vec<u32> {
    let (w, h, px) = sys::foto();
    let mut v = Vec::new();
    for y in h as usize - desktop::shell::LEISTE_H as usize..h as usize {
        v.extend_from_slice(&px[y * w as usize..y * w as usize + 520]);
    }
    v
}

fn bis(mut f: impl FnMut() -> bool, was: &str) -> Duration {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(2));
    }
    t.elapsed()
}

#[test]
fn eintrag_kommt_mit_dem_fenster() {
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@taskbar").unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    for runde in 0..3 {
        let vorher = leiste();
        let pid = kern::starte_name("rechner").unwrap();
        bis(|| sys::fenster_liste().iter().any(|f| f.pid == pid), "Fenster");
        let d = bis(|| leiste() != vorher, "Eintrag in der Taskleiste");
        assert!(d < Duration::from_millis(150), "Runde {runde}: Eintrag {d:?} nach dem Fenster (vorher bis 500 ms)");
        let mit = leiste();
        kern::zwangsbeenden(pid, 2000);
        bis(|| !sys::fenster_liste().iter().any(|f| f.pid == pid), "Fenster weg");
        let d = bis(|| leiste() != mit, "Eintrag weg");
        assert!(d < Duration::from_millis(150), "Runde {runde}: Eintrag weg {d:?} nach dem Fenster");
        std::thread::sleep(Duration::from_millis(200));
    }
}
