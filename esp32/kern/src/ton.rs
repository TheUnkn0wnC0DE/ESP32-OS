//! Tonausgabe der Programme (A5): ein PCM-Strom (Stereo, [`RATE`] Hz,
//! 16 Bit) über einen sperrfreien Ringpuffer (ein Erzeuger, ein
//! Verbraucher) zur Plattform. Am Board mischt der Audio-Faden der Firmware
//! ihn zum Synthesizer des Desktops (I²S → ES8311), am PC liest ihn der
//! Test.
//!
//! Immer nur ein Programm spielt: [`belegen`] liefert den Strom dem
//! aufrufenden Prozess, solange kein anderer, noch laufender ihn hält.

use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

/// Abtastrate des Stroms (wie der Synthesizer der Firmware).
pub const RATE: u32 = 22_050;
/// Platz im Ring: 16 384 Stereo-Frames ≈ 0,74 s.
pub const FRAMES: usize = 1 << 14;

/// Frames als `L | R << 16` (im Heap des Systems, am Board im PSRAM).
static PUFFER: OnceLock<Box<[AtomicU32]>> = OnceLock::new();
/// Geschriebene bzw. gelesene Frames (laufen über, Abstand ≤ FRAMES).
static SCHREIB: AtomicU32 = AtomicU32::new(0);
static LESE: AtomicU32 = AtomicU32::new(0);
/// Gespielte Frames insgesamt (Nachweis im Log).
static GESPIELT: AtomicU32 = AtomicU32::new(0);
/// PID des Programms, das den Strom hält (0 = frei).
static BESITZER: AtomicU32 = AtomicU32::new(0);

fn puffer() -> &'static [AtomicU32] {
    PUFFER.get_or_init(|| crate::kern_abschnitt(|| (0..FRAMES).map(|_| AtomicU32::new(0)).collect()))
}

/// Strom für das laufende Programm belegen. Liefert false, wenn ein
/// anderes, noch laufendes Programm ihn hält.
pub fn belegen() -> bool {
    let Some(ich) = crate::eigene_pid() else { return false };
    let alt = BESITZER.load(Ordering::Acquire);
    if alt == ich {
        return true;
    }
    let laeuft = alt != 0 && crate::liste().iter().any(|p| p.pid == alt && !p.endet);
    if laeuft {
        return false;
    }
    if BESITZER.compare_exchange(alt, ich, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return false;
    }
    leeren();
    true
}

/// Strom freigeben (nur der Besitzer); Rest im Ring verworfen.
pub fn freigeben() {
    if let Some(ich) = crate::eigene_pid()
        && BESITZER.compare_exchange(ich, 0, Ordering::AcqRel, Ordering::Acquire).is_ok()
    {
        leeren();
    }
}

/// Noch nicht gespielte Frames verwerfen (Pause, Titelwechsel).
pub fn leeren() {
    LESE.store(SCHREIB.load(Ordering::Acquire), Ordering::Release);
}

/// Noch nicht gespielte Frames aus dem Ring zurücknehmen (Pause ohne
/// Sprung in der Datei); nur der Besitzer. Hängt sie an `aus` an.
pub fn zuruecknehmen(aus: &mut Vec<u32>) {
    if crate::eigene_pid().is_none_or(|p| BESITZER.load(Ordering::Acquire) != p) {
        return;
    }
    let Some(p) = PUFFER.get() else { return };
    loop {
        let l = LESE.load(Ordering::Acquire);
        let s = SCHREIB.load(Ordering::Acquire);
        let start = aus.len();
        for i in 0..s.wrapping_sub(l) {
            aus.push(p[(l.wrapping_add(i) as usize) % FRAMES].load(Ordering::Relaxed));
        }
        // der Verbraucher kann inzwischen weitergelesen haben: dann noch einmal
        if LESE.compare_exchange(l, s, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            return;
        }
        aus.truncate(start);
    }
}

/// Belegter Platz im Ring (Frames, warten auf die Ausgabe).
pub fn belegt() -> usize {
    (SCHREIB.load(Ordering::Acquire).wrapping_sub(LESE.load(Ordering::Acquire)) as usize).min(FRAMES)
}

/// Freier Platz im Ring (Frames).
pub fn frei() -> usize {
    let belegt = SCHREIB.load(Ordering::Acquire).wrapping_sub(LESE.load(Ordering::Acquire)) as usize;
    FRAMES.saturating_sub(belegt)
}

/// Frames (`L | R << 16`) anhängen, so viele Platz haben; nur der Besitzer.
/// Liefert die Zahl der übernommenen Frames.
pub fn schreiben(frames: &[u32]) -> usize {
    if crate::eigene_pid().is_none_or(|p| BESITZER.load(Ordering::Acquire) != p) {
        return 0;
    }
    let p = puffer();
    let n = frames.len().min(frei());
    let s = SCHREIB.load(Ordering::Acquire);
    for (i, &f) in frames[..n].iter().enumerate() {
        p[(s.wrapping_add(i as u32) as usize) % FRAMES].store(f, Ordering::Relaxed);
    }
    SCHREIB.store(s.wrapping_add(n as u32), Ordering::Release);
    n
}

/// Verbraucher (Audio-Faden der Plattform): bis zu `out.len() / 2` Frames
/// zu `out` (Stereo, verschachtelt) dazumischen. Liefert die Zahl der
/// gemischten Frames; fehlende bleiben Stille.
pub fn dazumischen(out: &mut [i16]) -> usize {
    let Some(p) = PUFFER.get() else { return 0 };
    let l = LESE.load(Ordering::Acquire);
    let vorrat = SCHREIB.load(Ordering::Acquire).wrapping_sub(l) as usize;
    let n = (out.len() / 2).min(vorrat);
    for i in 0..n {
        let f = p[(l.wrapping_add(i as u32) as usize) % FRAMES].load(Ordering::Relaxed);
        out[2 * i] = out[2 * i].saturating_add(f as u16 as i16);
        out[2 * i + 1] = out[2 * i + 1].saturating_add((f >> 16) as u16 as i16);
    }
    // nur vorrücken, wenn niemand inzwischen geleert hat
    let _ = LESE.compare_exchange(l, l.wrapping_add(n as u32), Ordering::AcqRel, Ordering::Acquire);
    GESPIELT.fetch_add(n as u32, Ordering::Relaxed);
    n
}

/// Insgesamt gespielte Frames.
pub fn gespielt() -> u32 {
    GESPIELT.load(Ordering::Relaxed)
}

/// Frame aus linkem und rechtem Wert.
pub fn frame(l: i16, r: i16) -> u32 {
    u32::from(l as u16) | u32::from(r as u16) << 16
}
