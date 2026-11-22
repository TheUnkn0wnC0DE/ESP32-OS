//! Geometrie-Primitive: AABB, Kugel, Ebene, Sichtpyramide.

use super::{Mat4, Vec3, abs};

/// Achsenparallele Box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Leere Box (neutral für `grow`).
    pub const EMPTY: Self = Self { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) };

    #[inline]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }
    #[inline]
    pub fn from_center_half(c: Vec3, h: Vec3) -> Self {
        Self::new(c - h, c + h)
    }
    #[inline]
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    #[inline]
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }
    #[inline]
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }
    #[inline]
    pub fn grow(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }
    #[inline]
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb::new(self.min.min(o.min), self.max.max(o.max))
    }
    #[inline]
    pub fn contains(&self, p: Vec3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
    #[inline]
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && self.max.x >= o.min.x
            && self.min.y <= o.max.y
            && self.max.y >= o.min.y
            && self.min.z <= o.max.z
            && self.max.z >= o.min.z
    }
    #[inline]
    pub fn closest_point(&self, p: Vec3) -> Vec3 {
        p.max(self.min).min(self.max)
    }
    /// Umschließende Box nach Transformation (Arvo).
    pub fn transformed(&self, m: &Mat4) -> Aabb {
        let c = m.transform_point(self.center());
        let h = self.half_extents();
        let a = |col: usize| m.cols[col].xyz().abs();
        let e = a(0) * h.x + a(1) * h.y + a(2) * h.z;
        Aabb::new(c - e, c + e)
    }
    /// Strahl-Box-Schnitt (Slab-Methode). Gibt `t` des Eintritts zurück.
    pub fn ray_hit(&self, origin: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
        let mut t0 = 0.0f32;
        let mut t1 = max_t;
        for (o, d, lo, hi) in [
            (origin.x, dir.x, self.min.x, self.max.x),
            (origin.y, dir.y, self.min.y, self.max.y),
            (origin.z, dir.z, self.min.z, self.max.z),
        ] {
            if abs(d) < 1e-12 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let (mut a, mut b) = ((lo - o) * inv, (hi - o) * inv);
                if a > b {
                    core::mem::swap(&mut a, &mut b);
                }
                t0 = t0.max(a);
                t1 = t1.min(b);
                if t0 > t1 {
                    return None;
                }
            }
        }
        Some(t0)
    }
}

/// Kugel (für Culling).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Sphere {
    #[inline]
    pub const fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }
    #[inline]
    pub fn transformed(&self, m: &Mat4) -> Sphere {
        Sphere::new(m.transform_point(self.center), self.radius * m.max_scale())
    }
}

/// Ebene `n·p + d = 0`; die Seite mit `n·p + d >= 0` gilt als „innen“.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Plane {
    pub n: Vec3,
    pub d: f32,
}

impl Plane {
    #[inline]
    pub fn from_point_normal(p: Vec3, n: Vec3) -> Self {
        let n = n.normalize_or_zero();
        Self { n, d: -n.dot(p) }
    }
    #[inline(always)]
    pub fn distance(&self, p: Vec3) -> f32 {
        self.n.dot(p) + self.d
    }
}

/// Sichtpyramide aus 6 nach innen zeigenden Ebenen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    /// Aus Kameraposition, Basisvektoren und halben Öffnungswinkeln (als Tangens).
    pub fn new(
        pos: Vec3,
        right: Vec3,
        up: Vec3,
        forward: Vec3,
        tan_half_x: f32,
        tan_half_y: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let p = |n: Vec3| Plane::from_point_normal(pos, n);
        Self {
            planes: [
                Plane::from_point_normal(pos + forward * near, forward),
                Plane::from_point_normal(pos + forward * far, -forward),
                p(right + forward * tan_half_x),
                p(-right + forward * tan_half_x),
                p(up + forward * tan_half_y),
                p(-up + forward * tan_half_y),
            ],
        }
    }

    /// `true`, wenn die Kugel (teilweise) sichtbar sein kann.
    #[inline]
    pub fn sphere_visible(&self, s: &Sphere) -> bool {
        self.planes.iter().all(|pl| pl.distance(s.center) >= -s.radius)
    }

    /// `true`, wenn die Box (teilweise) sichtbar sein kann.
    pub fn aabb_visible(&self, b: &Aabb) -> bool {
        let c = b.center();
        let h = b.half_extents();
        self.planes.iter().all(|pl| {
            let r = h.x * abs(pl.n.x) + h.y * abs(pl.n.y) + h.z * abs(pl.n.z);
            pl.distance(c) >= -r
        })
    }
}
