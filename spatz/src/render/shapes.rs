//! Prozedurale Grundkörper im Low-Poly-Stil.

use super::mesh::Mesh;
use crate::color::Color;
use crate::math::{TAU, Vec3, cos, sin};

/// Box von `min` bis `max`. Farben in der Reihenfolge +X, -X, +Y, -Y, +Z, -Z.
pub fn cuboid(min: Vec3, max: Vec3, colors: [Color; 6]) -> Mesh {
    let mut m = Mesh::new();
    add_cuboid(&mut m, min, max, colors);
    m.compute_bounds();
    m
}

/// Hängt eine Box an ein bestehendes Mesh an. Die 8 Ecken werden geteilt
/// (Normalen/Farben gelten pro Fläche) → nur 8 statt 24 Vertex-Transformationen.
pub fn add_cuboid(m: &mut Mesh, min: Vec3, max: Vec3, colors: [Color; 6]) {
    add_cuboid_faces(m, min, max, colors, 0b11_1111);
}

/// Wie [`add_cuboid`], aber nur mit den Flächen aus `mask` (Bit 0 = +X,
/// 1 = -X, 2 = +Y, 3 = -Y, 4 = +Z, 5 = -Z). Verdeckte Flächen weglassen spart
/// Dreiecke, z. B. bei Teilen, die in anderen Objekten stecken.
pub fn add_cuboid_faces(m: &mut Mesh, min: Vec3, max: Vec3, colors: [Color; 6], mask: u8) {
    let (a, b) = (min, max);
    // Ecke i: Bit 0 = x, Bit 1 = y, Bit 2 = z (0 = min, 1 = max).
    let mut c = [0u16; 8];
    for (i, idx) in c.iter_mut().enumerate() {
        let p = Vec3::new(
            if i & 1 != 0 { b.x } else { a.x },
            if i & 2 != 0 { b.y } else { a.y },
            if i & 4 != 0 { b.z } else { a.z },
        );
        *idx = m.vertex(p);
    }
    // Jede Fläche gegen den Uhrzeigersinn von außen gesehen.
    let faces = [
        [c[5], c[1], c[3], c[7]], // +X
        [c[0], c[4], c[6], c[2]], // -X
        [c[6], c[7], c[3], c[2]], // +Y
        [c[0], c[1], c[5], c[4]], // -Y
        [c[4], c[5], c[7], c[6]], // +Z
        [c[1], c[0], c[2], c[3]], // -Z
    ];
    for (i, f) in faces.iter().enumerate() {
        if mask & (1 << i) != 0 {
            m.quad(f[0], f[1], f[2], f[3], colors[i]);
        }
    }
}

/// Einfarbige Box mit Mittelpunkt im Ursprung.
pub fn box_centered(size: Vec3, color: Color) -> Mesh {
    let h = size * 0.5;
    cuboid(-h, h, [color; 6])
}

/// Zylinder/Kegelstumpf entlang +Y von `y0` bis `y1`. Radius 0 oben ergibt
/// einen Kegel. `top`/`bottom` = `None` lässt die Deckel weg.
pub fn add_cylinder(
    m: &mut Mesh,
    segments: usize,
    r0: f32,
    r1: f32,
    y0: f32,
    y1: f32,
    side: Color,
    top: Option<Color>,
    bottom: Option<Color>,
) {
    let segs = segments.max(3);
    let ring = |m: &mut Mesh, r: f32, y: f32| -> alloc::vec::Vec<u16> {
        (0..segs)
            .map(|i| {
                let a = i as f32 / segs as f32 * TAU;
                m.vertex(Vec3::new(cos(a) * r, y, -sin(a) * r))
            })
            .collect()
    };
    let lo = ring(m, r0, y0);
    let apex = r1 <= 1e-6;
    if apex {
        let tip = m.vertex(Vec3::new(0.0, y1, 0.0));
        for i in 0..segs {
            let j = (i + 1) % segs;
            m.tri(lo[i], lo[j], tip, side);
        }
    } else {
        let hi = ring(m, r1, y1);
        for i in 0..segs {
            let j = (i + 1) % segs;
            m.quad(lo[i], lo[j], hi[j], hi[i], side);
        }
        if let Some(c) = top {
            for i in 1..segs - 1 {
                m.tri(hi[0], hi[i], hi[i + 1], c);
            }
        }
    }
    if let Some(c) = bottom {
        for i in 1..segs - 1 {
            m.tri(lo[0], lo[i + 1], lo[i], c);
        }
    }
}

/// Zylinder als eigenes Mesh.
pub fn cylinder(segments: usize, radius: f32, height: f32, side: Color, caps: Color) -> Mesh {
    let mut m = Mesh::new();
    add_cylinder(&mut m, segments, radius, radius, 0.0, height, side, Some(caps), Some(caps));
    m.compute_bounds();
    m
}

/// UV-Kugel (facettiert).
pub fn add_uv_sphere(m: &mut Mesh, center: Vec3, radius: f32, segs: usize, rings: usize, color: Color) {
    let segs = segs.max(3);
    let rings = rings.max(2);
    let top = m.vertex(center + Vec3::new(0.0, radius, 0.0));
    let bottom = m.vertex(center - Vec3::new(0.0, radius, 0.0));
    let mut idx = alloc::vec::Vec::with_capacity(segs * (rings - 1));
    for r in 1..rings {
        let phi = r as f32 / rings as f32 * core::f32::consts::PI;
        let (sp, cp) = (sin(phi), cos(phi));
        for s in 0..segs {
            let th = s as f32 / segs as f32 * TAU;
            idx.push(m.vertex(center + Vec3::new(cos(th) * sp, cp, -sin(th) * sp) * radius));
        }
    }
    let at = |r: usize, s: usize| idx[r * segs + (s % segs)];
    for s in 0..segs {
        m.tri(top, at(0, s), at(0, s + 1), color);
        m.tri(bottom, at(rings - 2, s + 1), at(rings - 2, s), color);
    }
    for r in 0..rings - 2 {
        for s in 0..segs {
            m.quad(at(r, s), at(r + 1, s), at(r + 1, s + 1), at(r, s + 1), color);
        }
    }
}

/// Oktaeder (Edelstein-Form), gestreckt in Y.
pub fn gem(radius: f32, height: f32, color_top: Color, color_bottom: Color) -> Mesh {
    let mut m = Mesh::new();
    let t = m.vertex(Vec3::new(0.0, height, 0.0));
    let b = m.vertex(Vec3::new(0.0, -height, 0.0));
    let ring: alloc::vec::Vec<u16> = (0..4)
        .map(|i| {
            let a = i as f32 * TAU / 4.0;
            m.vertex(Vec3::new(cos(a) * radius, 0.0, -sin(a) * radius))
        })
        .collect();
    for i in 0..4 {
        let j = (i + 1) % 4;
        m.tri(ring[i], ring[j], t, color_top);
        m.tri(ring[j], ring[i], b, color_bottom);
    }
    m.compute_bounds();
    m
}

/// Torus in der XZ-Ebene (z. B. Donut).
pub fn add_torus(
    m: &mut Mesh,
    major: f32,
    minor: f32,
    seg_major: usize,
    seg_minor: usize,
    color_fn: &dyn Fn(usize, usize, Vec3) -> Color,
) {
    let mut idx = alloc::vec::Vec::with_capacity(seg_major * seg_minor);
    for i in 0..seg_major {
        let a = i as f32 / seg_major as f32 * TAU;
        let dir = Vec3::new(cos(a), 0.0, -sin(a));
        for j in 0..seg_minor {
            let b = j as f32 / seg_minor as f32 * TAU;
            let p = dir * (major + cos(b) * minor) + Vec3::new(0.0, sin(b) * minor, 0.0);
            idx.push(m.vertex(p));
        }
    }
    let at = |i: usize, j: usize| idx[(i % seg_major) * seg_minor + (j % seg_minor)];
    for i in 0..seg_major {
        for j in 0..seg_minor {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            let pa = m.positions[a as usize];
            let col = color_fn(i, j, pa);
            m.quad(a, b, c, d, col);
        }
    }
}

/// Flache, nach oben zeigende Scheibe (z. B. Schatten).
pub fn disc(segments: usize, radius: f32, color: Color) -> Mesh {
    let mut m = Mesh::new();
    let c = m.vertex(Vec3::ZERO);
    let ring: alloc::vec::Vec<u16> = (0..segments.max(3))
        .map(|i| {
            let a = i as f32 / segments as f32 * TAU;
            m.vertex(Vec3::new(cos(a) * radius, 0.0, -sin(a) * radius))
        })
        .collect();
    for i in 0..ring.len() {
        m.tri(c, ring[i], ring[(i + 1) % ring.len()], color);
    }
    m.compute_bounds();
    m
}

/// Regelmäßiges Gitter in der XZ-Ebene (`n`×`n` Zellen, Kantenlänge `size`),
/// zentriert im Ursprung. Gut für Wasser/Boden mit Vertex-Animation.
pub fn grid(n: usize, size: f32, color: Color) -> Mesh {
    let mut m = Mesh::new();
    let step = size / n as f32;
    let o = -size * 0.5;
    for z in 0..=n {
        for x in 0..=n {
            m.vertex(Vec3::new(o + x as f32 * step, 0.0, o + z as f32 * step));
        }
    }
    let at = |x: usize, z: usize| (z * (n + 1) + x) as u16;
    for z in 0..n {
        for x in 0..n {
            // Diagonale abwechselnd → schönere Facetten.
            if (x + z) % 2 == 0 {
                m.tri(at(x, z), at(x, z + 1), at(x + 1, z + 1), color);
                m.tri(at(x, z), at(x + 1, z + 1), at(x + 1, z), color);
            } else {
                m.tri(at(x, z), at(x, z + 1), at(x + 1, z), color);
                m.tri(at(x + 1, z), at(x, z + 1), at(x + 1, z + 1), color);
            }
        }
    }
    m.compute_bounds();
    m
}

/// Berechnet die Flächennormalen neu (nach Vertex-Animation).
pub fn recompute_normals(m: &mut Mesh) {
    for (t, n) in m.tris.iter().zip(m.normals.iter_mut()) {
        let (a, b, c) = (m.positions[t[0] as usize], m.positions[t[1] as usize], m.positions[t[2] as usize]);
        *n = (b - a).cross(c - a).normalize_or(Vec3::Y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuboid_normals_point_outward() {
        let m = cuboid(Vec3::splat(-1.0), Vec3::splat(1.0), [Color::WHITE; 6]);
        assert_eq!(m.tri_count(), 12);
        for (t, n) in m.tris.iter().zip(&m.normals) {
            let c = (m.positions[t[0] as usize] + m.positions[t[1] as usize] + m.positions[t[2] as usize]) / 3.0;
            assert!(n.dot(c) > 0.0, "Normale zeigt nach innen: {n:?} bei {c:?}");
        }
    }

    #[test]
    fn closed_shapes_have_outward_normals() {
        let mut m = Mesh::new();
        add_cylinder(&mut m, 8, 1.0, 0.5, -1.0, 1.0, Color::WHITE, Some(Color::WHITE), Some(Color::WHITE));
        add_uv_sphere(&mut m, Vec3::new(5.0, 0.0, 0.0), 1.0, 8, 6, Color::WHITE);
        let g = gem(0.5, 0.8, Color::WHITE, Color::WHITE);
        for mesh in [&m, &g] {
            for (t, n) in mesh.tris.iter().zip(&mesh.normals) {
                let c = (mesh.positions[t[0] as usize] + mesh.positions[t[1] as usize] + mesh.positions[t[2] as usize])
                    / 3.0;
                let center = if c.x > 3.0 { Vec3::new(5.0, 0.0, 0.0) } else { Vec3::ZERO };
                assert!(n.dot(c - center) > 0.0, "Normale nach innen bei {c:?}");
            }
        }
    }
}
