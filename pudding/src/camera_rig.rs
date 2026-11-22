//! Verfolgerkamera: weich nachgeführt, leichte Vorausschau, folgt der
//! Bewegungsrichtung automatisch (abschaltbar durch manuelles Drehen).

use crate::player::Player;
use spatz::math::{Vec2, angle_diff, atan2, cos, damp_factor, sin, smoothstep};
use spatz::prelude::*;

pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: Vec3,
    pub pos: Vec3,
    prev_pos: Vec3,
    prev_target: Vec3,
    /// Restzeit der Einführungsfahrt (Kamera schwenkt von vorn nach hinten).
    pub intro: f32,
    manual_timer: f32,
}

pub const INTRO_TIME: f32 = 2.6;

impl CameraRig {
    pub fn new(player_pos: Vec3, yaw: f32) -> Self {
        let mut r = Self {
            yaw,
            pitch: -0.34,
            dist: 6.6,
            target: player_pos,
            pos: player_pos,
            prev_pos: player_pos,
            prev_target: player_pos,
            intro: 0.0,
            manual_timer: 0.0,
        };
        r.snap(player_pos, yaw);
        r
    }

    fn offset(&self, extra_yaw: f32) -> Vec3 {
        let y = self.yaw + extra_yaw;
        let cp = cos(self.pitch);
        Vec3::new(-sin(y) * cp, sin(self.pitch), -cos(y) * cp) * self.dist
    }

    /// Springt ohne Glättung hinter die Figur.
    pub fn snap(&mut self, player_pos: Vec3, yaw: f32) {
        self.yaw = yaw;
        self.target = player_pos + Vec3::new(0.0, 1.1, 0.0);
        self.pos = self.target - self.offset(0.0);
        self.prev_pos = self.pos;
        self.prev_target = self.target;
    }

    pub fn update(&mut self, p: &Player, turn: f32, dt: f32) {
        self.prev_pos = self.pos;
        self.prev_target = self.target;
        if turn.abs() > 0.05 {
            self.yaw -= turn * 2.4 * dt;
            self.manual_timer = 1.2;
        } else {
            self.manual_timer = (self.manual_timer - dt).max(0.0);
        }
        // Automatisch hinter die Laufrichtung schwenken (nicht beim Rückwärtslaufen).
        let v = Vec2::new(p.body.vel.x, p.body.vel.z);
        if v.length() > 2.0 && self.manual_timer <= 0.0 && self.intro <= 0.0 {
            let heading = atan2(-v.x, -v.y);
            let d = angle_diff(self.yaw, heading);
            if d.abs() < 2.2 {
                self.yaw += d.clamp(-1.0, 1.0) * 1.1 * dt;
            }
        }
        let want = p.body.pos + Vec3::new(p.body.vel.x * 0.12, 1.1, p.body.vel.z * 0.12);
        let kxz = damp_factor(0.06, dt);
        self.target.x += (want.x - self.target.x) * kxz;
        self.target.z += (want.z - self.target.z) * kxz;
        // Vertikal träger, damit Sprünge nicht wackeln – außer beim tiefen Fallen.
        let hl = if p.body.on_ground {
            0.12
        } else if want.y < self.target.y - 2.0 {
            0.06
        } else {
            0.45
        };
        self.target.y += (want.y - self.target.y) * damp_factor(hl, dt);
        let mut extra = 0.0;
        if self.intro > 0.0 {
            self.intro = (self.intro - dt).max(0.0);
            extra = spatz::math::PI * (1.0 - smoothstep(0.0, 1.0, 1.0 - self.intro / INTRO_TIME));
        }
        self.pos = self.target - self.offset(extra);
    }

    /// Interpolierte Kamera für das Rendern.
    pub fn camera(&self, alpha: f32) -> Camera {
        let mut c = Camera { fov_y: 1.08, near: 0.1, far: 140.0, ..Camera::default() };
        let pos = self.prev_pos.lerp(self.pos, alpha);
        let target = self.prev_target.lerp(self.target, alpha);
        c.look_at(pos, target);
        c
    }
}
