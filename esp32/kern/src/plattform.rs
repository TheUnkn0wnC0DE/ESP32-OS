//! Plattform-Schnittstelle des Kerns: was am Board (FreeRTOS/ESP-IDF) und am
//! PC (Simulator, Tests) verschieden ist. Alles als einfache Funktionszeiger,
//! damit der globale Allocator ohne Trait-Objekt darauf zugreifen kann.

use crate::heap::Heap;
use crate::prozess::Faden;

/// Ergebnis eines Programm-Fadens, wie es die Plattform zurückmeldet.
pub enum Lauf {
    /// Startfunktion ist normal zurückgekehrt.
    Normal,
    /// Der Faden hat sich schon selbst beim Kern abgemeldet
    /// (Systemfunktion −1, Beenden, Speicher voll).
    SchonBeendet,
    /// Rust-Panik (nur Plattformen mit Abwickeln).
    Panik,
}

/// Marker-Nutzlast, mit der sich ein Faden am PC selbst abwickelt.
pub struct ProzessEnde;

pub struct Plattform {
    /// Faden-Kontext des laufenden Threads (null = Kern/System).
    pub faden_lesen: fn() -> *mut Faden,
    pub faden_setzen: fn(*mut Faden),
    /// Startet einen Task/Thread. `heap` = Heap des Programms (Board: Stapel
    /// dort anlegen). Liefert einen Griff für `task_weg`/`task_freigeben`.
    /// `fd` = Fadenkontext; der Task ruft [`crate::prozess::faden_eintritt`]
    /// (keine Box im System-Heap, Q4).
    pub task_starten: fn(name: &str, stapel: usize, heap: &Heap, fd: usize) -> Option<usize>,
    /// Führt die Startfunktion im neuen Faden aus.
    pub ausfuehren: fn(Box<dyn FnOnce() + Send>) -> Lauf,
    /// Beendet den laufenden Faden sofort (kehrt nie zurück).
    pub task_beenden: fn() -> !,
    /// Ist der Task vollständig weg (Stapel/Kopf werden nicht mehr benutzt)?
    pub task_weg: fn(usize) -> bool,
    /// Gibt die Verwaltung des Tasks frei (nach `task_weg`).
    pub task_freigeben: fn(usize),
    /// Hält einen fremden Task an (Zwangsbeenden, A6). true = steht (läuft
    /// auf keinem Kern mehr); false = stand nicht rechtzeitig (läuft
    /// weiter). None = Plattform kann nicht anhalten (PC-Threads).
    pub task_anhalten: Option<fn(usize) -> bool>,
    /// Lässt einen mit `task_anhalten` angehaltenen Task weiterlaufen.
    pub task_fortsetzen: fn(usize),
    /// Speicherbereich für einen Programm-Heap (Board: PSRAM).
    pub bereich_holen: fn(usize) -> *mut u8,
    pub bereich_frei: fn(*mut u8, usize),
    /// true: eine Panik in einem Programm-Faden beendet nur diesen Faden über
    /// `task_beenden` (Plattformen ohne Abwickeln, z. B. das Board).
    pub panik_beendet_task: bool,
    /// Grösster freier Block des System-Heaps in Bytes (Board: PSRAM). Grosse
    /// Belegungen für Programme (Fensterpuffer) prüfen damit vorher, statt
    /// bei vollem Speicher die ganze Firmware abzubrechen.
    pub system_frei: fn() -> usize,
    /// Freier interner RAM in Bytes (Board: den brauchen WLAN, lwIP und TLS).
    /// M3: neue Netzverbindungen nur mit Reserve.
    pub intern_frei: fn() -> usize,
}

#[cfg(not(target_os = "espidf"))]
mod pc {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
    use std::sync::Mutex;
    use std::thread::JoinHandle;

    thread_local! {
        static FADEN: Cell<*mut Faden> = const { Cell::new(core::ptr::null_mut()) };
    }

    static GRIFFE: Mutex<Vec<Option<JoinHandle<()>>>> = Mutex::new(Vec::new());
    /// Q4 (Test): Fäden, die ohne Abwickeln geendet haben (geparkt).
    static GEPARKT: Mutex<Vec<std::thread::ThreadId>> = Mutex::new(Vec::new());

    fn lesen() -> *mut Faden {
        FADEN.try_with(Cell::get).unwrap_or(core::ptr::null_mut())
    }

    fn setzen(f: *mut Faden) {
        let _ = FADEN.try_with(|c| c.set(f));
    }

    fn starten(name: &str, stapel: usize, _heap: &Heap, fd: usize) -> Option<usize> {
        let _ = crate::alloc::spur::PC_START.try_with(|c| c.set(true));
        let h = std::thread::Builder::new()
            .name(name.into())
            .stack_size(stapel.max(256 * 1024))
            .spawn(move || crate::prozess::faden_eintritt(fd));
        let _ = crate::alloc::spur::PC_START.try_with(|c| c.set(false));
        let h = h.ok()?;
        let mut g = GRIFFE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = g.iter().position(Option::is_none) {
            g[i] = Some(h);
            Some(i)
        } else {
            g.push(Some(h));
            Some(g.len() - 1)
        }
    }

    fn ausfuehren(f: Box<dyn FnOnce() + Send>) -> Lauf {
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(()) => Lauf::Normal,
            Err(e) if e.is::<ProzessEnde>() => Lauf::SchonBeendet,
            Err(_) => Lauf::Panik,
        }
    }

    fn beenden() -> ! {
        // Q4 (Test): wie am Board ohne Abwickeln enden – keine Destruktoren,
        // was der Faden hält, bleibt liegen; der Faden parkt für immer
        if crate::TEST_OHNE_ABWICKELN.load(core::sync::atomic::Ordering::Relaxed) {
            GEPARKT.lock().unwrap_or_else(|e| e.into_inner()).push(std::thread::current().id());
            loop {
                std::thread::park();
            }
        }
        resume_unwind(Box::new(ProzessEnde))
    }

    fn geparkt(h: &JoinHandle<()>) -> bool {
        GEPARKT.lock().unwrap_or_else(|e| e.into_inner()).contains(&h.thread().id())
    }

    fn weg(g: usize) -> bool {
        let l = GRIFFE.lock().unwrap_or_else(|e| e.into_inner());
        l.get(g).and_then(Option::as_ref).is_none_or(|h| h.is_finished() || geparkt(h))
    }

    fn freigeben(g: usize) {
        let h = GRIFFE.lock().unwrap_or_else(|e| e.into_inner()).get_mut(g).and_then(Option::take);
        if let Some(h) = h
            && !geparkt(&h)
        {
            let _ = h.join();
        }
    }

    fn fortsetzen(_g: usize) {}

    /// PC: unbegrenzt, ausser ein Test setzt eine Grenze
    /// ([`crate::grafik::TEST_SYSTEM_FREI`]).
    fn system_frei() -> usize {
        crate::grafik::TEST_SYSTEM_FREI.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// PC: unbegrenzt, ausser ein Test setzt eine Grenze
    /// ([`crate::netz::TEST_INTERN_FREI`]).
    fn intern_frei() -> usize {
        crate::netz::TEST_INTERN_FREI.load(core::sync::atomic::Ordering::Relaxed)
    }

    fn layout(n: usize) -> Layout {
        Layout::from_size_align(n, 64).expect("Bereichsgrösse")
    }

    fn holen(n: usize) -> *mut u8 {
        // SAFETY: n > 0 (vom Kern garantiert).
        unsafe { System.alloc(layout(n)) }
    }

    fn frei(p: *mut u8, n: usize) {
        // SAFETY: p stammt aus `holen(n)`.
        unsafe { System.dealloc(p, layout(n)) }
    }

    /// PC-Plattform (Simulator und Tests): Threads, Abwickeln, System-Heap.
    pub static PC: Plattform = Plattform {
        faden_lesen: lesen,
        faden_setzen: setzen,
        task_starten: starten,
        ausfuehren,
        task_beenden: beenden,
        task_weg: weg,
        task_freigeben: freigeben,
        system_frei,
        intern_frei,
        // Std-Threads lassen sich nicht von aussen anhalten
        task_anhalten: None,
        task_fortsetzen: fortsetzen,
        bereich_holen: holen,
        bereich_frei: frei,
        panik_beendet_task: false,
    };
}

#[cfg(not(target_os = "espidf"))]
pub use pc::PC;
