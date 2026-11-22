//! Klang: kleiner Synthesizer, Soundeffekte und Musik – komplett berechnet,
//! ohne Sampledateien. Läuft auf dem ESP32 auf dem zweiten Kern und schreibt
//! 16-Bit-Stereo mit 22 050 Hz in den I²S-DMA-Puffer.
//!
//! * 9 Stimmen: 5 für die Musik (Melodie, Begleitung, Bass, 2× Schlagzeug),
//!   4 für Effekte. Bandbegrenzte Puls-/Sägezahnwellen (PolyBLEP – klingt
//!   nicht kratzig), Dreieck, Sinus, Rauschen; Hüllkurve (ADSR), Vibrato,
//!   Pulsbreitenmodulation, Tiefpass, Tonhöhen-Rutschen, Stereo-Panorama.
//! * Musik-Bus mit Stereo-Ping-Pong-Echo, sanfter Begrenzer am Ausgang.
//! * Befehle kommen über eine sperrfreie Warteschlange ([`CmdQueue`]) vom
//!   Spiel-Kern – kein Mutex, keine Allokation, sicher zwischen zwei Kernen.

pub mod music;
pub mod sfx;

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use music::{Instr, Song};
use spatz::anim::fast_sin;

pub const SAMPLE_RATE: u32 = 22_050;
const SR: f32 = SAMPLE_RATE as f32;
const INV_SR: f32 = 1.0 / SR;

/// Soundeffekte des Spiels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Sfx {
    Jump = 1,
    Land,
    LandHard,
    Cherry,
    CherryBonus,
    Ice,
    Stomp,
    Hit,
    Boing,
    Splash,
    Melt,
    Whoosh,
    Checkpoint,
    Finish,
    Giggle,
    MenuMove,
    MenuSelect,
    MenuBack,
    MusicOn,
    MusicOff,
    Record,
    /// Würfelwelt: Block abgebaut, gesetzt, Schritt.
    Abbauen,
    Setzen,
    Schritt,
}

impl Sfx {
    pub const ALL: [Sfx; 24] = [
        Sfx::Jump,
        Sfx::Land,
        Sfx::LandHard,
        Sfx::Cherry,
        Sfx::CherryBonus,
        Sfx::Ice,
        Sfx::Stomp,
        Sfx::Hit,
        Sfx::Boing,
        Sfx::Splash,
        Sfx::Melt,
        Sfx::Whoosh,
        Sfx::Checkpoint,
        Sfx::Finish,
        Sfx::Giggle,
        Sfx::MenuMove,
        Sfx::MenuSelect,
        Sfx::MenuBack,
        Sfx::MusicOn,
        Sfx::MusicOff,
        Sfx::Record,
        Sfx::Abbauen,
        Sfx::Setzen,
        Sfx::Schritt,
    ];

    fn from_u8(v: u8) -> Option<Sfx> {
        Self::ALL.iter().copied().find(|s| *s as u8 == v)
    }
}

/// Musikstücke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SongId {
    Menu = 1,
    Level = 2,
}

/// Befehle vom Spiel an den Synthesizer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCmd {
    Sfx(Sfx),
    /// Musikstück wählen (`None` = Stille).
    Song(Option<SongId>),
    /// Musik an/aus (sanft ein-/ausblenden).
    MusicEnabled(bool),
    /// Lautstärken 0..=10.
    MusicVolume(u8),
    SfxVolume(u8),
}

impl AudioCmd {
    fn encode(self) -> u32 {
        let (k, v) = match self {
            AudioCmd::Sfx(s) => (1, s as u8),
            AudioCmd::Song(s) => (2, s.map(|s| s as u8).unwrap_or(0)),
            AudioCmd::MusicEnabled(b) => (3, b as u8),
            AudioCmd::MusicVolume(v) => (4, v),
            AudioCmd::SfxVolume(v) => (5, v),
        };
        (k << 8) | v as u32
    }

    fn decode(x: u32) -> Option<AudioCmd> {
        let v = x as u8;
        Some(match x >> 8 {
            1 => AudioCmd::Sfx(Sfx::from_u8(v)?),
            2 => AudioCmd::Song(match v {
                1 => Some(SongId::Menu),
                2 => Some(SongId::Level),
                _ => None,
            }),
            3 => AudioCmd::MusicEnabled(v != 0),
            4 => AudioCmd::MusicVolume(v.min(10)),
            5 => AudioCmd::SfxVolume(v.min(10)),
            _ => return None,
        })
    }
}

/// Was der Ton-Kern abspielt: der Synthesizer (Konsole, Pudding-Panik) oder
/// ein anderer Mischer (z. B. Doom).
pub trait Klangquelle: Send {
    /// Befehle aus der Warteschlange übernehmen.
    fn poll(&mut self, q: &CmdQueue);
    /// `out` (Stereo, verschachtelt L/R, [`SAMPLE_RATE`]) füllen.
    fn render(&mut self, out: &mut [i16]);
}

impl Klangquelle for Synth {
    fn poll(&mut self, q: &CmdQueue) {
        Synth::poll(self, q);
    }

    fn render(&mut self, out: &mut [i16]) {
        Synth::render(self, out);
    }
}

/// Sperrfreie Warteschlange (ein Erzeuger, ein Verbraucher) für Befehle
/// zwischen den Kernen. Als `static` anlegbar.
pub struct CmdQueue {
    buf: [AtomicU32; 32],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl Default for CmdQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl CmdQueue {
    pub const fn new() -> Self {
        Self { buf: [const { AtomicU32::new(0) }; 32], head: AtomicUsize::new(0), tail: AtomicUsize::new(0) }
    }

    /// Einreihen (verwirft bei voller Schlange – lieber ein Effekt zu wenig
    /// als ein blockierter Spiel-Kern).
    pub fn push(&self, cmd: AudioCmd) -> bool {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Acquire);
        if h.wrapping_sub(t) >= self.buf.len() {
            return false;
        }
        self.buf[h % self.buf.len()].store(cmd.encode(), Ordering::Relaxed);
        self.head.store(h.wrapping_add(1), Ordering::Release);
        true
    }

    pub fn pop(&self) -> Option<AudioCmd> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t == h {
                return None;
            }
            let v = self.buf[t % self.buf.len()].load(Ordering::Relaxed);
            self.tail.store(t.wrapping_add(1), Ordering::Release);
            if let Some(c) = AudioCmd::decode(v) {
                return Some(c);
            }
        }
    }
}

/// Wellenform einer Stimme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wave {
    #[default]
    Pulse,
    Triangle,
    Saw,
    Sine,
    Noise,
}

/// Baustein eines Soundeffekts: ein Ton mit Startzeit, Tonhöhen-Rutsch,
/// Hüllkurve und Klangfarbe.
#[derive(Clone, Copy, Debug)]
pub struct Tone {
    pub at: f32,
    pub wave: Wave,
    pub f0: f32,
    pub f1: f32,
    pub dur: f32,
    pub vol: f32,
    pub duty: f32,
    pub vib: f32,
    pub vib_rate: f32,
    pub attack: f32,
    pub release: f32,
    pub lp: f32,
    pub pan: f32,
}

impl Tone {
    pub const fn new(wave: Wave, f0: f32, f1: f32, dur: f32, vol: f32) -> Self {
        Self {
            at: 0.0,
            wave,
            f0,
            f1,
            dur,
            vol,
            duty: 0.5,
            vib: 0.0,
            vib_rate: 0.0,
            attack: 0.002,
            release: 0.05,
            lp: 1.0,
            pan: 0.0,
        }
    }
    pub const fn at(mut self, t: f32) -> Self {
        self.at = t;
        self
    }
    pub const fn duty(mut self, d: f32) -> Self {
        self.duty = d;
        self
    }
    pub const fn vib(mut self, depth: f32, rate: f32) -> Self {
        self.vib = depth;
        self.vib_rate = rate;
        self
    }
    pub const fn lp(mut self, k: f32) -> Self {
        self.lp = k;
        self
    }
    pub const fn pan(mut self, p: f32) -> Self {
        self.pan = p;
        self
    }
    pub const fn env(mut self, attack: f32, release: f32) -> Self {
        self.attack = attack;
        self.release = release;
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Voice {
    on: bool,
    wave: Wave,
    phase: f32,
    freq: f32,
    slide: f32,
    duty: f32,
    pwm: f32,
    pwm_phase: f32,
    vib_depth: f32,
    vib_rate: f32,
    vib_phase: f32,
    vib_delay: f32,
    level: f32,
    attack_inc: f32,
    decay_mul: f32,
    sustain: f32,
    release_mul: f32,
    /// 0 = Anschlag, 1 = Abklingen/Halten, 2 = Ausklingen.
    stage: u8,
    /// Samples bis zum automatischen Loslassen (< 0: bis Note-Off).
    gate: i32,
    gain_l: f32,
    gain_r: f32,
    noise: u32,
    noise_val: f32,
    noise_acc: f32,
    lp: f32,
    lp_state: f32,
    /// Lautstärke für den Stimmenklau (je leiser, desto eher frei).
    age: u32,
}

/// Zeitkonstante in Sekunden → Faktor pro Sample (exponentieller Abfall auf 1/1000).
fn decay_factor(seconds: f32) -> f32 {
    if seconds <= 0.0 { 0.0 } else { spatz::math::exp(-6.9 / (seconds * SR)) }
}

/// Glättungskorrektur für Sprünge der Wellenform (PolyBLEP).
#[inline(always)]
fn blep(t: f32, dt: f32, inv_dt: f32) -> f32 {
    if t < dt {
        let x = t * inv_dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) * inv_dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

impl Voice {
    fn start(&mut self, wave: Wave, freq: f32, vol: f32, pan: f32) {
        self.on = true;
        self.wave = wave;
        self.freq = freq;
        self.slide = 0.0;
        self.level = 0.0;
        self.stage = 0;
        self.lp_state = 0.0;
        self.noise_acc = 0.0;
        if self.noise == 0 {
            self.noise = 0x1234_5678;
        }
        self.age = 0;
        let p = pan.clamp(-1.0, 1.0);
        self.gain_l = vol * (1.0 - p.max(0.0) * 0.7);
        self.gain_r = vol * (1.0 + p.min(0.0) * 0.7);
    }

    fn set_env(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack_inc = if attack <= 0.0 { 1.0 } else { 1.0 / (attack * SR) };
        self.decay_mul = decay_factor(decay);
        self.sustain = sustain;
        self.release_mul = decay_factor(release.max(0.005));
    }

    fn release(&mut self) {
        if self.on {
            self.stage = 2;
        }
    }

    #[inline]
    fn sample(&mut self) -> f32 {
        // Hüllkurve.
        match self.stage {
            0 => {
                self.level += self.attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = 1;
                }
            }
            1 => self.level = self.sustain + (self.level - self.sustain) * self.decay_mul,
            _ => {
                self.level *= self.release_mul;
                if self.level < 0.0005 {
                    self.on = false;
                    return 0.0;
                }
            }
        }
        if self.gate > 0 {
            self.gate -= 1;
            if self.gate == 0 {
                self.stage = 2;
            }
        }
        // Tonhöhe: Rutschen und (verzögertes) Vibrato.
        self.freq = (self.freq + self.slide).max(20.0);
        let mut f = self.freq;
        if self.vib_depth > 0.0 {
            if self.vib_delay > 0.0 {
                self.vib_delay -= 1.0;
            } else {
                self.vib_phase += self.vib_rate * INV_SR;
                if self.vib_phase >= 1.0 {
                    self.vib_phase -= 1.0;
                }
                f *= 1.0 + self.vib_depth * fast_sin(self.vib_phase * spatz::math::TAU);
            }
        }
        let dt = (f * INV_SR).min(0.45);
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let t = self.phase;
        let v = match self.wave {
            Wave::Pulse => {
                if self.pwm > 0.0 {
                    self.pwm_phase += self.pwm * INV_SR;
                    if self.pwm_phase >= 1.0 {
                        self.pwm_phase -= 1.0;
                    }
                }
                let d = if self.pwm > 0.0 {
                    self.duty + 0.18 * fast_sin(self.pwm_phase * spatz::math::TAU)
                } else {
                    self.duty
                }
                .clamp(0.05, 0.95);
                let inv = 1.0 / dt;
                let mut v = if t < d { 1.0 } else { -1.0 };
                v += blep(t, dt, inv);
                let t2 = if t >= d { t - d } else { t - d + 1.0 };
                v -= blep(t2, dt, inv);
                v
            }
            Wave::Saw => 2.0 * t - 1.0 - blep(t, dt, 1.0 / dt),
            Wave::Triangle => 4.0 * if t < 0.5 { t } else { 1.0 - t } - 1.0,
            Wave::Sine => fast_sin(t * spatz::math::TAU),
            Wave::Noise => {
                self.noise_acc += f * INV_SR;
                while self.noise_acc >= 1.0 {
                    self.noise_acc -= 1.0;
                    // 32-Bit-Xorshift: weißes Rauschen in [-1, 1).
                    let mut x = self.noise;
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    self.noise = x;
                    self.noise_val = (x >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0;
                }
                self.noise_val
            }
        };
        let v = if self.lp < 1.0 {
            self.lp_state += (v - self.lp_state) * self.lp;
            self.lp_state
        } else {
            v
        };
        v * self.level
    }
}

/// Ein noch nicht gestarteter Effekt-Ton (Startverzögerung in Samples).
#[derive(Clone, Copy, Debug)]
struct Pending {
    delay: u32,
    tone: Tone,
}

const MUSIC_VOICES: usize = 5;
const SFX_VOICES: usize = 4;
const ECHO_L: usize = 2867; // ~130 ms
const ECHO_R: usize = 3307; // ~150 ms (versetzt → Breite)
const PENDING: usize = 24;

/// Der Synthesizer. Komplett ohne Heap – kann als `static` im internen RAM
/// des Audio-Kerns liegen.
pub struct Synth {
    voices: [Voice; MUSIC_VOICES + SFX_VOICES],
    pending: [Option<Pending>; PENDING],
    note_freq: [f32; 128],
    // Musik
    song: Option<&'static Song>,
    order_pos: usize,
    row: usize,
    row_samples: f32,
    to_next_row: f32,
    music_on: bool,
    music_fade: f32,
    music_gain: f32,
    sfx_gain: f32,
    // Echo (Ping-Pong) auf dem Musik-Bus
    echo_l: [i16; ECHO_L],
    echo_r: [i16; ECHO_R],
    echo_pos_l: usize,
    echo_pos_r: usize,
    counter: u32,
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}

fn volume_curve(v: u8) -> f32 {
    let x = v.min(10) as f32 / 10.0;
    x * x
}

impl Synth {
    pub fn new() -> Self {
        let mut note_freq = [0.0f32; 128];
        for (n, f) in note_freq.iter_mut().enumerate() {
            // 440 Hz · 2^((n − 69) / 12)
            *f = 440.0 * spatz::math::exp((n as f32 - 69.0) * (core::f32::consts::LN_2 / 12.0));
        }
        Self {
            voices: [Voice::default(); MUSIC_VOICES + SFX_VOICES],
            pending: [None; PENDING],
            note_freq,
            song: None,
            order_pos: 0,
            row: 0,
            row_samples: 1.0,
            to_next_row: 0.0,
            music_on: true,
            music_fade: 0.0,
            music_gain: volume_curve(7),
            sfx_gain: volume_curve(8),
            echo_l: [0; ECHO_L],
            echo_r: [0; ECHO_R],
            echo_pos_l: 0,
            echo_pos_r: 0,
            counter: 0,
        }
    }

    /// Führt einen Befehl aus.
    pub fn command(&mut self, cmd: AudioCmd) {
        match cmd {
            AudioCmd::Sfx(s) => self.play_sfx(s),
            AudioCmd::Song(id) => self.set_song(id.map(music::song)),
            AudioCmd::MusicEnabled(on) => self.music_on = on,
            AudioCmd::MusicVolume(v) => self.music_gain = volume_curve(v),
            AudioCmd::SfxVolume(v) => self.sfx_gain = volume_curve(v),
        }
    }

    /// Holt alle wartenden Befehle aus der Warteschlange.
    pub fn poll(&mut self, q: &CmdQueue) {
        while let Some(c) = q.pop() {
            self.command(c);
        }
    }

    fn set_song(&mut self, song: Option<&'static Song>) {
        if let (Some(a), Some(b)) = (self.song, song)
            && core::ptr::eq(a, b)
        {
            return;
        }
        self.song = song;
        self.order_pos = 0;
        self.row = 0;
        self.to_next_row = 0.0;
        if let Some(s) = song {
            self.row_samples = SR * 60.0 / (s.bpm as f32 * 4.0);
        }
        for v in &mut self.voices[..MUSIC_VOICES] {
            v.release();
        }
    }

    pub fn play_sfx(&mut self, s: Sfx) {
        for tone in sfx::tones(s) {
            let delay = (tone.at * SR) as u32;
            if let Some(slot) = self.pending.iter_mut().find(|p| p.is_none()) {
                *slot = Some(Pending { delay, tone: *tone });
            }
        }
    }

    fn start_tone(&mut self, t: &Tone) {
        // Freie Effekt-Stimme, sonst die älteste übernehmen.
        let range = MUSIC_VOICES..MUSIC_VOICES + SFX_VOICES;
        let idx = range
            .clone()
            .find(|&i| !self.voices[i].on)
            .unwrap_or_else(|| range.max_by_key(|&i| self.voices[i].age).unwrap_or(MUSIC_VOICES));
        let v = &mut self.voices[idx];
        v.start(t.wave, t.f0, t.vol, t.pan);
        let n = (t.dur * SR).max(1.0);
        v.slide = (t.f1 - t.f0) / n;
        v.duty = t.duty;
        v.pwm = 0.0;
        v.vib_depth = t.vib * 0.0578;
        v.vib_rate = t.vib_rate;
        v.vib_delay = 0.0;
        v.vib_phase = 0.0;
        v.lp = t.lp;
        v.set_env(t.attack, 1.0, 1.0, t.release);
        v.gate = n as i32;
    }

    fn note_on(&mut self, ch: usize, note: u8, ins: &Instr) {
        let f = if ins.fixed > 0.0 { ins.fixed } else { self.note_freq[note.min(127) as usize] };
        let v = &mut self.voices[ch];
        let legato = ins.legato && v.on && v.stage != 2;
        if !legato {
            v.start(ins.wave, f, ins.vol, ins.pan);
            v.phase = 0.0;
            v.vib_phase = 0.0;
        }
        v.freq = f * ins.pitch_mul;
        v.slide = ins.slide;
        v.duty = ins.duty;
        v.pwm = ins.pwm;
        v.vib_depth = ins.vib * 0.0578;
        v.vib_rate = ins.vib_rate;
        v.vib_delay = ins.vib_delay * SR;
        v.lp = ins.lp;
        v.set_env(ins.attack, ins.decay, ins.sustain, ins.release);
        v.gate = if ins.gate > 0.0 { (ins.gate * SR) as i32 } else { -1 };
    }

    /// Nächste Musikzeile.
    fn tick_row(&mut self) {
        let Some(song) = self.song else { return };
        let pat = &song.patterns[song.order[self.order_pos] as usize];
        let row = self.row;
        let chans: [(&[u8; 32], &Instr); 3] =
            [(&pat.lead, &song.lead), (&pat.harm, &song.harm), (&pat.bass, &song.bass)];
        for (ch, (notes, ins)) in chans.into_iter().enumerate() {
            match notes[row] {
                music::HOLD => {}
                music::OFF => self.voices[ch].release(),
                n => self.note_on(ch, n, ins),
            }
        }
        match pat.drums[row] {
            b'K' => self.note_on(3, 0, &music::KICK),
            b'S' => self.note_on(3, 0, &music::SNARE),
            b'H' => self.note_on(4, 0, &music::HAT),
            b'O' => self.note_on(4, 0, &music::OPEN_HAT),
            b'C' => self.note_on(4, 0, &music::CLAP),
            _ => {}
        }
        self.row += 1;
        if self.row >= 32 {
            self.row = 0;
            self.order_pos = (self.order_pos + 1) % song.order.len();
        }
    }

    /// Füllt `out` (Stereo, verschachtelt L/R) mit Klang.
    pub fn render(&mut self, out: &mut [i16]) {
        for frame in out.as_chunks_mut::<2>().0 {
            self.counter = self.counter.wrapping_add(1);
            // Verzögerte Effekt-Töne alle 32 Samples prüfen (~1,5 ms).
            if self.counter.is_multiple_of(32) {
                for i in 0..PENDING {
                    if let Some(p) = &mut self.pending[i] {
                        if p.delay <= 32 {
                            let t = p.tone;
                            self.pending[i] = None;
                            self.start_tone(&t);
                        } else {
                            p.delay -= 32;
                        }
                    }
                }
                for v in &mut self.voices {
                    v.age = v.age.saturating_add(1);
                }
            }
            // Musik weiterschalten.
            let target = if self.music_on && self.song.is_some() { 1.0 } else { 0.0 };
            self.music_fade += (target - self.music_fade) * 0.0002;
            if target == 0.0 && self.music_fade < 0.001 && self.voices[..MUSIC_VOICES].iter().any(|v| v.on) {
                // Ausgeblendet: gehaltene Töne loslassen, damit nichts weiterrechnet.
                for v in &mut self.voices[..MUSIC_VOICES] {
                    v.release();
                }
            }
            if self.song.is_some() && self.music_fade > 0.001 {
                self.to_next_row -= 1.0;
                if self.to_next_row <= 0.0 {
                    self.to_next_row += self.row_samples;
                    self.tick_row();
                }
            }
            let (mut ml, mut mr, mut sl, mut sr) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (i, v) in self.voices.iter_mut().enumerate() {
                if !v.on {
                    continue;
                }
                let s = v.sample();
                if i < MUSIC_VOICES {
                    ml += s * v.gain_l;
                    mr += s * v.gain_r;
                } else {
                    sl += s * v.gain_l;
                    sr += s * v.gain_r;
                }
            }
            // Ping-Pong-Echo: links speist rechts und umgekehrt.
            let el = self.echo_l[self.echo_pos_l] as f32 * (1.0 / 32768.0);
            let er = self.echo_r[self.echo_pos_r] as f32 * (1.0 / 32768.0);
            let gain = self.music_gain * self.music_fade;
            let (ml, mr) = (ml * gain, mr * gain);
            self.echo_l[self.echo_pos_l] = to_i16(mr * 0.5 + er * 0.42);
            self.echo_r[self.echo_pos_r] = to_i16(ml * 0.5 + el * 0.42);
            self.echo_pos_l = (self.echo_pos_l + 1) % ECHO_L;
            self.echo_pos_r = (self.echo_pos_r + 1) % ECHO_R;
            let l = ml + el * 0.55 + sl * self.sfx_gain;
            let r = mr + er * 0.55 + sr * self.sfx_gain;
            frame[0] = to_i16(soft_clip(l * 0.9));
            frame[1] = to_i16(soft_clip(r * 0.9));
        }
    }

    /// Spielt gerade irgendetwas (für Tests/Energiesparen)?
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.on).count()
    }
}

/// Sanfter Begrenzer (rationale tanh-Näherung): laut, aber nie kratzig.
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[inline(always)]
fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn rms(s: &[i16]) -> f32 {
        let sum: f64 = s.iter().map(|&v| (v as f64) * (v as f64)).sum();
        (sum / s.len() as f64).sqrt() as f32 / 32768.0
    }

    #[test]
    fn queue_roundtrip() {
        let q = CmdQueue::new();
        assert!(q.push(AudioCmd::Sfx(Sfx::Boing)));
        assert!(q.push(AudioCmd::Song(Some(SongId::Level))));
        assert!(q.push(AudioCmd::MusicVolume(4)));
        assert_eq!(q.pop(), Some(AudioCmd::Sfx(Sfx::Boing)));
        assert_eq!(q.pop(), Some(AudioCmd::Song(Some(SongId::Level))));
        assert_eq!(q.pop(), Some(AudioCmd::MusicVolume(4)));
        assert_eq!(q.pop(), None);
        for _ in 0..40 {
            q.push(AudioCmd::Sfx(Sfx::Jump));
        }
        assert_eq!(core::iter::from_fn(|| q.pop()).count(), 32);
    }

    #[test]
    fn every_sfx_sounds_and_fades_out() {
        for s in Sfx::ALL {
            let mut syn = Synth::new();
            syn.play_sfx(s);
            let mut buf = vec![0i16; 2 * SAMPLE_RATE as usize * 3];
            syn.render(&mut buf);
            let head = rms(&buf[..2 * 4410]);
            assert!(head > 0.01, "{s:?} ist zu leise ({head})");
            assert!(buf.iter().all(|&v| v > -32767 && v < 32767), "{s:?} übersteuert");
            assert_eq!(syn.active_voices(), 0, "{s:?} klingt nicht aus");
        }
    }

    #[test]
    fn music_plays_loud_but_clean_and_can_be_muted() {
        for id in [SongId::Menu, SongId::Level] {
            let mut syn = Synth::new();
            syn.command(AudioCmd::Song(Some(id)));
            let mut buf = vec![0i16; 2 * SAMPLE_RATE as usize * 8];
            syn.render(&mut buf);
            let level = rms(&buf[2 * SAMPLE_RATE as usize..]);
            assert!(level > 0.04 && level < 0.5, "{id:?}: Pegel {level}");
            let peak = buf.iter().map(|v| v.unsigned_abs()).max().unwrap();
            assert!(peak < 32000, "{id:?}: Spitze {peak}");
            syn.command(AudioCmd::MusicEnabled(false));
            syn.render(&mut buf);
            assert!(rms(&buf[buf.len() / 2..]) < 0.002, "{id:?}: Musik aus muss still sein");
        }
    }
}
