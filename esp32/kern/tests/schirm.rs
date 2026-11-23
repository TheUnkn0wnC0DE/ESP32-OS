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
        if s == 1 {
            // O1: nur die Zeile des Pixels hat sich geändert
            let z = schirm.letzte_zeilen.expect("Zeilen");
            assert!(z.1 - z.0 <= 2 && z.0 > 0, "Zeilen nach dem Pixel: {z:?}");
        }
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

/// O1: Gleiche Felder aus beiden Bildern des Doppelpuffers zählen einmal;
/// statt des ganzen Bildschirms wird höchstens die Hülle zusammengesetzt.
#[test]
fn rechtecke_zusammenfassen_scrollen() {
    use kern::grafik::Clip;
    let feld = Clip { x0: 5, y0: 40, x1: 635, y1: 300 };
    // Scrollen: dasselbe Feld jetzt und im vorigen Bild, dazu der Rollbalken
    let balken = Clip { x0: 630, y0: 40, x1: 640, y1: 300 };
    let r = sys::rechtecke_zusammenfassen(vec![feld, balken, feld]);
    let huelle = Clip { x0: 5, y0: 40, x1: 640, y1: 300 };
    assert_eq!(r, vec![huelle], "ein Feld plus Rollbalken → Hülle");
    assert!(huelle.flaeche() < 640 * 360, "nicht der ganze Bildschirm");
    // enthaltenes Rechteck entfällt, getrennte kleine bleiben einzeln
    let klein = Clip { x0: 10, y0: 50, x1: 20, y1: 60 };
    assert_eq!(sys::rechtecke_zusammenfassen(vec![feld, klein]), vec![feld]);
    let a = Clip { x0: 0, y0: 0, x1: 10, y1: 10 };
    let b = Clip { x0: 600, y0: 300, x1: 610, y1: 310 };
    assert_eq!(sys::rechtecke_zusammenfassen(vec![a, b, a]), vec![a, b]);
    assert!(sys::rechtecke_zusammenfassen(vec![]).is_empty());
}
