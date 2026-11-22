//! Zählender Allokator: misst Heap-Spitze und prüft, dass im laufenden Spiel
//! (nach dem Aufwärmen) pro Frame keine Allokationen mehr stattfinden.
//! Kosten: ein paar atomare Additionen pro Allokation – und die gibt es im
//! Frame ja gerade nicht.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

pub struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static THREAD_ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn note_alloc(size: usize) {
    let _ = THREAD_ALLOCS.try_with(|c| c.set(c.get() + 1));
    let cur = CURRENT.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(cur, Relaxed);
}

// SAFETY: reicht alle Aufrufe unverändert an den System-Allokator weiter.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: Vertrag von GlobalAlloc::alloc wird durchgereicht.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            note_alloc(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        // SAFETY: wie oben.
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            note_alloc(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: wie oben.
        unsafe { System.dealloc(p, l) };
        CURRENT.fetch_sub(l.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: wie oben.
        let q = unsafe { System.realloc(p, l, new_size) };
        if !q.is_null() {
            CURRENT.fetch_sub(l.size(), Relaxed);
            note_alloc(new_size);
        }
        q
    }
}

/// Anzahl der Allokationen dieses Threads seit Start.
pub fn thread_allocs() -> usize {
    THREAD_ALLOCS.try_with(|c| c.get()).unwrap_or(0)
}

/// Höchster gleichzeitig belegter Heap (Bytes).
pub fn peak_bytes() -> usize {
    PEAK.load(Relaxed)
}
