//! S4: Ersteinrichtung mit dem Kreuz schliessen gilt als erledigt – sie
//! kommt beim nächsten Start nicht wieder (am Board des Nutzers kam sie bei
//! jedem Einschalten).
use kern::{KernAlloc, sys};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn kreuz_schliesst_die_einrichtung_ab() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_s4_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    assert!(desktop::konfig::einrichtung_noetig(), "frisches System braucht die Einrichtung");
    let pid = kern::starte_name("ersteinrichtung").unwrap();
    bis(|| sys::fenster_liste().iter().any(|f| f.titel == "Welcome to ESP32-OS"), "Assistent erscheint");
    // Kreuz (Systemknopf 1) des Assistenten
    let (x, y) = kern::fenster::mit_welt(|w| {
        let s = *w.stapel.iter().rev().find(|&&s| w.slots[s].as_ref().is_some_and(|z| z.pid == pid)).expect("Fenster");
        let f = w.slots[s].as_ref().and_then(|z| z.fenster.as_ref()).expect("Fenster");
        let k = f.knoepfe.iter().find(|k| k.system && k.id == 1).expect("Kreuz");
        (f.x + k.x + k.w / 2, f.y + k.y + k.h / 2)
    });
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(40));
    sys::eingabe::maus(x, y, 0);
    bis(|| { kern::aufraeumen(); kern::ende_von(pid).is_some() }, "Assistent endet");
    assert!(!desktop::konfig::einrichtung_noetig(), "nach dem Kreuz kein Assistent mehr beim Start");
    let _ = std::fs::remove_dir_all(&wurzel);
}
