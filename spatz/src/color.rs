//! Farben im Format `0x00RRGGBB` (entspricht direkt BGRA im Speicher, also
//! dem Format, das Windows-DIBs und die meisten Displays erwarten).

/// 24-Bit-RGB-Farbe, gepackt in `u32` (`0x00RRGGBB`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Color = Color::hex(0x000000);
    pub const WHITE: Color = Color::hex(0xFFFFFF);
    pub const RED: Color = Color::hex(0xFF3030);
    pub const GREEN: Color = Color::hex(0x30E040);
    pub const YELLOW: Color = Color::hex(0xFFE040);

    #[inline(always)]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }
    #[inline(always)]
    pub const fn hex(v: u32) -> Self {
        Color(v & 0x00FF_FFFF)
    }
    #[inline(always)]
    pub const fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }
    #[inline(always)]
    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    #[inline(always)]
    pub const fn b(self) -> u8 {
        self.0 as u8
    }

    /// Lineare Mischung, `t` in `[0, 1]`.
    #[inline]
    pub fn lerp(self, o: Color, t: f32) -> Color {
        let t = (crate::math::clamp(t, 0.0, 1.0) * 256.0) as u32;
        Color(blend256(self.0, o.0, t))
    }

    /// Helligkeit skalieren (gesättigt).
    #[inline]
    pub fn scale(self, f: f32) -> Color {
        self.mul_rgb([f, f, f])
    }

    /// Kanalweise Multiplikation mit Lichtfaktoren (gesättigt bei 255).
    #[inline]
    pub fn mul_rgb(self, l: [f32; 3]) -> Color {
        let c = |v: u8, f: f32| {
            let x = v as f32 * f;
            if x >= 255.0 {
                255u8
            } else if x <= 0.0 {
                0
            } else {
                x as u8
            }
        };
        Color::rgb(c(self.r(), l[0]), c(self.g(), l[1]), c(self.b(), l[2]))
    }

    /// Modulation zweier Farben (wie Textur × Licht).
    #[inline]
    pub fn modulate(self, o: Color) -> Color {
        let m = |a: u8, b: u8| ((a as u32 * (b as u32 + 1)) >> 8) as u8;
        Color::rgb(m(self.r(), o.r()), m(self.g(), o.g()), m(self.b(), o.b()))
    }

    /// 50/50-Mischung (sehr billig).
    #[inline(always)]
    pub fn avg(self, o: Color) -> Color {
        Color(avg_packed(self.0, o.0))
    }
}

/// Gepackte Mischung `a·(256-t) + b·t` mit `t` in `0..=256`.
#[inline(always)]
pub fn blend256(a: u32, b: u32, t: u32) -> u32 {
    let it = 256 - t;
    let rb = ((a & 0x00FF_00FF) * it + (b & 0x00FF_00FF) * t) >> 8;
    let g = ((a & 0x0000_FF00) * it + (b & 0x0000_FF00) * t) >> 8;
    (rb & 0x00FF_00FF) | (g & 0x0000_FF00)
}

/// Gepackter Mittelwert zweier Farben.
#[inline(always)]
pub fn avg_packed(a: u32, b: u32) -> u32 {
    ((a & 0x00FE_FEFE) >> 1) + ((b & 0x00FE_FEFE) >> 1)
}

/// Halbiert die Helligkeit (für Schatten).
#[inline(always)]
pub fn darken_half(c: u32) -> u32 {
    (c >> 1) & 0x007F_7F7F
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_and_blending() {
        let c = Color::rgb(10, 200, 30);
        assert_eq!((c.r(), c.g(), c.b()), (10, 200, 30));
        assert_eq!(blend256(0x000000, 0xFFFFFF, 256), 0xFFFFFF);
        assert_eq!(blend256(0x123456, 0xFFFFFF, 0), 0x123456);
        assert_eq!(Color::WHITE.scale(0.5), Color::rgb(127, 127, 127));
        assert_eq!(darken_half(0xFFFFFF), 0x7F7F7F);
        assert_eq!(avg_packed(0xFF0000, 0x0000FF), 0x7F007F);
    }
}
