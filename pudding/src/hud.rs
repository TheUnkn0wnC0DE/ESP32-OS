//! Spiel-HUD: Kirschen, Zeit, Schmelz-Anzeige, Meldungen, Tipps, Titel,
//! Ziel- und Pausenbildschirm. (Das Perf-Overlay liefert die Engine.)

use core::fmt::Write as _;
use spatz::render::Framebuffer;
use spatz::render::font::{LINE_H, draw_text_centered, draw_text_shadow, text_width};
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
    /// Kapazität vorab reservieren: höchstens 4 Einträge gleichzeitig.
    pub fn new() -> Self {
        Self { list: Vec::with_capacity(4) }
    }

    pub fn push(&mut self, text: &str, color: u32) {
        self.list.retain(|m| m.t > 0.15 || m.dur < 1.0); // ältere sofort verdrängen
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

pub struct FinishStats {
    pub time: f32,
    pub cherries: u32,
    pub total: u32,
    pub deaths: u32,
}

pub struct HudData<'a> {
    pub cherries: u32,
    pub total_cherries: u32,
    pub time: f32,
    pub melt: f32,
    pub hint: Option<&'a str>,
    pub finished: Option<FinishStats>,
    pub finish_t: f32,
    pub paused: bool,
    pub title_t: f32,
    pub demo: bool,
    pub dead_t: Option<f32>,
}

const SHADOW: u32 = 0x1A0A10;

fn clock(t: f32) -> (u32, u32, u32) {
    let total = (t.max(0.0) * 10.0) as u32;
    (total / 600, (total / 10) % 60, total % 10)
}

pub fn draw(fb: &mut Framebuffer, d: &HudData, msgs: &Messages) {
    let w = fb.width() as i32;
    let h = fb.height() as i32;
    let s = (h / 270).max(1);

    // --- Oben rechts: Kirschen, Zeit, Kühlung ---------------------------
    let panel_w = 92 * s;
    let x = w - panel_w - 4 * s;
    let mut y = 4 * s;
    fb.blend_rect(x - 3 * s, y - 3 * s, panel_w + 6 * s, 3 * LINE_H * s + 12 * s, 0x2A1020, 128);
    let t = fmt::<32>(format_args!("● {}/{}", d.cherries, d.total_cherries));
    draw_text_shadow(fb, x, y, t.as_str(), 0xFF4060, SHADOW, s);
    y += LINE_H * s + 2 * s;
    let (m, sec, ds) = clock(d.time);
    let t = fmt::<32>(format_args!("ZEIT {:02}:{:02}.{}", m, sec, ds));
    draw_text_shadow(fb, x, y, t.as_str(), 0xFFFFFF, SHADOW, s);
    y += LINE_H * s + 2 * s;
    // Schmelz-Balken: blau (kalt) → rot (heiß), blinkt kurz vor Schluss.
    draw_text_shadow(fb, x, y, "HITZE", 0xFFD0A0, SHADOW, s);
    let bx = x + 32 * s;
    let bw = panel_w - 34 * s;
    let bh = 7 * s;
    fb.fill_rect(bx, y, bw, bh, 0x301018);
    let m = d.melt.clamp(0.0, 1.0);
    let col = if m < 0.5 {
        spatz::color::Color::hex(0x5AC8FF).lerp(spatz::color::Color::hex(0xFFD040), m * 2.0).0
    } else {
        spatz::color::Color::hex(0xFFD040).lerp(spatz::color::Color::hex(0xFF3030), (m - 0.5) * 2.0).0
    };
    let blink = m > 0.8 && ((d.time * 6.0) as i32) % 2 == 0;
    if !blink {
        fb.fill_rect(bx + s, y + s, ((bw - 2 * s) as f32 * m) as i32, bh - 2 * s, col);
    }
    fb.rect_outline(bx, y, bw, bh, 0xFFFFFF);

    // --- Meldungen in der Mitte (mit „Plopp“-Animation) -----------------
    let mut my = h * 32 / 100;
    for msg in msgs.list.iter().rev().take(2) {
        let pop = if msg.t < 0.12 { 2 } else { 0 };
        let scale = 2 * s + pop * s / 2;
        let fade = msg.t > msg.dur - 0.2 && ((msg.t * 20.0) as i32) % 2 == 0;
        if !fade {
            draw_text_centered(fb, w / 2, my - pop * 3, msg.text.as_str(), msg.color, SHADOW, scale);
        }
        my += 20 * s;
    }

    // --- Tipp unten ------------------------------------------------------
    if let Some(hint) = d.hint {
        let tw = text_width(hint, s);
        let hy = h - 16 * s;
        fb.blend_rect(w / 2 - tw / 2 - 5 * s, hy - 4 * s, tw + 10 * s, 15 * s, 0x000000, 140);
        draw_text_centered(fb, w / 2, hy, hint, 0xFFF2A0, SHADOW, s);
    }

    if d.demo {
        draw_text_centered(fb, w / 2, 4 * s, "DEMO: AUTOPILOT (TAB)", 0xA0FFA0, SHADOW, s);
    }

    // --- Titel am Anfang -------------------------------------------------
    if d.title_t > 0.0 && d.finished.is_none() {
        let k = (d.title_t / 0.4).min(1.0);
        if k > 0.3 {
            let ty = h * 36 / 100;
            draw_text_centered(fb, w / 2, ty, "PUDDING-PANIK!", 0xFFE14D, 0x80203A, 3 * s);
            draw_text_centered(
                fb,
                w / 2,
                ty + 28 * s,
                "AB IN DEN KÜHLSCHRANK, BEVOR DU SCHMILZT!",
                0xFFFFFF,
                SHADOW,
                s,
            );
        }
    }

    if let Some(t) = d.dead_t {
        if t > 0.6 {
            fb.blend_rect(0, 0, w, h, 0x000000, ((t - 0.6) * 300.0).min(200.0) as u32);
        }
    }

    // --- Ziel ------------------------------------------------------------
    if let Some(f) = &d.finished {
        let a = ((d.finish_t - 1.0) * 200.0).clamp(0.0, 150.0) as u32;
        if a > 0 {
            fb.blend_rect(0, h * 3 / 10, w, h * 2 / 5, 0x200818, a);
            let cy = h * 3 / 10 + 10 * s;
            draw_text_centered(fb, w / 2, cy, "GESCHAFFT!", 0x7CFF7C, 0x103010, 3 * s);
            let (m, sec, ds) = clock(f.time);
            let l1 = fmt::<64>(format_args!("ZEIT {:02}:{:02}.{}   KIRSCHEN {}/{}", m, sec, ds, f.cherries, f.total));
            draw_text_centered(fb, w / 2, cy + 30 * s, l1.as_str(), 0xFFFFFF, SHADOW, s);
            let l2 = fmt::<64>(format_args!("PLATSCHER: {}   {}", f.deaths, rating(f)));
            draw_text_centered(fb, w / 2, cy + 42 * s, l2.as_str(), 0xFFE14D, SHADOW, s);
            draw_text_centered(fb, w / 2, cy + 60 * s, "ENTER: NOCHMAL    ESC: ENDE", 0xC0E0FF, SHADOW, s);
        }
    }

    if d.paused {
        fb.blend_rect(0, 0, w, h, 0x000000, 120);
        draw_text_centered(fb, w / 2, h / 2 - 20 * s, "PAUSE", 0xFFFFFF, SHADOW, 3 * s);
        draw_text_centered(fb, w / 2, h / 2 + 10 * s, "P: WEITER   R: CHECKPOINT   ESC: ENDE", 0xFFF2A0, SHADOW, s);
    }
}

fn rating(f: &FinishStats) -> &'static str {
    let all = f.cherries == f.total;
    match (all, f.deaths) {
        (true, 0) => "PERFEKTER PUDDING!",
        (true, _) => "KIRSCHEN-KÖNIG!",
        (false, 0) => "OHNE PLATSCHER!",
        _ => "LECKER!",
    }
}
