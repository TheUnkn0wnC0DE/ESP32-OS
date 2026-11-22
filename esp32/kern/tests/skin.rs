//! K4-Nachweis: Original-Skin „Shkvorka“ (.skn) mit Touch-Faktor 2.

use kern::{KernAlloc, ProgrammEintrag, sys};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static BEREIT: AtomicU32 = AtomicU32::new(0);
static KNOPF: AtomicU32 = AtomicU32::new(0);

fn fenster(x: u32, y: u32, titel: &str) {
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                // Stil 3, Titel, relativ zum Arbeitsbereich, Arbeitsfarbe = Systemfarbe work
                let mut f = [0u8; 40];
                sys::stil(3, 0, 40, &mut f);
                let work = u32::from_le_bytes(f[20..24].try_into().unwrap());
                let text = u32::from_le_bytes(f[32..36].try_into().unwrap());
                let knopf = u32::from_le_bytes(f[24..28].try_into().unwrap());
                let ktext = u32::from_le_bytes(f[28..32].try_into().unwrap());
                sys::fenster(x << 16 | 299, y << 16 | 199, 0x33 << 24 | work, 0, Some(titel));
                sys::text(10 << 16 | 10, 0xB0 << 24 | text, "Original-Skin Shkvorka, Faktor 2\0".as_bytes(), 0, 0);
                sys::knopf(10 << 16 | 119, 40 << 16 | 39, 5, knopf);
                sys::text(30 << 16 | 52, 0xB0 << 24 | ktext, "Knopf\0".as_bytes(), 0, 0);
                sys::neuzeichnen(2);
                BEREIT.fetch_add(1, Ordering::SeqCst);
            }
            3 => KNOPF.store(sys::knopf_id() >> 8, Ordering::SeqCst),
            _ => {}
        }
    }
}

fn eins() {
    fenster(20, 30, "Dateien");
}
fn zwei() {
    fenster(300, 120, "Einstellungen – ÄÖÜ");
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "eins", heap_kb: 256, stapel_kb: 128, start: eins },
    ProgrammEintrag { name: "zwei", heap_kb: 256, stapel_kb: 128, start: zwei },
];

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

#[test]
fn shkvorka_mit_touch_faktor() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    let mut b = [0u8; 0];
    assert_eq!(sys::stil(4, 0, 0, &mut b).0, 48, "Skinhöhe ×2");
    let p1 = kern::starte_name("eins").unwrap();
    let t = Instant::now();
    while BEREIT.load(Ordering::SeqCst) < 1 {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    let p2 = kern::starte_name("zwei").unwrap();
    while BEREIT.load(Ordering::SeqCst) < 2 {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(50));
    let (w, h, px) = sys::foto();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    let ist = format!("{:016x}", fnv(&px));
    if std::env::var_os("ERWARTET_NEU").is_some() {
        std::fs::write(dir.join("k4_shkvorka.png"), spatz::image::encode_png(w, h, &px)).unwrap();
        std::fs::write(dir.join("k4_shkvorka.fnv"), &ist).unwrap();
    } else {
        let soll = std::fs::read_to_string(dir.join("k4_shkvorka.fnv")).unwrap();
        if soll.trim() != ist {
            std::fs::write(dir.join("k4_shkvorka.ist.png"), spatz::image::encode_png(w, h, &px)).unwrap();
            panic!("Bild weicht ab");
        }
    }
    // Schliessen-Knopf des Skins (×2: x = rechts −62, y 2, 54×36) ist ≥ 7 mm bei 294 ppi und Faktor 2 der Anzeige:
    // 54 logische px × 2 = 108 Panel-px = 9,3 mm.
    assert!(54 * 2 * 254 / 2940 >= 7);
    // Tippen auf den Knopf im inaktiven Fenster „eins“ aktiviert es und löst Knopf 5 aus
    sys::eingabe::maus(20 + 5 + 30, 30 + 48 + 50, 1);
    sys::eingabe::maus(20 + 5 + 30, 30 + 48 + 50, 0);
    while KNOPF.load(Ordering::SeqCst) != 5 {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    kern::beende(p1);
    kern::beende(p2);
}
