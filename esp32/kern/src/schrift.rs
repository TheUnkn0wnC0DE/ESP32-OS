//! Systemschriften aus den KolibriOS-Originaldaten (GPL-2.0, siehe
//! `kern/daten/LIESMICH.md`): `char.mt` 6×9 cp866 (256 Zeichen à 9 Byte) und
//! `charUni.mt` 8×16 Unicode (1419 Zeichen à 16 Byte, Index = Codepunkt).
//! Bit 0 eines Zeilenbytes ist das linke Pixel (wie `drawChar` im Kern).
//!
//! Funktion 4 (Text) und 47 (Zahl) zeichnen über einen Rechteck-Rückruf, damit
//! dieselbe Logik ins Fenster und in Nutzerpuffer (32 bpp) schreiben kann.

static FONT1: &[u8; 2304] = include_bytes!("../daten/char.mt");
static UNI: &[u8; 22704] = include_bytes!("../daten/charUni.mt");
const UNI_ANZ: u32 = 1419;

pub(crate) static CP866: [u16; 128] = [
    0x0410, 0x0411, 0x0412, 0x0413, 0x0414, 0x0415, 0x0416, 0x0417, 0x0418, 0x0419, 0x041A, 0x041B, 0x041C, 0x041D, 0x041E, 0x041F,
    0x0420, 0x0421, 0x0422, 0x0423, 0x0424, 0x0425, 0x0426, 0x0427, 0x0428, 0x0429, 0x042A, 0x042B, 0x042C, 0x042D, 0x042E, 0x042F,
    0x0430, 0x0431, 0x0432, 0x0433, 0x0434, 0x0435, 0x0436, 0x0437, 0x0438, 0x0439, 0x043A, 0x043B, 0x043C, 0x043D, 0x043E, 0x043F,
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x2561, 0x2562, 0x2556, 0x2555, 0x2563, 0x2551, 0x2557, 0x255D, 0x255C, 0x255B, 0x2510,
    0x2514, 0x2534, 0x252C, 0x251C, 0x2500, 0x253C, 0x255E, 0x255F, 0x255A, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256C, 0x2567,
    0x2568, 0x2564, 0x2565, 0x2559, 0x2558, 0x2552, 0x2553, 0x256B, 0x256A, 0x2518, 0x250C, 0x2588, 0x2584, 0x258C, 0x2590, 0x2580,
    0x0440, 0x0441, 0x0442, 0x0443, 0x0444, 0x0445, 0x0446, 0x0447, 0x0448, 0x0449, 0x044A, 0x044B, 0x044C, 0x044D, 0x044E, 0x044F,
    0x0401, 0x0451, 0x0404, 0x0454, 0x0407, 0x0457, 0x040E, 0x045E, 0x00B0, 0x2219, 0x00B7, 0x221A, 0x2116, 0x00A4, 0x25A0, 0x00A0,
];

pub(crate) static CP1251: [u16; 128] = [
    0x0402, 0x0403, 0x201A, 0x0453, 0x201E, 0x2026, 0x2020, 0x2021, 0x20AC, 0x2030, 0x0409, 0x2039, 0x040A, 0x040C, 0x040B, 0x040F,
    0x0452, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0x0459, 0x203A, 0x045A, 0x045C, 0x045B, 0x045F,
    0x00A0, 0x040E, 0x045E, 0x0408, 0x00A4, 0x0490, 0x00A6, 0x00A7, 0x0401, 0x00A9, 0x0404, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x0407,
    0x00B0, 0x00B1, 0x0406, 0x0456, 0x0491, 0x00B5, 0x00B6, 0x00B7, 0x0451, 0x2116, 0x0454, 0x00BB, 0x0458, 0x0405, 0x0455, 0x0457,
    0x0410, 0x0411, 0x0412, 0x0413, 0x0414, 0x0415, 0x0416, 0x0417, 0x0418, 0x0419, 0x041A, 0x041B, 0x041C, 0x041D, 0x041E, 0x041F,
    0x0420, 0x0421, 0x0422, 0x0423, 0x0424, 0x0425, 0x0426, 0x0427, 0x0428, 0x0429, 0x042A, 0x042B, 0x042C, 0x042D, 0x042E, 0x042F,
    0x0430, 0x0431, 0x0432, 0x0433, 0x0434, 0x0435, 0x0436, 0x0437, 0x0438, 0x0439, 0x043A, 0x043B, 0x043C, 0x043D, 0x043E, 0x043F,
    0x0440, 0x0441, 0x0442, 0x0443, 0x0444, 0x0445, 0x0446, 0x0447, 0x0448, 0x0449, 0x044A, 0x044B, 0x044C, 0x044D, 0x044E, 0x044F,
];

/// cp1251-Byte → Unicode (Zwischenablage, Kodierung 2).
pub fn cp1251_zu_uni(b: u8) -> u32 {
    if b < 0x80 { u32::from(b) } else { u32::from(CP1251[usize::from(b - 0x80)]) }
}

/// cp866-Byte → Unicode.
pub fn cp866_zu_uni(b: u8) -> u32 {
    if b < 0x80 { u32::from(b) } else { u32::from(CP866[usize::from(b - 0x80)]) }
}

/// Unicode → cp866-Byte (für Texte aus Rust-Programmen in Schrift 0/1).
pub fn uni_zu_cp866(c: char) -> u8 {
    let u = c as u32;
    if u < 0x80 {
        return u as u8;
    }
    CP866.iter().position(|&x| u32::from(x) == u).map_or(b'?', |i| 0x80 + i as u8)
}

/// Schriftart aus den FF-Bits von Funktion 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kodierung {
    Cp866Klein,
    Cp866,
    Utf16,
    Utf8,
}

impl Kodierung {
    pub fn aus_bits(ff: u32) -> Kodierung {
        match ff & 3 {
            0 => Kodierung::Cp866Klein,
            1 => Kodierung::Cp866,
            2 => Kodierung::Utf16,
            _ => Kodierung::Utf8,
        }
    }
    /// Zeichenzelle (Breite, Höhe) bei Faktor 1.
    pub fn zelle(self) -> (i32, i32) {
        if self == Kodierung::Cp866Klein { (6, 9) } else { (8, 16) }
    }
}

/// Zerlegt Bytes in Zeichen (Glyph-Daten, Breite). `null_ende`: an 0 stoppen;
/// `max`: höchstens so viele Zeichen.
pub fn zeichen(bytes: &[u8], k: Kodierung, null_ende: bool, max: usize) -> Vec<u32> {
    let mut v = Vec::new();
    let mut i = 0;
    while v.len() < max && i < bytes.len() {
        let c = match k {
            Kodierung::Cp866Klein | Kodierung::Cp866 => {
                i += 1;
                u32::from(bytes[i - 1])
            }
            Kodierung::Utf16 => {
                if i + 1 >= bytes.len() {
                    break;
                }
                i += 2;
                u32::from(u16::from_le_bytes([bytes[i - 2], bytes[i - 1]]))
            }
            Kodierung::Utf8 => {
                let b = bytes[i];
                let (n, mut c) = match b {
                    0..0x80 => (1, u32::from(b)),
                    0xC0..0xE0 => (2, u32::from(b & 0x1F)),
                    0xE0..0xF0 => (3, u32::from(b & 0x0F)),
                    0xF0..0xF8 => (4, u32::from(b & 0x07)),
                    _ => (1, 0),
                };
                for k in 1..n {
                    c = c << 6 | u32::from(bytes.get(i + k).copied().unwrap_or(0x80) & 0x3F);
                }
                i += n;
                c
            }
        };
        if null_ende && c == 0 {
            break;
        }
        v.push(c);
    }
    v
}

/// Glyphe der Systemschrift 8×16 (Unicode) – Zeile 0 oben, Bit 0 links;
/// unbekannte Zeichen wie beim Textzeichnen (PDF-Betrachter).
pub fn glyphe(c: char) -> [u8; 16] {
    let mut g = [0u8; 16];
    g.copy_from_slice(&glyph(c as u32, Kodierung::Utf8)[..16]);
    g
}

/// Glyph-Zeilen eines Zeichens (nach `zeichen`).
fn glyph(c: u32, k: Kodierung) -> &'static [u8] {
    match k {
        Kodierung::Cp866Klein => {
            let o = (c as usize & 0xFF) * 9;
            &FONT1[o..o + 9]
        }
        _ => {
            let u = if k == Kodierung::Cp866 { cp866_zu_uni(c as u8) } else { c };
            let u = if u < UNI_ANZ { u } else { 0 } as usize;
            &UNI[u * 16..u * 16 + 16]
        }
    }
}

/// Zeichnet Zeichen ab (x, y) mit Faktor `mult` über `rect(x, y, w, h)`
/// (Vordergrund) und optional `hg(x, y, w, h)` (Hintergrund je Zelle).
/// Liefert die Breite in Pixeln.
pub fn zeichnen(
    zeichen: &[u32],
    k: Kodierung,
    mult: i32,
    x: i32,
    y: i32,
    rect: &mut dyn FnMut(i32, i32, i32, i32),
    mut hg: Option<&mut dyn FnMut(i32, i32, i32, i32)>,
) -> i32 {
    let (zw, zh) = k.zelle();
    let mut px = x;
    for &c in zeichen {
        if let Some(h) = hg.as_mut() {
            h(px, y, zw * mult, zh * mult);
        }
        for (r, &bits) in glyph(c, k).iter().enumerate() {
            let mut b = bits;
            while b != 0 {
                let bit = b.trailing_zeros() as i32;
                // zusammenhängende Bits als ein Rechteck
                let lauf = (b >> bit).trailing_ones() as i32;
                rect(px + bit * mult, y + r as i32 * mult, lauf * mult, mult);
                b &= !(((1u16 << lauf) - 1) << bit) as u8;
            }
        }
        px += zw * mult;
    }
    px - x
}

/// Funktion 47: Zahl in Text umsetzen. `ebx` wie im Systemaufruf (Basis in
/// bh, Stellen in Bits 16–21, Bit 31 = führende Nullen weglassen).
pub fn zahl_text(wert: u64, ebx: u32) -> Vec<u8> {
    let basis = match ebx >> 8 & 0xFF {
        1 => 16,
        2 => 2,
        _ => 10,
    };
    let stellen = (ebx >> 16 & 0x3F) as usize;
    let mut z = Vec::new();
    let mut w = wert;
    for _ in 0..stellen {
        let d = (w % basis) as u8;
        z.push(if d < 10 { b'0' + d } else { b'A' + d - 10 });
        w /= basis;
    }
    z.reverse();
    if ebx & 0x8000_0000 != 0 {
        let erste = z.iter().position(|&c| c != b'0').unwrap_or(z.len().saturating_sub(1));
        z.drain(..erste);
    }
    z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_und_umlaute() {
        let z = zeichen("Grüezi".as_bytes(), Kodierung::Utf8, false, 100);
        assert_eq!(z, ['G', 'r', 'ü', 'e', 'z', 'i'].map(|c| c as u32));
        assert_eq!(uni_zu_cp866('Ж'), 0x86);
        assert_eq!(cp866_zu_uni(0x86), 'Ж' as u32);
    }

    #[test]
    fn zahl() {
        assert_eq!(zahl_text(42, 5 << 16), b"00042");
        assert_eq!(zahl_text(42, 5 << 16 | 0x8000_0000), b"42");
        assert_eq!(zahl_text(255, 2 << 16 | 1 << 8), b"FF");
        assert_eq!(zahl_text(0, 3 << 16 | 0x8000_0000), b"0");
    }

    #[test]
    fn glyph_a_hat_pixel() {
        let mut n = 0;
        zeichnen(&[u32::from(b'A')], Kodierung::Cp866Klein, 1, 0, 0, &mut |_, _, w, h| n += w * h, None);
        assert!(n > 10);
    }
}
