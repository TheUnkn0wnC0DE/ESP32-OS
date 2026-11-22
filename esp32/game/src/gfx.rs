//! Bildpuffer und 2D-Zeichnen.
//!
//! * Farben sind **RGB565 in nativer Byte-Reihenfolge** – genau so liest sie
//!   der Bildbeschleuniger (PPA) des ESP32-P4, bevor er das Bild skaliert und
//!   gedreht in den Bildspeicher des MIPI-Displays schreibt.
//! * **Logische Koordinaten:** Menüs und HUD werden immer in 320×180 angelegt
//!   ([`W`]×[`H`]). Rendert das Spiel in 640×360, zeichnet der [`Canvas`] alles
//!   mit Faktor 2 – das Layout bleibt gleich, nur die 3D-Szene wird schärfer.
//! * Alle Zeichenfunktionen schneiden auf ein Zeilenband zu: Beide Kerne
//!   zeichnen gleichzeitig je eine Bildhälfte.

use spatz::color::Color;
use spatz::math::sqrt;
use spatz::render::font::{ADVANCE, GLYPH_H, glyph};

/// Logische Auflösung (Layout von Menüs und HUD), 16:9.
pub const W: usize = 320;
pub const H: usize = 180;
/// Größte logische Breite für breite Displays (448×180 ≈ 22,4:9).
pub const MAX_LOGICAL_W: usize = 448;

static LOGICAL_W: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(W);

/// Aktuelle logische Breite: [`W`] (16:9, ESP32) oder breiter, damit Menüs und
/// Spiel auf Handys mit 18:9 bis 22:9 den ganzen Bildschirm füllen.
#[inline]
pub fn logical_w() -> usize {
    LOGICAL_W.load(core::sync::atomic::Ordering::Relaxed)
}

/// Setzt die logische Breite (gerade, [`W`]..=[`MAX_LOGICAL_W`]).
pub fn set_logical_w(w: usize) {
    LOGICAL_W.store(w.clamp(W, MAX_LOGICAL_W) & !1, core::sync::atomic::Ordering::Relaxed);
}

/// Größte logische Höhe für Displays schmaler als 16:9 (320×256 = 5:4).
pub const MAX_LOGICAL_H: usize = 256;

static LOGICAL_H: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(H);

/// Aktuelle logische Höhe: [`H`] (16:9 und breiter) oder höher für Tablets
/// und aufgeklappte Foldables (16:10, 4:3), damit auch dort nichts schwarz bleibt.
#[inline]
pub fn logical_h() -> usize {
    LOGICAL_H.load(core::sync::atomic::Ordering::Relaxed)
}

/// Setzt die logische Höhe (gerade, [`H`]..=[`MAX_LOGICAL_H`]).
pub fn set_logical_h(h: usize) {
    LOGICAL_H.store(h.clamp(H, MAX_LOGICAL_H) & !1, core::sync::atomic::Ordering::Relaxed);
}
/// Größter Faktor zwischen Render- und logischer Auflösung.
pub const MAX_SCALE: usize = 2;
/// Größte Renderauflösung (640×360).
pub const MAX_W: usize = W * MAX_SCALE;
pub const MAX_H: usize = H * MAX_SCALE;

/// RGB565.
#[inline(always)]
pub const fn rgb(r: u8, g: u8, b: u8) -> u16 {
    (((r as u16) & 0xF8) << 8) | (((g as u16) & 0xFC) << 3) | ((b as u16) >> 3)
}

/// Aus `0xRRGGBB`.
#[inline(always)]
pub const fn hex(v: u32) -> u16 {
    rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

#[inline(always)]
pub fn from_color(c: Color) -> u16 {
    hex(c.0)
}

/// Zurück nach `0x00RRGGBB` (für PC-Vorschau, PNG/GIF).
pub fn to_rgb888(p: u16) -> u32 {
    let v = p as u32;
    let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
    (((r << 3) | (r >> 2)) << 16) | (((g << 2) | (g >> 4)) << 8) | ((b << 3) | (b >> 2))
}

/// Halbe Helligkeit (Schatten).
#[inline(always)]
pub fn darken(p: u16) -> u16 {
    (p >> 1) & 0x7BEF
}

/// 50/50-Mischung zweier Farben.
#[inline(always)]
pub fn avg(a: u16, b: u16) -> u16 {
    ((a & 0xF7DE) >> 1) + ((b & 0xF7DE) >> 1)
}

/// RGB565 → 32 Bit mit Grün in den oberen Bits (Platz für ×32 je Kanal).
#[inline(always)]
fn spread(v: u16) -> u32 {
    let v = v as u32;
    (v | (v << 16)) & 0x07E0_F81F
}

/// Wendet `one` bzw. – für ausgerichtete Pixelpaare – `pair` (zwei Pixel in
/// einem 32-Bit-Wort) auf alle Pixel von `row` an.
#[inline(always)]
fn map_pairs(row: &mut [u16], one: impl Fn(u16) -> u16, pair: impl Fn(u32) -> u32) {
    // SAFETY: align_to_mut liefert nur korrekt ausgerichtete u32.
    let (head, mid, tail) = unsafe { row.align_to_mut::<u32>() };
    for p in head.iter_mut().chain(tail.iter_mut()) {
        *p = one(*p);
    }
    for w in mid {
        *w = pair(*w);
    }
}

/// Mischt `a` → `b` mit `t` in 0..=32.
#[inline(always)]
pub fn mix(a: u16, b: u16, t: u32) -> u16 {
    // Grün in die oberen Bits spreizen: alle drei Kanäle in einem Durchgang.
    let spread = |v: u32| (v | (v << 16)) & 0x07E0_F81F;
    let (sx, sy) = (spread(a as u32), spread(b as u32));
    // Jedes Feld hat genug Luft für ×32 (R: Bit 11..20, G: 21..31, B: 0..9).
    let t = t.min(32);
    let m = ((sx * (32 - t) + sy * t) >> 5) & 0x07E0_F81F;
    (m | (m >> 16)) as u16
}

/// Mischt in 24-Bit-Farbe (genauer, z. B. für Nebel pro Dreieck).
#[inline]
pub fn mix_color(a: Color, b: Color, t: f32) -> Color {
    a.lerp(b, t)
}

/// Ein Bildpuffer (oder ein Zeilenstreifen davon) mit logischen Koordinaten.
/// Als Aufzeichner ([`Canvas::recorder`]) sammelt er die Zeichenbefehle in
/// einem [`Overlay`], statt Pixel zu setzen.
pub struct Canvas<'a> {
    px: &'a mut [u16],
    rec: Option<&'a mut Overlay>,
    /// Logische Breite/Höhe (Layout-Koordinaten).
    pub w: usize,
    pub h: usize,
    /// Pixel pro logischer Einheit (1 oder 2).
    pub s: i32,
    /// Physische Zeilenlänge.
    stride: usize,
    /// Erste physische Zeile, die in `px` liegt.
    row0: i32,
    /// Zeichenbereich (physische Zeilen) – alles außerhalb bleibt unberührt.
    y_min: i32,
    y_max: i32,
}

impl<'a> Canvas<'a> {
    /// Ganzes Bild, Faktor 1.
    pub fn new(px: &'a mut [u16], w: usize, h: usize) -> Self {
        assert!(px.len() >= w * h);
        Self { px, rec: None, w, h, s: 1, stride: w, row0: 0, y_min: 0, y_max: h as i32 }
    }

    /// Zeichnet nichts, sondern zeichnet die Befehle für ein Bild der
    /// physischen Größe `pw`×`ph` mit Faktor `s` in `ov` auf.
    pub fn recorder(ov: &'a mut Overlay, pw: usize, ph: usize, s: usize) -> Self {
        let s = s.max(1);
        Self {
            px: &mut [],
            rec: Some(ov),
            w: pw / s,
            h: ph / s,
            s: s as i32,
            stride: pw,
            row0: 0,
            y_min: 0,
            y_max: ph as i32,
        }
    }

    /// Im Aufzeichnungsmodus: Befehl ablegen (Zeilen `[y0, y1)` logisch) und
    /// `true` liefern.
    #[inline]
    fn record(&mut self, y0: i32, y1: i32, op: Op) -> bool {
        let s = self.s;
        match self.rec.as_deref_mut() {
            Some(r) => {
                r.ops.push(Rec { y0: y0 * s, y1: y1 * s, op });
                true
            }
            None => false,
        }
    }

    /// Wie [`Canvas::record`] für Kreise (Zeilen wie in `disc`).
    #[inline]
    fn record_disc(&mut self, cy: f32, r: f32, op: Op) -> bool {
        let s = self.s as f32;
        let (y0, y1) = (((cy - r) * s) as i32, ((cy + r) * s) as i32 + 1);
        match self.rec.as_deref_mut() {
            Some(rec) => {
                rec.ops.push(Rec { y0, y1, op });
                true
            }
            None => false,
        }
    }

    /// Streifen eines Bildes der physischen Größe `pw`×`ph` mit Faktor `s`:
    /// `px` enthält die Zeilen ab `row0`.
    pub fn scaled(px: &'a mut [u16], pw: usize, ph: usize, s: usize, row0: usize) -> Self {
        let rows = (px.len() / pw.max(1)).min(ph.saturating_sub(row0));
        let s = s.max(1);
        Self {
            px,
            rec: None,
            w: pw / s,
            h: ph / s,
            s: s as i32,
            stride: pw,
            row0: row0 as i32,
            y_min: row0 as i32,
            y_max: (row0 + rows) as i32,
        }
    }

    /// Nur physische Zeilen `[y0, y1)` zeichnen (innerhalb des Streifens).
    pub fn band(&mut self, y0: usize, y1: usize) {
        debug_assert!(self.rec.is_none(), "band() wird nicht aufgezeichnet");
        let rows_end = self.row0 + (self.px.len() / self.stride.max(1)) as i32;
        self.y_min = (y0 as i32).max(self.row0);
        self.y_max = (y1 as i32).min(rows_end).min((self.h as i32) * self.s);
    }

    /// Physisches Rechteck `[x0, x1) × [y0, y1)` zeilenweise bearbeiten.
    #[inline]
    fn rows_phys(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, mut f: impl FnMut(&mut [u16])) {
        let (x0, x1) = (x0.max(0), x1.min(self.stride as i32));
        let (y0, y1) = (y0.max(self.y_min), y1.min(self.y_max));
        if x0 >= x1 {
            return;
        }
        for yy in y0..y1 {
            let o = (yy - self.row0) as usize * self.stride;
            f(&mut self.px[o + x0 as usize..o + x1 as usize]);
        }
    }

    /// Rechteck füllen.
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        if self.record(y, y + h, Op::Fill { x, y, w, h, c }) {
            return;
        }
        let s = self.s;
        self.rows_phys(x * s, y * s, (x + w) * s, (y + h) * s, |row| row.fill(c));
    }

    /// Gefülltes Vieleck (logische Koordinaten, bis 16 Ecken, darf konkav
    /// sein: gefüllt wird nach der Gerade-ungerade-Regel, Pixelmitten).
    pub fn fill_poly(&mut self, pts: &[(f32, f32)], c: u16) {
        self.mix_poly(pts, c, 32);
    }

    /// Vieleck mit Farbe `c` und Anteil `t` (0..=32, 32 = deckend).
    pub fn mix_poly(&mut self, pts: &[(f32, f32)], c: u16, t: u32) {
        if t == 0 || pts.len() < 3 {
            return;
        }
        let pts = &pts[..pts.len().min(MAX_ECKEN)];
        let s = self.s as f32;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for &(_, y) in pts {
            lo = lo.min(y * s);
            hi = hi.max(y * s);
        }
        let (y0, y1) = (floor_i(lo), ceil_i(hi) + 1);
        if let Some(r) = self.rec.as_deref_mut() {
            let at = r.pts.len() as u32;
            r.pts.extend_from_slice(pts);
            r.ops.push(Rec { y0, y1, op: Op::Poly { at, len: pts.len() as u32, c, t } });
            return;
        }
        let n = pts.len();
        for py in y0.max(self.y_min)..y1.min(self.y_max) {
            let yc = py as f32 + 0.5;
            let mut xs = [0f32; MAX_ECKEN];
            let mut k = 0;
            for i in 0..n {
                let (ax, ay) = (pts[i].0 * s, pts[i].1 * s);
                let (bx, by) = (pts[(i + 1) % n].0 * s, pts[(i + 1) % n].1 * s);
                if (ay <= yc && by > yc) || (by <= yc && ay > yc) {
                    xs[k] = ax + (yc - ay) * (bx - ax) / (by - ay);
                    k += 1;
                }
            }
            xs[..k].sort_unstable_by(|a, b| a.total_cmp(b));
            let o = (py - self.row0) as usize * self.stride;
            for paar in xs[..k].chunks_exact(2) {
                let x0 = ceil_i(paar[0] - 0.5).max(0) as usize;
                let x1 = ceil_i(paar[1] - 0.5).min(self.stride as i32).max(0) as usize;
                if x0 < x1 {
                    let row = &mut self.px[o + x0..o + x1];
                    if t >= 32 {
                        row.fill(c);
                    } else {
                        for p in row {
                            *p = mix(*p, c, t);
                        }
                    }
                }
            }
        }
    }

    /// Halbtransparentes Rechteck (50 %).
    pub fn blend_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        if self.record(y, y + h, Op::Blend { x, y, w, h, c }) {
            return;
        }
        // Zwei Pixel je 32-Bit-Wort (kein Übertrag zwischen ihnen, siehe avg).
        let half = ((c & 0xF7DE) >> 1) as u32;
        let cc = half | half << 16;
        let s = self.s;
        self.rows_phys(x * s, y * s, (x + w) * s, (y + h) * s, |row| {
            map_pairs(row, |p| avg(p, c), |w| ((w & 0xF7DE_F7DE) >> 1) + cc)
        });
    }

    /// Abdunkeln (für Text-Hintergründe).
    pub fn darken_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if self.record(y, y + h, Op::Darken { x, y, w, h }) {
            return;
        }
        let s = self.s;
        self.rows_phys(x * s, y * s, (x + w) * s, (y + h) * s, |row| {
            map_pairs(row, darken, |w| (w >> 1) & 0x7BEF_7BEF)
        });
    }

    /// Mischen mit Anteil `t` (0..=32).
    pub fn tint_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16, t: u32) {
        if self.record(y, y + h, Op::Tint { x, y, w, h, c, t }) {
            return;
        }
        // Wie `mix`, aber Zielfarbe und Anteil nur einmal gerechnet; zwei
        // Pixel je Durchlauf mit getrennten Rechenketten (die Multiplikationen
        // warten nicht aufeinander).
        let t = t.min(32);
        let (inv, k) = (32 - t, spread(c) * t);
        let one = move |p: u16| {
            let m = ((spread(p) * inv + k) >> 5) & 0x07E0_F81F;
            (m | (m >> 16)) as u16
        };
        let s = self.s;
        self.rows_phys(x * s, y * s, (x + w) * s, (y + h) * s, |row| {
            map_pairs(row, one, |w| one(w as u16) as u32 | (one((w >> 16) as u16) as u32) << 16)
        });
    }

    /// Rahmen.
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        self.fill_rect(x, y, w, 1, c);
        self.fill_rect(x, y + h - 1, w, 1, c);
        self.fill_rect(x, y, 1, h, c);
        self.fill_rect(x + w - 1, y, 1, h, c);
    }

    /// Kreisscheibe um (`cx`, `cy`) mit Radius `r` (logisch), Pixel für Pixel
    /// in voller Auflösung: `f` bekommt je Zeile die Spanne und den Abstand zur
    /// Mitte in logischen Einheiten (für Ringe).
    fn disc(&mut self, cx: f32, cy: f32, r: f32, inner: f32, mut f: impl FnMut(&mut [u16])) {
        let s = self.s as f32;
        let (y0, y1) = (((cy - r) * s) as i32, ((cy + r) * s) as i32 + 1);
        for py in y0.max(self.y_min)..y1.min(self.y_max) {
            let dy = (py as f32 + 0.5) / s - cy;
            let o2 = r * r - dy * dy;
            if o2 <= 0.0 {
                continue;
            }
            let ho = sqrt(o2) * s;
            let hi = if inner > 0.0 && inner * inner > dy * dy { sqrt(inner * inner - dy * dy) * s } else { -1.0 };
            let xc = cx * s;
            let o = (py - self.row0) as usize * self.stride;
            let clamp = |v: f32| (v as i32).clamp(0, self.stride as i32) as usize;
            if hi < 0.0 {
                let (a, b) = (clamp(xc - ho + 0.5), clamp(xc + ho + 0.5));
                if a < b {
                    f(&mut self.px[o + a..o + b]);
                }
            } else {
                for (a, b) in [(xc - ho, xc - hi), (xc + hi, xc + ho)] {
                    let (a, b) = (clamp(a + 0.5), clamp(b + 0.5));
                    if a < b {
                        f(&mut self.px[o + a..o + b]);
                    }
                }
            }
        }
    }

    /// Halbtransparente Kreisscheibe.
    pub fn blend_disc(&mut self, cx: f32, cy: f32, r: f32, c: u16) {
        if self.record_disc(cy, r, Op::BlendDisc { cx, cy, r, c }) {
            return;
        }
        self.disc(cx, cy, r, 0.0, |row| {
            for p in row {
                *p = avg(*p, c);
            }
        });
    }

    /// Kreisring (Breite `thick`, logisch).
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: u16) {
        if self.record_disc(cy, r, Op::Ring { cx, cy, r, thick, c }) {
            return;
        }
        self.disc(cx, cy, r, (r - thick).max(0.0), |row| row.fill(c));
    }

    /// Durchscheinende Kreisscheibe: Farbe `c` mit Anteil `t` (0..=32).
    pub fn mix_disc(&mut self, cx: f32, cy: f32, r: f32, c: u16, t: u32) {
        if t == 0 || self.record_disc(cy, r, Op::MixDisc { cx, cy, r, c, t }) {
            return;
        }
        self.disc(cx, cy, r, 0.0, |row| {
            for p in row {
                *p = mix(*p, c, t);
            }
        });
    }

    /// Durchscheinender Kreisring: Farbe `c` mit Anteil `t` (0..=32).
    pub fn mix_ring(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: u16, t: u32) {
        if t == 0 || self.record_disc(cy, r, Op::MixRing { cx, cy, r, thick, c, t }) {
            return;
        }
        self.disc(cx, cy, r, (r - thick).max(0.0), |row| {
            for p in row {
                *p = mix(*p, c, t);
            }
        });
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, c: u16) {
        self.fill_rect(x, y, 1, 1, c);
    }

    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    fn glyph5x7(&mut self, rows: &[u8; 7], gx: i32, gy: i32, c: u16, sc: i32) {
        // Ganze Glyphe außerhalb des Bandes → nichts zu tun.
        if gy * self.s >= self.y_max || (gy + GLYPH_H * sc) * self.s <= self.y_min {
            return;
        }
        // Je Glyphenzeile die zusammenhängenden Pixel als ein Rechteck
        // (gleiches Ergebnis wie Pixel für Pixel, viel weniger Aufrufe).
        for (ry, &bits) in rows.iter().enumerate() {
            let y = gy + ry as i32 * sc;
            let mut rx = 0;
            while rx < 5 {
                if bits & (0x10 >> rx) == 0 {
                    rx += 1;
                    continue;
                }
                let start = rx;
                while rx < 5 && bits & (0x10 >> rx) != 0 {
                    rx += 1;
                }
                self.fill_rect(gx + start * sc, y, (rx - start) * sc, sc, c);
            }
        }
    }

    /// Text im 5×7-Zeichensatz (Großbuchstaben, Umlaute, Ziffern, Zeichen).
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    pub fn text(&mut self, x: i32, y: i32, s: &str, c: u16, scale: i32) {
        let sc = scale.max(1);
        if let Some(r) = self.rec.as_deref_mut() {
            // Umlautpunkte reichen 2 Einheiten über die Glyphe hinaus.
            let at = r.text.len() as u32;
            r.text.push_str(s);
            let op = Op::Text { x, y, at, len: s.len() as u32, c, scale };
            r.ops.push(Rec { y0: (y - 2 * sc) * self.s, y1: (y + GLYPH_H * sc) * self.s, op });
            return;
        }
        let mut cx = x;
        for ch in s.chars() {
            let base = match ch {
                'Ä' | 'ä' => Some('A'),
                'Ö' | 'ö' => Some('O'),
                'Ü' | 'ü' => Some('U'),
                _ => None,
            };
            if let Some(b) = base {
                // Umlaut: großer Grundbuchstabe, Punkte darüber (nicht verkleinert).
                self.glyph5x7(&glyph(b), cx, y, c, sc);
                self.fill_rect(cx + sc, y - 2 * sc, sc, sc, c);
                self.fill_rect(cx + 3 * sc, y - 2 * sc, sc, sc, c);
            } else if ch != ' ' {
                self.glyph5x7(&glyph(ch), cx, y, c, sc);
            }
            cx += ADVANCE * sc;
        }
    }

    /// Text mit Schatten (lesbar auf jedem Hintergrund).
    pub fn text_shadow(&mut self, x: i32, y: i32, s: &str, c: u16, shadow: u16, scale: i32) {
        let sc = scale.max(1);
        self.text(x + sc, y + sc, s, shadow, sc);
        self.text(x, y, s, c, sc);
    }

    /// Horizontal um `cx` zentriert, mit Schatten.
    pub fn text_centered(&mut self, cx: i32, y: i32, s: &str, c: u16, shadow: u16, scale: i32) {
        let w = text_width(s, scale);
        self.text_shadow(cx - w / 2, y, s, c, shadow, scale);
    }

    /// Winzige 3×5-Schrift (für die FPS-Anzeige).
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    pub fn tiny(&mut self, x: i32, y: i32, s: &str, c: u16) {
        if let Some(r) = self.rec.as_deref_mut() {
            let at = r.text.len() as u32;
            r.text.push_str(s);
            r.ops.push(Rec { y0: y * self.s, y1: (y + 5) * self.s, op: Op::Tiny { x, y, at, len: s.len() as u32, c } });
            return;
        }
        if y * self.s >= self.y_max || (y + 5) * self.s <= self.y_min {
            return;
        }
        let mut cx = x;
        for ch in s.chars() {
            let rows = tiny_glyph(ch);
            for (ry, &bits) in rows.iter().enumerate() {
                for rx in 0..3 {
                    if bits & (0b100 >> rx) != 0 {
                        self.put(cx + rx, y + ry as i32, c);
                    }
                }
            }
            cx += 4;
        }
    }
}

/// Höchstzahl der Ecken eines Vielecks ([`Canvas::fill_poly`]).
pub const MAX_ECKEN: usize = 16;

/// Abrunden auf eine ganze Zahl (ohne `std`).
fn floor_i(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) > x { i - 1 } else { i }
}

/// Aufrunden auf eine ganze Zahl (ohne `std`).
fn ceil_i(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) < x { i + 1 } else { i }
}

/// Ein aufgezeichneter Zeichenbefehl (Argumente wie beim gleichnamigen
/// [`Canvas`]-Aufruf; Text liegt in [`Overlay::text`]).
#[derive(Clone, Copy, Debug)]
enum Op {
    Fill { x: i32, y: i32, w: i32, h: i32, c: u16 },
    Blend { x: i32, y: i32, w: i32, h: i32, c: u16 },
    Darken { x: i32, y: i32, w: i32, h: i32 },
    Tint { x: i32, y: i32, w: i32, h: i32, c: u16, t: u32 },
    BlendDisc { cx: f32, cy: f32, r: f32, c: u16 },
    Ring { cx: f32, cy: f32, r: f32, thick: f32, c: u16 },
    MixDisc { cx: f32, cy: f32, r: f32, c: u16, t: u32 },
    MixRing { cx: f32, cy: f32, r: f32, thick: f32, c: u16, t: u32 },
    Text { x: i32, y: i32, at: u32, len: u32, c: u16, scale: i32 },
    Tiny { x: i32, y: i32, at: u32, len: u32, c: u16 },
    /// Vieleck: Ecken `pts[at..at + len]` in [`Overlay`], Anteil `t`.
    Poly { at: u32, len: u32, c: u16, t: u32 },
}

/// Befehl mit den physischen Zeilen `[y0, y1)`, die er höchstens berührt.
#[derive(Clone, Copy, Debug)]
struct Rec {
    y0: i32,
    y1: i32,
    op: Op,
}

/// HUD/Menü eines Bildes als Befehlsliste: einmal pro Bild aufzeichnen
/// (Texte formatieren, Layout rechnen), dann in jedem Streifen nur die
/// Befehle abspielen, die ihn berühren – bitgleich zum direkten Zeichnen.
#[derive(Default)]
pub struct Overlay {
    ops: alloc::vec::Vec<Rec>,
    text: alloc::string::String,
    pts: alloc::vec::Vec<(f32, f32)>,
}

impl Overlay {
    /// Mit Platz für typische Bilder (> 16 KB, landet auf dem ESP32 im PSRAM
    /// statt im knappen internen RAM).
    pub fn new() -> Self {
        Self {
            ops: alloc::vec::Vec::with_capacity(1024),
            text: alloc::string::String::with_capacity(20 * 1024),
            pts: alloc::vec::Vec::with_capacity(4096),
        }
    }

    pub fn clear(&mut self) {
        self.ops.clear();
        self.text.clear();
        self.pts.clear();
    }

    /// Anzahl der Befehle (Messung, Tests).
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Zieht eine Abtönung des ganzen Bildes (`tint_rect` über alles, `w`×`h`
    /// logisch) aus der Liste heraus, damit der Renderer sie auf die Farben
    /// der Dreiecke legt statt auf jedes Pixel. Die Abtönung ist affin
    /// (`p·(1−a) + C·a`), daher genügt es, die Farben aller früheren Befehle
    /// ebenso zu tönen (Abdunkeln wird Mischen mit getöntem Schwarz) – bis auf
    /// Rundung dasselbe Bild. Nur bei genau einer solchen Abtönung.
    pub fn hoist_full_tint(&mut self, w: i32, h: i32) -> Option<(u16, u32)> {
        let full = |r: &Rec| {
            matches!(r.op, Op::Tint { x, y, w: tw, h: th, t, .. }
                if x <= 0 && y <= 0 && x + tw >= w && y + th >= h && t > 0)
        };
        let mut found = self.ops.iter().enumerate().filter(|(_, r)| full(r)).map(|(k, _)| k);
        let k = found.next()?;
        if found.next().is_some() {
            return None;
        }
        let Op::Tint { c, t, .. } = self.ops[k].op else {
            return None;
        };
        let t = t.min(32);
        let m = |col: u16| mix(col, c, t);
        for r in &mut self.ops[..k] {
            r.op = match r.op {
                Op::Fill { x, y, w, h, c } => Op::Fill { x, y, w, h, c: m(c) },
                Op::Blend { x, y, w, h, c } => Op::Blend { x, y, w, h, c: m(c) },
                Op::Darken { x, y, w, h } => Op::Blend { x, y, w, h, c: m(0) },
                Op::Tint { x, y, w, h, c, t } => Op::Tint { x, y, w, h, c: m(c), t },
                Op::BlendDisc { cx, cy, r, c } => Op::BlendDisc { cx, cy, r, c: m(c) },
                Op::Ring { cx, cy, r, thick, c } => Op::Ring { cx, cy, r, thick, c: m(c) },
                Op::MixDisc { cx, cy, r, c, t } => Op::MixDisc { cx, cy, r, c: m(c), t },
                Op::MixRing { cx, cy, r, thick, c, t } => Op::MixRing { cx, cy, r, thick, c: m(c), t },
                Op::Text { x, y, at, len, c, scale } => Op::Text { x, y, at, len, c: m(c), scale },
                Op::Tiny { x, y, at, len, c } => Op::Tiny { x, y, at, len, c: m(c) },
                Op::Poly { at, len, c, t } => Op::Poly { at, len, c: m(c), t },
            };
        }
        self.ops.remove(k);
        Some((c, t))
    }

    /// Spielt alle Befehle ab, die die Zeilen von `c` berühren.
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    #[inline(never)]
    pub fn replay(&self, c: &mut Canvas) {
        for r in &self.ops {
            if r.y1 <= c.y_min || r.y0 >= c.y_max {
                continue;
            }
            match r.op {
                Op::Fill { x, y, w, h, c: col } => c.fill_rect(x, y, w, h, col),
                Op::Blend { x, y, w, h, c: col } => c.blend_rect(x, y, w, h, col),
                Op::Darken { x, y, w, h } => c.darken_rect(x, y, w, h),
                Op::Tint { x, y, w, h, c: col, t } => c.tint_rect(x, y, w, h, col, t),
                Op::BlendDisc { cx, cy, r, c: col } => c.blend_disc(cx, cy, r, col),
                Op::Ring { cx, cy, r, thick, c: col } => c.ring(cx, cy, r, thick, col),
                Op::MixDisc { cx, cy, r, c: col, t } => c.mix_disc(cx, cy, r, col, t),
                Op::MixRing { cx, cy, r, thick, c: col, t } => c.mix_ring(cx, cy, r, thick, col, t),
                Op::Text { x, y, at, len, c: col, scale } => {
                    c.text(x, y, &self.text[at as usize..(at + len) as usize], col, scale)
                }
                Op::Tiny { x, y, at, len, c: col } => c.tiny(x, y, &self.text[at as usize..(at + len) as usize], col),
                Op::Poly { at, len, c: col, t } => c.mix_poly(&self.pts[at as usize..(at + len) as usize], col, t),
            }
        }
    }
}

/// Breite eines Textes im 5×7-Satz.
pub fn text_width(s: &str, scale: i32) -> i32 {
    let n = s.chars().count() as i32;
    if n == 0 { 0 } else { (n * ADVANCE - 1) * scale.max(1) }
}

/// Breite im 3×5-Satz.
pub fn tiny_width(s: &str) -> i32 {
    let n = s.chars().count() as i32;
    if n == 0 { 0 } else { n * 4 - 1 }
}

/// 3×5-Glyphen (Bit 2 = linke Spalte).
fn tiny_glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 2, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'A' => [2, 5, 7, 5, 5],
        'B' => [6, 5, 6, 5, 6],
        'C' => [3, 4, 4, 4, 3],
        'D' => [6, 5, 5, 5, 6],
        'E' => [7, 4, 6, 4, 7],
        'F' => [7, 4, 6, 4, 4],
        'G' => [3, 4, 5, 5, 3],
        'H' => [5, 5, 7, 5, 5],
        'I' => [7, 2, 2, 2, 7],
        'J' => [1, 1, 1, 5, 2],
        'K' => [5, 5, 6, 5, 5],
        'L' => [4, 4, 4, 4, 7],
        'M' => [5, 7, 7, 5, 5],
        'N' => [6, 5, 5, 5, 5],
        'O' => [2, 5, 5, 5, 2],
        'P' => [6, 5, 6, 4, 4],
        'Q' => [2, 5, 5, 6, 3],
        'R' => [6, 5, 6, 5, 5],
        'S' => [3, 4, 2, 1, 6],
        'T' => [7, 2, 2, 2, 2],
        'U' => [5, 5, 5, 5, 7],
        'V' => [5, 5, 5, 5, 2],
        'W' => [5, 5, 7, 7, 5],
        'X' => [5, 5, 2, 5, 5],
        'Y' => [5, 5, 2, 2, 2],
        'Z' => [7, 1, 2, 4, 7],
        '.' => [0, 0, 0, 0, 2],
        ',' => [0, 0, 0, 2, 4],
        ':' => [0, 2, 0, 2, 0],
        '/' => [1, 1, 2, 4, 4],
        '%' => [5, 1, 2, 4, 5],
        '-' => [0, 0, 7, 0, 0],
        '+' => [0, 2, 7, 2, 0],
        '(' => [1, 2, 2, 2, 1],
        ')' => [4, 2, 2, 2, 4],
        _ => [0; 5],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    #[test]
    fn colors_roundtrip_and_blend() {
        let red = rgb(255, 0, 0);
        assert_eq!(red, 0xF800);
        assert_eq!(to_rgb888(red), 0xFF0000);
        assert_eq!(to_rgb888(hex(0x00FF00)), 0x00FF00);
        assert_eq!(mix(red, rgb(0, 0, 255), 0), red);
        assert_eq!(mix(red, rgb(0, 0, 255), 32), rgb(0, 0, 255));
        let half = to_rgb888(mix(rgb(0, 0, 0), rgb(255, 255, 255), 16));
        assert!((((half >> 16) & 255) as i32 - 128).abs() < 10);
        assert_eq!(darken(rgb(255, 255, 255)), 0x7BEF);
    }

    #[test]
    fn band_clipping_only_touches_band() {
        let mut buf = [0u16; 10 * 10];
        let mut c = Canvas::new(&mut buf, 10, 10);
        c.band(2, 5);
        c.fill_rect(0, 0, 10, 10, 7);
        c.text(0, 0, "AB", 9, 1);
        assert!(buf[..20].iter().all(|&p| p == 0));
        assert!(buf[50..].iter().all(|&p| p == 0));
        assert!(buf[20..50].iter().all(|&p| p == 7 || p == 9));
    }

    /// Faktor 2: Dasselbe Layout füllt doppelt so viele Pixel je Achse, und
    /// zwei Streifen ergeben zusammen dasselbe Bild wie ein ganzer Puffer.
    #[test]
    fn scaled_strips_match_full_canvas() {
        let draw = |c: &mut Canvas| {
            c.fill_rect(3, 2, 10, 5, 5);
            c.text(1, 3, "PUDDING 42", 9, 1);
            c.tiny(2, 12, "60 FPS", 3);
            c.blend_disc(12.0, 9.0, 5.5, 0xFFFF);
            c.ring(20.0, 9.0, 6.0, 1.5, 4);
        };
        let (pw, ph) = (64, 32);
        let mut full = vec![0u16; pw * ph];
        let mut c = Canvas::scaled(&mut full, pw, ph, 2, 0);
        assert_eq!((c.w, c.h), (32, 16));
        draw(&mut c);
        let mut split = vec![0u16; pw * ph];
        let (top, bottom) = split.split_at_mut(pw * 13);
        draw(&mut Canvas::scaled(top, pw, ph, 2, 0));
        draw(&mut Canvas::scaled(bottom, pw, ph, 2, 13));
        assert_eq!(full, split);
        // Ein logisches Pixel = 2×2 physische.
        assert_eq!(full[4 * pw + 6], 5);
        assert_eq!(full[5 * pw + 7], 5);
        assert!(full.contains(&4), "Ring sichtbar");
    }

    /// Die schnellen Rechteck-Effekte (Pixelpaare, vorgerechnete Mischung)
    /// stimmen mit den Einzelpixel-Funktionen überein – auch bei ungeraden
    /// Rändern.
    #[test]
    fn rect_effects_match_per_pixel_reference() {
        let (w, h) = (37usize, 9usize);
        let src: Vec<u16> = (0..w * h).map(|i| (i as u32).wrapping_mul(2_654_435_761) as u16).collect();
        let c = 0x2A1030u32;
        let col = hex(c);
        for (x0, x1) in [(0, 37), (1, 36), (3, 4), (2, 3)] {
            for t in [0u32, 7, 14, 32, 40] {
                type Eff = (&'static str, fn(u16, u16, u32) -> u16);
                let effects: [Eff; 3] = [
                    ("blend", |p, c, _| avg(p, c)),
                    ("darken", |p, _, _| darken(p)),
                    ("tint", |p, c, t| mix(p, c, t)),
                ];
                for (name, f) in effects {
                    let mut a = src.clone();
                    let mut cv = Canvas::new(&mut a, w, h);
                    let rw = (x1 - x0) as i32;
                    match name {
                        "blend" => cv.blend_rect(x0 as i32, 1, rw, 7, col),
                        "darken" => cv.darken_rect(x0 as i32, 1, rw, 7),
                        _ => cv.tint_rect(x0 as i32, 1, rw, 7, col, t),
                    }
                    for y in 0..h {
                        for x in 0..w {
                            let i = y * w + x;
                            let inside = (x0..x1).contains(&x) && (1..8).contains(&y);
                            let want = if inside { f(src[i], col, t) } else { src[i] };
                            assert_eq!(a[i], want, "{name} t={t} x0={x0} ({x},{y})");
                        }
                    }
                }
            }
        }
    }

    /// Ein Dreieck füllt die Pixel, deren Mitte innen liegt.
    #[test]
    fn poly_fuellt_dreieck() {
        let mut px = vec![0u16; 40 * 40];
        let mut c = Canvas::new(&mut px, 40, 40);
        c.fill_poly(&[(10.0, 10.0), (30.0, 10.0), (10.0, 30.0)], 7);
        assert_eq!(px[12 * 40 + 12], 7, "innen");
        assert_eq!(px[11 * 40 + 20], 7, "oben innen");
        assert_eq!(px[28 * 40 + 28], 0, "außerhalb der Schräge");
        assert_eq!(px[9 * 40 + 15], 0, "über dem Dreieck");
        assert_eq!(px[20 * 40 + 9], 0, "links daneben");
    }

    /// Aufgezeichnet und in Streifen abgespielt = direkt gezeichnet, auch
    /// halb durchsichtig und mit Faktor 2.
    #[test]
    fn poly_aufgezeichnet_gleich_direkt() {
        let (w, h) = (64usize, 48usize);
        let pts = [(3.5, 2.0), (28.0, 6.5), (25.0, 21.0), (8.0, 23.5)];
        let grund: Vec<u16> = (0..w * h).map(|i| (i as u32).wrapping_mul(2_654_435_761) as u16).collect();
        for t in [32u32, 16] {
            let mut direkt = grund.clone();
            {
                let mut c = Canvas::scaled(&mut direkt, w, h, 2, 0);
                c.mix_poly(&pts, 0x1234, t);
            }
            let mut ov = Overlay::new();
            {
                let mut c = Canvas::recorder(&mut ov, w, h, 2);
                c.mix_poly(&pts, 0x1234, t);
            }
            let mut streifen = grund.clone();
            for y0 in (0..h).step_by(8) {
                let mut c = Canvas::scaled(&mut streifen[y0 * w..(y0 + 8) * w], w, h, 2, y0);
                ov.replay(&mut c);
            }
            assert_eq!(direkt, streifen, "t={t}");
            assert_ne!(direkt, grund, "t={t}: etwas gezeichnet");
        }
    }
}
