//! Mauszeiger (Funktion 37.4–37.8) und Touchpad-Modus (Auftrag 4: Finger
//! bewegt den Zeiger relativ). Zeigerbilder im Windows-`.cur`-Format wie bei
//! KolibriOS; Standard ist der Original-Pfeil `kernel/trunk/video/arrow.cur`
//! (GPL-2.0, `daten/arrow.cur`).

/// Zeigerbild 32×32 (0xAARRGGBB) mit Hotspot.
#[derive(Clone, Debug)]
pub struct Zeigerbild {
    pub px: Vec<u32>,
    pub hot: (i32, i32),
}

pub static PFEIL: &[u8] = include_bytes!("../daten/arrow.cur");

fn u16le(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 2).map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])))
}
fn u32le(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Länge einer `.cur`-Datei aus ihrem Kopf (für 37.4 aus dem Speicher).
pub fn cur_laenge(d: &[u8]) -> Option<usize> {
    let off = u32le(d, 18)? as usize;
    let n = u32le(d, 14)? as usize;
    Some(off + n)
}

/// `.cur` lesen (erstes Bild, 1/4/8/24/32 bpp, AND-Maske), auf 32×32.
pub fn cur_lesen(d: &[u8]) -> Option<Zeigerbild> {
    if u16le(d, 0)? != 0 || u16le(d, 2)? != 2 || u16le(d, 4)? == 0 {
        return None;
    }
    let hot = (u16le(d, 10)? as i32, u16le(d, 12)? as i32);
    let off = u32le(d, 18)? as usize;
    let dib = d.get(off..)?;
    let kopf = u32le(dib, 0)? as usize;
    let w = u32le(dib, 4)? as usize;
    let h = (u32le(dib, 8)? as usize) / 2;
    let bpp = u16le(dib, 14)? as usize;
    if w == 0 || h == 0 || w > 256 || h > 256 {
        return None;
    }
    let pal_n = if bpp <= 8 { 1usize << bpp } else { 0 };
    let pal: Vec<u32> = (0..pal_n).map(|i| u32le(dib, kopf + 4 * i).unwrap_or(0) & 0xFF_FFFF).collect();
    let xor_off = kopf + 4 * pal_n;
    let xor_zeile = (w * bpp).div_ceil(32) * 4;
    let and_off = xor_off + xor_zeile * h;
    let and_zeile = w.div_ceil(32) * 4;
    let mut px = vec![0u32; 32 * 32];
    for y in 0..h.min(32) {
        let zy = h - 1 - y; // Zeilen von unten
        let z = dib.get(xor_off + zy * xor_zeile..xor_off + (zy + 1) * xor_zeile)?;
        let m = dib.get(and_off + zy * and_zeile..and_off + (zy + 1) * and_zeile);
        for x in 0..w.min(32) {
            let c = match bpp {
                1 => pal[usize::from(z[x / 8] >> (7 - x % 8) & 1)],
                4 => pal[usize::from(z[x / 2] >> (4 - 4 * (x % 2)) & 15)],
                8 => pal[usize::from(z[x])],
                24 => u32::from(z[3 * x + 2]) << 16 | u32::from(z[3 * x + 1]) << 8 | u32::from(z[3 * x]),
                32 => u32::from_le_bytes([z[4 * x], z[4 * x + 1], z[4 * x + 2], z[4 * x + 3]]),
                _ => return None,
            };
            let maske = m.is_some_and(|m| m[x / 8] >> (7 - x % 8) & 1 == 1);
            px[y * 32 + x] = if bpp == 32 {
                c
            } else if maske {
                0
            } else {
                0xFF00_0000 | c
            };
        }
    }
    Some(Zeigerbild { px, hot })
}

/// ARGB 32×32 aus dem Speicher (37.4, dx = 2).
pub fn argb(d: &[u8], hot: (i32, i32)) -> Option<Zeigerbild> {
    let b = d.get(..4096)?;
    Some(Zeigerbild { px: b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(), hot })
}

pub fn pfeil() -> Zeigerbild {
    cur_lesen(PFEIL).unwrap_or(Zeigerbild { px: vec![0xFF00_0000; 1024], hot: (0, 0) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kolibri_pfeil() {
        let z = cur_lesen(PFEIL).unwrap();
        assert_eq!(z.hot, (0, 0));
        // Spitze oben links deckend, rechts unten durchsichtig
        assert_eq!(z.px[0] >> 24, 255);
        assert_eq!(z.px[31 * 32 + 31] >> 24, 0);
        assert!(z.px.iter().filter(|&&p| p >> 24 == 255).count() > 50);
        assert_eq!(cur_laenge(PFEIL), Some(PFEIL.len()));
    }
}
