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
    let titel = || sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<String>>();
    // Startmenü → „Mehr Programme...“ → Programm (Lagen aus dem Menü selbst,
    // damit neue Einträge den Test nicht verschieben)
    let mut aus_mehr = |programm: &str, fenster: &str| {
        tippen(40, 340);
        std::thread::sleep(Duration::from_millis(500));
        let (x, y) = desktop::shell::menue_eintrag_mitte("@mehr").expect("Mehr Programme");
        tippen(x, y);
        std::thread::sleep(Duration::from_millis(500));
        let (x, y) = desktop::shell::menue_eintrag_mitte_in("mehr", programm).expect(programm);
        tippen(x, y);
        std::thread::sleep(Duration::from_millis(800));
        assert!(titel().iter().any(|t| t.starts_with(fenster)), "{programm}: {:?}", titel());
    };
    // Z3: die übernommenen KolibriOS-Programme; boxlib_demo zuletzt (sein
    // Textfeld hat den Fokus → Bildschirmtastatur deckt die Taskleiste)
    aus_mehr("passwordgen", "PasswordGen");
    aus_mehr("thashview", "Open file");
    aus_mehr("boxlib_demo", "Boxlib example");
}
