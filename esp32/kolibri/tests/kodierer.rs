//! P6: PNG-Kodierer – Rundreise durch den eigenen Dekoder (Inflate wurde
//! gegen Pillow geprüft, siehe dekoder.rs) und Pillow-Gegenprobe als Datei.

use kolibri::dekoder::{self, Rgba};
use kolibri::kodierer;

fn bild(w: u32, h: u32, f: impl Fn(u32, u32) -> u32) -> Rgba {
    let px = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| 0xFF00_0000 | f(x, y) & 0xFF_FFFF).collect();
    Rgba { w, h, px, delay: 0 }
}

fn rundreise(b: &Rgba) -> usize {
    let png = kodierer::png(b);
    let (f, v) = dekoder::dekodieren(&png).expect("eigenes PNG lesbar");
    assert_eq!(f, dekoder::Format::Png);
    assert_eq!((v[0].w, v[0].h), (b.w, b.h));
    assert!(v[0].px == b.px, "Pixel gleich");
    png.len()
}

#[test]
fn png_rundreise_und_kompression() {
    // Oberfläche: Flächen, Verlauf, Text-ähnliche Muster
    let ui = bild(640, 360, |x, y| if y > 312 { 0x5A6478 } else if (x / 8 + y / 16) % 7 == 0 { 0x202020 } else { 0x3A6EA5 - (y / 20) * 0x010101 });
    let n = rundreise(&ui);
    assert!(n < 40_000, "Oberfläche komprimiert: {n} Bytes von {}", 640 * 360 * 3);
    // Rauschen (kein Treffer) und Wiederholungen über 32 KB Abstand
    let zufall: Vec<u32> = (0..97 * 61u32).scan(12345u32, |z, _| {
        *z = z.wrapping_mul(1_103_515_245).wrapping_add(12345);
        Some(*z >> 8)
    }).collect();
    let rausch = bild(97, 61, |x, y| zufall[(y * 97 + x) as usize]);
    rundreise(&rausch);
    rundreise(&bild(1, 1, |_, _| 0x123456));
    rundreise(&bild(300, 200, |x, y| (x * 7 ^ y * 13) * 0x10101));
    // Datei für die Gegenprobe mit Pillow (werkzeuge/png_pruefen.py)
    let ziel = std::env::temp_dir().join("esp32os_kodierer_ui.png");
    std::fs::write(&ziel, kodierer::png(&ui)).unwrap();
}
