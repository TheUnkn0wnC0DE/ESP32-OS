//! Das Konsolen-Menü: Karten der Spiele im Karussell, wischen zum Blättern,
//! tippen zum Starten.

use crate::kunst::{abgerundet, titelbild};
use crate::{Spiel, SpielId, Wechsel};
use alloc::vec;
use alloc::vec::Vec;
use pudding_core::app::{FrameData, Perf};
use pudding_core::audio::{AudioCmd, Sfx, SongId};
use pudding_core::gfx::{Canvas, hex, mix, text_width};
use pudding_core::input::Input;
use spatz::math::{abs, round, sin, sqrt};

/// Mittlere Karte (x, y, Breite, Höhe) im 640×360-Bild.
pub const KARTE: (i32, i32, i32, i32) = (210, 62, 220, 246);
/// Abstand der Nachbarkarten zur Mitte.
pub const NACHBAR_DX: i32 = 250;
/// Größe der Nachbarkarten relativ zur Mitte.
const NACHBAR_GROESSE: f32 = 0.78;
/// Ab so vielen Bildpunkten ist eine Berührung ein Wischen statt ein Tippen.
const WISCH_SCHWELLE: f32 = 12.0;
/// Dauer der Start-Animation (s).
const START_DAUER: f32 = 0.35;

const W: usize = 640;
const H: usize = 360;

/// Name, Beschreibung und Akzentfarbe einer Karte.
fn karte_info(id: SpielId) -> (&'static str, &'static str, u16) {
    match id {
        SpielId::Pudding => ("PUDDING-PANIK", "JUMP'N'RUN MIT WACKELPUDDING", hex(0xFF9EC4)),
        SpielId::Doom => ("DOOM", "DER SHOOTER-KLASSIKER VON 1993", hex(0xE8401C)),
        SpielId::Wuerfelwelt => ("WÜRFELWELT", "BAUEN IN EINER BLOCKWELT", hex(0x6ACB46)),
        SpielId::Quake => ("QUAKE", "ECHTES 3D: DER SHOOTER VON 1996", hex(0xC8803A)),
        SpielId::Konsole => ("KONSOLE", "", hex(0xFFFFFF)),
    }
}

/// Eine laufende Berührung.
#[derive(Clone, Copy, Debug)]
struct Zug {
    x0: f32,
    y0: f32,
    x: f32,
    pos0: f32,
    /// Geschwindigkeit des Fingers (Bildpunkte/s, waagerecht).
    v: f32,
    bewegt: bool,
}

pub struct Launcher {
    zeit: f32,
    /// Index der Karte in der Mitte (gleitend, für die Animation).
    pos: f32,
    tempo: f32,
    ziel: usize,
    zug: Option<Zug>,
    /// Startanimation: Spiel und vergangene Zeit.
    start: Option<(SpielId, f32)>,
    ton: Vec<AudioCmd>,
    /// Auswahl hat sich geändert → speichern.
    geaendert: bool,
}

impl Launcher {
    /// Menü mit `auswahl` in der Mitte (zuletzt gespieltes Spiel).
    pub fn new(auswahl: SpielId) -> Self {
        let ziel = SpielId::SPIELE.iter().position(|&s| s == auswahl).unwrap_or(0);
        Self {
            zeit: 0.0,
            pos: ziel as f32,
            tempo: 0.0,
            ziel,
            zug: None,
            start: None,
            ton: vec![AudioCmd::MusicEnabled(true), AudioCmd::Song(Some(SongId::Menu))],
            geaendert: false,
        }
    }

    /// Spiel, das gerade in der Mitte steht bzw. dorthin gleitet.
    pub fn auswahl(&self) -> SpielId {
        SpielId::SPIELE[self.ziel]
    }

    fn waehle(&mut self, i: usize) {
        if i != self.ziel {
            self.ziel = i;
            self.geaendert = true;
            self.ton.push(AudioCmd::Sfx(Sfx::MenuMove));
        }
    }

    /// Rechteck (x, y, Breite, Höhe) der Karte im Abstand `d` zur Mitte.
    fn karte_rect(d: f32) -> (f32, f32, f32, f32) {
        let s = 1.0 - (1.0 - NACHBAR_GROESSE) * abs(d).min(1.0);
        let (kx, ky, kw, kh) = KARTE;
        let (w, h) = (kw as f32 * s, kh as f32 * s);
        let cx = (kx + kw / 2) as f32 + d * NACHBAR_DX as f32;
        let cy = (ky + kh / 2) as f32;
        (cx - w / 2.0, cy - h / 2.0, w, h)
    }

    /// Antippen bei (`x`, `y`): mittlere Karte startet, Nachbarkarte rückt in
    /// die Mitte, daneben passiert nichts.
    fn tippen(&mut self, x: f32, y: f32) {
        for i in 0..SpielId::SPIELE.len() {
            let d = i as f32 - self.pos;
            let (rx, ry, rw, rh) = Self::karte_rect(d);
            if x < rx || x >= rx + rw || y < ry || y >= ry + rh {
                continue;
            }
            if i == self.ziel && abs(d) < 0.3 {
                self.start = Some((SpielId::SPIELE[i], 0.0));
                self.ton.push(AudioCmd::Sfx(Sfx::MenuSelect));
            } else {
                self.waehle(i);
            }
            return;
        }
    }

    fn loslassen(&mut self, z: Zug) {
        if !z.bewegt {
            self.tippen(z.x0, z.y0);
            return;
        }
        let n = SpielId::SPIELE.len() as f32;
        let start = round(z.pos0);
        let mut r = round(self.pos).clamp(0.0, n - 1.0);
        // Schnell gewischt, aber noch nicht weit genug: trotzdem eine Karte weiter.
        if r == start && abs(z.v) > 600.0 {
            r = (r + if z.v < 0.0 { 1.0 } else { -1.0 }).clamp(0.0, n - 1.0);
        }
        self.waehle(r as usize);
    }

    fn zeichne(&self, c: &mut Canvas) {
        let t = self.zeit;
        // Weiche Lichtflecken im Hintergrund
        for i in 0..12 {
            let f = i as f32;
            let x = ((f * 97.0 + t * (6.0 + (i % 3) as f32 * 4.0)) % 720.0) - 40.0;
            let y = 40.0 + ((f * 53.0) % 300.0) + sin(t * 0.5 + f) * 12.0;
            let r = 18.0 + (i % 4) as f32 * 9.0;
            let farbe = [hex(0xFF7AB0), hex(0x9A7AFF), hex(0x6AB4FF)][i % 3];
            c.mix_disc(x, y, r, farbe, 3);
        }
        // Überschrift
        c.text_centered(W as i32 / 2, 22, "SPIELEKONSOLE", hex(0xFFE8F4), hex(0x2A1030), 2);
        // Karten: entfernte zuerst, die mittlere zuletzt
        let n = SpielId::SPIELE.len();
        let mut reihenfolge: Vec<usize> = (0..n).collect();
        reihenfolge.sort_by(|&a, &b| abs(b as f32 - self.pos).total_cmp(&abs(a as f32 - self.pos)));
        for i in reihenfolge {
            let d = i as f32 - self.pos;
            if abs(d) < 1.6 {
                self.karte(c, i, d);
            }
        }
        // Hinweis unter der Karte (pulsiert) und Seitenpunkte
        if self.start.is_none() {
            let k = ((sin(t * 3.0) * 0.5 + 0.5) * 20.0) as u32;
            let farbe = mix(hex(0xFFFFFF), hex(0x3A2A5A), k);
            c.text_centered(W as i32 / 2, 318, "ZUM STARTEN TIPPEN", farbe, hex(0x1A1028), 1);
        }
        for i in 0..n {
            let x = W as f32 / 2.0 + (i as f32 - (n as f32 - 1.0) / 2.0) * 16.0;
            let aktiv = i == self.ziel;
            let (r, farbe, t) = if aktiv { (4.5, hex(0xFFFFFF), 32) } else { (3.5, hex(0x8A80A8), 20) };
            c.mix_disc(x, 342.0, r, farbe, t);
        }
    }

    fn karte(&self, c: &mut Canvas, i: usize, d: f32) {
        let id = SpielId::SPIELE[i];
        let (name, info, akzent) = karte_info(id);
        let (mut x, mut y, mut w, mut h) = Self::karte_rect(d);
        // Startanimation: Karte wächst kurz und leuchtet auf
        let mut blitz = 0u32;
        if let Some((sid, st)) = self.start
            && sid == id
        {
            let k = (st / START_DAUER).min(1.0);
            let g = 1.0 + 0.08 * sin(k * core::f32::consts::PI);
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            w *= g;
            h *= g;
            x = cx - w / 2.0;
            y = cy - h / 2.0;
            blitz = (24.0 * sin(k * core::f32::consts::PI)) as u32;
        }
        let mitte = abs(d) < 0.5;
        // Schatten, Rahmen, Fläche
        c.mix_poly(&abgerundet(x + 4.0, y + 7.0, w, h, 14.0), hex(0x05030A), 16);
        let puls = if mitte { 0.5 + 0.5 * sin(self.zeit * 2.5) } else { 0.0 };
        let rand = mix(akzent, hex(0xFFFFFF), (puls * 10.0) as u32);
        c.fill_poly(&abgerundet(x, y, w, h, 14.0), rand);
        c.fill_poly(&abgerundet(x + 2.5, y + 2.5, w - 5.0, h - 5.0, 12.0), hex(0x1E1A30));
        // Titelbild
        let (bx, by) = ((x + 8.0) as i32, (y + 8.0) as i32);
        let (bw, bh) = ((w - 16.0) as i32, (h * 0.64) as i32);
        titelbild(c, id, bx, by, bw, bh, self.zeit + i as f32 * 1.3);
        c.frame(bx - 1, by - 1, bw + 2, bh + 2, hex(0x0E0A18));
        // Name und Beschreibung
        let sc = if text_width(name, 3) <= bw { 3 } else { 2 };
        let ny = by + bh + 12;
        c.text_centered((x + w / 2.0) as i32, ny, name, hex(0xFFFFFF), hex(0x0A0614), sc);
        if mitte && !info.is_empty() {
            let farbe = mix(akzent, hex(0xFFFFFF), 18);
            c.text_centered((x + w / 2.0) as i32, ny + 7 * sc + 10, info, farbe, hex(0x0A0614), 1);
        }
        // Nachbarn abdunkeln, Startblitz
        let dunkel = (abs(d).min(1.0) * 15.0) as u32;
        if dunkel > 0 {
            c.mix_poly(&abgerundet(x, y, w, h, 14.0), hex(0x120A24), dunkel);
        }
        if blitz > 0 {
            c.mix_poly(&abgerundet(x, y, w, h, 14.0), hex(0xFFFFFF), blitz);
        }
    }
}

impl Spiel for Launcher {
    fn update(&mut self, input: &Input, dt: f32) -> Wechsel {
        self.zeit += dt;
        if let Some((id, st)) = self.start.as_mut() {
            *st += dt;
            if *st >= START_DAUER {
                return Wechsel::Starte(*id);
            }
            return Wechsel::Bleiben;
        }
        match (input.first_touch(), self.zug) {
            (Some((x, y)), None) => {
                let (x, y) = (x as f32, y as f32);
                self.zug = Some(Zug { x0: x, y0: y, x, pos0: self.pos, v: 0.0, bewegt: false });
                self.tempo = 0.0;
            }
            (Some((x, _)), Some(mut z)) => {
                let x = x as f32;
                z.v = (x - z.x) / dt.max(1e-3);
                z.x = x;
                if abs(x - z.x0) > WISCH_SCHWELLE {
                    z.bewegt = true;
                }
                if z.bewegt {
                    let n = SpielId::SPIELE.len() as f32;
                    self.pos = (z.pos0 - (x - z.x0) / NACHBAR_DX as f32).clamp(-0.35, n - 0.65);
                }
                self.zug = Some(z);
            }
            (None, Some(z)) => {
                self.zug = None;
                self.loslassen(z);
            }
            (None, None) => {}
        }
        if self.zug.is_none() {
            // Feder zur Zielkarte (leicht unterdämpft: schwingt sanft nach)
            let k = 140.0;
            let daempf = 2.0 * sqrt(k) * 0.75;
            let a = (self.ziel as f32 - self.pos) * k - self.tempo * daempf;
            self.tempo += a * dt;
            self.pos += self.tempo * dt;
            if abs(self.ziel as f32 - self.pos) < 0.001 && abs(self.tempo) < 0.01 {
                self.pos = self.ziel as f32;
                self.tempo = 0.0;
            }
        }
        Wechsel::Bleiben
    }

    fn render_size(&self) -> (usize, usize) {
        (W, H)
    }

    fn bild(&mut self, fd: &mut FrameData, _perf: &Perf) {
        let t = self.zeit;
        let (oben, unten) = (hex(0x241038), hex(0x0C1638));
        let welle = (sin(t * 0.3) * 0.5 + 0.5) * 6.0;
        let himmel = |y: usize| mix(oben, unten, (y as f32 * 32.0 / H as f32 + welle).min(32.0) as u32);
        fd.flach(W, H, &himmel, &mut |c| self.zeichne(c));
    }

    fn ton(&mut self, f: &mut dyn FnMut(AudioCmd)) {
        for c in self.ton.drain(..) {
            f(c);
        }
    }

    fn speichern(&mut self) -> Option<Vec<u8>> {
        if !self.geaendert {
            return None;
        }
        self.geaendert = false;
        Some(vec![self.auswahl().als_u8()])
    }

    fn eigene_home_taste(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Spiel, SpielId, Wechsel};
    use pudding_core::input::Input;

    const DT: f32 = 1.0 / 55.0;

    fn bilder(l: &mut Launcher, n: usize, input: Input) -> Wechsel {
        let mut w = Wechsel::Bleiben;
        for _ in 0..n {
            let r = l.update(&input, DT);
            if r != Wechsel::Bleiben {
                w = r;
            }
        }
        w
    }

    fn tippen(l: &mut Launcher, x: u16, y: u16) -> Wechsel {
        let a = bilder(l, 2, Input::touch_at(x, y));
        let b = bilder(l, 60, Input::default());
        if a != Wechsel::Bleiben { a } else { b }
    }

    #[test]
    fn wischen_blaettert() {
        let mut l = Launcher::new(SpielId::Pudding);
        for i in 0..6 {
            l.update(&Input::touch_at(500 - i * 60, 180), DT);
        }
        bilder(&mut l, 60, Input::default());
        assert_eq!(l.auswahl(), SpielId::Doom);
    }

    #[test]
    fn tipp_mitte_startet() {
        let mut l = Launcher::new(SpielId::Doom);
        let (x, y, w, h) = KARTE;
        let w = tippen(&mut l, (x + w / 2) as u16, (y + h / 2) as u16);
        assert_eq!(w, Wechsel::Starte(SpielId::Doom));
    }

    #[test]
    fn kurzer_tipp_neben_karte_startet_nichts() {
        let mut l = Launcher::new(SpielId::Doom);
        let (x, _, _, h) = KARTE;
        assert_eq!(tippen(&mut l, (x - 30) as u16, (h / 2 + 60) as u16), Wechsel::Bleiben);
        assert_eq!(tippen(&mut l, 320, 352), Wechsel::Bleiben);
        assert_eq!(l.auswahl(), SpielId::Doom);
    }

    #[test]
    fn tipp_nachbar_holt_karte() {
        let mut l = Launcher::new(SpielId::Doom);
        let (x, y, w, h) = KARTE;
        let rechts = (x + w / 2 + NACHBAR_DX) as u16;
        assert_eq!(tippen(&mut l, rechts.min(630), (y + h / 2) as u16), Wechsel::Bleiben);
        assert_eq!(l.auswahl(), SpielId::Quake, "rechts neben Doom");
    }

    #[test]
    fn zeichnet_ohne_fehler() {
        let mut l = Launcher::new(SpielId::Pudding);
        let mut fd = pudding_core::app::FrameData::new();
        for _ in 0..3 {
            l.update(&Input::default(), DT);
            l.bild(&mut fd, &pudding_core::app::Perf::default());
        }
        assert!(fd.prims() == 0);
    }
}
