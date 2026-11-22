//! Die Soundeffekte – jeder aus wenigen synthetischen Tönen gebaut.

use super::Wave::{Noise, Pulse, Saw, Sine, Triangle};
use super::{Sfx, Tone};

// Häufige Tonhöhen (Hz).
const C5: f32 = 523.25;
const E5: f32 = 659.26;
const G5: f32 = 783.99;
const A5: f32 = 880.0;
const B5: f32 = 987.77;
const C6: f32 = 1046.5;
const E6: f32 = 1318.5;
const G6: f32 = 1568.0;
const C7: f32 = 2093.0;
const GS6: f32 = 1661.2;
const B6: f32 = 1975.5;
const E7: f32 = 2637.0;

/// Hüpfer: kurzer, aufsteigender Puls.
static JUMP: [Tone; 2] = [
    Tone::new(Pulse, 260.0, 640.0, 0.13, 0.30).duty(0.25).lp(0.6).env(0.002, 0.06),
    Tone::new(Triangle, 520.0, 1280.0, 0.10, 0.12).env(0.002, 0.05),
];
/// Weiches „Plopp“ beim Landen.
static LAND: [Tone; 2] = [
    Tone::new(Sine, 140.0, 55.0, 0.09, 0.55).env(0.0, 0.05),
    Tone::new(Noise, 2600.0, 1400.0, 0.05, 0.10).lp(0.3).env(0.0, 0.04),
];
/// Kräftiges Klatschen nach tiefem Fall (der Pudding schwappt).
static LAND_HARD: [Tone; 3] = [
    Tone::new(Sine, 125.0, 38.0, 0.20, 0.80).env(0.0, 0.08),
    Tone::new(Noise, 3200.0, 900.0, 0.14, 0.25).lp(0.35).env(0.0, 0.06),
    Tone::new(Sine, 300.0, 180.0, 0.18, 0.20).vib(3.0, 14.0).at(0.04),
];
/// Kirsche: helles „Ding-Ding“.
static CHERRY: [Tone; 2] = [
    Tone::new(Pulse, B5, B5, 0.06, 0.22).duty(0.25).env(0.001, 0.03),
    Tone::new(Pulse, E6, E6, 0.16, 0.22).duty(0.25).env(0.001, 0.12).at(0.06),
];
/// Jede zehnte Kirsche: aufsteigendes Arpeggio mit Glitzern.
static CHERRY_BONUS: [Tone; 5] = [
    Tone::new(Pulse, C6, C6, 0.06, 0.22).duty(0.25),
    Tone::new(Pulse, E6, E6, 0.06, 0.22).duty(0.25).at(0.06),
    Tone::new(Pulse, G6, G6, 0.06, 0.22).duty(0.25).at(0.12),
    Tone::new(Pulse, C7, C7, 0.22, 0.22).duty(0.25).env(0.001, 0.15).at(0.18),
    Tone::new(Triangle, 4186.0, 4186.0, 0.25, 0.08).vib(0.5, 18.0).at(0.18),
];
/// Eiswürfel: glitzernd kühl, absteigend.
static ICE: [Tone; 4] = [
    Tone::new(Triangle, E7, E7, 0.08, 0.20).env(0.001, 0.05),
    Tone::new(Triangle, B6, B6, 0.08, 0.20).env(0.001, 0.05).at(0.06),
    Tone::new(Triangle, GS6, GS6, 0.25, 0.20).vib(0.3, 9.0).env(0.001, 0.2).at(0.12),
    Tone::new(Noise, 14000.0, 14000.0, 0.3, 0.05).env(0.05, 0.2),
];
/// Donut plattmachen: matschiges Platzen.
static STOMP: [Tone; 3] = [
    Tone::new(Noise, 4000.0, 800.0, 0.15, 0.40).lp(0.5).env(0.0, 0.06),
    Tone::new(Pulse, 520.0, 110.0, 0.18, 0.28).env(0.0, 0.06),
    Tone::new(Sine, 900.0, 1500.0, 0.08, 0.15).at(0.12),
];
/// Getroffen: absteigendes Jaulen.
static HIT: [Tone; 2] = [
    Tone::new(Pulse, 700.0, 160.0, 0.25, 0.28).vib(1.5, 20.0).env(0.0, 0.08),
    Tone::new(Noise, 3000.0, 2000.0, 0.08, 0.18).lp(0.5),
];
/// Marshmallow-Trampolin: wackeliges „Boiiing“.
static BOING: [Tone; 2] = [
    Tone::new(Sine, 220.0, 520.0, 0.08, 0.50).env(0.0, 0.02),
    Tone::new(Sine, 520.0, 300.0, 0.34, 0.45).vib(1.6, 12.0).env(0.0, 0.15).at(0.08),
];
/// Schokoladensee: Platschen und Blubbern.
static SPLASH: [Tone; 4] = [
    Tone::new(Noise, 6000.0, 1000.0, 0.45, 0.40).lp(0.4).env(0.0, 0.2),
    Tone::new(Sine, 600.0, 900.0, 0.05, 0.20).at(0.12),
    Tone::new(Sine, 500.0, 800.0, 0.05, 0.18).at(0.22),
    Tone::new(Sine, 700.0, 1050.0, 0.05, 0.15).at(0.33),
];
/// Geschmolzen: trauriges „Wah-wah-wahhh“.
static MELT: [Tone; 3] = [
    Tone::new(Saw, 392.0, 370.0, 0.25, 0.28).lp(0.3).env(0.01, 0.05),
    Tone::new(Saw, 370.0, 349.0, 0.25, 0.28).lp(0.3).env(0.01, 0.05).at(0.28),
    Tone::new(Saw, 349.0, 262.0, 0.65, 0.30).lp(0.3).vib(0.6, 6.0).env(0.01, 0.2).at(0.56),
];
/// Zuckerstange: Luftzug.
static WHOOSH: [Tone; 1] = [Tone::new(Noise, 9000.0, 1500.0, 0.30, 0.30).lp(0.6).env(0.03, 0.1)];
/// Checkpoint: fröhliches Arpeggio, gespeichert!
static CHECKPOINT: [Tone; 5] = [
    Tone::new(Pulse, G5, G5, 0.07, 0.22).duty(0.25),
    Tone::new(Pulse, C6, C6, 0.07, 0.22).duty(0.25).at(0.07),
    Tone::new(Pulse, E6, E6, 0.07, 0.22).duty(0.25).at(0.14),
    Tone::new(Pulse, G6, G6, 0.30, 0.22).duty(0.25).vib(0.2, 6.0).env(0.001, 0.2).at(0.21),
    Tone::new(Triangle, C6, C6, 0.40, 0.18).env(0.01, 0.2).at(0.21),
];
/// Ziel erreicht: kleine Fanfare.
static FINISH: [Tone; 8] = [
    Tone::new(Pulse, C5, C5, 0.10, 0.24).duty(0.25),
    Tone::new(Pulse, E5, E5, 0.10, 0.24).duty(0.25).at(0.11),
    Tone::new(Pulse, G5, G5, 0.10, 0.24).duty(0.25).at(0.22),
    Tone::new(Pulse, C6, C6, 0.55, 0.26).duty(0.25).vib(0.25, 5.5).env(0.002, 0.3).at(0.33),
    Tone::new(Triangle, E5, E5, 0.55, 0.20).env(0.002, 0.3).at(0.33),
    Tone::new(Triangle, G5, G5, 0.55, 0.16).env(0.002, 0.3).at(0.33),
    Tone::new(Pulse, G5, G5, 0.12, 0.22).duty(0.25).at(0.95),
    Tone::new(Pulse, C6, C6, 0.6, 0.26).duty(0.25).vib(0.3, 5.5).env(0.002, 0.35).at(1.08),
];
/// Kicheranfall: „hi-hi-hi-hi“.
static GIGGLE: [Tone; 4] = [
    Tone::new(Triangle, 950.0, 1150.0, 0.07, 0.24).vib(0.8, 25.0),
    Tone::new(Triangle, 1100.0, 1300.0, 0.07, 0.22).vib(0.8, 25.0).at(0.11),
    Tone::new(Triangle, 1000.0, 1200.0, 0.07, 0.20).vib(0.8, 25.0).at(0.22),
    Tone::new(Triangle, 1150.0, 1400.0, 0.09, 0.18).vib(0.8, 25.0).at(0.33),
];
static MENU_MOVE: [Tone; 1] = [Tone::new(Pulse, 1200.0, 1200.0, 0.025, 0.16).duty(0.25).env(0.0, 0.02)];
static MENU_SELECT: [Tone; 2] =
    [Tone::new(Pulse, A5, A5, 0.05, 0.20).duty(0.25), Tone::new(Pulse, 1320.0, 1320.0, 0.09, 0.20).duty(0.25).at(0.05)];
static MENU_BACK: [Tone; 2] =
    [Tone::new(Pulse, A5, A5, 0.05, 0.20).duty(0.25), Tone::new(Pulse, 587.0, 587.0, 0.09, 0.20).duty(0.25).at(0.05)];
static MUSIC_ON: [Tone; 3] = [
    Tone::new(Triangle, C5, C5, 0.08, 0.25),
    Tone::new(Triangle, E5, E5, 0.08, 0.25).at(0.08),
    Tone::new(Triangle, G5, G5, 0.2, 0.25).env(0.002, 0.15).at(0.16),
];
static MUSIC_OFF: [Tone; 3] = [
    Tone::new(Triangle, G5, G5, 0.08, 0.25),
    Tone::new(Triangle, E5, E5, 0.08, 0.25).at(0.08),
    Tone::new(Triangle, C5, C5, 0.2, 0.25).env(0.002, 0.15).at(0.16),
];
/// Neuer Rekord!
/// Würfelwelt: Block bricht – knirschendes Rauschen mit tiefem Knacken.
static ABBAUEN: [Tone; 3] = [
    Tone::new(Noise, 3800.0, 900.0, 0.16, 0.42).lp(0.45).env(0.0, 0.08),
    Tone::new(Pulse, 180.0, 70.0, 0.10, 0.30).duty(0.3).env(0.0, 0.05),
    Tone::new(Noise, 7000.0, 5000.0, 0.06, 0.12).env(0.0, 0.04).at(0.05),
];
/// Würfelwelt: Block gesetzt – dumpfes, kurzes Klopfen.
static SETZEN: [Tone; 2] = [
    Tone::new(Sine, 210.0, 120.0, 0.08, 0.55).env(0.0, 0.04),
    Tone::new(Noise, 2000.0, 1200.0, 0.04, 0.16).lp(0.3).env(0.0, 0.03),
];
/// Würfelwelt: leiser Schritt.
static SCHRITT: [Tone; 1] = [Tone::new(Noise, 1600.0, 700.0, 0.05, 0.14).lp(0.25).env(0.0, 0.035)];
static RECORD: [Tone; 6] = [
    Tone::new(Pulse, G5, G5, 0.06, 0.22).duty(0.25),
    Tone::new(Pulse, C6, C6, 0.06, 0.22).duty(0.25).at(0.06),
    Tone::new(Pulse, E6, E6, 0.06, 0.22).duty(0.25).at(0.12),
    Tone::new(Pulse, G6, G6, 0.06, 0.22).duty(0.25).at(0.18),
    Tone::new(Pulse, C7, C7, 0.4, 0.22).duty(0.25).vib(0.3, 7.0).env(0.001, 0.3).at(0.24),
    Tone::new(Triangle, 4186.0, 4186.0, 0.4, 0.08).vib(0.6, 20.0).at(0.24),
];

/// Die Töne eines Effekts.
pub fn tones(s: Sfx) -> &'static [Tone] {
    match s {
        Sfx::Jump => &JUMP,
        Sfx::Land => &LAND,
        Sfx::LandHard => &LAND_HARD,
        Sfx::Cherry => &CHERRY,
        Sfx::CherryBonus => &CHERRY_BONUS,
        Sfx::Ice => &ICE,
        Sfx::Stomp => &STOMP,
        Sfx::Hit => &HIT,
        Sfx::Boing => &BOING,
        Sfx::Splash => &SPLASH,
        Sfx::Melt => &MELT,
        Sfx::Whoosh => &WHOOSH,
        Sfx::Checkpoint => &CHECKPOINT,
        Sfx::Finish => &FINISH,
        Sfx::Giggle => &GIGGLE,
        Sfx::MenuMove => &MENU_MOVE,
        Sfx::MenuSelect => &MENU_SELECT,
        Sfx::MenuBack => &MENU_BACK,
        Sfx::MusicOn => &MUSIC_ON,
        Sfx::MusicOff => &MUSIC_OFF,
        Sfx::Record => &RECORD,
        Sfx::Abbauen => &ABBAUEN,
        Sfx::Setzen => &SETZEN,
        Sfx::Schritt => &SCHRITT,
    }
}
