//! Hardware-JPEG (P2) für `kolibri::dekoder`: JPEG-Einheit des ESP32-P4 über
//! `komponenten/esp32os/jpeg.c`. Die Einheit liefert YUV444 (bzw. grau); die
//! Umrechnung nach RGB geschieht hier mit vollem JFIF-Bereich (die DMA2D-
//! Farbumrechnung rechnet mit TV-Bereich und übersättigt die Farben).
//! Progressive JPEGs kann die Einheit nicht (Treiber: nur Baseline) →
//! `None`, der Dekoder nimmt dann die Software.

use core::ffi::c_void;
use kolibri::dekoder::Rgba;

unsafe extern "C" {
    fn esp32os_jpeg_dekodieren(
        daten: *const u8,
        laenge: u32,
        ziel: *mut c_void,
        fertig: extern "C" fn(*mut c_void, *const u8, u32, u32, u32, u32),
    ) -> i32;
}

/// ESP-IDF-Codes, mit denen der Treiber progressive JPEGs ablehnt.
pub const NICHT_GEFUNDEN: i32 = 0x105;
pub const NICHT_UNTERSTUETZT: i32 = 0x106;

/// Lage von Y, Cb, Cr im gepackten YUV444-Pixel der JPEG-Einheit (am Board
/// gemessen, siehe `testbilder`: beste Übereinstimmung mit der Software).
pub const YUV_LAGE: [usize; 3] = [2, 1, 0];

/// YCbCr (JFIF, voller Bereich) → 0xFFRRGGBB.
pub fn ycbcr(y: u8, cb: u8, cr: u8) -> u32 {
    let (y, cb, cr) = (i32::from(y) << 16, i32::from(cb) - 128, i32::from(cr) - 128);
    let k = |v: i32| ((v + (1 << 15)) >> 16).clamp(0, 255) as u32;
    let r = k(y + 91_881 * cr);
    let g = k(y - 22_554 * cb - 46_802 * cr);
    let b = k(y + 116_130 * cb);
    0xFF00_0000 | r << 16 | g << 8 | b
}

struct Ziel {
    bild: Option<Rgba>,
    lage: [usize; 3],
}

extern "C" fn fertig(ziel: *mut c_void, px: *const u8, w: u32, h: u32, zb: u32, bpp: u32) {
    // SAFETY: `ziel` zeigt auf das `Ziel` des Aufrufers (gleicher Faden);
    // `px` hat zb × h × bpp Bytes (jpeg.c, Zeilen auf MCUs aufgerundet).
    let z = unsafe { &mut *(ziel as *mut Ziel) };
    let quelle = unsafe { core::slice::from_raw_parts(px, (zb * h * bpp) as usize) };
    let mut v: Vec<u32> = Vec::new();
    // Speicher knapp → kein Absturz, sondern „nicht dekodiert“
    if v.try_reserve_exact((w * h) as usize).is_err() {
        return;
    }
    let [iy, iu, iv] = z.lage;
    for y in 0..h {
        let zeile = &quelle[(y * zb * bpp) as usize..((y * zb + w) * bpp) as usize];
        if bpp == 1 {
            v.extend(zeile.iter().map(|&g| 0xFF00_0000 | u32::from(g) * 0x01_0101));
        } else {
            v.extend(zeile.chunks_exact(3).map(|p| ycbcr(p[iy], p[iu], p[iv])));
        }
    }
    z.bild = Some(Rgba { w, h, px: v, delay: 0 });
}

/// Dekodieren mit gegebener YUV-Lage: (Bild, Fehlercode, µs).
pub fn dekodieren_lage(d: &[u8], lage: [usize; 3]) -> (Option<Rgba>, i32, i64) {
    let mut z = Ziel { bild: None, lage };
    let t0 = crate::hw::now_us();
    // SAFETY: Puffer gültig während des Aufrufs; `fertig` schreibt in `z`.
    let r = unsafe { esp32os_jpeg_dekodieren(d.as_ptr(), d.len() as u32, (&raw mut z).cast(), fertig) };
    (if r == 0 { z.bild } else { None }, r, crate::hw::now_us() - t0)
}

/// Dekodieren mit Fehlercode (Nachweis/Messung).
pub fn dekodieren_mit_code(d: &[u8]) -> (Option<Rgba>, i32, i64) {
    dekodieren_lage(d, YUV_LAGE)
}

/// Haken für `kolibri::dekoder::jpeg_hw_setzen`.
pub fn dekodieren(d: &[u8]) -> Option<Rgba> {
    let (b, r, us) = dekodieren_mit_code(d);
    match &b {
        Some(b) => log::info!("JPEG HW: {}x{} in {:.1} ms", b.w, b.h, us as f32 / 1000.0),
        None if r == NICHT_UNTERSTUETZT || r == NICHT_GEFUNDEN => log::info!("JPEG HW: kein Baseline-JPEG (progressiv?) → Software"),
        None => log::warn!("JPEG HW: Fehler {r:#x} → Software"),
    }
    b
}
