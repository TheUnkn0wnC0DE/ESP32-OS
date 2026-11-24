//! C3 Standby per Touch (Vorbereitung des Board-Nachweises): Matrix
//! läuft, Startmenü → „Ruhezustand“. Am PC prüfbar: der Menüeintrag fordert
//! den Ruhezustand an (`konfig::RUHE_JETZT`), Matrix läuft weiter. Die
//! Tipps mit Board-Zeiten ergeben den Autopilot (`C3_AUTOPILOT_NEU=1` →
//! `werkzeuge/board/c3_autopilot.txt`, Fotozeiten `c3_fotos.txt`): 90 s
//! schlafen, Antippen weckt (die Berührung erreicht kein Programm), Fotos
//! direkt danach und 15 s später (Matrix hat weiter gezeichnet), Log prüft
//! das WLAN nach dem Aufwachen.
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

const START: (i32, i32) = (40, 340);

fn titel() -> Vec<String> {
    sys::fenster_liste().into_iter().map(|f| f.titel).collect()
}

#[test]
fn ruhezustand_aus_dem_menue() {
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let _ = kern::neue_abstuerze();
    let mut g = Gesten::default();
    let mut t = 5000u64;
    // Board-Zeitleiste (ms ab Start): Tipps und Fotos
    let mut board: Vec<(u64, i32, i32)> = Vec::new();
    let mut fotos: Vec<u64> = Vec::new();
    let mut bt = 25_000u64;
    let mut tipp = |x: i32, y: i32, warte: u64, board: &mut Vec<(u64, i32, i32)>, bt: &mut u64| {
        board.push((*bt, x, y));
        *bt += warte;
        g.schritt(t, [Some((x, y)), None]);
        t += 100;
        g.schritt(t, [None, None]);
        for _ in 0..8 {
            t += 20;
            g.schritt(t, [None, None]);
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(400));
    };
    // Matrix über das Menü
    tipp(START.0, START.1, 1200, &mut board, &mut bt);
    let (x, y) = desktop::shell::menue_eintrag_mitte("@mehr").expect("Mehr");
    tipp(x, y, 1200, &mut board, &mut bt);
    let (x, y) = desktop::shell::menue_eintrag_mitte_in("mehr", "matrix").expect("Matrix");
    tipp(x, y, 5000, &mut board, &mut bt);
    assert!(titel().iter().any(|t| t == "Matrix"), "Matrix läuft: {:?}", titel());
    // Startmenü → Ruhezustand
    desktop::konfig::RUHE_JETZT.store(false, Relaxed);
    tipp(START.0, START.1, 1200, &mut board, &mut bt);
    let (x, y) = desktop::shell::menue_eintrag_mitte("@ruhe").expect("Ruhezustand im Menü");
    tipp(x, y, 90_000, &mut board, &mut bt);
    assert!(desktop::konfig::RUHE_JETZT.load(Relaxed), "Menüeintrag fordert den Ruhezustand an");
    assert!(titel().iter().any(|t| t == "Matrix"), "Matrix läuft weiter: {:?}", titel());
    // Board: Antippen in der Mitte weckt, Fotos danach und 15 s später
    board.push((bt, 320, 180));
    bt += 3_000;
    fotos.push(bt / 1000);
    bt += 15_000;
    fotos.push(bt / 1000);
    bt += 9_000;
    let a = kern::neue_abstuerze();
    assert!(a.is_empty(), "Abstürze {a:?}");
    if std::env::var_os("C3_AUTOPILOT_NEU").is_some() {
        let ap: Vec<String> = board.iter().flat_map(|&(t, x, y)| [format!("{t}:{x}:{y}:1"), format!("{}:{x}:{y}:0", t + 120)]).collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../werkzeuge/board");
        std::fs::write(dir.join("c3_autopilot.txt"), ap.join(",")).unwrap();
        std::fs::write(dir.join("c3_fotos.txt"), fotos.iter().map(u64::to_string).collect::<Vec<_>>().join(",")).unwrap();
        eprintln!("Board-Lauf {} s, Fotos {fotos:?}", bt / 1000);
    }
}
