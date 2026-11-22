//! K5-Nachweis (PC-Teil): Dirty-Rect-Doppelpuffer liefert dasselbe Bild wie
//! vollständiges Zusammensetzen und setzt bei kleinen Änderungen wenig neu.

use kern::sys::{self, Schirm};
use kern::{KernAlloc, ProgrammEintrag};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static SCHRITT: AtomicU32 = AtomicU32::new(0);
static FERTIG: AtomicU32 = AtomicU32::new(0);

fn prog() {
    let mut zuletzt = u32::MAX;
    loop {
        let s = SCHRITT.load(Ordering::SeqCst);
        if s != zuletzt {
            match s {
                0 => {
                    sys::pruefe_ereignis();
                    sys::fenster(50 << 16 | 199, 40 << 16 | 149, 0x33_FFFFFF, 0, Some("Dirty"));
                }
                1 => sys::pixel(10, 10, 0xFF0000),
                2 => sys::rechteck(20 << 16 | 30, 20 << 16 | 10, 0x00FF00),
                3 => sys::fenster_bewegen(300, 150, -1, -1),
                4 => sys::text(5 << 16 | 60, 0x90_000000, b"Text\0", 0, 0),
                _ => {}
            }
            zuletzt = s;
            FERTIG.store(s + 1, Ordering::SeqCst);
        }
        sys::warte_ereignis_max(1);
    }
}

static P: &[ProgrammEintrag] = &[ProgrammEintrag { name: "dirty", heap_kb: 256, stapel_kb: 128, start: prog }];

fn vergleiche(schirm: &mut Schirm) -> i64 {
    // zwei Runden, damit beide Puffer aktuell sind
    let mut fl = 0;
    for _ in 0..2 {
        if let Some(i) = schirm.aktualisieren() {
            fl = fl.max(schirm.letzte_flaeche);
            let (_, _, voll) = sys::foto();
            let teil: Vec<u32> = schirm.puffer[i].px.iter().map(|&c| kern::grafik::von565(c)).collect();
            assert!(teil == voll, "Teilbild weicht vom Vollbild ab");
        }
    }
    fl
}

#[test]
fn dirty_rects_gleich_vollbild_und_klein() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    let mut schirm = Schirm::default();
    let pid = kern::starte_name("dirty").unwrap();
    let t = Instant::now();
    let mut flaechen = Vec::new();
    for s in 0..5 {
        SCHRITT.store(s, Ordering::SeqCst);
        while FERTIG.load(Ordering::SeqCst) != s + 1 {
            assert!(t.elapsed() < Duration::from_secs(5), "Schritt {s}");
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(5));
        flaechen.push(vergleiche(&mut schirm));
        if s == 0 {
            // Start: beide Puffer wurden neu angelegt, einmal leer nachziehen
            vergleiche(&mut schirm);
        }
    }
    // ohne Änderung: nichts zu tun
    assert!(schirm.aktualisieren().is_none() && schirm.aktualisieren().is_none());
    // Pixel und kleines Rechteck setzen nur winzige Flächen neu
    eprintln!("Flächen je Schritt: {flaechen:?}");
    assert!(flaechen[1] <= 4, "Pixel: {}", flaechen[1]);
    assert!(flaechen[2] <= 31 * 11 * 2, "Rechteck: {}", flaechen[2]);
    // Verschieben: alter + neuer Platz (2 × 200×150), weniger als halber Schirm
    assert!(flaechen[3] <= 2 * 200 * 150 && flaechen[3] < 640 * 360 / 2, "Verschieben: {}", flaechen[3]);
    eprintln!("Flächen je Schritt: {flaechen:?}");
    kern::beende(pid);
}
