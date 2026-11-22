//! Würfelwelt: eine Insel aus Blöcken zum Erkunden und Bauen – komplett in
//! Rust auf dem eigenen 3D-Renderer der Konsole. Tag und Nacht, Wasser,
//! Wälder, Berge mit Schnee; Blöcke abbauen und setzen, fliegen.

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod himmel;
pub mod netz;
pub mod spieler;
pub mod steuerung;
pub mod welt;

use alloc::vec::Vec;
use konsole::{Spiel, Wechsel};
use pudding_core::app::{FrameData, Perf};
use pudding_core::audio::{AudioCmd, Sfx};
use pudding_core::gfx::{Canvas, hex};
use pudding_core::input::Input;
use spatz::color::Color;
use spatz::math::{Mat4, Vec3, cos, floor, sin};
use spatz::render::{Camera, DrawList, DrawMode, DrawParams, Mesh, MeshId, MeshStore};
use spieler::{Absicht, Spieler};
use steuerung::{SLOTS, Steuerung, slot_rechteck};
use welt::{ABSCHNITT, Block, GX, GZ, NX, NY, NZ, Seite, Treffer, WASSER, Welt, abschnitt_index};

pub const W: usize = 640;
pub const H: usize = 360;
const REICHWEITE: f32 = 6.0;

/// Blöcke der Leiste.
pub const LEISTE: [Block; SLOTS] = [
    Block::Erde,
    Block::Stein,
    Block::Bretter,
    Block::Ziegel,
    Block::Glas,
    Block::Sand,
    Block::Stamm,
    Block::Laub,
    Block::Lampe,
];

struct Partikel {
    pos: Vec3,
    vel: Vec3,
    leben: f32,
    farbe: Color,
}

pub struct Wuerfelwelt {
    welt: Welt,
    store: MeshStore,
    /// Je Abschnitt: 6 Richtungsnetze, durchscheinend, leuchtend.
    netze: Vec<[MeshId; 8]>,
    meer: MeshId,
    meer_boden: MeshId,
    list: DrawList,
    spieler: Spieler,
    st: Steuerung,
    zeit: f32,
    auswahl: usize,
    ziel: Option<Treffer>,
    abbau: f32,
    abbau_pos: [i32; 3],
    partikel: Vec<Partikel>,
    ton: Vec<AudioCmd>,
    geaendert: bool,
    speicher_t: f32,
    muss_speichern: bool,
    name_t: f32,
    schritt_weg: f32,
    bauer: netz::Bauer,
    /// Abschnitt des zuletzt geänderten Blocks (wird zuerst neu gebaut).
    zuletzt: Option<(i32, i32, i32)>,
    /// Entwicklung/Messung: dreht sich langsam, setzt und baut fortlaufend ab.
    pub autopilot: bool,
    autopilot_t: f32,
}

fn hex_farbe(c: u32) -> Color {
    Color::hex(c)
}

/// Startpunkt: freie Wiese etwas oberhalb des Meeres, ~30 Felder von der
/// Inselmitte, Blick zur Mitte (Hügel, Berge, Wald im Bild).
fn startpunkt(w: &Welt) -> (Vec3, f32) {
    let (mx, mz) = (GX / 2, GZ / 2);
    for r in [30, 26, 34, 22, 38, 18, 14, 10] {
        for k in 0..24 {
            let a = k as f32 / 24.0 * core::f32::consts::TAU;
            let (x, z) = (mx + (cos(a) * r as f32) as i32, mz + (sin(a) * r as f32) as i32);
            let y = w.oberflaeche(x, z);
            if w.block(x, y, z) != Block::Gras || !(WASSER + 3..WASSER + 14).contains(&y) {
                continue;
            }
            let frei = (-3..=3).all(|dz| (-3..=3).all(|dx| (1..6).all(|dy| !w.block(x + dx, y + dy, z + dz).fest())));
            if frei {
                let (dx, dz) = ((mx - x) as f32, (mz - z) as f32);
                return (Vec3::new(x as f32 + 0.5, (y + 1) as f32, z as f32 + 0.5), spatz::math::atan2(-dx, -dz));
            }
        }
    }
    (Vec3::new(mx as f32 + 0.5, (w.oberflaeche(mx, mz) + 1) as f32, mz as f32 + 0.5), 0.0)
}

/// Meer rund um die Insel (Oberfläche durchscheinend) und Meeresboden.
fn meer_netze() -> (Mesh, Mesh) {
    let mut boden = Mesh::new();
    let mut oben = Mesh::new();
    let r = 400.0;
    let (gx, gz) = (GX as f32, GZ as f32);
    // Vier Rechtecke um das Weltquadrat.
    let teile = [(-r, -r, gx + r, 0.0), (-r, gz, gx + r, gz + r), (-r, 0.0, 0.0, gz), (gx, 0.0, gx + r, gz)];
    for (m, y, farbe) in [(&mut boden, 6.0, 0xC9B880u32), (&mut oben, WASSER as f32, 0x2E78D2)] {
        for &(x0, z0, x1, z1) in &teile {
            let a = m.vertex(Vec3::new(x0, y, z0));
            let b = m.vertex(Vec3::new(x0, y, z1));
            let c = m.vertex(Vec3::new(x1, y, z1));
            let d = m.vertex(Vec3::new(x1, y, z0));
            m.quad(a, b, c, d, hex_farbe(farbe));
        }
        m.compute_bounds();
    }
    (boden, oben)
}

impl Wuerfelwelt {
    /// Neue Welt oder gespeicherter Stand (Bytes aus `speichern`).
    pub fn neu(spielstand: Option<&[u8]>) -> Self {
        let geladen = spielstand.and_then(Welt::laden);
        let (welt, rest) = match geladen {
            Some((w, rest)) => (w, Some(rest)),
            None => (Welt::erzeugen(0x5EED_2026), None),
        };
        let mut store = MeshStore::new();
        let mut netze = Vec::with_capacity((NX * NY * NZ) as usize);
        for _ in 0..NX * NY * NZ {
            netze.push(core::array::from_fn(|_| store.add(Mesh::new())));
        }
        let (boden, oben) = meer_netze();
        let meer_boden = store.add(boden);
        let meer = store.add(oben);
        let mut s = Self {
            spieler: Spieler::new(Vec3::ZERO),
            welt,
            store,
            netze,
            meer,
            meer_boden,
            list: DrawList::new(),
            st: Steuerung::new(),
            zeit: 0.32,
            auswahl: 2,
            ziel: None,
            abbau: 0.0,
            abbau_pos: [0; 3],
            partikel: Vec::new(),
            ton: alloc::vec![AudioCmd::Song(None)],
            geaendert: false,
            speicher_t: 0.0,
            muss_speichern: false,
            name_t: 0.0,
            schritt_weg: 0.0,
            bauer: netz::Bauer::new(),
            zuletzt: None,
            autopilot: false,
            autopilot_t: 0.0,
        };
        let (pos, yaw) = startpunkt(&s.welt);
        s.spieler.pos = pos;
        s.spieler.yaw = yaw;
        if let Some(r) = rest {
            s.spieler_laden(r);
        }
        s.netze_bauen(usize::MAX);
        s
    }

    fn spieler_laden(&mut self, r: &[u8]) {
        let f = |i: usize| r.get(i * 4..i * 4 + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if let (Some(x), Some(y), Some(z), Some(yaw), Some(pitch), Some(zeit)) = (f(0), f(1), f(2), f(3), f(4), f(5)) {
            self.spieler.pos = Vec3::new(x, y, z);
            self.spieler.yaw = yaw;
            self.spieler.pitch = pitch;
            self.zeit = zeit - floor(zeit);
            if let Some(&a) = r.get(24) {
                self.auswahl = (a as usize).min(SLOTS - 1);
            }
            if r.get(25) == Some(&1) {
                self.spieler.fliegt = true;
                self.st.fliegt = true;
            }
        }
    }

    /// Bis zu `max` geänderte Abschnitte neu vernetzen (nächste zuerst).
    fn netze_bauen(&mut self, max: usize) {
        let p = self.spieler.pos;
        let nahe = self.zuletzt.unwrap_or(((p.x as i32) / ABSCHNITT, (p.y as i32) / ABSCHNITT, (p.z as i32) / ABSCHNITT));
        for _ in 0..max {
            let Some(a) = self.welt.schmutziger_abschnitt(nahe) else {
                self.zuletzt = None;
                break;
            };
            // In die bisherigen Netze bauen (deren Puffer bleiben erhalten).
            let ids = self.netze[abschnitt_index(a.0, a.1, a.2)];
            let mut n = netz::Abschnittsnetze::default();
            for (m, id) in n.alle_mut().into_iter().zip(&ids) {
                *m = core::mem::take(self.store.get_mut(*id));
            }
            netz::abschnitt_in(&self.welt, a, &mut self.bauer, &mut n);
            for (m, id) in n.alle_mut().into_iter().zip(&ids) {
                *self.store.get_mut(*id) = core::mem::take(m);
            }
        }
    }

    pub fn welt(&self) -> &Welt {
        &self.welt
    }

    pub fn spieler(&self) -> &Spieler {
        &self.spieler
    }

    /// Simulation: Tageszeit setzen (Bilder für Tests/Vorschau).
    pub fn setze_zeit(&mut self, zeit: f32) {
        self.zeit = zeit;
    }

    fn sfx(&mut self, s: Sfx) {
        self.ton.push(AudioCmd::Sfx(s));
    }

    fn kamera(&self) -> Camera {
        Camera { position: self.spieler.auge(), yaw: self.spieler.yaw, pitch: self.spieler.pitch, fov_y: 1.05, near: 0.06, far: himmel::NEBEL_ENDE }
    }

    fn abbauen(&mut self, t: Treffer) {
        let [x, y, z] = t.pos;
        if !self.welt.setzen(x, y, z, Block::Luft) {
            return;
        }
        self.zuletzt = Some((x / ABSCHNITT, y / ABSCHNITT, z / ABSCHNITT));
        self.sfx(Sfx::Abbauen);
        let farbe = t.block.farbe(Seite::Oben);
        for i in 0..14u32 {
            let r = |k: u32| welt::zufall((i * 7 + k) as i32, x + y * 3 + z * 7, 0xF00D) - 0.5;
            let c = 0.8 + r(5) * 0.4;
            let f = |s: u32| ((((farbe >> s) & 0xFF) as f32 * c).clamp(0.0, 255.0) as u32) << s;
            self.partikel.push(Partikel {
                pos: Vec3::new(x as f32 + 0.5 + r(1) * 0.8, y as f32 + 0.5 + r(2) * 0.8, z as f32 + 0.5 + r(3) * 0.8),
                vel: Vec3::new(r(1) * 4.0, 2.0 + r(4) * 3.0, r(3) * 4.0),
                leben: 0.5 + r(6) * 0.3,
                farbe: Color::hex(f(16) | f(8) | f(0)),
            });
        }
        self.geaendert = true;
    }

    fn setzen(&mut self, t: Treffer) {
        let [x, y, z] = [t.pos[0] + t.normale[0], t.pos[1] + t.normale[1], t.pos[2] + t.normale[2]];
        if !self.welt.drin(x, y, z) || self.spieler.beruehrt_block(x, y, z) {
            return;
        }
        if !matches!(self.welt.block(x, y, z), Block::Luft | Block::Wasser) {
            return;
        }
        if self.welt.setzen(x, y, z, LEISTE[self.auswahl]) {
            self.zuletzt = Some((x / ABSCHNITT, y / ABSCHNITT, z / ABSCHNITT));
            self.sfx(Sfx::Setzen);
            self.geaendert = true;
        }
    }

    fn spielstand(&self) -> Vec<u8> {
        let mut v = self.welt.aenderungen_bytes();
        let p = self.spieler.pos;
        for f in [p.x, p.y, p.z, self.spieler.yaw, self.spieler.pitch, self.zeit] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.push(self.auswahl as u8);
        v.push(self.spieler.fliegt as u8);
        v
    }
}

impl Spiel for Wuerfelwelt {
    fn update(&mut self, input: &Input, dt: f32) -> Wechsel {
        let dt = dt.clamp(0.0, 0.05);
        let mut e = self.st.update(input, dt);
        if self.autopilot {
            // Viel los: alle 0,25 s abwechselnd setzen und (gehalten) abbauen,
            // dabei langsam drehen – Netze werden laufend neu gebaut.
            self.autopilot_t += dt;
            self.spieler.pitch = -0.75;
            e.yaw = 0.35 * dt;
            let takt = (self.autopilot_t / 0.25) as u32;
            e.setzen = takt % 4 == 0 && ((self.autopilot_t - dt) / 0.25) as u32 != takt;
            e.abbauen = takt % 4 == 2 || takt % 4 == 3;
        }
        if let Some(s) = e.slot
            && s != self.auswahl
        {
            self.auswahl = s;
            self.name_t = 1.6;
            self.sfx(Sfx::MenuMove);
        }
        if e.fliegen_umschalten {
            self.spieler.fliegt = self.st.fliegt;
            self.sfx(Sfx::Whoosh);
        }
        self.spieler.yaw += e.yaw;
        self.spieler.pitch = (self.spieler.pitch + e.pitch).clamp(-1.5, 1.5);
        let war_am_boden = self.spieler.am_boden;
        self.spieler.schritt(&self.welt, Absicht { vor: e.vor, rechts: e.rechts, springen: e.springen, sinken: e.sinken }, dt);
        self.st.fliegt = self.spieler.fliegt;
        if self.spieler.weg - self.schritt_weg > 1.7 {
            self.schritt_weg = self.spieler.weg;
            self.sfx(Sfx::Schritt);
        }
        if !war_am_boden && self.spieler.am_boden {
            self.schritt_weg = self.spieler.weg;
        }
        // Ziel unter dem Fadenkreuz, abbauen (halten) und setzen.
        self.ziel = self.welt.strahl(self.spieler.auge(), self.spieler.blick(), REICHWEITE);
        match self.ziel {
            Some(t) if e.abbauen => {
                if t.pos != self.abbau_pos {
                    self.abbau_pos = t.pos;
                    self.abbau = 0.0;
                }
                self.abbau += dt;
                if self.abbau >= t.block.haerte() {
                    self.abbau = 0.0;
                    self.abbauen(t);
                }
            }
            _ => self.abbau = 0.0,
        }
        if e.setzen
            && let Some(t) = self.ziel
        {
            self.setzen(t);
        }
        // Höchstens zwei Abschnitte je Bild (gleichmäßiger Takt beim Bauen).
        self.netze_bauen(2);
        // Teilchen.
        for p in &mut self.partikel {
            p.vel.y -= 18.0 * dt;
            p.pos = p.pos + p.vel * dt;
            p.leben -= dt;
        }
        self.partikel.retain(|p| p.leben > 0.0);
        self.zeit += dt / himmel::TAG_SEKUNDEN;
        self.zeit -= floor(self.zeit);
        self.name_t = (self.name_t - dt).max(0.0);
        self.speicher_t += dt;
        Wechsel::Bleiben
    }

    fn render_size(&self) -> (usize, usize) {
        (W, H)
    }

    fn bild(&mut self, fd: &mut FrameData, _perf: &Perf) {
        let cam = self.kamera();
        let env = himmel::umgebung(self.zeit);
        let auge = cam.position;
        self.list.clear();
        let reichweite = himmel::NEBEL_ENDE + 12.0;
        let licht = himmel::tageslicht(self.zeit);
        let fest = DrawParams { unlit: true, tint: licht, ..DrawParams::OPAQUE };
        let durch = DrawParams { mode: DrawMode::Blend50, double_sided: true, ..fest };
        let leucht = DrawParams { unlit: true, ..DrawParams::OPAQUE };
        for az in 0..NZ {
            for ax in 0..NX {
                let (x0, z0) = ((ax * ABSCHNITT) as f32, (az * ABSCHNITT) as f32);
                let (mx, mz) = (x0 + 8.0 - auge.x, z0 + 8.0 - auge.z);
                if mx * mx + mz * mz > reichweite * reichweite {
                    continue;
                }
                for ay in 0..NY {
                    let y0 = (ay * ABSCHNITT) as f32;
                    let ids = self.netze[abschnitt_index(ax, ay, az)];
                    let s = ABSCHNITT as f32;
                    // Richtungsnetze, deren Flächen alle vom Auge abgewandt sind, entfallen.
                    let zu = [auge.x > x0 + 1.0, auge.x < x0 + s - 1.0, auge.y > y0 + 1.0, auge.y < y0 + s - 1.0, auge.z > z0 + 1.0, auge.z < z0 + s - 1.0];
                    for (k, id) in ids.iter().enumerate() {
                        if (k < 6 && !zu[k]) || self.store.get(*id).tris.is_empty() {
                            continue;
                        }
                        let params = match k {
                            0..6 => fest,
                            6 => durch,
                            _ => leucht,
                        };
                        self.list.push(*id, Mat4::IDENTITY, params);
                    }
                }
            }
        }
        // Meer und Meeresboden um die Insel.
        self.list.push(self.meer_boden, Mat4::IDENTITY, DrawParams { sort_bias: 1000.0, ..fest });
        self.list.push(self.meer, Mat4::IDENTITY, DrawParams { sort_bias: 1000.0, ..durch });
        for p in &self.partikel {
            self.list.sprite(p.pos, 0.14, p.farbe);
        }
        fd.szene_beginnen(&cam, &env, W, H, &self.store, &self.list);

        // Umriss des Zielblocks und Himmelskörper (Bildpunkte vorab).
        let umriss = self.ziel.and_then(|t| umriss_linien(fd, t.pos));
        let tag = himmel::tag(self.zeit);
        let mut sterne: Vec<(f32, f32, u32)> = Vec::new();
        if tag < 0.7 {
            for i in 0..70i32 {
                let a = welt::zufall(i, 1, 77) * core::f32::consts::TAU;
                let h = 0.12 + welt::zufall(i, 2, 77) * 0.85;
                let d = Vec3::new(cos(a) * (1.0 - h * h).max(0.0), h, sin(a) * (1.0 - h * h).max(0.0)).normalize_or_zero();
                if self.welt.strahl(auge, d, 40.0).is_some() {
                    continue;
                }
                if let Some((x, y, _)) = fd.projizieren(auge + d * 100.0) {
                    sterne.push((x, y, ((1.0 - tag / 0.7) * 26.0) as u32 + (welt::zufall(i, 3, 77) * 6.0) as u32));
                }
            }
        }
        let sonne = himmel::sonne(self.zeit);
        let mut scheiben: Vec<(f32, f32, f32, u32, u32)> = Vec::new();
        for (dir, farbe, r, ist_sonne) in [(sonne, 0xFFF4C8u32, 15.0f32, true), (sonne * -1.0, 0xE8EEFF, 11.0, false)] {
            if dir.y < -0.08 || self.welt.strahl(auge, dir, 40.0).is_some() {
                continue;
            }
            let staerke = if ist_sonne { 1.0 } else { 1.0 - tag };
            if staerke < 0.05 {
                continue;
            }
            if let Some((x, y, _)) = fd.projizieren(auge + dir * 100.0) {
                scheiben.push((x, y, r, farbe, (staerke * 32.0) as u32));
            }
        }

        let st = &self.st;
        let auswahl = self.auswahl;
        let abbau = self.ziel.map(|t| (self.abbau / t.block.haerte()).min(1.0)).unwrap_or(0.0);
        let name_t = self.name_t;
        let unter_wasser = self.welt.block(auge.x as i32, auge.y as i32, auge.z as i32) == Block::Wasser;
        fd.szene_abschliessen(&self.store, &self.list, &mut |c: &mut Canvas| {
            for &(x, y, r, farbe, a) in &scheiben {
                c.mix_disc(x, y, r * 2.2, hex(farbe), a / 6);
                c.mix_disc(x, y, r, hex(farbe), a);
            }
            for &(x, y, a) in &sterne {
                c.mix_disc(x, y, 1.3, hex(0xFFFFFF), a.min(32));
            }
            if unter_wasser {
                mix_rect(c, 0, 0, W as i32, H as i32, hex(0x1A4A9A), 14);
            }
            if let Some(l) = &umriss {
                for &(a, b) in l {
                    linie(c, a, b, hex(0x101014), 1.3);
                }
            }
            fadenkreuz(c, abbau);
            leiste(c, auswahl);
            if name_t > 0.0 {
                let a = (name_t / 0.4).min(1.0);
                mix_rect(c, 250, 288, 140, 20, hex(0x101018), (a * 20.0) as u32);
                c.text_centered(320, 293, LEISTE[auswahl].name(), hex(0xFFFFFF), hex(0x000000), 1);
            }
            st.zeichne(c);
        });
    }

    fn ton(&mut self, f: &mut dyn FnMut(AudioCmd)) {
        for c in self.ton.drain(..) {
            f(c);
        }
    }

    fn speichern(&mut self) -> Option<Vec<u8>> {
        if self.muss_speichern || (self.geaendert && self.speicher_t > 20.0) {
            self.muss_speichern = false;
            self.geaendert = false;
            self.speicher_t = 0.0;
            return Some(self.spielstand());
        }
        None
    }

    fn beenden(&mut self) {
        self.muss_speichern = true;
    }

    /// Festes halbes Tempo (27,5 Bilder/s): gleichmäßiger als schwankende 30+.
    fn bildrate_teiler(&self) -> u32 {
        2
    }
}

/// Bildschirmlinien der 12 Kanten eines Blocks (nur, wenn alle Ecken vor
/// der Kamera liegen); leicht vergrößert gegen Flimmern auf den Flächen.
fn umriss_linien(fd: &FrameData, p: [i32; 3]) -> Option<Vec<((f32, f32), (f32, f32))>> {
    let e = 0.004;
    let mut ecken = [(0.0f32, 0.0f32); 8];
    for (i, ecke) in ecken.iter_mut().enumerate() {
        let d = |b: usize| if i & b != 0 { 1.0 + e } else { -e };
        let v = Vec3::new(p[0] as f32 + d(1), p[1] as f32 + d(2), p[2] as f32 + d(4));
        let (x, y, _) = fd.projizieren(v)?;
        *ecke = (x, y);
    }
    let mut l = Vec::with_capacity(12);
    for i in 0..8usize {
        for b in [1usize, 2, 4] {
            if i & b == 0 {
                l.push((ecken[i], ecken[i | b]));
            }
        }
    }
    Some(l)
}

/// Rechteck mit Deckkraft `t` (0..=32).
fn mix_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, farbe: u16, t: u32) {
    let (x0, y0, x1, y1) = (x as f32, y as f32, (x + w) as f32, (y + h) as f32);
    c.mix_poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], farbe, t);
}

/// Linie als schmales Viereck.
fn linie(c: &mut Canvas, a: (f32, f32), b: (f32, f32), farbe: u16, dicke: f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = spatz::math::sqrt(dx * dx + dy * dy);
    if l < 0.5 || l > 2000.0 {
        return;
    }
    let (nx, ny) = (-dy / l * dicke * 0.5, dx / l * dicke * 0.5);
    c.fill_poly(&[(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)], farbe);
}

fn fadenkreuz(c: &mut Canvas, abbau: f32) {
    let (x, y) = (W as i32 / 2, H as i32 / 2);
    let dunkel = hex(0x000000);
    let weiss = hex(0xFFFFFF);
    mix_rect(c, x - 9, y - 2, 18, 4, dunkel, 12);
    mix_rect(c, x - 2, y - 9, 4, 18, dunkel, 12);
    c.fill_rect(x - 8, y - 1, 16, 2, weiss);
    c.fill_rect(x - 1, y - 8, 2, 16, weiss);
    if abbau > 0.0 {
        mix_rect(c, x - 16, y + 14, 32, 5, dunkel, 20);
        c.fill_rect(x - 15, y + 15, (30.0 * abbau) as i32, 3, hex(0xFFB070));
    }
}

/// Kleiner Würfel (Draufsicht schräg) als Symbol eines Blocks.
pub fn wuerfel_symbol(c: &mut Canvas, cx: f32, cy: f32, a: f32, b: Block) {
    let k = |f: u32, m: f32| {
        let g = |s: u32| ((((f >> s) & 0xFF) as f32 * m).clamp(0.0, 255.0) as u32) << s;
        hex(g(16) | g(8) | g(0))
    };
    let (o, s) = (b.farbe(Seite::Oben), b.farbe(Seite::Mantel));
    let h = a * 0.5;
    c.fill_poly(&[(cx - a, cy - h), (cx, cy - a), (cx + a, cy - h), (cx, cy)], k(o, 1.08));
    c.fill_poly(&[(cx - a, cy - h), (cx, cy), (cx, cy + a), (cx - a, cy + h)], k(s, 0.82));
    c.fill_poly(&[(cx + a, cy - h), (cx, cy), (cx, cy + a), (cx + a, cy + h)], k(s, 0.64));
}

fn leiste(c: &mut Canvas, auswahl: usize) {
    for (i, &b) in LEISTE.iter().enumerate() {
        let (x, y, w, h) = slot_rechteck(i);
        let an = i == auswahl;
        mix_rect(c, x, y, w, h, hex(0x101018), if an { 24 } else { 18 });
        wuerfel_symbol(c, (x + w / 2) as f32, (y + h / 2 + 1) as f32, 10.0, b);
        let rand = if an { hex(0xFFFFFF) } else { hex(0x505060) };
        let d = if an { 2 } else { 1 };
        c.fill_rect(x, y, w, d, rand);
        c.fill_rect(x, y + h - d, w, d, rand);
        c.fill_rect(x, y, d, h, rand);
        c.fill_rect(x + w - d, y, d, h, rand);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 55.0;

    #[test]
    fn startet_auf_der_insel_und_zeichnet() {
        let mut w = Wuerfelwelt::neu(None);
        for _ in 0..60 {
            w.update(&Input::default(), DT);
        }
        assert!(w.spieler.am_boden, "steht");
        let mut fd = FrameData::new();
        w.bild(&mut fd, &Perf::default());
        assert!(fd.prims() > 200, "Gelände sichtbar ({})", fd.prims());
    }

    #[test]
    fn setzen_abbauen_und_speichern() {
        let mut w = Wuerfelwelt::neu(None);
        for _ in 0..60 {
            w.update(&Input::default(), DT);
        }
        // Schräg nach unten: Boden zwei, drei Felder voraus.
        w.spieler.pitch = -0.6;
        w.update(&Input::default(), DT);
        let boden = w.ziel.expect("Boden im Blick");
        // Ziegel wählen und setzen.
        let (x, y, ww, hh) = slot_rechteck(3);
        w.update(&Input::touch_at((x + ww / 2) as u16, (y + hh / 2) as u16), DT);
        w.update(&Input::default(), DT);
        w.update(&Input::touch_at(590, 214), DT);
        w.update(&Input::default(), DT);
        let p = [boden.pos[0] + boden.normale[0], boden.pos[1] + boden.normale[1], boden.pos[2] + boden.normale[2]];
        assert_eq!(w.welt.block(p[0], p[1], p[2]), Block::Ziegel, "gesetzt");
        // Abbauen halten (Ziegel: 0,9 s), danach loslassen.
        for _ in 0..52 {
            w.update(&Input::touch_at(520, 262), DT);
        }
        w.update(&Input::default(), DT);
        assert_eq!(w.welt.block(p[0], p[1], p[2]), Block::Luft, "abgebaut");
        assert!(w.welt.block(boden.pos[0], boden.pos[1], boden.pos[2]).fest(), "Boden noch da");
        // Nochmal setzen, speichern, laden.
        w.update(&Input::touch_at(590, 214), DT);
        w.update(&Input::default(), DT);
        assert_eq!(w.welt.block(p[0], p[1], p[2]), Block::Ziegel);
        w.beenden();
        let bytes = w.speichern().expect("Spielstand");
        let v = Wuerfelwelt::neu(Some(&bytes));
        assert_eq!(v.welt.block(p[0], p[1], p[2]), Block::Ziegel);
        assert!((v.spieler.pos.x - w.spieler.pos.x).abs() < 1e-4, "Position geladen");
        assert_eq!(v.auswahl, 3);
    }
}
