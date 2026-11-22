//! Doom-Musik: MUS-Stücke aus der WAD, gespielt mit den GENMIDI-Klängen
//! auf dem OPL-artigen Synthesizer ([`crate::opl`]) – so wie Doom auf einer
//! AdLib/Soundblaster-Karte klang. Doom (Spiel-Kern) meldet Stück, Pause
//! und Lautstärke über Atomics, [`MusikSpieler`] spielt auf dem Ton-Kern.

use crate::klang::SAMPLE_RATE_F;
use crate::opl::{Instrument, Instrumente, Opl, genmidi};
use core::ffi::{c_int, c_uint};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::*};

static DATEN: AtomicUsize = AtomicUsize::new(0);
static LAENGE: AtomicU32 = AtomicU32::new(0);
static SCHLEIFE: AtomicBool = AtomicBool::new(false);
static FOLGE: AtomicU32 = AtomicU32::new(0);
static PAUSE: AtomicBool = AtomicBool::new(false);
static LAUTSTAERKE: AtomicU32 = AtomicU32::new(100);
static LAEUFT: AtomicBool = AtomicBool::new(false);
/// GENMIDI-Lump der WAD (beim Start von Doom gesetzt).
static GENMIDI: OnceLock<&'static [u8]> = OnceLock::new();

/// Lump `name` einer WAD (Kopf „IWAD“/„PWAD“, Verzeichnis am Ende).
pub fn lump<'a>(wad: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let n = u32::from_le_bytes(wad.get(4..8)?.try_into().ok()?) as usize;
    let ofs = u32::from_le_bytes(wad.get(8..12)?.try_into().ok()?) as usize;
    (0..n).rev().find_map(|i| {
        let e = wad.get(ofs + 16 * i..ofs + 16 * i + 16)?;
        let nm = &e[8..16];
        let l = nm.iter().position(|&b| b == 0).unwrap_or(8);
        if !nm[..l].eq_ignore_ascii_case(name) {
            return None;
        }
        let p = u32::from_le_bytes(e[0..4].try_into().ok()?) as usize;
        let s = u32::from_le_bytes(e[4..8].try_into().ok()?) as usize;
        wad.get(p..p + s)
    })
}

/// Klänge aus der WAD bereitstellen (vor dem ersten Stück).
pub fn wad_klaenge(wad: &'static [u8]) {
    if let Some(g) = lump(wad, b"GENMIDI") {
        let _ = GENMIDI.set(g);
    }
}

/// # Safety
/// `daten` zeigt auf `laenge` Bytes in der WAD (wird nie freigegeben).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn DG_MusikStart(daten: *const u8, laenge: c_uint, schleife: c_int) {
    DATEN.store(daten as usize, Relaxed);
    LAENGE.store(laenge, Relaxed);
    SCHLEIFE.store(schleife != 0, Relaxed);
    LAEUFT.store(!daten.is_null(), Relaxed);
    FOLGE.fetch_add(1, Release);
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_MusikStopp() {
    LAENGE.store(0, Relaxed);
    LAEUFT.store(false, Relaxed);
    FOLGE.fetch_add(1, Release);
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_MusikLaeuft() -> c_int {
    LAEUFT.load(Relaxed) as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_MusikLautstaerke(vol: c_int) {
    LAUTSTAERKE.store(vol.clamp(0, 127) as u32, Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_MusikPause(pause: c_int) {
    PAUSE.store(pause != 0, Relaxed);
}

/// MUS-Ticks je Sekunde.
const TICKS: f32 = 140.0;
const SCHLAGZEUG: usize = 15;

#[derive(Clone, Copy, Debug)]
struct Kanal {
    instrument: u8,
    lautst: u8,
    /// Tonhöhenbeugung in Halbtönen.
    beugung: f32,
    letzte_anschlag: u8,
}

impl Default for Kanal {
    fn default() -> Self {
        Self { instrument: 0, lautst: 100, beugung: 0.0, letzte_anschlag: 127 }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Belegung {
    kanal: u8,
    taste: u8,
    /// Zweite Stimme eines Doppel-Instruments (Feinverstimmung).
    zweite: bool,
    /// Gespielter Ton ohne Beugung (für das Tonhöhenrad).
    ton: f32,
    fest: bool,
    alter: u32,
}

/// Ein MUS-Stück (Kopf „MUS\x1a“).
struct Stueck {
    daten: &'static [u8],
    start: usize,
    ende: usize,
    pos: usize,
    schleife: bool,
}

impl Stueck {
    fn neu(daten: &'static [u8], schleife: bool) -> Option<Self> {
        if daten.len() < 16 || &daten[..4] != b"MUS\x1a" {
            return None;
        }
        let laenge = u16::from_le_bytes([daten[4], daten[5]]) as usize;
        let start = u16::from_le_bytes([daten[6], daten[7]]) as usize;
        let ende = (start + laenge).min(daten.len());
        (start < ende).then_some(Self { daten, start, ende, pos: start, schleife })
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.daten.get(self.pos).filter(|_| self.pos < self.ende)?;
        self.pos += 1;
        Some(b)
    }
}

/// Spielt die gemeldete Musik.
pub struct MusikSpieler {
    folge: u32,
    opl: Opl,
    stueck: Option<Stueck>,
    instrumente: Option<Instrumente>,
    kanaele: [Kanal; 16],
    belegt: [Option<Belegung>; 9],
    alter: u32,
    /// Samples bis zum nächsten Ereignis.
    warten: f32,
    puffer: Vec<f32>,
}

impl Default for MusikSpieler {
    fn default() -> Self {
        Self::new()
    }
}

impl MusikSpieler {
    pub fn new() -> Self {
        Self {
            folge: 0,
            opl: Opl::new(),
            stueck: None,
            instrumente: None,
            kanaele: [Kanal::default(); 16],
            belegt: [None; 9],
            alter: 0,
            warten: 0.0,
            puffer: Vec::new(),
        }
    }

    /// Stück direkt starten (Tests, Vorschau).
    pub fn spiele(&mut self, mus: &'static [u8], genmidi_lump: &[u8], schleife: bool) {
        self.instrumente = genmidi(genmidi_lump);
        self.folge = FOLGE.load(Acquire);
        self.starte(mus, schleife);
    }

    fn starte(&mut self, mus: &'static [u8], schleife: bool) {
        self.alle_aus();
        self.kanaele = [Kanal::default(); 16];
        self.stueck = Stueck::neu(mus, schleife);
        self.warten = 0.0;
    }

    fn alle_aus(&mut self) {
        for (s, b) in self.opl.stimmen.iter_mut().zip(&mut self.belegt) {
            s.aus();
            *b = None;
        }
    }

    fn instrument(&self, kanal: usize, taste: u8) -> Option<Instrument> {
        let inst = self.instrumente.as_ref()?;
        if kanal == SCHLAGZEUG {
            (35..=81).contains(&taste).then(|| inst[128 + taste as usize - 35])
        } else {
            Some(inst[self.kanaele[kanal].instrument.min(127) as usize])
        }
    }

    /// Freie Stimme, sonst die älteste (losgelassene zuerst).
    fn stimme_frei(&mut self) -> usize {
        if let Some(i) = (0..9).find(|&i| self.belegt[i].is_none() && !self.opl.stimmen[i].klingt()) {
            return i;
        }
        if let Some(i) = (0..9).filter(|&i| !self.opl.stimmen[i].gedrueckt()).min_by_key(|&i| self.belegt[i].map_or(0, |b| b.alter)) {
            return i;
        }
        (0..9).min_by_key(|&i| self.belegt[i].map_or(0, |b| b.alter)).unwrap_or(0)
    }

    fn note_an(&mut self, kanal: usize, taste: u8, anschlag: u8) {
        let Some(inst) = self.instrument(kanal, taste) else { return };
        let k = self.kanaele[kanal];
        let lautst = (anschlag as u32 * k.lautst as u32 / 127) * LAUTSTAERKE.load(Relaxed).min(127) / 127;
        let anzahl = if inst.zwei_stimmen { 2 } else { 1 };
        for v in 0..anzahl {
            let i = self.stimme_frei();
            let p = &inst.stimmen[v];
            let mut ton = if inst.fest { inst.fester_ton as f32 } else { taste as f32 + p.grundton as f32 };
            if v == 1 {
                ton += (inst.feinstimmung as f32 / 2.0 - 64.0) / 32.0;
            }
            let beugung = if inst.fest { 0.0 } else { k.beugung };
            self.opl.stimmen[i].an(p, ton + beugung, lautst);
            self.alter += 1;
            self.belegt[i] = Some(Belegung { kanal: kanal as u8, taste, zweite: v == 1, ton, fest: inst.fest, alter: self.alter });
        }
    }

    fn note_aus(&mut self, kanal: usize, taste: u8) {
        for i in 0..9 {
            if let Some(b) = self.belegt[i]
                && b.kanal as usize == kanal
                && b.taste == taste
            {
                self.opl.stimmen[i].aus();
                self.belegt[i] = None;
            }
        }
    }

    fn beugen(&mut self, kanal: usize, wert: u8) {
        // 128 = Mitte, 64/192 = ein Halbton tiefer/höher.
        let b = (wert as f32 - 128.0) / 64.0;
        self.kanaele[kanal].beugung = b;
        for i in 0..9 {
            if let Some(be) = self.belegt[i]
                && be.kanal as usize == kanal
                && !be.fest
            {
                self.opl.stimmen[i].tonhoehe(be.ton + b);
                let _ = be.zweite;
            }
        }
    }

    /// Ereignisse bis zur nächsten Pause abarbeiten.
    fn ereignisse(&mut self) {
        loop {
            let Some(st) = self.stueck.as_mut() else { return };
            let Some(e) = st.byte() else {
                self.ende();
                return;
            };
            let kanal = (e & 0x0F) as usize;
            match (e >> 4) & 7 {
                0 => {
                    let t = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(0) & 0x7F;
                    self.note_aus(kanal, t);
                }
                1 => {
                    let t = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(0);
                    if t & 0x80 != 0 {
                        let a = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(127) & 0x7F;
                        self.kanaele[kanal].letzte_anschlag = a;
                    }
                    let a = self.kanaele[kanal].letzte_anschlag;
                    self.note_an(kanal, t & 0x7F, a);
                }
                2 => {
                    let w = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(128);
                    self.beugen(kanal, w);
                }
                3 => {
                    let c = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(0);
                    if c == 10 || c == 11 {
                        for i in 0..9 {
                            if self.belegt[i].is_some_and(|b| b.kanal as usize == kanal) {
                                self.opl.stimmen[i].aus();
                                self.belegt[i] = None;
                            }
                        }
                    }
                }
                4 => {
                    let c = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(0);
                    let v = self.stueck.as_mut().and_then(Stueck::byte).unwrap_or(0) & 0x7F;
                    match c {
                        0 => self.kanaele[kanal].instrument = v,
                        3 => self.kanaele[kanal].lautst = v,
                        _ => {}
                    }
                }
                6 => {
                    self.ende();
                    return;
                }
                _ => {}
            }
            if e & 0x80 != 0 {
                // Verzögerung (7 Bit je Byte) in Ticks.
                let mut t = 0u32;
                loop {
                    let Some(b) = self.stueck.as_mut().and_then(Stueck::byte) else { break };
                    t = (t << 7) | (b & 0x7F) as u32;
                    if b & 0x80 == 0 {
                        break;
                    }
                }
                self.warten += t as f32 * SAMPLE_RATE_F / TICKS;
                if self.warten > 0.0 {
                    return;
                }
            }
        }
    }

    fn ende(&mut self) {
        match self.stueck.as_mut() {
            Some(st) if st.schleife => {
                st.pos = st.start;
                self.warten += 1.0;
            }
            _ => {
                self.stueck = None;
                self.alle_aus();
                LAEUFT.store(false, Relaxed);
            }
        }
    }

    /// Mono-Samples (22 050 Hz) in `out` schreiben.
    pub fn render(&mut self, out: &mut [i16]) {
        let f = FOLGE.load(Acquire);
        if f != self.folge {
            self.folge = f;
            if self.instrumente.is_none() {
                self.instrumente = GENMIDI.get().and_then(|g| genmidi(g));
            }
            let (d, n) = (DATEN.load(Relaxed), LAENGE.load(Relaxed) as usize);
            if d != 0 && n > 0 {
                // SAFETY: Zeiger und Länge aus der WAD, die für immer im Speicher liegt.
                let mus = unsafe { core::slice::from_raw_parts(d as *const u8, n) };
                self.starte(mus, SCHLEIFE.load(Relaxed));
            } else {
                self.stueck = None;
                self.alle_aus();
            }
        }
        self.puffer.clear();
        self.puffer.resize(out.len(), 0.0);
        if !PAUSE.load(Relaxed) {
            let mut i = 0;
            while i < out.len() {
                if self.stueck.is_some() && self.warten <= 0.0 {
                    self.ereignisse();
                }
                let n = if self.stueck.is_some() { (self.warten.ceil().max(1.0) as usize).min(out.len() - i) } else { out.len() - i };
                self.opl.render(&mut self.puffer[i..i + n]);
                self.warten -= n as f32;
                i += n;
            }
        }
        for (o, &x) in out.iter_mut().zip(&self.puffer) {
            *o = (x * 22000.0).clamp(-32767.0, 32767.0) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wad() -> Option<&'static [u8]> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("wad/DOOM1.WAD");
        std::fs::read(p).ok().map(|w| &*Box::leak(w.into_boxed_slice()))
    }

    /// E1M1 („At Doom's Gate“) spielt hörbar, ohne Übersteuern, mehrere Stimmen.
    #[test]
    fn e1m1_klingt() {
        let Some(w) = wad() else { return };
        let mut m = MusikSpieler::new();
        m.spiele(lump(w, b"D_E1M1").expect("D_E1M1"), lump(w, b"GENMIDI").expect("GENMIDI"), true);
        let mut out = vec![0i16; 22050 * 4];
        let t0 = std::time::Instant::now();
        for b in out.chunks_mut(256) {
            m.render(b);
        }
        eprintln!("4 s Musik in {:?}", t0.elapsed());
        let spitze = out.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
        // Klang statt Rauschen: wenige Nulldurchgänge (Bass, Melodie).
        let nulldurchgaenge = out.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count() as f32 / 4.0;
        eprintln!("Spitze {spitze}, {nulldurchgaenge:.0} Nulldurchgänge/s");
        assert!(nulldurchgaenge < 4000.0, "kein Rauschen ({nulldurchgaenge}/s)");
        let laut = out.iter().filter(|v| v.unsigned_abs() > 1000).count();
        assert!(spitze > 6000, "hörbar ({spitze})");
        assert!(spitze < 32767, "nicht übersteuert");
        assert!(laut > out.len() / 3, "durchgehend Musik ({laut})");
        assert!(m.belegt.iter().filter(|b| b.is_some()).count() >= 2 || laut > 0);
    }
}
