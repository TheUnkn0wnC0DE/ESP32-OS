//! Eigener Heap je Programm: First-Fit-Freiliste (nach Adresse sortiert) mit
//! Verschmelzen benachbarter freier Blöcke, über einem festen Speicherbereich.
//!
//! Der Verwaltungskopf (`Heap`) liegt am Anfang des Bereichs selbst, damit das
//! Anlegen keinen weiteren Speicher braucht. Wird das Programm beendet, gibt
//! der Kern den ganzen Bereich auf einmal frei – Lecks des Programms bleiben
//! dadurch folgenlos.
//!
//! Blockaufbau (alle Grenzen auf `GRAN` ausgerichtet):
//! `[b] = Blockgröße`, Nutzzeiger `p = align_up(b + GRAN, align)`,
//! `[p - W] = b`. Freie Blöcke tragen `[b] = Größe`, `[b + W] = nächster`.

use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const W: usize = core::mem::size_of::<usize>();
/// Ausrichtung aller Blockgrenzen.
const GRAN: usize = 2 * W;
/// Kleinster Block: Kopf + Freilisten-Knoten.
const MIN_BLOCK: usize = 2 * GRAN;

#[repr(C)]
struct Frei {
    groesse: usize,
    next: *mut Frei,
}

/// Heap über einem festen Bereich. Thread-sicher (kurze Spin-Sperre).
pub struct Heap {
    arena_start: usize,
    arena_ende: usize,
    frei: UnsafeCell<*mut Frei>,
    belegt: AtomicUsize,
    spitze: AtomicUsize,
    sperre: AtomicBool,
}

// SAFETY: Zugriff auf `frei` nur unter `sperre`.
unsafe impl Sync for Heap {}
// SAFETY: wie oben.
unsafe impl Send for Heap {}

const fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

const fn align_down(x: usize, a: usize) -> usize {
    x & !(a - 1)
}

/// Kleinste Bereichsgrösse, in der ein Heap mit Nutzfläche Platz hat.
pub const MIN_BEREICH: usize = core::mem::size_of::<Heap>() + 4 * GRAN + MIN_BLOCK;

impl Heap {
    /// Legt einen Heap über `[basis, basis + laenge)` an.
    ///
    /// # Safety
    /// Der Bereich muss gültig, beschreibbar, exklusiv und mindestens
    /// `MIN_BEREICH` gross sein und so lange leben wie der zurückgegebene Heap.
    pub unsafe fn anlegen(basis: *mut u8, laenge: usize) -> *const Heap {
        let kopf = align_up(basis as usize, core::mem::align_of::<Heap>().max(GRAN));
        let start = align_up(kopf + core::mem::size_of::<Heap>(), GRAN);
        let ende = align_down(basis as usize + laenge, GRAN);
        assert!(ende > start && ende - start >= MIN_BLOCK, "Heap-Bereich zu klein");
        let erster = start as *mut Frei;
        // SAFETY: Bereich laut Vertrag gültig und exklusiv.
        unsafe {
            (*erster).groesse = ende - start;
            (*erster).next = ptr::null_mut();
            let h = kopf as *mut Heap;
            h.write(Heap {
                arena_start: start,
                arena_ende: ende,
                frei: UnsafeCell::new(erster),
                belegt: AtomicUsize::new(0),
                spitze: AtomicUsize::new(0),
                sperre: AtomicBool::new(false),
            });
            h
        }
    }

    fn sperren(&self) {
        let mut runden = 0u32;
        while self
            .sperre
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            runden += 1;
            if runden < 64 {
                core::hint::spin_loop();
            } else if runden % 16 == 0 {
                // Halter evtl. auf demselben Kern mit tieferer Priorität: kurz schlafen.
                std::thread::sleep(std::time::Duration::from_millis(1));
            } else {
                std::thread::yield_now();
            }
        }
    }

    fn entsperren(&self) {
        self.sperre.store(false, Ordering::Release);
    }

    /// Belegt `groesse` Bytes mit Ausrichtung `align` (Zweierpotenz). Null, wenn voll.
    pub fn alloc(&self, groesse: usize, align: usize) -> *mut u8 {
        let align = align.max(GRAN);
        let groesse = groesse.max(1);
        self.sperren();
        // SAFETY: Freiliste nur unter Sperre, Knoten liegen in der Arena.
        let p = unsafe {
            let mut vor: *mut *mut Frei = self.frei.get();
            let mut gefunden = ptr::null_mut();
            while !(*vor).is_null() {
                let blk = *vor;
                let b = blk as usize;
                let s = (*blk).groesse;
                // Überlauf (riesige Anfragen, z. B. malloc(-1)) = passt nicht
                let p = (b + GRAN).checked_add(align - 1).map(|v| v & !(align - 1));
                let ende = p.and_then(|p| p.checked_add(groesse)).and_then(|e| e.checked_add(GRAN - 1)).map(|e| e & !(GRAN - 1));
                if let (Some(p), Some(ende)) = (p, ende)
                    && ende <= b + s
                {
                    let rest = b + s - ende;
                    let next = (*blk).next;
                    let nutz = if rest >= MIN_BLOCK {
                        let r = ende as *mut Frei;
                        (*r).groesse = rest;
                        (*r).next = next;
                        *vor = r;
                        ende - b
                    } else {
                        *vor = next;
                        s
                    };
                    *(b as *mut usize) = nutz;
                    *((p - W) as *mut usize) = b;
                    let neu = self.belegt.fetch_add(nutz, Ordering::Relaxed) + nutz;
                    self.spitze.fetch_max(neu, Ordering::Relaxed);
                    gefunden = p as *mut u8;
                    break;
                }
                vor = &raw mut (*blk).next;
            }
            gefunden
        };
        self.entsperren();
        p
    }

    /// Gibt einen von `alloc` gelieferten Zeiger zurück.
    ///
    /// # Safety
    /// `p` stammt aus `alloc` dieses Heaps und wurde noch nicht freigegeben.
    pub unsafe fn free(&self, p: *mut u8) {
        // SAFETY: Kopf laut Vertrag gültig.
        let (b, s) = unsafe {
            let b = *((p as usize - W) as *const usize);
            (b, *(b as *const usize))
        };
        self.sperren();
        // SAFETY: Freiliste nur unter Sperre.
        unsafe {
            self.belegt.fetch_sub(s, Ordering::Relaxed);
            let mut vor: *mut *mut Frei = self.frei.get();
            let mut links: *mut Frei = ptr::null_mut();
            while !(*vor).is_null() && (*vor as usize) < b {
                links = *vor;
                vor = &raw mut (*links).next;
            }
            let rechts = *vor;
            let neu = b as *mut Frei;
            (*neu).groesse = s;
            (*neu).next = rechts;
            *vor = neu;
            if !rechts.is_null() && b + s == rechts as usize {
                (*neu).groesse += (*rechts).groesse;
                (*neu).next = (*rechts).next;
            }
            if !links.is_null() && links as usize + (*links).groesse == b {
                (*links).groesse += (*neu).groesse;
                (*links).next = (*neu).next;
            }
        }
        self.entsperren();
    }

    /// Nutzbare Grösse des Blocks hinter `p` (Funktion 68.20).
    ///
    /// # Safety
    /// `p` stammt aus `alloc` dieses Heaps und ist belegt.
    pub unsafe fn nutzgroesse(&self, p: *mut u8) -> usize {
        // SAFETY: Kopf laut Vertrag gültig.
        unsafe {
            let b = *((p as usize - W) as *const usize);
            b + *(b as *const usize) - p as usize
        }
    }

    /// Ist die Sperre gesetzt (Zwangsbeenden: ein angehaltener Faden darf
    /// sie nicht halten)?
    pub fn gesperrt(&self) -> bool {
        self.sperre.load(Ordering::Acquire)
    }

    /// Belegte Bytes (inkl. Blockköpfe).
    pub fn belegt(&self) -> usize {
        self.belegt.load(Ordering::Relaxed)
    }

    /// Höchststand von `belegt`.
    pub fn spitze(&self) -> usize {
        self.spitze.load(Ordering::Relaxed)
    }

    /// Nutzbare Grösse der Arena.
    pub fn groesse(&self) -> usize {
        self.arena_ende - self.arena_start
    }

    /// Liegt `p` in der Arena dieses Heaps?
    pub fn enthaelt(&self, p: *const u8) -> bool {
        (self.arena_start..self.arena_ende).contains(&(p as usize))
    }

    /// Anzahl freier Blöcke (Prüfung des Verschmelzens).
    pub fn freie_bloecke(&self) -> usize {
        self.sperren();
        let mut n = 0;
        // SAFETY: unter Sperre.
        unsafe {
            let mut k = *self.frei.get();
            while !k.is_null() {
                n += 1;
                k = (*k).next;
            }
        }
        self.entsperren();
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bereich(n: usize) -> (Vec<u64>, *const Heap) {
        let mut v = vec![0u64; n / 8];
        let h = unsafe { Heap::anlegen(v.as_mut_ptr().cast(), n) };
        (v, h)
    }

    #[test]
    fn belegen_freigeben_verschmilzt_wieder() {
        let (_v, h) = bereich(64 * 1024);
        let h = unsafe { &*h };
        let mut z = Vec::new();
        for i in 0..200 {
            let p = h.alloc(1 + (i * 37) % 300, 8);
            assert!(!p.is_null());
            assert!(h.enthaelt(p));
            unsafe { p.write_bytes(i as u8, 1 + (i * 37) % 300) };
            z.push((p, i));
        }
        // Inhalte unversehrt
        for &(p, i) in &z {
            assert_eq!(unsafe { *p }, i as u8);
        }
        // jede zweite frei, dann Rest
        for (k, &(p, _)) in z.iter().enumerate() {
            if k % 2 == 0 {
                unsafe { h.free(p) };
            }
        }
        for (k, &(p, _)) in z.iter().enumerate() {
            if k % 2 == 1 {
                unsafe { h.free(p) };
            }
        }
        assert_eq!(h.belegt(), 0);
        assert_eq!(h.freie_bloecke(), 1);
        assert!(h.spitze() > 0);
    }

    #[test]
    fn ausrichtung() {
        let (_v, h) = bereich(256 * 1024);
        let h = unsafe { &*h };
        for a in [1, 2, 8, 16, 64, 256, 4096] {
            let p = h.alloc(10, a);
            assert_eq!(p as usize % a, 0, "align {a}");
            unsafe { h.free(p) };
        }
        assert_eq!(h.freie_bloecke(), 1);
    }

    #[test]
    fn voll_liefert_null() {
        let (_v, h) = bereich(8 * 1024);
        let h = unsafe { &*h };
        assert!(h.alloc(16 * 1024, 8).is_null());
        let p = h.alloc(1024, 8);
        assert!(!p.is_null());
    }

    #[test]
    fn mehrere_threads() {
        let (_v, h) = bereich(1024 * 1024);
        let h = h as usize;
        let t: Vec<_> = (0..4)
            .map(|k| {
                std::thread::spawn(move || {
                    let h = unsafe { &*(h as *const Heap) };
                    for i in 0..2000 {
                        let p = h.alloc(8 + (i + k) % 100, 8);
                        assert!(!p.is_null());
                        unsafe { h.free(p) };
                    }
                })
            })
            .collect();
        for x in t {
            x.join().unwrap();
        }
        let h = unsafe { &*(h as *const Heap) };
        assert_eq!(h.belegt(), 0);
        assert_eq!(h.freie_bloecke(), 1);
    }
}
