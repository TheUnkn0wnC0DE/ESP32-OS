//! Framebuffer: Farbpuffer (`0x00RRGGBB`) + Tiefenpuffer (Ganzzahl).

use alloc::vec;
use alloc::vec::Vec;

/// Fixpunkt-Skala der Tiefenwerte: `depth = near / w · DEPTH_SCALE`.
/// Größer = näher; `0` = leer (Himmel).
pub const DEPTH_SCALE: f32 = (1u32 << 30) as f32;

/// Farb- und Tiefenpuffer in der internen Renderauflösung.
pub struct Framebuffer {
    width: usize,
    height: usize,
    /// Farbwerte zeilenweise, oben links beginnend.
    pub color: Vec<u32>,
    /// Tiefe als `near/w · 2^30` (0 = nichts gezeichnet).
    /// Wer hier direkt schreibt, ruft danach [`Framebuffer::mark_depth_dirty`].
    pub depth: Vec<i32>,
    /// Tiefenpuffer ist komplett 0 (z. B. vom Nebel-Durchlauf mitgelöscht).
    depth_clean: bool,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Self { width, height, color: vec![0; n], depth: vec![0; n], depth_clean: true }
    }

    /// Ist der Tiefenpuffer garantiert leer?
    #[inline(always)]
    pub fn depth_is_clean(&self) -> bool {
        self.depth_clean
    }

    /// Nach direkten Schreibzugriffen auf `depth` aufrufen.
    #[inline(always)]
    pub fn mark_depth_dirty(&mut self) {
        self.depth_clean = false;
    }

    /// Nur für den Renderer: Tiefenpuffer wurde vollständig geleert.
    #[inline(always)]
    pub(crate) fn mark_depth_clean(&mut self) {
        self.depth_clean = true;
    }

    /// Ändert die Größe (alloziert nur, wenn nötig).
    pub fn resize(&mut self, width: usize, height: usize) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        let n = width * height;
        self.color.clear();
        self.color.resize(n, 0);
        self.depth.clear();
        self.depth.resize(n, 0);
        self.depth_clean = true;
    }

    #[inline(always)]
    pub fn width(&self) -> usize {
        self.width
    }
    #[inline(always)]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn clear_depth(&mut self) {
        self.depth.fill(0);
        self.depth_clean = true;
    }

    #[inline]
    pub fn clear_color(&mut self, c: u32) {
        self.color.fill(c);
    }

    #[inline]
    pub fn pixel(&self, x: usize, y: usize) -> u32 {
        self.color[y * self.width + x]
    }

    /// Setzt einen Pixel (mit Grenzprüfung).
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.color[y as usize * self.width + x as usize] = c;
        }
    }

    /// Clippt ein Rechteck auf den Puffer; `None`, wenn leer.
    #[inline]
    fn clip_rect(&self, x: i32, y: i32, w: i32, h: i32) -> Option<(usize, usize, usize, usize)> {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.width as i32);
        let y1 = (y + h).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 { None } else { Some((x0 as usize, y0 as usize, x1 as usize, y1 as usize)) }
    }

    /// Gefülltes Rechteck (ohne Tiefentest).
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        if let Some((x0, y0, x1, y1)) = self.clip_rect(x, y, w, h) {
            for row in y0..y1 {
                let o = row * self.width;
                self.color[o + x0..o + x1].fill(c);
            }
        }
    }

    /// Halbtransparentes Rechteck: mischt `c` mit Anteil `alpha` (0..=256).
    /// `alpha == 128` nutzt einen besonders billigen 50-%-Pfad.
    pub fn blend_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32, alpha: u32) {
        if let Some((x0, y0, x1, y1)) = self.clip_rect(x, y, w, h) {
            for row in y0..y1 {
                let o = row * self.width;
                let span = &mut self.color[o + x0..o + x1];
                if alpha == 128 {
                    for p in span {
                        *p = crate::color::avg_packed(*p, c);
                    }
                } else {
                    for p in span {
                        *p = crate::color::blend256(*p, c, alpha);
                    }
                }
            }
        }
    }

    /// Rechteck-Umriss.
    pub fn rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        self.fill_rect(x, y, w, 1, c);
        self.fill_rect(x, y + h - 1, w, 1, c);
        self.fill_rect(x, y, 1, h, c);
        self.fill_rect(x + w - 1, y, 1, h, c);
    }

    /// Bresenham-Linie (für Debug-Zeichnungen).
    pub fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: u32) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Zeilen-Slices für Farbe und Tiefe (für Rasterizer-Spans).
    #[inline(always)]
    pub(crate) fn rows_mut(&mut self, y: usize) -> (&mut [u32], &mut [i32]) {
        let o = y * self.width;
        let w = self.width;
        (&mut self.color[o..o + w], &mut self.depth[o..o + w])
    }

    /// Skaliert das Bild per Nearest-Neighbour um ganzzahlige Faktoren
    /// hoch (für Screenshots).
    pub fn upscaled(&self, factor: usize) -> (usize, usize, Vec<u32>) {
        let f = factor.max(1);
        let (w, h) = (self.width * f, self.height * f);
        let mut out = vec![0u32; w * h];
        for y in 0..h {
            let src = &self.color[(y / f) * self.width..(y / f + 1) * self.width];
            let dst = &mut out[y * w..(y + 1) * w];
            for (x, d) in dst.iter_mut().enumerate() {
                *d = src[x / f];
            }
        }
        (w, h, out)
    }
}
