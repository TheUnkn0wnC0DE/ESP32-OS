//! Absicherung (P1): Ein Fenster der obersten Ebene (wie die
//! Bildschirmtastatur) wird minimiert und wiederhergestellt; Teilbild
//! (Dirty-Rects) und Vollbild müssen jeweils gleich sein und das Fenster
//! sichtbar/unsichtbar. Den Board-Fehler (Tastatur beim zweiten Einblenden
//! unsichtbar, `docs/nachweise/p1_board_tastatur_*.png`) bildet der Test am
//! PC nicht nach; behoben in `Welt::aktivieren` (wiederhergestellt → immer
//! neu zeichnen), am Board per Foto bestätigt.

use kern::sys::{self, Schirm};
use kern::{KernAlloc, ProgrammEintrag};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static SCHRITT: AtomicU32 = AtomicU32::new(0);
static FERTIG: AtomicU32 = AtomicU32::new(0);

fn unten() {
    sys::pruefe_ereignis();
    sys::fenster(20 << 16 | 399, 20 << 16 | 299, 0x33_FFFFFF, 0, Some("Unten"));
    loop {
        sys::warte_ereignis_max(5);
    }
}

fn oben() {
    let mut zuletzt = u32::MAX;
    let slot = sys::system(21, kern::eigene_tid().unwrap_or(0));
    loop {
        let s = SCHRITT.load(Ordering::SeqCst);
        if s != zuletzt {
            match s {
                0 => {
                    sys::pruefe_ereignis();
                    sys::fenster(0 << 16 | 639, 150 << 16 | 99, 0x01_000000, 0, None);
                    sys::fensterebene(2, u32::MAX, 1);
                    sys::rechteck(639, 99, 0x2C3444);
                }
                1 => {
                    sys::minimieren(0, slot);
                }
                2 => {
                    sys::minimieren(2, slot);
                }
                _ => {}
            }
            zuletzt = s;
            FERTIG.store(s + 1, Ordering::SeqCst);
        }
        sys::warte_ereignis_max(1);
    }
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "unten", heap_kb: 256, stapel_kb: 128, start: unten },
    ProgrammEintrag { name: "oben", heap_kb: 256, stapel_kb: 128, start: oben },
];

fn vergleiche(schirm: &mut Schirm, was: &str) {
    for _ in 0..2 {
        if let Some(i) = schirm.aktualisieren() {
            let (_, _, voll) = sys::foto();
            let teil: Vec<u32> = schirm.puffer[i].px.iter().map(|&c| kern::grafik::von565(c)).collect();
            assert!(teil == voll, "Teilbild weicht vom Vollbild ab: {was}");
        }
    }
}

#[test]
fn oberste_ebene_wiederherstellen_zeichnet_neu() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    let mut schirm = Schirm::default();
    kern::starte_name("unten").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    kern::starte_name("oben").unwrap();
    for s in 0..3 {
        SCHRITT.store(s, Ordering::SeqCst);
        let t = Instant::now();
        while FERTIG.load(Ordering::SeqCst) <= s {
            assert!(t.elapsed() < Duration::from_secs(5), "Schritt {s}");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(50));
        vergleiche(&mut schirm, ["gezeigt", "minimiert", "wiederhergestellt"][s as usize]);
        let (w, _, px) = sys::foto();
        let da = px[200 * w + 320] == kern::grafik::von565(kern::grafik::zu565(0x2C3444));
        assert_eq!(da, s != 1, "oberes Fenster sichtbar nach Schritt {s}");
    }
    // Pixel in der Mitte des oberen Fensters ist wieder dessen Farbe
    let (w, _, px) = sys::foto();
    assert_eq!(px[200 * w + 320], kern::grafik::von565(kern::grafik::zu565(0x2C3444)));
}
