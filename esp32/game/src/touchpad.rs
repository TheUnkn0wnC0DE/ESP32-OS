//! Touch-Steuerung im Spiel.
//!
//! * **Steuerknüppel** auf der einen Bildhälfte: schwebend (entsteht dort, wo
//!   der Daumen aufsetzt) oder fest an seinem Platz.
//! * **Springen** auf der anderen Hälfte: Trefferfläche ist die ganze Hälfte,
//!   der sichtbare Kreis nur die Markierung.
//! * **Pause** oben links (Feld [`PAUSE_W`]×[`PAUSE_H`]).
//! * Einstellbar: Größe, Deckkraft, fester/schwebender Knüppel,
//!   Links-/Rechtshänder (Seiten getauscht).
//! * Sichtbares Feedback statt Tastengefühl: Knüppel „ploppt“ beim Aufsetzen
//!   auf und leuchtet, der Sprungknopf füllt sich und sendet einen Ring aus.
//! * Die Elemente bleiben oberhalb der Tippzeile unten und werden **vor** dem
//!   HUD gezeichnet – Zahlen und Meldungen liegen immer obenauf.

use crate::gfx::{Canvas, hex, logical_h, logical_w};
use crate::input::{MAX_TOUCH, btn};
use crate::save::Settings;
use spatz::math::sqrt;

/// Pause-Feld oben links (logische Pixel).
pub const PAUSE_W: u16 = 56;
pub const PAUSE_H: u16 = 50;
/// Abstand der Elemente zum Bildrand.
const MARGIN: f32 = 14.0;
/// Darunter bleibt die Tippzeile des HUD frei.
pub const BOTTOM: f32 = 16.0;
/// Namen der Größenstufen (Einstellung `touch_size`).
pub const SIZE_NAMES: [&str; 3] = ["KLEIN", "MITTEL", "GROSS"];
const STICK_R: [f32; 3] = [24.0, 30.0, 38.0];
const JUMP_R: [f32; 3] = [20.0, 25.0, 31.0];
/// Deckkraft-Stufen 1..=5 → Mischanteil von 32.
const ALPHA: [u32; 5] = [6, 10, 14, 18, 22];
/// Dauer des Rings nach einem Sprung (s).
const PULSE: f32 = 0.25;
/// Dauer des Aufploppens des Knüppels (s).
const POP: f32 = 0.1;

/// Lage, Größe und Aussehen aus den Einstellungen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub stick_r: f32,
    pub jump_r: f32,
    /// Linkshänder: Knüppel rechts, Springen links.
    pub left_handed: bool,
    /// Fester statt schwebender Knüppel.
    pub fixed: bool,
    /// Mischanteil 0..=32.
    pub alpha: u32,
}

impl Layout {
    pub fn new(s: &Settings) -> Self {
        let size = (s.touch_size as usize).min(2);
        Self {
            stick_r: STICK_R[size],
            jump_r: JUMP_R[size],
            left_handed: s.left_handed,
            fixed: s.stick_fixed,
            alpha: ALPHA[(s.touch_alpha.clamp(1, 5) - 1) as usize],
        }
    }

    /// Spiegelt x für Linkshänder.
    fn side(&self, x: f32) -> f32 {
        if self.left_handed { logical_w() as f32 - x } else { x }
    }

    /// Liegt x auf der Seite des Knüppels?
    pub fn on_stick_side(&self, x: f32) -> bool {
        (x < logical_w() as f32 * 0.5) != self.left_handed
    }

    /// Platz des festen Knüppels (und des schwebenden in Ruhe).
    pub fn stick_home(&self) -> (f32, f32) {
        (self.side(MARGIN + self.stick_r), logical_h() as f32 - BOTTOM - self.stick_r)
    }

    /// Mitte des Sprungknopfs.
    pub fn jump_center(&self) -> (f32, f32) {
        (self.side(logical_w() as f32 - MARGIN - self.jump_r), logical_h() as f32 - BOTTOM - self.jump_r)
    }

    /// Mitte des schwebenden Knüppels: unter dem Daumen, aber ganz sichtbar,
    /// auf seiner Bildhälfte, in der unteren Hälfte und über der Tippzeile.
    fn origin_for(&self, x: f32, y: f32) -> (f32, f32) {
        let r = self.stick_r;
        let lx = self.side(x).clamp(r + 4.0, logical_w() as f32 * 0.5 - r);
        let ly = y.clamp(logical_h() as f32 * 0.5, logical_h() as f32 - BOTTOM - r);
        (self.side(lx), ly)
    }

    /// Größter Bereich (x0, y0, x1, y1), den die sichtbaren Elemente je
    /// einnehmen können (Knüppel an jeder möglichen Stelle, Sprungknopf).
    pub fn extents(&self) -> [(f32, f32, f32, f32); 2] {
        let r = self.stick_r;
        let half = logical_w() as f32 * 0.5;
        let stick = if self.left_handed {
            (half, logical_h() as f32 * 0.5 - r, logical_w() as f32 - 4.0, logical_h() as f32 - BOTTOM)
        } else {
            (4.0, logical_h() as f32 * 0.5 - r, half, logical_h() as f32 - BOTTOM)
        };
        let (jx, jy) = self.jump_center();
        let jr = self.jump_r;
        [stick, (jx - jr, jy - jr, jx + jr, jy + jr)]
    }
}

/// Liegt der Punkt im Pause-Feld?
pub fn in_pause(x: u16, y: u16) -> bool {
    x < PAUSE_W && y < PAUSE_H
}

/// Zustand der Touch-Steuerung.
#[derive(Clone, Debug)]
pub struct TouchPad {
    /// Ausschlag des Knüppels (x rechts, y vorwärts), Länge ≤ 1.
    pub stick: (f32, f32),
    origin: Option<(f32, f32)>,
    /// Sprungknopf wird gerade gehalten.
    pub jump: bool,
    jump_age: f32,
    stick_age: f32,
}

impl Default for TouchPad {
    fn default() -> Self {
        Self { stick: (0.0, 0.0), origin: None, jump: false, jump_age: PULSE, stick_age: POP }
    }
}

impl TouchPad {
    /// Wertet die Finger aus (das Pause-Feld zählt nicht mit) und liefert die
    /// virtuellen Knöpfe (Springen).
    pub fn update(&mut self, l: &Layout, touches: &[Option<(u16, u16)>; MAX_TOUCH]) -> u8 {
        let mut stick = None;
        let mut jump = false;
        for &(x, y) in touches.iter().flatten() {
            if in_pause(x, y) {
                continue;
            }
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            if l.on_stick_side(fx) {
                stick = Some((fx, fy));
            } else {
                jump = true;
            }
        }
        if jump && !self.jump {
            self.jump_age = 0.0;
        }
        self.jump = jump;
        match stick {
            Some((x, y)) => {
                if self.origin.is_none() {
                    self.stick_age = 0.0;
                }
                let (ox, oy) = if l.fixed { l.stick_home() } else { *self.origin.get_or_insert(l.origin_for(x, y)) };
                self.origin = Some((ox, oy));
                let (mut dx, mut dy) = ((x - ox) / l.stick_r, (y - oy) / l.stick_r);
                let len = sqrt(dx * dx + dy * dy);
                if len > 1.0 {
                    dx /= len;
                    dy /= len;
                }
                // Bildschirm nach oben = vorwärts; kleine tote Zone.
                self.stick = if len < 0.15 { (0.0, 0.0) } else { (dx, -dy) };
            }
            None => {
                self.origin = None;
                self.stick = (0.0, 0.0);
            }
        }
        if jump { btn::JUMP } else { 0 }
    }

    /// Alle Finger weg.
    pub fn release(&mut self) {
        self.stick = (0.0, 0.0);
        self.origin = None;
        self.jump = false;
    }

    /// Feedback-Animationen weiterlaufen lassen.
    pub fn tick(&mut self, dt: f32) {
        self.jump_age = (self.jump_age + dt).min(PULSE);
        self.stick_age = (self.stick_age + dt).min(POP);
    }

    /// Zeichnet Knüppel, Sprungknopf und (falls `pause`) den Pause-Knopf.
    pub fn draw(&self, c: &mut Canvas, l: &Layout, pause: bool) {
        let a = l.alpha;
        let light = hex(0xFFE8F0);
        // Knüppel: Sockel ploppt beim Aufsetzen auf, Knauf folgt dem Daumen.
        let active = self.origin.is_some();
        let (ox, oy) = self.origin.unwrap_or(l.stick_home());
        let pop = if active { 0.8 + 0.2 * (self.stick_age / POP) } else { 1.0 };
        let r = l.stick_r * pop;
        c.mix_disc(ox, oy, r, hex(0x2A1020), a);
        c.mix_ring(ox, oy, r, 1.5, light, (a + if active { 16 } else { 8 }).min(32));
        let (kx, ky) = (ox + self.stick.0 * l.stick_r, oy - self.stick.1 * l.stick_r);
        let kr = l.stick_r * 0.38;
        c.mix_disc(kx, ky, kr, hex(0xFFFFFF), (a + if active { 12 } else { 4 }).min(32));

        // Sprungknopf: gefüllt solange gehalten, Ring beim Drücken.
        let (jx, jy) = l.jump_center();
        let fill = if self.jump { hex(0xFF5C8A) } else { hex(0x2A1020) };
        c.mix_disc(jx, jy, l.jump_r, fill, (a + if self.jump { 10 } else { 0 }).min(32));
        c.mix_ring(jx, jy, l.jump_r, 1.5, light, (a + 8).min(32));
        if self.jump_age < PULSE {
            // Heller Ring wächst von innen bis an den Rand und verblasst –
            // bleibt innerhalb des Knopfs.
            let k = self.jump_age / PULSE;
            c.mix_ring(jx, jy, l.jump_r * (0.45 + 0.55 * k), 2.5, hex(0xFFFFFF), ((1.0 - k) * 28.0) as u32);
        }
        c.text_centered(jx as i32, jy as i32 - 3, "SPRUNG", hex(0xFFFFFF), hex(0x301020), 1);

        // Pause-Knopf unter der FPS-Anzeige.
        if pause {
            let (px, py) = (4, 30);
            c.darken_rect(px, py, 18, 16);
            c.frame(px, py, 18, 16, light);
            c.fill_rect(px + 5, py + 4, 3, 8, hex(0xFFFFFF));
            c.fill_rect(px + 10, py + 4, 3, 8, hex(0xFFFFFF));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{H, W};

    fn layout(size: u8, left: bool, fixed: bool) -> Layout {
        let s = Settings { touch_size: size, left_handed: left, stick_fixed: fixed, ..Settings::default() };
        Layout::new(&s)
    }

    /// Die Steuerelemente überdecken in keiner Einstellung das HUD:
    /// Kirschen/Zeit/Hitze oben rechts, FPS-Anzeige und Pause oben links,
    /// Tippzeile unten.
    #[test]
    fn controls_never_cover_the_hud() {
        let hud = [(228.0, 0.0, 320.0, 26.0), (0.0, 0.0, 60.0, 48.0)];
        for size in 0..3 {
            for left in [false, true] {
                for fixed in [false, true] {
                    let l = layout(size, left, fixed);
                    for (x0, y0, x1, y1) in l.extents() {
                        assert!(y1 <= H as f32 - BOTTOM + 0.01, "Tippzeile frei");
                        assert!(x0 >= 0.0 && x1 <= W as f32);
                        for &(hx0, hy0, hx1, hy1) in &hud {
                            let overlap = x0 < hx1 && hx0 < x1 && y0 < hy1 && hy0 < y1;
                            assert!(!overlap, "size={size} left={left}: ({x0},{y0},{x1},{y1}) überdeckt HUD");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn floating_fixed_and_left_handed() {
        let none = [None, None];
        // Schwebend: Knüppel entsteht unter dem Daumen.
        let l = layout(1, false, false);
        let mut p = TouchPad::default();
        assert_eq!(p.update(&l, &[Some((80, 120)), None]), 0);
        assert_eq!(p.origin, Some((80.5, 120.5)));
        p.update(&l, &[Some((80, 90)), None]);
        assert!(p.stick.1 > 0.9, "nach oben = vorwärts");
        // Rechte Hälfte: Springen, auch weit weg vom sichtbaren Kreis.
        assert_eq!(p.update(&l, &[Some((80, 90)), Some((200, 70))]), btn::JUMP);
        assert!(p.jump);
        p.update(&l, &none);
        assert!(p.origin.is_none() && !p.jump);
        // Pause-Feld zählt nicht.
        assert_eq!(p.update(&l, &[Some((10, 20)), None]), 0);
        assert!(p.origin.is_none());
        // Fest: Ausschlag relativ zum festen Platz.
        let l = layout(1, false, true);
        let (hx, hy) = l.stick_home();
        p.update(&l, &[Some(((hx + 20.0) as u16, hy as u16)), None]);
        assert!(p.stick.0 > 0.5);
        // Linkshänder: Knüppel rechts, Springen links.
        let l = layout(1, true, false);
        let mut p = TouchPad::default();
        assert_eq!(p.update(&l, &[Some((80, 120)), None]), btn::JUMP);
        assert_eq!(p.update(&l, &[Some((250, 120)), None]), 0);
        assert!(p.origin.is_some());
        assert!(l.jump_center().0 < 160.0 && l.stick_home().0 > 160.0);
    }
}
