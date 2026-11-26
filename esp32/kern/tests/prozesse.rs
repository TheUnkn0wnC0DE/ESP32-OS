//! K1-Nachweis: Programme starten, beenden, abstürzen, Heap wieder frei.

use kern::{Ende, KernAlloc, ProgrammEintrag};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Tests laufen parallel, teilen aber Kern und Regionstabelle.
static SERIELL: Mutex<()> = Mutex::new(());

static ADRESSE: AtomicUsize = AtomicUsize::new(0);
static BELEGT_INNEN: AtomicUsize = AtomicUsize::new(0);
static LAEUFT: AtomicBool = AtomicBool::new(false);
static FADEN_OK: AtomicBool = AtomicBool::new(false);

fn normal() {
    let v = vec![7u8; 100 * 1024];
    ADRESSE.store(v.as_ptr() as usize, Ordering::SeqCst);
    let me = kern::eigene_pid().unwrap();
    let info = kern::liste().into_iter().find(|p| p.pid == me).unwrap();
    BELEGT_INNEN.store(info.heap_belegt, Ordering::SeqCst);
    // absichtliches Leck: wird mit dem Bereich freigegeben
    std::mem::forget(vec![1u8; 4096]);
}

fn absturz() {
    let _v = vec![0u8; 1000];
    panic!("Absicht");
}

fn endlos() {
    LAEUFT.store(true, Ordering::SeqCst);
    loop {
        kern::pruefe_ende();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn gierig() {
    let mut v: Vec<Vec<u8>> = Vec::new();
    loop {
        v.push(vec![0u8; 64 * 1024]);
        kern::pruefe_ende();
    }
}

fn selbst_ende() {
    kern::beende_selbst(Ende::Normal);
}

fn mit_faden() {
    let fertig = std::sync::Arc::new(AtomicBool::new(false));
    let f2 = fertig.clone();
    kern::faden_starten(64, Box::new(move || {
        let v = vec![3u8; 10_000];
        FADEN_OK.store(v.iter().all(|&b| b == 3), Ordering::SeqCst);
        f2.store(true, Ordering::SeqCst);
    }))
    .unwrap();
    while !fertig.load(Ordering::SeqCst) {
        kern::pruefe_ende();
        std::thread::sleep(Duration::from_millis(1));
    }
}

static NEBEN_OK: AtomicBool = AtomicBool::new(false);

/// 30 Nebenfäden nacheinander; jeder wird abgeräumt, während das Programm läuft.
fn viele_faeden() {
    let me = kern::eigene_pid().unwrap();
    for _ in 0..30 {
        let fertig = std::sync::Arc::new(AtomicBool::new(false));
        let f2 = fertig.clone();
        kern::faden_starten(64, Box::new(move || f2.store(true, Ordering::SeqCst))).unwrap();
        while !fertig.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    std::thread::sleep(Duration::from_millis(200));
    let info = kern::liste().into_iter().find(|p| p.pid == me).unwrap();
    NEBEN_OK.store(info.faeden == 1, Ordering::SeqCst);
}

static PROGRAMME: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "vielefaeden", heap_kb: 256, stapel_kb: 64, start: viele_faeden },
    ProgrammEintrag { name: "normal", heap_kb: 512, stapel_kb: 64, start: normal },
    ProgrammEintrag { name: "absturz", heap_kb: 64, stapel_kb: 64, start: absturz },
    ProgrammEintrag { name: "endlos", heap_kb: 64, stapel_kb: 64, start: endlos },
    ProgrammEintrag { name: "gierig", heap_kb: 256, stapel_kb: 64, start: gierig },
    ProgrammEintrag { name: "selbst", heap_kb: 64, stapel_kb: 64, start: selbst_ende },
    ProgrammEintrag { name: "faden", heap_kb: 256, stapel_kb: 64, start: mit_faden },
    ProgrammEintrag { name: "@dienst", heap_kb: 64, stapel_kb: 64, start: selbst_ende },
];

fn init() -> std::sync::MutexGuard<'static, ()> {
    let g = SERIELL.lock().unwrap_or_else(|e| e.into_inner());
    kern::init(&kern::PC, PROGRAMME);
    g
}

fn warte_ende(pid: u32) -> Ende {
    let t = Instant::now();
    loop {
        kern::aufraeumen();
        if let Some(e) = kern::ende_von(pid) {
            return e;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "Programm {pid} endet nicht");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn nichts_mehr_offen() {
    let t = Instant::now();
    while !kern::liste().is_empty() || kern::regionen() != 0 || kern::notbelegungen() != 0 {
        kern::aufraeumen();
        assert!(t.elapsed() < Duration::from_secs(10), "Reste: {:?}", kern::liste());
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// M8: unter `START_RESERVE` freiem System-Heap startet kein Programm
/// (Fehler „kein Speicher“), Systemdienste „@…“ starten weiter.
#[test]
fn kein_start_bei_knappem_system_heap() {
    let _g = init();
    kern::grafik::TEST_SYSTEM_FREI.store(kern::prozess::START_RESERVE - 100 * 1024, Ordering::SeqCst);
    let r = kern::starte_name("normal");
    let d = kern::starte_name("@dienst");
    kern::grafik::TEST_SYSTEM_FREI.store(usize::MAX, Ordering::SeqCst);
    assert!(matches!(r, Err(kern::StartFehler::KeinSpeicher)), "{r:?}");
    let d = d.expect("Systemdienst startet");
    assert_eq!(warte_ende(d), Ende::Normal);
    let pid = kern::starte_name("normal").expect("wieder genug Speicher");
    assert_eq!(warte_ende(pid), Ende::Normal);
    nichts_mehr_offen();
}

#[test]
fn normal_eigener_heap_und_frei() {
    let _g = init();
    let pid = kern::starte_name("normal").unwrap();
    assert_eq!(warte_ende(pid), Ende::Normal);
    assert!(BELEGT_INNEN.load(Ordering::SeqCst) >= 100 * 1024, "Belegung nicht im Programm-Heap");
    nichts_mehr_offen();
    // Adresse lag in einem Bereich, der jetzt nicht mehr eingetragen ist
    assert_ne!(ADRESSE.load(Ordering::SeqCst), 0);
}

#[test]
fn absturz_reisst_system_nicht_mit() {
    let _g = init();
    let pid = kern::starte_name("absturz").unwrap();
    match warte_ende(pid) {
        Ende::Absturz(m) => assert!(m.contains("Absicht"), "{m}"),
        e => panic!("falsches Ende {e:?}"),
    }
    nichts_mehr_offen();
    let pid2 = kern::starte_name("normal").unwrap();
    assert_eq!(warte_ende(pid2), Ende::Normal);
    nichts_mehr_offen();
}

#[test]
fn beenden_von_aussen() {
    let _g = init();
    LAEUFT.store(false, Ordering::SeqCst);
    let pid = kern::starte_name("endlos").unwrap();
    while !LAEUFT.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(kern::liste().iter().any(|p| p.pid == pid));
    assert!(kern::beende(pid));
    assert_eq!(warte_ende(pid), Ende::Beendet);
    nichts_mehr_offen();
}

#[test]
fn speicher_voll_beendet_nur_programm() {
    let _g = init();
    let pid = kern::starte_name("gierig").unwrap();
    assert_eq!(warte_ende(pid), Ende::SpeicherVoll);
    nichts_mehr_offen();
}

#[test]
fn systemfunktion_minus_eins() {
    let _g = init();
    let pid = kern::starte_name("selbst").unwrap();
    assert_eq!(warte_ende(pid), Ende::Normal);
    nichts_mehr_offen();
}

#[test]
fn zweiter_faden_teilt_heap() {
    let _g = init();
    FADEN_OK.store(false, Ordering::SeqCst);
    let pid = kern::starte_name("faden").unwrap();
    assert_eq!(warte_ende(pid), Ende::Normal);
    assert!(FADEN_OK.load(Ordering::SeqCst));
    nichts_mehr_offen();
}

#[test]
fn nebenfaeden_werden_einzeln_abgeraeumt() {
    let _g = init();
    let pid = kern::starte_name("vielefaeden").unwrap();
    assert_eq!(warte_ende(pid), Ende::Normal);
    assert!(NEBEN_OK.load(Ordering::SeqCst), "beendete Nebenfäden noch gezählt");
    nichts_mehr_offen();
}

#[test]
fn viele_starts_ohne_leck() {
    let _g = init();
    for _ in 0..40 {
        let pid = kern::starte_name("normal").unwrap();
        assert_eq!(warte_ende(pid), Ende::Normal);
    }
    nichts_mehr_offen();
    assert_eq!(kern::starte_name("gibtsnicht"), Err(kern::StartFehler::Unbekannt));
}
