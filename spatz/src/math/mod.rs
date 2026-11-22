//! Mathe-Grundlagen: Skalarfunktionen, Vektoren, Matrizen, Quaternionen,
//! Geometrie-Primitive.
//!
//! Mit Feature `std` werden die Hardware-Instruktionen der Standardbibliothek
//! benutzt (`sqrt`, `sin`, ...). Ohne `std` (`no_std`-Ziele, z. B.
//! Taschenrechner) greifen kompakte Software-Näherungen, die für Spiele genau
//! genug sind (rel. Fehler ~1e-5).

mod geom;
mod mat;
mod quat;
mod vec;

pub use geom::{Aabb, Frustum, Plane, Sphere};
pub use mat::Mat4;
pub use quat::Quat;
pub use vec::{Vec2, Vec3, Vec4};

/// π
pub const PI: f32 = core::f32::consts::PI;
/// 2π
pub const TAU: f32 = core::f32::consts::TAU;
/// π/2
pub const FRAC_PI_2: f32 = core::f32::consts::FRAC_PI_2;

/// Betrag (bitweise, funktioniert ohne `std`).
#[inline(always)]
pub fn abs(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

/// Begrenzt `x` auf `[lo, hi]`.
#[inline(always)]
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// Lineare Interpolation.
#[inline(always)]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Hermite-Glättung zwischen `e0` und `e1`.
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Vorzeichen (-1, 0, 1).
#[inline]
pub fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Bewegt `current` maximal um `max_delta` in Richtung `target`.
#[inline]
pub fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    if current < target { (current + max_delta).min(target) } else { (current - max_delta).max(target) }
}

/// Bringt einen Winkel in den Bereich `[-π, π]`.
#[inline]
pub fn wrap_angle(a: f32) -> f32 {
    a - TAU * floor(a / TAU + 0.5)
}

/// Kürzeste Winkeldifferenz `to - from` in `[-π, π]`.
#[inline]
pub fn angle_diff(from: f32, to: f32) -> f32 {
    wrap_angle(to - from)
}

/// Bildrate-unabhängiger Glättungsfaktor: Anteil, der in `dt` Sekunden bei
/// Halbwertszeit `half_life` zurückgelegt wird.
#[inline]
pub fn damp_factor(half_life: f32, dt: f32) -> f32 {
    if half_life <= 0.0 { 1.0 } else { 1.0 - exp(-core::f32::consts::LN_2 * dt / half_life) }
}

#[cfg(feature = "std")]
mod imp {
    #[inline(always)]
    pub fn sqrt(x: f32) -> f32 {
        x.sqrt()
    }
    #[inline(always)]
    pub fn floor(x: f32) -> f32 {
        x.floor()
    }
    #[inline(always)]
    pub fn ceil(x: f32) -> f32 {
        x.ceil()
    }
    #[inline(always)]
    pub fn round(x: f32) -> f32 {
        x.round()
    }
    #[inline(always)]
    pub fn sin(x: f32) -> f32 {
        x.sin()
    }
    #[inline(always)]
    pub fn cos(x: f32) -> f32 {
        x.cos()
    }
    #[inline(always)]
    pub fn atan2(y: f32, x: f32) -> f32 {
        y.atan2(x)
    }
    #[inline(always)]
    pub fn exp(x: f32) -> f32 {
        x.exp()
    }
}

#[cfg(not(feature = "std"))]
mod imp {
    use super::{FRAC_PI_2, PI, TAU, abs};

    /// Abrunden ohne `std`.
    #[inline]
    pub fn floor(x: f32) -> f32 {
        if abs(x) >= 8_388_608.0 || x.is_nan() {
            return x;
        }
        let t = x as i32 as f32;
        if t > x { t - 1.0 } else { t }
    }

    #[inline]
    pub fn ceil(x: f32) -> f32 {
        -floor(-x)
    }

    #[inline]
    pub fn round(x: f32) -> f32 {
        if x >= 0.0 { floor(x + 0.5) } else { -floor(-x + 0.5) }
    }

    /// Quadratwurzel: Bit-Trick als Startwert + 3 Newton-Schritte.
    #[inline]
    pub fn sqrt(x: f32) -> f32 {
        if x <= 0.0 || x.is_nan() {
            return 0.0;
        }
        if x == f32::INFINITY {
            return x;
        }
        let mut y = f32::from_bits(0x1fbd_1df5 + (x.to_bits() >> 1));
        y = 0.5 * (y + x / y);
        y = 0.5 * (y + x / y);
        0.5 * (y + x / y)
    }

    /// Sinus: Reduktion auf [-π/2, π/2], Polynom 9. Ordnung.
    #[inline]
    pub fn sin(x: f32) -> f32 {
        let mut x = x - TAU * floor(x / TAU + 0.5);
        if x > FRAC_PI_2 {
            x = PI - x;
        } else if x < -FRAC_PI_2 {
            x = -PI - x;
        }
        let x2 = x * x;
        x * (1.0 + x2 * (-1.0 / 6.0 + x2 * (1.0 / 120.0 + x2 * (-1.0 / 5040.0 + x2 * (1.0 / 362_880.0)))))
    }

    #[inline]
    pub fn cos(x: f32) -> f32 {
        sin(x + FRAC_PI_2)
    }

    /// atan2 über Polynom-Näherung auf [0, 1] (max. Fehler ~1e-5 rad).
    pub fn atan2(y: f32, x: f32) -> f32 {
        if x == 0.0 && y == 0.0 {
            return 0.0;
        }
        let ax = abs(x);
        let ay = abs(y);
        let (a, swap) = if ax >= ay { (ay / ax, false) } else { (ax / ay, true) };
        let s = a * a;
        let mut r = a * (0.999_866 + s * (-0.330_299_5 + s * (0.180_141 + s * (-0.085_133 + s * 0.020_835_1))));
        if swap {
            r = FRAC_PI_2 - r;
        }
        if x < 0.0 {
            r = PI - r;
        }
        if y < 0.0 { -r } else { r }
    }

    /// e^x über 2^(x·log2 e), Polynom 5. Ordnung für den Bruchteil.
    pub fn exp(x: f32) -> f32 {
        if x < -87.0 {
            return 0.0;
        }
        if x > 88.0 {
            return f32::INFINITY;
        }
        let t = x * core::f32::consts::LOG2_E;
        let i = floor(t);
        let f = t - i;
        let p = 1.0 + f * (0.693_147_2 + f * (0.240_226_5 + f * (0.055_504_1 + f * (0.009_618_13 + f * 0.001_333_36))));
        p * f32::from_bits(((i as i32 + 127) as u32) << 23)
    }
}

pub use imp::{atan2, ceil, cos, exp, floor, round, sin, sqrt};

/// Tangens als sin/cos.
#[inline]
pub fn tan(x: f32) -> f32 {
    sin(x) / cos(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        abs(a - b) <= eps
    }

    #[test]
    fn scalar_functions_are_accurate() {
        for i in -200..200 {
            let x = i as f32 * 0.173;
            assert!(close(sin(x), std::primitive::f32::sin(x), 2e-4), "sin {x}");
            assert!(close(cos(x), std::primitive::f32::cos(x), 2e-4), "cos {x}");
            assert!(close(floor(x), std::primitive::f32::floor(x), 0.0), "floor {x}");
            let y = i as f32 * 0.031 - 1.0;
            assert!(close(atan2(y, x), std::primitive::f32::atan2(y, x), 1e-3), "atan2");
        }
        for i in 1..500 {
            let x = i as f32 * 0.77;
            assert!(close(sqrt(x), std::primitive::f32::sqrt(x), 1e-4 * x.max(1.0)));
        }
        assert!(close(exp(1.0), core::f32::consts::E, 1e-3));
        assert!(close(wrap_angle(3.0 * PI), PI, 1e-4) || close(wrap_angle(3.0 * PI), -PI, 1e-4));
    }
}
