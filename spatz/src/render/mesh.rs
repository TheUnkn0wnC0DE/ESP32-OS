//! Dreiecksnetze mit Flächenfarben (Flat-Shading, Low-Poly-Stil) und ein
//! einfacher Asset-Speicher mit Handles.

use crate::color::Color;
use crate::math::{Aabb, Mat4, Sphere, Vec3};
use alloc::vec::Vec;

/// Indiziertes Dreiecksnetz. Normalen und Farben gelten pro Dreieck.
/// Winding: gegen den Uhrzeigersinn = Vorderseite (von außen gesehen).
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub tris: Vec<[u16; 3]>,
    pub normals: Vec<Vec3>,
    pub colors: Vec<Color>,
    /// Umschließende Kugel im Modellraum (für Frustum-Culling).
    pub bounds: Sphere,
    /// Umschließende Box im Modellraum.
    pub aabb: Aabb,
}

impl Mesh {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fügt einen Vertex hinzu und gibt seinen Index zurück.
    #[inline]
    pub fn vertex(&mut self, p: Vec3) -> u16 {
        assert!(self.positions.len() < u16::MAX as usize, "Mesh zu groß für u16-Indizes");
        self.positions.push(p);
        (self.positions.len() - 1) as u16
    }

    /// Fügt ein Dreieck hinzu (Normale wird berechnet).
    pub fn tri(&mut self, a: u16, b: u16, c: u16, color: Color) {
        let (pa, pb, pc) = (self.positions[a as usize], self.positions[b as usize], self.positions[c as usize]);
        let n = (pb - pa).cross(pc - pa).normalize_or(Vec3::Y);
        self.tris.push([a, b, c]);
        self.normals.push(n);
        self.colors.push(color);
    }

    /// Viereck `a b c d` (gegen den Uhrzeigersinn) als zwei Dreiecke.
    pub fn quad(&mut self, a: u16, b: u16, c: u16, d: u16, color: Color) {
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    #[inline]
    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }

    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Berechnet Bounding-Box und -Kugel neu.
    pub fn compute_bounds(&mut self) {
        let mut b = Aabb::EMPTY;
        for &p in &self.positions {
            b.grow(p);
        }
        if b.is_empty() {
            b = Aabb::new(Vec3::ZERO, Vec3::ZERO);
        }
        let c = b.center();
        let r = self.positions.iter().map(|p| p.distance_sq(c)).fold(0.0f32, f32::max);
        self.aabb = b;
        self.bounds = Sphere::new(c, crate::math::sqrt(r));
    }

    /// Hängt ein anderes Mesh (transformiert) an. Normalen werden neu
    /// berechnet, Bounds aktualisiert.
    pub fn append(&mut self, other: &Mesh, m: &Mat4) {
        let base = self.positions.len();
        assert!(base + other.positions.len() < u16::MAX as usize, "Mesh zu groß");
        self.positions.extend(other.positions.iter().map(|&p| m.transform_point(p)));
        let flip = {
            let (a, b, c) = (m.cols[0].xyz(), m.cols[1].xyz(), m.cols[2].xyz());
            a.cross(b).dot(c) < 0.0
        };
        for (t, &col) in other.tris.iter().zip(other.colors.iter()) {
            let (i0, mut i1, mut i2) = (t[0] + base as u16, t[1] + base as u16, t[2] + base as u16);
            if flip {
                core::mem::swap(&mut i1, &mut i2);
            }
            self.tri(i0, i1, i2, col);
        }
        self.compute_bounds();
    }

    /// Kopie mit eingebrannter Transformation.
    pub fn transformed(&self, m: &Mat4) -> Mesh {
        let mut out = Mesh::new();
        out.append(self, m);
        out
    }

    /// Färbt alle Dreiecke über eine Funktion (Farbe, Normale) → Farbe um.
    pub fn recolor(&mut self, mut f: impl FnMut(Color, Vec3) -> Color) {
        for (c, &n) in self.colors.iter_mut().zip(self.normals.iter()) {
            *c = f(*c, n);
        }
    }
}

/// Handle auf ein Mesh im [`MeshStore`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(pub u32);

/// Besitzt alle Meshes; Spiele referenzieren sie über [`MeshId`].
#[derive(Default)]
pub struct MeshStore {
    meshes: Vec<Mesh>,
}

impl MeshStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Nimmt ein Mesh auf (Bounds werden berechnet).
    pub fn add(&mut self, mut mesh: Mesh) -> MeshId {
        mesh.compute_bounds();
        self.meshes.push(mesh);
        MeshId((self.meshes.len() - 1) as u32)
    }

    #[inline(always)]
    pub fn get(&self, id: MeshId) -> &Mesh {
        &self.meshes[id.0 as usize]
    }

    #[inline]
    pub fn get_mut(&mut self, id: MeshId) -> &mut Mesh {
        &mut self.meshes[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }

    /// Gesamtzahl der Dreiecke aller Meshes.
    /// Vertexzahl des größten Meshes (für [`crate::render::Renderer::reserve`]).
    pub fn max_vertices(&self) -> usize {
        self.meshes.iter().map(Mesh::vertex_count).max().unwrap_or(0)
    }

    pub fn total_tris(&self) -> usize {
        self.meshes.iter().map(Mesh::tri_count).sum()
    }
}
