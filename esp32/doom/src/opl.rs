//! Kleiner FM-Synthesizer nach Art des OPL2 (YM3812, „AdLib“), für den die
//! Doom-Musik geschrieben ist: je Stimme zwei Operatoren (Modulator →
//! Träger, oder beide hörbar), vier Wellenformen, Hüllkurven mit den Zeiten
//! des Chips, Rückkopplung, Vibrato und Tremolo. Die Klangfarben kommen aus
//! dem GENMIDI-Lump der WAD ([`Instrument`]).

use crate::klang::SAMPLE_RATE_F;

/// Einstellungen eines Operators (wie in den OPL-Registern 0x20–0xE0).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OpParam {
    pub tremolo: bool,
    pub vibrato: bool,
    /// Hält auf dem Sustain-Pegel, solange die Taste gedrückt ist.
    pub halten: bool,
    pub ksr: bool,
    pub mult: u8,
    pub ar: u8,
    pub dr: u8,
    pub sl: u8,
    pub rr: u8,
    pub welle: u8,
    pub ksl: u8,
    pub tl: u8,
}

impl OpParam {
    /// Aus den 6 Bytes eines GENMIDI-Operators.
    pub fn aus(b: &[u8]) -> Self {
        Self {
            tremolo: b[0] & 0x80 != 0,
            vibrato: b[0] & 0x40 != 0,
            halten: b[0] & 0x20 != 0,
            ksr: b[0] & 0x10 != 0,
            mult: b[0] & 0x0F,
            ar: b[1] >> 4,
            dr: b[1] & 0x0F,
            sl: b[2] >> 4,
            rr: b[2] & 0x0F,
            welle: b[3] & 0x03,
            ksl: b[4] >> 6,
            tl: b[5] & 0x3F,
        }
    }
}

/// Eine Stimme eines GENMIDI-Instruments.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StimmenParam {
    pub modulator: OpParam,
    pub traeger: OpParam,
    /// Rückkopplung des Modulators 0..=7.
    pub rueck: u8,
    /// Beide Operatoren hörbar (statt Frequenzmodulation).
    pub additiv: bool,
    pub grundton: i16,
}

/// Instrument aus dem GENMIDI-Lump.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Instrument {
    pub fest: bool,
    pub zwei_stimmen: bool,
    pub feinstimmung: u8,
    pub fester_ton: u8,
    pub stimmen: [StimmenParam; 2],
}

/// GENMIDI: „#OPL_II#“, dann 175 Instrumente zu 36 Bytes (128 Melodie-,
/// 47 Schlaginstrumente für die Töne 35–81).
/// Alle 175 GENMIDI-Instrumente.
pub type Instrumente = Box<[Instrument; 175]>;

pub fn genmidi(lump: &[u8]) -> Option<Instrumente> {
    if lump.len() < 8 + 175 * 36 || &lump[..8] != b"#OPL_II#" {
        return None;
    }
    // Gleich auf dem Heap (175 × 48 Byte): Auf dem Board läuft das im
    // Ton-Thread mit 6 KB Stapel – ein Feld auf dem Stapel sprengte ihn.
    let mut v: Instrumente = vec![Instrument::default(); 175].into_boxed_slice().try_into().ok()?;
    for (i, inst) in v.iter_mut().enumerate() {
        let b = &lump[8 + i * 36..8 + (i + 1) * 36];
        let flags = u16::from_le_bytes([b[0], b[1]]);
        let stimme = |o: usize| {
            let s = &b[4 + o * 16..4 + (o + 1) * 16];
            StimmenParam {
                modulator: OpParam::aus(&s[0..6]),
                rueck: (s[6] >> 1) & 7,
                additiv: s[6] & 1 != 0,
                traeger: OpParam::aus(&s[7..13]),
                grundton: i16::from_le_bytes([s[14], s[15]]),
            }
        };
        *inst = Instrument { fest: flags & 1 != 0, zwei_stimmen: flags & 4 != 0, feinstimmung: b[2], fester_ton: b[3], stimmen: [stimme(0), stimme(1)] };
    }
    Some(v)
}

const SINUS_N: usize = 1024;
/// Hüllkurve und Lautstärke werden alle 8 Samples neu berechnet.
pub const EG_TAKT: usize = 8;
const STILL_DB: f32 = 96.0;

fn sinus_tabelle() -> Box<[f32; SINUS_N]> {
    let mut t = Box::new([0.0f32; SINUS_N]);
    for (i, v) in t.iter_mut().enumerate() {
        *v = (i as f32 / SINUS_N as f32 * core::f32::consts::TAU).sin();
    }
    t
}

/// Zeit (ms) einer Hüllkurvenrate (OPL: R = 4·Rate + Tastaturskalierung).
fn rate_ms(basis_ms: f32, r_eff: u32) -> f32 {
    let r = (r_eff >> 2).min(15);
    let teil = (r_eff & 3) as f32;
    basis_ms / (1u32 << (r - 1)) as f32 / (1.0 + 0.25 * teil)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Aus,
    Anstieg,
    Abfall,
    Halten,
    Ausklang,
}

#[derive(Clone, Copy, Debug, Default)]
struct Op {
    p: OpParam,
    phase: f32,
    schritt: f32,
    eg: Phase,
    /// Dämpfung der Hüllkurve in dB (0 = voll, 96 = still).
    att: f32,
    /// Anstieg: Faktor je EG-Takt; Abfall/Ausklang: dB je EG-Takt.
    k_an: f32,
    db_ab: f32,
    db_aus: f32,
    sl_db: f32,
    /// Feste Dämpfung (Pegel, Tonhöhenskalierung, Lautstärke) in dB.
    fest_db: f32,
    amp: f32,
    amp_ziel: f32,
    alt: [f32; 2],
}

impl Op {
    fn starten(&mut self, p: OpParam, f: f32, ton: f32, extra_db: f32) {
        self.p = p;
        let mult = [0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 10.0, 12.0, 12.0, 15.0, 15.0][p.mult as usize];
        self.schritt = f * mult / SAMPLE_RATE_F;
        // Tastaturskalierung der Raten (Oktave · 2 + obere Hälfte).
        let oktave = ((ton - 19.0) / 12.0).clamp(0.0, 7.0);
        let ks = (oktave as u32) * 2 + ((oktave.fract() >= 0.5) as u32);
        let rof = if p.ksr { ks } else { ks >> 2 };
        let takt_ms = EG_TAKT as f32 / SAMPLE_RATE_F * 1000.0;
        self.k_an = if p.ar == 0 {
            0.0
        } else if 4 * p.ar as u32 + rof >= 60 {
            1.0
        } else {
            // Dämpfung schrumpft je Takt um einen Anteil: 96 dB → 0,1 dB in der Anstiegszeit.
            (6.87 * takt_ms / rate_ms(2826.24, 4 * p.ar as u32 + rof)).min(1.0)
        };
        let db = |r: u8| if r == 0 { 0.0 } else { STILL_DB * takt_ms / rate_ms(39280.64, 4 * r as u32 + rof) };
        self.db_ab = db(p.dr);
        self.db_aus = db(p.rr);
        self.sl_db = if p.sl == 15 { 93.0 } else { p.sl as f32 * 3.0 };
        let ksl_db_okt = [0.0, 3.0, 1.5, 6.0][p.ksl as usize];
        let ksl = ksl_db_okt * ((f / 130.81).log2()).max(0.0);
        self.fest_db = p.tl as f32 * 0.75 + ksl + extra_db;
        self.eg = Phase::Anstieg;
        if self.k_an >= 1.0 {
            self.att = 0.0;
            self.eg = Phase::Abfall;
        } else if self.att > STILL_DB || self.amp == 0.0 {
            self.att = STILL_DB;
        }
        self.phase = 0.0;
    }

    fn loslassen(&mut self) {
        if self.eg != Phase::Aus {
            self.eg = Phase::Ausklang;
        }
    }

    /// Hüllkurve einen EG-Takt weiter; neue Ziel-Lautstärke.
    fn eg_takt(&mut self, trem_db: f32) {
        match self.eg {
            Phase::Aus => self.att = STILL_DB,
            Phase::Anstieg => {
                self.att -= self.att * self.k_an + if self.k_an > 0.0 { 0.01 } else { 0.0 };
                if self.att <= 0.1 {
                    self.att = 0.0;
                    self.eg = Phase::Abfall;
                }
            }
            Phase::Abfall => {
                self.att += self.db_ab;
                if self.att >= self.sl_db {
                    self.att = self.sl_db;
                    self.eg = if self.p.halten { Phase::Halten } else { Phase::Ausklang };
                }
            }
            Phase::Halten => {}
            Phase::Ausklang => {
                self.att += self.db_aus;
                if self.att >= STILL_DB {
                    self.att = STILL_DB;
                    self.eg = Phase::Aus;
                }
            }
        }
        let t = if self.p.tremolo { trem_db } else { 0.0 };
        let db = self.att + self.fest_db + t;
        self.amp_ziel = if db >= STILL_DB { 0.0 } else { (-db / 6.0206).exp2() };
    }
}

/// Eine Stimme (zwei Operatoren).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stimme {
    m: Op,
    t: Op,
    rueck: u8,
    additiv: bool,
}

impl Stimme {
    /// Ton `ton` (MIDI-Notennummer, gebrochen für Tonhöhenbeugung) mit
    /// Lautstärke 0..=127 anschlagen.
    pub fn an(&mut self, p: &StimmenParam, ton: f32, lautst: u32) {
        let f = 440.0 * ((ton - 69.0) / 12.0).exp2();
        // Lautstärke wie beim Chip: Trägerpegel (und bei additiv beide) dämpfen.
        // Lautstärkekurve wie Doom (DMX): etwa v^0,6, dann Trägerpegel.
        let anteil = ((lautst.min(127) as f32 / 127.0).powf(0.6) * 127.0 / 128.0).max(1e-3);
        let extra = |tl: u8| (63 - tl.min(63)) as f32 * 0.75 * (1.0 - anteil);
        self.rueck = p.rueck;
        self.additiv = p.additiv;
        self.m.starten(p.modulator, f, ton, if p.additiv { extra(p.modulator.tl) } else { 0.0 });
        self.t.starten(p.traeger, f, ton, extra(p.traeger.tl));
    }

    /// Tonhöhe ändern (Tonhöhenrad), ohne neu anzuschlagen.
    pub fn tonhoehe(&mut self, ton: f32) {
        let f = 440.0 * ((ton - 69.0) / 12.0).exp2();
        let mult = |m: u8| [0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 10.0, 12.0, 12.0, 15.0, 15.0][m as usize];
        self.m.schritt = f * mult(self.m.p.mult) / SAMPLE_RATE_F;
        self.t.schritt = f * mult(self.t.p.mult) / SAMPLE_RATE_F;
    }

    pub fn aus(&mut self) {
        self.m.loslassen();
        self.t.loslassen();
    }

    /// Klingt (noch) hörbar?
    pub fn klingt(&self) -> bool {
        self.t.eg != Phase::Aus || (self.additiv && self.m.eg != Phase::Aus)
    }

    pub fn gedrueckt(&self) -> bool {
        matches!(self.t.eg, Phase::Anstieg | Phase::Abfall | Phase::Halten)
    }
}

/// Neun Stimmen wie der OPL2, mit gemeinsamem Vibrato/Tremolo.
pub struct Opl {
    pub stimmen: [Stimme; 9],
    sinus: Box<[f32; SINUS_N]>,
    lfo_vib: f32,
    lfo_trem: f32,
    zaehler: usize,
}

impl Default for Opl {
    fn default() -> Self {
        Self::new()
    }
}

#[inline(always)]
fn welle(t: &[f32; SINUS_N], w: u8, phase: f32) -> f32 {
    let i = ((phase * SINUS_N as f32) as i32 as usize) & (SINUS_N - 1);
    let s = t[i];
    match w {
        0 => s,
        1 => s.max(0.0),
        2 => s.abs(),
        _ => {
            if i & 256 != 0 {
                0.0
            } else {
                s.abs()
            }
        }
    }
}

impl Opl {
    pub fn new() -> Self {
        Self { stimmen: [Stimme::default(); 9], sinus: sinus_tabelle(), lfo_vib: 0.0, lfo_trem: 0.0, zaehler: 0 }
    }

    /// Mono-Samples (−1..1) addieren.
    pub fn render(&mut self, out: &mut [f32]) {
        let sin = &*self.sinus;
        for o in out.iter_mut() {
            if self.zaehler % EG_TAKT == 0 {
                self.lfo_vib = (self.lfo_vib + 6.07 * EG_TAKT as f32 / SAMPLE_RATE_F).fract();
                self.lfo_trem = (self.lfo_trem + 3.7 * EG_TAKT as f32 / SAMPLE_RATE_F).fract();
                let trem_db = 0.5 + 0.5 * welle(sin, 0, self.lfo_trem);
                for s in self.stimmen.iter_mut() {
                    if s.m.eg == Phase::Aus && s.t.eg == Phase::Aus {
                        s.m.amp = 0.0;
                        s.t.amp = 0.0;
                        continue;
                    }
                    s.m.eg_takt(trem_db);
                    s.t.eg_takt(trem_db);
                }
            }
            self.zaehler = self.zaehler.wrapping_add(1);
            let vib = 1.0 + 0.004 * welle(sin, 0, self.lfo_vib);
            let mut summe = 0.0;
            for s in self.stimmen.iter_mut() {
                if s.m.amp_ziel == 0.0 && s.t.amp_ziel == 0.0 && s.m.amp < 1e-4 && s.t.amp < 1e-4 {
                    continue;
                }
                // Lautstärke weich zum Ziel (kein Knacken zwischen EG-Takten).
                s.m.amp += (s.m.amp_ziel - s.m.amp) * 0.125;
                s.t.amp += (s.t.amp_ziel - s.t.amp) * 0.125;
                let vm = if s.m.p.vibrato { vib } else { 1.0 };
                let vt = if s.t.p.vibrato { vib } else { 1.0 };
                s.m.phase = (s.m.phase + s.m.schritt * vm).fract();
                s.t.phase = (s.t.phase + s.t.schritt * vt).fract();
                let fb = if s.rueck > 0 { (s.m.alt[0] + s.m.alt[1]) * (1u32 << s.rueck) as f32 / 128.0 } else { 0.0 };
                let mo = welle(sin, s.m.p.welle, s.m.phase + fb) * s.m.amp;
                s.m.alt = [s.m.alt[1], mo];
                if s.additiv {
                    summe += mo + welle(sin, s.t.p.welle, s.t.phase) * s.t.amp;
                } else {
                    summe += welle(sin, s.t.p.welle, s.t.phase + mo * 4.0) * s.t.amp;
                }
            }
            *o += summe;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn klavier() -> StimmenParam {
        // Einfacher Klang: Modulator leise, Träger mit Abklingen.
        let m = OpParam { mult: 1, ar: 15, dr: 2, sl: 2, rr: 6, tl: 30, halten: true, ..OpParam::default() };
        let t = OpParam { mult: 1, ar: 15, dr: 3, sl: 4, rr: 7, tl: 0, halten: true, ..OpParam::default() };
        StimmenParam { modulator: m, traeger: t, rueck: 3, additiv: false, grundton: 0 }
    }

    #[test]
    fn ton_klingt_und_verklingt_nach_dem_loslassen() {
        let mut o = Opl::new();
        o.stimmen[0].an(&klavier(), 60.0, 127);
        let mut b = vec![0.0f32; 4410];
        o.render(&mut b);
        let spitze = b.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
        assert!(spitze > 0.3 && spitze <= 1.5, "hörbar ({spitze})");
        assert!(b.iter().all(|x| x.is_finite()));
        o.stimmen[0].aus();
        let mut b = vec![0.0f32; 22050 * 2];
        o.render(&mut b);
        let rest = b[b.len() - 2000..].iter().fold(0.0f32, |a, &x| a.max(x.abs()));
        assert!(rest < 0.01, "nach dem Loslassen still ({rest})");
        assert!(!o.stimmen[0].klingt());
    }

    #[test]
    fn genmidi_aus_der_wad() {
        let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("wad/DOOM1.WAD");
        let Ok(w) = std::fs::read(&pfad) else { return };
        let lump = crate::musik::lump(&w, b"GENMIDI").expect("GENMIDI");
        let inst = genmidi(lump).expect("lesbar");
        // Schlagzeug (Bass Drum, Ton 35/36) hat einen festen Ton.
        assert!(inst[128].fest || inst[129].fest);
        assert!(inst.iter().any(|i| i.zwei_stimmen), "Doppelstimmen vorhanden");
        assert!(inst[0].stimmen[0].traeger.ar > 0, "Klavier schlägt an");
    }
}
