//! Schnittstelle zu quakegeneric (C): Aufrufe nach C und die `QG_*`-
//! Funktionen, die Quake von der Plattform erwartet. Quake läuft nur auf
//! dem Spiel-Kern; der Ton-Kern liest allein den Ringpuffer ([`crate::klang`]).

use core::ffi::{CStr, c_char, c_double, c_int, c_uchar, c_void};
use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

unsafe extern "C" {
    pub fn QG_Create(argc: c_int, argv: *mut *mut c_char);
    pub fn QG_Tick(dauer: c_double);
    // qg_konsole.c
    pub fn QG_Befehl(text: *const c_char);
    pub fn QG_Modus() -> c_int;
    fn QG_HunkFrei() -> c_int;
    fn QG_Karte() -> *const c_char;
    // cvar.c
    fn Cvar_VariableValue(name: *const c_char) -> f32;
}

// qg_esp.c (nur Board)
#[cfg(target_os = "espidf")]
unsafe extern "C" {
    fn QG_StapelMerken();
    fn QG_StapelFrei() -> c_int;
}

pub const BREITE: usize = 320;
pub const HOEHE: usize = 240;

/// Woher die Spieldaten (id1/pak0.pak) kommen.
#[derive(Clone, Copy)]
pub enum Pak {
    /// Ganz im Speicher (PC).
    Speicher(&'static [u8]),
    /// Stückweise lesen (Board: Flash-Partition).
    Lesen { groesse: usize, lesen: fn(usize, &mut [u8]) -> bool },
}

static PAK: OnceLock<Pak> = OnceLock::new();
/// Mit `fopen` lesbarer Pfad der PAK (Demos öffnen sie als Datei).
static PAK_DATEI: OnceLock<CString> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
pub(crate) static VIRTUELL: AtomicBool = AtomicBool::new(false);
/// Virtuelle Zeit in Mikrosekunden (nur Simulation/Tests, < 71 min).
pub(crate) static ZEIT_US: AtomicU32 = AtomicU32::new(0);
static BEENDEN: OnceLock<fn()> = OnceLock::new();
static FEHLER: OnceLock<fn(&str)> = OnceLock::new();
static FEHLERMELDUNG: OnceLock<String> = OnceLock::new();
static BERICHT: OnceLock<fn(&str)> = OnceLock::new();

pub(crate) struct Gemeinsam {
    pub tasten: VecDeque<(bool, i32)>,
    pub maus: (i32, i32),
    pub achsen: [f32; 6],
    /// Bild (320×240 Farbindizes, gehört Quake) und Palette.
    pub bild: usize,
    pub pal: [u16; 256],
    pub bilder: u32,
    pub beendet: bool,
}

pub(crate) static GEMEINSAM: Mutex<Gemeinsam> =
    Mutex::new(Gemeinsam { tasten: VecDeque::new(), maus: (0, 0), achsen: [0.0; 6], bild: 0, pal: [0; 256], bilder: 0, beendet: false });

pub(crate) fn gemeinsam() -> std::sync::MutexGuard<'static, Gemeinsam> {
    GEMEINSAM.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn einrichten(pak: Pak, pak_datei: &str) {
    let _ = PAK.set(pak);
    let _ = PAK_DATEI.set(CString::new(pak_datei).expect("Pfad ohne Nullbyte"));
    START.get_or_init(Instant::now);
}

/// Haken für „Quit“ im Quake-Menü (Firmware: Neustart in die Konsole);
/// ohne Haken beendet Quake das Programm.
pub fn beim_beenden(f: fn()) {
    let _ = BEENDEN.set(f);
}

/// Haken für schwere Fehler in Quake (`Sys_Error`, z. B. Speicher voll):
/// bekommt die Meldung im Quake-Thread und soll nicht zurückkehren
/// (Firmware: loggen, Meldung sichern, Neustart). Kehrt er doch zurück –
/// oder fehlt er –, bricht das Programm ab (ohne Haken mit der Meldung auf
/// stderr).
pub fn beim_fehler(f: fn(&str)) {
    let _ = FEHLER.set(f);
}

/// Meldung des `Sys_Error`, der gerade zum Abbruch führt (für den Haken aus
/// [`beim_fehler`]); vorher `None`.
pub fn fehlermeldung() -> Option<&'static str> {
    FEHLERMELDUNG.get().map(String::as_str)
}

/// Haken für die Zeilen des Autopiloten (eine je Levelwechsel, siehe
/// [`crate::Quake::autopilot`]); ohne Haken gehen sie an `println!`.
pub fn beim_bericht(f: fn(&str)) {
    let _ = BERICHT.set(f);
}

pub(crate) fn berichten(text: &str) {
    match BERICHT.get() {
        Some(f) => f(text),
        None => println!("{text}"),
    }
}

/// Konsolenbefehl an Quake.
pub fn befehl(text: &str) {
    let c = CString::new(text).expect("Befehl ohne Nullbyte");
    // SAFETY: gültige C-Zeichenkette; Quake kopiert sie in seinen Puffer.
    unsafe { QG_Befehl(c.as_ptr()) };
}

// --- Messwerte: nur abfragen, während der Quake-Thread wartet ----------------------

/// Freier Platz im Hunk (Bytes; dort liegt auch der verdrängbare Cache).
pub(crate) fn hunk_frei() -> usize {
    // SAFETY: liest nur Zähler des Hunks; Quake rechnet gerade nicht.
    unsafe { QG_HunkFrei() }.max(0) as usize
}

/// Name der laufenden Karte ("" ohne laufenden Server).
pub(crate) fn karte() -> String {
    // SAFETY: Zeiger auf sv.name (statisch) bzw. ""; Quake rechnet gerade nicht.
    unsafe { CStr::from_ptr(QG_Karte()) }.to_string_lossy().into_owned()
}

/// Wert einer Quake-Variablen (cvar), 0 wenn es sie nicht gibt.
pub(crate) fn variable(name: &str) -> f32 {
    let c = CString::new(name).expect("Name ohne Nullbyte");
    // SAFETY: gültige C-Zeichenkette, Quake liest sie nur.
    unsafe { Cvar_VariableValue(c.as_ptr()) }
}

/// Im Quake-Thread aufrufen: merkt ihn sich für [`stapel_frei`].
#[cfg(target_os = "espidf")]
pub(crate) fn stapel_merken() {
    // SAFETY: merkt sich nur den aufrufenden FreeRTOS-Task.
    unsafe { QG_StapelMerken() }
}

#[cfg(not(target_os = "espidf"))]
pub(crate) fn stapel_merken() {}

/// Kleinster bisher freier Rest des Stapels im Quake-Thread (Bytes); nur am
/// Board bekannt (FreeRTOS-Füllmuster).
#[cfg(target_os = "espidf")]
pub(crate) fn stapel_frei() -> Option<usize> {
    // SAFETY: liest den Stapel des gemerkten Tasks; der wartet gerade.
    let frei = unsafe { QG_StapelFrei() };
    usize::try_from(frei).ok()
}

#[cfg(not(target_os = "espidf"))]
pub(crate) fn stapel_frei() -> Option<usize> {
    None
}

fn ist_pak(pfad: &[u8]) -> bool {
    let p: Vec<u8> = pfad.iter().map(|b| if *b == b'\\' { b'/' } else { b.to_ascii_lowercase() }).collect();
    p.ends_with(b"id1/pak0.pak")
}

// --- von Quake aufgerufen ---------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn QG_Init() {}

#[unsafe(no_mangle)]
pub extern "C" fn QG_Quit() {}

/// # Safety
/// `pixels` zeigt auf das 320×240-Bild von Quake (statisch).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_DrawFrame(pixels: *mut c_void) {
    let mut g = gemeinsam();
    g.bild = pixels as usize;
    g.bilder += 1;
}

/// # Safety
/// `palette` zeigt auf 768 Bytes (RGB).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_SetPalette(palette: *mut c_uchar) {
    // SAFETY: siehe oben.
    let p = unsafe { core::slice::from_raw_parts(palette, 768) };
    let mut g = gemeinsam();
    for (o, c) in g.pal.iter_mut().zip(p.chunks_exact(3)) {
        let (r, gr, b) = (c[0] as u16, c[1] as u16, c[2] as u16);
        *o = ((r & 0xF8) << 8) | ((gr & 0xFC) << 3) | (b >> 3);
    }
}

/// # Safety
/// Quake übergibt gültige Zeiger.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_GetKey(down: *mut c_int, key: *mut c_int) -> c_int {
    match gemeinsam().tasten.pop_front() {
        Some((unten, k)) => {
            // SAFETY: siehe oben.
            unsafe {
                *down = unten as c_int;
                *key = k;
            }
            1
        }
        None => 0,
    }
}

/// # Safety
/// Quake übergibt gültige Zeiger.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_GetMouseMove(x: *mut c_int, y: *mut c_int) {
    let (dx, dy) = core::mem::take(&mut gemeinsam().maus);
    // SAFETY: siehe oben.
    unsafe {
        *x = dx;
        *y = dy;
    }
}

/// # Safety
/// `achsen` zeigt auf 6 Werte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_GetJoyAxes(achsen: *mut f32) {
    let a = gemeinsam().achsen;
    // SAFETY: siehe oben.
    unsafe { core::ptr::copy_nonoverlapping(a.as_ptr(), achsen, 6) };
}

/// Größe der Spieldaten, falls `pfad` sie meint (sonst −1).
///
/// # Safety
/// `pfad` ist eine C-Zeichenkette.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_DateiGroesse(pfad: *const c_char) -> c_int {
    if pfad.is_null() {
        return -1;
    }
    // SAFETY: siehe oben.
    let p = unsafe { core::ffi::CStr::from_ptr(pfad) }.to_bytes();
    match (ist_pak(p), PAK.get()) {
        (true, Some(Pak::Speicher(d))) => d.len() as c_int,
        (true, Some(Pak::Lesen { groesse, .. })) => *groesse as c_int,
        _ => -1,
    }
}

/// Aus den Spieldaten lesen.
///
/// # Safety
/// `ziel` bietet Platz für `anzahl` Bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_DateiLesen(pos: c_int, ziel: *mut c_void, anzahl: c_int) -> c_int {
    if pos < 0 || anzahl <= 0 {
        return 0;
    }
    let (pos, n) = (pos as usize, anzahl as usize);
    // SAFETY: siehe oben.
    let ziel = unsafe { core::slice::from_raw_parts_mut(ziel as *mut u8, n) };
    match PAK.get() {
        Some(Pak::Speicher(d)) => {
            let n = n.min(d.len().saturating_sub(pos));
            ziel[..n].copy_from_slice(&d[pos..pos + n]);
            n as c_int
        }
        Some(Pak::Lesen { groesse, lesen }) => {
            let n = n.min(groesse.saturating_sub(pos));
            if lesen(pos, &mut ziel[..n]) { n as c_int } else { 0 }
        }
        None => 0,
    }
}

/// Pfad, unter dem `fopen` die Spieldaten öffnen kann (sonst `pfad` selbst).
///
/// # Safety
/// `pfad` ist eine C-Zeichenkette, die bis nach dem `fopen` lebt.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_PakPfad(pfad: *const c_char) -> *const c_char {
    if pfad.is_null() {
        return pfad;
    }
    // SAFETY: siehe oben.
    let p = unsafe { core::ffi::CStr::from_ptr(pfad) }.to_bytes();
    match PAK_DATEI.get() {
        Some(d) if ist_pak(p) => d.as_ptr(),
        _ => pfad,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn QG_Zeit() -> c_double {
    if VIRTUELL.load(Relaxed) {
        ZEIT_US.load(Relaxed) as f64 * 1e-6
    } else {
        START.get_or_init(Instant::now).elapsed().as_secs_f64()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn QG_Beenden() {
    gemeinsam().beendet = true;
    if let Some(f) = BEENDEN.get() {
        f();
    }
}

/// `Sys_Error` (sys_null.c): Meldung merken, Haken aus [`beim_fehler`]
/// rufen; kehrt nie zurück.
///
/// # Safety
/// `meldung` ist eine C-Zeichenkette (oder null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn QG_Fehler(meldung: *const c_char) -> ! {
    let text = if meldung.is_null() {
        String::new()
    } else {
        // SAFETY: siehe oben.
        unsafe { CStr::from_ptr(meldung) }.to_string_lossy().into_owned()
    };
    let text = FEHLERMELDUNG.get_or_init(|| text);
    match FEHLER.get() {
        Some(f) => f(text),
        None => eprintln!("Sys_Error: {text}"),
    }
    std::process::abort()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_pak0_kommt_von_der_rust_seite() {
        assert!(ist_pak(b"./id1/pak0.pak"));
        assert!(ist_pak(b"/daten/quake/ID1/PAK0.PAK"));
        assert!(ist_pak(b"C:\\x\\id1\\pak0.pak"));
        assert!(!ist_pak(b"./id1/pak1.pak"));
        assert!(!ist_pak(b"./id1/config.cfg"));
    }
}
