//! K7-Nachweis libimg-Dekoder: Vergleich mit Pillow (Referenz `.rgba`).
//! PNG/BMP/GIF bitgenau (Farbe; Alpha bei BMP immer 255), JPEG mit Toleranz.

use kolibri::dekoder::{dekodieren, Format};
use std::path::Path;

fn referenz(p: &Path) -> Vec<(u32, u32, Vec<u32>)> {
    let d = std::fs::read(p).unwrap();
    let mut o = 0;
    let mut v = Vec::new();
    while o < d.len() {
        let w = u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let h = u32::from_le_bytes(d[o + 4..o + 8].try_into().unwrap());
        o += 8;
        let px = d[o..o + (w * h * 4) as usize]
            .chunks_exact(4)
            .map(|c| u32::from(c[3]) << 24 | u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2]))
            .collect();
        o += (w * h * 4) as usize;
        v.push((w, h, px));
    }
    v
}

#[test]
fn alle_testbilder_wie_pillow() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bilder");
    let mut geprueft = 0;
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "rgba") {
            continue;
        }
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        let daten = std::fs::read(&p).unwrap();
        let (fmt, frames) = dekodieren(&daten).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let soll = referenz(&p.with_extension("rgba"));
        assert_eq!(frames.len(), soll.len(), "{name}: Anzahl Bilder");
        for (i, (f, (w, h, px))) in frames.iter().zip(&soll).enumerate() {
            assert_eq!((f.w, f.h), (*w, *h), "{name}[{i}]: Grösse");
            if fmt == Format::Jpeg {
                let mut summe = 0u64;
                let mut max = 0i32;
                for (a, b) in f.px.iter().zip(px) {
                    for s in [0, 8, 16] {
                        let d = ((a >> s & 255) as i32 - (b >> s & 255) as i32).abs();
                        summe += d as u64;
                        max = max.max(d);
                    }
                }
                let mittel = summe as f64 / (f.px.len() * 3) as f64;
                assert!(mittel < 3.0 && max < 64, "{name}: mittlere Abweichung {mittel:.2}, max {max}");
            } else {
                for (k, (a, b)) in f.px.iter().zip(px).enumerate() {
                    // ganz durchsichtige Pixel: Farbe egal
                    if a >> 24 == 0 && b >> 24 == 0 {
                        continue;
                    }
                    let (a2, b2) = if fmt == Format::Bmp { (a & 0xFF_FFFF, b & 0xFF_FFFF) } else { (*a, *b) };
                    assert_eq!(a2, b2, "{name}[{i}] Pixel {k} ({},{})", k as u32 % w, k as u32 / w);
                }
            }
        }
        geprueft += 1;
    }
    assert!(geprueft >= 16, "nur {geprueft} Bilder geprüft");
}

#[test]
fn bmp_rle8_von_hand() {
    // 4×2, RLE8: Zeile 0 (unten): 4× Index 1; Zeile 1: absolut 0,1,0,1
    let mut d = Vec::new();
    let pal = [[0u8, 0, 0, 0], [0, 0, 255, 0]];
    let daten: Vec<u8> = vec![4, 1, 0, 0, 0, 4, 0, 1, 0, 1, 0, 1];
    let off = 14 + 40 + 8;
    d.extend_from_slice(b"BM");
    d.extend_from_slice(&((off + daten.len()) as u32).to_le_bytes());
    d.extend_from_slice(&[0; 4]);
    d.extend_from_slice(&(off as u32).to_le_bytes());
    for v in [40u32, 4, 2] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&8u16.to_le_bytes());
    for v in [1u32, daten.len() as u32, 0, 0, 2, 0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for p in pal {
        d.extend_from_slice(&p);
    }
    d.extend_from_slice(&daten);
    let (_, f) = dekodieren(&d).unwrap();
    let rot = 0xFFFF_0000;
    let sw = 0xFF00_0000;
    assert_eq!(f[0].px, vec![sw, rot, sw, rot, rot, rot, rot, rot]);
}

#[test]
fn logo_png_aus_dem_original() {
    let d = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("daten/logo.png")).unwrap();
    let (fmt, f) = dekodieren(&d).unwrap();
    assert_eq!(fmt, Format::Png);
    assert_eq!((f[0].w, f[0].h), (256, 256));
}

/// B0b: progressive JPEGs (SOF2, Spektralauswahl + schrittweise Näherung)
/// so genau wie Baseline – mittlere Abweichung zu Pillow (libjpeg) < 1.
#[test]
fn progressive_jpegs_genau() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bilder");
    for name in ["jpg_prog_444", "jpg_prog_420", "jpg_prog_grau", "jpg_prog_gross", "jpg_444", "jpg_420"] {
        let daten = std::fs::read(dir.join(format!("{name}.jpg"))).unwrap();
        let (_, frames) = dekodieren(&daten).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let (w, h, px) = &referenz(&dir.join(format!("{name}.rgba")))[0];
        assert_eq!((frames[0].w, frames[0].h), (*w, *h), "{name}");
        let mut summe = 0u64;
        let mut max = 0i32;
        for (a, b) in frames[0].px.iter().zip(px) {
            for s in [0, 8, 16] {
                let d = ((a >> s & 255) as i32 - (b >> s & 255) as i32).abs();
                summe += d as u64;
                max = max.max(d);
            }
        }
        let mittel = summe as f64 / (frames[0].px.len() * 3) as f64;
        eprintln!("{name}: mittlere Abweichung {mittel:.3}, max {max}");
        assert!(mittel < 1.0 && max < 32, "{name}: mittlere Abweichung {mittel:.3}, max {max}");
    }
}
