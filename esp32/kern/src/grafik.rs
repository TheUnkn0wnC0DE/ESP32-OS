//! Zeichenprimitive der KolibriOS-Systemfunktionen auf RGB565-Puffern:
//! Pixel (1), Rechteck (13), Linie (38), Bild (7), Palettenbild (65),
//! 32-Bit-Blit (73). Farben im KolibriOS-Format 0x00RRGGBB.

/// 0x00RRGGBB → RGB565.
#[inline]
pub fn zu565(c: u32) -> u16 {
    (((c >> 8) & 0xF800) | ((c >> 5) & 0x07E0) | ((c >> 3) & 0x001F)) as u16
}

/// RGB565 → 0x00RRGGBB (Bits aufgefüllt).
#[inline]
pub fn von565(p: u16) -> u32 {
    let r = u32::from(p >> 11) & 31;
    let g = u32::from(p >> 5) & 63;
    let b = u32::from(p) & 31;
    ((r << 3 | r >> 2) << 16) | ((g << 2 | g >> 4) << 8) | (b << 3 | b >> 2)
}

/// Bildpuffer RGB565, zeilenweise.
#[derive(Clone)]
pub struct Puffer {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u16>,
}

impl Puffer {
    pub fn neu(w: usize, h: usize, farbe: u32) -> Puffer {
        Puffer { w, h, px: vec![zu565(farbe); w * h] }
    }

    /// Wie [`Puffer::neu`] für Puffer, die ein Programm anfordert
    /// (Fenster): None, wenn der System-Heap dafür nicht reicht – mit
    /// [`RESERVE`] Luft für WLAN und Kern. Fehlerbericht 24.11.: Mit offenem
    /// Browser war der PSRAM knapp; das Startmenü forderte seinen
    /// Fensterpuffer an, die Belegung schlug fehl und brach die ganze
    /// Firmware ab (Neustart, „@menu“). Jetzt endet nur das Programm.
    pub fn versuche(w: usize, h: usize, farbe: u32, system_frei: usize) -> Option<Puffer> {
        let n = w.checked_mul(h)?;
        if n.checked_mul(2)?.checked_add(RESERVE)? > system_frei {
            return None;
        }
        let mut px = Vec::new();
        px.try_reserve_exact(n).ok()?;
        px.resize(n, zu565(farbe));
        Some(Puffer { w, h, px })
    }
}

/// Mindestens so viel System-Heap bleibt frei, wenn ein Programm einen
/// Puffer anfordert.
pub const RESERVE: usize = 256 * 1024;

/// PC-Tests: grösster freier Block des System-Heaps vortäuschen
/// (Plattform `PC`, `system_frei`); Standard unbegrenzt.
pub static TEST_SYSTEM_FREI: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(usize::MAX);

/// Zeichenbereich in Pufferkoordinaten (Ende exklusiv).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Clip {
    pub fn schnitt(self, o: Clip) -> Clip {
        Clip { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }
    pub fn leer(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// Zeichnet in einen Puffer: Koordinaten relativ zum Ursprung (`ox`, `oy`),
/// beschnitten auf `clip`.
pub struct Zeichner<'a> {
    pub p: &'a mut Puffer,
    pub ox: i32,
    pub oy: i32,
    pub clip: Clip,
    /// Umrandung aller geschriebenen Pixel (Pufferkoordinaten, Dirty-Rect).
    pub beruehrt: Clip,
}

/// Leere Umrandung (für `beruehrt`).
pub const LEER: Clip = Clip { x0: i32::MAX, y0: i32::MAX, x1: i32::MIN, y1: i32::MIN };

impl Clip {
    /// Kleinstes Rechteck, das beide enthält (leere zählen nicht).
    pub fn vereinigung(self, o: Clip) -> Clip {
        if o.leer() {
            return self;
        }
        if self.leer() {
            return o;
        }
        Clip { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
    pub fn flaeche(self) -> i64 {
        if self.leer() { 0 } else { i64::from(self.x1 - self.x0) * i64::from(self.y1 - self.y0) }
    }
    pub fn verschoben(self, dx: i32, dy: i32) -> Clip {
        if self.leer() {
            return self;
        }
        Clip { x0: self.x0 + dx, y0: self.y0 + dy, x1: self.x1 + dx, y1: self.y1 + dy }
    }
}

/// Invertier-Bit für Pixel/Linien (0x01xxxxxx).
pub const INVERT: u32 = 0x0100_0000;
/// Verlaufs-Bit für Rechtecke (0x80RRGGBB).
pub const VERLAUF: u32 = 0x8000_0000;

impl Zeichner<'_> {
    pub fn neu(p: &mut Puffer) -> Zeichner<'_> {
        let clip = Clip { x0: 0, y0: 0, x1: p.w as i32, y1: p.h as i32 };
        Zeichner { p, ox: 0, oy: 0, clip, beruehrt: LEER }
    }

    /// Rechteck (Koordinaten wie Zeichenaufrufe) als berührt merken.
    fn merke(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let r = Clip { x0: x + self.ox, y0: y + self.oy, x1: x + self.ox + w, y1: y + self.oy + h }.schnitt(self.clip);
        self.beruehrt = self.beruehrt.vereinigung(r);
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let (x, y) = (x + self.ox, y + self.oy);
        (x >= self.clip.x0 && x < self.clip.x1 && y >= self.clip.y0 && y < self.clip.y1)
            .then(|| y as usize * self.p.w + x as usize)
    }

    /// Funktion 1: Pixel setzen (oder invertieren).
    pub fn pixel(&mut self, x: i32, y: i32, farbe: u32) {
        if let Some(i) = self.index(x, y) {
            self.p.px[i] = if farbe & INVERT != 0 { !self.p.px[i] } else { zu565(farbe) };
            self.merke(x, y, 1, 1);
        }
    }

    /// Pixel lesen (0x00RRGGBB), ausserhalb 0.
    pub fn lesen(&self, x: i32, y: i32) -> u32 {
        self.index(x, y).map_or(0, |i| von565(self.p.px[i]))
    }

    /// Gefülltes Rechteck, Farbe direkt in RGB565.
    pub fn fuellen565(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        let r = Clip { x0: x + self.ox, y0: y + self.oy, x1: x + self.ox + w, y1: y + self.oy + h }.schnitt(self.clip);
        if r.leer() {
            return;
        }
        self.beruehrt = self.beruehrt.vereinigung(r);
        for yy in r.y0..r.y1 {
            let a = yy as usize * self.p.w;
            self.p.px[a + r.x0 as usize..a + r.x1 as usize].fill(c);
        }
    }

    /// Funktion 13: Rechteck `w`×`h`; mit Verlaufs-Bit wird die Farbe je
    /// Zeile um 1 je Kanal dunkler (wie der KolibriOS-Kern).
    pub fn rechteck(&mut self, x: i32, y: i32, w: i32, h: i32, farbe: u32) {
        if farbe & VERLAUF == 0 {
            self.fuellen565(x, y, w, h, zu565(farbe));
            return;
        }
        let mut c = farbe & 0xFF_FFFF;
        for k in 0..h {
            self.fuellen565(x, y + k, w, 1, zu565(c));
            let ab = |v: u32| v.saturating_sub(1);
            c = (ab(c >> 16 & 0xFF) << 16) | (ab(c >> 8 & 0xFF) << 8) | ab(c & 0xFF);
        }
    }

    /// Funktion 38: Linie inklusive Endpunkt (Bresenham), Invertieren möglich.
    pub fn linie(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, farbe: u32) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.pixel(x, y, farbe);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Funktion 65 (und 7 = 24 bpp, Zeilenabstand 0): Bild mit `bpp` Bit je
    /// Pixel, Palette für bpp ≤ 8, `zeilen_extra` Bytes nach jeder Zeile.
    #[allow(clippy::too_many_arguments)]
    pub fn bild(&mut self, data: &[u8], w: i32, h: i32, x: i32, y: i32, bpp: u32, palette: &[u32], zeilen_extra: i32) {
        let zeile_bits = |w: i32| (w as usize * bpp as usize).div_ceil(8);
        let schritt = match bpp {
            9 => w as usize,
            15 | 16 => 2 * w as usize,
            24 => 3 * w as usize,
            32 => 4 * w as usize,
            1 | 2 | 4 | 8 => zeile_bits(w),
            _ => return,
        };
        let pal = |i: usize| palette.get(i).copied().unwrap_or(0);
        self.merke(x, y, w, h);
        // 16 bpp ist schon das Format des Puffers (RGB565): einmal je Bild
        // beschneiden, dann ganze Zeilen übernehmen (B5: Browser scrollt
        // flüssig – vorher Umweg über 24 Bit und Prüfung je Bildpunkt)
        if bpp == 16 && zeilen_extra >= 0 {
            self.bild565(data, w, h, x, y, schritt + zeilen_extra as usize);
            return;
        }
        let mut off = 0usize;
        for yy in 0..h {
            let z = match data.get(off..off + schritt) {
                Some(z) => z,
                None => return,
            };
            for xx in 0..w {
                let xu = xx as usize;
                let c = match bpp {
                    1 => pal(usize::from(z[xu / 8] >> (7 - xu % 8) & 1)),
                    2 => pal(usize::from(z[xu / 4] >> (6 - 2 * (xu % 4)) & 3)),
                    4 => pal(usize::from(z[xu / 2] >> (4 - 4 * (xu % 2)) & 15)),
                    8 => pal(usize::from(z[xu])),
                    9 => {
                        let g = u32::from(z[xu]);
                        g << 16 | g << 8 | g
                    }
                    15 => {
                        let v = u32::from(u16::from_le_bytes([z[2 * xu], z[2 * xu + 1]]));
                        let (r, g, b) = (v >> 10 & 31, v >> 5 & 31, v & 31);
                        (r << 3 | r >> 2) << 16 | (g << 3 | g >> 2) << 8 | (b << 3 | b >> 2)
                    }
                    16 => von565(u16::from_le_bytes([z[2 * xu], z[2 * xu + 1]])),
                    24 => u32::from(z[3 * xu + 2]) << 16 | u32::from(z[3 * xu + 1]) << 8 | u32::from(z[3 * xu]),
                    _ => u32::from(z[4 * xu + 2]) << 16 | u32::from(z[4 * xu + 1]) << 8 | u32::from(z[4 * xu]),
                };
                if let Some(i) = self.index(x + xx, y + yy) {
                    self.p.px[i] = zu565(c);
                }
            }
            off = (off as isize + schritt as isize + zeilen_extra as isize).max(0) as usize;
        }
    }

    /// Funktion 65 mit 16 bpp: RGB565 (Little Endian) zeilenweise in den
    /// Puffer, `abstand` Bytes von Zeile zu Zeile. Wie der allgemeine Weg:
    /// fehlt einer Zeile ein Teil der Daten, endet das Bild dort.
    fn bild565(&mut self, data: &[u8], w: i32, h: i32, x: i32, y: i32, abstand: usize) {
        let r = Clip { x0: x + self.ox, y0: y + self.oy, x1: x + self.ox + w, y1: y + self.oy + h }.schnitt(self.clip);
        if r.leer() {
            return;
        }
        let zeile = 2 * w as usize;
        let links = 2 * (r.x0 - x - self.ox) as usize;
        let n = (r.x1 - r.x0) as usize;
        for zy in r.y0..r.y1 {
            let off = (zy - y - self.oy) as usize * abstand;
            let Some(z) = data.get(off..off + zeile) else { return };
            let a = zy as usize * self.p.w + r.x0 as usize;
            for (d, s) in self.p.px[a..a + n].iter_mut().zip(z[links..links + 2 * n].chunks_exact(2)) {
                *d = u16::from_le_bytes([s[0], s[1]]);
            }
        }
    }

    /// Funktion 73 (ROP 0 = Kopie): 32-bpp-Quelle (BGRX), Ziel- und
    /// Quellrechteck mit Beschnitt, `transparent`: Pixel mit Alpha 0 auslassen.
    #[allow(clippy::too_many_arguments)]
    pub fn blit32(
        &mut self,
        ziel: (i32, i32, i32, i32),
        quelle: (i32, i32, i32, i32),
        data: &[u8],
        zeile_bytes: usize,
        transparent: bool,
    ) {
        let (zx, zy, zw, zh) = ziel;
        let (qx, qy, qw, qh) = quelle;
        let w = zw.min(qw);
        let h = zh.min(qh);
        self.merke(zx, zy, w, h);
        for yy in 0..h {
            let sy = qy + yy;
            if sy < 0 {
                continue;
            }
            for xx in 0..w {
                let sx = qx + xx;
                if sx < 0 {
                    continue;
                }
                let o = sy as usize * zeile_bytes + sx as usize * 4;
                let Some(p) = data.get(o..o + 4) else { continue };
                if transparent && p[3] == 0 {
                    continue;
                }
                let c = u32::from(p[2]) << 16 | u32::from(p[1]) << 8 | u32::from(p[0]);
                if let Some(i) = self.index(zx + xx, zy + yy) {
                    self.p.px[i] = zu565(c);
                }
            }
        }
    }

    /// Puffer `q` an (x, y) kopieren (Compositor).
    pub fn puffer(&mut self, q: &Puffer, x: i32, y: i32) {
        let r = Clip { x0: x + self.ox, y0: y + self.oy, x1: x + self.ox + q.w as i32, y1: y + self.oy + q.h as i32 }
            .schnitt(self.clip);
        if r.leer() {
            return;
        }
        self.beruehrt = self.beruehrt.vereinigung(r);
        let qx0 = (r.x0 - x - self.ox) as usize;
        let n = (r.x1 - r.x0) as usize;
        for yy in r.y0..r.y1 {
            let qy = (yy - y - self.oy) as usize;
            let a = yy as usize * self.p.w + r.x0 as usize;
            let b = qy * q.w + qx0;
            self.p.px[a..a + n].copy_from_slice(&q.px[b..b + n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bild_16bpp_zeilen_beschnitten() {
        // 5×4-Bild mit 2 Byte Zeilen-Zusatz, teils ausserhalb (links, unten)
        // und mit Versatz des Arbeitsbereichs – Ergebnis wie je Bildpunkt
        let (w, h, extra) = (5i32, 4i32, 2usize);
        let mut data = Vec::new();
        for yy in 0..h {
            for xx in 0..w {
                data.extend_from_slice(&(0x1234u16.wrapping_mul((yy * 7 + xx + 1) as u16)).to_le_bytes());
            }
            data.extend_from_slice(&[0xEE; 2]);
        }
        let mut p = Puffer::neu(8, 6, 0);
        let mut z = Zeichner::neu(&mut p);
        z.ox = 1;
        z.oy = 1;
        z.clip = Clip { x0: 1, y0: 1, x1: 8, y1: 4 };
        z.bild(&data, w, h, -2, 1, 16, &[], extra as i32);
        let beruehrt = z.beruehrt;
        for py in 0..6i32 {
            for px in 0..8i32 {
                let (bx, by) = (px - 1 + 2, py - 1 - 1);
                let innen = (1..8).contains(&px) && (1..4).contains(&py) && (0..w).contains(&bx) && (0..h).contains(&by);
                let soll = if innen {
                    let o = by as usize * (2 * w as usize + extra) + 2 * bx as usize;
                    u16::from_le_bytes([data[o], data[o + 1]])
                } else {
                    zu565(0)
                };
                assert_eq!(p.px[py as usize * 8 + px as usize], soll, "Bildpunkt {px},{py}");
            }
        }
        assert_eq!(beruehrt, Clip { x0: 1, y0: 2, x1: 4, y1: 4 });
        // zu wenig Daten: endet an der ersten unvollständigen Zeile
        let mut p = Puffer::neu(4, 4, 0);
        let mut z = Zeichner::neu(&mut p);
        z.bild(&[0xFF; 2 * 4 + 3], 4, 2, 0, 0, 16, &[], 0);
        assert_eq!(&p.px[..4], &[0xFFFF; 4]);
        assert_eq!(&p.px[4..8], &[0; 4]);
    }

    #[test]
    fn farben_hin_und_zurueck() {
        for c in [0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF] {
            assert_eq!(von565(zu565(c)), c);
        }
    }

    #[test]
    fn linie_mit_endpunkt_und_clip() {
        let mut p = Puffer::neu(10, 10, 0);
        let mut z = Zeichner::neu(&mut p);
        z.linie(0, 0, 9, 9, 0xFFFFFF);
        z.linie(-5, 2, 20, 2, 0xFF0000);
        assert_eq!(z.lesen(9, 9), 0xFFFFFF);
        assert_eq!(z.lesen(0, 2), 0xFF0000);
        assert_eq!(z.lesen(9, 2), 0xFF0000);
        assert_eq!(z.lesen(3, 4), 0);
    }

    #[test]
    fn palettenbild_1bit_msb_zuerst() {
        let mut p = Puffer::neu(8, 1, 0);
        let mut z = Zeichner::neu(&mut p);
        z.bild(&[0b1000_0001], 8, 1, 0, 0, 1, &[0x000000, 0xFFFFFF], 0);
        assert_eq!(z.lesen(0, 0), 0xFFFFFF);
        assert_eq!(z.lesen(1, 0), 0);
        assert_eq!(z.lesen(7, 0), 0xFFFFFF);
    }
}
