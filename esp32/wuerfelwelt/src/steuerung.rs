//! Touch-Steuerung (640×360): links schwebender Steuerknüppel (laufen),
//! rechts wischen = umsehen; Knöpfe rechts unten im Bogen: Springen
//! (doppelt tippen = Fliegen an/aus), Abbauen (halten), Setzen (tippen);
//! beim Fliegen zusätzlich Sinken. Unten die Blockleiste zum Antippen.

use pudding_core::gfx::{Canvas, hex};
use pudding_core::input::{Input, MAX_TOUCH};
use spatz::math::sqrt;

fn laenge(x: f32, y: f32) -> f32 {
    sqrt(x * x + y * y)
}

const MITTE_X: f32 = 320.0;
const STICK_R: f32 = 50.0;
/// Radiant je Bildpunkt Wischen.
const BLICK_JE_PX: f32 = 0.0058;
pub const SLOTS: usize = 9;
const SLOT: i32 = 34;
const SLOT_ABSTAND: i32 = 2;
pub const LEISTE_Y: i32 = 318;

pub fn leiste_x0() -> i32 {
    320 - (SLOTS as i32 * (SLOT + SLOT_ABSTAND) - SLOT_ABSTAND) / 2
}

/// Rechteck des Felds `i` der Blockleiste.
pub fn slot_rechteck(i: usize) -> (i32, i32, i32, i32) {
    (leiste_x0() + i as i32 * (SLOT + SLOT_ABSTAND), LEISTE_Y, SLOT, SLOT)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knopf {
    Springen,
    Abbauen,
    Setzen,
    Sinken,
}

struct KnopfDef {
    k: Knopf,
    x: f32,
    y: f32,
    r: f32,
}

const KNOEPFE: [KnopfDef; 4] = [
    KnopfDef { k: Knopf::Springen, x: 590.0, y: 300.0, r: 33.0 },
    KnopfDef { k: Knopf::Abbauen, x: 520.0, y: 262.0, r: 28.0 },
    KnopfDef { k: Knopf::Setzen, x: 590.0, y: 214.0, r: 28.0 },
    KnopfDef { k: Knopf::Sinken, x: 520.0, y: 182.0, r: 24.0 },
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Rolle {
    Stick { x0: f32, y0: f32 },
    Blick,
    Knopf(Knopf),
    Leiste,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    x: f32,
    y: f32,
    rolle: Rolle,
}

/// Was die Steuerung in diesem Bild will.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Eingabe {
    pub vor: f32,
    pub rechts: f32,
    /// Drehung (Radiant): links/rechts, hoch/runter.
    pub yaw: f32,
    pub pitch: f32,
    pub springen: bool,
    pub sinken: bool,
    pub abbauen: bool,
    pub setzen: bool,
    pub fliegen_umschalten: bool,
    pub slot: Option<usize>,
}

pub struct Steuerung {
    finger: [Option<Finger>; MAX_TOUCH],
    stick: (f32, f32),
    /// Zeit seit dem letzten Druck auf Springen (Doppeltipp = Fliegen).
    seit_sprung: f32,
    pub fliegt: bool,
}

impl Default for Steuerung {
    fn default() -> Self {
        Self::new()
    }
}

impl Steuerung {
    pub fn new() -> Self {
        Self { finger: [None; MAX_TOUCH], stick: (0.0, 0.0), seit_sprung: 9.0, fliegt: false }
    }

    fn treffer(&self, x: f32, y: f32) -> Option<Rolle> {
        for d in &KNOEPFE {
            if d.k == Knopf::Sinken && !self.fliegt {
                continue;
            }
            if laenge(x - d.x, y - d.y) <= d.r + 8.0 {
                return Some(Rolle::Knopf(d.k));
            }
        }
        let (x0, y0) = (leiste_x0() as f32, LEISTE_Y as f32);
        let breite = (SLOTS as i32 * (SLOT + SLOT_ABSTAND)) as f32;
        if x >= x0 - 4.0 && x < x0 + breite + 4.0 && y >= y0 - 6.0 {
            return Some(Rolle::Leiste);
        }
        None
    }

    fn slot_bei(x: f32) -> usize {
        (((x - leiste_x0() as f32) / (SLOT + SLOT_ABSTAND) as f32).max(0.0) as usize).min(SLOTS - 1)
    }

    pub fn update(&mut self, input: &Input, dt: f32) -> Eingabe {
        let mut e = Eingabe::default();
        self.seit_sprung += dt;
        let punkte: alloc::vec::Vec<(f32, f32)> = input.touch.iter().flatten().map(|&(x, y)| (x as f32, y as f32)).collect();
        let alt = self.finger;
        let mut neu: [Option<Finger>; MAX_TOUCH] = [None; MAX_TOUCH];
        let mut benutzt = [false; MAX_TOUCH];
        let mut paare: alloc::vec::Vec<(f32, usize, usize)> = alloc::vec::Vec::new();
        for (i, p) in punkte.iter().enumerate() {
            for (j, f) in alt.iter().enumerate() {
                if let Some(f) = f {
                    let d = laenge(p.0 - f.x, p.1 - f.y);
                    if d < 160.0 {
                        paare.push((d, i, j));
                    }
                }
            }
        }
        paare.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut zu: [Option<usize>; MAX_TOUCH] = [None; MAX_TOUCH];
        for (_, i, j) in paare {
            if zu[i].is_none() && !benutzt[j] {
                zu[i] = Some(j);
                benutzt[j] = true;
            }
        }
        for (i, &(x, y)) in punkte.iter().enumerate().take(MAX_TOUCH) {
            let f = match zu[i].and_then(|j| alt[j]) {
                Some(mut f) => {
                    let (dx, dy) = (x - f.x, y - f.y);
                    match f.rolle {
                        Rolle::Blick => {
                            e.yaw -= dx * BLICK_JE_PX;
                            e.pitch -= dy * BLICK_JE_PX;
                        }
                        Rolle::Stick { x0, y0 } => {
                            let (vx, vy) = (x - x0, y - y0);
                            let l = laenge(vx, vy);
                            if l > STICK_R {
                                f.rolle = Rolle::Stick { x0: x - vx / l * STICK_R, y0: y - vy / l * STICK_R };
                            }
                        }
                        _ => {}
                    }
                    f.x = x;
                    f.y = y;
                    f
                }
                None => {
                    let rolle = match self.treffer(x, y) {
                        Some(Rolle::Knopf(Knopf::Springen)) => {
                            if self.seit_sprung < 0.3 {
                                e.fliegen_umschalten = true;
                                self.fliegt = !self.fliegt;
                            }
                            self.seit_sprung = 0.0;
                            Rolle::Knopf(Knopf::Springen)
                        }
                        Some(Rolle::Knopf(Knopf::Setzen)) => {
                            e.setzen = true;
                            Rolle::Knopf(Knopf::Setzen)
                        }
                        Some(Rolle::Leiste) => {
                            e.slot = Some(Self::slot_bei(x));
                            Rolle::Leiste
                        }
                        Some(r) => r,
                        None if x < MITTE_X => Rolle::Stick { x0: x, y0: y },
                        None => Rolle::Blick,
                    };
                    Finger { x, y, rolle }
                }
            };
            neu[i] = Some(f);
        }
        self.finger = neu;
        self.stick = (0.0, 0.0);
        for f in self.finger.iter().flatten() {
            match f.rolle {
                Rolle::Stick { x0, y0 } => {
                    self.stick = (((f.x - x0) / STICK_R).clamp(-1.0, 1.0), ((f.y - y0) / STICK_R).clamp(-1.0, 1.0));
                }
                Rolle::Knopf(Knopf::Springen) => e.springen = true,
                Rolle::Knopf(Knopf::Sinken) => e.sinken = true,
                Rolle::Knopf(Knopf::Abbauen) => e.abbauen = true,
                _ => {}
            }
        }
        let tot = |v: f32| if v.abs() < 0.12 { 0.0 } else { v };
        e.vor = -tot(self.stick.1);
        e.rechts = tot(self.stick.0);
        e
    }

    fn gedrueckt(&self, k: Knopf) -> bool {
        self.finger.iter().flatten().any(|f| f.rolle == Rolle::Knopf(k))
    }

    /// Knöpfe und Knüppel zeichnen.
    pub fn zeichne(&self, c: &mut Canvas) {
        for d in &KNOEPFE {
            if d.k == Knopf::Sinken && !self.fliegt {
                continue;
            }
            let an = self.gedrueckt(d.k);
            let farbe = hex(match d.k {
                Knopf::Springen if self.fliegt => 0x9CE7FF,
                Knopf::Abbauen => 0xFFB070,
                _ => 0xFFFFFF,
            });
            c.mix_disc(d.x, d.y, d.r, if an { farbe } else { hex(0x0C0C12) }, if an { 18 } else { 18 });
            c.mix_ring(d.x, d.y, d.r, 2.0, farbe, if an { 32 } else { 22 });
            let f = if an { hex(0x101014) } else { farbe };
            symbol(c, d, f, self.fliegt);
        }
        match self.finger.iter().flatten().find_map(|f| match f.rolle {
            Rolle::Stick { x0, y0 } => Some((x0, y0, f.x, f.y)),
            _ => None,
        }) {
            Some((x0, y0, x, y)) => {
                c.mix_disc(x0, y0, STICK_R, hex(0x000000), 9);
                c.mix_ring(x0, y0, STICK_R, 2.0, hex(0xFFFFFF), 14);
                let (vx, vy) = (x - x0, y - y0);
                let l = laenge(vx, vy).max(1.0);
                let k = l.min(STICK_R) / l;
                c.mix_disc(x0 + vx * k, y0 + vy * k, 20.0, hex(0xFFFFFF), 14);
            }
            None => {
                c.mix_ring(110.0, 250.0, STICK_R, 2.0, hex(0xFFFFFF), 6);
                c.mix_disc(110.0, 250.0, 18.0, hex(0xFFFFFF), 5);
            }
        }
    }
}

fn symbol(c: &mut Canvas, d: &KnopfDef, f: u16, fliegt: bool) {
    let (x, y) = (d.x, d.y);
    let p = |pts: &[(f32, f32)]| -> alloc::vec::Vec<(f32, f32)> { pts.iter().map(|&(a, b)| (x + a, y + b)).collect() };
    match d.k {
        Knopf::Springen => {
            // Pfeil nach oben; beim Fliegen ein Doppelpfeil.
            c.fill_poly(&p(&[(0.0, -15.0), (12.0, -2.0), (5.0, -2.0), (5.0, 12.0), (-5.0, 12.0), (-5.0, -2.0), (-12.0, -2.0)]), f);
            if fliegt {
                c.fill_poly(&p(&[(-14.0, 6.0), (-8.0, 0.0), (-8.0, 12.0)]), f);
                c.fill_poly(&p(&[(14.0, 6.0), (8.0, 0.0), (8.0, 12.0)]), f);
            }
        }
        Knopf::Sinken => {
            c.fill_poly(&p(&[(0.0, 13.0), (11.0, 1.0), (5.0, 1.0), (5.0, -11.0), (-5.0, -11.0), (-5.0, 1.0), (-11.0, 1.0)]), f);
        }
        Knopf::Abbauen => {
            // Spitzhacke: Stiel und gebogener Kopf.
            c.fill_poly(&p(&[(-11.0, 12.0), (-8.0, 15.0), (6.0, 1.0), (3.0, -2.0)]), f);
            c.fill_poly(&p(&[(-6.0, -10.0), (2.0, -13.0), (10.0, -11.0), (14.0, -4.0), (13.0, 4.0), (9.0, -3.0), (3.0, -7.0), (-3.0, -7.0)]), f);
        }
        Knopf::Setzen => {
            // Würfel mit Plus.
            c.mix_poly(&p(&[(-10.0, -4.0), (0.0, -9.0), (10.0, -4.0), (0.0, 1.0)]), f, 32);
            c.mix_poly(&p(&[(-10.0, -4.0), (0.0, 1.0), (0.0, 12.0), (-10.0, 7.0)]), f, 20);
            c.mix_poly(&p(&[(10.0, -4.0), (0.0, 1.0), (0.0, 12.0), (10.0, 7.0)]), f, 12);
            c.fill_rect((x + 8.0) as i32, (y - 17.0) as i32, 10, 3, f);
            c.fill_rect((x + 11.5) as i32, (y - 20.5) as i32, 3, 10, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 55.0;

    #[test]
    fn knueppel_und_umsehen_gleichzeitig() {
        let mut s = Steuerung::new();
        let zwei = |a: (u16, u16), b: (u16, u16)| Input { held: 0, touch: [Some(a), Some(b)] };
        s.update(&zwei((120, 240), (420, 150)), DT);
        let e = s.update(&zwei((120, 190), (440, 140)), DT);
        assert!(e.vor > 0.9, "vorwärts");
        assert!(e.yaw < 0.0, "nach rechts gewischt: Drehung rechts (yaw kleiner)");
        assert!(e.pitch > 0.0, "nach oben gewischt: nach oben sehen");
    }

    #[test]
    fn setzen_einmal_abbauen_gehalten() {
        let mut s = Steuerung::new();
        assert!(s.update(&Input::touch_at(590, 214), DT).setzen);
        assert!(!s.update(&Input::touch_at(590, 214), DT).setzen, "nur beim Antippen");
        s.update(&Input::default(), DT);
        assert!(s.update(&Input::touch_at(520, 262), DT).abbauen);
        assert!(s.update(&Input::touch_at(520, 262), DT).abbauen);
        assert!(!s.update(&Input::default(), DT).abbauen);
    }

    #[test]
    fn doppeltipp_springen_schaltet_fliegen() {
        let mut s = Steuerung::new();
        s.update(&Input::touch_at(590, 300), DT);
        s.update(&Input::default(), DT);
        let e = s.update(&Input::touch_at(590, 300), DT);
        assert!(e.fliegen_umschalten && s.fliegt);
    }

    #[test]
    fn leiste_waehlt_feld() {
        let mut s = Steuerung::new();
        let (x, y, w, h) = slot_rechteck(4);
        let e = s.update(&Input::touch_at((x + w / 2) as u16, (y + h / 2) as u16), DT);
        assert_eq!(e.slot, Some(4));
    }
}
