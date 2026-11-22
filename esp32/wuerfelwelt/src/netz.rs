//! Netze je Abschnitt (16³ Blöcke): nur Flächen zwischen einem Block und
//! einem durchsichtigen Nachbarn. Eckpunkte werden geteilt (Farbe gilt je
//! Dreieck); jedes Dreieck bekommt die Umgebungsverdeckung seiner Ecken
//! und eine leichte Farbstreuung je Block – so wirken die Flächen plastisch
//! ohne Texturen.
//!
//! Für die Geschwindigkeit: undurchsichtige Flächen liegen je Richtung in
//! einem eigenen Netz (abgewandte Richtungen fallen beim Zeichnen ganz
//! weg), und das Licht je Richtung ist schon in die Farben eingerechnet
//! (oben hell, Seiten mittel, unten dunkel) – der Renderer tönt nur noch
//! mit dem Tageslicht.

use crate::welt::{ABSCHNITT, Block, Seite, Welt, zufall};
use alloc::vec;
use alloc::vec::Vec;
use spatz::color::Color;
use spatz::math::Vec3;
use spatz::render::Mesh;

/// Netze eines Abschnitts: undurchsichtig je Richtung (Reihenfolge wie
/// [`RICHTUNGEN`]), durchscheinend (Wasser, Glas), selbstleuchtend (Lampen).
#[derive(Default)]
pub struct Abschnittsnetze {
    pub fest: [Mesh; 6],
    pub durch: Mesh,
    pub licht: Mesh,
}

impl Abschnittsnetze {
    /// Alle acht Netze: fest je Richtung, durchscheinend, leuchtend.
    pub fn alle_mut(&mut self) -> [&mut Mesh; 8] {
        let [f0, f1, f2, f3, f4, f5] = &mut self.fest;
        [f0, f1, f2, f3, f4, f5, &mut self.durch, &mut self.licht]
    }
}

/// Flächennormalen der sechs Richtungsnetze.
pub const RICHTUNGEN: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// Eingerechnetes Licht je Richtung (klassische Würfelschattierung).
const RICHTUNGSLICHT: [f32; 6] = [0.80, 0.80, 1.0, 0.52, 0.66, 0.66];

/// Flächen: Normale, Tangenten u, v (u × v = Normale), Ursprung (0/1 je Achse).
const FLAECHEN: [([i32; 3], [i32; 3], [i32; 3], [i32; 3]); 6] = [
    ([1, 0, 0], [0, 1, 0], [0, 0, 1], [1, 0, 0]),
    ([-1, 0, 0], [0, 0, 1], [0, 1, 0], [0, 0, 0]),
    ([0, 1, 0], [0, 0, 1], [1, 0, 0], [0, 1, 0]),
    ([0, -1, 0], [1, 0, 0], [0, 0, 1], [0, 0, 0]),
    ([0, 0, 1], [1, 0, 0], [0, 1, 0], [0, 0, 1]),
    ([0, 0, -1], [0, 1, 0], [1, 0, 0], [0, 0, 0]),
];

const KANTE: usize = (ABSCHNITT + 1) as usize;

/// Teilt Eckpunkte innerhalb eines Netzes (Gitter 17³). Statt die Tabelle
/// je Abschnitt zu leeren, trägt jeder Eintrag die Nummer des Durchlaufs.
struct Ecken {
    /// (Durchlauf << 16) | Eckpunkt-Index.
    index: Vec<u32>,
}

impl Ecken {
    fn new() -> Self {
        Self { index: vec![0; KANTE * KANTE * KANTE] }
    }

    fn ecke(&mut self, lauf: u32, m: &mut Mesh, basis: [i32; 3], p: [i32; 3]) -> u16 {
        let l = [p[0] - basis[0], p[1] - basis[1], p[2] - basis[2]];
        let i = (l[1] as usize * KANTE + l[2] as usize) * KANTE + l[0] as usize;
        let e = self.index[i];
        if e >> 16 == lauf {
            return e as u16;
        }
        let v = m.vertex(Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32));
        self.index[i] = (lauf << 16) | v as u32;
        v
    }
}

/// Wiederverwendbarer Arbeitsspeicher für [`abschnitt`] (keine Zuteilung
/// und kein Leeren großer Tabellen je Abschnitt).
pub struct Bauer {
    ecken: [Ecken; 8],
    lauf: u32,
}

impl Default for Bauer {
    fn default() -> Self {
        Self::new()
    }
}

impl Bauer {
    pub fn new() -> Self {
        Self { ecken: core::array::from_fn(|_| Ecken::new()), lauf: 0 }
    }
}

/// Dreieck mit bekannter Normale (spart Kreuzprodukt und Wurzel).
fn dreieck(m: &mut Mesh, a: u16, b: u16, c: u16, n: Vec3, farbe: Color) {
    m.tris.push([a, b, c]);
    m.normals.push(n);
    m.colors.push(farbe);
}

fn mal(f: u32, k: f32) -> Color {
    let c = |s: u32| (((f >> s) & 0xFF) as f32 * k).clamp(0.0, 255.0) as u32;
    Color::hex((c(16) << 16) | (c(8) << 8) | c(0))
}

/// Ist die Fläche von `b` zum Nachbarn `n` sichtbar?
fn sichtbar(b: Block, n: Block) -> bool {
    match b {
        Block::Wasser => n == Block::Luft,
        Block::Glas => n != Block::Glas && n.durchsichtig(),
        Block::Laub => n != Block::Laub && n.durchsichtig(),
        _ => n.durchsichtig(),
    }
}

/// Eckpunkte eines Teilvielecks von [`grasseite`] (ein Viereck, einmal
/// waagerecht geschnitten: höchstens 6 Ecken je Teil) – ohne Heap.
#[derive(Clone, Copy)]
struct Teil {
    p: [(Vec3, f32); 6],
    n: usize,
}

impl Teil {
    const LEER: Self = Self { p: [(Vec3::ZERO, 0.0); 6], n: 0 };

    fn push(&mut self, e: (Vec3, f32)) {
        self.p[self.n] = e;
        self.n += 1;
    }
}

/// Grünkante der Grasblöcke: Seitenfläche oben grün (22 %), darunter Erde.
fn grasseite(m: &mut Mesh, pts: &[[i32; 3]; 4], aos: &[u32; 4], y: i32, streu: f32, normale: Vec3) {
    let v3 = |q: [i32; 3]| Vec3::new(q[0] as f32, q[1] as f32, q[2] as f32);
    let schnitt = y as f32 + 0.78;
    let (mut unten, mut oben) = (Teil::LEER, Teil::LEER);
    for k in 0..4 {
        let (q, n) = (pts[k], pts[(k + 1) % 4]);
        let (a, b) = (v3(q), v3(n));
        let ao = aos[k] as f32;
        if q[1] > y {
            oben.push((a, ao));
        } else {
            unten.push((a, ao));
        }
        if (q[1] > y) != (n[1] > y) {
            let t = (schnitt - a.y) / (b.y - a.y);
            let p = a + (b - a) * t;
            let ao_p = ao + (aos[(k + 1) % 4] as f32 - ao) * t;
            oben.push((p, ao_p));
            unten.push((p, ao_p));
        }
    }
    for (teil, farbe) in [(unten, Block::Erde.farbe(Seite::Mantel)), (oben, 0x57A13F)] {
        let poly = &teil.p[..teil.n];
        let mut idx = [0u16; 6];
        for (i, (p, _)) in idx.iter_mut().zip(poly) {
            *i = m.vertex(*p);
        }
        for j in 1..poly.len().saturating_sub(1) {
            let s = (poly[0].1 + poly[j].1 + poly[j + 1].1) / 3.0;
            dreieck(m, idx[0], idx[j], idx[j + 1], normale, mal(farbe, streu * (1.0 - 0.14 * s)));
        }
    }
}

/// Netze des Abschnitts (ax, ay, az) bauen.
pub fn abschnitt(w: &Welt, a: (i32, i32, i32), bauer: &mut Bauer) -> Abschnittsnetze {
    let mut netze = Abschnittsnetze::default();
    abschnitt_in(w, a, bauer, &mut netze);
    netze
}

/// Wie [`abschnitt`], aber in vorhandene Netze (werden geleert): Ihre
/// Puffer bleiben erhalten – auf dem ESP32 liegt der Heap im PSRAM, und
/// Hunderte Zuteilungen je Abschnitt kosteten dort mehrere Millisekunden.
pub fn abschnitt_in(w: &Welt, a: (i32, i32, i32), bauer: &mut Bauer, netze: &mut Abschnittsnetze) {
    for m in netze.alle_mut() {
        m.positions.clear();
        m.tris.clear();
        m.normals.clear();
        m.colors.clear();
    }
    let basis = [a.0 * ABSCHNITT, a.1 * ABSCHNITT, a.2 * ABSCHNITT];
    // Geteilte Ecken je Netz: 6 Richtungen, durchscheinend, leuchtend.
    bauer.lauf = (bauer.lauf + 1) & 0xFFFF;
    if bauer.lauf == 0 {
        for e in &mut bauer.ecken {
            e.index.fill(0);
        }
        bauer.lauf = 1;
    }
    let lauf = bauer.lauf;
    let ecken = &mut bauer.ecken;
    for ly in 0..ABSCHNITT {
        for lz in 0..ABSCHNITT {
            for lx in 0..ABSCHNITT {
                let (x, y, z) = (basis[0] + lx, basis[1] + ly, basis[2] + lz);
                let b = w.block(x, y, z);
                if b == Block::Luft {
                    continue;
                }
                let ziel = match b {
                    Block::Wasser | Block::Glas => 1,
                    Block::Lampe => 2,
                    _ => 0,
                };
                let z0 = zufall(x * 31 + y, z * 17 - y, 0x5EED);
                // Laub kräftiger gestreut (wirkt wie Blätterwerk).
                let streu = if b == Block::Laub { 0.76 + 0.42 * z0 } else { 0.93 + 0.14 * z0 };
                for (richtung, &(n, u, v, o)) in FLAECHEN.iter().enumerate() {
                    let nb = w.block(x + n[0], y + n[1], z + n[2]);
                    if !sichtbar(b, nb) {
                        continue;
                    }
                    let seite = match n[1] {
                        1 => Seite::Oben,
                        -1 => Seite::Unten,
                        _ => Seite::Mantel,
                    };
                    let p0 = [x + o[0], y + o[1], z + o[2]];
                    let ecke = |i: i32, j: i32| [p0[0] + u[0] * i + v[0] * j, p0[1] + u[1] * i + v[1] * j, p0[2] + u[2] * i + v[2] * j];
                    let pts = [ecke(0, 0), ecke(1, 0), ecke(1, 1), ecke(0, 1)];
                    // Umgebungsverdeckung je Ecke: Nachbarn vor der Fläche.
                    let vor = [x + n[0], y + n[1], z + n[2]];
                    let voll = |d: [i32; 3]| w.block(vor[0] + d[0], vor[1] + d[1], vor[2] + d[2]).verdeckt() as u32;
                    let ao = |su: i32, sv: i32| {
                        let a = voll([u[0] * su, u[1] * su, u[2] * su]);
                        let b = voll([v[0] * sv, v[1] * sv, v[2] * sv]);
                        let c = voll([u[0] * su + v[0] * sv, u[1] * su + v[1] * sv, u[2] * su + v[2] * sv]);
                        if a == 1 && b == 1 { 3 } else { a + b + c }
                    };
                    let aos = if ziel == 2 { [0; 4] } else { [ao(-1, -1), ao(1, -1), ao(1, 1), ao(-1, 1)] };
                    let farbe = b.farbe(seite);
                    let licht = if ziel == 2 { 1.0 } else { RICHTUNGSLICHT[richtung] };
                    let normale = Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32);
                    if b == Block::Gras && seite == Seite::Mantel {
                        grasseite(&mut netze.fest[richtung], &pts, &aos, y, streu * licht, normale);
                        continue;
                    }
                    let (m, e) = match ziel {
                        0 => (&mut netze.fest[richtung], &mut ecken[richtung]),
                        1 => (&mut netze.durch, &mut ecken[6]),
                        _ => (&mut netze.licht, &mut ecken[7]),
                    };
                    let i = [e.ecke(lauf, m, basis, pts[0]), e.ecke(lauf, m, basis, pts[1]), e.ecke(lauf, m, basis, pts[2]), e.ecke(lauf, m, basis, pts[3])];
                    let hell = |k: [usize; 3]| {
                        let s = (aos[k[0]] + aos[k[1]] + aos[k[2]]) as f32 / 3.0;
                        mal(farbe, licht * streu * (1.0 - 0.14 * s))
                    };
                    // Diagonale entlang der helleren Ecken (keine Streifen).
                    if aos[0] + aos[2] > aos[1] + aos[3] {
                        dreieck(m, i[1], i[2], i[3], normale, hell([1, 2, 3]));
                        dreieck(m, i[1], i[3], i[0], normale, hell([1, 3, 0]));
                    } else {
                        dreieck(m, i[0], i[1], i[2], normale, hell([0, 1, 2]));
                        dreieck(m, i[0], i[2], i[3], normale, hell([0, 2, 3]));
                    }
                }
            }
        }
    }
    for m in netze.alle_mut() {
        m.compute_bounds();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::welt::{GY, NY};

    #[test]
    fn einzelner_block_hat_sechs_flaechen_mit_normalen_nach_aussen() {
        let mut w = Welt::erzeugen(1);
        // Hoch über dem Meer (Ecke der Welt): ein Ziegel allein.
        let (x, y, z) = (3, GY - 3, 3);
        w.setzen(x, y, z, Block::Ziegel);
        let n = abschnitt(&w, (x / 16, y / 16, z / 16), &mut Bauer::new());
        let mitte = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
        for (r, m) in n.fest.iter().enumerate() {
            assert_eq!(m.tris.len(), 2, "eine Fläche je Richtung");
            assert_eq!(m.positions.len(), 4, "Ecken geteilt");
            for (t, nrm) in m.tris.iter().zip(&m.normals) {
                let p = m.positions[t[0] as usize];
                assert!((p - mitte).dot(*nrm) > 0.0, "Normale zeigt nach außen");
                let d = RICHTUNGEN[r];
                assert!((nrm.x - d[0] as f32).abs() + (nrm.y - d[1] as f32).abs() + (nrm.z - d[2] as f32).abs() < 1e-4, "Richtung {r}");
            }
        }
    }

    #[test]
    fn verdeckte_flaechen_fehlen_und_wasser_ist_eigenes_netz() {
        let w = Welt::erzeugen(2);
        let mut b = Bauer::new();
        let mut fest = 0;
        let mut durch = 0;
        for ay in 0..NY {
            let n = abschnitt(&w, (4, ay, 4), &mut b);
            fest += n.fest.iter().map(|m| m.tris.len()).sum::<usize>();
            durch += n.durch.tris.len();
        }
        // Eine 16×16-Säule mit Gelände: weit weniger Flächen als Blöcke.
        assert!(fest > 400 && fest < 6000, "{fest}");
        let rand = (0..NY).map(|ay| abschnitt(&w, (0, ay, 0), &mut b).durch.tris.len()).sum::<usize>();
        assert!(rand > 0 || durch > 0, "Wasser am Rand");
    }

    /// In benutzte Netze gebaut ergibt dasselbe wie frisch gebaut.
    #[test]
    fn wiederverwendete_netze_gleiches_ergebnis() {
        let mut w = Welt::erzeugen(9);
        let mut b = Bauer::new();
        let mut alt = abschnitt(&w, (5, 0, 5), &mut b);
        // Danach ein anderer, voller Abschnitt in dieselben Netze.
        w.setzen(4 * 16 + 3, 20, 4 * 16 + 3, Block::Glas);
        abschnitt_in(&w, (4, 1, 4), &mut b, &mut alt);
        let frisch = abschnitt(&w, (4, 1, 4), &mut Bauer::new());
        for (m, n) in alt.fest.iter().chain([&alt.durch, &alt.licht]).zip(frisch.fest.iter().chain([&frisch.durch, &frisch.licht])) {
            assert_eq!(m.positions, n.positions);
            assert_eq!(m.tris, n.tris);
            assert_eq!(m.normals, n.normals);
            assert_eq!(m.colors, n.colors);
            assert_eq!(m.bounds.radius, n.bounds.radius);
        }
    }

    /// Wiederverwendeter Arbeitsspeicher liefert dasselbe Netz wie frischer.
    #[test]
    fn bauer_wiederverwenden_gleiches_ergebnis() {
        let w = Welt::erzeugen(9);
        let mut b = Bauer::new();
        for a in [(3, 0, 3), (3, 1, 3), (4, 0, 4), (3, 0, 3)] {
            let x = abschnitt(&w, a, &mut b);
            let y = abschnitt(&w, a, &mut Bauer::new());
            for (m, n) in x.fest.iter().zip(&y.fest) {
                assert_eq!(m.positions.len(), n.positions.len());
                assert_eq!(m.tris, n.tris);
            }
        }
    }
}
