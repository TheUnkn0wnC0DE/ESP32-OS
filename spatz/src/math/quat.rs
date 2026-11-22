//! Quaternionen für Rotationen.

use super::{Vec3, cos, sin, sqrt};
use core::ops::Mul;

/// Einheits-Quaternion (x, y, z = Vektorteil, w = Skalarteil).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    /// Rotation um eine (normierte) Achse.
    #[inline]
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let h = angle * 0.5;
        let s = sin(h);
        Self { x: axis.x * s, y: axis.y * s, z: axis.z * s, w: cos(h) }
    }
    #[inline]
    pub fn from_rotation_x(a: f32) -> Self {
        Self::from_axis_angle(Vec3::X, a)
    }
    #[inline]
    pub fn from_rotation_y(a: f32) -> Self {
        Self::from_axis_angle(Vec3::Y, a)
    }
    #[inline]
    pub fn from_rotation_z(a: f32) -> Self {
        Self::from_axis_angle(Vec3::Z, a)
    }
    /// Gier (Y), dann Nick (X), dann Roll (Z) – typisch für Spielfiguren.
    #[inline]
    pub fn from_yaw_pitch_roll(yaw: f32, pitch: f32, roll: f32) -> Self {
        Self::from_rotation_y(yaw) * Self::from_rotation_x(pitch) * Self::from_rotation_z(roll)
    }
    #[inline]
    pub fn conjugate(self) -> Self {
        Self { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }
    #[inline]
    pub fn normalize(self) -> Self {
        let l = sqrt(self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w);
        if l < 1e-12 {
            return Self::IDENTITY;
        }
        let i = 1.0 / l;
        Self { x: self.x * i, y: self.y * i, z: self.z * i, w: self.w * i }
    }
    /// Dreht einen Vektor.
    #[inline]
    pub fn rotate(self, v: Vec3) -> Vec3 {
        let u = Vec3::new(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }
    /// Normalisierte lineare Interpolation (kürzester Weg).
    pub fn nlerp(self, o: Self, t: f32) -> Self {
        let d = self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w;
        let o = if d < 0.0 { Self { x: -o.x, y: -o.y, z: -o.z, w: -o.w } } else { o };
        Self {
            x: self.x + (o.x - self.x) * t,
            y: self.y + (o.y - self.y) * t,
            z: self.z + (o.z - self.z) * t,
            w: self.w + (o.w - self.w) * t,
        }
        .normalize()
    }
}

impl Mul for Quat {
    type Output = Quat;
    /// Hamilton-Produkt: `a * b` wendet erst `b`, dann `a` an.
    #[inline]
    fn mul(self, b: Quat) -> Quat {
        let a = self;
        Quat {
            w: a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
            x: a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
            y: a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
            z: a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        }
    }
}
