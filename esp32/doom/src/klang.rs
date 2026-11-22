//! Ton für Doom: Die Soundeffekte (8-Bit-Samples aus der WAD, meist
//! 11 025 Hz) mischt [`DoomKlang`] auf dem Ton-Kern in den 22 050-Hz-Strom;
//! die Musik (MUS) spielt [`crate::musik`]. Doom (Spiel-Kern) meldet
//! Start/Stopp je Kanal über Atomics – keine Sperren zwischen den Kernen.

use crate::musik::MusikSpieler;
use core::ffi::{c_int, c_uint};
use pudding_core::audio::{AudioCmd, CmdQueue, Klangquelle, SAMPLE_RATE};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::*};

pub const KANAELE: usize = 16;
pub const SAMPLE_RATE_F: f32 = SAMPLE_RATE as f32;

struct Kanal {
    daten: AtomicUsize,
    laenge: AtomicU32,
    rate: AtomicU32,
    /// Lautstärke 0..=127.
    vol: AtomicU32,
    /// Zählt jeden Start/Stopp hoch: der Mischer übernimmt dann neu.
    folge: AtomicU32,
    laeuft: AtomicBool,
}

static KANAL: [Kanal; KANAELE] = [const {
    Kanal {
        daten: AtomicUsize::new(0),
        laenge: AtomicU32::new(0),
        rate: AtomicU32::new(0),
        vol: AtomicU32::new(0),
        folge: AtomicU32::new(0),
        laeuft: AtomicBool::new(false),
    }
}; KANAELE];

/// Gesamtlautstärke der Effekte in Prozent (Einstellung der Konsole).
pub static EFFEKTE_PROZENT: AtomicU32 = AtomicU32::new(100);

fn kanal(k: c_int) -> Option<&'static Kanal> {
    KANAL.get(usize::try_from(k).ok()?)
}

/// Effekt auf Kanal `k` starten (`daten` bleibt gültig: die WAD liegt
/// für immer im Speicher).
pub fn start(k: c_int, daten: &'static [u8], rate: u32, vol: u32) {
    let Some(c) = kanal(k) else { return };
    c.daten.store(daten.as_ptr() as usize, Relaxed);
    c.laenge.store(daten.len() as u32, Relaxed);
    c.rate.store(rate.max(1), Relaxed);
    c.vol.store(vol.min(127), Relaxed);
    c.laeuft.store(true, Relaxed);
    c.folge.fetch_add(1, Release);
}

pub fn stopp(k: c_int) {
    if let Some(c) = kanal(k) {
        c.laenge.store(0, Relaxed);
        c.laeuft.store(false, Relaxed);
        c.folge.fetch_add(1, Release);
    }
}

// --- von Doom aufgerufen ------------------------------------------------------

/// # Safety
/// `daten` zeigt auf `laenge` Bytes in der WAD (die nie freigegeben wird).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn DG_TonStart(k: c_int, daten: *const u8, laenge: c_uint, rate: c_int, vol: c_int, _sep: c_int) {
    if daten.is_null() {
        return;
    }
    // SAFETY: siehe oben.
    let d = unsafe { core::slice::from_raw_parts(daten, laenge as usize) };
    start(k, d, rate.max(1) as u32, vol.max(0) as u32);
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_TonStopp(k: c_int) {
    stopp(k);
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_TonLaeuft(k: c_int) -> c_int {
    kanal(k).is_some_and(|c| c.laeuft.load(Relaxed)) as c_int
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_TonParameter(k: c_int, vol: c_int, _sep: c_int) {
    if let Some(c) = kanal(k) {
        c.vol.store(vol.clamp(0, 127) as u32, Relaxed);
    }
}

// --- Mischer (Ton-Kern) -------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Stimme {
    folge: u32,
    daten: usize,
    laenge: u32,
    /// Position in Samples, 16.16 Festkomma.
    pos: u64,
    schritt: u64,
}

/// Mischt die Doom-Kanäle und die Musik; ersetzt auf dem Ton-Kern den
/// Synthesizer, solange Doom läuft.
pub struct DoomKlang {
    stimmen: [Stimme; KANAELE],
    musik: MusikSpieler,
    musik_puffer: Vec<i16>,
    /// Summe der Effekte je Block (nicht auf dem knappen Ton-Stapel).
    summe: Vec<i32>,
}

impl Default for DoomKlang {
    fn default() -> Self {
        Self::new()
    }
}

impl DoomKlang {
    pub fn new() -> Self {
        Self { stimmen: [Stimme::default(); KANAELE], musik: MusikSpieler::new(), musik_puffer: Vec::new(), summe: vec![0; 512] }
    }

    fn uebernehmen(&mut self) {
        for (s, c) in self.stimmen.iter_mut().zip(&KANAL) {
            let f = c.folge.load(Acquire);
            if f != s.folge {
                s.folge = f;
                s.daten = c.daten.load(Relaxed);
                s.laenge = c.laenge.load(Relaxed);
                s.pos = 0;
                s.schritt = ((c.rate.load(Relaxed) as u64) << 16) / SAMPLE_RATE as u64;
            }
        }
    }
}

impl Klangquelle for DoomKlang {
    fn poll(&mut self, q: &CmdQueue) {
        while let Some(c) = q.pop() {
            if let AudioCmd::SfxVolume(v) = c {
                EFFEKTE_PROZENT.store(v as u32 * 10, Relaxed);
            }
        }
    }

    fn render(&mut self, out: &mut [i16]) {
        self.uebernehmen();
        let n = out.len() / 2;
        let mut summe = core::mem::take(&mut self.summe);
        let mut start = 0;
        while start < n {
            let m = (n - start).min(summe.len());
            let s = &mut summe[..m];
            s.fill(0);
            self.effekte(s);
            self.musik_puffer.resize(m, 0);
            self.musik.render(&mut self.musik_puffer[..m]);
            for (i, (a, &b)) in s.iter().zip(&self.musik_puffer[..m]).enumerate() {
                let v = (*a + b as i32).clamp(-32767, 32767) as i16;
                out[2 * (start + i)] = v;
                out[2 * (start + i) + 1] = v;
            }
            start += m;
        }
        self.summe = summe;
    }
}

impl DoomKlang {
    /// Effekte linear interpoliert in `summe` addieren (Stellen ≈ 0,55 je
    /// Kanal bei voller Lautstärke, damit 8 gleichzeitige Effekte nicht
    /// hart übersteuern).
    fn effekte(&mut self, summe: &mut [i32]) {
        let gesamt = EFFEKTE_PROZENT.load(Relaxed) as i32;
        for (s, c) in self.stimmen.iter_mut().zip(&KANAL) {
            if s.laenge == 0 || s.daten == 0 {
                continue;
            }
            // SAFETY: Zeiger und Länge stammen aus `start` (WAD, für immer gültig).
            let d = unsafe { core::slice::from_raw_parts(s.daten as *const u8, s.laenge as usize) };
            let gain = c.vol.load(Relaxed) as i32 * gesamt * 140 / 100; // 0..=17780 (×1/32768)
            let ende = (s.laenge as u64) << 16;
            for o in summe.iter_mut() {
                if s.pos >= ende {
                    break;
                }
                let i = (s.pos >> 16) as usize;
                let f = (s.pos & 0xFFFF) as i32;
                let a = d[i] as i32 - 128;
                let b = *d.get(i + 1).unwrap_or(&128) as i32 - 128;
                let x = (a * (0x10000 - f) + b * f) >> 8; // ±32768
                *o += x * gain >> 15;
                s.pos += s.schritt;
            }
            if s.pos >= ende {
                s.laenge = 0;
                c.laeuft.store(false, Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TON: [u8; 100] = {
        let mut t = [128u8; 100];
        let mut i = 0;
        while i < 100 {
            t[i] = if i % 2 == 0 { 228 } else { 28 };
            i += 1;
        }
        t
    };

    /// Ein Kanal: spielt in doppelter Länge (11 025 → 22 050 Hz), endet
    /// dann und meldet das; Stopp beendet sofort. (Ein Test: die Kanäle
    /// sind global.)
    #[test]
    fn effekt_spielt_endet_und_stoppt() {
        let k = 13;
        let mut m = DoomKlang::new();
        start(k, &TON, 11_025, 127);
        assert_eq!(DG_TonLaeuft(k), 1);
        let mut out = vec![0i16; 2 * 150];
        m.render(&mut out);
        assert!(out.iter().any(|&v| v.abs() > 3000), "hörbar");
        let mut out = vec![0i16; 2 * 100];
        m.render(&mut out);
        assert_eq!(DG_TonLaeuft(k), 0, "nach 200 Samples vorbei");
        let mut out = vec![1i16; 2 * 64];
        m.render(&mut out);
        assert!(out.iter().all(|&v| v == 0), "danach still");

        start(k, &TON, 11_025, 127);
        stopp(k);
        assert_eq!(DG_TonLaeuft(k), 0);
        let mut out = vec![1i16; 2 * 64];
        m.render(&mut out);
        assert!(out.iter().all(|&v| v == 0), "gestoppt: still");
    }
}
