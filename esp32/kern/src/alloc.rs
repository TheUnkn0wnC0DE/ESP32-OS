//! Globaler Allocator des Systems: Programm-Fäden belegen aus dem Heap ihres
//! Programms, Kern und System aus dem System-Heap. Freigegeben wird nach
//! Adresse – egal, welcher Faden freigibt.

use crate::heap::Heap;
use crate::plattform::Plattform;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};

pub(crate) static PL: AtomicPtr<Plattform> = AtomicPtr::new(ptr::null_mut());

const MAX_REGIONEN: usize = 64;
static REGIONEN: [AtomicPtr<Heap>; MAX_REGIONEN] = [const { AtomicPtr::new(ptr::null_mut()) }; MAX_REGIONEN];

pub(crate) fn region_eintragen(h: *const Heap) -> bool {
    REGIONEN.iter().any(|r| {
        r.compare_exchange(ptr::null_mut(), h.cast_mut(), Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    })
}

pub(crate) fn region_austragen(h: *const Heap) {
    for r in &REGIONEN {
        let _ = r.compare_exchange(h.cast_mut(), ptr::null_mut(), Ordering::AcqRel, Ordering::Relaxed);
    }
}

/// Anzahl eingetragener Programm-Heaps.
pub fn regionen() -> usize {
    REGIONEN.iter().filter(|r| !r.load(Ordering::Relaxed).is_null()).count()
}

fn region_von(p: *const u8) -> Option<&'static Heap> {
    for r in &REGIONEN {
        let h = r.load(Ordering::Acquire);
        if !h.is_null() {
            // SAFETY: eingetragene Heaps leben, bis sie ausgetragen werden.
            let h = unsafe { &*h };
            if h.enthaelt(p) {
                return Some(h);
            }
        }
    }
    None
}

// Notbelegungen: Ist der Heap eines Programms voll, wird die Belegung noch aus
// dem System bedient (das Programm endet bei der nächsten Systemfunktion).
// Diese Blöcke werden hier je Programm geführt und beim Aufräumen freigegeben.
const MAX_NOT: usize = 64;
/// M3: grösste Notbelegung aus dem System für ein Programm mit vollem Heap.
const NOT_MAX_BELEGUNG: usize = 64 * 1024;
/// M3: so viel System-Heap (grösster Block) bleibt bei Notbelegungen frei.
const NOT_RESERVE: usize = 512 * 1024;
/// Review 27.11.: in einem geschützten Abschnitt hält das Programm oft eine
/// gemeinsame Sperre (Shell-Zwischenspeicher); dort endet es nicht sofort
/// (am Board bliebe die Sperre für immer belegt), sondern beim nächsten
/// Systemaufruf. Notbelegungen dort bis zu dieser Grösse und mit dieser
/// kleineren Reserve.
const NOT_MAX_GESCHUETZT: usize = 256 * 1024;
const NOT_RESERVE_GESCHUETZT: usize = 128 * 1024;
static NOT_PTR: [AtomicUsize; MAX_NOT] = [const { AtomicUsize::new(0) }; MAX_NOT];
static NOT_GROESSE: [AtomicUsize; MAX_NOT] = [const { AtomicUsize::new(0) }; MAX_NOT];
static NOT_ALIGN: [AtomicUsize; MAX_NOT] = [const { AtomicUsize::new(0) }; MAX_NOT];
static NOT_PID: [AtomicU32; MAX_NOT] = [const { AtomicU32::new(0) }; MAX_NOT];
static NOT_ANZ: AtomicUsize = AtomicUsize::new(0);

fn not_eintragen(p: *mut u8, l: Layout, pid: u32) {
    for i in 0..MAX_NOT {
        if NOT_PTR[i].compare_exchange(0, usize::MAX, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
            NOT_GROESSE[i].store(l.size(), Ordering::Relaxed);
            NOT_ALIGN[i].store(l.align(), Ordering::Relaxed);
            NOT_PID[i].store(pid, Ordering::Relaxed);
            NOT_ANZ.fetch_add(1, Ordering::AcqRel);
            NOT_PTR[i].store(p as usize, Ordering::Release);
            return;
        }
    }
}

fn not_austragen(p: *mut u8) {
    if NOT_ANZ.load(Ordering::Acquire) == 0 {
        return;
    }
    for i in 0..MAX_NOT {
        if NOT_PTR[i].compare_exchange(p as usize, 0, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
            NOT_ANZ.fetch_sub(1, Ordering::AcqRel);
            return;
        }
    }
}

/// Gibt alle noch offenen Notbelegungen eines beendeten Programms frei.
pub(crate) fn not_freigeben(pid: u32) {
    for i in 0..MAX_NOT {
        let p = NOT_PTR[i].load(Ordering::Acquire);
        if p != 0 && p != usize::MAX && NOT_PID[i].load(Ordering::Relaxed) == pid
            && NOT_PTR[i].compare_exchange(p, 0, Ordering::AcqRel, Ordering::Relaxed).is_ok()
        {
            NOT_ANZ.fetch_sub(1, Ordering::AcqRel);
            let l = Layout::from_size_align(NOT_GROESSE[i].load(Ordering::Relaxed), NOT_ALIGN[i].load(Ordering::Relaxed));
            if let Ok(l) = l {
                // gezählt wurde er bei der Belegung (system_zaehlen)
                SYSTEM_BYTES.fetch_sub(l.size() as isize, Ordering::Relaxed);
                #[cfg(not(target_os = "espidf"))]
                spur::frei(p as *mut u8);
                #[cfg(not(target_os = "espidf"))]
                spur::pc_frei(p as *mut u8);
                // SAFETY: Block stammt aus System.alloc mit diesem Layout.
                unsafe { System.dealloc(p as *mut u8, l) };
            }
        }
    }
}

/// Anzahl offener Notbelegungen (Tests).
pub fn notbelegungen() -> usize {
    NOT_ANZ.load(Ordering::Acquire)
}

/// Heap des laufenden Programm-Fadens, falls nicht gerade Kern-Code läuft.
fn programm_heap() -> Option<(&'static Heap, *mut crate::prozess::Faden)> {
    let pl = PL.load(Ordering::Acquire);
    if pl.is_null() {
        return None;
    }
    // SAFETY: PL zeigt auf eine `'static`-Plattform.
    let f = unsafe { ((*pl).faden_lesen)() };
    if f.is_null() {
        return None;
    }
    // SAFETY: Faden gehört diesem Thread und lebt, solange er gesetzt ist.
    unsafe {
        if (*f).im_kern.load(core::sync::atomic::Ordering::Relaxed) != 0 {
            return None;
        }
        Some((&*(*f).heap, f))
    }
}

/// M8 (Lecksuche): Bytes, die gerade im System-Heap belegt sind (über
/// diesen Allokator; Board und PC). Ein PC-Test öffnet und schliesst
/// Programme und vergleicht den Stand.
static SYSTEM_BYTES: core::sync::atomic::AtomicIsize = core::sync::atomic::AtomicIsize::new(0);

/// Belegte Bytes im System-Heap über [`KernAlloc`] (siehe [`SYSTEM_BYTES`]).
pub fn system_belegt() -> isize {
    SYSTEM_BYTES.load(Ordering::Relaxed)
}

fn system_zaehlen(p: *mut u8, n: usize) -> *mut u8 {
    if !p.is_null() {
        SYSTEM_BYTES.fetch_add(n as isize, Ordering::Relaxed);
        #[cfg(not(target_os = "espidf"))]
        spur::belegt(p, n);
        #[cfg(not(target_os = "espidf"))]
        spur::pc_belegt(p, n);
    }
    p
}

/// Q4 (nur PC-Tests): Belegungen im System-Heap mit Backtrace, um zu
/// finden, wer Speicher anlegt, der nach dem Ende eines Programms liegen
/// bleibt (`spur_starten` / `spur_offen`).
#[cfg(not(target_os = "espidf"))]
pub mod spur {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    static AN: AtomicBool = AtomicBool::new(false);
    static OFFEN: Mutex<Option<HashMap<usize, (usize, String)>>> = Mutex::new(None);
    thread_local! {
        static DRIN: Cell<bool> = const { Cell::new(false) };
    }
    fn mit<R>(f: impl FnOnce() -> R) -> Option<R> {
        if !AN.load(Ordering::Relaxed) || DRIN.try_with(Cell::get).unwrap_or(true) {
            return None;
        }
        let _ = DRIN.try_with(|d| d.set(true));
        let r = f();
        let _ = DRIN.try_with(|d| d.set(false));
        Some(r)
    }
    pub(crate) fn belegt(p: *mut u8, n: usize) {
        mit(|| {
            let bt = std::backtrace::Backtrace::force_capture().to_string();
            OFFEN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(p as usize, (n, bt));
        });
    }
    // Belegungen der PC-Plattform beim Start eines Threads (std::thread:
    // Name, Thread-Daten) – bleiben im Testmodus „ohne Abwickeln“ liegen,
    // am Board gibt es sie nicht; der Lecktest zieht sie ab.
    thread_local! {
        pub(crate) static PC_START: Cell<bool> = const { Cell::new(false) };
    }
    static PC_INTERN: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
    static PC_SUMME: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    pub(crate) fn pc_belegt(p: *mut u8, n: usize) {
        if PC_START.try_with(Cell::get).unwrap_or(false) {
            let _ = PC_START.try_with(|c| c.set(false));
            // die Tabelle selbst belegt und gibt frei (Wachsen): nicht erneut
            // sperren (`DRIN`)
            let _ = DRIN.try_with(|d| d.set(true));
            PC_INTERN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(p as usize, n);
            let _ = DRIN.try_with(|d| d.set(false));
            PC_SUMME.fetch_add(n as isize, Ordering::Relaxed);
            let _ = PC_START.try_with(|c| c.set(true));
        }
    }
    pub(crate) fn pc_frei(p: *mut u8) {
        if DRIN.try_with(Cell::get).unwrap_or(true) {
            return;
        }
        let _ = DRIN.try_with(|d| d.set(true));
        let n = PC_INTERN.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|m| m.remove(&(p as usize)));
        let _ = DRIN.try_with(|d| d.set(false));
        if let Some(n) = n {
            PC_SUMME.fetch_sub(n as isize, Ordering::Relaxed);
        }
    }
    /// Noch belegte Bytes der PC-Plattform beim Thread-Start (siehe oben).
    pub fn pc_plattform_belegt() -> isize {
        PC_SUMME.load(Ordering::Relaxed)
    }
    pub(crate) fn frei(p: *mut u8) {
        mit(|| {
            if let Some(m) = OFFEN.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                m.remove(&(p as usize));
            }
        });
    }
    /// Spur leeren und einschalten.
    pub fn spur_starten() {
        mit(|| ());
        *OFFEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(HashMap::new());
        AN.store(true, Ordering::Relaxed);
    }
    /// Spur ausschalten; noch belegte Blöcke (Grösse, Backtrace).
    pub fn spur_offen() -> Vec<(usize, String)> {
        AN.store(false, Ordering::Relaxed);
        OFFEN.lock().unwrap_or_else(|e| e.into_inner()).take().map(|m| m.into_values().collect()).unwrap_or_default()
    }
}

/// `#[global_allocator]` des Systems (Board und PC).
pub struct KernAlloc;

// SAFETY: Belegen/Freigeben je Heap unter dessen Sperre; System-Heap ist
// thread-sicher.
unsafe impl GlobalAlloc for KernAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if let Some((h, f)) = programm_heap() {
            let p = h.alloc(l.size(), l.align());
            if !p.is_null() {
                return p;
            }
            // Programm-Heap voll: diese Belegung noch aus dem System bedienen,
            // das Programm wird bei der nächsten Systemfunktion beendet.
            // SAFETY: eigener Faden.
            let pid = unsafe {
                (*f).speicher_voll.set(true);
                (*f).pid
            };
            // M3 (feste Grenzen): nur kleine Notbelegungen und nur, solange
            // der System-Heap Reserve hat – eine grosse Belegung (Bild im
            // Browser) leerte sonst den System-Heap und das ganze System
            // stürzte ab. Dann endet das Programm sofort (nur auf
            // Plattformen ohne Abwickeln: der Faden hält dort einfach an).
            let pl = PL.load(Ordering::Acquire);
            // SAFETY: PL zeigt auf eine `'static`-Plattform.
            if !pl.is_null() && unsafe { (*pl).panik_beendet_task } {
                // SAFETY: wie oben.
                let frei = unsafe { ((*pl).system_frei)() };
                // SAFETY: eigener Faden.
                let geschuetzt = unsafe { (*f).geschuetzt.load(Ordering::Relaxed) } > 0;
                let (max, reserve) = if geschuetzt { (NOT_MAX_GESCHUETZT, NOT_RESERVE_GESCHUETZT) } else { (NOT_MAX_BELEGUNG, NOT_RESERVE) };
                if l.size() > max || frei < l.size().saturating_add(reserve) || NOT_ANZ.load(Ordering::Acquire) >= MAX_NOT {
                    crate::prozess::beende_selbst(crate::prozess::Ende::SpeicherVoll);
                }
            }
            // SAFETY: gültiges Layout laut Vertrag.
            let p = system_zaehlen(unsafe { System.alloc(l) }, l.size());
            if !p.is_null() {
                not_eintragen(p, l, pid);
            }
            return p;
        }
        // SAFETY: gültiges Layout laut Vertrag.
        system_zaehlen(unsafe { System.alloc(l) }, l.size())
    }

    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if programm_heap().is_some() {
            // SAFETY: wie `alloc`.
            let p = unsafe { self.alloc(l) };
            if !p.is_null() {
                // SAFETY: frisch belegt, l.size() Bytes.
                unsafe { p.write_bytes(0, l.size()) };
            }
            return p;
        }
        // SAFETY: gültiges Layout.
        system_zaehlen(unsafe { System.alloc_zeroed(l) }, l.size())
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        match region_von(p) {
            // SAFETY: p stammt aus diesem Heap.
            Some(h) => unsafe { h.free(p) },
            None => {
                not_austragen(p);
                SYSTEM_BYTES.fetch_sub(l.size() as isize, Ordering::Relaxed);
                #[cfg(not(target_os = "espidf"))]
                spur::frei(p);
                #[cfg(not(target_os = "espidf"))]
                spur::pc_frei(p);
                // SAFETY: p stammt aus dem System-Heap.
                unsafe { System.dealloc(p, l) }
            }
        }
    }
}
