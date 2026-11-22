//! Autopilot: spielt das Level anhand der `route`-Schritte aus der
//! Leveldatei. Benutzt exakt dieselben Steuerbefehle wie ein Mensch
//! (kameraabhängig) – damit ist er zugleich Integrationstest und
//! realistischer Benchmark.

use crate::level::RouteStep;
use crate::player::{Controls, Player};
use crate::world::World;
use spatz::math::{Vec2, abs, cos, sin};

pub struct Autopilot {
    pub step: usize,
    timer: f32,
    jumped: bool,
    left_ground: bool,
    pub finished_route: bool,
}

impl Default for Autopilot {
    fn default() -> Self {
        Self::new()
    }
}

impl Autopilot {
    pub fn new() -> Self {
        Self { step: 0, timer: 0.0, jumped: false, left_ground: false, finished_route: false }
    }

    fn next(&mut self) {
        self.step += 1;
        self.timer = 0.0;
        self.jumped = false;
        self.left_ground = false;
    }

    /// Nach einem Respawn: am Routenschritt weitermachen, der dem
    /// Rücksetzpunkt am nächsten liegt.
    pub fn resync(&mut self, world: &World, pos: spatz::math::Vec3) {
        let mut best = (f32::MAX, 0usize);
        for (i, s) in world.route.iter().enumerate() {
            if let RouteStep::Walk(p) | RouteStep::Jump(p) | RouteStep::Air(p) = s {
                let d = (*p - pos.xz()).length();
                if d < best.0 {
                    best = (d, i);
                }
            }
        }
        self.step = best.1;
        self.timer = 0.0;
        self.jumped = false;
        self.left_ground = false;
    }

    /// Erzeugt die Steuerbefehle für einen Simulationsschritt.
    pub fn controls(&mut self, world: &World, p: &Player, cam_yaw: f32, dt: f32) -> Controls {
        let mut c = Controls::default();
        let Some(&step) = world.route.get(self.step) else {
            self.finished_route = true;
            return c;
        };
        self.timer += dt;
        let pos = p.body.pos.xz();
        let steer = |c: &mut Controls, to: Vec2, slow_radius: f32| {
            let d = to - pos;
            let dist = d.length();
            let k = if slow_radius > 0.0 { (dist / slow_radius).min(1.0) } else { 1.0 };
            let w = d.normalize_or_zero() * k;
            let fwd = Vec2::new(-sin(cam_yaw), -cos(cam_yaw));
            let right = Vec2::new(cos(cam_yaw), -sin(cam_yaw));
            c.move_x = w.dot(right);
            c.move_y = w.dot(fwd);
            dist
        };
        match step {
            RouteStep::Walk(to) => {
                if steer(&mut c, to, 0.0) < 0.35 || self.timer > 8.0 {
                    self.next();
                }
            }
            RouteStep::Jump(to) => {
                steer(&mut c, to, 1.2);
                if !self.jumped {
                    c.jump_pressed = true;
                    self.jumped = true;
                }
                c.jump_held = true;
                if !p.body.on_ground {
                    self.left_ground = true;
                }
                let bounced = self.left_ground && p.body.vel.y > crate::player::JUMP_VEL + 2.0;
                if (self.left_ground && p.body.on_ground && self.timer > 0.1) || bounced {
                    self.next();
                } else if !self.left_ground && self.timer > 0.25 {
                    // Sprung kam nicht zustande (z. B. noch in der Luft) → erneut.
                    self.jumped = false;
                    self.timer = 0.0;
                } else if self.timer > 6.0 {
                    self.next();
                }
            }
            RouteStep::Air(to) => {
                steer(&mut c, to, 1.0);
                c.jump_held = true;
                if !p.body.on_ground {
                    self.left_ground = true;
                }
                if (self.left_ground && p.body.on_ground) || self.timer > 6.0 {
                    self.next();
                }
            }
            RouteStep::Wait(t) => {
                if self.timer >= t {
                    self.next();
                }
            }
            RouteStep::WaitPlat { idx, s, tol } => {
                if abs(world.moving_fraction(idx) - s) <= tol || self.timer > 12.0 {
                    self.next();
                }
            }
        }
        c
    }
}
