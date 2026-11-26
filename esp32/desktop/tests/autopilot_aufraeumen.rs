//! Q4 (Dauertest 26.11.): Das Autopilot-Ereignis `t:0:0:9` beendet zu
//! Beginn einer Board-Runde alle noch offenen Programme (Systemdienste
//! „@…“ bleiben). Vorher hinterliess ein daneben gegangener Tipp offene
//! Fenster, die sich über die Runden summierten.
use core::sync::atomic::{AtomicU64, Ordering};
use kern::{KernAlloc, sys};
use konsole::Spiel;
use pudding_core::input::Input;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static UHR_US: AtomicU64 = AtomicU64::new(0);

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn rundenbeginn_beendet_offene_programme() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_ap_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@taskbar").unwrap();
    let mut d = desktop::Desktop::neu(|| UHR_US.load(Ordering::SeqCst));
    d.autopilot("500:0:0:9");
    let rechner = kern::starte_name("rechner").unwrap();
    bis(|| sys::fenster_liste().iter().any(|f| f.pid == rechner), "Rechner offen");
    let eingabe = Input::default();
    // vor dem Ereignis: nichts passiert
    UHR_US.store(400_000, Ordering::SeqCst);
    d.update(&eingabe, 0.016);
    std::thread::sleep(Duration::from_millis(300));
    assert!(kern::liste().iter().any(|p| p.pid == rechner), "vor 500 ms läuft der Rechner");
    // Rundenbeginn: Rechner wird beendet, die Taskleiste (Dienst) bleibt
    UHR_US.store(600_000, Ordering::SeqCst);
    d.update(&eingabe, 0.016);
    bis(
        || {
            kern::aufraeumen();
            !kern::liste().iter().any(|p| p.pid == rechner)
        },
        "Rechner beendet",
    );
    assert!(kern::liste().iter().any(|p| p.name == "@taskbar"), "Dienst läuft weiter");
    let _ = std::fs::remove_dir_all(&wurzel);
}
