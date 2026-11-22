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
            // SAFETY: gültiges Layout laut Vertrag.
            let p = unsafe { System.alloc(l) };
            if !p.is_null() {
                not_eintragen(p, l, pid);
            }
            return p;
        }
        // SAFETY: gültiges Layout laut Vertrag.
        unsafe { System.alloc(l) }
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
        unsafe { System.alloc_zeroed(l) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        match region_von(p) {
            // SAFETY: p stammt aus diesem Heap.
            Some(h) => unsafe { h.free(p) },
            None => {
                not_austragen(p);
                // SAFETY: p stammt aus dem System-Heap.
                unsafe { System.dealloc(p, l) }
            }
        }
    }
}
