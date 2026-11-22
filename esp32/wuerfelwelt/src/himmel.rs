//! Tag und Nacht: Sonnenstand, Himmels- und Nebelfarben, Licht. `zeit` ist
//! der Tagesanteil 0..1 (0,25 Sonnenaufgang, 0,5 Mittag, 0,75 Untergang).

use spatz::color::Color;
use spatz::math::{Vec3, cos, sin};
use spatz::render::Environment;

/// Sekunden je Tag.
pub const TAG_SEKUNDEN: f32 = 480.0;
pub const NEBEL_START: f32 = 12.0;
pub const NEBEL_ENDE: f32 = 30.0;

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let k = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t) as u32) << s
    };
    k(16) | k(8) | k(0)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Richtung zur Sonne (normiert); unter dem Horizont: y < 0.
pub fn sonne(zeit: f32) -> Vec3 {
    let a = (zeit - 0.25) * core::f32::consts::TAU;
    Vec3::new(cos(a) * 0.85, sin(a), 0.38).normalize_or_zero()
}

/// Tageslicht 0 (Nacht) .. 1 (Tag).
pub fn tag(zeit: f32) -> f32 {
    smooth(-0.14, 0.22, sonne(zeit).y)
}

/// Abend-/Morgenröte 0..1.
pub fn roete(zeit: f32) -> f32 {
    let h = sonne(zeit).y;
    (1.0 - (h / 0.3).abs()).clamp(0.0, 1.0)
}

/// Tönung der (vorbeleuchteten) Blockfarben: Tag weiß, Abendrot warm,
/// Nacht bläulich gedämpft (Mondlicht).
pub fn tageslicht(zeit: f32) -> [f32; 3] {
    let t = tag(zeit);
    let r = roete(zeit);
    let l = mix3([0.34, 0.40, 0.62], [1.0, 1.0, 0.98], t);
    mix3(l, [1.0, 0.78, 0.62], r * 0.55)
}

pub fn umgebung(zeit: f32) -> Environment {
    let t = tag(zeit);
    let r = roete(zeit);
    let s = sonne(zeit);
    // Nachts leuchtet der Mond aus der Gegenrichtung.
    let licht_von = if s.y > -0.05 { s } else { Vec3::new(-s.x, -s.y, -s.z) };
    let zenit = mix(mix(0x070B1E, 0x3B7CD6, t), 0x4A5AA8, r * 0.5);
    let horizont = mix(mix(0x1C2B4E, 0xAED6F2, t), 0xF29E5A, r * 0.75);
    let sonnenfarbe = mix3([0.20, 0.24, 0.40], [0.80, 0.76, 0.64], t);
    let sonnenfarbe = mix3(sonnenfarbe, [0.95, 0.62, 0.38], r * 0.6);
    Environment {
        sun_dir: Vec3::new(-licht_von.x, -licht_von.y.max(0.15), -licht_von.z).normalize_or_zero(),
        sun_color: sonnenfarbe,
        ambient_sky: mix3([0.21, 0.24, 0.38], [0.50, 0.55, 0.66], t),
        ambient_ground: mix3([0.10, 0.10, 0.16], [0.30, 0.27, 0.25], t),
        sky_zenith: Color::hex(zenit),
        sky_horizon: Color::hex(horizont),
        sky_ground: Color::hex(mix(0x0E1424, 0x9CB9CC, t)),
        fog: true,
        fog_start: NEBEL_START,
        fog_end: NEBEL_ENDE,
        ..Environment::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mittag_hell_mitternacht_dunkel() {
        assert!(sonne(0.5).y > 0.9);
        assert!(tag(0.5) > 0.99);
        assert!(tag(0.0) < 0.01);
        assert!(roete(0.25) > 0.9 && roete(0.5) < 0.01);
        let (m, n) = (umgebung(0.5), umgebung(0.0));
        assert!(m.ambient_sky[0] > n.ambient_sky[0] * 2.0);
        assert!(m.sun_dir.y < 0.0, "Licht fällt von oben");
    }
}
