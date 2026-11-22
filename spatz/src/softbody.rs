//! Weichkörper-Simulation für wabbelige Objekte wie Wackelpudding
//! (XPBD – „Extended Position Based Dynamics“, Macklin/Müller 2016/2019).
//!
//! * **Partikel** mit Masse, verbunden durch nachgiebige Abstands-Bedingungen.
//!   Die Nachgiebigkeit (Kehrwert der Steifigkeit) ist physikalisch definiert
//!   und damit unabhängig von Zeitschritt und Iterationszahl.
//! * **Volumenerhalt** über die geschlossene Hülle: Gelatine ist fast
//!   inkompressibel – wird der Körper gestaucht, wölbt er sich zur Seite, wird
//!   er gestreckt, schnürt er sich ein.
//! * **Unterteilte Zeitschritte** (Substeps) halten die numerische Dämpfung
//!   klein: der Körper schwingt lange und lebendig nach, wie echte Gelatine.
//! * Simulation im **mitbewegten Bezugssystem** (z. B. der Spielfigur):
//!   Beschleunigungen und Drehungen des Systems wirken als Trägheitskräfte –
//!   genau das lässt einen echten Pudding auf dem Teller wackeln.
//! * **Anker** (die Unterseite klebt am Teller) und **Bodenkontakt**.
//!
//! [`LatheCage`] baut das Partikelgitter für Drehkörper (Ringe um eine Achse)
//! und bildet Punkte eines Darstellungs-Meshes glatt darauf ab (kubisch über
//! die Höhe), sodass ein grobes Gitter ein feines Mesh weich verformt.
//!
//! Kein Heap-Zugriff pro Schritt; alles in `f32`, deterministisch.

use crate::math::{Mat4, TAU, Vec3, Vec4, atan2, cos, floor, sin, sqrt};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug)]
struct Edge {
    a: u16,
    b: u16,
    rest: f32,
    inv_rest: f32,
    compliance: f32,
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    i: u16,
    target: Vec3,
}

/// Materialparameter (dürfen sich jeden Schritt ändern).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoftParams {
    /// Unterschritte pro [`SoftBody::step`].
    pub substeps: u32,
    /// Dämpfung der Bewegung relativ zum Bezugssystem (1/s).
    pub damping: f32,
    /// Faktor auf alle Abstands-Nachgiebigkeiten (größer = weicher).
    pub softness: f32,
    /// Nachgiebigkeit der Volumen-Bedingung (0 = inkompressibel).
    pub volume_compliance: f32,
    /// Nachgiebigkeit der Anker, wenn sie nicht fest sind (z. B. in der Luft).
    pub anchor_compliance: f32,
    /// Versteifung bei großer Dehnung (Gelatine-Netzwerke werden bei starker
    /// Verformung deutlich steifer): Steifigkeit × (1 + k · Dehnung²).
    /// Kleine Schwingungen bleiben weich, Extreme werden begrenzt.
    pub stiffening: f32,
}

impl Default for SoftParams {
    /// Abgestimmt auf Gelatine: wenig Dämpfung, 3 Unterschritte (weniger
    /// verschlucken schnelle Schwingungen wie das Nachfedern beim Landen).
    fn default() -> Self {
        Self {
            substeps: 3,
            damping: 0.3,
            softness: 1.0,
            volume_compliance: 0.0,
            anchor_compliance: 1e-3,
            stiffening: 0.0,
        }
    }
}

/// Bewegung des Bezugssystems während eines Schritts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameMotion {
    /// Beschleunigung aller Partikel im System, typischerweise
    /// `Schwerkraft − Beschleunigung des Systems` (Trägheitskraft).
    pub accel: Vec3,
    /// Drehung des Systems um +Y in diesem Schritt (Radiant). Die Partikel
    /// bleiben träge zurück und werden über Anker und Bedingungen mitgezogen.
    pub spin: f32,
    /// Anker fest (auf dem Boden) oder nachgiebig (in der Luft).
    pub anchored: bool,
    /// Bodenhöhe im System: kein Partikel sinkt darunter.
    pub ground: Option<f32>,
}

/// Ein Weichkörper aus Partikeln und Bedingungen.
#[derive(Clone, Debug, Default)]
pub struct SoftBody {
    pub pos: Vec<Vec3>,
    pub vel: Vec<Vec3>,
    /// Ruhelage (Modellraum).
    pub rest: Vec<Vec3>,
    prev: Vec<Vec3>,
    inv_mass: Vec<f32>,
    /// Effektive inverse Masse im aktuellen Schritt (fest verankert = 0).
    w: Vec<f32>,
    edges: Vec<Edge>,
    anchors: Vec<Anchor>,
    anchored: Vec<bool>,
    /// Geschlossene Hülle (Dreiecke, außen gegen den Uhrzeigersinn).
    hull: Vec<[u16; 3]>,
    hull_verts: Vec<u16>,
    grad: Vec<Vec3>,
    /// 6 × Ruhevolumen der Hülle.
    rest_volume6: f32,
    /// Kanten schon für die Ausführung sortiert?
    ordered: bool,
    pub params: SoftParams,
}

impl SoftBody {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fügt ein Partikel hinzu (Masse > 0) und gibt seinen Index zurück.
    pub fn add_particle(&mut self, p: Vec3, mass: f32) -> u16 {
        assert!(self.pos.len() < u16::MAX as usize);
        self.pos.push(p);
        self.vel.push(Vec3::ZERO);
        self.rest.push(p);
        self.prev.push(p);
        self.inv_mass.push(1.0 / mass.max(1e-6));
        self.w.push(0.0);
        self.anchored.push(false);
        self.grad.push(Vec3::ZERO);
        (self.pos.len() - 1) as u16
    }

    pub fn len(&self) -> usize {
        self.pos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Abstands-Bedingung mit Nachgiebigkeit `compliance` (m/N; 0 = starr).
    pub fn add_edge(&mut self, a: u16, b: u16, compliance: f32) {
        let rest = (self.rest[a as usize] - self.rest[b as usize]).length();
        self.edges.push(Edge { a, b, rest, inv_rest: 1.0 / rest.max(1e-6), compliance });
        self.ordered = false;
    }

    /// Sortiert die Abstands-Bedingungen so, dass aufeinanderfolgende keine
    /// gemeinsamen Partikel haben: dann hängt keine Rechnung an der vorigen
    /// und der Prozessor kann mehrere überlappend ausführen (spürbar schneller,
    /// gleiche Physik – nur eine andere Gauss-Seidel-Reihenfolge).
    fn order_edges(&mut self) {
        const WINDOW: usize = 4;
        let mut left = core::mem::take(&mut self.edges);
        let mut out: Vec<Edge> = Vec::with_capacity(left.len());
        while !left.is_empty() {
            let recent = &out[out.len().saturating_sub(WINDOW)..];
            let free = |e: &Edge| recent.iter().all(|r| r.a != e.a && r.a != e.b && r.b != e.a && r.b != e.b);
            let i = left.iter().position(free).unwrap_or(0);
            out.push(left.remove(i));
        }
        self.edges = out;
        self.ordered = true;
    }

    /// Hält Partikel `i` an seiner Ruhelage fest (bzw. zieht es dorthin).
    pub fn add_anchor(&mut self, i: u16) {
        self.anchors.push(Anchor { i, target: self.rest[i as usize] });
        self.anchored[i as usize] = true;
    }

    /// Setzt die geschlossene Hülle für den Volumenerhalt.
    pub fn set_hull(&mut self, tris: Vec<[u16; 3]>) {
        let mut verts: Vec<u16> = tris.iter().flatten().copied().collect();
        verts.sort_unstable();
        verts.dedup();
        self.hull = tris;
        self.hull_verts = verts;
        self.rest_volume6 = volume6(&self.hull, &self.rest);
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Volumen der Hülle (aktuell / Ruhelage).
    pub fn volume_ratio(&self) -> f32 {
        if self.rest_volume6 == 0.0 { 1.0 } else { volume6(&self.hull, &self.pos) / self.rest_volume6 }
    }

    /// Alles zurück in die Ruhelage.
    pub fn reset(&mut self) {
        self.pos.copy_from_slice(&self.rest);
        self.prev.copy_from_slice(&self.rest);
        self.vel.fill(Vec3::ZERO);
    }

    /// Gibt jedem freien Partikel eine Geschwindigkeitsänderung abhängig von
    /// seiner Ruhelage (z. B. ein Stoß, der oben stärker wirkt).
    pub fn push(&mut self, mut dv: impl FnMut(Vec3) -> Vec3) {
        for i in 0..self.pos.len() {
            if !self.anchored[i] {
                self.vel[i] += dv(self.rest[i]);
            }
        }
    }

    /// Ein Zeitschritt `dt` im bewegten Bezugssystem.
    pub fn step(&mut self, dt: f32, m: &FrameMotion) {
        if self.pos.is_empty() || dt <= 0.0 {
            return;
        }
        if !self.ordered {
            self.order_edges();
        }
        // Das System dreht sich unter den Partikeln weg: im System gesehen
        // drehen sie sich um −spin (Trägheit).
        if m.spin != 0.0 {
            let (s, c) = (sin(-m.spin), cos(-m.spin));
            let rot = |v: Vec3| Vec3::new(v.x * c + v.z * s, v.y, v.z * c - v.x * s);
            for (p, v) in self.pos.iter_mut().zip(self.vel.iter_mut()) {
                *p = rot(*p);
                *v = rot(*v);
            }
        }
        for (i, w) in self.w.iter_mut().enumerate() {
            *w = if m.anchored && self.anchored[i] { 0.0 } else { self.inv_mass[i] };
        }
        let p = self.params;
        let n = p.substeps.max(1);
        let h = dt / n as f32;
        let inv_h2 = 1.0 / (h * h);
        let damp = 1.0 / (1.0 + p.damping * h);
        let edge_scale = p.softness * inv_h2;
        for _ in 0..n {
            // Vorhersage.
            for i in 0..self.pos.len() {
                self.prev[i] = self.pos[i];
                if self.w[i] == 0.0 {
                    continue;
                }
                self.vel[i] = (self.vel[i] + m.accel * h) * damp;
                self.pos[i] += self.vel[i] * h;
            }
            // Anker: fest oder nachgiebig zur Ruhelage.
            let a_soft = p.anchor_compliance * inv_h2;
            for a in &self.anchors {
                let i = a.i as usize;
                if self.w[i] == 0.0 {
                    self.pos[i] = a.target;
                } else {
                    let k = self.w[i] / (self.w[i] + a_soft);
                    let d = (a.target - self.pos[i]) * k;
                    self.pos[i] += d;
                }
            }
            // Abstände.
            for e in &self.edges {
                let (ia, ib) = (e.a as usize, e.b as usize);
                let (wa, wb) = (self.w[ia], self.w[ib]);
                let d = self.pos[ia] - self.pos[ib];
                let len2 = d.length_sq();
                if len2 < 1e-18 || wa + wb == 0.0 {
                    continue;
                }
                let len = sqrt(len2);
                // Nachgiebigkeit α/f mit Versteifung f = 1 + k·Dehnung²; der
                // Bruch ist so umgeformt, dass nur eine Division bleibt.
                let strain = (len - e.rest) * e.inv_rest;
                let f = 1.0 + p.stiffening * strain * strain;
                let s = (e.rest - len) * f / (((wa + wb) * f + e.compliance * edge_scale) * len);
                self.pos[ia] += d * (s * wa);
                self.pos[ib] -= d * (s * wb);
            }
            // Volumen.
            if !self.hull.is_empty() {
                self.solve_volume(p.volume_compliance * inv_h2);
            }
            // Boden.
            if let Some(g) = m.ground {
                for i in 0..self.pos.len() {
                    if self.pos[i].y < g {
                        self.pos[i].y = g;
                        // Haftreibung: am Boden rutscht Gelatine kaum.
                        let pr = self.prev[i];
                        self.pos[i].x = pr.x + (self.pos[i].x - pr.x) * 0.3;
                        self.pos[i].z = pr.z + (self.pos[i].z - pr.z) * 0.3;
                    }
                }
            }
            // Geschwindigkeiten aus den Lageänderungen.
            let inv = 1.0 / h;
            for i in 0..self.pos.len() {
                self.vel[i] = if self.w[i] == 0.0 { Vec3::ZERO } else { (self.pos[i] - self.prev[i]) * inv };
            }
        }
    }

    fn solve_volume(&mut self, alpha: f32) {
        for &i in &self.hull_verts {
            self.grad[i as usize] = Vec3::ZERO;
        }
        let mut v6 = 0.0;
        for t in &self.hull {
            let (a, b, c) = (self.pos[t[0] as usize], self.pos[t[1] as usize], self.pos[t[2] as usize]);
            v6 += a.dot(b.cross(c));
            self.grad[t[0] as usize] += b.cross(c);
            self.grad[t[1] as usize] += c.cross(a);
            self.grad[t[2] as usize] += a.cross(b);
        }
        let mut wsum = 0.0;
        for &i in &self.hull_verts {
            wsum += self.w[i as usize] * self.grad[i as usize].length_sq();
        }
        if wsum + alpha < 1e-12 {
            return;
        }
        let s = (self.rest_volume6 - v6) / (wsum + alpha);
        for &i in &self.hull_verts {
            let i = i as usize;
            self.pos[i] += self.grad[i] * (s * self.w[i]);
        }
    }
}

/// 6 × Volumen einer geschlossenen, nach außen orientierten Hülle.
fn volume6(tris: &[[u16; 3]], pos: &[Vec3]) -> f32 {
    tris.iter().map(|t| pos[t[0] as usize].dot(pos[t[1] as usize].cross(pos[t[2] as usize]))).sum()
}

/// Partikelgitter eines Drehkörpers: pro Höhenstufe ein Mittelpunkt auf der
/// Achse und `n` Ringpunkte, alle Prismen zwischen zwei Stufen vollständig
/// ausgesteift (Ringe, Speichen, Senkrechte, Kreuzstreben auf Außen- und
/// Radialflächen). Die Außenhaut dient als Hülle für den Volumenerhalt.
#[derive(Clone, Debug)]
pub struct LatheCage {
    first: u16,
    n: usize,
    levels: Vec<(f32, f32)>,
}

/// Vorberechnete Abbildung eines Punkts auf das Gitter ([`LatheCage::map`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct CageBinding {
    lv: [u16; 4],
    w: [f32; 4],
    i0: u16,
    i1: u16,
    a: f32,
    b: f32,
    delta: Vec3,
}

impl LatheCage {
    /// Baut das Gitter in `body`. `levels` = (Höhe, Radius) von unten nach
    /// oben (mindestens 2), `n` Ringpunkte, Masse pro Partikel, Nachgiebigkeit
    /// aller Gitterkanten. Winkel wie bei den Drehkörpern der Engine: Punkt 0
    /// auf +X, dann Richtung −Z.
    pub fn build(body: &mut SoftBody, levels: &[(f32, f32)], n: usize, mass: f32, compliance: f32) -> Self {
        assert!(levels.len() >= 2 && n >= 3);
        let first = body.len() as u16;
        for &(y, r) in levels {
            body.add_particle(Vec3::new(0.0, y, 0.0), mass);
            for i in 0..n {
                let a = i as f32 / n as f32 * TAU;
                body.add_particle(Vec3::new(cos(a) * r, y, -sin(a) * r), mass);
            }
        }
        let cage = Self { first, n, levels: levels.to_vec() };
        let (c, r) = (|l: usize| cage.center(l), |l: usize, i: usize| cage.node(l, i % n));
        for l in 0..levels.len() {
            for i in 0..n {
                body.add_edge(r(l, i), r(l, i + 1), compliance);
                body.add_edge(c(l), r(l, i), compliance);
            }
            if l + 1 == levels.len() {
                continue;
            }
            body.add_edge(c(l), c(l + 1), compliance);
            for i in 0..n {
                body.add_edge(r(l, i), r(l + 1, i), compliance);
                body.add_edge(r(l, i), r(l + 1, i + 1), compliance);
                body.add_edge(r(l, i + 1), r(l + 1, i), compliance);
                body.add_edge(c(l), r(l + 1, i), compliance);
                body.add_edge(c(l + 1), r(l, i), compliance);
            }
        }
        let top = levels.len() - 1;
        let mut hull = Vec::with_capacity(2 * n * levels.len());
        for i in 0..n {
            hull.push([c(0), r(0, i + 1), r(0, i)]);
            hull.push([c(top), r(top, i), r(top, i + 1)]);
            for l in 0..top {
                hull.push([r(l, i), r(l, i + 1), r(l + 1, i + 1)]);
                hull.push([r(l, i), r(l + 1, i + 1), r(l + 1, i)]);
            }
        }
        body.set_hull(hull);
        cage
    }

    /// Partikelindex des Mittelpunkts der Stufe `l`.
    #[inline]
    pub fn center(&self, l: usize) -> u16 {
        self.first + (l * (self.n + 1)) as u16
    }

    /// Partikelindex des Ringpunkts `i` der Stufe `l`.
    #[inline]
    pub fn node(&self, l: usize, i: usize) -> u16 {
        self.first + (l * (self.n + 1) + 1 + i) as u16
    }

    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    pub fn ring_size(&self) -> usize {
        self.n
    }

    /// Bindet einen Punkt (Modellraum, Ruhelage) an das Gitter: Winkelsektor,
    /// kubische Gewichte über die Höhe (Catmull-Rom), Radialanteile.
    pub fn bind(&self, body: &SoftBody, p: Vec3) -> CageBinding {
        let n = self.n;
        let lv = &self.levels;
        let sector = TAU / n as f32;
        let mut ang = atan2(-p.z, p.x);
        if ang < 0.0 {
            ang += TAU;
        }
        let s = (floor(ang / sector) as usize).min(n - 1);
        let (a0, a1) = (s as f32 * sector, (s + 1) as f32 * sector);
        let (d0, d1) = (Vec3::new(cos(a0), 0.0, -sin(a0)), Vec3::new(cos(a1), 0.0, -sin(a1)));
        // Höhenabschnitt (außerhalb wird linear fortgesetzt).
        let last = lv.len() - 2;
        let k = lv.iter().take(last + 1).rposition(|&(y, _)| y <= p.y).unwrap_or(0);
        let t = (p.y - lv[k].0) / (lv[k + 1].0 - lv[k].0);
        let idx = |j: isize| (k as isize + j).clamp(0, lv.len() as isize - 1) as u16;
        let (t2, t3) = (t * t, t * t * t);
        let w = [
            0.5 * (-t3 + 2.0 * t2 - t),
            0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
            0.5 * (-3.0 * t3 + 4.0 * t2 + t),
            0.5 * (t3 - t2),
        ];
        let levels = [idx(-1), idx(0), idx(1), idx(2)];
        let radius: f32 = (0..4).map(|j| w[j] * lv[levels[j] as usize].1).sum();
        // p_xz = radius · (a·d0 + b·d1)
        let (px, pz) = (p.x / radius.max(1e-6), p.z / radius.max(1e-6));
        let det = d0.x * d1.z - d0.z * d1.x;
        let a = (px * d1.z - pz * d1.x) / det;
        let b = (d0.x * pz - d0.z * px) / det;
        let mut bind = CageBinding { lv: levels, w, i0: s as u16, i1: ((s + 1) % n) as u16, a, b, delta: Vec3::ZERO };
        bind.delta = p - self.map(&body.rest, &bind);
        bind
    }

    /// Lage eines gebundenen Punkts für die Partikellagen `pos`.
    #[inline]
    pub fn map(&self, pos: &[Vec3], b: &CageBinding) -> Vec3 {
        let mut out = b.delta;
        for j in 0..4 {
            let l = b.lv[j] as usize;
            let c = pos[self.center(l) as usize];
            let n0 = pos[self.node(l, b.i0 as usize) as usize];
            let n1 = pos[self.node(l, b.i1 as usize) as usize];
            out += (c + (n0 - c) * b.a + (n1 - c) * b.b) * b.w[j];
        }
        out
    }

    /// Bindungen für einen Anbaupunkt samt lokalem Rahmen (siehe [`LatheCage::frame`]).
    pub fn bind_frame(&self, body: &SoftBody, p: Vec3) -> [CageBinding; 4] {
        const E: f32 = 0.04;
        [
            self.bind(body, p),
            self.bind(body, p + Vec3::new(E, 0.0, 0.0)),
            self.bind(body, p + Vec3::new(0.0, E, 0.0)),
            self.bind(body, p + Vec3::new(0.0, 0.0, E)),
        ]
    }

    /// Affine Abbildung, die die Umgebung eines Anbaupunkts mitverformt
    /// (Ursprung = Punkt in Ruhelage → aktuelle Lage, Achsen = lokale
    /// Verzerrung). Augen, Mund usw. kleben so auf der Oberfläche.
    pub fn frame(&self, pos: &[Vec3], b: &[CageBinding; 4]) -> Mat4 {
        const INV_E: f32 = 1.0 / 0.04;
        let o = self.map(pos, &b[0]);
        let x = (self.map(pos, &b[1]) - o) * INV_E;
        let y = (self.map(pos, &b[2]) - o) * INV_E;
        let z = (self.map(pos, &b[3]) - o) * INV_E;
        Mat4 {
            cols: [Vec4::from_vec3(x, 0.0), Vec4::from_vec3(y, 0.0), Vec4::from_vec3(z, 0.0), Vec4::from_vec3(o, 1.0)],
        }
    }
}

/// Frequenz (Hz) und Abklingen (Maximum der zweiten Hälfte / der ersten) einer
/// um 0 schwingenden Messreihe – zum Abstimmen von Materialwerten und für Tests.
pub fn measure_oscillation(samples: &[f32], dt: f32) -> (f32, f32) {
    let mut crossings = 0;
    let mut first = None;
    let mut last = 0;
    for i in 1..samples.len() {
        if (samples[i - 1] < 0.0) != (samples[i] < 0.0) {
            crossings += 1;
            first.get_or_insert(i);
            last = i;
        }
    }
    let freq = match first {
        Some(f) if crossings > 1 => (crossings - 1) as f32 / 2.0 / ((last - f) as f32 * dt),
        _ => 0.0,
    };
    let half = samples.len() / 2;
    let peak = |s: &[f32]| s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let decay = peak(&samples[half..]) / peak(&samples[..half]).max(1e-9);
    (freq, decay)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pudding_like() -> (SoftBody, LatheCage) {
        let mut b = SoftBody::new();
        let cage = LatheCage::build(&mut b, &[(0.0, 0.45), (0.28, 0.45), (0.56, 0.41), (0.84, 0.30)], 8, 1.0, 2e-4);
        for i in 0..=8 {
            b.add_anchor(cage.first + i as u16);
        }
        (b, cage)
    }

    #[test]
    fn cage_mapping_reproduces_rest_shape() {
        let (b, cage) = pudding_like();
        for &(x, y, z) in &[(0.3, 0.1, -0.2), (-0.41, 0.5, 0.05), (0.0, 0.84, 0.0), (0.2, 0.7, 0.25)] {
            let p = Vec3::new(x, y, z);
            let bind = cage.bind(&b, p);
            assert!((cage.map(&b.rest, &bind) - p).length() < 1e-5);
            // Verschiebt sich das ganze Gitter, verschiebt sich der Punkt mit.
            let moved: Vec<Vec3> = b.rest.iter().map(|&q| q + Vec3::new(0.3, -0.1, 0.2)).collect();
            assert!((cage.map(&moved, &bind) - (p + Vec3::new(0.3, -0.1, 0.2))).length() < 1e-5);
        }
        let f = cage.frame(&b.rest, &cage.bind_frame(&b, Vec3::new(0.1, 0.4, -0.3)));
        assert!((f.cols[0].x - 1.0).abs() < 1e-3 && (f.cols[1].y - 1.0).abs() < 1e-3 && f.cols[2].x.abs() < 1e-3);
    }

    #[test]
    fn jelly_wobbles_keeps_volume_and_settles() {
        let (mut b, cage) = pudding_like();
        let top = cage.center(3) as usize;
        let rest_v = b.volume_ratio();
        assert!((rest_v - 1.0).abs() < 1e-6);
        b.push(|r| Vec3::new(1.5 * r.y, 0.0, 0.0));
        let dt = 1.0 / 120.0;
        let still = FrameMotion { anchored: true, ground: Some(0.0), ..FrameMotion::default() };
        let mut xs = Vec::new();
        let mut worst = 0.0f32;
        for _ in 0..240 {
            b.step(dt, &still);
            xs.push(b.pos[top].x);
            worst = worst.max((b.volume_ratio() - 1.0).abs());
        }
        let (freq, decay) = measure_oscillation(&xs, dt);
        assert!(freq > 1.5 && freq < 12.0, "Schwankfrequenz {freq} Hz");
        assert!(decay < 0.9, "muss abklingen ({decay})");
        assert!(worst < 0.05, "Volumen muss erhalten bleiben ({worst})");
        for _ in 0..1200 {
            b.step(dt, &still);
        }
        assert!((b.pos[top] - b.rest[top]).length() < 0.01, "kommt zur Ruhe");
    }

    #[test]
    fn squash_bulges_sideways() {
        let (mut b, cage) = pudding_like();
        let mid = cage.node(1, 0) as usize;
        let r0 = b.rest[mid].x;
        // Landung: alles bewegt sich schlagartig nach unten.
        b.push(|r| Vec3::new(0.0, -3.0 * (r.y / 0.84), 0.0));
        let m = FrameMotion { anchored: true, ground: Some(0.0), ..FrameMotion::default() };
        let mut max_r = r0;
        for _ in 0..30 {
            b.step(1.0 / 120.0, &m);
            max_r = max_r.max(b.pos[mid].x);
        }
        assert!(max_r > r0 * 1.02, "Bauch wölbt sich nach außen ({r0} → {max_r})");
    }

    /// Gelatine-Charakter mit den Standardwerten: schwankt mit 2–4 Hz und
    /// lange nach, federt in der Höhe schneller (fast inkompressibel).
    #[test]
    fn gelatin_character() {
        let run = |kick: &dyn Fn(Vec3) -> Vec3, probe: &dyn Fn(&SoftBody, &LatheCage) -> f32| {
            let mut b = SoftBody::new();
            let cage =
                LatheCage::build(&mut b, &[(0.0, 0.445), (0.28, 0.452), (0.56, 0.41), (0.84, 0.30)], 6, 1.0, 2.5e-4);
            for i in 0..=6 {
                b.add_anchor(cage.first + i as u16);
            }
            let m = FrameMotion { anchored: true, ground: Some(0.0), accel: Vec3::new(0.0, -11.0, 0.0), spin: 0.0 };
            for _ in 0..600 {
                b.step(1.0 / 120.0, &m);
            }
            let base = probe(&b, &cage);
            b.push(kick);
            let xs: Vec<f32> = (0..240)
                .map(|_| {
                    b.step(1.0 / 120.0, &m);
                    probe(&b, &cage) - base
                })
                .collect();
            measure_oscillation(&xs, 1.0 / 120.0)
        };
        let sway = run(&|r| Vec3::new(r.y, 0.0, 0.0), &|b, c| b.pos[c.center(3) as usize].x);
        let vert = run(&|r| Vec3::new(0.0, -2.0 * r.y, 0.0), &|b, c| b.pos[c.center(3) as usize].y);
        assert!(sway.0 > 2.0 && sway.0 < 4.0, "Schwanken {sway:?}");
        assert!(sway.1 > 0.4, "schwingt lange nach {sway:?}");
        assert!(vert.0 > sway.0 * 1.5, "Höhe federt schneller {vert:?}");
    }

    #[test]
    fn spin_twists_the_top() {
        let (mut b, cage) = pudding_like();
        let top = cage.node(3, 0) as usize;
        let m = FrameMotion { spin: 0.2, anchored: true, ground: Some(0.0), ..FrameMotion::default() };
        b.step(1.0 / 120.0, &m);
        // Die Oberseite bleibt hinter der Drehung zurück (dreht sich im System negativ).
        assert!(b.pos[top].z > 0.01, "{:?}", b.pos[top]);
    }
}
