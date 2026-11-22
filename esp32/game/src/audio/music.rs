//! Die Musik: zwei selbst komponierte Stücke im Tracker-Format.
//!
//! Pro Muster 32 Zeilen (zwei 4/4-Takte in Sechzehnteln) und vier Spuren:
//! Melodie, Begleitung (Arpeggien), Bass, Schlagzeug. Die Noten stehen gut
//! lesbar als Text im Quelltext (`C-5`, `F#4`, `...` = halten, `===` = aus)
//! und werden **zur Übersetzungszeit** in Bytes umgewandelt – sie liegen also
//! fertig im Flash, der Audio-Kern liest sie direkt (kein Heap, kein PSRAM).
//!
//! * „Pudding-Panik“ (Level): C-Dur, 138 BPM, hüpfende Arpeggien,
//!   Ohrwurm-Melodie über C–G–Am–F, B-Teil F–G–Em–Am / F–G–C.
//! * „Kühlschrank-Lounge“ (Menü): F-Dur, 96 BPM, entspannte Septakkorde.

use super::{SongId, Wave};

pub const HOLD: u8 = 0;
pub const OFF: u8 = 1;

/// Klangfarbe einer Spur.
#[derive(Clone, Copy, Debug)]
pub struct Instr {
    pub wave: Wave,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub duty: f32,
    /// Pulsbreitenmodulation (Hz, 0 = aus).
    pub pwm: f32,
    /// Vibrato in Halbtönen, Rate (Hz), Verzögerung (s).
    pub vib: f32,
    pub vib_rate: f32,
    pub vib_delay: f32,
    /// Tiefpass (1 = offen, kleiner = weicher).
    pub lp: f32,
    pub vol: f32,
    pub pan: f32,
    /// Tonhöhenänderung pro Sample (Hz) – z. B. für die Bass-Trommel.
    pub slide: f32,
    /// Feste Frequenz statt Note (Schlagzeug), 0 = Note verwenden.
    pub fixed: f32,
    pub pitch_mul: f32,
    /// Automatisch loslassen nach (s), 0 = bis zur nächsten Note.
    pub gate: f32,
    pub legato: bool,
}

impl Instr {
    const BASE: Instr = Instr {
        wave: Wave::Pulse,
        attack: 0.003,
        decay: 0.2,
        sustain: 0.6,
        release: 0.08,
        duty: 0.5,
        pwm: 0.0,
        vib: 0.0,
        vib_rate: 0.0,
        vib_delay: 0.0,
        lp: 1.0,
        vol: 0.3,
        pan: 0.0,
        slide: 0.0,
        fixed: 0.0,
        pitch_mul: 1.0,
        gate: 0.0,
        legato: false,
    };
}

pub const KICK: Instr = Instr {
    wave: Wave::Sine,
    attack: 0.0,
    decay: 0.16,
    sustain: 0.0,
    release: 0.05,
    vol: 0.95,
    slide: -0.05,
    fixed: 165.0,
    ..Instr::BASE
};
pub const SNARE: Instr = Instr {
    wave: Wave::Noise,
    attack: 0.0,
    decay: 0.13,
    sustain: 0.0,
    release: 0.05,
    lp: 0.55,
    vol: 0.5,
    fixed: 7000.0,
    ..Instr::BASE
};
pub const HAT: Instr = Instr {
    wave: Wave::Noise,
    attack: 0.0,
    decay: 0.03,
    sustain: 0.0,
    release: 0.02,
    vol: 0.17,
    pan: 0.25,
    fixed: 15000.0,
    ..Instr::BASE
};
pub const OPEN_HAT: Instr = Instr { decay: 0.16, vol: 0.14, ..HAT };
pub const CLAP: Instr = Instr {
    wave: Wave::Noise,
    attack: 0.0,
    decay: 0.09,
    sustain: 0.0,
    release: 0.04,
    lp: 0.7,
    vol: 0.38,
    pan: -0.1,
    fixed: 5000.0,
    ..Instr::BASE
};

/// Ein Muster: 32 Zeilen je Spur.
#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    pub lead: [u8; 32],
    pub harm: [u8; 32],
    pub bass: [u8; 32],
    pub drums: [u8; 32],
}

/// Ein Musikstück.
#[derive(Debug)]
pub struct Song {
    pub bpm: u16,
    pub lead: Instr,
    pub harm: Instr,
    pub bass: Instr,
    pub patterns: &'static [Pattern],
    pub order: &'static [u8],
}

pub fn song(id: SongId) -> &'static Song {
    match id {
        SongId::Menu => &MENU,
        SongId::Level => &LEVEL,
    }
}

// ---------------------------------------------------------------------------
// Notenschrift → Bytes (zur Übersetzungszeit)
// ---------------------------------------------------------------------------

/// 16 Noten eines Takts: `C-5 D#4 ... ===`.
const fn bar(s: &str) -> [u8; 16] {
    let b = s.as_bytes();
    let mut out = [HOLD; 16];
    let (mut i, mut n) = (0, 0);
    while i < b.len() {
        if b[i] == b' ' {
            i += 1;
            continue;
        }
        assert!(n < 16 && i + 2 < b.len(), "Takt hat mehr als 16 Noten oder eine halbe Note");
        let (c0, c1, c2) = (b[i], b[i + 1], b[i + 2]);
        out[n] = if c0 == b'.' {
            HOLD
        } else if c0 == b'=' {
            OFF
        } else {
            let base = match c0 {
                b'C' => 0,
                b'D' => 2,
                b'E' => 4,
                b'F' => 5,
                b'G' => 7,
                b'A' => 9,
                b'B' => 11,
                _ => panic!("unbekannte Note"),
            };
            let sharp = if c1 == b'#' { 1 } else { 0 };
            assert!(c2 >= b'0' && c2 <= b'8', "Oktave fehlt");
            (c2 - b'0' + 1) * 12 + base + sharp
        };
        n += 1;
        i += 3;
    }
    assert!(n == 16, "Takt braucht genau 16 Noten");
    out
}

/// 16 Schläge eines Takts: `K` Bass-, `S` Snare-, `C` Klatsch-, `H` Hihat,
/// `O` offenes Hihat, `.` Pause.
const fn beat(s: &str) -> [u8; 16] {
    let b = s.as_bytes();
    let mut out = [b'.'; 16];
    let (mut i, mut n) = (0, 0);
    while i < b.len() {
        if b[i] != b' ' {
            assert!(n < 16, "Takt hat mehr als 16 Schläge");
            out[n] = b[i];
            n += 1;
        }
        i += 1;
    }
    assert!(n == 16, "Takt braucht genau 16 Schläge");
    out
}

const fn two(a: [u8; 16], b: [u8; 16]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 16 {
        out[i] = a[i];
        out[i + 16] = b[i];
        i += 1;
    }
    out
}

const REST: [u8; 16] = [HOLD; 16];

// ---------------------------------------------------------------------------
// „Pudding-Panik“ – Levelmusik
// ---------------------------------------------------------------------------

// Begleitung: Akkord-Arpeggien in Sechzehnteln.
const ARP_C: [u8; 16] = bar("C-5 E-5 G-5 C-6 G-5 E-5 C-5 E-5 G-5 C-6 G-5 E-5 C-5 E-5 G-5 E-5");
const ARP_G: [u8; 16] = bar("B-4 D-5 G-5 B-5 G-5 D-5 B-4 D-5 G-5 B-5 G-5 D-5 B-4 D-5 G-5 D-5");
const ARP_AM: [u8; 16] = bar("A-4 C-5 E-5 A-5 E-5 C-5 A-4 C-5 E-5 A-5 E-5 C-5 A-4 C-5 E-5 C-5");
const ARP_F: [u8; 16] = bar("A-4 C-5 F-5 A-5 F-5 C-5 A-4 C-5 F-5 A-5 F-5 C-5 A-4 C-5 F-5 C-5");
const ARP_EM: [u8; 16] = bar("G-4 B-4 E-5 G-5 E-5 B-4 G-4 B-4 E-5 G-5 E-5 B-4 G-4 B-4 E-5 B-4");

// Bass: hüpfende Oktaven in Achteln, am Taktende ein Durchgang.
const BASS_C: [u8; 16] = bar("C-3 ... C-4 ... C-3 ... C-4 ... C-3 ... C-4 ... C-3 ... B-2 ...");
const BASS_G: [u8; 16] = bar("G-2 ... G-3 ... G-2 ... G-3 ... G-2 ... G-3 ... G-2 ... B-2 ...");
const BASS_AM: [u8; 16] = bar("A-2 ... A-3 ... A-2 ... A-3 ... A-2 ... A-3 ... A-2 ... G-2 ...");
const BASS_F: [u8; 16] = bar("F-2 ... F-3 ... F-2 ... F-3 ... F-2 ... F-3 ... G-2 ... G-3 ...");
const BASS_EM: [u8; 16] = bar("E-2 ... E-3 ... E-2 ... E-3 ... E-2 ... E-3 ... E-2 ... G-2 ...");
const BASS_CEND: [u8; 16] = bar("C-3 ... C-4 ... C-3 ... C-4 ... G-2 ... ... ... B-2 ... ... ...");

const GROOVE: [u8; 16] = beat("K.H. S.H. K.KH S.HH");
const GROOVE_B: [u8; 16] = beat("K.H. S.HO K.KH S.HH");
const FILL: [u8; 16] = beat("K.H. S.H. K.S. SSCS");
const INTRO_BEAT: [u8; 16] = beat("K... H... K... H.HH");

static LEVEL_PATTERNS: [Pattern; 7] = [
    // 0: A1 – C | G
    Pattern {
        lead: two(
            bar("E-5 ... G-5 ... C-6 ... ... ... B-5 ... A-5 ... G-5 ... E-5 ..."),
            bar("D-5 ... ... ... G-5 ... ... ... B-5 ... A-5 ... G-5 ... ... ..."),
        ),
        harm: two(ARP_C, ARP_G),
        bass: two(BASS_C, BASS_G),
        drums: two(GROOVE, GROOVE),
    },
    // 1: A2 – Am | F
    Pattern {
        lead: two(
            bar("C-6 ... ... ... B-5 ... A-5 ... E-5 ... ... ... A-5 ... C-6 ..."),
            bar("A-5 ... G-5 ... F-5 ... ... ... A-5 ... ... ... G-5 ... ... ..."),
        ),
        harm: two(ARP_AM, ARP_F),
        bass: two(BASS_AM, BASS_F),
        drums: two(GROOVE, FILL),
    },
    // 2: B1 – F | G
    Pattern {
        lead: two(
            bar("F-5 ... A-5 ... C-6 ... A-5 ... F-5 ... A-5 ... C-6 ... D-6 ..."),
            bar("D-6 ... ... ... B-5 ... ... ... G-5 ... A-5 ... B-5 ... D-6 ..."),
        ),
        harm: two(ARP_F, ARP_G),
        bass: two(BASS_F, BASS_G),
        drums: two(GROOVE_B, GROOVE_B),
    },
    // 3: B2 – Em | Am
    Pattern {
        lead: two(
            bar("E-6 ... ... ... D-6 ... B-5 ... G-5 ... ... ... E-5 ... G-5 ..."),
            bar("A-5 ... ... ... ... ... ... ... C-6 ... B-5 ... A-5 ... ... ..."),
        ),
        harm: two(ARP_EM, ARP_AM),
        bass: two(BASS_EM, BASS_AM),
        drums: two(GROOVE_B, GROOVE_B),
    },
    // 4: B3 – F | G
    Pattern {
        lead: two(
            bar("F-5 ... A-5 ... C-6 ... F-6 ... E-6 ... ... ... C-6 ... ... ..."),
            bar("D-6 ... ... ... B-5 ... G-5 ... D-6 ... ... ... ... ... ... ..."),
        ),
        harm: two(ARP_F, ARP_G),
        bass: two(BASS_F, BASS_G),
        drums: two(GROOVE_B, GROOVE_B),
    },
    // 5: B4 – C | C (Schluss mit Wirbel)
    Pattern {
        lead: two(
            bar("C-6 ... G-5 ... E-5 ... G-5 ... C-6 ... ... ... E-6 ... ... ..."),
            bar("D-6 ... ... ... C-6 ... ... ... ... ... ... ... === ... ... ..."),
        ),
        harm: two(ARP_C, ARP_C),
        bass: two(BASS_C, BASS_CEND),
        drums: two(GROOVE, FILL),
    },
    // 6: Intro – nur Begleitung, Bass und leichtes Schlagzeug
    Pattern { lead: two(REST, REST), harm: two(ARP_C, ARP_G), bass: two(BASS_C, BASS_G), drums: two(INTRO_BEAT, FILL) },
];

static LEVEL_ORDER: [u8; 11] = [6, 0, 1, 0, 1, 2, 3, 4, 5, 0, 1];

pub static LEVEL: Song = Song {
    bpm: 138,
    lead: Instr {
        wave: Wave::Pulse,
        attack: 0.004,
        decay: 0.25,
        sustain: 0.55,
        release: 0.12,
        duty: 0.25,
        pwm: 0.35,
        vib: 0.25,
        vib_rate: 5.5,
        vib_delay: 0.18,
        lp: 0.55,
        vol: 0.30,
        pan: -0.15,
        ..Instr::BASE
    },
    harm: Instr {
        wave: Wave::Pulse,
        attack: 0.002,
        decay: 0.09,
        sustain: 0.25,
        release: 0.06,
        duty: 0.125,
        lp: 0.45,
        vol: 0.17,
        pan: 0.35,
        ..Instr::BASE
    },
    bass: Instr {
        wave: Wave::Pulse,
        attack: 0.003,
        decay: 0.2,
        sustain: 0.6,
        release: 0.05,
        duty: 0.5,
        lp: 0.22,
        vol: 0.42,
        ..Instr::BASE
    },
    patterns: &LEVEL_PATTERNS,
    order: &LEVEL_ORDER,
};

// ---------------------------------------------------------------------------
// „Kühlschrank-Lounge“ – Menümusik
// ---------------------------------------------------------------------------

const MARP_FMAJ7: [u8; 16] = bar("F-4 ... A-4 ... C-5 ... E-5 ... C-5 ... A-4 ... C-5 ... E-5 ...");
const MARP_EM7: [u8; 16] = bar("E-4 ... G-4 ... B-4 ... D-5 ... B-4 ... G-4 ... B-4 ... D-5 ...");
const MARP_DM7: [u8; 16] = bar("D-4 ... F-4 ... A-4 ... C-5 ... A-4 ... F-4 ... A-4 ... C-5 ...");
const MARP_CMAJ7: [u8; 16] = bar("C-4 ... E-4 ... G-4 ... B-4 ... G-4 ... E-4 ... G-4 ... B-4 ...");
const MARP_BBMAJ7: [u8; 16] = bar("A#3 ... D-4 ... F-4 ... A-4 ... F-4 ... D-4 ... F-4 ... A-4 ...");
const MARP_AM7: [u8; 16] = bar("A-3 ... C-4 ... E-4 ... G-4 ... E-4 ... C-4 ... E-4 ... G-4 ...");
const MARP_GM7: [u8; 16] = bar("G-3 ... A#3 ... D-4 ... F-4 ... D-4 ... A#3 ... D-4 ... F-4 ...");
const MARP_C7: [u8; 16] = bar("C-4 ... E-4 ... G-4 ... A#4 ... G-4 ... E-4 ... G-4 ... A#4 ...");

const MBASS_F: [u8; 16] = bar("F-3 ... ... ... ... ... ... ... C-3 ... ... ... A-3 ... ... ...");
const MBASS_EM: [u8; 16] = bar("E-3 ... ... ... ... ... ... ... B-2 ... ... ... G-3 ... ... ...");
const MBASS_DM: [u8; 16] = bar("D-3 ... ... ... ... ... ... ... A-2 ... ... ... F-3 ... ... ...");
const MBASS_C: [u8; 16] = bar("C-3 ... ... ... ... ... ... ... G-2 ... ... ... E-3 ... ... ...");
const MBASS_BB: [u8; 16] = bar("A#2 ... ... ... ... ... ... ... F-3 ... ... ... D-3 ... ... ...");
const MBASS_AM: [u8; 16] = bar("A-2 ... ... ... ... ... ... ... E-3 ... ... ... C-3 ... ... ...");
const MBASS_GM: [u8; 16] = bar("G-2 ... ... ... ... ... ... ... D-3 ... ... ... A#2 ... ... ...");
const MBASS_C7: [u8; 16] = bar("C-3 ... ... ... ... ... ... ... G-2 ... ... ... A#2 ... ... ...");

const LOUNGE: [u8; 16] = beat("K.H. C.H. K.HH C..H");
const LOUNGE_END: [u8; 16] = beat("K.H. C.H. K.H. C.CC");

static MENU_PATTERNS: [Pattern; 4] = [
    Pattern {
        lead: two(
            bar("A-5 ... ... ... C-6 ... ... ... A-5 ... G-5 ... F-5 ... ... ..."),
            bar("G-5 ... ... ... ... ... E-5 ... G-5 ... ... ... B-5 ... ... ..."),
        ),
        harm: two(MARP_FMAJ7, MARP_EM7),
        bass: two(MBASS_F, MBASS_EM),
        drums: two(LOUNGE, LOUNGE),
    },
    Pattern {
        lead: two(
            bar("A-5 ... ... ... F-5 ... ... ... D-5 ... F-5 ... A-5 ... ... ..."),
            bar("G-5 ... ... ... ... ... ... ... E-5 ... ... ... ... ... ... ..."),
        ),
        harm: two(MARP_DM7, MARP_CMAJ7),
        bass: two(MBASS_DM, MBASS_C),
        drums: two(LOUNGE, LOUNGE),
    },
    Pattern {
        lead: two(
            bar("F-5 ... A-5 ... C-6 ... E-6 ... D-6 ... C-6 ... A-5 ... ... ..."),
            bar("C-6 ... ... ... A-5 ... ... ... E-5 ... ... ... G-5 ... ... ..."),
        ),
        harm: two(MARP_BBMAJ7, MARP_AM7),
        bass: two(MBASS_BB, MBASS_AM),
        drums: two(LOUNGE, LOUNGE),
    },
    Pattern {
        lead: two(
            bar("A#5 ... ... ... A-5 ... G-5 ... F-5 ... ... ... D-5 ... ... ..."),
            bar("E-5 ... ... ... G-5 ... ... ... C-6 ... ... ... === ... ... ..."),
        ),
        harm: two(MARP_GM7, MARP_C7),
        bass: two(MBASS_GM, MBASS_C7),
        drums: two(LOUNGE, LOUNGE_END),
    },
];

static MENU_ORDER: [u8; 4] = [0, 1, 2, 3];

pub static MENU: Song = Song {
    bpm: 96,
    lead: Instr {
        wave: Wave::Triangle,
        attack: 0.02,
        decay: 0.6,
        sustain: 0.6,
        release: 0.25,
        vib: 0.2,
        vib_rate: 4.5,
        vib_delay: 0.25,
        vol: 0.40,
        pan: -0.1,
        ..Instr::BASE
    },
    harm: Instr {
        wave: Wave::Saw,
        attack: 0.01,
        decay: 0.3,
        sustain: 0.35,
        release: 0.2,
        lp: 0.18,
        vol: 0.16,
        pan: 0.3,
        ..Instr::BASE
    },
    bass: Instr {
        wave: Wave::Pulse,
        attack: 0.004,
        decay: 0.4,
        sustain: 0.6,
        release: 0.1,
        lp: 0.2,
        vol: 0.38,
        ..Instr::BASE
    },
    patterns: &MENU_PATTERNS,
    order: &MENU_ORDER,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notation_parses() {
        let b = bar("C-4 ... === C#4 A-4 ... ... ... ... ... ... ... ... ... ... B-8");
        assert_eq!(b[0], 60);
        assert_eq!(b[1], HOLD);
        assert_eq!(b[2], OFF);
        assert_eq!(b[3], 61);
        assert_eq!(b[4], 69);
        assert_eq!(beat("K.H. S.H. K.KH S.HH")[10], b'K');
        for s in [&LEVEL, &MENU] {
            assert!(s.order.iter().all(|&o| (o as usize) < s.patterns.len()));
        }
    }
}
