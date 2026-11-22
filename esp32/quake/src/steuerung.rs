//! Touch-Steuerung für Quake im 640×360-Raster der Konsole.
//!
//! * Im Spiel: links schwebender Steuerknüppel (laufen, seitwärts), rechts
//!   wischen = umsehen (auch hoch/runter); Knöpfe im rechten Rand: Feuer
//!   (dreht beim Wischen mit), Springen, Waffe, Menü.
//! * In Menüs und Konsole: Pfeile, OK und Zurück.
//! * In der Vorführung (Demo): antippen öffnet das Menü.

use pudding_core::gfx::{Canvas, hex};
use pudding_core::input::{Input, MAX_TOUCH};
use spatz_sqrt::laenge;

/// Quake-Bild im Raster: (x, y, Breite, Höhe) – 320×240 · 1,5.
pub const BILD: (i32, i32, i32, i32) = (80, 0, 480, 360);
const MITTE_X: f32 = 320.0;
const STICK_R: f32 = 52.0;
/// Maus-Einheiten je Bildpunkt Wischen (mit `sensitivity 5`: ~0,44° je Punkt).
const MAUS_JE_PX: f32 = 4.0;
const PULS: f32 = 0.12;

mod spatz_sqrt {
    pub fn laenge(x: f32, y: f32) -> f32 {
        (x * x + y * y).sqrt()
    }
}

/// Quake-Tastencodes (quakekeys.h).
pub mod taste {
    pub const ENTER: i32 = 13;
    pub const ESC: i32 = 27;
    pub const LEER: i32 = 32;
    pub const HOCH: i32 = 128;
    pub const RUNTER: i32 = 129;
    pub const LINKS: i32 = 130;
    pub const RECHTS: i32 = 131;
    pub const STRG: i32 = 133;
    /// „impulse 10“ (nächste Waffe) in der Standardbelegung.
    pub const WAFFE: i32 = b'/' as i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modus {
    Spiel,
    Menue,
    Vorfuehrung,
}

impl Modus {
    /// Aus `QG_Modus` (key_dest + 16 bei laufender Demo).
    pub fn aus_c(v: i32) -> Self {
        match v & 15 {
            0 if v & 16 != 0 => Modus::Vorfuehrung,
            0 => Modus::Spiel,
            _ => Modus::Menue,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knopf {
    Menue,
    Waffe,
    Springen,
    Feuer,
    Hoch,
    Runter,
    Links,
    Rechts,
    Ok,
}

impl Knopf {
    fn taste(self) -> i32 {
        match self {
            Knopf::Menue => taste::ESC,
            Knopf::Waffe => taste::WAFFE,
            Knopf::Springen => taste::LEER,
            Knopf::Feuer => taste::STRG,
            Knopf::Hoch => taste::HOCH,
            Knopf::Runter => taste::RUNTER,
            Knopf::Links => taste::LINKS,
            Knopf::Rechts => taste::RECHTS,
            Knopf::Ok => taste::ENTER,
        }
    }

    fn gehalten(self) -> bool {
        matches!(self, Knopf::Feuer | Knopf::Springen)
    }
}

struct KnopfDef {
    k: Knopf,
    x: f32,
    y: f32,
    r: f32,
    farbe: u32,
}

const fn kd(k: Knopf, x: f32, y: f32, r: f32, farbe: u32) -> KnopfDef {
    KnopfDef { k, x, y, r, farbe }
}

const KNOEPFE_SPIEL: [KnopfDef; 4] = [
    kd(Knopf::Menue, 600.0, 36.0, 25.0, 0xFFFFFF),
    kd(Knopf::Waffe, 600.0, 120.0, 28.0, 0xFFD860),
    kd(Knopf::Springen, 600.0, 214.0, 31.0, 0x9CE7FF),
    kd(Knopf::Feuer, 600.0, 310.0, 36.0, 0xFF5A48),
];
const KNOEPFE_MENUE: [KnopfDef; 6] = [
    kd(Knopf::Menue, 600.0, 36.0, 25.0, 0xFFFFFF),
    kd(Knopf::Hoch, 600.0, 126.0, 28.0, 0xFFFFFF),
    kd(Knopf::Runter, 600.0, 208.0, 28.0, 0xFFFFFF),
    kd(Knopf::Ok, 600.0, 304.0, 34.0, 0x8CFF8C),
    kd(Knopf::Links, 40.0, 240.0, 26.0, 0xFFFFFF),
    kd(Knopf::Rechts, 40.0, 314.0, 26.0, 0xFFFFFF),
];
const KNOEPFE_VORFUEHRUNG: [KnopfDef; 1] = [kd(Knopf::Menue, 600.0, 36.0, 25.0, 0xFFFFFF)];

fn knoepfe(m: Modus) -> &'static [KnopfDef] {
    match m {
        Modus::Spiel => &KNOEPFE_SPIEL,
        Modus::Menue => &KNOEPFE_MENUE,
        Modus::Vorfuehrung => &KNOEPFE_VORFUEHRUNG,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Rolle {
    Stick { x0: f32, y0: f32 },
    Blick,
    Knopf(Knopf),
    Tipp,
    Aus,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    x: f32,
    y: f32,
    weg: f32,
    rolle: Rolle,
}

/// Ergebnis eines Bildes.
#[derive(Debug, Default, PartialEq)]
pub struct Ausgabe {
    pub tasten: Vec<(bool, i32)>,
    /// Mausbewegung (x = drehen, y = hoch/runter sehen).
    pub maus: (i32, i32),
    /// Achsen: [0] seitwärts (+ rechts), [1] laufen (− vorwärts).
    pub achsen: [f32; 6],
}

pub struct Steuerung {
    finger: [Option<Finger>; MAX_TOUCH],
    gehalten: Vec<i32>,
    pulse: Vec<(i32, f32)>,
    maus_rest: (f32, f32),
    modus: Modus,
}

impl Default for Steuerung {
    fn default() -> Self {
        Self::new()
    }
}

impl Steuerung {
    pub fn new() -> Self {
        Self { finger: [None; MAX_TOUCH], gehalten: Vec::new(), pulse: Vec::new(), maus_rest: (0.0, 0.0), modus: Modus::Vorfuehrung }
    }

    fn treffer(&self, x: f32, y: f32) -> Option<Knopf> {
        knoepfe(self.modus).iter().find(|k| laenge(x - k.x, y - k.y) <= k.r + 8.0).map(|k| k.k)
    }

    fn puls(&mut self, aus: &mut Ausgabe, t: i32) {
        aus.tasten.push((true, t));
        self.pulse.push((t, PULS));
    }

    pub fn update(&mut self, input: &Input, modus: Modus, dt: f32) -> Ausgabe {
        let mut aus = Ausgabe::default();
        if modus != self.modus {
            self.modus = modus;
            for f in self.finger.iter_mut().flatten() {
                f.rolle = Rolle::Aus;
            }
        }
        let punkte: Vec<(f32, f32)> = input.touch.iter().flatten().map(|&(x, y)| (x as f32, y as f32)).collect();
        let alt = self.finger;
        let mut neu: [Option<Finger>; MAX_TOUCH] = [None; MAX_TOUCH];
        let mut benutzt = [false; MAX_TOUCH];
        let mut paare: Vec<(f32, usize, usize)> = Vec::new();
        for (i, p) in punkte.iter().enumerate() {
            for (j, f) in alt.iter().enumerate() {
                if let Some(f) = f {
                    let d = laenge(p.0 - f.x, p.1 - f.y);
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
        let mut dreh = (0.0f32, 0.0f32);
        for (i, &(x, y)) in punkte.iter().enumerate().take(MAX_TOUCH) {
            let f = match zu[i].and_then(|j| alt[j]) {
                Some(mut f) => {
                    let (dx, dy) = (x - f.x, y - f.y);
                    f.weg += laenge(dx, dy);
                    match f.rolle {
                        Rolle::Blick | Rolle::Knopf(Knopf::Feuer) => {
                            dreh.0 += dx;
                            dreh.1 += dy;
                        }
                        Rolle::Stick { x0, y0 } => {
                            let (vx, vy) = (x - x0, y - y0);
                            let l = laenge(vx, vy);
                            if l > STICK_R {
                                f.rolle = Rolle::Stick { x0: x - vx / l * STICK_R, y0: y - vy / l * STICK_R };
                            }
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
                                self.puls(&mut aus, k.taste());
                            }
                            Rolle::Knopf(k)
                        }
                        None if modus == Modus::Spiel && x < MITTE_X => Rolle::Stick { x0: x, y0: y },
                        None if modus == Modus::Spiel => Rolle::Blick,
                        None => Rolle::Tipp,
                    };
                    Finger { x, y, weg: 0.0, rolle }
                }
            };
            neu[i] = Some(f);
        }
        for (j, f) in alt.iter().enumerate() {
            if let Some(f) = f
                && !benutzt[j]
                && f.rolle == Rolle::Tipp
                && f.weg < 14.0
                && modus == Modus::Vorfuehrung
            {
                self.puls(&mut aus, taste::ESC);
            }
        }
        self.finger = neu;
        // Drehen: Mauseinheiten (Rest für das nächste Bild).
        self.maus_rest.0 += dreh.0 * MAUS_JE_PX;
        self.maus_rest.1 += dreh.1 * MAUS_JE_PX;
        let (mx, my) = (self.maus_rest.0.trunc(), self.maus_rest.1.trunc());
        self.maus_rest.0 -= mx;
        self.maus_rest.1 -= my;
        aus.maus = (mx as i32, my as i32);
        // Knüppel → Achsen.
        for f in self.finger.iter().flatten() {
            if let Rolle::Stick { x0, y0 } = f.rolle {
                aus.achsen[0] = ((f.x - x0) / STICK_R).clamp(-1.0, 1.0);
                aus.achsen[1] = ((f.y - y0) / STICK_R).clamp(-1.0, 1.0);
            }
        }
        // Gehaltene Knöpfe.
        let mut soll: Vec<i32> = self
            .finger
            .iter()
            .flatten()
            .filter_map(|f| match f.rolle {
                Rolle::Knopf(k) if k.gehalten() => Some(k.taste()),
                _ => None,
            })
            .collect();
        soll.dedup();
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
        self.gehalten = soll;
        for p in &mut self.pulse {
            p.1 -= dt;
            if p.1 <= 0.0 {
                aus.tasten.push((false, p.0));
            }
        }
        self.pulse.retain(|p| p.1 > 0.0);
        aus
    }

    pub fn zeichne(&self, c: &mut Canvas) {
        let gedrueckt = |k: Knopf| self.finger.iter().flatten().any(|f| f.rolle == Rolle::Knopf(k));
        for d in knoepfe(self.modus) {
            let an = gedrueckt(d.k);
            let farbe = hex(d.farbe);
            c.mix_disc(d.x, d.y, d.r, if an { farbe } else { hex(0x0C0C12) }, if an { 16 } else { 20 });
            c.mix_ring(d.x, d.y, d.r, 2.0, farbe, if an { 32 } else { 20 });
            symbol(c, d, if an { hex(0x101014) } else { farbe }, self.modus);
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
                let l = laenge(vx, vy).max(1.0);
                let k = l.min(STICK_R) / l;
                c.mix_disc(x0 + vx * k, y0 + vy * k, 20.0, hex(0xFFFFFF), 14);
            }
            None => {
                c.mix_ring(160.0, 228.0, STICK_R, 2.0, hex(0xFFFFFF), 6);
                c.mix_disc(160.0, 228.0, 18.0, hex(0xFFFFFF), 5);
            }
        }
    }
}

fn symbol(c: &mut Canvas, d: &KnopfDef, f: u16, modus: Modus) {
    let (x, y) = (d.x, d.y);
    let p = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> { pts.iter().map(|&(a, b)| (x + a, y + b)).collect() };
    let r = |c: &mut Canvas, x0: f32, y0: f32, w: f32, h: f32| c.fill_rect((x + x0) as i32, (y + y0) as i32, w as i32, h as i32, f);
    match d.k {
        Knopf::Menue if modus == Modus::Spiel || modus == Modus::Vorfuehrung => {
            for dy in [-8.0, -1.0, 6.0] {
                r(c, -10.0, dy, 20.0, 3.0);
            }
        }
        Knopf::Menue => {
            c.fill_poly(&p(&[(-12.0, 0.0), (-3.0, -9.0), (-3.0, 9.0)]), f);
            r(c, -4.0, -2.0, 15.0, 4.0);
        }
        Knopf::Waffe => {
            c.fill_poly(&p(&[(-15.0, -7.0), (8.0, -7.0), (8.0, -1.0), (-15.0, -1.0)]), f);
            c.fill_poly(&p(&[(1.0, -1.0), (8.0, -1.0), (10.0, 10.0), (4.0, 10.0)]), f);
            c.fill_poly(&p(&[(11.0, -16.0), (17.0, -12.0), (11.0, -8.0)]), f);
            r(c, 4.0, -13.0, 8.0, 2.0);
        }
        Knopf::Springen => {
            c.fill_poly(&p(&[(0.0, -15.0), (12.0, -2.0), (5.0, -2.0), (5.0, 12.0), (-5.0, 12.0), (-5.0, -2.0), (-12.0, -2.0)]), f);
        }
        Knopf::Feuer => {
            c.mix_ring(x, y, 13.0, 2.4, f, 32);
            for (a, b, w, h) in [(-19.0, -1.2, 10.0, 2.4), (9.0, -1.2, 10.0, 2.4), (-1.2, -19.0, 2.4, 10.0), (-1.2, 9.0, 2.4, 10.0)] {
                r(c, a, b, w, h);
            }
            c.mix_disc(x, y, 2.6, f, 32);
        }
        Knopf::Hoch => c.fill_poly(&p(&[(0.0, -12.0), (12.0, 7.0), (-12.0, 7.0)]), f),
        Knopf::Runter => c.fill_poly(&p(&[(0.0, 12.0), (12.0, -7.0), (-12.0, -7.0)]), f),
        Knopf::Links => c.fill_poly(&p(&[(-12.0, 0.0), (7.0, -12.0), (7.0, 12.0)]), f),
        Knopf::Rechts => c.fill_poly(&p(&[(12.0, 0.0), (-7.0, -12.0), (-7.0, 12.0)]), f),
        Knopf::Ok => c.text_centered(x as i32, y as i32 - 7, "OK", f, hex(0x000000), 2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 55.0;

    #[test]
    fn knueppel_gibt_achsen_und_wischen_maus() {
        let mut s = Steuerung::new();
        let zwei = |a: (u16, u16), b: (u16, u16)| Input { held: 0, touch: [Some(a), Some(b)] };
        s.update(&zwei((150, 250), (420, 200)), Modus::Spiel, DT);
        let a = s.update(&zwei((150, 200), (440, 190)), Modus::Spiel, DT);
        assert!(a.achsen[1] < -0.9, "vorwärts (Achse negativ)");
        assert_eq!(a.maus, (80, -40), "rechts und hoch gewischt");
    }

    #[test]
    fn feuer_und_springen_gehalten() {
        let mut s = Steuerung::new();
        let a = s.update(&Input::touch_at(600, 310), Modus::Spiel, DT);
        assert_eq!(a.tasten, vec![(true, taste::STRG)]);
        let a = s.update(&Input::default(), Modus::Spiel, DT);
        assert_eq!(a.tasten, vec![(false, taste::STRG)]);
        let a = s.update(&Input::touch_at(600, 214), Modus::Spiel, DT);
        assert_eq!(a.tasten, vec![(true, taste::LEER)]);
    }

    #[test]
    fn menue_hat_pfeile_und_ok() {
        let mut s = Steuerung::new();
        let a = s.update(&Input::touch_at(600, 208), Modus::Menue, DT);
        assert_eq!(a.tasten, vec![(true, taste::RUNTER)]);
        s.update(&Input::default(), Modus::Menue, DT);
        let a = s.update(&Input::touch_at(600, 304), Modus::Menue, DT);
        assert_eq!(a.tasten, vec![(true, taste::ENTER)]);
    }

    #[test]
    fn tipp_in_der_vorfuehrung_oeffnet_menue() {
        let mut s = Steuerung::new();
        s.update(&Input::touch_at(300, 150), Modus::Vorfuehrung, DT);
        let a = s.update(&Input::default(), Modus::Vorfuehrung, DT);
        assert_eq!(a.tasten, vec![(true, taste::ESC)]);
    }
}
