//! Animationshilfen: gedämpfte Federn für sekundäre Bewegung (Nachfedern von
//! Plattformen, Gegnern, Glubschaugen …). Rein optisch.
//!
//! Für echte Weichkörper (Wackelpudding) siehe [`crate::softbody`].

use crate::math::{Vec2, sqrt};

/// Gedämpfte Feder um die Ruhelage 0 (Masse 1).
///
/// `omega` = Eigenkreisfrequenz (rad/s, 2π·Hz), `zeta` = Dämpfungsgrad
/// (0 = schwingt ewig, 1 = kritisch gedämpft). Semi-implizites Euler; stabil
/// solange `omega * dt` deutlich unter 1 liegt.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub x: f32,
    pub v: f32,
}

impl Spring {
    pub const REST: Spring = Spring { x: 0.0, v: 0.0 };

    /// Ein Zeitschritt ohne äußere Kraft.
    #[inline]
    pub fn step(&mut self, omega: f32, zeta: f32, dt: f32) {
        self.step_force(0.0, omega, zeta, dt);
    }

    /// Ein Zeitschritt mit zusätzlicher äußerer Beschleunigung `force`.
    #[inline]
    pub fn step_force(&mut self, force: f32, omega: f32, zeta: f32, dt: f32) {
        let a = force - omega * omega * self.x - 2.0 * zeta * omega * self.v;
        self.v += a * dt;
        self.x += self.v * dt;
    }

    /// Stoß: ändert die Geschwindigkeit sofort.
    #[inline]
    pub fn kick(&mut self, dv: f32) {
        self.v += dv;
    }

    /// Momentane Schwingungsweite (aus Auslenkung und Geschwindigkeit).
    #[inline]
    pub fn amplitude(&self, omega: f32) -> f32 {
        let w = self.v / omega;
        sqrt(self.x * self.x + w * w)
    }

    /// Begrenzt die Auslenkung; nach außen gerichtete Geschwindigkeit verpufft.
    #[inline]
    pub fn limit(&mut self, max: f32) {
        if self.x > max {
            self.x = max;
            self.v = self.v.min(0.0);
        } else if self.x < -max {
            self.x = -max;
            self.v = self.v.max(0.0);
        }
    }
}

/// Zweidimensionale Feder.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring2 {
    pub x: Vec2,
    pub v: Vec2,
}

impl Spring2 {
    pub const REST: Spring2 = Spring2 { x: Vec2::ZERO, v: Vec2::ZERO };

    #[inline]
    pub fn step_force(&mut self, force: Vec2, omega: f32, zeta: f32, dt: f32) {
        let a = force - self.x * (omega * omega) - self.v * (2.0 * zeta * omega);
        self.v += a * dt;
        self.x += self.v * dt;
    }

    #[inline]
    pub fn kick(&mut self, dv: Vec2) {
        self.v += dv;
    }

    /// Begrenzt die Auslenkung auf die Länge `max`.
    #[inline]
    pub fn limit(&mut self, max: f32) {
        let l2 = self.x.length_sq();
        if l2 > max * max {
            let n = self.x * (1.0 / sqrt(l2));
            self.x = n * max;
            let out = self.v.dot(n);
            if out > 0.0 {
                self.v -= n * out;
            }
        }
    }
}

/// Schneller Sinus für Optik (Fehler < 2e-4), ohne Bibliotheksaufruf.
#[inline]
pub fn fast_sin(x: f32) -> f32 {
    use crate::math::{FRAC_PI_2, PI, TAU};
    let k = (x * (1.0 / TAU)) as i32 as f32;
    let mut r = x - k * TAU;
    if r > PI {
        r -= TAU;
    } else if r < -PI {
        r += TAU;
    }
    if r > FRAC_PI_2 {
        r = PI - r;
    } else if r < -FRAC_PI_2 {
        r = -PI - r;
    }
    let r2 = r * r;
    r * (1.0 - r2 * (1.0 / 6.0 - r2 * (1.0 / 120.0 - r2 * (1.0 / 5040.0))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_rings_and_settles() {
        let mut s = Spring::REST;
        s.kick(1.0);
        let mut crossed = 0;
        let mut prev = s.x;
        for _ in 0..240 {
            s.step(20.0, 0.1, 1.0 / 120.0);
            if prev.signum() != s.x.signum() && prev != 0.0 {
                crossed += 1;
            }
            prev = s.x;
        }
        assert!(crossed >= 4, "schwach gedämpfte Feder muss mehrfach durchschwingen ({crossed})");
        for _ in 0..1200 {
            s.step(20.0, 0.1, 1.0 / 120.0);
        }
        assert!(s.amplitude(20.0) < 1e-3);
    }

    #[test]
    fn fast_sin_matches() {
        let mut x = -40.0f32;
        while x < 40.0 {
            assert!((fast_sin(x) - crate::math::sin(x)).abs() < 3e-4, "{x}");
            x += 0.013;
        }
    }
}
