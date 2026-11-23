//! Grosse Bilder verkleinert dekodieren (Bildbetrachter, Handyfotos):
//! JPEG mit 1/2, 1/4, 1/8 in der IDCT, PNG zeilenweise; Datei stückweise.
//! Vergleich mit dem vollen Bild, gemittelt über Faktor×Faktor-Blöcke.

use kolibri::dekoder::{Fehler, Format, Rgba, dekodieren, verkleinert};
use std::path::Path;

fn datei(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).unwrap()
}

/// Wie eine Datei über Funktion 70: krumme Stücke zu höchstens 997 Byte.
fn stueckweise(d: &[u8]) -> impl FnMut(u64, &mut [u8]) -> usize + '_ {
    move |ab, puf| {
        let ab = ab as usize;
        if ab >= d.len() {
            return 0;
        }
        let n = puf.len().min(997).min(d.len() - ab);
        puf[..n].copy_from_slice(&d[ab..ab + n]);
        n
    }
}

/// Mittelwert je f×f-Block (abgerundet, wie der PNG-Weg).
fn mitteln(b: &Rgba, f: u32) -> Vec<u32> {
    let (ow, oh) = (b.w.div_ceil(f), b.h.div_ceil(f));
    let mut aus = Vec::new();
    for oy in 0..oh {
        for ox in 0..ow {
            let mut s = [0u32; 5];
            for y in oy * f..((oy + 1) * f).min(b.h) {
                for x in ox * f..((ox + 1) * f).min(b.w) {
                    let c = b.px[(y * b.w + x) as usize];
                    s[0] += c >> 24;
                    s[1] += c >> 16 & 255;
                    s[2] += c >> 8 & 255;
                    s[3] += c & 255;
                    s[4] += 1;
                }
            }
            aus.push(s[0] / s[4] << 24 | s[1] / s[4] << 16 | s[2] / s[4] << 8 | s[3] / s[4]);
        }
    }
    aus
}

/// EXIF 6: 90° im Uhrzeigersinn.
fn drehen6(px: &[u32], w: u32, h: u32) -> Vec<u32> {
    let mut aus = vec![0; px.len()];
    for y in 0..h {
        for x in 0..w {
            aus[(x * h + (h - 1 - y)) as usize] = px[(y * w + x) as usize];
        }
    }
    aus
}

/// Mittlere Abweichung der RGB-Kanäle und Anteil der Werte, die um mehr
/// als `grenze` abweichen. Die verkleinerte IDCT ist ein Tiefpass: an
/// scharfen Kanten innerhalb eines Blocks weicht sie vom Mittelwert ab
/// (libjpeg ebenso), bei 4:2:0 und 1/8 bleibt je 16×16 Punkte eine
/// Farbe; daher kein Höchstwert, sondern Mittel und Anteil.
fn abweichung(a: &[u32], b: &[u32], grenze: i32) -> (f64, f64) {
    assert_eq!(a.len(), b.len());
    let (mut summe, mut drueber) = (0u64, 0usize);
    for (p, q) in a.iter().zip(b) {
        for s in [0, 8, 16] {
            let d = ((p >> s & 255) as i32 - (q >> s & 255) as i32).abs();
            summe += d as u64;
            drueber += usize::from(d > grenze);
        }
    }
    let n = (a.len() * 3) as f64;
    (summe as f64 / n, drueber as f64 / n)
}

#[test]
fn jpeg_verkleinert_und_gedreht() {
    let d = datei("tests/gross/foto.jpg");
    let voll = dekodieren(&d).unwrap().1.remove(0);
    assert_eq!((voll.w, voll.h), (1600, 1200), "libimg: Rohbild, ungedreht");
    for (max_px, f) in [(600_000, 2), (200_000, 4), (40_000, 8)] {
        let v = verkleinert(stueckweise(&d), max_px, 64 << 20).unwrap();
        assert_eq!((v.format, v.original, v.teiler), (Format::Jpeg, (1600, 1200), f), "Faktor bei {max_px}");
        // EXIF 6: hochkant
        let (ow, oh) = (1600 / f, 1200 / f);
        assert_eq!((v.bild.w, v.bild.h), (oh, ow), "1/{f} gedreht");
        let soll = drehen6(&mitteln(&voll, f), ow, oh);
        let (mittel, anteil) = abweichung(&v.bild.px, &soll, 24);
        eprintln!("JPEG 1/{f}: mittlere Abweichung {mittel:.2}, Anteil > 24: {anteil:.4}");
        assert!(mittel < 2.0 && anteil < 0.02, "1/{f}: {mittel:.2} / {anteil:.4}");
    }
}

#[test]
fn progressives_jpeg_verkleinert() {
    let d = datei("tests/gross/foto_prog.jpg");
    let voll = dekodieren(&d).unwrap().1.remove(0);
    for (max_px, f) in [(2_000_000, 1), (200_000, 4), (40_000, 8)] {
        let v = verkleinert(stueckweise(&d), max_px, 64 << 20).unwrap();
        assert_eq!(v.teiler, f);
        assert_eq!((v.bild.w, v.bild.h), (1600 / f, 1200 / f));
        let (mittel, anteil) = abweichung(&v.bild.px, &mitteln(&voll, f), 24);
        eprintln!("progressiv 1/{f}: mittlere Abweichung {mittel:.2}, Anteil > 24: {anteil:.4}");
        if f == 1 {
            assert_eq!(v.bild.px, voll.px, "Faktor 1: gleich wie das volle Bild");
        } else {
            assert!(mittel < 2.0 && anteil < 0.02, "1/{f}: {mittel:.2} / {anteil:.4}");
        }
    }
    // die kleinen progressiven Testbilder (Pillow-Referenz) über den Strom
    for name in ["jpg_prog_420", "jpg_prog_444", "jpg_prog_grau", "jpg_prog_gross"] {
        let d = datei(&format!("tests/bilder/{name}.jpg"));
        let v = verkleinert(stueckweise(&d), 1 << 30, 64 << 20).unwrap();
        assert_eq!(v.bild.px, dekodieren(&d).unwrap().1[0].px, "{name}");
    }
}

#[test]
fn png_zeilenweise() {
    let d = datei("tests/gross/bild.png");
    let voll = dekodieren(&d).unwrap().1.remove(0);
    let v = verkleinert(stueckweise(&d), 200_000, 64 << 20).unwrap();
    assert_eq!((v.format, v.original, v.teiler), (Format::Png, (1600, 1200), 4));
    assert_eq!(v.bild.px, mitteln(&voll, 4), "Mittelwert genau");
    // ungerader Faktor mit Rand: 37×23 → Faktor 3
    let d = datei("tests/bilder/png_rgba8.png");
    let voll = dekodieren(&d).unwrap().1.remove(0);
    let v = verkleinert(stueckweise(&d), 120, 64 << 20).unwrap();
    assert_eq!((v.teiler, v.bild.w, v.bild.h), (3, 13, 8));
    assert_eq!(v.bild.px, mitteln(&voll, 3));
    // Interlace (Adam7): nächster Bildpunkt
    let d = datei("tests/bilder/png_adam7.png");
    let voll = dekodieren(&d).unwrap().1.remove(0);
    let v = verkleinert(stueckweise(&d), 100, 64 << 20).unwrap();
    assert_eq!((v.teiler, v.bild.w, v.bild.h), (4, 10, 6));
    for y in 0..6 {
        for x in 0..10 {
            assert_eq!(v.bild.px[(y * 10 + x) as usize], voll.px[(y * 4 * 37 + x * 4) as usize], "({x}, {y})");
        }
    }
}

#[test]
fn speichergrenze_und_kaputte_dateien() {
    let d = datei("tests/gross/foto.jpg");
    // selbst 1/8 braucht mehr als 100 KB → abgelehnt statt Speicher voll
    assert_eq!(verkleinert(stueckweise(&d), 1 << 30, 100_000).unwrap_err(), Fehler::ZuGross);
    // abgeschnitten: Fehler, kein Absturz
    let _ = verkleinert(stueckweise(&d[..d.len() / 2]), 200_000, 64 << 20);
    let p = datei("tests/gross/bild.png");
    assert!(verkleinert(stueckweise(&p[..p.len() / 2]), 200_000, 64 << 20).is_err());
    assert_eq!(verkleinert(stueckweise(b"GIF89a....."), 1000, 1 << 20).unwrap_err(), Fehler::NichtUnterstuetzt);
    // IHDR mit falscher Länge (15 MB): abgelehnt, ohne sie anzulegen
    let mut kaputt = p.clone();
    kaputt[8..12].copy_from_slice(&0x00F0_0000u32.to_be_bytes());
    assert_eq!(verkleinert(stueckweise(&kaputt), 200_000, 64 << 20).unwrap_err(), Fehler::Format);
    assert!(dekodieren(&kaputt).is_err());
    // DC-Tabelle mit Klasse 17 (> 16 Bit): Format-Fehler statt Überlauf
    let mut kaputt = d.clone();
    let dht = (0..kaputt.len() - 4).find(|&i| kaputt[i] == 0xFF && kaputt[i + 1] == 0xC4 && kaputt[i + 4] >> 4 == 0).unwrap();
    let anzahl: usize = kaputt[dht + 5..dht + 21].iter().map(|&n| usize::from(n)).sum();
    kaputt[dht + 21..dht + 21 + anzahl].fill(17);
    assert_eq!(verkleinert(stueckweise(&kaputt), 200_000, 64 << 20).unwrap_err(), Fehler::Format);
}

/// Graubild (nur Helligkeit, keine Farbunterabtastung): verkleinerte IDCT
/// gegen libjpeg (Pillow `draft`, `tests/gross/grau_libjpeg_*.png`). 1/8
/// (nur DC) ist gleich; bei 1/2 und 1/4 rechnet libjpeg die verkleinerte
/// IDCT etwas anders, beide sind Tiefpässe derselben Frequenzen.
#[test]
fn graues_jpeg_wie_libjpeg() {
    let d = datei("tests/gross/grau.jpg");
    for (max_px, f) in [(600_000, 2), (200_000, 4), (40_000, 8)] {
        let v = verkleinert(stueckweise(&d), max_px, 64 << 20).unwrap();
        assert_eq!(v.teiler, f);
        let r = datei(&format!("tests/gross/grau_libjpeg_{f}.png"));
        let soll = dekodieren(&r).unwrap().1.remove(0);
        assert_eq!((v.bild.w, v.bild.h), (soll.w, soll.h));
        let (mittel, anteil) = abweichung(&v.bild.px, &soll.px, 2);
        eprintln!("grau 1/{f} gegen libjpeg: mittlere Abweichung {mittel:.3}, Anteil > 2: {anteil:.4}");
        if f == 8 {
            assert_eq!(v.bild.px, soll.px, "1/8 wie libjpeg");
        } else {
            assert!(mittel < 0.5 && anteil < 0.03, "1/{f}: {mittel:.3} / {anteil:.4}");
        }
    }
}
