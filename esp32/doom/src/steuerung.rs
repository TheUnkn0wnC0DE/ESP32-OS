//! Touch-Steuerung für Doom im 640×360-Raster der Konsole.
//!
//! * Das Doom-Bild (4:3) liegt in der Mitte, rechts daneben die Knöpfe
//!   (Feuer, Benutzen, Waffe, Karte, Menü); links sitzt die Home-Taste.
//! * Linke Hälfte: schwebender Steuerknüppel (laufen, seitwärts).
//! * Rechte Hälfte: wischen = drehen, kurz tippen = benutzen (Türen).
//!   Auch der Feuer-Finger dreht beim Wischen mit.
//! * Menüs, Meldungen, Zwischenbilder: direkt antippen; Ja/Nein-Fragen
//!   bekommen große Knöpfe.

use pudding_core::gfx::{Canvas, hex};
use pudding_core::input::{Input, MAX_TOUCH};

/// Doom-Bild im Raster: (x, y, Breite, Höhe).
pub const BILD: (i32, i32, i32, i32) = (80, 0, 480, 360);
const MITTE_X: f32 = 320.0;
const STICK_R: f32 = 52.0;
/// Maus-Einheiten je Bildpunkt Wischen (8192 = volle Drehung).
const DREH_JE_PX: f32 = 11.0;
/// Mindestdauer kurzer Tastendrücke (Doom fragt je Tic = 28,6 ms ab).
const PULS: f32 = 0.12;
const TIPP_WEG: f32 = 14.0;

/// Doom-Tastencodes (doomkeys.h).
pub mod taste {
    pub const ESC: u8 = 27;
    pub const ENTER: u8 = 13;
    pub const TAB: u8 = 9;
    pub const FEUER: u8 = 0xa3;
    pub const BENUTZEN: u8 = 0xa2;
    pub const LINKS: u8 = 0xac;
    pub const RECHTS: u8 = 0xae;
    pub const STRAFE_L: u8 = 0xa0;
    pub const STRAFE_R: u8 = 0xa1;
    /// Nächste Waffe (in dg_konsole.c gebunden).
    pub const WAFFE: u8 = 0x80 + 0x52;
}

/// Was Doom gerade zeigt (`DG_Modus` in dg_konsole.c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modus {
    Spiel,
    Menue,
    Meldung,
    Frage,
    Name,
    Zwischen,
}

impl Modus {
    pub fn aus_c(v: i32) -> Self {
        match v {
            1 => Modus::Menue,
            2 => Modus::Meldung,
            3 => Modus::Frage,
            4 => Modus::Name,
            5 => Modus::Zwischen,
            _ => Modus::Spiel,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knopf {
    Menue,
    Karte,
    Waffe,
    Benutzen,
    Feuer,
    Ja,
    Nein,
}

impl Knopf {
    fn taste(self) -> u8 {
        match self {
            Knopf::Menue => taste::ESC,
            Knopf::Karte => taste::TAB,
            Knopf::Waffe => taste::WAFFE,
            Knopf::Benutzen => taste::BENUTZEN,
            Knopf::Feuer => taste::FEUER,
            Knopf::Ja => b'y',
            Knopf::Nein => b'n',
        }
    }

    /// Gehalten, solange der Finger liegt (sonst kurzer Druck).
    fn gehalten(self) -> bool {
        matches!(self, Knopf::Feuer | Knopf::Benutzen)
    }
}

struct KnopfDef {
    k: Knopf,
    x: f32,
    y: f32,
    r: f32,
    text: &'static str,
    farbe: u32,
}

const fn kd(k: Knopf, x: f32, y: f32, r: f32, text: &'static str, farbe: u32) -> KnopfDef {
    KnopfDef { k, x, y, r, text, farbe }
}

const MENUE: KnopfDef = kd(Knopf::Menue, 600.0, 36.0, 25.0, "MENÜ", 0xFFFFFF);
const ZURUECK: KnopfDef = kd(Knopf::Menue, 600.0, 36.0, 25.0, "ZURÜCK", 0xFFFFFF);
const KNOEPFE_SPIEL: [KnopfDef; 5] = [
    MENUE,
    kd(Knopf::Karte, 600.0, 100.0, 25.0, "KARTE", 0xD8E0FF),
    kd(Knopf::Waffe, 600.0, 168.0, 28.0, "WAFFE", 0xFFD860),
    kd(Knopf::Benutzen, 600.0, 241.0, 31.0, "BENUTZEN", 0x8CD8FF),
    kd(Knopf::Feuer, 600.0, 318.0, 35.0, "FEUER", 0xFF5A48),
];
const KNOEPFE_ZWISCHEN: [KnopfDef; 3] = [
    MENUE,
    kd(Knopf::Benutzen, 600.0, 241.0, 31.0, "WEITER", 0x8CD8FF),
    kd(Knopf::Feuer, 600.0, 318.0, 35.0, "FEUER", 0xFF5A48),
];
const KNOEPFE_MENUE: [KnopfDef; 1] = [ZURUECK];
const KNOEPFE_FRAGE: [KnopfDef; 3] = [
    ZURUECK,
    kd(Knopf::Ja, 250.0, 300.0, 38.0, "JA", 0x70FF90),
    kd(Knopf::Nein, 390.0, 300.0, 38.0, "NEIN", 0xFF7070),
];

fn knoepfe(m: Modus) -> &'static [KnopfDef] {
    match m {
        Modus::Spiel => &KNOEPFE_SPIEL,
        Modus::Zwischen => &KNOEPFE_ZWISCHEN,
        Modus::Frage => &KNOEPFE_FRAGE,
        Modus::Menue | Modus::Meldung | Modus::Name => &KNOEPFE_MENUE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Rolle {
    Stick { x0: f32, y0: f32 },
    Drehen,
    Knopf(Knopf),
    Tipp,
    /// Begann in einem anderen Modus: bis zum Loslassen wirkungslos.
    Aus,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    x: f32,
    y: f32,
    /// Zurückgelegter Weg und Dauer (Tipp oder Wischen?).
    weg: f32,
    t: f32,
    rolle: Rolle,
}

/// Ergebnis eines Bildes.
#[derive(Debug, Default, PartialEq)]
pub struct Ausgabe {
    /// Tastenereignisse (gedrückt?, Doom-Taste) in dieser Reihenfolge.
    pub tasten: Vec<(bool, u8)>,
    /// Tipps aufs Doom-Bild in Doom-Koordinaten (320×200).
    pub tipps: Vec<(i32, i32)>,
}

pub struct Steuerung {
    finger: [Option<Finger>; MAX_TOUCH],
    gehalten: Vec<u8>,
    pulse: Vec<(u8, f32)>,
    dreh: f32,
    stick: (f32, f32),
    modus: Modus,
}

impl Default for Steuerung {
    fn default() -> Self {
        Self::new()
    }
}

/// Raster → Doom-Bild (320×200).
pub fn doom_koordinaten(x: f32, y: f32) -> Option<(i32, i32)> {
    let (bx, by, bw, bh) = BILD;
    let (dx, dy) = (x - bx as f32, y - by as f32);
    if dx < 0.0 || dy < 0.0 || dx >= bw as f32 || dy >= bh as f32 {
        return None;
    }
    Some(((dx * 320.0 / bw as f32) as i32, (dy * 200.0 / bh as f32) as i32))
}

impl Steuerung {
    pub fn new() -> Self {
        Self { finger: [None; MAX_TOUCH], gehalten: Vec::new(), pulse: Vec::new(), dreh: 0.0, stick: (0.0, 0.0), modus: Modus::Zwischen }
    }

    /// Gesammelte Drehung seit dem letzten Abholen (Maus-Einheiten, + = rechts).
    pub fn dreh_nehmen(&mut self) -> i32 {
        let d = self.dreh.trunc();
        self.dreh -= d;
        d as i32
    }

    /// Laufen (Maus-Einheiten, + = vorwärts, 50 = rennen).
    pub fn lauf(&self) -> i32 {
        let y = -self.stick.1;
        if y.abs() < 0.12 { 0 } else { (y * 50.0).round() as i32 }
    }

    fn treffer(&self, x: f32, y: f32) -> Option<Knopf> {
        knoepfe(self.modus).iter().find(|k| (x - k.x).hypot(y - k.y) <= k.r + 8.0).map(|k| k.k)
    }

    pub fn update(&mut self, input: &Input, modus: Modus, dt: f32) -> Ausgabe {
        let mut aus = Ausgabe::default();
        if modus != self.modus {
            self.modus = modus;
            for f in self.finger.iter_mut().flatten() {
                f.rolle = Rolle::Aus;
            }
            self.stick = (0.0, 0.0);
            self.dreh = 0.0;
        }
        let punkte: Vec<(f32, f32)> = input.touch.iter().flatten().map(|&(x, y)| (x as f32, y as f32)).collect();
        let alt = self.finger;
        let mut neu: [Option<Finger>; MAX_TOUCH] = [None; MAX_TOUCH];
        let mut benutzt = [false; MAX_TOUCH];
        // Gierige Zuordnung: nächstgelegene Paare zuerst.
        let mut paare: Vec<(f32, usize, usize)> = Vec::new();
        for (i, p) in punkte.iter().enumerate() {
            for (j, f) in alt.iter().enumerate() {
                if let Some(f) = f {
                    let d = (p.0 - f.x).hypot(p.1 - f.y);
                    if d < 160.0 {
                        paare.push((d, i, j));
                    }
                }
            }
        }
        paare.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut zu: [Option<usize>; MAX_TOUCH] = [None; MAX_TOUCH];
        for (_, i, j) in paare {
            if zu[i].is_none() && !benutzt[j] {
                zu[i] = Some(j);
                benutzt[j] = true;
            }
        }
        for (i, &(x, y)) in punkte.iter().enumerate().take(MAX_TOUCH) {
            let f = match zu[i].and_then(|j| alt[j]) {
                Some(mut f) => {
                    let (dx, dy) = (x - f.x, y - f.y);
                    f.weg += dx.hypot(dy);
                    f.t += dt;
                    match f.rolle {
                        Rolle::Drehen | Rolle::Knopf(Knopf::Feuer) => self.dreh += dx * DREH_JE_PX,
                        Rolle::Stick { x0, y0 } => {
                            let (vx, vy) = (x - x0, y - y0);
                            let l = vx.hypot(vy);
                            // Schwebender Knüppel: der Mittelpunkt folgt dem Finger.
                            let (x0, y0) = if l > STICK_R { (x - vx / l * STICK_R, y - vy / l * STICK_R) } else { (x0, y0) };
                            f.rolle = Rolle::Stick { x0, y0 };
                        }
                        _ => {}
                    }
                    f.x = x;
                    f.y = y;
                    f
                }
                None => {
                    let rolle = match self.treffer(x, y) {
                        Some(k) => {
                            if !k.gehalten() {
                                aus.tasten.push((true, k.taste()));
                                self.pulse.push((k.taste(), PULS));
                            }
                            Rolle::Knopf(k)
                        }
                        None if modus == Modus::Spiel && x < MITTE_X => Rolle::Stick { x0: x, y0: y },
                        None if modus == Modus::Spiel => Rolle::Drehen,
                        None => Rolle::Tipp,
                    };
                    Finger { x, y, weg: 0.0, t: 0.0, rolle }
                }
            };
            neu[i] = Some(f);
        }
        // Losgelassene Finger.
        for (j, f) in alt.iter().enumerate() {
            let Some(f) = f else { continue };
            if benutzt[j] {
                continue;
            }
            match f.rolle {
                Rolle::Tipp if f.weg < TIPP_WEG => {
                    if modus == Modus::Zwischen {
                        aus.tasten.push((true, taste::BENUTZEN));
                        self.pulse.push((taste::BENUTZEN, PULS));
                    } else if let Some(p) = doom_koordinaten(f.x, f.y) {
                        aus.tipps.push(p);
                    }
                }
                Rolle::Drehen if f.weg < 10.0 && f.t < 0.3 => {
                    aus.tasten.push((true, taste::BENUTZEN));
                    self.pulse.push((taste::BENUTZEN, PULS));
                }
                _ => {}
            }
        }
        self.finger = neu;
        // Knüppel.
        self.stick = self
            .finger
            .iter()
            .flatten()
            .find_map(|f| match f.rolle {
                Rolle::Stick { x0, y0 } => Some((((f.x - x0) / STICK_R).clamp(-1.0, 1.0), ((f.y - y0) / STICK_R).clamp(-1.0, 1.0))),
                _ => None,
            })
            .unwrap_or((0.0, 0.0));
        // Gehaltene Tasten: Feuer/Benutzen-Finger, seitwärts per Knüppel.
        let mut soll: Vec<u8> = self
            .finger
            .iter()
            .flatten()
            .filter_map(|f| match f.rolle {
                Rolle::Knopf(k) if k.gehalten() => Some(k.taste()),
                _ => None,
            })
            .collect();
        if self.stick.0 < -0.45 {
            soll.push(taste::STRAFE_L);
        } else if self.stick.0 > 0.45 {
            soll.push(taste::STRAFE_R);
        }
        for &k in &self.gehalten {
            if !soll.contains(&k) {
                aus.tasten.push((false, k));
            }
        }
        for &k in &soll {
            if !self.gehalten.contains(&k) {
                aus.tasten.push((true, k));
            }
        }
        soll.dedup();
        self.gehalten = soll;
        // Kurze Drücke wieder loslassen.
        for p in &mut self.pulse {
            p.1 -= dt;
            if p.1 <= 0.0 {
                aus.tasten.push((false, p.0));
            }
        }
        self.pulse.retain(|p| p.1 > 0.0);
        aus
    }

    /// Knöpfe und Knüppel über das Bild zeichnen (Raster 640×360).
    pub fn zeichne(&self, c: &mut Canvas) {
        let gedrueckt = |k: Knopf| self.finger.iter().flatten().any(|f| f.rolle == Rolle::Knopf(k));
        for d in knoepfe(self.modus) {
            let an = gedrueckt(d.k);
            let farbe = hex(d.farbe);
            c.mix_disc(d.x, d.y, d.r, if an { farbe } else { hex(0x0C0C12) }, if an { 16 } else { 20 });
            c.mix_ring(d.x, d.y, d.r, 2.0, farbe, if an { 32 } else { 20 });
            symbol(c, d, if an { hex(0x101014) } else { farbe });
        }
        if self.modus != Modus::Spiel {
            return;
        }
        match self.finger.iter().flatten().find_map(|f| match f.rolle {
            Rolle::Stick { x0, y0 } => Some((x0, y0, f.x, f.y)),
            _ => None,
        }) {
            Some((x0, y0, x, y)) => {
                c.mix_disc(x0, y0, STICK_R, hex(0x000000), 10);
                c.mix_ring(x0, y0, STICK_R, 2.0, hex(0xFFFFFF), 14);
                let (vx, vy) = (x - x0, y - y0);
                let l = vx.hypot(vy).max(1.0);
                let k = l.min(STICK_R) / l;
                c.mix_disc(x0 + vx * k, y0 + vy * k, 20.0, hex(0xFFFFFF), 14);
            }
            None => {
                // Hinweis: hier liegt der Knüppel (über der Statusleiste).
                c.mix_ring(160.0, 228.0, STICK_R, 2.0, hex(0xFFFFFF), 6);
                c.mix_disc(160.0, 228.0, 18.0, hex(0xFFFFFF), 5);
            }
        }
    }
}

/// Symbol eines Knopfs (gezeichnet, sprachunabhängig); Ja/Nein als Text.
fn symbol(c: &mut Canvas, d: &KnopfDef, f: u16) {
    let (x, y) = (d.x, d.y);
    let p = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> { pts.iter().map(|&(a, b)| (x + a, y + b)).collect() };
    let r = |c: &mut Canvas, x0: f32, y0: f32, w: f32, h: f32| c.fill_rect((x + x0) as i32, (y + y0) as i32, w as i32, h as i32, f);
    match d.k {
        Knopf::Menue if d.text == "MENÜ" => {
            for dy in [-8.0, -1.0, 6.0] {
                r(c, -10.0, dy, 20.0, 3.0);
            }
        }
        Knopf::Menue => {
            // Pfeil nach links.
            c.fill_poly(&p(&[(-12.0, 0.0), (-3.0, -9.0), (-3.0, 9.0)]), f);
            r(c, -4.0, -2.0, 15.0, 4.0);
        }
        Knopf::Karte => {
            let k = p(&[(-13.0, -8.0), (-4.0, -12.0), (4.0, -8.0), (13.0, -12.0), (13.0, 8.0), (4.0, 12.0), (-4.0, 8.0), (-13.0, 12.0)]);
            c.mix_poly(&k, f, 12);
            for kante in [
                [(-13.0, -8.0), (-4.0, -12.0)],
                [(-4.0, -12.0), (4.0, -8.0)],
                [(4.0, -8.0), (13.0, -12.0)],
                [(-13.0, 12.0), (-4.0, 8.0)],
                [(-4.0, 8.0), (4.0, 12.0)],
                [(4.0, 12.0), (13.0, 8.0)],
            ] {
                let [(ax, ay), (bx, by)] = kante;
                let (nx, ny) = (0.0, 1.6);
                c.fill_poly(&p(&[(ax, ay), (bx, by), (bx + nx, by + ny), (ax + nx, ay + ny)]), f);
            }
            for sx in [-13.0, -4.5, 3.5, 11.5] {
                let oben = if sx == -4.5 || sx == 11.5 { -12.0 } else { -8.0 };
                r(c, sx, oben, 1.6, 20.0);
            }
            // Markierung „Hier bist du“.
            c.mix_disc(x + 7.0, y - 1.0, 2.5, f, 32);
        }
        Knopf::Waffe => {
            // Pistole im Profil, Pfeil = nächste Waffe.
            c.fill_poly(&p(&[(-15.0, -7.0), (8.0, -7.0), (8.0, -1.0), (-15.0, -1.0)]), f);
            c.fill_poly(&p(&[(1.0, -1.0), (8.0, -1.0), (10.0, 10.0), (4.0, 10.0)]), f);
            c.mix_ring(x - 1.0, y + 2.0, 3.0, 1.6, f, 32);
            c.fill_poly(&p(&[(11.0, -16.0), (17.0, -12.0), (11.0, -8.0)]), f);
            r(c, 4.0, -13.0, 8.0, 2.0);
        }
        Knopf::Benutzen if d.text == "WEITER" => {
            c.fill_poly(&p(&[(-8.0, -11.0), (6.0, 0.0), (-8.0, 11.0)]), f);
            r(c, 7.0, -11.0, 3.0, 22.0);
        }
        Knopf::Benutzen => {
            // Tür mit Klinke.
            r(c, -9.0, -14.0, 18.0, 2.0);
            r(c, -9.0, -14.0, 2.0, 28.0);
            r(c, 7.0, -14.0, 2.0, 28.0);
            c.mix_poly(&p(&[(-7.0, -12.0), (7.0, -12.0), (7.0, 14.0), (-7.0, 14.0)]), f, 10);
            c.mix_disc(x + 3.5, y + 1.0, 2.2, f, 32);
            r(c, -12.0, 13.0, 24.0, 2.0);
        }
        Knopf::Feuer => {
            c.mix_ring(x, y, 13.0, 2.4, f, 32);
            for (a, b, w, h) in [(-19.0, -1.2, 10.0, 2.4), (9.0, -1.2, 10.0, 2.4), (-1.2, -19.0, 2.4, 10.0), (-1.2, 9.0, 2.4, 10.0)] {
                r(c, a, b, w, h);
            }
            c.mix_disc(x, y, 2.6, f, 32);
        }
        Knopf::Ja | Knopf::Nein => {
            c.text_centered(x as i32, y as i32 - 10, d.text, f, hex(0x000000), 3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 55.0;

    fn zwei(a: (u16, u16), b: (u16, u16)) -> Input {
        Input { held: 0, touch: [Some(a), Some(b)] }
    }

    fn alle(s: &mut Steuerung, inputs: &[Input], m: Modus) -> Ausgabe {
        let mut a = Ausgabe::default();
        for i in inputs {
            let o = s.update(i, m, DT);
            a.tasten.extend(o.tasten);
            a.tipps.extend(o.tipps);
        }
        a
    }

    #[test]
    fn knueppel_nach_oben_laeuft_vorwaerts() {
        let mut s = Steuerung::new();
        s.update(&Input::touch_at(200, 260), Modus::Spiel, DT);
        assert_eq!(s.lauf(), 0);
        s.update(&Input::touch_at(200, 200), Modus::Spiel, DT);
        assert_eq!(s.lauf(), 50, "voll nach oben = rennen");
        assert_eq!(s.dreh_nehmen(), 0, "Knüppel dreht nicht");
        s.update(&Input::default(), Modus::Spiel, DT);
        assert_eq!(s.lauf(), 0, "losgelassen: stehen");
    }

    #[test]
    fn knueppel_seitwaerts_drueckt_strafe() {
        let mut s = Steuerung::new();
        s.update(&Input::touch_at(200, 260), Modus::Spiel, DT);
        let a = s.update(&Input::touch_at(160, 260), Modus::Spiel, DT);
        assert!(a.tasten.contains(&(true, taste::STRAFE_L)));
        let a = s.update(&Input::default(), Modus::Spiel, DT);
        assert!(a.tasten.contains(&(false, taste::STRAFE_L)));
    }

    #[test]
    fn wischen_rechts_dreht_nach_rechts() {
        let mut s = Steuerung::new();
        let a = alle(&mut s, &[Input::touch_at(400, 200), Input::touch_at(420, 200), Input::touch_at(450, 200)], Modus::Spiel);
        assert!(a.tasten.is_empty());
        assert_eq!(s.dreh_nehmen(), (50.0 * DREH_JE_PX) as i32);
        assert_eq!(s.dreh_nehmen(), 0, "abgeholt");
        let a = s.update(&Input::default(), Modus::Spiel, DT);
        assert!(a.tasten.is_empty(), "Wischen ist kein Tipp");
    }

    #[test]
    fn feuer_wird_gehalten_und_dreht_mit_knueppel_zugleich() {
        let mut s = Steuerung::new();
        let a = s.update(&zwei((200, 260), (600, 318)), Modus::Spiel, DT);
        assert_eq!(a.tasten, vec![(true, taste::FEUER)]);
        let a = s.update(&zwei((200, 210), (590, 318)), Modus::Spiel, DT);
        assert!(a.tasten.is_empty(), "Feuer bleibt gedrückt");
        assert_eq!(s.lauf(), 48);
        assert!(s.dreh_nehmen() < 0, "Feuer-Finger nach links: Drehung links");
        // Knüppel-Finger hebt ab: Reihenfolge der Punkte ändert sich.
        let a = s.update(&Input::touch_at(590, 318), Modus::Spiel, DT);
        assert!(a.tasten.is_empty(), "Feuer-Finger bleibt Feuer");
        assert_eq!(s.lauf(), 0);
        let a = s.update(&Input::default(), Modus::Spiel, DT);
        assert_eq!(a.tasten, vec![(false, taste::FEUER)]);
    }

    #[test]
    fn kurzer_tipp_rechts_benutzt() {
        let mut s = Steuerung::new();
        let a = alle(&mut s, &[Input::touch_at(450, 150), Input::default()], Modus::Spiel);
        assert_eq!(a.tasten, vec![(true, taste::BENUTZEN)]);
        let a = alle(&mut s, &[Input::default(); 8], Modus::Spiel);
        assert_eq!(a.tasten, vec![(false, taste::BENUTZEN)], "nach kurzer Zeit wieder los");
    }

    #[test]
    fn waffe_ist_ein_kurzer_druck() {
        let mut s = Steuerung::new();
        let a = s.update(&Input::touch_at(600, 168), Modus::Spiel, DT);
        assert_eq!(a.tasten, vec![(true, taste::WAFFE)]);
        let a = alle(&mut s, &[Input::touch_at(600, 168); 10], Modus::Spiel);
        assert_eq!(a.tasten, vec![(false, taste::WAFFE)]);
    }

    #[test]
    fn menue_tipp_liefert_doom_koordinaten() {
        let mut s = Steuerung::new();
        let a = alle(&mut s, &[Input::touch_at(320, 180), Input::default()], Modus::Menue);
        assert_eq!(a.tipps, vec![(160, 100)]);
        assert!(a.tasten.is_empty());
        // Gewischt: kein Tipp.
        let a = alle(&mut s, &[Input::touch_at(300, 100), Input::touch_at(300, 160), Input::default()], Modus::Menue);
        assert!(a.tipps.is_empty());
    }

    #[test]
    fn frage_hat_ja_und_nein() {
        let mut s = Steuerung::new();
        let a = s.update(&Input::touch_at(250, 300), Modus::Frage, DT);
        assert_eq!(a.tasten, vec![(true, b'y')]);
    }

    #[test]
    fn moduswechsel_laesst_tasten_los() {
        let mut s = Steuerung::new();
        s.update(&Input::touch_at(600, 318), Modus::Spiel, DT);
        let a = s.update(&Input::touch_at(600, 318), Modus::Menue, DT);
        assert!(a.tasten.contains(&(false, taste::FEUER)));
        let a = s.update(&Input::default(), Modus::Menue, DT);
        assert!(a.tipps.is_empty(), "Finger aus dem Spiel ist kein Menü-Tipp");
    }

    #[test]
    fn zwischenbild_tipp_ueberspringt() {
        let mut s = Steuerung::new();
        let a = alle(&mut s, &[Input::touch_at(300, 150), Input::default()], Modus::Zwischen);
        assert_eq!(a.tasten, vec![(true, taste::BENUTZEN)]);
    }
}
