//! Ton für Quake: Der WinQuake-Mischer (Spiel-Kern) schreibt voraus in einen
//! Ringpuffer (Stereo, 16 Bit, 22 050 Hz); [`QuakeKlang`] spielt ihn auf
//! dem Ton-Kern ab und meldet die Leseposition zurück (wie ein DMA-Zähler).

use core::ffi::c_int;
use pudding_core::audio::{CmdQueue, Klangquelle};
use std::sync::atomic::{AtomicU32, Ordering::*};

/// Ringgröße in Stereo-Frames (~0,19 s; Quake mischt 0,1 s voraus).
pub const RING: usize = 4096;

struct Ring(core::cell::UnsafeCell<[i16; RING * 2]>);
// SAFETY: Quake schreibt nur vor der Leseposition, der Ton-Kern liest und
// löscht nur dahinter (wie bei einer DMA-Soundkarte).
unsafe impl Sync for Ring {}

static PUFFER: Ring = Ring(core::cell::UnsafeCell::new([0; RING * 2]));
/// Abgespielte Frames seit dem Start.
static POS: AtomicU32 = AtomicU32::new(0);

/// # Safety
/// `frames` ist ein gültiger Zeiger.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_TonPuffer(frames: *mut c_int) -> *mut i16 {
    // SAFETY: siehe oben.
    unsafe { *frames = RING as c_int };
    PUFFER.0.get().cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn QG_TonPosition() -> c_int {
    (POS.load(Acquire) as usize % RING) as c_int
}

/// Spielt den Ringpuffer ab; ersetzt auf dem Ton-Kern den Synthesizer,
/// solange Quake läuft.
#[derive(Default)]
pub struct QuakeKlang;

impl Klangquelle for QuakeKlang {
    fn poll(&mut self, q: &CmdQueue) {
        while q.pop().is_some() {}
    }

    fn render(&mut self, out: &mut [i16]) {
        let start = POS.load(Relaxed);
        let p = PUFFER.0.get().cast::<i16>();
        for (i, f) in out.chunks_exact_mut(2).enumerate() {
            let k = (start as usize + i) % RING * 2;
            // SAFETY: Index im Ring; Quake schreibt an diese Stelle erst
            // wieder, wenn die Leseposition vorbei ist.
            unsafe {
                f[0] = core::ptr::read_volatile(p.add(k));
                f[1] = core::ptr::read_volatile(p.add(k + 1));
                // Gelesenes löschen: Kommt Quake nicht nach, bleibt es still.
                core::ptr::write_volatile(p.add(k), 0);
                core::ptr::write_volatile(p.add(k + 1), 0);
            }
        }
        POS.store(start.wrapping_add((out.len() / 2) as u32), Release);
    }
}
