//! D12-Nachweis (PC): Absturz eines Programms (Rust-Panik) → System läuft
//! weiter, Meldungsdialog nennt das Programm; ein abgestürzter Systemdienst
//! wird gemeldet und neu gestartet (Wächter, D-Review Befund 7).

use kern::{KernAlloc, ProgrammEintrag, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn wackelig() {
    sys::pruefe_ereignis();
    sys::fenster(50 << 16 | 299, 50 << 16 | 149, 0x33 << 24 | 0xE6E6E6, 0, Some("Wackelig"));
    sys::warten(20);
    let v: Vec<u32> = Vec::new();
    let _ = v[3]; // Panik: Index ausserhalb
}

static DIENST_LAEUFE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Stellt einen Systemdienst dar: stürzt beim ersten Lauf ab, läuft danach.
fn dienst() {
    if DIENST_LAEUFE.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
        sys::warten(10);
        panic!("Dienst kaputt");
    }
    loop {
        sys::warten(100);
    }
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "@icon", heap_kb: 128, stapel_kb: 128, start: dienst },
    ProgrammEintrag { name: "wackelig", heap_kb: 128, stapel_kb: 128, start: wackelig },
    ProgrammEintrag { name: "@taskbar", heap_kb: 512, stapel_kb: 128, start: desktop::shell::taskbar },
    ProgrammEintrag { name: "@meldung", heap_kb: 256, stapel_kb: 128, start: desktop::shell::meldung },
];

#[test]
fn absturz_wird_gemeldet() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::starte_name("@taskbar").unwrap();
    kern::starte_name("@icon").unwrap();
    let pid = kern::starte_name("wackelig").unwrap();
    // Beide Abstürze gemeldet (zwei Meldungen), Dienst @icon neu gestartet
    let t = Instant::now();
    loop {
        kern::aufraeumen();
        desktop::shell::waechter();
        let n = sys::fenster_liste().iter().filter(|f| f.titel == "Program crashed").count();
        if n == 2 && DIENST_LAEUFE.load(std::sync::atomic::Ordering::SeqCst) >= 2 {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(8), "Meldungen {n}, Dienst-Läufe {}", DIENST_LAEUFE.load(std::sync::atomic::Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(matches!(kern::ende_von(pid), Some(kern::Ende::Absturz(_))));
    std::thread::sleep(Duration::from_millis(200));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join("d12_meldung.png"), spatz::image::encode_png(w, h, &px)).unwrap();
}
