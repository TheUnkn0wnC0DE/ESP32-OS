//! A6: Zwangsbeenden (`kern::zwangsbeenden`). Am PC lassen sich Threads
//! nicht von aussen anhalten; die Test-Plattform bildet `vTaskSuspend` nach:
//! Ein angehaltener Faden steht – in seiner Schleife (Programmcode) oder dort,
//! wo er gerade wartet. Geprüft wird:
//! - kooperativ endendes Programm: `Beendet`, ohne Anhalten;
//! - ein Faden im Kern-Abschnitt und einer im geschützten Abschnitt
//!   (Treiber, Sockets) wird gar nicht angehalten (könnte Sperren halten),
//!   nur immer wieder geprüft;
//! - Endlosschleife ohne Systemaufruf (belegt und gibt ständig frei):
//!   angehalten, Fenster weg, Prozess aufgeräumt (Heap-Region frei,
//!   Protokoll „Beendet“), System läuft weiter;
//! - `pruefe_ende` beendet nicht im geschützten Abschnitt, erst danach;
//! - unbekannte PID.

use kern::{Ende, KernAlloc, Plattform, ProgrammEintrag, Zwang, sys};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

// ---- Test-Plattform: PC + Anhalten wie FreeRTOS
static HALT: AtomicBool = AtomicBool::new(false);
static PARKT: AtomicBool = AtomicBool::new(false);
static HALT_GRIFF: AtomicUsize = AtomicUsize::new(usize::MAX);
static ANHALTEN: AtomicUsize = AtomicUsize::new(0);

fn anhalten(g: usize) -> bool {
    ANHALTEN.fetch_add(1, Ordering::SeqCst);
    HALT_GRIFF.store(g, Ordering::SeqCst);
    HALT.store(true, Ordering::SeqCst);
    // steht, sobald er an der Schleife ankommt
    let t = Instant::now();
    while !PARKT.load(Ordering::SeqCst) {
        if t.elapsed() > Duration::from_secs(2) {
            HALT.store(false, Ordering::SeqCst);
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    true
}

fn fortsetzen(_g: usize) {
    HALT.store(false, Ordering::SeqCst);
}

fn weg(g: usize) -> bool {
    (HALT.load(Ordering::SeqCst) && HALT_GRIFF.load(Ordering::SeqCst) == g) || (kern::PC.task_weg)(g)
}

fn freigeben(g: usize) {
    // angehaltener Thread: steht für immer (wie ein gelöschter Task)
    if !(HALT.load(Ordering::SeqCst) && HALT_GRIFF.load(Ordering::SeqCst) == g) {
        (kern::PC.task_freigeben)(g);
    }
}

// ---- Programme
static PHASE: AtomicUsize = AtomicUsize::new(0);
static LOS: AtomicUsize = AtomicUsize::new(0);
static ZAEHLER: AtomicU64 = AtomicU64::new(0);

/// Wartet erst im Kern (bis LOS ≥ 1), dann geschützt (bis LOS ≥ 2), dann
/// Endlosschleife ohne Systemaufruf, die ständig belegt und freigibt.
fn haenger() {
    sys::fenster(10 << 16 | 199, 10 << 16 | 99, 0x33 << 24 | 0xFFFFFF, 0, Some("Haenger"));
    kern::kern_abschnitt(|| {
        PHASE.store(1, Ordering::SeqCst);
        while LOS.load(Ordering::SeqCst) < 1 {
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    kern::geschuetzt(|| {
        PHASE.store(2, Ordering::SeqCst);
        while LOS.load(Ordering::SeqCst) < 2 {
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    PHASE.store(3, Ordering::SeqCst);
    let mut v: Vec<u64> = Vec::new();
    loop {
        if HALT.load(Ordering::SeqCst) {
            PARKT.store(true, Ordering::SeqCst);
            while HALT.load(Ordering::SeqCst) {
                core::hint::spin_loop();
            }
            PARKT.store(false, Ordering::SeqCst);
        }
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        v.push(n);
        if v.len() >= 64 {
            v = Vec::new();
        }
    }
}

/// Wartet auf Ereignisse (endet kooperativ).
fn brav() {
    sys::fenster(10 << 16 | 199, 150 << 16 | 99, 0x33 << 24 | 0xFFFFFF, 0, Some("Brav"));
    loop {
        sys::warte_ereignis();
    }
}

static KURZ_OK: AtomicBool = AtomicBool::new(false);
fn kurz() {
    let v = vec![5u8; 1000];
    KURZ_OK.store(v.iter().all(|&b| b == 5), Ordering::SeqCst);
}

/// pruefe_ende im geschützten Abschnitt: endet dort nicht, erst danach.
static GESCH_LAEUFT: AtomicBool = AtomicBool::new(false);
static GESCH_FERTIG: AtomicBool = AtomicBool::new(false);
static GESCH_DANACH: AtomicBool = AtomicBool::new(false);
static GESCH_LOS: AtomicBool = AtomicBool::new(false);
fn geschuetzt_ende() {
    kern::geschuetzt(|| {
        GESCH_LAEUFT.store(true, Ordering::SeqCst);
        while !GESCH_LOS.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        kern::pruefe_ende();
        GESCH_FERTIG.store(true, Ordering::SeqCst);
    });
    kern::pruefe_ende();
    GESCH_DANACH.store(true, Ordering::SeqCst);
}

static PROGRAMME: [ProgrammEintrag; 4] = [
    ProgrammEintrag { name: "haenger", heap_kb: 256, stapel_kb: 64, start: haenger },
    ProgrammEintrag { name: "brav", heap_kb: 256, stapel_kb: 64, start: brav },
    ProgrammEintrag { name: "kurz", heap_kb: 128, stapel_kb: 64, start: kurz },
    ProgrammEintrag { name: "geschuetzt", heap_kb: 128, stapel_kb: 64, start: geschuetzt_ende },
];

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn fenster(titel: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel == titel)
}

fn laeuft(pid: u32) -> bool {
    kern::liste().iter().any(|p| p.pid == pid)
}

#[test]
fn zwangsbeenden() {
    let pl: &'static Plattform = Box::leak(Box::new(Plattform {
        task_anhalten: Some(anhalten),
        task_fortsetzen: fortsetzen,
        task_weg: weg,
        task_freigeben: freigeben,
        ..kern::PC
    }));
    kern::init(pl, &PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    let regionen_vorher = kern::regionen();

    // Unbekannte PID
    assert_eq!(kern::zwangsbeenden(99_999, 100), Zwang::UnbekanntePid);

    // Kooperativ: wartet auf Ereignisse → endet ohne Anhalten
    let pid = kern::starte_name("brav").unwrap();
    bis(|| fenster("Brav"), "Fenster Brav");
    let t = Instant::now();
    assert_eq!(kern::zwangsbeenden(pid, 2000), Zwang::Beendet);
    assert!(t.elapsed() < Duration::from_millis(1500), "kooperativ schnell");
    bis(|| !laeuft(pid) && !fenster("Brav"), "Brav aufgeräumt");
    // Protokoll wird kurz nach dem Austragen geschrieben
    bis(|| kern::ende_von(pid) == Some(Ende::Beendet), "Protokoll Beendet");
    assert_eq!(ANHALTEN.load(Ordering::SeqCst), 0, "kooperativ: nie angehalten");

    // pruefe_ende im geschützten Abschnitt beendet nicht, erst danach
    let pid = kern::starte_name("geschuetzt").unwrap();
    bis(|| GESCH_LAEUFT.load(Ordering::SeqCst), "geschützt läuft");
    assert!(kern::beende(pid));
    GESCH_LOS.store(true, Ordering::SeqCst);
    bis(|| GESCH_FERTIG.load(Ordering::SeqCst), "geschützter Abschnitt zu Ende");
    bis(|| !laeuft(pid), "danach beendet");
    assert!(!GESCH_DANACH.load(Ordering::SeqCst), "pruefe_ende nach dem Abschnitt beendet");
    bis(|| kern::ende_von(pid) == Some(Ende::Beendet), "Protokoll geschützt");

    // Hänger: steckt zuerst im Kern, dann geschützt → nie angehalten
    let pid = kern::starte_name("haenger").unwrap();
    bis(|| fenster("Haenger") && PHASE.load(Ordering::SeqCst) == 1, "Hänger im Kern");
    let z = std::thread::spawn(move || kern::zwangsbeenden(pid, 200));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(ANHALTEN.load(Ordering::SeqCst), 0, "im Kern nicht angehalten");
    assert!(laeuft(pid) && fenster("Haenger"), "im Kern nicht ausgetragen");
    LOS.store(1, Ordering::SeqCst);
    bis(|| PHASE.load(Ordering::SeqCst) == 2, "Hänger geschützt");
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(ANHALTEN.load(Ordering::SeqCst), 0, "geschützt nicht angehalten");
    assert!(laeuft(pid) && fenster("Haenger"), "geschützt nicht ausgetragen");
    // Abschnitt verlassen → Endlosschleife ohne Systemaufruf → angehalten
    LOS.store(2, Ordering::SeqCst);
    assert_eq!(z.join().unwrap(), Zwang::Angehalten);
    assert!(ANHALTEN.load(Ordering::SeqCst) >= 1);
    assert!(PARKT.load(Ordering::SeqCst), "Faden steht");
    let n = ZAEHLER.load(Ordering::SeqCst);
    assert!(n > 0, "lief in der Schleife");
    assert!(!fenster("Haenger"), "Fenster sofort weg");
    bis(|| !laeuft(pid), "Prozess aufgeräumt");
    bis(|| kern::ende_von(pid) == Some(Ende::Beendet), "Protokoll Beendet");
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(ZAEHLER.load(Ordering::SeqCst), n, "angehaltener Faden läuft nicht weiter");
    bis(|| kern::regionen() == regionen_vorher, "Heap-Region frei");
    // ein zweiter Aufruf findet nichts mehr
    assert_eq!(kern::zwangsbeenden(pid, 100), Zwang::UnbekanntePid);

    // System läuft weiter: neues Programm startet und endet normal
    let pid = kern::starte_name("kurz").unwrap();
    bis(|| !laeuft(pid), "kurz endet");
    assert!(KURZ_OK.load(Ordering::SeqCst));
    bis(|| kern::ende_von(pid) == Some(Ende::Normal), "Protokoll Normal");
}
