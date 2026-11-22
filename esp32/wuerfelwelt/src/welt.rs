//! Die Welt: eine Insel aus Blöcken (128 × 128 Felder, 48 hoch) mitten im
//! Meer, aus einer Saat erzeugt (Hügel, Berge mit Schnee, Strände, Wälder).
//! Gespeichert werden nur die Änderungen des Spielers.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use spatz::math::{Vec3, floor, sqrt};

pub const GX: i32 = 128;
pub const GZ: i32 = 128;
pub const GY: i32 = 48;
/// Blöcke unterhalb dieser Höhe sind (wo sonst Luft wäre) Wasser.
pub const WASSER: i32 = 14;
/// Kantenlänge eines Abschnitts (eigenes Netz je Abschnitt).
pub const ABSCHNITT: i32 = 16;
pub const NX: i32 = GX / ABSCHNITT;
pub const NY: i32 = GY / ABSCHNITT;
pub const NZ: i32 = GZ / ABSCHNITT;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Block {
    Luft = 0,
    Gras,
    Erde,
    Stein,
    Sand,
    Wasser,
    Stamm,
    Laub,
    Bretter,
    Ziegel,
    Glas,
    Schnee,
    Kies,
    Lampe,
    Fels,
}

const ALLE: [Block; 15] = [
    Block::Luft,
    Block::Gras,
    Block::Erde,
    Block::Stein,
    Block::Sand,
    Block::Wasser,
    Block::Stamm,
    Block::Laub,
    Block::Bretter,
    Block::Ziegel,
    Block::Glas,
    Block::Schnee,
    Block::Kies,
    Block::Lampe,
    Block::Fels,
];

/// Welche Fläche eines Blocks (für die Farbe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seite {
    Oben,
    Unten,
    Mantel,
}

impl Block {
    pub fn aus_u8(v: u8) -> Block {
        ALLE.get(v as usize).copied().unwrap_or(Block::Luft)
    }

    /// Hält den Spieler auf (Kollision, Ziel beim Abbauen).
    pub fn fest(self) -> bool {
        !matches!(self, Block::Luft | Block::Wasser)
    }

    /// Flächen dahinter bleiben sichtbar.
    pub fn durchsichtig(self) -> bool {
        matches!(self, Block::Luft | Block::Wasser | Block::Glas | Block::Laub)
    }

    /// Verdeckt Licht (Umgebungsverdeckung an Ecken).
    pub fn verdeckt(self) -> bool {
        self.fest() && !matches!(self, Block::Glas)
    }

    pub fn name(self) -> &'static str {
        match self {
            Block::Luft => "LUFT",
            Block::Gras => "GRAS",
            Block::Erde => "ERDE",
            Block::Stein => "STEIN",
            Block::Sand => "SAND",
            Block::Wasser => "WASSER",
            Block::Stamm => "BAUMSTAMM",
            Block::Laub => "LAUB",
            Block::Bretter => "BRETTER",
            Block::Ziegel => "ZIEGEL",
            Block::Glas => "GLAS",
            Block::Schnee => "SCHNEE",
            Block::Kies => "KIES",
            Block::Lampe => "LAMPE",
            Block::Fels => "URGESTEIN",
        }
    }

    /// Sekunden zum Abbauen (Urgestein: nie).
    pub fn haerte(self) -> f32 {
        match self {
            Block::Laub | Block::Glas | Block::Lampe => 0.25,
            Block::Gras | Block::Erde | Block::Sand | Block::Schnee | Block::Kies => 0.4,
            Block::Stamm | Block::Bretter => 0.65,
            Block::Stein | Block::Ziegel => 0.9,
            Block::Fels => f32::INFINITY,
            Block::Luft | Block::Wasser => 0.0,
        }
    }

    /// Grundfarbe (0xRRGGBB) der Fläche.
    pub fn farbe(self, s: Seite) -> u32 {
        match (self, s) {
            (Block::Gras, Seite::Oben) => 0x62B347,
            (Block::Gras, Seite::Mantel) => 0x8C7442,
            (Block::Gras, Seite::Unten) | (Block::Erde, _) => 0x8A5E3B,
            (Block::Stein, _) => 0x8D9199,
            (Block::Sand, _) => 0xE2CF8E,
            (Block::Wasser, _) => 0x2E78D2,
            (Block::Stamm, Seite::Mantel) => 0x6E4A2C,
            (Block::Stamm, _) => 0xB08A58,
            (Block::Laub, _) => 0x3C8F3F,
            (Block::Bretter, _) => 0xC28D52,
            (Block::Ziegel, _) => 0xB4513A,
            (Block::Glas, _) => 0xCDEEFF,
            (Block::Schnee, _) => 0xF1F5FA,
            (Block::Kies, _) => 0x9A9289,
            (Block::Lampe, _) => 0xFFE08A,
            (Block::Fels, _) => 0x4A4B52,
            (Block::Luft, _) => 0x000000,
        }
    }
}

/// Getroffener Block eines Strahls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Treffer {
    pub pos: [i32; 3],
    /// Fläche, durch die der Strahl eintrat (dorthin wird gesetzt).
    pub normale: [i32; 3],
    pub block: Block,
}

pub struct Welt {
    b: Vec<u8>,
    pub saat: u32,
    aenderungen: BTreeMap<u32, u8>,
    /// Abschnitte, deren Netz neu gebaut werden muss.
    schmutzig: Vec<bool>,
}

fn idx(x: i32, y: i32, z: i32) -> usize {
    ((y * GZ + z) * GX + x) as usize
}

pub fn abschnitt_index(ax: i32, ay: i32, az: i32) -> usize {
    ((ay * NZ + az) * NX + ax) as usize
}

fn hash(x: i32, z: i32, saat: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (z as u32).wrapping_mul(0xD816_3841) ^ saat.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^ (h >> 15)
}

/// Zufall 0..1 je Gitterpunkt.
pub fn zufall(x: i32, z: i32, saat: u32) -> f32 {
    (hash(x, z, saat) & 0xFFFF) as f32 / 65535.0
}

/// Weiches Wertrauschen 0..1.
fn rauschen(x: f32, z: f32, saat: u32) -> f32 {
    let (x0, z0) = (floor(x), floor(z));
    let (fx, fz) = (x - x0, z - z0);
    let (ix, iz) = (x0 as i32, z0 as i32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sz) = (s(fx), s(fz));
    let a = zufall(ix, iz, saat);
    let b = zufall(ix + 1, iz, saat);
    let c = zufall(ix, iz + 1, saat);
    let d = zufall(ix + 1, iz + 1, saat);
    let o = a + (b - a) * sx;
    let u = c + (d - c) * sx;
    o + (u - o) * sz
}

fn fbm(x: f32, z: f32, saat: u32, oktaven: u32) -> f32 {
    let (mut summe, mut amp, mut f, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..oktaven {
        summe += rauschen(x * f, z * f, saat.wrapping_add(o * 101)) * amp;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    summe / norm
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Welt {
    /// Höhe (Anzahl Blöcke) der Geländesäule bei (x, z).
    pub fn gelaende_hoehe(saat: u32, x: i32, z: i32) -> i32 {
        let (nx, nz) = (x as f32 - (GX as f32 - 1.0) * 0.5, z as f32 - (GZ as f32 - 1.0) * 0.5);
        // Leicht unregelmäßige Küste.
        let d = sqrt(nx * nx + nz * nz) / 60.0 + (fbm(x as f32 / 22.0, z as f32 / 22.0, saat ^ 0x55, 2) - 0.5) * 0.22;
        let insel = 1.0 - smooth(0.55, 1.0, d);
        let berge = fbm(x as f32 / 38.0, z as f32 / 38.0, saat, 4);
        let huegel = fbm(x as f32 / 12.0, z as f32 / 12.0, saat.wrapping_add(7), 2);
        let h = (WASSER - 8) as f32 + insel * (9.5 + berge * berge * 30.0 + huegel * 3.5);
        (h as i32).clamp(2, GY - 8)
    }

    /// Insel aus der Saat erzeugen.
    pub fn erzeugen(saat: u32) -> Self {
        let mut w = Welt {
            b: vec![0u8; (GX * GY * GZ) as usize],
            saat,
            aenderungen: BTreeMap::new(),
            schmutzig: vec![true; (NX * NY * NZ) as usize],
        };
        for z in 0..GZ {
            for x in 0..GX {
                let h = Self::gelaende_hoehe(saat, x, z);
                let oben = h - 1;
                let strand = oben <= WASSER;
                let schnee = oben >= 34 + (zufall(x, z, saat ^ 9) * 3.0) as i32;
                for y in 0..h {
                    let b = if y == 0 {
                        Block::Fels
                    } else if y == oben {
                        if oben < WASSER - 1 {
                            if zufall(x, z, saat ^ 3) < 0.3 { Block::Kies } else { Block::Sand }
                        } else if strand {
                            Block::Sand
                        } else if schnee {
                            Block::Schnee
                        } else if oben >= 30 {
                            Block::Stein
                        } else {
                            Block::Gras
                        }
                    } else if y >= oben - 3 && !schnee && oben < 30 {
                        if strand { Block::Sand } else { Block::Erde }
                    } else {
                        Block::Stein
                    };
                    w.b[idx(x, y, z)] = b as u8;
                }
                for y in h..WASSER {
                    w.b[idx(x, y, z)] = Block::Wasser as u8;
                }
            }
        }
        w.baeume();
        w
    }

    /// Je Zelle aus 6 × 6 Feldern höchstens ein Baum, dichter im Wald.
    fn baeume(&mut self) {
        let s = self.saat ^ 0xBAD5EED;
        for cz in 0..GZ / 6 {
            for cx in 0..GX / 6 {
                let x = cx * 6 + 1 + (zufall(cx, cz, s) * 4.0) as i32;
                let z = cz * 6 + 1 + (zufall(cz, cx, s ^ 1) * 4.0) as i32;
                let wald = fbm(x as f32 / 30.0, z as f32 / 30.0, s ^ 2, 2);
                if zufall(x, z, s ^ 3) > wald * 1.15 - 0.15 {
                    continue;
                }
                let h = Self::gelaende_hoehe(self.saat, x, z);
                if self.block(x, h - 1, z) != Block::Gras {
                    continue;
                }
                let stamm = 4 + (zufall(x, z, s ^ 4) * 3.0) as i32;
                let spitze = h + stamm;
                if spitze + 2 >= GY {
                    continue;
                }
                for y in h..spitze {
                    self.b[idx(x, y, z)] = Block::Stamm as u8;
                }
                for dy in -2..=1i32 {
                    let r: i32 = if dy >= 0 { 1 } else { 2 };
                    for dz in -r..=r {
                        for dx in -r..=r {
                            let ecke = dx.abs() == r && dz.abs() == r;
                            if ecke && (dy == 1 || zufall(x + dx * 7, z + dz * 13 + dy, s ^ 5) < 0.6) {
                                continue;
                            }
                            let (lx, ly, lz) = (x + dx, spitze + dy, z + dz);
                            if self.drin(lx, ly, lz) && self.block(lx, ly, lz) == Block::Luft {
                                self.b[idx(lx, ly, lz)] = Block::Laub as u8;
                            }
                        }
                    }
                }
                self.b[idx(x, spitze, z)] = Block::Laub as u8;
            }
        }
    }

    pub fn drin(&self, x: i32, y: i32, z: i32) -> bool {
        (0..GX).contains(&x) && (0..GY).contains(&y) && (0..GZ).contains(&z)
    }

    /// Block an (x, y, z); außerhalb: Meer bzw. Luft, darunter Urgestein.
    pub fn block(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 {
            return Block::Fels;
        }
        if y >= GY {
            return Block::Luft;
        }
        if !(0..GX).contains(&x) || !(0..GZ).contains(&z) {
            return if y < WASSER { Block::Wasser } else { Block::Luft };
        }
        Block::aus_u8(self.b[idx(x, y, z)])
    }

    /// Block setzen (nur innerhalb der Welt, Urgestein bleibt).
    pub fn setzen(&mut self, x: i32, y: i32, z: i32, b: Block) -> bool {
        if !self.drin(x, y, z) || y == 0 {
            return false;
        }
        let i = idx(x, y, z);
        if self.b[i] == b as u8 {
            return false;
        }
        self.b[i] = b as u8;
        self.aenderungen.insert(i as u32, b as u8);
        self.markieren(x, y, z);
        true
    }

    /// Abschnitt des Blocks (und Nachbarn an der Grenze) neu bauen lassen.
    fn markieren(&mut self, x: i32, y: i32, z: i32) {
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (px, py, pz) = (x + dx, y + dy, z + dz);
                    if self.drin(px, py, pz) {
                        let a = abschnitt_index(px / ABSCHNITT, py / ABSCHNITT, pz / ABSCHNITT);
                        self.schmutzig[a] = true;
                    }
                }
            }
        }
    }

    /// Nächster Abschnitt, der neu gebaut werden muss (und als erledigt markiert).
    pub fn schmutziger_abschnitt(&mut self, nahe: (i32, i32, i32)) -> Option<(i32, i32, i32)> {
        let mut best: Option<(i32, (i32, i32, i32))> = None;
        for az in 0..NZ {
            for ay in 0..NY {
                for ax in 0..NX {
                    if self.schmutzig[abschnitt_index(ax, ay, az)] {
                        let d = (ax - nahe.0).abs() + (ay - nahe.1).abs() + (az - nahe.2).abs();
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, (ax, ay, az)));
                        }
                    }
                }
            }
        }
        let (_, a) = best?;
        self.schmutzig[abschnitt_index(a.0, a.1, a.2)] = false;
        Some(a)
    }

    /// Oberster fester Block über (x, z) (−1, wenn keiner).
    pub fn oberflaeche(&self, x: i32, z: i32) -> i32 {
        (0..GY).rev().find(|&y| self.block(x, y, z).fest()).unwrap_or(-1)
    }

    /// Strahl von `o` in Richtung `d` (normiert) bis `max`: erster fester Block.
    pub fn strahl(&self, o: Vec3, d: Vec3, max: f32) -> Option<Treffer> {
        let mut p = [floor(o.x) as i32, floor(o.y) as i32, floor(o.z) as i32];
        let dir = [d.x, d.y, d.z];
        let org = [o.x, o.y, o.z];
        let mut schritt = [0i32; 3];
        let mut t_max = [f32::INFINITY; 3];
        let mut t_delta = [f32::INFINITY; 3];
        for i in 0..3 {
            if dir[i] > 0.0 {
                schritt[i] = 1;
                t_max[i] = ((p[i] + 1) as f32 - org[i]) / dir[i];
                t_delta[i] = 1.0 / dir[i];
            } else if dir[i] < 0.0 {
                schritt[i] = -1;
                t_max[i] = (org[i] - p[i] as f32) / -dir[i];
                t_delta[i] = -1.0 / dir[i];
            }
        }
        let mut normale = [0i32; 3];
        loop {
            let b = self.block(p[0], p[1], p[2]);
            if b.fest() {
                return Some(Treffer { pos: p, normale, block: b });
            }
            let a = if t_max[0] < t_max[1] {
                if t_max[0] < t_max[2] { 0 } else { 2 }
            } else if t_max[1] < t_max[2] {
                1
            } else {
                2
            };
            if t_max[a] > max {
                return None;
            }
            p[a] += schritt[a];
            t_max[a] += t_delta[a];
            normale = [0; 3];
            normale[a] = -schritt[a];
        }
    }

    /// Änderungen als Bytes: „WW01“, Saat, Anzahl, je Änderung 3 Byte Index + Block.
    pub fn aenderungen_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(12 + 4 * self.aenderungen.len());
        v.extend_from_slice(b"WW01");
        v.extend_from_slice(&self.saat.to_le_bytes());
        v.extend_from_slice(&(self.aenderungen.len() as u32).to_le_bytes());
        for (&i, &b) in &self.aenderungen {
            v.extend_from_slice(&i.to_le_bytes()[..3]);
            v.push(b);
        }
        v
    }

    /// Welt aus gespeicherten Bytes (erzeugt + Änderungen); Rest der Bytes
    /// (z. B. Spielerdaten) wird zurückgegeben.
    pub fn laden(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < 12 || &bytes[..4] != b"WW01" {
            return None;
        }
        let saat = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        let n = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
        let ende = 12 + 4 * n;
        if bytes.len() < ende {
            return None;
        }
        let mut w = Self::erzeugen(saat);
        for e in bytes[12..ende].chunks_exact(4) {
            let i = u32::from_le_bytes([e[0], e[1], e[2], 0]);
            if (i as usize) < w.b.len() {
                w.b[i as usize] = e[3];
                w.aenderungen.insert(i, e[3]);
            }
        }
        Some((w, &bytes[ende..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insel_im_meer_mit_land_und_urgestein() {
        let w = Welt::erzeugen(42);
        assert_eq!(w.block(0, WASSER - 1, 0), Block::Wasser, "Ecke: Meer");
        assert_eq!(w.block(-5, 3, 64), Block::Wasser, "außerhalb: Meer");
        assert_eq!(w.block(64, 0, 64), Block::Fels);
        let mitte = w.oberflaeche(64, 64);
        assert!(mitte > WASSER, "Mitte ist Land ({mitte})");
        let gras = (0..GX).flat_map(|x| (0..GZ).map(move |z| (x, z))).filter(|&(x, z)| w.block(x, w.oberflaeche(x, z), z) == Block::Gras).count();
        assert!(gras > 2000, "viel Wiese ({gras})");
        let staemme = w.b.iter().filter(|&&b| b == Block::Stamm as u8).count();
        assert!(staemme > 100, "Bäume ({staemme})");
    }

    #[test]
    fn gleiche_saat_gleiche_welt() {
        let (a, b) = (Welt::erzeugen(7), Welt::erzeugen(7));
        assert!(a.b == b.b);
        assert!(Welt::erzeugen(8).b != a.b);
    }

    #[test]
    fn strahl_trifft_boden_von_oben() {
        let w = Welt::erzeugen(1);
        let y = w.oberflaeche(64, 64);
        let t = w.strahl(Vec3::new(64.5, y as f32 + 3.5, 64.5), Vec3::new(0.0, -1.0, 0.0), 6.0).expect("Treffer");
        assert_eq!(t.pos, [64, y, 64]);
        assert_eq!(t.normale, [0, 1, 0], "von oben");
        assert!(w.strahl(Vec3::new(64.5, y as f32 + 3.5, 64.5), Vec3::new(0.0, 1.0, 0.0), 6.0).is_none(), "nach oben: Himmel");
    }

    #[test]
    fn aenderungen_werden_gespeichert_und_geladen() {
        let mut w = Welt::erzeugen(3);
        let y = w.oberflaeche(60, 60);
        assert!(w.setzen(60, y + 1, 60, Block::Ziegel));
        assert!(w.setzen(60, y, 60, Block::Luft));
        assert!(!w.setzen(60, 0, 60, Block::Luft), "Urgestein bleibt");
        let mut bytes = w.aenderungen_bytes();
        bytes.extend_from_slice(b"rest");
        let (v, rest) = Welt::laden(&bytes).expect("lesbar");
        assert_eq!(rest, b"rest");
        assert_eq!(v.block(60, y + 1, 60), Block::Ziegel);
        assert_eq!(v.block(60, y, 60), Block::Luft);
        assert!(Welt::laden(b"quatsch").is_none());
    }

    #[test]
    fn setzen_markiert_abschnitte() {
        let mut w = Welt::erzeugen(5);
        while w.schmutziger_abschnitt((0, 0, 0)).is_some() {}
        w.setzen(16, 20, 40, Block::Stein);
        let mut n = 0;
        while w.schmutziger_abschnitt((0, 0, 0)).is_some() {
            n += 1;
        }
        assert_eq!(n, 2, "Grenze bei x = 16: beide Abschnitte");
    }
}
