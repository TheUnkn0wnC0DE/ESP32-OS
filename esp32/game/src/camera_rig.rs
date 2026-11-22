//! Verfolgerkamera: weich nachgeführt, leichte Vorausschau, folgt der
//! Bewegungsrichtung automatisch (abschaltbar durch manuelles Drehen).

use crate::player::Player;
use spatz::math::{Vec2, abs, angle_diff, atan2, cos, damp_factor, sin, smoothstep};
use spatz::physics::CollisionWorld;
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
    /// Anteil des Abstands, den die Kamera gerade nutzt (< 1: ein Hindernis
    /// stand zwischen Figur und Kamera, sie ist näher herangerückt).
    zoom: f32,
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
            zoom: 1.0,
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

    /// Freie Sicht: Strecke Figur → Kamera gegen die Kollisionskörper; steht
    /// etwas dazwischen, rückt die Kamera schnell davor und gleitet langsam
    /// zurück, sobald die Sicht wieder frei ist.
    fn freie_sicht(&mut self, welt: &CollisionWorld, dt: f32) {
        let d = self.pos - self.target;
        let len = d.length();
        if len < 1e-3 {
            return;
        }
        let dir = d * (1.0 / len);
        let mut frei = 1.0f32;
        for c in welt.colliders.iter().filter(|c| c.enabled) {
            let mut b = c.shape.aabb();
            b.min = b.min - Vec3::splat(0.2);
            b.max = b.max + Vec3::splat(0.2);
            if b.contains(self.target) {
                continue;
            }
            if let Some(t) = b.ray_hit(self.target, dir, len) {
                frei = frei.min(((t - 0.3) / len).max(0.0));
            }
        }
        let ziel = frei.max(0.22);
        self.zoom = if ziel < self.zoom {
            self.zoom + (ziel - self.zoom) * damp_factor(0.04, dt)
        } else {
            self.zoom + (ziel - self.zoom) * damp_factor(0.35, dt)
        };
        self.pos = self.target + d * self.zoom;
    }

    pub fn update(&mut self, p: &Player, turn: f32, dt: f32, welt: &CollisionWorld) {
        self.prev_pos = self.pos;
        self.prev_target = self.target;
        if abs(turn) > 0.05 {
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
            if abs(d) < 2.2 {
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
        self.freie_sicht(welt, dt);
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

#[cfg(test)]
mod tests {
    use super::*;
    use spatz::math::Aabb;
    use spatz::physics::{Collider, Shape};

    /// Eine Wand zwischen Figur und Kamera: die Kamera rückt davor, ohne
    /// Wand bleibt sie auf vollem Abstand.
    #[test]
    fn kamera_rueckt_vor_ein_hindernis() {
        let mut p = Player::new(Vec3::ZERO, 0.0);
        p.body.on_ground = true;
        let mut rig = CameraRig::new(p.body.pos, 0.0);
        let leer = CollisionWorld::new();
        for _ in 0..60 {
            rig.update(&p, 0.0, 1.0 / 60.0, &leer);
        }
        let voll = (rig.pos - rig.target).length();
        assert!((voll - rig.dist).abs() < 0.05, "frei: voller Abstand ({voll})");
        // Kamera liegt hinter der Figur (+z); Wand bei z = 3..3,5.
        let mut welt = CollisionWorld::new();
        welt.add(Collider::new(Shape::Box(Aabb::new(Vec3::new(-3.0, -2.0, 3.0), Vec3::new(3.0, 6.0, 3.5))), 0));
        for _ in 0..30 {
            rig.update(&p, 0.0, 1.0 / 60.0, &welt);
        }
        let nah = (rig.pos - rig.target).length();
        assert!(rig.pos.z < 3.0, "vor der Wand ({})", rig.pos.z);
        assert!(nah < voll * 0.6, "näher heran ({nah})");
        // Wand weg: gleitet zurück.
        for _ in 0..240 {
            rig.update(&p, 0.0, 1.0 / 60.0, &leer);
        }
        assert!((rig.pos - rig.target).length() > voll * 0.95, "wieder voller Abstand");
    }
}
