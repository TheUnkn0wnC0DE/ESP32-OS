//! Alle Modelle des Spiels – prozedural aus Grundkörpern gebaut (keine
//! Dateien nötig). Stil: Low-Poly, Flat-Shading, Süßigkeitenfarben.
//!
//! Mikrocontroller-Fassung: weniger Streusel/Tropfen/Segmente als am PC (bei
//! 320×180 bzw. 640×360 Pixeln kaum zu sehen), damit Rechenzeit für 60 FPS
//! bleibt.

use crate::level::{CakeStyle, GoalStyle};
use alloc::vec::Vec;
use spatz::math::{PI, TAU, abs, atan2, cos, round, sin};
use spatz::prelude::*;
use spatz::render::shapes::{self, add_cuboid, add_cylinder, add_torus, add_uv_sphere};

pub mod pal {
    use spatz::color::Color;
    pub const VANILLA: Color = Color::hex(0xF7D354);
    pub const VANILLA_DARK: Color = Color::hex(0xD9A93A);
    pub const CARAMEL: Color = Color::hex(0xB8641F);
    pub const CREAM: Color = Color::hex(0xFFF8EC);
    pub const CHERRY: Color = Color::hex(0xE0203A);
    pub const STEM: Color = Color::hex(0x3C8C2C);
    pub const SPONGE: Color = Color::hex(0xF3D49B);
    pub const CHOCO: Color = Color::hex(0x7A4428);
    pub const CHOCO_DARK: Color = Color::hex(0x4F2A16);
    pub const COOKIE: Color = Color::hex(0xDDA05C);
    pub const COOKIE_DARK: Color = Color::hex(0xB57636);
    pub const WAFER: Color = Color::hex(0xE8B96A);
    pub const WAFER_DARK: Color = Color::hex(0xC4924A);
    pub const ICE: Color = Color::hex(0x9EE0FF);
    pub const ICE_CORE: Color = Color::hex(0xE8FAFF);
    pub const DOUGH: Color = Color::hex(0xD7934F);
    pub const FROST: Color = Color::hex(0xFF7EB6);
    pub const MALLOW: Color = Color::hex(0xFFEFF4);
    pub const MALLOW_SIDE: Color = Color::hex(0xF7C8D8);
    pub const FRIDGE: Color = Color::hex(0xEEF3F7);
    pub const FRIDGE_DARK: Color = Color::hex(0xB8C4CE);
    pub const FRIDGE_INSIDE: Color = Color::hex(0xBFEFFF);
    pub const STICK: Color = Color::hex(0xE6C896);
    pub const RED: Color = Color::hex(0xE8303A);
    pub const WHITE: Color = Color::hex(0xFAFAFA);
    pub const BLACK: Color = Color::hex(0x141414);
    pub const SEA: Color = Color::hex(0xA0643A);
    pub const SPRINKLES: [Color; 5] =
        [Color::hex(0xFFFFFF), Color::hex(0x4FC8FF), Color::hex(0xFFE14D), Color::hex(0x6EDC5F), Color::hex(0xB07CFF)];
}

/// Kleiner deterministischer Hash für „zufällige“ Details.
fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

/// Rotation, die `a` auf `b` dreht (beide normiert).
pub fn quat_from_to(a: Vec3, b: Vec3) -> Quat {
    let d = a.dot(b);
    if d > 0.9999 {
        return Quat::IDENTITY;
    }
    if d < -0.9999 {
        return Quat::from_rotation_x(PI);
    }
    let c = a.cross(b);
    Quat::from_axis_angle(c.normalize_or_zero(), atan2(c.length(), d))
}

fn add_box(m: &mut Mesh, min: Vec3, max: Vec3, c: Color) {
    add_cuboid(m, min, max, [c; 6]);
}

/// Box mit eigener Farbe für Ober- und Unterseite.
fn add_box3(m: &mut Mesh, min: Vec3, max: Vec3, top: Color, side: Color, bottom: Color) {
    add_cuboid(m, min, max, [side, side, top, bottom, side, side]);
}

/// Zylinder zwischen zwei Punkten.
fn add_segment(m: &mut Mesh, p0: Vec3, p1: Vec3, r: f32, segs: usize, c: Color) {
    let d = p1 - p0;
    let len = d.length();
    if len < 1e-5 {
        return;
    }
    let mut part = Mesh::new();
    add_cylinder(&mut part, segs, r, r, 0.0, len, c, Some(c), Some(c));
    m.append(&part, &Mat4::from_trs(p0, quat_from_to(Vec3::Y, d / len), Vec3::ONE));
}

/// Flache Quadfläche (nach oben) – für Streusel und Deko auf Oberseiten.
fn add_flat_quad(m: &mut Mesh, center: Vec3, len: f32, wid: f32, angle: f32, c: Color) {
    let (s, co) = (sin(angle), cos(angle));
    let ax = Vec3::new(co, 0.0, -s) * (len * 0.5);
    let az = Vec3::new(s, 0.0, co) * (wid * 0.5);
    let a = m.vertex(center - ax + az);
    let b = m.vertex(center + ax + az);
    let cc = m.vertex(center + ax - az);
    let d = m.vertex(center - ax - az);
    m.quad(a, b, cc, d, c);
}

/// Scheibe in der XY-Ebene, Vorderseite nach -Z (für Augen, Münder).
fn face_disc(segments: usize, r: f32, c: Color) -> Mesh {
    shapes::disc(segments, r, c).transformed(&Mat4::from_quat(Quat::from_rotation_x(-PI * 0.5)))
}

/// Gitterauflösung und Kantenlänge des Schokoladensees (folgt der Kamera;
/// der Nebel endet auf dem Mikrocontroller früher, daher kleiner als am PC).
pub const SEA_CELLS: usize = 12;
pub const SEA_SIZE: f32 = 176.0;

/// Höhe des Puddingkörpers bis zur Karamelloberseite.
pub const PUDDING_H: f32 = 0.84;
/// Mittelpunkte von Sahnehaube und Kirsche (Ruhelage, Modellraum).
pub const CREAM_POS: Vec3 = Vec3::new(0.0, 0.88, 0.0);
pub const CHERRY_POS: Vec3 = Vec3::new(0.0, 1.06, 0.0);

/// Vertexbereiche des Pudding-Meshes (für die Weichkörper-Haut).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuddingParts {
    /// Körper mit Karamellhaube und -nasen (folgt dem Gitter).
    pub body: core::ops::Range<usize>,
    /// Sahnehaube (eigenes Partikel, schwingt nach).
    pub cream: core::ops::Range<usize>,
    /// Kirsche mit Stiel (Pendel auf der Sahne).
    pub cherry: core::ops::Range<usize>,
}

/// Feste Modelle des Spiels.
pub struct Assets {
    /// Wird pro Frame vom Weichkörper verformt (siehe `jelly.rs`).
    pub pudding: MeshId,
    pub pudding_parts: PuddingParts,
    pub eye_white: MeshId,
    pub pupil: MeshId,
    pub mouth: MeshId,
    pub cherry: MeshId,
    pub cherry_far: MeshId,
    pub ice: MeshId,
    pub ice_core: MeshId,
    pub donut: MeshId,
    pub brow: MeshId,
    pub mallow: MeshId,
    pub spinner_post: MeshId,
    pub popsicle_off: MeshId,
    pub popsicle_on: MeshId,
    pub fridge: MeshId,
    pub fridge_door: MeshId,
    pub shadow: MeshId,
    pub sea: MeshId,
    pub cloud: MeshId,
    pub cupcake: MeshId,
    pub cocoa: MeshId,
}

impl Assets {
    pub fn build(store: &mut MeshStore) -> Self {
        let (mesh, pudding_parts) = pudding();
        let pudding = store.add(mesh);
        Self {
            pudding,
            pudding_parts,
            eye_white: store.add(face_disc(10, 0.11, pal::WHITE)),
            pupil: store.add(face_disc(8, 0.052, pal::BLACK)),
            mouth: store.add(face_disc(10, 0.06, Color::hex(0x6A1A14))),
            cherry: store.add(cherry()),
            cherry_far: store.add(cherry_far()),
            ice: store.add(shapes::box_centered(Vec3::splat(0.6), pal::ICE)),
            ice_core: store.add(shapes::box_centered(Vec3::splat(0.32), pal::ICE_CORE)),
            donut: store.add(donut()),
            brow: store.add(shapes::box_centered(Vec3::new(0.2, 0.05, 0.04), pal::CHOCO_DARK)),
            mallow: store.add(mallow()),
            spinner_post: store.add(spinner_post()),
            popsicle_off: store.add(popsicle(Color::hex(0x9DB4C8), Color::hex(0x7F96AA))),
            popsicle_on: store.add(popsicle(Color::hex(0xFF5C8A), Color::hex(0xFFD0DE))),
            fridge: store.add(fridge()),
            fridge_door: store.add(fridge_door()),
            shadow: store.add(shapes::disc(12, 1.0, pal::BLACK)),
            sea: store.add(shapes::grid(SEA_CELLS, SEA_SIZE, pal::SEA)),
            cloud: store.add(cloud()),
            cupcake: store.add(cupcake()),
            cocoa: store.add(cocoa()),
        }
    }
}

/// Rotationskörper um +Y aus Ringen. `profile` von unten nach oben: (y, Radius,
/// Farbe des Bandes darüber). `flute` riffelt die Seiten wie eine Puddingform
/// (Radius abwechselnd ±flute). Deckel optional.
fn add_lathe(m: &mut Mesh, segs: usize, flute: f32, profile: &[(f32, f32, Color)], top: Color, bottom: Color) {
    let first = m.vertex_count() as u16;
    for &(y, r, _) in profile {
        for i in 0..segs {
            let a = i as f32 / segs as f32 * TAU;
            let rr = r * if i % 2 == 0 { 1.0 + flute } else { 1.0 - flute };
            m.vertex(Vec3::new(cos(a) * rr, y, -sin(a) * rr));
        }
    }
    let at = |ring: usize, i: usize| first + (ring * segs + i % segs) as u16;
    for ring in 0..profile.len() - 1 {
        for i in 0..segs {
            m.quad(at(ring, i), at(ring, i + 1), at(ring + 1, i + 1), at(ring + 1, i), profile[ring].2);
        }
    }
    let last = profile.len() - 1;
    for i in 1..segs - 1 {
        m.tri(at(last, 0), at(last, i), at(last, i + 1), top);
        m.tri(at(0, 0), at(0, i + 1), at(0, i), bottom);
    }
}

/// Die Spielfigur: Vanillepudding aus der Riffelform mit Karamellhaube, Sahne
/// und Kirsche. Füße bei y = 0, Vorderseite zeigt nach -Z. Der Körper hat
/// mehrere Ringe, damit er sich beim Wackeln weich biegen und wölben kann.
fn pudding() -> (Mesh, PuddingParts) {
    let mut m = Mesh::new();
    let (v, c) = (pal::VANILLA, pal::CARAMEL);
    let profile =
        [(0.0, 0.445, v), (0.18, 0.46, v), (0.40, 0.44, v), (0.60, 0.40, v), (0.72, 0.375, c), (PUDDING_H, 0.30, c)];
    add_lathe(&mut m, 12, 0.03, &profile, pal::CARAMEL, pal::VANILLA_DARK);
    // Karamell-Nasen, die über den Rand laufen (Ober- und Innenseite stecken
    // in der Haube bzw. im Körper und werden weggelassen).
    for i in 0..5 {
        let a = i as f32 * TAU / 5.0 + 0.5;
        let (dx, dz) = (cos(a), -sin(a));
        let len = 0.10 + (i % 3) as f32 * 0.05;
        let r = 0.395;
        let mut drip = Mesh::new();
        let lo = Vec3::new(-0.05, -len, -0.03);
        shapes::add_cuboid_faces(&mut drip, lo, Vec3::new(0.05, 0.0, 0.03), [pal::CARAMEL; 6], 0b01_1011);
        let rot = Quat::from_rotation_y(a + PI * 0.5);
        m.append(&drip, &Mat4::from_trs(Vec3::new(dx * r, 0.76, dz * r), rot, Vec3::ONE));
    }
    let body_end = m.vertex_count();
    add_uv_sphere(&mut m, CREAM_POS, 0.14, 7, 4, pal::CREAM);
    let cream_end = m.vertex_count();
    add_uv_sphere(&mut m, CHERRY_POS, 0.075, 6, 4, pal::CHERRY);
    add_segment(&mut m, Vec3::new(0.0, 1.1, 0.0), Vec3::new(0.05, 1.22, 0.02), 0.012, 3, pal::STEM);
    let parts = PuddingParts { body: 0..body_end, cream: body_end..cream_end, cherry: cream_end..m.vertex_count() };
    (m, parts)
}

/// Kirschenpaar für die Ferne: zwei Oktaeder + ein Stiel (24 Dreiecke).
fn cherry_far() -> Mesh {
    let mut m = Mesh::new();
    let g = gem_at(Vec3::new(-0.17, -0.12, 0.0), 0.2);
    m.append(&g, &Mat4::IDENTITY);
    m.append(&gem_at(Vec3::new(0.17, -0.16, 0.03), 0.2), &Mat4::IDENTITY);
    add_segment(&mut m, Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.02, 0.36, 0.0), 0.03, 3, pal::STEM);
    m
}

fn gem_at(c: Vec3, r: f32) -> Mesh {
    shapes::gem(r, r, pal::CHERRY, Color::hex(0xB01830)).transformed(&Mat4::translation(c))
}

/// Kirschenpaar mit Stielen und Blatt (Mittelpunkt ~ Ursprung).
fn cherry() -> Mesh {
    let mut m = Mesh::new();
    let l = Vec3::new(-0.17, -0.12, 0.0);
    let r = Vec3::new(0.17, -0.16, 0.03);
    add_uv_sphere(&mut m, l, 0.19, 7, 4, pal::CHERRY);
    add_uv_sphere(&mut m, r, 0.19, 7, 4, pal::CHERRY);
    let top = Vec3::new(0.02, 0.36, 0.0);
    add_segment(&mut m, l + Vec3::new(0.0, 0.16, 0.0), top, 0.025, 3, pal::STEM);
    add_segment(&mut m, r + Vec3::new(0.0, 0.16, 0.0), top, 0.025, 3, pal::STEM);
    let a = m.vertex(top);
    let b = m.vertex(top + Vec3::new(0.22, 0.10, 0.02));
    let c = m.vertex(top + Vec3::new(0.12, 0.02, 0.0));
    let d = m.vertex(top + Vec3::new(0.24, -0.02, 0.05));
    m.tri(a, c, b, Color::hex(0x4CB03C));
    m.tri(c, d, b, Color::hex(0x3E9A30));
    m.tri(a, b, c, Color::hex(0x4CB03C));
    m.tri(c, b, d, Color::hex(0x3E9A30));
    m
}

/// Donut (flach liegend, Mittelpunkt im Ursprung) mit Glasur und Streuseln.
fn donut() -> Mesh {
    let mut m = Mesh::new();
    let color = |i: usize, j: usize, _p: Vec3| -> Color {
        let frosted = j < 4 || j == 7;
        if !frosted {
            return pal::DOUGH;
        }
        let h = hash(i as u32, j as u32);
        if j != 7 && h.is_multiple_of(4) { pal::SPRINKLES[(h >> 8) as usize % 5] } else { pal::FROST }
    };
    add_torus(&mut m, 0.5, 0.24, 12, 8, &color);
    m
}

fn mallow() -> Mesh {
    let mut m = Mesh::new();
    add_cylinder(&mut m, 12, 0.75, 0.70, 0.0, 0.5, pal::MALLOW_SIDE, None, Some(pal::MALLOW_SIDE));
    add_cylinder(&mut m, 12, 0.70, 0.55, 0.5, 0.62, pal::MALLOW, Some(pal::MALLOW), None);
    m
}

fn spinner_post() -> Mesh {
    let mut m = Mesh::new();
    add_cylinder(&mut m, 8, 0.28, 0.24, 0.0, 0.9, pal::WHITE, Some(pal::RED), None);
    add_uv_sphere(&mut m, Vec3::new(0.0, 0.95, 0.0), 0.2, 6, 4, pal::RED);
    m
}

/// Zuckerstangen-Balken entlang X, zentriert, Länge `len`.
pub fn spinner_bar(len: f32) -> Mesh {
    let mut m = Mesh::new();
    let n = ((len / 0.45) as usize).max(2);
    let step = len / n as f32;
    for k in 0..n {
        let x0 = -len * 0.5 + k as f32 * step;
        let c = if k % 2 == 0 { pal::RED } else { pal::WHITE };
        add_segment(&mut m, Vec3::new(x0, 0.0, 0.0), Vec3::new(x0 + step, 0.0, 0.0), 0.17, 6, c);
    }
    m
}

/// Eis am Stiel (Checkpoint).
fn popsicle(ice: Color, top: Color) -> Mesh {
    let mut m = Mesh::new();
    add_box(&mut m, Vec3::new(-0.07, 0.0, -0.04), Vec3::new(0.07, 0.75, 0.04), pal::STICK);
    add_box3(&mut m, Vec3::new(-0.32, 0.6, -0.13), Vec3::new(0.32, 1.45, 0.13), top, ice, ice);
    add_box3(&mut m, Vec3::new(-0.26, 1.45, -0.11), Vec3::new(0.26, 1.55, 0.11), top, top, top);
    m
}

/// Farben des Ziels je Stil: (Korpus, Kanten, Innenraum, Magnet).
fn goal_colors(style: GoalStyle) -> (Color, Color, Color, Color) {
    match style {
        GoalStyle::Kitchen => (pal::FRIDGE, pal::FRIDGE_DARK, pal::FRIDGE_INSIDE, Color::hex(0xFF5C8A)),
        GoalStyle::Freezer => (Color::hex(0xCFEFFF), Color::hex(0x86B4D2), Color::hex(0xF2FCFF), Color::hex(0x4FC8FF)),
        GoalStyle::Party => (Color::hex(0xFF9CC8), Color::hex(0xD8609A), Color::hex(0xFFF2B8), Color::hex(0xFFE14D)),
        GoalStyle::Bread => (Color::hex(0xC8894A), Color::hex(0x8A5426), Color::hex(0xFFE2A8), Color::hex(0xFFD24A)),
    }
}

/// Ziel (Korpus, Tür) im gewünschten Stil.
pub fn goal(style: GoalStyle) -> (Mesh, Mesh) {
    let (body, dark, inside, magnet) = goal_colors(style);
    (fridge_body(body, dark, inside), fridge_door_mesh(body, dark, magnet))
}

/// Kühlschrank-Korpus (Vorderseite +Z, Breite 2.2, Höhe 3.2).
fn fridge() -> Mesh {
    let (body, dark, inside, _) = goal_colors(GoalStyle::Kitchen);
    fridge_body(body, dark, inside)
}

fn fridge_body(body: Color, dark: Color, inside: Color) -> Mesh {
    let mut m = Mesh::new();
    add_cuboid(
        &mut m,
        Vec3::new(-1.1, 0.12, -0.7),
        Vec3::new(1.1, 3.2, 0.7),
        [body, body, body, dark, inside, body],
    );
    // Füße
    for &(x, z) in &[(-0.9, -0.5), (0.9, -0.5), (-0.9, 0.5), (0.9, 0.5)] {
        add_box(&mut m, Vec3::new(x - 0.1, 0.0, z - 0.1), Vec3::new(x + 0.1, 0.12, z + 0.1), Color::hex(0x55606A));
    }
    // Innen: Regalböden und eine Milchflasche
    for y in [1.0, 1.8, 2.5] {
        add_box(&mut m, Vec3::new(-1.0, y, 0.6), Vec3::new(1.0, y + 0.05, 0.78), Color::hex(0xE0F6FF));
    }
    add_box(&mut m, Vec3::new(0.4, 1.85, 0.58), Vec3::new(0.65, 2.3, 0.78), pal::WHITE);
    add_box(&mut m, Vec3::new(0.46, 2.3, 0.62), Vec3::new(0.59, 2.4, 0.74), Color::hex(0x3080FF));
    add_uv_sphere(&mut m, Vec3::new(-0.5, 1.25, 0.68), 0.2, 6, 4, pal::CHERRY);
    // Deko oben: Riesenkirsche
    add_uv_sphere(&mut m, Vec3::new(-0.5, 3.45, 0.0), 0.26, 8, 5, pal::CHERRY);
    add_segment(&mut m, Vec3::new(-0.5, 3.66, 0.0), Vec3::new(-0.35, 3.95, 0.0), 0.03, 4, pal::STEM);
    m
}

/// Tür (Scharnier bei x = 0, öffnet zur Seite).
fn fridge_door() -> Mesh {
    let (body, dark, _, magnet) = goal_colors(GoalStyle::Kitchen);
    fridge_door_mesh(body, dark, magnet)
}

fn fridge_door_mesh(body: Color, dark: Color, magnet: Color) -> Mesh {
    let mut m = Mesh::new();
    add_box(&mut m, Vec3::new(0.0, 0.16, 0.0), Vec3::new(2.16, 3.16, 0.14), body);
    add_box(&mut m, Vec3::new(0.05, 2.2, 0.14), Vec3::new(2.11, 2.25, 0.16), dark);
    add_box(&mut m, Vec3::new(1.85, 1.2, 0.14), Vec3::new(1.97, 2.05, 0.30), Color::hex(0x8C98A4));
    add_box(&mut m, Vec3::new(1.85, 2.4, 0.14), Vec3::new(1.97, 2.9, 0.30), Color::hex(0x8C98A4));
    // Magnet-Herz :)
    add_box(&mut m, Vec3::new(0.5, 1.4, 0.14), Vec3::new(0.8, 1.7, 0.18), magnet);
    m
}

/// Box-Plattform (Oberkante bei y = 0) mit Kante oben – Glasboden, Butter,
/// Käse, Eisscholle. `size`: Breite (x), Tiefe (z), Höhe.
pub fn block(size: Vec3, side: Color, top: Color) -> Mesh {
    let mut m = Mesh::new();
    let (hx, hz, h) = (size.x * 0.5, size.y * 0.5, size.z);
    let rim = (h * 0.12).min(0.12);
    add_box3(&mut m, Vec3::new(-hx, -h, -hz), Vec3::new(hx, -rim, hz), side, side, side.scale(0.8));
    add_box3(&mut m, Vec3::new(-hx, -rim, -hz), Vec3::new(hx, 0.0, hz), top, top.scale(0.92), top);
    m
}

/// Runde Plattform (Oberkante bei y = 0): Glas mit Deckel, Becher, Dose.
pub fn cyl(r: f32, h: f32, side: Color, top: Color) -> Mesh {
    let mut m = Mesh::new();
    let segs = if r > 1.2 { 14 } else { 10 };
    let lid = (h * 0.15).min(0.2);
    add_cylinder(&mut m, segs, r * 0.96, r * 0.96, -h, -lid, side, None, Some(side.scale(0.8)));
    add_cylinder(&mut m, segs, r, r, -lid, 0.0, top.scale(0.9), Some(top), Some(top.scale(0.8)));
    m
}

/// Deko-Box (Unterkante bei y = 0), z. B. Milchkarton, Eierschachtel.
pub fn prop_box(size: Vec3, c: Color) -> Mesh {
    let mut m = Mesh::new();
    let (hx, hz) = (size.x * 0.5, size.y * 0.5);
    add_box3(&mut m, Vec3::new(-hx, 0.0, -hz), Vec3::new(hx, size.z, hz), c.scale(1.05), c, c.scale(0.8));
    m
}

/// Deko-Zylinder (Unterkante bei y = 0), z. B. Flasche, Eiszapfen-Säule.
pub fn prop_cyl(r: f32, h: f32, c: Color) -> Mesh {
    let mut m = Mesh::new();
    add_cylinder(&mut m, 10, r, r * 0.92, 0.0, h, c, Some(c.scale(1.05)), Some(c.scale(0.8)));
    m
}

/// Tasse warmer Kakao mit Henkel und Sahnehaube (Höhe ~0.55).
fn cocoa() -> Mesh {
    let mut m = Mesh::new();
    add_cylinder(&mut m, 10, 0.22, 0.26, 0.0, 0.42, Color::hex(0xFF6A4A), None, Some(Color::hex(0xC84A30)));
    add_cylinder(&mut m, 10, 0.24, 0.24, 0.38, 0.40, Color::hex(0x6A3418), Some(Color::hex(0x6A3418)), None);
    add_uv_sphere(&mut m, Vec3::new(0.0, 0.44, 0.0), 0.15, 6, 3, pal::CREAM);
    add_segment(&mut m, Vec3::new(0.25, 0.32, 0.0), Vec3::new(0.38, 0.22, 0.0), 0.04, 4, Color::hex(0xFF6A4A));
    add_segment(&mut m, Vec3::new(0.38, 0.22, 0.0), Vec3::new(0.25, 0.10, 0.0), 0.04, 4, Color::hex(0xFF6A4A));
    m
}

fn cloud() -> Mesh {
    let mut m = Mesh::new();
    let c = Color::hex(0xFFF2FA);
    add_uv_sphere(&mut m, Vec3::new(0.0, 0.0, 0.0), 1.0, 7, 4, c);
    add_uv_sphere(&mut m, Vec3::new(1.1, -0.2, 0.2), 0.75, 6, 4, c);
    add_uv_sphere(&mut m, Vec3::new(-1.0, -0.25, -0.1), 0.7, 6, 4, Color::hex(0xFFE4F2));
    add_uv_sphere(&mut m, Vec3::new(0.3, 0.45, -0.3), 0.6, 5, 3, c);
    m
}

/// Cupcake (Höhe ~1.3 bei Skalierung 1).
fn cupcake() -> Mesh {
    let mut m = Mesh::new();
    let mut cup = Mesh::new();
    add_cylinder(&mut cup, 10, 0.42, 0.56, 0.0, 0.55, Color::hex(0x7AD0FF), None, Some(Color::hex(0x5AA8D8)));
    // Streifen: jede zweite Seitenfläche heller
    let mut k = 0;
    cup.recolor(|c, n| {
        k += 1;
        if abs(n.y) < 0.9 && (k / 2) % 2 == 0 { Color::hex(0xB8E8FF) } else { c }
    });
    m.append(&cup, &Mat4::IDENTITY);
    add_cylinder(&mut m, 10, 0.62, 0.48, 0.55, 0.8, pal::FROST, None, Some(pal::FROST));
    add_cylinder(&mut m, 10, 0.48, 0.30, 0.8, 1.02, Color::hex(0xFF9CC8), None, None);
    add_cylinder(&mut m, 10, 0.30, 0.0, 1.02, 1.2, pal::FROST, None, None);
    add_uv_sphere(&mut m, Vec3::new(0.0, 1.3, 0.0), 0.13, 6, 4, pal::CHERRY);
    m
}

/// Lolli: Stiel + Scheibe mit Spiralmuster.
pub fn lolly(h: f32, color: Color) -> Mesh {
    let mut m = Mesh::new();
    add_segment(&mut m, Vec3::ZERO, Vec3::new(0.0, h, 0.0), 0.06, 5, pal::WHITE);
    let r = 0.65;
    let n = 16;
    let mut disc = Mesh::new();
    for (side, z) in [(1.0f32, 0.09f32), (-1.0, -0.09)] {
        let c = disc.vertex(Vec3::new(0.0, 0.0, z));
        let ring: Vec<u16> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * TAU;
                disc.vertex(Vec3::new(cos(a) * r, sin(a) * r, z))
            })
            .collect();
        for i in 0..n {
            let col = if i % 2 == 0 { color } else { pal::WHITE };
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            if side > 0.0 {
                disc.tri(c, a, b, col);
            } else {
                disc.tri(c, b, a, col);
            }
        }
    }
    // Rand
    let front: Vec<u16> = (1..=n as u16).collect();
    let back: Vec<u16> = (n as u16 + 2..=2 * n as u16 + 1).collect();
    for i in 0..n {
        let j = (i + 1) % n;
        disc.quad(front[i], back[i], back[j], front[j], color);
    }
    m.append(&disc, &Mat4::translation(Vec3::new(0.0, h + r * 0.85, 0.0)));
    m
}

/// Zuckerstange (Deko) der Höhe `h`.
pub fn candy_cane(h: f32) -> Mesh {
    let mut m = Mesh::new();
    let r = 0.13;
    let seg = 0.32;
    let n = ((h / seg) as usize).max(1);
    for k in 0..n {
        let c = if k % 2 == 0 { pal::RED } else { pal::WHITE };
        add_segment(&mut m, Vec3::new(0.0, k as f32 * seg, 0.0), Vec3::new(0.0, (k + 1) as f32 * seg, 0.0), r, 7, c);
    }
    let top = n as f32 * seg;
    let hr = 0.38;
    let mut prev = Vec3::new(0.0, top, 0.0);
    for k in 1..=6 {
        let a = k as f32 / 6.0 * PI;
        let p = Vec3::new(hr - hr * cos(a), top + hr * sin(a), 0.0);
        add_segment(&mut m, prev, p, r, 7, if k % 2 == 0 { pal::RED } else { pal::WHITE });
        prev = p;
    }
    m
}

/// Kuchen-Plattform: Biskuit, Cremeschicht, Glasur – plus ein Detail-Mesh
/// (Tropfen, Streusel), das nur aus der Nähe gezeichnet wird (LOD).
/// Ursprung = Mitte der Oberkante.
pub fn cake(size: Vec3, style: CakeStyle, seed: u32) -> (Mesh, Mesh) {
    let (frost, sponge, stripe) = match style {
        CakeStyle::Pink => (Color::hex(0xF78FBF), pal::SPONGE, pal::CREAM),
        CakeStyle::Choco => (Color::hex(0x6B3A22), Color::hex(0x9A6040), pal::CREAM),
        CakeStyle::Mint => (Color::hex(0x86E3C3), Color::hex(0xF7E3B5), Color::hex(0x6B3A22)),
        CakeStyle::Lemon => (Color::hex(0xFFE45C), pal::SPONGE, pal::WHITE),
    };
    let (hw, hd, h) = (size.x * 0.5, size.y * 0.5, size.z);
    let mut m = Mesh::new();
    let ft = 0.3_f32.min(h * 0.3);
    add_box3(&mut m, Vec3::new(-hw, -h, -hd), Vec3::new(hw, -ft, hd), sponge, sponge, Color::hex(0xC8A070));
    let sy = -h * 0.5 - 0.06;
    add_box(&mut m, Vec3::new(-hw - 0.04, sy, -hd - 0.04), Vec3::new(hw + 0.04, sy + 0.14, hd + 0.04), stripe);
    add_box3(&mut m, Vec3::new(-hw - 0.1, -ft, -hd - 0.1), Vec3::new(hw + 0.1, 0.0, hd + 0.1), frost, frost, frost);
    let base = m;
    let mut m = Mesh::new();
    // Glasurtropfen entlang der Kanten.
    let mut k = seed;
    let mut drips = |m: &mut Mesh, from: Vec3, to: Vec3, out: Vec3| {
        let len = (to - from).length();
        let n = ((len / 2.4) as usize).max(1);
        for i in 0..n {
            k = k.wrapping_add(1);
            let t = (i as f32 + 0.5) / n as f32;
            let p = from.lerp(to, t) + out * 0.11;
            let dl = 0.18 + (hash(k, 7) % 100) as f32 / 100.0 * 0.35;
            let w = 0.22 + (hash(k, 9) % 100) as f32 / 100.0 * 0.2;
            let along = (to - from).normalize_or_zero() * (w * 0.5);
            let o = out * 0.05;
            let a = p - along - o;
            let b = p + along + o;
            // Oberseite und Rückseite stecken in Glasur bzw. Kuchen → weglassen.
            let back = if out.x > 0.5 {
                1 << 1
            } else if out.x < -0.5 {
                1 << 0
            } else if out.z > 0.5 {
                1 << 5
            } else {
                1 << 4
            };
            let mask = 0b11_1111 & !(1 << 2) & !back;
            shapes::add_cuboid_faces(
                m,
                Vec3::new(a.x.min(b.x), -ft - dl, a.z.min(b.z)),
                Vec3::new(a.x.max(b.x), -ft + 0.01, a.z.max(b.z)),
                [frost; 6],
                mask,
            );
        }
    };
    let (x0, x1, z0, z1) = (-hw, hw, -hd, hd);
    drips(&mut m, Vec3::new(x0, 0.0, z1), Vec3::new(x1, 0.0, z1), Vec3::Z);
    drips(&mut m, Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), -Vec3::Z);
    drips(&mut m, Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1), Vec3::X);
    drips(&mut m, Vec3::new(x0, 0.0, z0), Vec3::new(x0, 0.0, z1), -Vec3::X);
    // Streusel auf der Oberseite.
    let count = ((size.x * size.y) * 0.12) as u32;
    for i in 0..count {
        let hsh = hash(seed.wrapping_mul(31).wrapping_add(i), 3);
        let fx = (hsh % 1000) as f32 / 1000.0;
        let fz = ((hsh >> 10) % 1000) as f32 / 1000.0;
        let ang = ((hsh >> 20) % 628) as f32 / 100.0;
        let c = pal::SPRINKLES[(hsh >> 5) as usize % 5];
        let p = Vec3::new(-hw + 0.3 + fx * (size.x - 0.6), 0.012, -hd + 0.3 + fz * (size.y - 0.6));
        add_flat_quad(&mut m, p, 0.3, 0.08, ang, c);
    }
    (base, m)
}

/// Runde Keks-Plattform; Schokostückchen als Detail-Mesh. Ursprung = Mitte oben.
pub fn cookie(r: f32, h: f32, seed: u32) -> (Mesh, Mesh) {
    let mut m = Mesh::new();
    add_cylinder(&mut m, 10, r * 0.97, r, -h, -0.12, pal::COOKIE_DARK, None, None);
    add_cylinder(&mut m, 10, r, r * 0.93, -0.12, 0.0, pal::COOKIE, Some(pal::COOKIE), None);
    let mut chips = Mesh::new();
    let n = (r * r * 1.2) as u32 + 2;
    for i in 0..n {
        let hsh = hash(seed.wrapping_add(i * 17), 11);
        let a = (hsh % 628) as f32 / 100.0;
        let d = ((hsh >> 10) % 1000) as f32 / 1000.0 * r * 0.8;
        let p = Vec3::new(cos(a) * d, 0.0, sin(a) * d);
        // Nur die sichtbaren 5 Seiten (Unterseite liegt im Keks).
        let mut chip = Mesh::new();
        add_box(&mut chip, p - Vec3::new(0.12, 0.0, 0.1), p + Vec3::new(0.12, 0.09, 0.1), pal::CHOCO_DARK);
        let keep: Vec<usize> = (0..chip.tri_count()).filter(|&t| chip.normals[t].y > -0.5).collect();
        for t in keep {
            let [a, b, c] = chip.tris[t];
            let (pa, pb, pc) = (chip.positions[a as usize], chip.positions[b as usize], chip.positions[c as usize]);
            let (ia, ib, ic) = (chips.vertex(pa), chips.vertex(pb), chips.vertex(pc));
            chips.tri(ia, ib, ic, pal::CHOCO_DARK);
        }
    }
    (m, chips)
}

/// Schokoriegel: Sockel + erhabene Stückchen. Ursprung = Mitte oben.
pub fn choco_bar(size: Vec3) -> Mesh {
    let (hw, hd, h) = (size.x * 0.5, size.y * 0.5, size.z);
    let mut m = Mesh::new();
    add_box3(
        &mut m,
        Vec3::new(-hw, -h, -hd),
        Vec3::new(hw, -0.12, hd),
        pal::CHOCO_DARK,
        pal::CHOCO_DARK,
        pal::CHOCO_DARK,
    );
    let nx = (round(size.x / 1.5) as usize).max(1);
    let nz = (round(size.y / 1.5) as usize).max(1);
    let (tw, td) = (size.x / nx as f32, size.y / nz as f32);
    for ix in 0..nx {
        for iz in 0..nz {
            let x0 = -hw + ix as f32 * tw + 0.05;
            let z0 = -hd + iz as f32 * td + 0.05;
            add_box3(
                &mut m,
                Vec3::new(x0, -0.13, z0),
                Vec3::new(x0 + tw - 0.1, 0.0, z0 + td - 0.1),
                pal::CHOCO,
                Color::hex(0x6A381E),
                pal::CHOCO,
            );
        }
    }
    m
}

/// Waffel-Plattform (Schichten). Ursprung = Mitte oben.
pub fn wafer(size: Vec3) -> Mesh {
    let (hw, hd, h) = (size.x * 0.5, size.y * 0.5, size.z);
    let mut m = Mesh::new();
    let l = h / 3.0;
    add_box(&mut m, Vec3::new(-hw, -h, -hd), Vec3::new(hw, -2.0 * l, hd), pal::WAFER);
    add_box(&mut m, Vec3::new(-hw + 0.03, -2.0 * l, -hd + 0.03), Vec3::new(hw - 0.03, -l, hd - 0.03), pal::CREAM);
    add_box(&mut m, Vec3::new(-hw, -l, -hd), Vec3::new(hw, 0.0, hd), pal::WAFER);
    // Gittermuster oben
    let n = 4;
    for i in 1..n {
        let x = -hw + size.x * i as f32 / n as f32;
        add_flat_quad(&mut m, Vec3::new(x, 0.01, 0.0), size.y - 0.1, 0.07, PI * 0.5, pal::WAFER_DARK);
        let z = -hd + size.y * i as f32 / n as f32;
        add_flat_quad(&mut m, Vec3::new(0.0, 0.012, z), size.x - 0.1, 0.07, 0.0, pal::WAFER_DARK);
    }
    m
}
