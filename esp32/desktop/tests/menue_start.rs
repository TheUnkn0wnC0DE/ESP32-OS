//! Nachstellung Board-Ablauf: Startmenü → „Steuerelemente“ (boxlib_demo).
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

#[test]
fn menue_startet_programm() {
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let mut g = Gesten::default();
    let mut t = 5000u64;
    let mut tippen = |x: i32, y: i32| {
        g.schritt(t, [Some((x, y)), None]);
        t += 100;
        g.schritt(t, [None, None]);
        for _ in 0..10 {
            t += 20;
            g.schritt(t, [None, None]);
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    tippen(40, 340);
    std::thread::sleep(Duration::from_millis(500));
    let titel: Vec<String> = sys::fenster_liste().iter().map(|f| f.titel.clone()).collect();
    eprintln!("nach Start: {titel:?}");
    tippen(100, 204);
    std::thread::sleep(Duration::from_millis(800));
    let titel: Vec<String> = sys::fenster_liste().iter().map(|f| f.titel.clone()).collect();
    eprintln!("nach Steuerelemente: {titel:?}");
    assert!(titel.iter().any(|t| t == "Boxlib example"), "{titel:?}");
}
