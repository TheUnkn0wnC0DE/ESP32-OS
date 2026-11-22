//! Perspektivische Kamera (Gier/Nick, ohne Rollen).

use crate::math::{Vec3, atan2, cos, sin, sqrt};

/// Kamera. Gier 0 blickt nach -Z; positive Gier dreht nach links
/// (gegen den Uhrzeigersinn von oben gesehen); positiver Nick blickt nach oben.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// Vertikaler Öffnungswinkel in Radiant.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { position: Vec3::ZERO, yaw: 0.0, pitch: 0.0, fov_y: 1.05, near: 0.1, far: 150.0 }
    }
}

impl Camera {
    /// Blickrichtung (normiert).
    #[inline]
    pub fn forward(&self) -> Vec3 {
        let cp = cos(self.pitch);
        Vec3::new(-sin(self.yaw) * cp, sin(self.pitch), -cos(self.yaw) * cp)
    }

    /// Rechts-Vektor (horizontal).
    #[inline]
    pub fn right(&self) -> Vec3 {
        Vec3::new(cos(self.yaw), 0.0, -sin(self.yaw))
    }

    /// Orthonormale Basis (rechts, oben, vorne).
    #[inline]
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let f = self.forward();
        let r = self.right();
        (r, r.cross(f), f)
    }

    /// Richtet die Kamera von `eye` auf `target` aus.
    pub fn look_at(&mut self, eye: Vec3, target: Vec3) {
        self.position = eye;
        let d = target - eye;
        let horiz = sqrt(d.x * d.x + d.z * d.z);
        self.yaw = atan2(-d.x, -d.z);
        self.pitch = atan2(d.y, horiz);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_is_orthonormal_and_look_at_works() {
        let mut c = Camera::default();
        c.look_at(Vec3::new(3.0, 4.0, 5.0), Vec3::new(-1.0, 0.5, -2.0));
        let (r, u, f) = c.basis();
        assert!((r.length() - 1.0).abs() < 1e-5 && (u.length() - 1.0).abs() < 1e-5);
        assert!(r.dot(f).abs() < 1e-5 && u.dot(f).abs() < 1e-5 && r.dot(u).abs() < 1e-5);
        let want = (Vec3::new(-1.0, 0.5, -2.0) - Vec3::new(3.0, 4.0, 5.0)).normalize_or_zero();
        assert!((f - want).length() < 1e-4, "{f:?} vs {want:?}");
        assert!(u.y > 0.0);
    }
}
