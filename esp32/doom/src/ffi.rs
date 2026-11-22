//! Schnittstelle zu doomgeneric (C): Aufrufe nach C und die `DG_*`-Funktionen,
//! die Doom von der Plattform erwartet. Doom läuft nur auf dem Spiel-Kern;
//! der Ton-Kern liest allein die Atomics in [`crate::klang`].

use core::ffi::{c_char, c_int, c_uchar, c_uint};
use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

unsafe extern "C" {
    pub fn doomgeneric_Create(argc: c_int, argv: *mut *mut c_char);
    pub fn doomgeneric_Tick();
    /// Doom-Zeit in Tics (35 je Sekunde).
    pub fn I_GetTime() -> c_int;
    pub fn M_SaveDefaults();
    // dg_konsole.c
    pub fn DG_Maus(dx: c_int, dy: c_int);
    pub fn DG_Modus() -> c_int;
    pub fn DG_MenueTipp(x: c_int, y: c_int) -> c_int;
    pub fn DG_SpeicherNameLeer() -> c_int;
    pub fn DG_Level() -> c_int;
    pub fn DG_Einstellen();
    /// Entwicklungs-Build DOOM_SPEICHERTEST (dg_konsole.c).
    pub fn DG_Speichertest(tic: c_int) -> c_int;
    /// 320×200 Farbindizes (CMAP256), nach jedem `DG_DrawFrame` aktuell.
    pub static mut DG_ScreenBuffer: *mut u8;
    /// Palette (je Eintrag b | g << 8 | r << 16).
    pub static colors: [u32; 256];
}

pub const BREITE: usize = 320;
pub const HOEHE: usize = 200;

/// Was die `DG_*`-Funktionen mit der Rust-Seite teilen.
pub(crate) struct Gemeinsam {
    pub wad: Option<&'static [u8]>,
    pub tasten: VecDeque<(bool, u8)>,
    /// Gezeichnete Doom-Bilder seit dem Start.
    pub bilder: u32,
    pub beendet: bool,
}

pub(crate) static GEMEINSAM: Mutex<Gemeinsam> =
    Mutex::new(Gemeinsam { wad: None, tasten: VecDeque::new(), bilder: 0, beendet: false });

pub(crate) fn gemeinsam() -> std::sync::MutexGuard<'static, Gemeinsam> {
    GEMEINSAM.lock().unwrap_or_else(|e| e.into_inner())
}

static KONFIG: OnceLock<CString> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
/// Simulation/Tests: Zeit läuft nur mit `update` (und `DG_SleepMs`) weiter.
pub(crate) static VIRTUELL: AtomicBool = AtomicBool::new(false);
pub(crate) static ZEIT_MS: AtomicU32 = AtomicU32::new(0);
/// Wird beim Beenden aus dem Doom-Menü aufgerufen (Firmware: Neustart).
static BEENDEN: OnceLock<fn()> = OnceLock::new();

pub(crate) fn einrichten(wad: &'static [u8], ordner: &str) {
    gemeinsam().wad = Some(wad);
    crate::musik::wad_klaenge(wad);
    let _ = KONFIG.set(CString::new(ordner).expect("Ordner ohne Nullbyte"));
    START.get_or_init(Instant::now);
}

/// Haken für „Spiel beenden“ im Doom-Menü (sonst kehrt nur `update` zur
/// Konsole zurück).
pub fn beim_beenden(f: fn()) {
    let _ = BEENDEN.set(f);
}

fn ist_doom1(pfad: &[u8]) -> bool {
    let name = pfad.rsplit(|&b| b == b'/' || b == b'\\').next().unwrap_or(pfad);
    name.eq_ignore_ascii_case(b"doom1.wad")
}

// --- von Doom aufgerufen ------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn DG_Init() {}

#[unsafe(no_mangle)]
pub extern "C" fn DG_DrawFrame() {
    gemeinsam().bilder += 1;
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SleepMs(ms: u32) {
    if VIRTUELL.load(Relaxed) {
        ZEIT_MS.fetch_add(ms.max(1), Relaxed);
    } else {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    if VIRTUELL.load(Relaxed) {
        ZEIT_MS.load(Relaxed)
    } else {
        START.get_or_init(Instant::now).elapsed().as_millis() as u32
    }
}

/// Nächstes Tastenereignis (1 = vorhanden).
///
/// # Safety
/// Doom übergibt gültige Zeiger.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn DG_GetKey(pressed: *mut c_int, key: *mut c_uchar) -> c_int {
    match gemeinsam().tasten.pop_front() {
        Some((unten, k)) => {
            // SAFETY: siehe oben.
            unsafe {
                *pressed = unten as c_int;
                *key = k;
            }
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SetWindowTitle(_title: *const c_char) {}

/// WAD im Speicher für `pfad` (nur „doom1.wad“).
///
/// # Safety
/// `pfad` ist eine C-Zeichenkette, die Ausgabezeiger sind gültig.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn DG_WadDaten(pfad: *const c_char, daten: *mut *const u8, laenge: *mut c_uint) -> c_int {
    if pfad.is_null() {
        return 0;
    }
    // SAFETY: siehe oben.
    let p = unsafe { core::ffi::CStr::from_ptr(pfad) }.to_bytes();
    match gemeinsam().wad {
        Some(w) if ist_doom1(p) => {
            // SAFETY: siehe oben.
            unsafe {
                *daten = w.as_ptr();
                *laenge = w.len() as c_uint;
            }
            1
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_KonfigOrdner() -> *const c_char {
    KONFIG.get().map(|c| c.as_ptr()).unwrap_or(c"./".as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_Beenden() {
    gemeinsam().beendet = true;
    if let Some(f) = BEENDEN.get() {
        f();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_doom1_wad_kommt_aus_dem_speicher() {
        assert!(ist_doom1(b"doom1.wad"));
        assert!(ist_doom1(b"./DOOM1.WAD"));
        assert!(ist_doom1(b"/doom/doom1.wad"));
        assert!(!ist_doom1(b"doom2.wad"));
        assert!(!ist_doom1(b"default.cfg"));
    }
}
