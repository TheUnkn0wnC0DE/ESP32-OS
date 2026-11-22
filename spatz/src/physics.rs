//! Kollision für Plattformer: statische/kinematische Formen (Boxen,
//! senkrechte Zylinder) und ein robuster Charakter-Controller.
//!
//! Der Spieler ist ein senkrechter Zylinder. Überlappungen werden pro
//! Teilschritt aufgelöst: kam der Körper (relativ zum Collider) von oben,
//! landet er; von unten stößt er sich den Kopf; sonst wird er seitlich
//! herausgeschoben. Kleine Kanten werden automatisch erstiegen, bewegte
//! Plattformen tragen den Körper mit.

use crate::math::{Aabb, Vec2, Vec3};
use alloc::vec::Vec;

/// Kollisionsform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Achsenparallele Box.
    Box(Aabb),
    /// Senkrechter Zylinder (Mittelpunkt in XZ, Höhenbereich).
    Cylinder { center: Vec2, radius: f32, y_min: f32, y_max: f32 },
}

impl Shape {
    #[inline]
    pub fn top(&self) -> f32 {
        match self {
            Shape::Box(b) => b.max.y,
            Shape::Cylinder { y_max, .. } => *y_max,
        }
    }
    #[inline]
    pub fn bottom(&self) -> f32 {
        match self {
            Shape::Box(b) => b.min.y,
            Shape::Cylinder { y_min, .. } => *y_min,
        }
    }
    pub fn aabb(&self) -> Aabb {
        match *self {
            Shape::Box(b) => b,
            Shape::Cylinder { center, radius, y_min, y_max } => Aabb::new(
                Vec3::new(center.x - radius, y_min, center.y - radius),
                Vec3::new(center.x + radius, y_max, center.y + radius),
            ),
        }
    }
    pub fn translate(&mut self, d: Vec3) {
        match self {
            Shape::Box(b) => {
                b.min += d;
                b.max += d;
            }
            Shape::Cylinder { center, y_min, y_max, .. } => {
                *center += d.xz();
                *y_min += d.y;
                *y_max += d.y;
            }
        }
    }

    /// Horizontale Durchdringung eines Kreises (Mitte `p`, Radius `r`) in
    /// der XZ-Ebene: Schieberichtung und -tiefe.
    pub fn push_xz(&self, p: Vec2, r: f32) -> Option<(Vec2, f32)> {
        match *self {
            Shape::Box(b) => {
                let cx = p.x.clamp(b.min.x, b.max.x);
                let cz = p.y.clamp(b.min.z, b.max.z);
                let d = Vec2::new(p.x - cx, p.y - cz);
                let d2 = d.length_sq();
                if d2 >= r * r {
                    return None;
                }
                if d2 > 1e-12 {
                    let l = crate::math::sqrt(d2);
                    Some((d * (1.0 / l), r - l))
                } else {
                    // Mittelpunkt in der Box: zur nächsten Kante.
                    let c = [
                        (p.x - b.min.x, Vec2::new(-1.0, 0.0)),
                        (b.max.x - p.x, Vec2::new(1.0, 0.0)),
                        (p.y - b.min.z, Vec2::new(0.0, -1.0)),
                        (b.max.z - p.y, Vec2::new(0.0, 1.0)),
                    ];
                    let mut best = c[0];
                    for e in &c[1..] {
                        if e.0 < best.0 {
                            best = *e;
                        }
                    }
                    Some((best.1, best.0 + r))
                }
            }
            Shape::Cylinder { center, radius, .. } => {
                let d = p - center;
                let rr = r + radius;
                let d2 = d.length_sq();
                if d2 >= rr * rr {
                    return None;
                }
                let l = crate::math::sqrt(d2);
                let n = if l > 1e-6 { d * (1.0 / l) } else { Vec2::new(1.0, 0.0) };
                Some((n, rr - l))
            }
        }
    }
}

/// Ein Collider in der Welt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub shape: Shape,
    /// Bewegung im letzten Simulationsschritt (Plattformen tragen Mitfahrer).
    pub delta: Vec3,
    /// Frei verwendbare Kennung des Spiels.
    pub tag: u32,
    pub enabled: bool,
}

impl Collider {
    pub fn new(shape: Shape, tag: u32) -> Self {
        Self { shape, delta: Vec3::ZERO, tag, enabled: true }
    }

    /// Verschiebt den Collider und merkt sich die Bewegung.
    pub fn move_by(&mut self, d: Vec3) {
        self.shape.translate(d);
        self.delta = d;
    }
}

/// Alle Collider einer Szene (lineare Suche – bei einigen hundert Formen
/// schneller als jede Beschleunigungsstruktur).
#[derive(Default)]
pub struct CollisionWorld {
    pub colliders: Vec<Collider>,
}

impl CollisionWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, c: Collider) -> usize {
        self.colliders.push(c);
        self.colliders.len() - 1
    }

    /// Höchste Oberkante unter `p` (Punkt-Test in XZ), max. `max_drop` tiefer.
    pub fn ground_below(&self, p: Vec3, max_drop: f32) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, c) in self.colliders.iter().enumerate() {
            if !c.enabled {
                continue;
            }
            let top = c.shape.top();
            if top > p.y + 0.01 || top < p.y - max_drop {
                continue;
            }
            if c.shape.push_xz(p.xz(), 1e-4).is_some() && best.is_none_or(|b| top > b.0) {
                best = Some((top, i));
            }
        }
        best
    }
}

/// Ergebnis eines Bewegungsschritts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveResult {
    /// In diesem Schritt (neu) gelandet.
    pub landed: bool,
    /// Aufprallgeschwindigkeit beim Landen (positiv).
    pub landing_speed: f32,
    pub hit_ceiling: bool,
    pub hit_wall: bool,
}

/// Senkrechter Zylinder als Spielfigur. `pos` = Mitte der Fußfläche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterBody {
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f32,
    pub height: f32,
    /// Maximale Kantenhöhe, die automatisch erstiegen wird.
    pub step_height: f32,
    pub on_ground: bool,
    /// Index des Colliders, auf dem die Figur steht.
    pub ground: Option<usize>,
}

const EPS: f32 = 0.02;

impl CharacterBody {
    pub fn new(pos: Vec3, radius: f32, height: f32) -> Self {
        Self { pos, vel: Vec3::ZERO, radius, height, step_height: 0.3, on_ground: false, ground: None }
    }

    /// Bewegt die Figur um `vel · dt` und löst Kollisionen auf.
    pub fn step(&mut self, world: &CollisionWorld, dt: f32) -> MoveResult {
        let mut res = MoveResult::default();
        let was_grounded = self.on_ground;
        // Mitfahren auf bewegten Plattformen.
        if let (true, Some(g)) = (self.on_ground, self.ground) {
            if let Some(c) = world.colliders.get(g) {
                if c.enabled {
                    self.pos += c.delta;
                }
            }
        }
        self.on_ground = false;
        self.ground = None;

        let disp = self.vel * dt;
        let max_sub = (self.radius * 0.5).max(0.05);
        let n = ((disp.length() / max_sub) as u32 + 1).min(16);
        let sub = disp * (1.0 / n as f32);
        for _ in 0..n {
            let prev = self.pos;
            self.pos += sub;
            self.resolve(world, prev, was_grounded, &mut res);
        }

        // Bodenkontakt halten (Plattformen abwärts, kleine Stufen nach unten).
        if !self.on_ground && self.vel.y <= 0.0 {
            let probe = if was_grounded { self.step_height } else { 0.04 };
            if let Some((top, i)) = self.ground_probe(world, probe) {
                self.pos.y = top;
                self.vel.y = 0.0;
                self.on_ground = true;
                self.ground = Some(i);
            }
        }
        if self.on_ground && !was_grounded {
            res.landed = true;
        }
        res
    }

    fn resolve(&mut self, world: &CollisionWorld, prev: Vec3, was_grounded: bool, res: &mut MoveResult) {
        for _ in 0..4 {
            let mut any = false;
            for (i, c) in world.colliders.iter().enumerate() {
                if !c.enabled {
                    continue;
                }
                let (cmin, cmax) = (c.shape.bottom(), c.shape.top());
                let bot = self.pos.y;
                let top = bot + self.height;
                if bot >= cmax - 1e-5 || top <= cmin + 1e-5 {
                    continue;
                }
                let Some((n, depth)) = c.shape.push_xz(self.pos.xz(), self.radius) else {
                    continue;
                };
                any = true;
                // Position relativ zum Collider vor dessen Bewegung.
                let prev_cmax = cmax - c.delta.y;
                let prev_cmin = cmin - c.delta.y;
                if prev.y >= prev_cmax - EPS {
                    // Von oben: landen.
                    self.pos.y = cmax;
                    if self.vel.y < 0.0 {
                        res.landing_speed = res.landing_speed.max(-self.vel.y);
                        self.vel.y = 0.0;
                    }
                    self.on_ground = true;
                    self.ground = Some(i);
                } else if prev.y + self.height <= prev_cmin + EPS && self.vel.y >= 0.0 {
                    // Von unten: Kopf stoßen.
                    self.pos.y = cmin - self.height;
                    if self.vel.y > 0.0 {
                        self.vel.y = 0.0;
                    }
                    res.hit_ceiling = true;
                } else if (was_grounded || self.on_ground) && cmax - bot <= self.step_height {
                    // Kleine Kante: hochsteigen.
                    self.pos.y = cmax;
                    self.on_ground = true;
                    self.ground = Some(i);
                } else {
                    // Seitlich herausschieben, Geschwindigkeit in die Wand entfernen.
                    let p = self.pos.xz() + n * (depth + 1e-4);
                    self.pos.x = p.x;
                    self.pos.z = p.y;
                    let v = Vec2::new(self.vel.x, self.vel.z);
                    let vn = v.dot(n);
                    if vn < 0.0 {
                        let v2 = v - n * vn;
                        self.vel.x = v2.x;
                        self.vel.z = v2.y;
                    }
                    res.hit_wall = true;
                }
            }
            if !any {
                break;
            }
        }
    }

    /// Sucht eine Oberkante knapp unter den Füßen.
    fn ground_probe(&self, world: &CollisionWorld, probe: f32) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        for (i, c) in world.colliders.iter().enumerate() {
            if !c.enabled {
                continue;
            }
            let top = c.shape.top();
            if top > self.pos.y + 1e-3 || top < self.pos.y - probe {
                continue;
            }
            if c.shape.push_xz(self.pos.xz(), self.radius).is_some() && best.is_none_or(|b| top > b.0) {
                best = Some((top, i));
            }
        }
        best
    }

    /// Prüft, ob die Figur einen Punkt (mit Radius) berührt.
    pub fn touches(&self, p: Vec3, r: f32) -> bool {
        let d = self.pos.xz() - p.xz();
        let rr = self.radius + r;
        d.length_sq() < rr * rr && p.y + r > self.pos.y && p.y - r < self.pos.y + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor_world() -> CollisionWorld {
        let mut w = CollisionWorld::new();
        w.add(Collider::new(Shape::Box(Aabb::new(Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 0.0, 10.0))), 0));
        w.add(Collider::new(Shape::Box(Aabb::new(Vec3::new(2.0, 0.0, -1.0), Vec3::new(3.0, 3.0, 1.0))), 1));
        w
    }

    #[test]
    fn falls_and_lands_on_floor() {
        let w = floor_world();
        let mut b = CharacterBody::new(Vec3::new(0.0, 3.0, 0.0), 0.4, 0.9);
        let mut landed = false;
        for _ in 0..240 {
            b.vel.y -= 30.0 / 120.0;
            landed |= b.step(&w, 1.0 / 120.0).landed;
        }
        assert!(landed && b.on_ground);
        assert!(b.pos.y.abs() < 1e-3, "y = {}", b.pos.y);
    }

    #[test]
    fn wall_blocks_horizontal_motion() {
        let w = floor_world();
        let mut b = CharacterBody::new(Vec3::new(0.0, 0.0, 0.0), 0.4, 0.9);
        b.on_ground = true;
        b.ground = Some(0);
        for _ in 0..240 {
            b.vel = Vec3::new(6.0, -1.0, 0.0);
            b.step(&w, 1.0 / 120.0);
        }
        assert!(b.pos.x <= 2.0 - 0.4 + 1e-3, "durch die Wand: x = {}", b.pos.x);
        assert!(b.on_ground);
    }

    #[test]
    fn moving_platform_carries_rider() {
        let mut w = CollisionWorld::new();
        let p = w.add(Collider::new(
            Shape::Cylinder { center: Vec2::new(0.0, 0.0), radius: 2.0, y_min: -0.5, y_max: 0.0 },
            0,
        ));
        let mut b = CharacterBody::new(Vec3::new(0.0, 0.0, 0.0), 0.4, 0.9);
        b.on_ground = true;
        b.ground = Some(p);
        for _ in 0..120 {
            w.colliders[p].move_by(Vec3::new(0.02, 0.01, 0.0));
            b.vel.y -= 0.25;
            b.step(&w, 1.0 / 120.0);
        }
        assert!((b.pos.x - 2.4).abs() < 1e-2, "x = {}", b.pos.x);
        assert!((b.pos.y - 1.2).abs() < 1e-2, "y = {}", b.pos.y);
        assert!(b.on_ground);
    }
}
