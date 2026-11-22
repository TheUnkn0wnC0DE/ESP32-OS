//! Spielfigur: Quader 0,6 × 1,8 × 0,6, Augen auf 1,62. Laufen, Springen
//! (mit automatischem Sprung auf einzelne Stufen – angenehmer per Touch),
//! Schwimmen, Fliegen. Kollision achsenweise in kleinen Schritten.

use crate::welt::{GX, GZ, Welt};
use spatz::math::{Vec3, ceil, cos, floor, sin, sqrt};

pub const HALB: f32 = 0.3;
pub const GROESSE: f32 = 1.8;
pub const AUGE: f32 = 1.62;
const SCHWERE: f32 = 28.0;
const SPRUNG: f32 = 8.6;
const GEHEN: f32 = 4.6;
const FLIEGEN: f32 = 9.0;

pub struct Spieler {
    /// Füße (Mitte der Grundfläche).
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub am_boden: bool,
    pub fliegt: bool,
    pub im_wasser: bool,
    /// Zurückgelegter Weg am Boden (für Schrittgeräusche).
    pub weg: f32,
}

/// Was der Spieler in diesem Schritt will.
#[derive(Clone, Copy, Debug, Default)]
pub struct Absicht {
    /// −1..1: vorwärts, rechts.
    pub vor: f32,
    pub rechts: f32,
    pub springen: bool,
    pub sinken: bool,
}

impl Spieler {
    pub fn new(pos: Vec3) -> Self {
        Self { pos, vel: Vec3::ZERO, yaw: 0.0, pitch: -0.15, am_boden: false, fliegt: false, im_wasser: false, weg: 0.0 }
    }

    pub fn auge(&self) -> Vec3 {
        self.pos + Vec3::new(0.0, AUGE, 0.0)
    }

    pub fn blick(&self) -> Vec3 {
        let cp = cos(self.pitch);
        Vec3::new(-sin(self.yaw) * cp, sin(self.pitch), -cos(self.yaw) * cp)
    }

    /// Überschneidet der Körper bei `p` einen festen Block?
    pub fn kollidiert(w: &Welt, p: Vec3) -> bool {
        let (x0, x1) = (floor(p.x - HALB) as i32, floor(p.x + HALB - 1e-4) as i32);
        let (y0, y1) = (floor(p.y) as i32, floor(p.y + GROESSE - 1e-4) as i32);
        let (z0, z1) = (floor(p.z - HALB) as i32, floor(p.z + HALB - 1e-4) as i32);
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if w.block(x, y, z).fest() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Würde ein Block bei (x, y, z) den Körper schneiden?
    pub fn beruehrt_block(&self, x: i32, y: i32, z: i32) -> bool {
        let p = self.pos;
        (x as f32) < p.x + HALB && (x + 1) as f32 > p.x - HALB && (y as f32) < p.y + GROESSE && (y + 1) as f32 > p.y && (z as f32) < p.z + HALB && (z + 1) as f32 > p.z - HALB
    }

    pub fn schritt(&mut self, w: &Welt, a: Absicht, dt: f32) {
        let dt = dt.min(0.05);
        let fuss = w.block(floor(self.pos.x) as i32, floor(self.pos.y + 0.4) as i32, floor(self.pos.z) as i32);
        self.im_wasser = fuss == crate::welt::Block::Wasser;
        // Richtung aus dem Blickwinkel (waagrecht).
        let vorne = Vec3::new(-sin(self.yaw), 0.0, -cos(self.yaw));
        let rechts = Vec3::new(cos(self.yaw), 0.0, -sin(self.yaw));
        let mut wunsch = vorne * a.vor + rechts * a.rechts;
        let l = sqrt(wunsch.x * wunsch.x + wunsch.z * wunsch.z);
        if l > 1.0 {
            wunsch = wunsch * (1.0 / l);
        }
        let tempo = if self.fliegt {
            FLIEGEN
        } else if self.im_wasser {
            GEHEN * 0.6
        } else {
            GEHEN
        };
        let griff = if self.am_boden || self.fliegt { 14.0 } else { 4.0 };
        let k = (griff * dt).min(1.0);
        self.vel.x += (wunsch.x * tempo - self.vel.x) * k;
        self.vel.z += (wunsch.z * tempo - self.vel.z) * k;
        if self.fliegt {
            let ziel = if a.springen { 6.0 } else if a.sinken { -6.0 } else { 0.0 };
            self.vel.y += (ziel - self.vel.y) * (10.0 * dt).min(1.0);
        } else if self.im_wasser {
            self.vel.y -= SCHWERE * 0.25 * dt;
            if a.springen {
                self.vel.y = self.vel.y.max(3.2);
            }
            self.vel.y = self.vel.y.max(-3.0);
        } else {
            self.vel.y -= SCHWERE * dt;
            if a.springen && self.am_boden {
                self.vel.y = SPRUNG;
            }
            self.vel.y = self.vel.y.max(-45.0);
        }
        // In kleinen Schritten bewegen (nicht durch Blöcke tunneln).
        let weg = self.vel * dt;
        let n = ceil(weg.length() / 0.35).max(1.0) as i32;
        let d = weg * (1.0 / n as f32);
        let vorher = self.pos;
        self.am_boden = false;
        for _ in 0..n {
            for achse in [0, 2, 1] {
                let mut p = self.pos;
                match achse {
                    0 => p.x += d.x,
                    1 => p.y += d.y,
                    _ => p.z += d.z,
                }
                if !Self::kollidiert(w, p) {
                    self.pos = p;
                    continue;
                }
                match achse {
                    1 => {
                        // Nur die Geschwindigkeit in Stoßrichtung nehmen (ein
                        // eben ausgelöster Stufensprung bleibt erhalten).
                        if d.y < 0.0 {
                            self.am_boden = true;
                            self.pos.y = floor(self.pos.y + 1e-3);
                            self.vel.y = self.vel.y.max(0.0);
                        } else {
                            self.vel.y = self.vel.y.min(0.0);
                        }
                    }
                    _ => {
                        // Stufe von einem Block: automatisch hinaufspringen.
                        let stufe = p + Vec3::new(0.0, 1.05, 0.0);
                        if !self.fliegt && self.vel.y <= 0.0 && !Self::kollidiert(w, stufe) && self.boden_unter(w) {
                            self.vel.y = SPRUNG;
                        }
                        if achse == 0 {
                            self.vel.x = 0.0;
                        } else {
                            self.vel.z = 0.0;
                        }
                    }
                }
            }
        }
        // Am Inselrand bleiben (draußen gibt es keine Blöcke mehr).
        self.pos.x = self.pos.x.clamp(HALB + 0.01, GX as f32 - HALB - 0.01);
        self.pos.z = self.pos.z.clamp(HALB + 0.01, GZ as f32 - HALB - 0.01);
        if self.pos.y < -5.0 {
            self.pos.y = (w.oberflaeche(self.pos.x as i32, self.pos.z as i32) + 1) as f32;
            self.vel = Vec3::ZERO;
        }
        if self.am_boden && self.fliegt {
            self.fliegt = false;
        }
        if self.am_boden {
            let dx = self.pos.x - vorher.x;
            let dz = self.pos.z - vorher.z;
            self.weg += sqrt(dx * dx + dz * dz);
        }
    }

    fn boden_unter(&self, w: &Welt) -> bool {
        self.am_boden || Self::kollidiert(w, self.pos - Vec3::new(0.0, 0.05, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::welt::Block;

    fn welt_mit_boden() -> (Welt, Vec3) {
        let w = Welt::erzeugen(11);
        let y = w.oberflaeche(64, 64);
        (w, Vec3::new(64.5, (y + 4) as f32, 64.5))
    }

    #[test]
    fn faellt_auf_den_boden_und_bleibt_stehen() {
        let (w, p) = welt_mit_boden();
        let mut s = Spieler::new(p);
        for _ in 0..120 {
            s.schritt(&w, Absicht::default(), 1.0 / 60.0);
        }
        assert!(s.am_boden);
        assert_eq!(s.pos.y, (w.oberflaeche(64, 64) + 1) as f32);
    }

    #[test]
    fn wand_haelt_auf_und_stufe_wird_erklommen() {
        let mut w = Welt::erzeugen(11);
        let y = w.oberflaeche(64, 64);
        // Freie Fläche schaffen, dann eine Mauer (2 hoch) und eine Stufe (1 hoch).
        for x in 60..70 {
            for z in 60..70 {
                for yy in y + 1..y + 5 {
                    w.setzen(x, yy, z, Block::Luft);
                }
                w.setzen(x, y, z, Block::Stein);
            }
        }
        for z in 60..70 {
            w.setzen(66, y + 1, z, Block::Stein);
            w.setzen(66, y + 2, z, Block::Stein);
        }
        let mut s = Spieler::new(Vec3::new(64.5, (y + 1) as f32, 64.5));
        s.yaw = -core::f32::consts::FRAC_PI_2; // Blick nach +x
        for _ in 0..120 {
            s.schritt(&w, Absicht { vor: 1.0, ..Absicht::default() }, 1.0 / 60.0);
        }
        assert!(s.pos.x < 66.0 - HALB + 0.01, "Mauer hält auf ({})", s.pos.x);
        // Obere Reihe weg: nur noch eine Stufe → automatisch hinauf.
        for z in 60..70 {
            w.setzen(66, y + 2, z, Block::Luft);
        }
        for _ in 0..120 {
            s.schritt(&w, Absicht { vor: 1.0, ..Absicht::default() }, 1.0 / 60.0);
        }
        assert!(s.pos.x > 66.5, "Stufe erklommen ({})", s.pos.x);
    }

    #[test]
    fn springen_hebt_ab() {
        let (w, p) = welt_mit_boden();
        let mut s = Spieler::new(p);
        for _ in 0..120 {
            s.schritt(&w, Absicht::default(), 1.0 / 60.0);
        }
        let y0 = s.pos.y;
        s.schritt(&w, Absicht { springen: true, ..Absicht::default() }, 1.0 / 60.0);
        for _ in 0..10 {
            s.schritt(&w, Absicht::default(), 1.0 / 60.0);
        }
        assert!(s.pos.y > y0 + 0.8, "Sprung ({} → {})", y0, s.pos.y);
    }
}
