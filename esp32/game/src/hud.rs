//! Spiel-HUD im 320×180-Layout (bei 640×360 doppelt so groß gezeichnet):
//! Kirschen, Zeit, Hitze-Balken, Meldungen, Tipps, Titel, Ziel-Anzeige und
//! die kleine FPS-/Renderzeit-Anzeige oben links.

use crate::gfx::{Canvas, hex, text_width, tiny_width};
use alloc::vec::Vec;
use core::fmt::Write as _;
use spatz::color::Color;
use spatz::render::font::LINE_H;
use spatz::render::overlay::{TextBuf, fmt};

/// Meldungstext auf dem Stack – Meldungen erzeugen keine Heap-Allokation.
pub type MsgText = TextBuf<48>;

pub struct Message {
    pub text: MsgText,
    pub t: f32,
    pub dur: f32,
    pub color: u32,
}

pub struct Messages {
    pub list: Vec<Message>,
}

impl Default for Messages {
    fn default() -> Self {
        Self::new()
    }
}

impl Messages {
    pub fn new() -> Self {
        Self { list: Vec::with_capacity(4) }
    }

    pub fn push(&mut self, text: &str, color: u32) {
        self.list.retain(|m| m.t > 0.15 || m.dur < 1.0);
        let mut t = MsgText::new();
        let _ = t.write_str(text);
        self.list.push(Message { text: t, t: 0.0, dur: 1.2, color });
        if self.list.len() > 3 {
            self.list.remove(0);
        }
    }

    pub fn update(&mut self, dt: f32) {
        for m in &mut self.list {
            m.t += dt;
        }
        self.list.retain(|m| m.t < m.dur);
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

pub struct HudData<'a> {
    pub cherries: u32,
    pub total_cherries: u32,
    pub time: f32,
    pub melt: f32,
    pub hint: Option<&'a str>,
    pub title_t: f32,
    pub level_name: &'a str,
    pub demo: bool,
    pub dead_t: Option<f32>,
    /// Gefrierfach: Leiste zeigt Frost (blau) statt Hitze.
    pub frost: bool,
    /// Ziel-Text am Kapitelanfang.
    pub goal: &'a str,
    /// Übrige und gesamte Leben (nur auf „Schwer“).
    pub lives: Option<(u32, u32)>,
    /// Akkustand in Prozent (nur mit Akkuanzeige, siehe Firmware `board.rs`).
    pub battery: Option<u8>,
}

const SHADOW: u16 = hex(0x1A0A10);

/// Minuten, Sekunden, Zehntel.
pub fn clock(t: f32) -> (u32, u32, u32) {
    let total = (t.max(0.0) * 10.0) as u32;
    (total / 600, (total / 10) % 60, total % 10)
}

/// Text mittig, bei Überbreite automatisch eine Stufe kleiner.
pub fn centered_fit(c: &mut Canvas, y: i32, s: &str, color: u16, scale: i32) {
    let mut sc = scale;
    while sc > 1 && text_width(s, sc) > c.w as i32 - 8 {
        sc -= 1;
    }
    c.text_centered(c.w as i32 / 2, y, s, color, SHADOW, sc);
}

pub fn draw(c: &mut Canvas, d: &HudData, msgs: &Messages) {
    let w = c.w as i32;
    let h = c.h as i32;

    // --- Oben rechts: Kirschen, Zeit, Hitze ------------------------------
    let pw = 88;
    let x = w - pw - 2;
    let rows = if d.lives.is_some() { 3 } else { 2 };
    c.blend_rect(x - 2, 1, pw + 3, rows * LINE_H + 6, hex(0x2A1020));
    let t = fmt::<24>(format_args!("● {}/{}", d.cherries, d.total_cherries));
    c.text_shadow(x, 3, t.as_str(), hex(0xFF4060), SHADOW, 1);
    let (m, sec, ds) = clock(d.time);
    let t = fmt::<24>(format_args!("{:02}:{:02}.{}", m, sec, ds));
    c.text_shadow(x + pw - text_width(t.as_str(), 1), 3, t.as_str(), hex(0xFFFFFF), SHADOW, 1);
    let y = 3 + LINE_H + 2;
    let (label, label_col) = if d.frost { ("FROST", hex(0xB0E8FF)) } else { ("HITZE", hex(0xFFD0A0)) };
    c.text_shadow(x, y, label, label_col, SHADOW, 1);
    let (bx, bw, bh) = (x + 34, pw - 34, 7);
    c.fill_rect(bx, y, bw, bh, hex(0x301018));
    let melt = d.melt.clamp(0.0, 1.0);
    let col = if d.frost {
        Color::hex(0x9EE0FF).lerp(Color::hex(0x3060FF), melt)
    } else if melt < 0.5 {
        Color::hex(0x5AC8FF).lerp(Color::hex(0xFFD040), melt * 2.0)
    } else {
        Color::hex(0xFFD040).lerp(Color::hex(0xFF3030), (melt - 0.5) * 2.0)
    };
    let blink = melt > 0.8 && ((d.time * 6.0) as i32) % 2 == 0;
    if !blink {
        c.fill_rect(bx + 1, y + 1, ((bw - 2) as f32 * melt) as i32, bh - 2, crate::gfx::from_color(col));
    }
    c.frame(bx, y, bw, bh, hex(0xFFFFFF));
    if let Some((left, total)) = d.lives {
        let ly = y + LINE_H + 1;
        c.text_shadow(x, ly, "LEBEN", hex(0xFFB0C0), SHADOW, 1);
        for i in 0..total as i32 {
            let col = if (i as u32) < left { hex(0xFF4060) } else { hex(0x503040) };
            c.fill_rect(bx + 2 + i * 10, ly + 1, 7, 6, col);
        }
    }

    // --- Meldungen in der Mitte ------------------------------------------
    let mut my = h * 30 / 100;
    for msg in msgs.list.iter().rev().take(2) {
        let pop = if msg.t < 0.12 { 1 } else { 0 };
        let fade = msg.t > msg.dur - 0.2 && ((msg.t * 20.0) as i32) % 2 == 0;
        if !fade {
            centered_fit(c, my - pop * 2, msg.text.as_str(), hex(msg.color), 2 + pop);
        }
        my += 18;
    }

    // --- Tipp unten --------------------------------------------------------
    if let Some(hint) = d.hint {
        let tw = text_width(hint, 1).min(w - 4);
        let hy = h - 12;
        c.blend_rect(w / 2 - tw / 2 - 3, hy - 3, tw + 6, 12, 0);
        centered_fit(c, hy, hint, hex(0xFFF2A0), 1);
    }

    if d.demo {
        c.text_centered(w / 2, h - 24, "DEMO", hex(0xA0FFA0), SHADOW, 1);
    }

    // --- Titel am Levelanfang ---------------------------------------------
    if d.title_t > 0.0 {
        let k = (d.title_t / 0.4).min(1.0);
        if k > 0.3 {
            let ty = h * 30 / 100;
            centered_fit(c, ty, "PUDDING-PANIK!", hex(0xFFE14D), 2);
            centered_fit(c, ty + 20, d.level_name, hex(0xFFFFFF), 1);
            centered_fit(c, ty + 32, d.goal, hex(0xFFD0E0), 1);
        }
    }

    // --- Akku oben links neben dem Pause-Knopf -----------------------------
    if let Some(p) = d.battery {
        battery_icon(c, 26, 33, p, d.time);
    }

    if let Some(t) = d.dead_t
        && t > 0.6
    {
        let k = (((t - 0.6) * 60.0) as u32).min(26);
        c.tint_rect(0, 0, w, h, 0, k);
    }
}

/// Kleine Batterie mit Füllstand und Prozent (32×9 logisch ab `x`, `y`):
/// grün, unter 35 % gelb, unter 15 % rot, unter 10 % blinkend.
pub fn battery_icon(c: &mut Canvas, x: i32, y: i32, pct: u8, t: f32) {
    let p = pct.min(100) as i32;
    if p < 10 && ((t * 2.0) as i32) % 2 == 0 {
        return;
    }
    let col = if p < 15 {
        hex(0xFF4040)
    } else if p < 35 {
        hex(0xFFD040)
    } else {
        hex(0x7CFF7C)
    };
    let white = hex(0xFFFFFF);
    c.darken_rect(x - 1, y - 1, 33, 9);
    c.frame(x, y, 13, 7, white);
    c.fill_rect(x + 13, y + 2, 1, 3, white);
    c.fill_rect(x + 1, y + 1, (11 * p + 50) / 100, 5, col);
    let txt = fmt::<8>(format_args!("{p}%"));
    c.tiny(x + 16, y + 1, txt.as_str(), col);
}

/// Leistungswerte für die Anzeige oben links (misst die Firmware).
#[derive(Clone, Copy, Debug, Default)]
pub struct Perf {
    pub fps: f32,
    /// Rendern (3D + HUD), längster der beiden Kerne.
    pub render_ms: f32,
    /// Scharfes Vergrößern durch die CPU (0 bei „weich“: macht der PPA).
    pub scale_ms: f32,
    /// Arbeitszeit pro Frame (Eingabe, Simulation, Rendern, Ausgabe) ohne
    /// das Warten auf VSync – für 55 FPS muss sie unter ~18,2 ms bleiben.
    pub frame_ms: f32,
}

/// Kleine Leistungsanzeige oben links: Bilder pro Sekunde, Renderzeit,
/// Zeit fürs Vergrößern und Frame-Zeit.
pub fn perf_overlay(c: &mut Canvas, p: &Perf) {
    let lines = [
        fmt::<24>(format_args!("{:.0} FPS", p.fps)),
        fmt::<24>(format_args!("RENDER {:.1}MS", p.render_ms)),
        fmt::<24>(format_args!("SKALIER {:.1}MS", p.scale_ms)),
        fmt::<24>(format_args!("FRAME {:.1}MS", p.frame_ms)),
    ];
    let wbox = lines.iter().map(|l| tiny_width(l.as_str())).max().unwrap_or(0) + 4;
    c.darken_rect(0, 0, wbox, 26);
    let colors = [hex(0x9CFF9C), hex(0xFFFFFF), hex(0xFFE0A0), hex(0xC8C8FF)];
    for (i, (l, col)) in lines.iter().zip(colors).enumerate() {
        c.tiny(2, 2 + 6 * i as i32, l.as_str(), col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{H, W};
    use alloc::vec;

    /// Die Akkuanzeige liegt im HUD-Bereich oben links (den die
    /// Touch-Steuerung nie überdeckt) und neben dem Pause-Knopf (x 4–21).
    #[test]
    fn battery_icon_stays_in_its_corner() {
        for pct in [0u8, 9, 14, 34, 87, 100] {
            let mut px = vec![0u16; W * H];
            let mut c = Canvas::new(&mut px, W, H);
            battery_icon(&mut c, 26, 33, pct, 0.75);
            let mut drawn = 0;
            for y in 0..H {
                for x in 0..W {
                    if px[y * W + x] != 0 {
                        drawn += 1;
                        assert!((25..60).contains(&x) && (32..48).contains(&y), "{pct}%: Pixel bei ({x},{y})");
                    }
                }
            }
            assert!(drawn > 40, "{pct}% sichtbar");
        }
    }

    #[test]
    fn hud_and_perf_draw_without_panicking() {
        let mut px = vec![0u16; W * H];
        let mut c = Canvas::new(&mut px, W, H);
        let mut m = Messages::new();
        m.push("CHECKPOINT! FRISCH GEKÜHLT", 0x7CFF7C);
        let d = HudData {
            cherries: 3,
            total_cherries: 27,
            time: 83.4,
            melt: 0.9,
            hint: Some("KNÖPFE: LAUFEN   SPRINGEN: HÜPFEN"),
            title_t: 2.0,
            level_name: "KEKSINSEL",
            demo: true,
            dead_t: Some(1.0),
            frost: true,
            goal: "AB IN DEN KÜHLSCHRANK!",
            lives: Some((2, 3)),
            battery: Some(8),
        };
        draw(&mut c, &d, &m);
        perf_overlay(&mut c, &Perf { fps: 54.9, render_ms: 9.2, scale_ms: 2.1, frame_ms: 18.2 });
        assert!(px.iter().any(|&p| p != 0));
        assert_eq!(clock(83.45), (1, 23, 4));
    }
}
