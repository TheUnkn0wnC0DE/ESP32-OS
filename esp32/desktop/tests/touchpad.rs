//! D10-Nachweis (PC): Touchpad-Modus – Finger bewegt den Zeiger relativ,
//! Tippen klickt am Zeiger (kleines Ziel 20×20), Halten = Rechtsklick,
//! Zeiger (KolibriOS-Pfeil) sichtbar. Dazu 37.4/37.5 (eigener Zeiger).

use desktop::gesten::{Gesten, TP_FAKTOR};
use kern::{KernAlloc, ProgrammEintrag, sys};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static KNOPF: AtomicU32 = AtomicU32::new(0);

fn ziel() {
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster(100 << 16 | 299, 60 << 16 | 199, 0x33 << 24 | 0xE6E6E6, 0, Some("Touchpad"));
                // winziges Ziel 20×20 bei (100, 50) im Arbeitsbereich
                sys::knopf(100 << 16 | 19, 50 << 16 | 19, 9, 0xC03030);
                sys::neuzeichnen(2);
            }
            3 => KNOPF.store(sys::knopf_id(), Ordering::SeqCst),
            _ => {}
        }
    }
}

static P: &[ProgrammEintrag] = &[ProgrammEintrag { name: "ziel", heap_kb: 256, stapel_kb: 128, start: ziel }];

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn schritt(g: &mut Gesten, t: &mut u64, f: [Option<(i32, i32)>; 2], dt: u64) {
    *t += dt;
    g.schritt(*t, f);
    std::thread::sleep(Duration::from_millis(15));
}

#[test]
fn touchpad_zeiger_tippen_halten() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::starte_name("ziel").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    sys::eingabe::touchpad_setzen(true);
    sys::eingabe::maus(300, 300, 0);
    let mut g = Gesten::default();
    let mut t = 1000u64;
    // Ziel: Fenster 100,60 + Arbeitsbereich 5,48 + (100,50) + Mitte 10 → (215, 168)
    let (zx, zy) = (215, 168);
    let (dx, dy) = ((zx - 300) * 10 / TP_FAKTOR, (zy - 300) * 10 / TP_FAKTOR);
    let start = (400, 330);
    schritt(&mut g, &mut t, [Some(start), None], 0);
    for i in 1..=10 {
        schritt(&mut g, &mut t, [Some((start.0 + dx * i / 10, start.1 + dy * i / 10)), None], 30);
    }
    schritt(&mut g, &mut t, [None, None], 30);
    let p = sys::eingabe::zeiger();
    assert!((p.0 - zx).abs() <= 2 && (p.1 - zy).abs() <= 2, "Zeiger bei {p:?}, Ziel ({zx}, {zy})");
    // Tippen irgendwo = Klick am Zeiger
    schritt(&mut g, &mut t, [Some((50, 50)), None], 500);
    schritt(&mut g, &mut t, [None, None], 80);
    std::thread::sleep(Duration::from_millis(60));
    schritt(&mut g, &mut t, [None, None], 100);
    bis(|| KNOPF.load(Ordering::SeqCst) == 9 << 8, "Klick am Zeiger");
    std::thread::sleep(Duration::from_millis(100));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join("d10_touchpad.png"), spatz::image::encode_png(w, h, &px)).unwrap();
    // Zeiger sichtbar: Pfeilspitze (schwarzer Rand) an der Zeigerposition
    assert_ne!(px[(p.1 as usize) * w + p.0 as usize], kern::grafik::von565(kern::grafik::zu565(0xC03030)), "Pfeil liegt über dem Ziel");
    // Halten = Rechtsklick am Zeiger
    KNOPF.store(0, Ordering::SeqCst);
    // > 450 ms Pause, sonst gilt „Tippen + gleich aufsetzen“ als Ziehen
    t += 600;
    for _ in 0..14 {
        schritt(&mut g, &mut t, [Some((50, 50)), None], 50);
    }
    schritt(&mut g, &mut t, [None, None], 50);
    std::thread::sleep(Duration::from_millis(60));
    schritt(&mut g, &mut t, [None, None], 100);
    bis(|| KNOPF.load(Ordering::SeqCst) == (9 << 8 | 2), "Rechtsklick am Zeiger");
    // Touchpad aus: Zeiger verschwindet, normales Tippen wirkt wieder an der Fingerstelle
    sys::eingabe::touchpad_setzen(false);
    std::thread::sleep(Duration::from_millis(50));
    let (_, _, px2) = sys::foto();
    let i = (p.1 as usize) * w + p.0 as usize;
    assert_ne!(px2[i], px[i], "ohne Touchpad kein Zeiger (Pfeilspitze weg)");
    let _ = h;
}
