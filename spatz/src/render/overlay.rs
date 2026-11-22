//! Performance-Overlay: FPS, Frametimes (min/avg/max/p99), CPU-Aufteilung,
//! Dreieckszähler und ein Frametime-Graph. Komplett allokationsfrei.

use super::font::{LINE_H, draw_text_shadow};
use super::framebuffer::Framebuffer;
use super::pipeline::RenderStats;
use crate::time::{FrameStats, FrameSummary};
use core::fmt::Write;

/// Text-Puffer auf dem Stack (für `write!` ohne Allokation).
pub struct TextBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Default for TextBuf<N> {
    fn default() -> Self {
        Self { buf: [0; N], len: 0 }
    }
}

impl<const N: usize> TextBuf<N> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn as_str(&self) -> &str {
        // Nur ganze UTF-8-Zeichen werden geschrieben (siehe write_str).
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl<const N: usize> Write for TextBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let mut end = s.len().min(N - self.len);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        self.buf[self.len..self.len + end].copy_from_slice(&s.as_bytes()[..end]);
        self.len += end;
        Ok(())
    }
}

/// Formatiert in einen Stack-Puffer.
pub fn fmt<const N: usize>(args: core::fmt::Arguments<'_>) -> TextBuf<N> {
    let mut t = TextBuf::<N>::new();
    let _ = t.write_fmt(args);
    t
}

const WHITE: u32 = 0xFFFFFF;
const SHADOW: u32 = 0x000000;
const DIM: u32 = 0xA0B0C0;

/// Perf-Overlay mit Zwischenspeicher: Die Zusammenfassung ändert sich nur
/// ~4× pro Sekunde, deshalb werden ihre Gleitkommazahlen nur dann neu
/// formatiert (Schlüssel = die Zusammenfassung selbst). Ausgabe identisch zu
/// [`draw_perf_overlay`].
#[derive(Default)]
pub struct PerfOverlay {
    key: Option<FrameSummary>,
    fps_col: u32,
    l1: TextBuf<48>,
    l2: TextBuf<48>,
    l3: TextBuf<48>,
    top: f32,
    top_label: TextBuf<24>,
}

impl PerfOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    fn refresh(&mut self, s: FrameSummary) {
        if self.key == Some(s) {
            return;
        }
        self.fps_col = if s.fps >= 55.0 {
            0x7CFF7C
        } else if s.fps >= 28.0 {
            0xFFE060
        } else {
            0xFF6060
        };
        self.l1 = fmt::<48>(format_args!("FPS {:.0}  {:.2} MS", s.fps, s.avg_ms));
        self.l2 = fmt::<48>(format_args!("MIN {:.2} MAX {:.2} P99 {:.2}", s.min_ms, s.max_ms, s.p99_ms));
        self.l3 =
            fmt::<48>(format_args!("CPU {:.2} U{:.2} R{:.2} P{:.2}", s.cpu_ms, s.update_ms, s.render_ms, s.present_ms));
        self.top = nice_ceiling((s.max_ms * 1.2).max(1.0));
        self.top_label = fmt::<24>(format_args!("{} MS", self.top));
        self.key = Some(s);
    }

    /// Zeichnet das kompakte Overlay oben links. `extra` = zusätzliche
    /// Kurzzeilen (z. B. VSync-Status). Bei hoher Auflösung wird skaliert.
    pub fn draw(&mut self, fb: &mut Framebuffer, stats: &FrameStats, r: &RenderStats, extra: &[&str]) {
        self.refresh(stats.summary());
        let k = (fb.height() as i32 / 270).max(1);
        let x = 3 * k;
        let mut y = 3 * k;
        let lh = LINE_H * k;
        let graph_w = 156 * k;
        let graph_h = 22 * k;
        let lines = 4 + extra.len() as i32;
        fb.blend_rect(0, 0, graph_w + 6 * k, lines * lh + graph_h + 8 * k, 0x101820, 128);
        draw_text_shadow(fb, x, y, self.l1.as_str(), self.fps_col, SHADOW, k);
        y += lh;
        draw_text_shadow(fb, x, y, self.l2.as_str(), WHITE, SHADOW, k);
        y += lh;
        draw_text_shadow(fb, x, y, self.l3.as_str(), 0x80E0FF, SHADOW, k);
        y += lh;
        let t = fmt::<48>(format_args!(
            "TRI {}/{} OBJ {}/{}",
            r.tris_drawn,
            r.tris_in,
            r.objects - r.objects_culled,
            r.objects
        ));
        draw_text_shadow(fb, x, y, t.as_str(), DIM, SHADOW, k);
        y += lh;
        for line in extra {
            draw_text_shadow(fb, x, y, line, DIM, SHADOW, k);
            y += lh;
        }
        draw_graph(fb, stats, self.top, self.top_label.as_str(), x, y + k, graph_w, graph_h, k);
    }
}

/// Zeichnet das Overlay ohne Zwischenspeicher (formatiert jedes Mal neu).
pub fn draw_perf_overlay(fb: &mut Framebuffer, stats: &FrameStats, r: &RenderStats, extra: &[&str]) {
    PerfOverlay::new().draw(fb, stats, r, extra);
}

/// Frametime-Graph: Balken = Frame-Zeit (grün/gelb/rot nach 60/30-FPS-Grenze),
/// cyan = reine CPU-Zeit. Die Skala passt sich an (Beschriftung oben links).
fn draw_graph(
    fb: &mut Framebuffer,
    stats: &FrameStats,
    top: f32,
    top_label: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    k: i32,
) {
    fb.fill_rect(x, y, w, h, 0x182028);
    let scale = h as f32 / top;
    let cols = w / k;
    let n = stats.len() as i32;
    let skip = (n - cols).max(0) as usize;
    let mut bx = x + (cols - n.min(cols)) * k;
    for (ft, ct) in stats.history().zip(stats.cpu_history()).skip(skip) {
        let fh = ((ft * scale) as i32).clamp(1, h);
        let ch = ((ct * scale) as i32).clamp(0, h);
        let col = if ft <= 17.0 {
            0x50D050
        } else if ft <= 34.0 {
            0xE0C040
        } else {
            0xE05050
        };
        fb.fill_rect(bx, y + h - fh, k, fh, col);
        fb.fill_rect(bx, y + h - ch, k, ch.min(fh), 0x40C0FF);
        bx += k;
    }
    for (ms, label) in [(16.67f32, "60"), (33.33, "30")] {
        if ms < top {
            let ly = y + h - (ms * scale) as i32;
            fb.fill_rect(x, ly, w, 1, 0x707070);
            draw_text_shadow(fb, x + w - 13 * k, ly - 8 * k, label, 0xC0C0C0, SHADOW, k);
        }
    }
    draw_text_shadow(fb, x + 2 * k, y + 2 * k, top_label, 0xC0C0C0, SHADOW, k);
}

/// Nächster „schöner“ Skalenwert (1, 2, 5, 10, 20, 50, ...).
fn nice_ceiling(v: f32) -> f32 {
    let mut base = 1.0f32;
    while base * 10.0 <= v {
        base *= 10.0;
    }
    while base > v {
        base /= 10.0;
    }
    for m in [1.0, 2.0, 5.0, 10.0] {
        if base * m >= v {
            return base * m;
        }
    }
    base * 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textbuf_and_nice_ceiling() {
        let t = fmt::<8>(format_args!("{}-{}", 12345, 678));
        assert_eq!(t.as_str(), "12345-67");
        assert_eq!(nice_ceiling(3.1), 5.0);
        assert_eq!(nice_ceiling(0.7), 1.0);
        assert_eq!(nice_ceiling(18.0), 20.0);
    }
}
