//! Testbild für die Hardware-Prüfung (Programm `hwtest` der Firmware).
//!
//! Zeigt, ob Display, Vergrößerung, Drehung, Bildwechsel und Touch stimmen:
//!
//! * weißer 1-Pixel-Rand und rote Ecken mit Beschriftung (OL/OR/UL/UR) –
//!   nichts abgeschnitten, Bild richtig herum?
//! * Feld „1 PX“: Schachbrett aus einzelnen Renderpixeln. Bei scharfer
//!   Vergrößerung klare Karos, bei bilinearer graue Fläche.
//! * Feld „GITTER“: 1-Pixel-Linien, Farbverläufe R/G/B, Farbbalken.
//! * Kreis: muss rund sein (Seitenverhältnis).
//! * Laufender Balken: ruckelt oder zerreißt er, stimmt der Bildwechsel nicht.
//! * Touch: Ring mit Koordinaten unter jedem Finger.
//! * Unten drei Tippfelder: Auflösung, Vergrößerung, Drehung umschalten.
//!
//! Gezeichnet wird streifenweise (wie das Spiel), damit auch der
//! Streifen-Weg der Firmware geprüft wird.

use crate::gfx::{Canvas, H, W, hex, rgb};
use crate::input::{MAX_TOUCH, btn};
use crate::render::Target;
use core::fmt::Write as _;
use spatz::render::overlay::TextBuf;

/// Eine Infozeile.
pub type Line = TextBuf<64>;
/// Berührung: (logisch 320×180, roh im Panel).
pub type TouchPoint = ((u16, u16), (u16, u16));

/// Was das Testbild anzeigt.
pub struct TestInfo {
    /// Bildzähler (bewegt den Balken).
    pub frame: u32,
    /// Freie Infozeilen (FPS, Bildwiederholrate, Chip, Speicher …).
    pub lines: [Line; 6],
    /// Berührungen in logischen Koordinaten (320×180) und roh (Panel).
    pub touches: [Option<TouchPoint>; MAX_TOUCH],
    /// Gedrückte Knöpfe (Bitmaske aus [`btn`]).
    pub buttons: u8,
    pub hires: bool,
    pub smooth: bool,
    pub flip: bool,
}

impl Default for TestInfo {
    fn default() -> Self {
        Self {
            frame: 0,
            lines: core::array::from_fn(|_| Line::new()),
            touches: [None; MAX_TOUCH],
            buttons: 0,
            hires: true,
            smooth: false,
            flip: false,
        }
    }
}

/// Tippfelder unten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Resolution,
    Scaling,
    Rotation,
}

const ZONE_Y: i32 = 156;

/// Welches Tippfeld liegt unter (`x`, `y`) (logisch)?
pub fn zone_at(x: u16, y: u16) -> Option<Zone> {
    if (y as i32) < ZONE_Y {
        return None;
    }
    Some(match x as usize * 3 / W {
        0 => Zone::Resolution,
        1 => Zone::Scaling,
        _ => Zone::Rotation,
    })
}

/// Zeichnet die Zeilen des Streifens `t`.
pub fn draw(info: &TestInfo, t: &mut Target) {
    let (w, h) = (t.w, t.h);
    let s = (w / W).max(1);
    let (y0, y1) = t.rows();
    let px = |t: &mut Target, x: usize, y: usize, c: u16| t.color[(y - y0) * w + x] = c;

    // --- Pixelgenaue Teile (physische Renderpixel) --------------------------
    for y in y0..y1 {
        // Hintergrund: senkrechter Verlauf.
        let k = (y * 255 / h) as u8;
        let o = (y - y0) * w;
        t.color[o..o + w].fill(rgb(24, 24 + k / 5, 48 + k / 4));
        // Feld „1 PX“: Schachbrett.
        if (40 * s..76 * s).contains(&y) {
            for x in 16 * s..96 * s {
                px(t, x, y, if (x + y) % 2 == 0 { hex(0xFFFFFF) } else { hex(0x000000) });
            }
            // Feld „GITTER“: jede 4. Zeile/Spalte eine 1-Pixel-Linie.
            for x in 104 * s..184 * s {
                if x % 4 == 0 || y % 4 == 0 {
                    px(t, x, y, hex(0xFFFFFF));
                }
            }
            // Verläufe R, G, B (je 12 logische Zeilen).
            let band = (y / s - 40) / 12;
            for x in 192 * s..304 * s {
                let v = ((x - 192 * s) * 255 / (112 * s)) as u8;
                let c = match band {
                    0 => rgb(v, 0, 0),
                    1 => rgb(0, v, 0),
                    _ => rgb(0, 0, v),
                };
                px(t, x, y, c);
            }
        }
        // Weißer Rand (1 Pixel).
        if y == 0 || y == h - 1 {
            t.color[o..o + w].fill(hex(0xFFFFFF));
        } else {
            px(t, 0, y, hex(0xFFFFFF));
            px(t, w - 1, y, hex(0xFFFFFF));
        }
    }

    // --- Logisches Layout (320×180, bei 640×360 doppelt gezeichnet) ---------
    let (lw, lh) = (W as i32, H as i32);
    let mut c = Canvas::scaled(t.color, w, h, s, t.row0);
    let white = hex(0xFFFFFF);
    let shadow = hex(0x000000);
    c.text_centered(lw / 2, 3, "PUDDING-PANIK HARDWARETEST", hex(0xFFE14D), shadow, 1);
    // Ecken.
    for (x, y, label, lx) in
        [(1, 1, "OL", 12), (lw - 9, 1, "OR", lw - 22), (1, lh - 9, "UL", 12), (lw - 9, lh - 9, "UR", lw - 22)]
    {
        c.fill_rect(x, y, 8, 8, hex(0xFF2020));
        c.text_shadow(lx, y, label, white, shadow, 1);
    }
    // Farbbalken.
    let bars = [0xFFFFFF, 0xFFFF00, 0x00FFFF, 0x00FF00, 0xFF00FF, 0xFF0000, 0x0000FF, 0x000000];
    for (i, &col) in bars.iter().enumerate() {
        c.fill_rect(16 + i as i32 * 36, 14, 36, 20, hex(col));
    }
    c.text_shadow(16, 78, "1 PX", white, shadow, 1);
    c.text_shadow(104, 78, "GITTER", white, shadow, 1);
    c.text_shadow(192, 78, "R G B", white, shadow, 1);
    // Laufender Balken (Bildwechsel / Ruckeln).
    c.fill_rect(0, 88, lw, 8, hex(0x202030));
    let bx = (info.frame as i32 * 3) % (lw + 12) - 12;
    c.fill_rect(bx, 88, 12, 8, hex(0xFFE14D));
    // Kreis: muss rund sein.
    c.ring(268.0, 126.0, 22.0, 2.0, white);
    c.ring(268.0, 126.0, 2.0, 2.0, white);
    // Infozeilen.
    for (i, l) in info.lines.iter().enumerate() {
        c.text_shadow(8, 100 + i as i32 * 9, l.as_str(), white, shadow, 1);
    }
    // Knöpfe.
    for (i, (bit, label)) in
        [(btn::UP, "V"), (btn::DOWN, "H"), (btn::LEFT, "L"), (btn::RIGHT, "R"), (btn::JUMP, "S"), (btn::MENU, "B")]
            .into_iter()
            .enumerate()
    {
        let x = 200 + i as i32 * 10;
        let on = info.buttons & bit != 0;
        c.fill_rect(x, 100, 9, 11, if on { hex(0x40FF60) } else { hex(0x303040) });
        c.text(x + 2, 102, label, if on { hex(0x000000) } else { white }, 1);
    }
    // Tippfelder.
    let labels = [
        if info.hires { "640X360" } else { "320X180" },
        if info.smooth { "WEICH" } else { "SCHARF" },
        if info.flip { "DREHEN 270" } else { "DREHEN 90" },
    ];
    for (i, label) in labels.into_iter().enumerate() {
        let x = 4 + i as i32 * (lw - 8) / 3;
        c.blend_rect(x, ZONE_Y, (lw - 8) / 3 - 4, lh - ZONE_Y - 12, hex(0x5050A0));
        c.frame(x, ZONE_Y, (lw - 8) / 3 - 4, lh - ZONE_Y - 12, white);
        c.text_centered(x + ((lw - 8) / 3 - 4) / 2, ZONE_Y + 4, label, white, shadow, 1);
    }
    // Touch.
    for (logical, raw) in info.touches.iter().flatten() {
        let (x, y) = (logical.0 as f32 + 0.5, logical.1 as f32 + 0.5);
        c.ring(x, y, 10.0, 1.5, hex(0x40FF60));
        c.fill_rect(logical.0 as i32 - 14, logical.1 as i32, 29, 1, hex(0x40FF60));
        c.fill_rect(logical.0 as i32, logical.1 as i32 - 14, 1, 29, hex(0x40FF60));
        let mut l = TextBuf::<24>::new();
        let _ = write!(l, "{},{} ({},{})", logical.0, logical.1, raw.0, raw.1);
        let tx = (logical.0 as i32 + 14).min(lw - 4 - crate::gfx::text_width(l.as_str(), 1));
        c.text_shadow(tx, (logical.1 as i32 - 20).max(12), l.as_str(), white, shadow, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn strips_equal_full_picture_and_zones() {
        let mut info = TestInfo { frame: 17, ..TestInfo::default() };
        let _ = info.lines[0].write_str("60 FPS");
        info.touches[0] = Some(((100, 60), (360, 640)));
        for (w, h) in [(640, 360), (320, 180)] {
            let n = w * h;
            let (mut c1, mut d1) = (vec![0u16; n], vec![0u16; n]);
            draw(&info, &mut Target::new(&mut c1, &mut d1, w, h));
            let (mut c2, mut d2) = (vec![0u16; n], vec![0u16; n]);
            let (mut a, mut b) = Target::new(&mut c2, &mut d2, w, h).split(h / 3);
            draw(&info, &mut a);
            draw(&info, &mut b);
            assert_eq!(c1, c2);
            // Schachbrett pixelgenau.
            let s = w / W;
            let y = 50 * s;
            assert_ne!(c1[y * w + 20 * s], c1[y * w + 20 * s + 1]);
        }
        assert_eq!(zone_at(10, 170), Some(Zone::Resolution));
        assert_eq!(zone_at(160, 170), Some(Zone::Scaling));
        assert_eq!(zone_at(300, 170), Some(Zone::Rotation));
        assert_eq!(zone_at(160, 100), None);
    }
}
