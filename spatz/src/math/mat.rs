//! 4x4-Matrix (spaltenweise gespeichert).

use super::{Quat, Vec3, Vec4};
use core::ops::Mul;

/// Spaltenweise 4x4-Matrix: `cols[3]` enthält die Translation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Mat4 {
    pub cols: [Vec4; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );

    #[inline(always)]
    pub const fn from_cols(c0: Vec4, c1: Vec4, c2: Vec4, c3: Vec4) -> Self {
        Self { cols: [c0, c1, c2, c3] }
    }

    #[inline]
    pub fn translation(t: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.cols[3] = Vec4::from_vec3(t, 1.0);
        m
    }

    #[inline]
    pub fn scale(s: Vec3) -> Self {
        Self::from_cols(
            Vec4::new(s.x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, s.y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, s.z, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Rotationsmatrix aus einem Einheits-Quaternion.
    pub fn from_quat(q: Quat) -> Self {
        let (x2, y2, z2) = (q.x + q.x, q.y + q.y, q.z + q.z);
        let (xx, xy, xz) = (q.x * x2, q.x * y2, q.x * z2);
        let (yy, yz, zz) = (q.y * y2, q.y * z2, q.z * z2);
        let (wx, wy, wz) = (q.w * x2, q.w * y2, q.w * z2);
        Self::from_cols(
            Vec4::new(1.0 - (yy + zz), xy + wz, xz - wy, 0.0),
            Vec4::new(xy - wz, 1.0 - (xx + zz), yz + wx, 0.0),
            Vec4::new(xz + wy, yz - wx, 1.0 - (xx + yy), 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
    }

    /// Translation · Rotation · Skalierung (die übliche Objekt-Transformation).
    pub fn from_trs(t: Vec3, r: Quat, s: Vec3) -> Self {
        let mut m = Self::from_quat(r);
        m.cols[0] *= s.x;
        m.cols[1] *= s.y;
        m.cols[2] *= s.z;
        m.cols[3] = Vec4::from_vec3(t, 1.0);
        m
    }

    #[inline]
    pub fn rotation_y(angle: f32) -> Self {
        Self::from_quat(Quat::from_rotation_y(angle))
    }

    #[inline(always)]
    pub fn row(&self, i: usize) -> Vec4 {
        let c = &self.cols;
        match i {
            0 => Vec4::new(c[0].x, c[1].x, c[2].x, c[3].x),
            1 => Vec4::new(c[0].y, c[1].y, c[2].y, c[3].y),
            2 => Vec4::new(c[0].z, c[1].z, c[2].z, c[3].z),
            _ => Vec4::new(c[0].w, c[1].w, c[2].w, c[3].w),
        }
    }

    #[inline(always)]
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let c = &self.cols;
        Vec3::new(
            c[0].x * p.x + c[1].x * p.y + c[2].x * p.z + c[3].x,
            c[0].y * p.x + c[1].y * p.y + c[2].y * p.z + c[3].y,
            c[0].z * p.x + c[1].z * p.y + c[2].z * p.z + c[3].z,
        )
    }

    #[inline(always)]
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let c = &self.cols;
        Vec3::new(
            c[0].x * v.x + c[1].x * v.y + c[2].x * v.z,
            c[0].y * v.x + c[1].y * v.y + c[2].y * v.z,
            c[0].z * v.x + c[1].z * v.y + c[2].z * v.z,
        )
    }

    #[inline(always)]
    pub fn transform_vec4(&self, v: Vec4) -> Vec4 {
        let c = &self.cols;
        c[0] * v.x + c[1] * v.y + c[2] * v.z + c[3] * v.w
    }

    #[inline(always)]
    pub fn translation_part(&self) -> Vec3 {
        self.cols[3].xyz()
    }

    /// Größter Skalierungsfaktor der 3x3-Teilmatrix (für Bounding-Spheres).
    pub fn max_scale(&self) -> f32 {
        let a = self.cols[0].xyz().length_sq();
        let b = self.cols[1].xyz().length_sq();
        let c = self.cols[2].xyz().length_sq();
        super::sqrt(a.max(b).max(c))
    }

    /// Inverse einer affinen Matrix (Rotation/Skalierung/Scherung + Translation).
    pub fn inverse_affine(&self) -> Self {
        let a = self.cols[0].xyz();
        let b = self.cols[1].xyz();
        let c = self.cols[2].xyz();
        let r0 = b.cross(c);
        let r1 = c.cross(a);
        let r2 = a.cross(b);
        let det = a.dot(r0);
        let inv_det = if super::abs(det) > 1e-20 { 1.0 / det } else { 0.0 };
        let (r0, r1, r2) = (r0 * inv_det, r1 * inv_det, r2 * inv_det);
        let t = self.cols[3].xyz();
        Self::from_cols(
            Vec4::new(r0.x, r1.x, r2.x, 0.0),
            Vec4::new(r0.y, r1.y, r2.y, 0.0),
            Vec4::new(r0.z, r1.z, r2.z, 0.0),
            Vec4::new(-r0.dot(t), -r1.dot(t), -r2.dot(t), 1.0),
        )
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    #[inline]
    fn mul(self, o: Mat4) -> Mat4 {
        Mat4::from_cols(
            self.transform_vec4(o.cols[0]),
            self.transform_vec4(o.cols[1]),
            self.transform_vec4(o.cols[2]),
            self.transform_vec4(o.cols[3]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trs_and_inverse_roundtrip() {
        let m = Mat4::from_trs(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_yaw_pitch_roll(0.7, -0.3, 0.2),
            Vec3::new(2.0, 0.5, 1.5),
        );
        let inv = m.inverse_affine();
        let p = Vec3::new(-4.0, 5.0, 0.25);
        let q = inv.transform_point(m.transform_point(p));
        assert!((p - q).length() < 1e-4, "{p:?} vs {q:?}");
        let id = m * inv;
        for c in 0..4 {
            let e = Mat4::IDENTITY.cols[c];
            assert!((id.cols[c] - e).dot(id.cols[c] - e) < 1e-8);
        }
    }

    #[test]
    fn quat_matches_matrix() {
        let q = Quat::from_yaw_pitch_roll(1.1, 0.4, -0.9);
        let m = Mat4::from_quat(q);
        let v = Vec3::new(0.3, -1.2, 2.0);
        assert!((q.rotate(v) - m.transform_vector(v)).length() < 1e-5);
        // Rotation um Y um +90°: +X wird zu -Z (rechtshändig).
        let r = Quat::from_rotation_y(core::f32::consts::FRAC_PI_2).rotate(Vec3::X);
        assert!((r - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5);
    }
}
