//! Eingebaute 5×7-Bitmap-Schrift (ASCII-Großbuchstaben, Ziffern, Satzzeichen,
//! deutsche Umlaute). Kleinbuchstaben werden als Großbuchstaben dargestellt.

use super::framebuffer::Framebuffer;

pub const GLYPH_W: i32 = 5;
pub const GLYPH_H: i32 = 7;
/// Horizontaler Abstand pro Zeichen (inkl. 1 px Lücke).
pub const ADVANCE: i32 = 6;
/// Zeilenhöhe.
pub const LINE_H: i32 = 9;

/// Wandelt „Pixel-Art“ (`#` = an, `.` = aus, Leerzeichen ignoriert) in
/// 7 Zeilen-Bitmasken um (Bit 4 = linke Spalte).
const fn g(s: &str) -> [u8; 7] {
    let b = s.as_bytes();
    let mut rows = [0u8; 7];
    let mut i = 0;
    let mut n = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'#' || c == b'.' {
            if c == b'#' {
                rows[n / 5] |= 1 << (4 - (n % 5));
            }
            n += 1;
        }
        i += 1;
    }
    rows
}

const BLANK: [u8; 7] = [0; 7];

const GLYPHS: &[(char, [u8; 7])] = &[
    ('!', g("..#.. ..#.. ..#.. ..#.. ..#.. ..... ..#..")),
    ('"', g(".#.#. .#.#. ..... ..... ..... ..... .....")),
    ('#', g(".#.#. ##### .#.#. .#.#. .#.#. ##### .#.#.")),
    ('%', g("##..# ##..# ...#. ..#.. .#... #..## #..##")),
    ('\'', g("..#.. ..#.. ..... ..... ..... ..... .....")),
    ('(', g("...#. ..#.. .#... .#... .#... ..#.. ...#.")),
    (')', g(".#... ..#.. ...#. ...#. ...#. ..#.. .#...")),
    ('*', g("..... ..#.. #.#.# .###. #.#.# ..#.. .....")),
    ('+', g("..... ..#.. ..#.. ##### ..#.. ..#.. .....")),
    (',', g("..... ..... ..... ..... ..... ..#.. .#...")),
    ('-', g("..... ..... ..... ##### ..... ..... .....")),
    ('.', g("..... ..... ..... ..... ..... ..... ..#..")),
    ('/', g("....# ....# ...#. ..#.. .#... #.... #....")),
    ('0', g(".###. #...# #..## #.#.# ##..# #...# .###.")),
    ('1', g("..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###.")),
    ('2', g(".###. #...# ....# ...#. ..#.. .#... #####")),
    ('3', g("####. ....# ....# .###. ....# ....# ####.")),
    ('4', g("...#. ..##. .#.#. #..#. ##### ...#. ...#.")),
    ('5', g("##### #.... ####. ....# ....# #...# .###.")),
    ('6', g("..##. .#... #.... ####. #...# #...# .###.")),
    ('7', g("##### ....# ...#. ..#.. .#... .#... .#...")),
    ('8', g(".###. #...# #...# .###. #...# #...# .###.")),
    ('9', g(".###. #...# #...# .#### ....# ...#. .##..")),
    (':', g("..... ..#.. ..... ..... ..... ..#.. .....")),
    (';', g("..... ..#.. ..... ..... ..... ..#.. .#...")),
    ('<', g("...#. ..#.. .#... #.... .#... ..#.. ...#.")),
    ('=', g("..... ..... ##### ..... ##### ..... .....")),
    ('>', g(".#... ..#.. ...#. ....# ...#. ..#.. .#...")),
    ('?', g(".###. #...# ....# ...#. ..#.. ..... ..#..")),
    ('A', g(".###. #...# #...# ##### #...# #...# #...#")),
    ('B', g("####. #...# #...# ####. #...# #...# ####.")),
    ('C', g(".###. #...# #.... #.... #.... #...# .###.")),
    ('D', g("####. #...# #...# #...# #...# #...# ####.")),
    ('E', g("##### #.... #.... ####. #.... #.... #####")),
    ('F', g("##### #.... #.... ####. #.... #.... #....")),
    ('G', g(".###. #...# #.... #.### #...# #...# .####")),
    ('H', g("#...# #...# #...# ##### #...# #...# #...#")),
    ('I', g(".###. ..#.. ..#.. ..#.. ..#.. ..#.. .###.")),
    ('J', g("..### ...#. ...#. ...#. ...#. #..#. .##..")),
    ('K', g("#...# #..#. #.#.. ##... #.#.. #..#. #...#")),
    ('L', g("#.... #.... #.... #.... #.... #.... #####")),
    ('M', g("#...# ##.## #.#.# #.#.# #...# #...# #...#")),
    ('N', g("#...# #...# ##..# #.#.# #..## #...# #...#")),
    ('O', g(".###. #...# #...# #...# #...# #...# .###.")),
    ('P', g("####. #...# #...# ####. #.... #.... #....")),
    ('Q', g(".###. #...# #...# #...# #.#.# #..#. .##.#")),
    ('R', g("####. #...# #...# ####. #.#.. #..#. #...#")),
    ('S', g(".#### #.... #.... .###. ....# ....# ####.")),
    ('T', g("##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#..")),
    ('U', g("#...# #...# #...# #...# #...# #...# .###.")),
    ('V', g("#...# #...# #...# #...# #...# .#.#. ..#..")),
    ('W', g("#...# #...# #...# #.#.# #.#.# #.#.# .#.#.")),
    ('X', g("#...# #...# .#.#. ..#.. .#.#. #...# #...#")),
    ('Y', g("#...# #...# .#.#. ..#.. ..#.. ..#.. ..#..")),
    ('Z', g("##### ....# ...#. ..#.. .#... #.... #####")),
    ('[', g(".###. .#... .#... .#... .#... .#... .###.")),
    (']', g(".###. ...#. ...#. ...#. ...#. ...#. .###.")),
    ('_', g("..... ..... ..... ..... ..... ..... #####")),
    ('|', g("..#.. ..#.. ..#.. ..#.. ..#.. ..#.. ..#..")),
    ('Ä', g("#...# .###. #...# #...# ##### #...# #...#")),
    ('Ö', g("#...# .###. #...# #...# #...# #...# .###.")),
    ('Ü', g("#...# ..... #...# #...# #...# #...# .###.")),
    ('ß', g(".##.. #..#. #..#. #.#.. #..#. #..#. #.##.")),
    ('°', g(".##.. #..#. .##.. ..... ..... ..... .....")),
    // Symbole für HUDs: Herz, Kirsche, Eiskristall, Pfeile.
    ('♥', g("..... .#.#. ##### ##### .###. ..#.. .....")),
    ('●', g("..... .###. ##### ##### ##### .###. .....")),
    ('→', g("..... ..#.. ...#. ##### ...#. ..#.. .....")),
    ('←', g("..... ..#.. .#... ##### .#... ..#.. .....")),
    ('↑', g("..#.. .###. #.#.# ..#.. ..#.. ..#.. .....")),
    ('↓', g("..... ..#.. ..#.. ..#.. #.#.# .###. ..#..")),
];

const QUESTION: [u8; 7] = g(".###. #...# ....# ...#. ..#.. ..... ..#..");

/// ASCII-Direktindex in [`GLYPHS`] (+1; 0 = keine Glyphe), zur Compile-Zeit
/// gebaut. Bei Doppelungen gewinnt wie bei der linearen Suche der erste Eintrag.
const fn build_ascii_index() -> [u8; 128] {
    let mut t = [0u8; 128];
    let mut i = 0;
    while i < GLYPHS.len() {
        let c = GLYPHS[i].0 as u32;
        if c < 128 && t[c as usize] == 0 {
            t[c as usize] = (i + 1) as u8;
        }
        i += 1;
    }
    t
}

static ASCII_INDEX: [u8; 128] = build_ascii_index();

/// Bitmaske eines Zeichens (unbekannte Zeichen → `?`).
pub fn glyph(c: char) -> [u8; 7] {
    let c = match c {
        'ä' => 'Ä',
        'ö' => 'Ö',
        'ü' => 'Ü',
        c if c.is_ascii_lowercase() => c.to_ascii_uppercase(),
        c => c,
    };
    if c == ' ' {
        return BLANK;
    }
    if (c as u32) < 128 {
        // O(1) für ASCII.
        let idx = ASCII_INDEX[c as usize];
        return if idx != 0 { GLYPHS[idx as usize - 1].1 } else { QUESTION };
    }
    for &(ch, rows) in GLYPHS {
        if ch == c {
            return rows;
        }
    }
    QUESTION
}

/// Breite eines Textes in Pixeln.
pub fn text_width(text: &str, scale: i32) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 { 0 } else { (n * ADVANCE - 1) * scale }
}

/// Zeichnet eine Glyphe mit linker oberer Ecke `(gx, gy)`.
#[inline(always)]
fn draw_glyph(fb: &mut Framebuffer, rows: &[u8; 7], gx: i32, gy: i32, color: u32, s: i32) {
    let (fw, fh) = (fb.width() as i32, fb.height() as i32);
    let inside = gx >= 0 && gy >= 0 && gx + GLYPH_W * s <= fw && gy + GLYPH_H * s <= fh;
    for (ry, &bits) in rows.iter().enumerate() {
        // Nur gesetzte Bits besuchen (höchstes Bit = linke Spalte).
        let mut b = bits & 0x1F;
        while b != 0 {
            let lz = b.leading_zeros();
            b &= !(0x80u8 >> lz);
            let rx = lz as i32 - 3;
            let (px, py) = (gx + rx * s, gy + ry as i32 * s);
            if inside {
                // Schnellpfad: Glyphe liegt komplett im Bild → direkt schreiben.
                if s == 1 {
                    fb.color[(py * fw + px) as usize] = color;
                } else {
                    for yy in py..py + s {
                        let o = (yy * fw + px) as usize;
                        fb.color[o..o + s as usize].fill(color);
                    }
                }
            } else {
                fb.fill_rect(px, py, s, s, color);
            }
        }
    }
}

/// Zeichnet Text (ohne Tiefentest). `scale` vergrößert ganzzahlig.
pub fn draw_text(fb: &mut Framebuffer, x: i32, y: i32, text: &str, color: u32, scale: i32) {
    let s = scale.max(1);
    let mut cx = x;
    for ch in text.chars() {
        if ch != ' ' {
            draw_glyph(fb, &glyph(ch), cx, y, color, s);
        }
        cx += ADVANCE * s;
    }
}

/// Text mit 1-Pixel-Schatten (gut lesbar auf jedem Hintergrund).
/// Ein Durchlauf: pro Zeichen erst Schatten, dann Glyphe. Pixelgleich zu
/// „alle Schatten, dann aller Text“, weil der Schatten (Versatz +s, Breite
/// 5s) nie den Text der nächsten Glyphe (Abstand 6s) berührt.
pub fn draw_text_shadow(fb: &mut Framebuffer, x: i32, y: i32, text: &str, color: u32, shadow: u32, scale: i32) {
    let s = scale.max(1);
    let mut cx = x;
    for ch in text.chars() {
        if ch != ' ' {
            let rows = glyph(ch);
            draw_glyph(fb, &rows, cx + s, y + s, shadow, s);
            draw_glyph(fb, &rows, cx, y, color, s);
        }
        cx += ADVANCE * s;
    }
}

/// Zentriert Text horizontal um `cx`.
pub fn draw_text_centered(fb: &mut Framebuffer, cx: i32, y: i32, text: &str, color: u32, shadow: u32, scale: i32) {
    let w = text_width(text, scale);
    draw_text_shadow(fb, cx - w / 2, y, text, color, shadow, scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_parse_and_render() {
        assert_eq!(glyph('A')[3], 0b11111);
        assert_eq!(glyph('a'), glyph('A'));
        assert_eq!(glyph('ü'), glyph('Ü'));
        let mut fb = Framebuffer::new(40, 10);
        draw_text(&mut fb, 0, 0, "Hi!", 0xFFFFFF, 1);
        assert_eq!(fb.pixel(0, 0), 0xFFFFFF); // H, linke Spalte
        assert_eq!(text_width("Hi!", 1), 17);
    }
}
