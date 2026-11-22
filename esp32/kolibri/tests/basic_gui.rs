//! K3-Nachweis: KolibriOS-Beispiel `basic_gui.c` (unverändert) läuft über die
//! native ksys.h. Nur 32 Bit (`--target i686-pc-windows-msvc`).
#![cfg(target_pointer_width = "32")]

use kern::{Ende, KernAlloc, sys};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

const HG: u32 = 0x3A6EA5;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) -> Vec<u32> {
    let (w, h, px) = sys::foto();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    let ist = format!("{:016x}", fnv(&px));
    if std::env::var_os("ERWARTET_NEU").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
        std::fs::write(dir.join(format!("{name}.fnv")), &ist).unwrap();
    } else {
        let soll = std::fs::read_to_string(dir.join(format!("{name}.fnv"))).expect("Erwartung fehlt – ERWARTET_NEU=1");
        if soll.trim() != ist {
            std::fs::write(dir.join(format!("{name}.ist.png")), spatz::image::encode_png(w, h, &px)).unwrap();
            panic!("Bild {name} weicht ab");
        }
    }
    px
}

fn tippen(x: i32, y: i32) {
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(30));
    sys::eingabe::maus(x, y, 0);
}

#[test]
fn basic_gui_laeuft_und_reagiert_auf_touch() {
    kern::init(&kern::PC, kolibri::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    let pid = kern::starte_name("basic_gui").unwrap();
    let hg = kern::grafik::von565(kern::grafik::zu565(HG));
    bis(|| sys::foto().2.iter().any(|&p| p != hg), "Fenster erscheint");
    std::thread::sleep(Duration::from_millis(100));
    let vorher = bild("k3_basic_gui");
    // Fenster 600×400 passt nicht → y = 0, Höhe = 360; x = 10. Stil 0x13 ohne
    // Flag B: Koordinaten relativ zum Fenster. BUTTON1 (BTN_POP) bei (10,30) 71×21.
    tippen(10 + 10 + 30, 30 + 10);
    std::thread::sleep(Duration::from_millis(150));
    let nachher = bild("k3_basic_gui_pop");
    assert_ne!(vorher, nachher, "Statusleiste nach BUTTON1 unverändert");
    // Schliessen-Knopf (id 1) → main() kehrt zurück
    tippen(10 + 601 - 12, 11);
    let t = Instant::now();
    loop {
        kern::aufraeumen();
        if let Some(e) = kern::ende_von(pid) {
            assert_eq!(e, Ende::Normal);
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "basic_gui endet nicht");
        std::thread::sleep(Duration::from_millis(5));
    }
}
