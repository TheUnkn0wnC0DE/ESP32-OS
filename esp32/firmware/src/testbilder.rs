//! Entwicklungs-Build `ESP32OS_TESTBILDER=1` (P2-Nachweis): legt die
//! Testbilder unter `/rd/1/bilder` ab (interner Speicher, nicht die SD) und
//! vergleicht Hardware- und Software-JPEG (Zeit, Abweichung, progressiv).

const BILDER: [(&str, &[u8]); 5] = [
    ("animation.gif", include_bytes!("../../desktop/tests/bilder/animation.gif")),
    ("farben.bmp", include_bytes!("../../desktop/tests/bilder/farben.bmp")),
    ("foto.jpg", include_bytes!("../../desktop/tests/bilder/foto.jpg")),
    ("progressiv.jpg", include_bytes!("../../desktop/tests/bilder/progressiv.jpg")),
    ("transparent.png", include_bytes!("../../desktop/tests/bilder/transparent.png")),
];

pub fn ablegen_und_messen() {
    let _ = kern::datei::ordner_erstellen("/rd/1/bilder");
    for (n, d) in BILDER {
        let (c, _) = kern::datei::erstellen(&format!("/rd/1/bilder/{n}"), d);
        log::info!("Testbild /rd/1/bilder/{n}: {} Bytes, Code {c}", d.len());
    }
    let foto = BILDER[2].1;
    // Lage von Y/Cb/Cr im YUV444-Pixel: alle Reihenfolgen gegen die Software
    if let Ok(sw) = kolibri::dekoder::jpeg_software(foto) {
        for lage in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            if let (Some(h), _, _) = crate::jpeg::dekodieren_lage(foto, lage) {
                let d: u64 = h.px.iter().zip(&sw.px).map(|(a, b)| [0, 8, 16].iter().map(|k| ((a >> k & 255) as i64 - (b >> k & 255) as i64).unsigned_abs()).sum::<u64>()).sum();
                log::info!("JPEG YUV-Lage {lage:?}: Abweichung Mittel {:.2}", d as f32 / (h.px.len() * 3) as f32);
            }
        }
    }
    let (hw, r, us_hw) = crate::jpeg::dekodieren_mit_code(foto);
    let t0 = crate::hw::now_us();
    let sw = kolibri::dekoder::jpeg_software(foto);
    let us_sw = crate::hw::now_us() - t0;
    match (&hw, &sw) {
        (Some(h), Ok(s)) if h.w == s.w && h.h == s.h => {
            let kanal = |p: u32, k: u32| (p >> k & 255) as i32;
            let (mut summe, mut max) = (0u64, 0i32);
            for (a, b) in h.px.iter().zip(&s.px) {
                for k in [0, 8, 16] {
                    let d = (kanal(*a, k) - kanal(*b, k)).abs();
                    summe += d as u64;
                    max = max.max(d);
                }
            }
            let mittel = summe as f32 / (h.px.len() * 3) as f32;
            let (a, b) = (h.px[40 * h.w as usize + 40], s.px[40 * s.w as usize + 40]);
            log::info!(
                "JPEG-Messung {}x{}: Hardware {:.1} ms, Software {:.1} ms; Abweichung HW/SW Mittel {mittel:.2}, max {max} (Feld rot HW {a:08x}, SW {b:08x})",
                h.w,
                h.h,
                us_hw as f32 / 1000.0,
                us_sw as f32 / 1000.0
            );
        }
        _ => log::error!("JPEG-Messung: HW {:?} (Code {r:#x}), SW {:?}", hw.as_ref().map(|b| (b.w, b.h)), sw.as_ref().map(|b| (b.w, b.h))),
    }
    let (p, r, _) = crate::jpeg::dekodieren_mit_code(BILDER[3].1);
    log::info!("JPEG progressiv über Hardware: {} (Code {r:#x})", if p.is_some() { "dekodiert" } else { "nicht unterstützt" });
    // B0b: progressiv in Software; gleiches Motiv wie foto.jpg (Baseline)
    let t0 = crate::hw::now_us();
    let prog = kolibri::dekoder::jpeg_software(BILDER[3].1);
    let us = crate::hw::now_us() - t0;
    match (&prog, &sw) {
        (Ok(p), Ok(s)) if p.w == s.w && p.h == s.h => {
            let d: u64 = p.px.iter().zip(&s.px).map(|(a, b)| [0, 8, 16].iter().map(|k| ((a >> k & 255) as i64 - (b >> k & 255) as i64).unsigned_abs()).sum::<u64>()).sum();
            log::info!(
                "JPEG progressiv über Software: {}x{} in {:.1} ms, Abweichung zum Baseline-Foto Mittel {:.2}",
                p.w,
                p.h,
                us as f32 / 1000.0,
                d as f32 / (p.px.len() * 3) as f32
            );
        }
        _ => log::error!("JPEG progressiv über Software: {:?}", prog.as_ref().map(|b| (b.w, b.h))),
    }
}
