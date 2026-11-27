//! Eigener Heap je Programm über einem festen Speicherbereich.
//!
//! - Kleine Blöcke (bis `KLEIN_MAX` Bytes samt Kopf) haben je Grösse eine
//!   eigene Freiliste (LIFO): Belegen und Freigeben ohne Suche, O(1).
//! - Grosse Blöcke: First-Fit nach Adresse (der freie Block mit der
//!   niedrigsten Adresse, der passt), mit Verschmelzen benachbarter freier
//!   Blöcke. Sie liegen in einem nach Adresse sortierten Treap, dessen Knoten
//!   die grösste Blockgrösse ihres Teilbaums kennen (M7, 27.11.): Suchen,
//!   Einfügen und Verschmelzen in O(log n) statt linear – dieselbe
//!   Verteilung wie die frühere sortierte Liste, also nicht mehr Verschnitt.
//!   Die Knoten liegen in den freien Blöcken selbst; die Priorität ist ein
//!   Hash der Adresse (kein Zufall, kein Platz).
//! - Reicht der Platz für einen grossen Block nicht, werden die kleinen
//!   freien Blöcke zurück in die grosse Liste geführt (nach Adresse
//!   sortiert, verschmolzen) und erneut gesucht: nichts geht durch die
//!   Klassen verloren.
//!
//! Vorher (bis 22.11.) gab es nur die eine sortierte Liste: beim Laden einer
//! Wikipedia-Seite im Browser 240 Mio. durchlaufene Knoten (734 je Aufruf),
//! am Board (Liste im PSRAM) über eine Minute nur dafür.
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
/// Ausrichtung aller Blockgrenzen – mindestens 16: Programme belegen über
/// Funktion 68.12 mit Ausrichtung 16; so liegt jeder Nutzzeiger `b + GRAN`
/// von selbst richtig und kleine Blöcke kommen aus ihrer Klasse (auch auf
/// 32 Bit, wo 2 × W nur 8 wäre).
const GRAN: usize = if 2 * W > 16 { 2 * W } else { 16 };
/// Kleinster Block: Kopf + Freilisten-Knoten.
const MIN_BLOCK: usize = 2 * GRAN;
/// Grösste Blockgrösse (samt Kopf) mit eigener Freiliste.
const KLEIN_MAX: usize = 1024;
/// Anzahl Grössenklassen (Index = Blockgrösse / GRAN).
const KLASSEN: usize = KLEIN_MAX / GRAN + 1;

#[repr(C)]
struct Frei {
    groesse: usize,
    next: *mut Frei,
}

/// Freier grosser Block im Treap: Grösse, Kinder (kleinere / grössere
/// Adressen), grösste Grösse im Teilbaum. 4 Wörter = `MIN_BLOCK` auf 64 Bit.
#[repr(C)]
struct Knoten {
    groesse: usize,
    links: *mut Knoten,
    rechts: *mut Knoten,
    max: usize,
}

const _: () = assert!(core::mem::size_of::<Knoten>() <= MIN_BLOCK);

fn prio(k: *mut Knoten) -> u32 {
    ((k as usize as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as u32
}

/// Ein besuchter Knoten (Messung, siehe [`zaehler`]).
fn schritt() {
    SCHRITTE.fetch_add(1, Ordering::Relaxed);
}

/// # Safety
/// `k` null oder ein gültiger Knoten.
unsafe fn max_von(k: *mut Knoten) -> usize {
    if k.is_null() { 0 } else { unsafe { (*k).max } }
}

/// # Safety
/// `k` gültig, Kinder null oder gültig.
unsafe fn neu_rechnen(k: *mut Knoten) {
    unsafe { (*k).max = (*k).groesse.max(max_von((*k).links)).max(max_von((*k).rechts)) };
}

/// Teilt nach Adresse: (Knoten < `a`, Knoten >= `a`).
///
/// # Safety
/// Baum gültig.
unsafe fn teilen(k: *mut Knoten, a: usize) -> (*mut Knoten, *mut Knoten) {
    if k.is_null() {
        return (ptr::null_mut(), ptr::null_mut());
    }
    schritt();
    // SAFETY: laut Vertrag.
    unsafe {
        if (k as usize) < a {
            let (l, r) = teilen((*k).rechts, a);
            (*k).rechts = l;
            neu_rechnen(k);
            (k, r)
        } else {
            let (l, r) = teilen((*k).links, a);
            (*k).links = r;
            neu_rechnen(k);
            (l, k)
        }
    }
}

/// Verbindet zwei Bäume; alle Adressen in `a` liegen vor denen in `b`.
///
/// # Safety
/// Beide Bäume gültig und disjunkt.
unsafe fn verbinden(a: *mut Knoten, b: *mut Knoten) -> *mut Knoten {
    if a.is_null() {
        return b;
    }
    if b.is_null() {
        return a;
    }
    schritt();
    // SAFETY: laut Vertrag.
    unsafe {
        if prio(a) > prio(b) {
            (*a).rechts = verbinden((*a).rechts, b);
            neu_rechnen(a);
            a
        } else {
            (*b).links = verbinden(a, (*b).links);
            neu_rechnen(b);
            b
        }
    }
}

/// Trennt den Knoten mit der höchsten Adresse ab: (Rest, Knoten).
///
/// # Safety
/// Baum gültig, nicht leer.
unsafe fn letzten_abtrennen(k: *mut Knoten) -> (*mut Knoten, *mut Knoten) {
    schritt();
    // SAFETY: laut Vertrag.
    unsafe {
        if (*k).rechts.is_null() {
            let rest = (*k).links;
            (*k).links = ptr::null_mut();
            neu_rechnen(k);
            return (rest, k);
        }
        let (rest, letzter) = letzten_abtrennen((*k).rechts);
        (*k).rechts = rest;
        neu_rechnen(k);
        (k, letzter)
    }
}

/// Trennt den Knoten mit der niedrigsten Adresse ab: (Knoten, Rest).
///
/// # Safety
/// Baum gültig, nicht leer.
unsafe fn ersten_abtrennen(k: *mut Knoten) -> (*mut Knoten, *mut Knoten) {
    schritt();
    // SAFETY: laut Vertrag.
    unsafe {
        if (*k).links.is_null() {
            let rest = (*k).rechts;
            (*k).rechts = ptr::null_mut();
            neu_rechnen(k);
            return (k, rest);
        }
        let (erster, rest) = ersten_abtrennen((*k).links);
        (*k).links = rest;
        neu_rechnen(k);
        (erster, k)
    }
}

/// Knoten mit der niedrigsten Adresse, dessen Grösse mindestens `n` ist.
///
/// # Safety
/// Baum gültig.
unsafe fn finden(mut k: *mut Knoten, n: usize) -> *mut Knoten {
    // SAFETY: laut Vertrag.
    unsafe {
        if max_von(k) < n {
            return ptr::null_mut();
        }
        loop {
            schritt();
            if max_von((*k).links) >= n {
                k = (*k).links;
            } else if (*k).groesse >= n {
                return k;
            } else {
                k = (*k).rechts;
            }
        }
    }
}

/// Knoten im Baum zählen.
///
/// # Safety
/// Baum gültig.
unsafe fn zaehlen_im_baum(k: *mut Knoten) -> usize {
    if k.is_null() { 0 } else { unsafe { 1 + zaehlen_im_baum((*k).links) + zaehlen_im_baum((*k).rechts) } }
}

/// Heap über einem festen Bereich. Thread-sicher (kurze Spin-Sperre).
pub struct Heap {
    arena_start: usize,
    arena_ende: usize,
    /// grosse freie Blöcke: Treap nach Adresse, verschmolzen (M7)
    frei: UnsafeCell<*mut Knoten>,
    /// kleine freie Blöcke je Grösse
    klassen: UnsafeCell<[*mut Frei; KLASSEN]>,
    belegt: AtomicUsize,
    spitze: AtomicUsize,
    sperre: AtomicBool,
    /// Q3: verworfen (Zwangsbeenden, während die Sperre gesetzt war): Belegen
    /// liefert null, Freigeben tut nichts – der Bereich geht als Ganzes an
    /// den Pool zurück, niemand wartet mehr auf die Sperre.
    verworfen: AtomicBool,
    /// Faden (Kontext-Zeiger), der die Sperre zuletzt nahm (Diagnose).
    halter: AtomicUsize,
}

// SAFETY: Zugriff auf `frei` und `klassen` nur unter `sperre`.
unsafe impl Sync for Heap {}
// SAFETY: wie oben.
unsafe impl Send for Heap {}

/// Messung (alle Heaps): Aufrufe von alloc/free und durchlaufene Knoten
/// der Freiliste – zeigt, wie teuer die Suche ist.
static AUFRUFE: AtomicUsize = AtomicUsize::new(0);
static SCHRITTE: AtomicUsize = AtomicUsize::new(0);

/// (Aufrufe, Schritte) seit dem Start bzw. dem letzten Zurücksetzen.
pub fn zaehler(zuruecksetzen: bool) -> (usize, usize) {
    if zuruecksetzen {
        (AUFRUFE.swap(0, Ordering::Relaxed), SCHRITTE.swap(0, Ordering::Relaxed))
    } else {
        (AUFRUFE.load(Ordering::Relaxed), SCHRITTE.load(Ordering::Relaxed))
    }
}

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
        let erster = start as *mut Knoten;
        // SAFETY: Bereich laut Vertrag gültig und exklusiv.
        unsafe {
            (*erster).groesse = ende - start;
            (*erster).links = ptr::null_mut();
            (*erster).rechts = ptr::null_mut();
            (*erster).max = ende - start;
            let h = kopf as *mut Heap;
            h.write(Heap {
                arena_start: start,
                arena_ende: ende,
                frei: UnsafeCell::new(erster),
                klassen: UnsafeCell::new([ptr::null_mut(); KLASSEN]),
                belegt: AtomicUsize::new(0),
                spitze: AtomicUsize::new(0),
                sperre: AtomicBool::new(false),
                verworfen: AtomicBool::new(false),
                halter: AtomicUsize::new(0),
            });
            h
        }
    }

    /// Sperre nehmen; false = Heap verworfen (dann ohne Sperre zurück).
    fn sperren(&self) -> bool {
        let mut runden = 0u32;
        while self
            .sperre
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            if self.verworfen.load(Ordering::Acquire) {
                return false;
            }
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
        if self.verworfen.load(Ordering::Acquire) {
            self.entsperren();
            return false;
        }
        self.halter.store(crate::prozess::faden() as usize, Ordering::Relaxed);
        true
    }

    /// Heap verwerfen (Zwangsbeenden: ein angehaltener Faden kann die
    /// Sperre halten). Danach liefert `alloc` null und `free` tut nichts.
    pub fn verwerfen(&self) {
        self.verworfen.store(true, Ordering::Release);
    }

    /// Wurde der Heap verworfen?
    pub fn ist_verworfen(&self) -> bool {
        self.verworfen.load(Ordering::Acquire)
    }

    /// Kontext des Fadens, der die Sperre zuletzt nahm (0 = keiner).
    pub fn sperre_halter(&self) -> usize {
        self.halter.load(Ordering::Relaxed)
    }

    fn entsperren(&self) {
        self.sperre.store(false, Ordering::Release);
    }

    fn zaehlen(&self, schritte: usize) {
        AUFRUFE.fetch_add(1, Ordering::Relaxed);
        SCHRITTE.fetch_add(schritte, Ordering::Relaxed);
    }

    /// Belegt `groesse` Bytes mit Ausrichtung `align` (Zweierpotenz). Null, wenn voll.
    pub fn alloc(&self, groesse: usize, align: usize) -> *mut u8 {
        let align = align.max(GRAN);
        let groesse = groesse.max(1);
        if !self.sperren() {
            return ptr::null_mut();
        }
        // SAFETY: Freilisten nur unter Sperre, Knoten liegen in der Arena.
        let p = unsafe {
            // kleiner Block ohne besondere Ausrichtung: aus seiner Klasse
            let klein = (align == GRAN)
                .then(|| groesse.checked_add(GRAN - 1).map(|g| GRAN + (g & !(GRAN - 1))))
                .flatten()
                .filter(|&s| s <= KLEIN_MAX);
            let mut p = ptr::null_mut();
            if let Some(s) = klein {
                let liste = &mut (*self.klassen.get())[s / GRAN];
                let blk = *liste;
                if !blk.is_null() {
                    *liste = (*blk).next;
                    let b = blk as usize;
                    *(b as *mut usize) = s;
                    *((b + GRAN - W) as *mut usize) = b;
                    let neu = self.belegt.fetch_add(s, Ordering::Relaxed) + s;
                    self.spitze.fetch_max(neu, Ordering::Relaxed);
                    self.zaehlen(0);
                    p = (b + GRAN) as *mut u8;
                }
            }
            if p.is_null() {
                p = self.gross_suchen(groesse, align);
            }
            if p.is_null() && self.zusammenfuehren() {
                p = self.gross_suchen(groesse, align);
            }
            p
        };
        self.entsperren();
        p
    }

    /// First-Fit nach Adresse im Baum.
    ///
    /// # Safety
    /// Nur unter Sperre.
    unsafe fn gross_suchen(&self, groesse: usize, align: usize) -> *mut u8 {
        // Blockgrösse, die sicher reicht: Kopf, Ausgleich für die Ausrichtung
        // (bei `GRAN` keiner – dann genau dieselbe Wahl wie First-Fit), Nutz-
        // fläche auf `GRAN` gerundet. Überlauf (malloc(-1)) = passt nicht.
        let Some(noetig) = groesse
            .checked_add(GRAN - 1)
            .map(|g| g & !(GRAN - 1))
            .and_then(|g| g.checked_add(GRAN + (align - GRAN)))
        else {
            self.zaehlen(0);
            return ptr::null_mut();
        };
        // SAFETY: laut Vertrag unter Sperre; Knoten in der Arena.
        let p = unsafe {
            let wurzel = self.frei.get();
            let blk = finden(*wurzel, noetig);
            if blk.is_null() {
                ptr::null_mut()
            } else {
                let b = blk as usize;
                let s = (*blk).groesse;
                // aus dem Baum nehmen
                let (l, r) = teilen(*wurzel, b);
                let (_, r) = ersten_abtrennen(r);
                *wurzel = verbinden(l, r);
                let p = (b + GRAN + align - 1) & !(align - 1);
                let ende = (p + groesse + GRAN - 1) & !(GRAN - 1);
                let rest = b + s - ende;
                let nutz = if rest >= MIN_BLOCK && rest <= KLEIN_MAX {
                    // kleiner Rest: in seine Klasse
                    let r = ende as *mut Frei;
                    let liste = &mut (*self.klassen.get())[rest / GRAN];
                    (*r).groesse = rest;
                    (*r).next = *liste;
                    *liste = r;
                    ende - b
                } else if rest >= MIN_BLOCK {
                    self.gross_einfuegen(ende, rest);
                    ende - b
                } else {
                    s
                };
                *(b as *mut usize) = nutz;
                *((p - W) as *mut usize) = b;
                let neu = self.belegt.fetch_add(nutz, Ordering::Relaxed) + nutz;
                self.spitze.fetch_max(neu, Ordering::Relaxed);
                p as *mut u8
            }
        };
        AUFRUFE.fetch_add(1, Ordering::Relaxed);
        p
    }

    /// Block `[b, b + s)` in den Baum, mit den freien Nachbarn verschmolzen.
    ///
    /// # Safety
    /// Nur unter Sperre; der Block ist frei und gehört zur Arena.
    unsafe fn gross_einfuegen(&self, mut b: usize, mut s: usize) {
        // SAFETY: laut Vertrag.
        unsafe {
            let wurzel = self.frei.get();
            let (mut l, mut r) = teilen(*wurzel, b);
            if !l.is_null() {
                let (rest, vor) = letzten_abtrennen(l);
                if vor as usize + (*vor).groesse == b {
                    b = vor as usize;
                    s += (*vor).groesse;
                    l = rest;
                } else {
                    l = verbinden(rest, vor);
                }
            }
            if !r.is_null() {
                let (nach, rest) = ersten_abtrennen(r);
                if b + s == nach as usize {
                    s += (*nach).groesse;
                    r = rest;
                } else {
                    r = verbinden(nach, rest);
                }
            }
            let k = b as *mut Knoten;
            (*k).groesse = s;
            (*k).links = ptr::null_mut();
            (*k).rechts = ptr::null_mut();
            (*k).max = s;
            *wurzel = verbinden(verbinden(l, k), r);
        }
        AUFRUFE.fetch_add(1, Ordering::Relaxed);
    }

    /// Alle kleinen freien Blöcke zurück in den Baum (verschmolzen).
    /// Liefert false, wenn es keine gab.
    ///
    /// # Safety
    /// Nur unter Sperre.
    unsafe fn zusammenfuehren(&self) -> bool {
        let mut gab = false;
        // SAFETY: laut Vertrag unter Sperre; alle Knoten in der Arena.
        unsafe {
            for i in 0..KLASSEN {
                let mut x = core::mem::replace(&mut (*self.klassen.get())[i], ptr::null_mut());
                while !x.is_null() {
                    let n = (*x).next;
                    self.gross_einfuegen(x as usize, (*x).groesse);
                    gab = true;
                    x = n;
                }
            }
        }
        gab
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
        if !self.sperren() {
            return;
        }
        // SAFETY: Freilisten nur unter Sperre.
        unsafe {
            self.belegt.fetch_sub(s, Ordering::Relaxed);
            if s <= KLEIN_MAX {
                let neu = b as *mut Frei;
                let liste = &mut (*self.klassen.get())[s / GRAN];
                (*neu).groesse = s;
                (*neu).next = *liste;
                *liste = neu;
                self.zaehlen(0);
            } else {
                self.gross_einfuegen(b, s);
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

    /// Anzahl freier Blöcke nach dem Zusammenführen der kleinen Blöcke
    /// (Prüfung des Verschmelzens).
    pub fn freie_bloecke(&self) -> usize {
        if !self.sperren() {
            return 0;
        }
        // SAFETY: unter Sperre.
        let n = unsafe {
            self.zusammenfuehren();
            zaehlen_im_baum(*self.frei.get())
        };
        self.entsperren();
        n
    }

    /// Nutzbare Grösse des grössten freien Blocks (nach dem Zusammenführen
    /// der kleinen): passt eine grosse Belegung am Stück? (O1: ein Bild im
    /// Browser bei zerstückeltem Heap – vorher zählte nur der freie Platz
    /// insgesamt, die Belegung scheiterte und beendete das Programm.)
    pub fn groesster_freier_block(&self) -> usize {
        if !self.sperren() {
            return 0;
        }
        // SAFETY: unter Sperre.
        let g = unsafe {
            self.zusammenfuehren();
            max_von(*self.frei.get())
        };
        self.entsperren();
        g.saturating_sub(GRAN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests, die die globalen Zähler (`zaehler`) messen, nacheinander –
    /// sonst zählt der eine die Schritte des anderen mit.
    static ZAEHLER_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

    /// Q3: Ein Heap, dessen Sperre ein (zwangsweise angehaltener) Faden hält,
    /// wird verworfen: wer auf die Sperre wartet, kommt zurück (Belegen null),
    /// Freigeben tut nichts, der Halter wird gemeldet.
    #[test]
    fn verworfener_heap_wartet_nicht() {
        let (_v, h) = bereich(64 * 1024);
        let h: &'static Heap = unsafe { &*h };
        let p = h.alloc(100, 8);
        assert!(!p.is_null());
        // Sperre „hält ein angehaltener Faden“: setzen und nie lösen
        assert!(h.sperren());
        assert!(h.gesperrt());
        let wartet = std::thread::spawn(move || h.alloc(64, 8) as usize);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!wartet.is_finished(), "wartet auf die Sperre");
        h.verwerfen();
        assert_eq!(wartet.join().unwrap(), 0, "nach dem Verwerfen: null statt warten");
        assert!(h.alloc(8, 8).is_null());
        unsafe { h.free(p) }; // kehrt zurück, ohne zu warten
        assert_eq!(h.freie_bloecke(), 0);
        assert_eq!(h.groesster_freier_block(), 0);
        assert!(h.ist_verworfen());
    }

    /// O1: Zerstückelt ist viel frei, aber kein grosser Block am Stück –
    /// `groesster_freier_block` sagt, was wirklich noch passt.
    #[test]
    fn groesster_freier_block_bei_zerstueckelung() {
        let (_v, h) = bereich(256 * 1024);
        let h = unsafe { &*h };
        let leer = h.groesster_freier_block();
        assert!(leer > 250 * 1024, "leer fast alles am Stück: {leer}");
        let bloecke: Vec<*mut u8> = (0..60).map(|_| h.alloc(4000, 16)).collect();
        assert!(bloecke.iter().all(|p| !p.is_null()));
        // jeder zweite frei: ~120 KB frei, aber nur in 4-KB-Stücken (plus Rest am Ende)
        for p in bloecke.iter().step_by(2) {
            unsafe { h.free(*p) };
        }
        let g = h.groesster_freier_block();
        let p = h.alloc(g, 16);
        assert!(!p.is_null(), "der gemeldete grösste Block lässt sich belegen ({g})");
        unsafe { h.free(p) };
        assert!(h.alloc(g + 64, 16).is_null(), "etwas Grösseres nicht");
        let frei = h.groesse() - h.belegt();
        assert!(frei > g + 100 * 1024, "insgesamt viel mehr frei ({frei}) als am Stück ({g})");
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

    /// Kleine Blöcke kommen ohne Suche aus ihrer Klasse; gleich grosse
    /// Blöcke werden wiederverwendet.
    #[test]
    fn kleine_bloecke_ohne_suche() {
        let _z = ZAEHLER_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let (_v, h) = bereich(1024 * 1024);
        let h = unsafe { &*h };
        // Blöcke verschiedener Grösse freigeben (Lücken), dann gleich
        // grosse wieder belegen: dieselben Adressen, keine Schritte
        let z: Vec<_> = (0..2000).map(|i| h.alloc(16 + (i % 20) * 8, 8)).collect();
        for &p in z.iter().step_by(2) {
            unsafe { h.free(p) };
        }
        zaehler(true);
        let mut wieder = Vec::new();
        for i in (0..2000).step_by(2) {
            let p = h.alloc(16 + (i % 20) * 8, 8);
            assert!(z.contains(&p), "Block {i} wiederverwendet");
            wieder.push(p);
        }
        let (aufrufe, schritte) = zaehler(false);
        assert!(aufrufe >= 1000);
        // andere Heaps (parallele Tests) zählen mit: nur grobe Grenze
        assert!(schritte < aufrufe, "Schritte {schritte} bei {aufrufe} Aufrufen");
    }

    /// Grosser Block passt erst, wenn die kleinen freien Blöcke wieder
    /// verschmolzen sind.
    #[test]
    fn gross_nach_vielen_kleinen() {
        let (_v, h) = bereich(64 * 1024);
        let h = unsafe { &*h };
        let mut z = Vec::new();
        loop {
            let p = h.alloc(40, 8);
            if p.is_null() {
                break;
            }
            z.push(p);
        }
        assert!(h.alloc(8 * 1024, 8).is_null(), "voll");
        for &p in &z {
            unsafe { h.free(p) };
        }
        // alles klein freigegeben: grosser Block erst nach dem Zusammenführen
        let g = h.alloc(32 * 1024, 8);
        assert!(!g.is_null(), "grosser Block nach dem Zusammenführen");
        unsafe { h.free(g) };
        assert_eq!(h.belegt(), 0);
        assert_eq!(h.freie_bloecke(), 1);
    }

    /// M7: browserähnliche Last – viele lebende Blöcke gemischter Grösse
    /// (16 B bis 64 KB, meist klein), zufällig belegt und freigegeben, Heap
    /// fast voll (Zerstückelung). Misst die durchlaufenen Knoten je Aufruf
    /// (`zaehler`); Grenze siehe M7 in PLAN.
    #[test]
    fn zerstueckelt_ohne_lange_suche() {
        let _z = ZAEHLER_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let (_v, h) = bereich(8 * 1024 * 1024);
        let h = unsafe { &*h };
        let mut zufall = 0x1234_5678u32;
        let mut naechste = || {
            zufall ^= zufall << 13;
            zufall ^= zufall >> 17;
            zufall ^= zufall << 5;
            zufall
        };
        let mut lebend: Vec<*mut u8> = Vec::new();
        // 1. füllen: meist kleine Blöcke (Knoten, Texte), dazwischen 1–8 KB
        loop {
            let r = naechste();
            let g = if r % 8 == 0 { 1024 + (r >> 8) as usize % 7168 } else { 24 + (r >> 8) as usize % 400 };
            let p = h.alloc(g, 8);
            if p.is_null() {
                break;
            }
            lebend.push(p);
        }
        // 2. die Hälfte verstreut freigeben (Seite verlassen, Teile bleiben)
        for i in (0..lebend.len()).rev() {
            if naechste() % 2 == 0 {
                unsafe { h.free(lebend.swap_remove(i)) };
            }
        }
        let _ = zaehler(true);
        // 3. grosse Blöcke (Bilder), dann gemischt weiter
        let mut fehl = 0usize;
        for k in 0..60_000usize {
            let r = naechste();
            if k % 50 == 0 {
                let p = h.alloc(16 * 1024 + (r as usize % (48 * 1024)), 8);
                if p.is_null() { fehl += 1 } else { lebend.push(p) }
            } else if r % 2 == 0 && !lebend.is_empty() {
                let i = naechste() as usize % lebend.len();
                unsafe { h.free(lebend.swap_remove(i)) };
            } else {
                let g = if r % 8 == 0 { 1024 + (r >> 8) as usize % 7168 } else { 24 + (r >> 8) as usize % 400 };
                let p = h.alloc(g, 8);
                if p.is_null() { fehl += 1 } else { lebend.push(p) }
            }
        }
        let (aufrufe, schritte) = zaehler(false);
        let je = schritte as f64 / aufrufe.max(1) as f64;
        println!("M7: {aufrufe} Aufrufe, {schritte} Knoten ({je:.1} je Aufruf), {fehl} fehlgeschlagen, {} lebend", lebend.len());
        for p in lebend {
            unsafe { h.free(p) };
        }
        assert_eq!(h.belegt(), 0);
        assert!(je < 50.0, "Suche zu lang: {je:.1} Knoten je Aufruf");
    }
}
