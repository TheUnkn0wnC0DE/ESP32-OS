//! 2D/3D/4D-Vektoren (f32).

use super::{cos, sin, sqrt};
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

macro_rules! impl_vec_ops {
    ($t:ident, $($f:ident),+) => {
        impl Add for $t {
            type Output = $t;
            #[inline(always)]
            fn add(self, o: $t) -> $t { $t { $($f: self.$f + o.$f),+ } }
        }
        impl Sub for $t {
            type Output = $t;
            #[inline(always)]
            fn sub(self, o: $t) -> $t { $t { $($f: self.$f - o.$f),+ } }
        }
        impl Mul<f32> for $t {
            type Output = $t;
            #[inline(always)]
            fn mul(self, s: f32) -> $t { $t { $($f: self.$f * s),+ } }
        }
        impl Mul<$t> for f32 {
            type Output = $t;
            #[inline(always)]
            fn mul(self, v: $t) -> $t { $t { $($f: v.$f * self),+ } }
        }
        impl Mul<$t> for $t {
            type Output = $t;
            #[inline(always)]
            fn mul(self, o: $t) -> $t { $t { $($f: self.$f * o.$f),+ } }
        }
        impl Div<f32> for $t {
            type Output = $t;
            #[inline(always)]
            fn div(self, s: f32) -> $t { let inv = 1.0 / s; $t { $($f: self.$f * inv),+ } }
        }
        impl Neg for $t {
            type Output = $t;
            #[inline(always)]
            fn neg(self) -> $t { $t { $($f: -self.$f),+ } }
        }
        impl AddAssign for $t {
            #[inline(always)]
            fn add_assign(&mut self, o: $t) { $(self.$f += o.$f;)+ }
        }
        impl SubAssign for $t {
            #[inline(always)]
            fn sub_assign(&mut self, o: $t) { $(self.$f -= o.$f;)+ }
        }
        impl MulAssign<f32> for $t {
            #[inline(always)]
            fn mul_assign(&mut self, s: f32) { $(self.$f *= s;)+ }
        }
        impl DivAssign<f32> for $t {
            #[inline(always)]
            fn div_assign(&mut self, s: f32) { let inv = 1.0 / s; $(self.$f *= inv;)+ }
        }
    };
}

/// 2D-Vektor (oft für die XZ-Ebene genutzt).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}
impl_vec_ops!(Vec2, x, y);

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y
    }
    /// 2D-Kreuzprodukt (z-Komponente).
    #[inline(always)]
    pub fn perp_dot(self, o: Self) -> f32 {
        self.x * o.y - self.y * o.x
    }
    #[inline(always)]
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }
    #[inline]
    pub fn length(self) -> f32 {
        sqrt(self.length_sq())
    }
    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { Self::ZERO }
    }
    #[inline]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        self + (o - self) * t
    }
    /// Dreht um `angle` (gegen den Uhrzeigersinn).
    #[inline]
    pub fn rotate(self, angle: f32) -> Self {
        let (s, c) = (sin(angle), cos(angle));
        Self::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    /// Begrenzt die Länge auf `max`.
    #[inline]
    pub fn clamp_length(self, max: f32) -> Self {
        let l2 = self.length_sq();
        if l2 > max * max { self * (max / sqrt(l2)) } else { self }
    }
}

/// 3D-Vektor. Konvention: rechtshändig, +Y oben, Kamera blickt nach -Z.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl_vec_ops!(Vec3, x, y, z);

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[inline(always)]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    #[inline(always)]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }
    /// Baut aus einem XZ-Vektor und einer Höhe einen 3D-Vektor.
    #[inline(always)]
    pub const fn from_xz(v: Vec2, y: f32) -> Self {
        Self::new(v.x, y, v.y)
    }
    #[inline(always)]
    pub fn xz(self) -> Vec2 {
        Vec2::new(self.x, self.z)
    }
    #[inline(always)]
    pub fn with_y(self, y: f32) -> Self {
        Self::new(self.x, y, self.z)
    }
    #[inline(always)]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[inline(always)]
    pub fn cross(self, o: Self) -> Self {
        Self::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    #[inline(always)]
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }
    #[inline]
    pub fn length(self) -> f32 {
        sqrt(self.length_sq())
    }
    #[inline]
    pub fn distance(self, o: Self) -> f32 {
        (self - o).length()
    }
    #[inline]
    pub fn distance_sq(self, o: Self) -> f32 {
        (self - o).length_sq()
    }
    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { Self::ZERO }
    }
    #[inline]
    pub fn normalize_or(self, fallback: Self) -> Self {
        let l = self.length();
        if l > 1e-12 { self * (1.0 / l) } else { fallback }
    }
    #[inline]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        self + (o - self) * t
    }
    #[inline]
    pub fn min(self, o: Self) -> Self {
        Self::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    #[inline]
    pub fn max(self, o: Self) -> Self {
        Self::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    #[inline]
    pub fn abs(self) -> Self {
        Self::new(super::abs(self.x), super::abs(self.y), super::abs(self.z))
    }
    #[inline]
    pub fn max_element(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }
}

/// 4D-Vektor (homogene Koordinaten).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
impl_vec_ops!(Vec4, x, y, z, w);

impl Vec4 {
    #[inline(always)]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    #[inline(always)]
    pub const fn from_vec3(v: Vec3, w: f32) -> Self {
        Self::new(v.x, v.y, v.z, w)
    }
    #[inline(always)]
    pub fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
    #[inline(always)]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }
}
