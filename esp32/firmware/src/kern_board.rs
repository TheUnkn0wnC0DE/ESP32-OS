//! ESP32-OS: Board-Plattform des Kerns (FreeRTOS).
//!
//! * Faden-Kontext im FreeRTOS-TLS-Zeiger `TLS_INDEX` (pthread benutzt andere).
//! * Programm-Tasks statisch erzeugt: Stapel im Heap des Programms (PSRAM),
//!   Task-Kopf (TCB) im internen RAM. Ein Faden endet, indem er sich selbst
//!   anhält (`vTaskSuspend(NULL)`); der Aufräumer löscht den angehaltenen
//!   Task (`vTaskDelete` – sofort, weil er nicht läuft) und gibt danach TCB
//!   und Stapel frei. Kein Warten auf die Idle-Tasks.
//! * Programm-Bereiche aus dem PSRAM.

use core::ffi::c_void;
use esp_idf_svc::sys;
use kern::heap::Heap;
use kern::{Faden, Lauf, Plattform};

/// FreeRTOS-TLS-Platz für den Faden-Kontext (sdkconfig: 3 Plätze).
const TLS_INDEX: i32 = 1;
/// Programme unter der Anzeige-/Eingabeschleife (5): ein rechnendes
/// Programm darf die Bedienung nie blockieren.
const PRIO: u32 = 3;

struct Griff {
    handle: sys::TaskHandle_t,
    tcb: *mut sys::StaticTask_t,
    stapel: *mut u8,
    heap: *const Heap,
}

fn lesen() -> *mut Faden {
    // SAFETY: TLS des laufenden Tasks.
    unsafe { sys::pvTaskGetThreadLocalStoragePointer(core::ptr::null_mut(), TLS_INDEX).cast() }
}

fn setzen(f: *mut Faden) {
    // SAFETY: TLS des laufenden Tasks.
    unsafe { sys::vTaskSetThreadLocalStoragePointer(core::ptr::null_mut(), TLS_INDEX, f.cast()) }
}

extern "C" fn trampolin(arg: *mut c_void) {
    // SAFETY: arg stammt aus `starten` (Box<Box<dyn FnOnce>>).
    let f = unsafe { Box::from_raw(arg.cast::<Box<dyn FnOnce() + Send>>()) };
    f();
    beenden();
}

fn starten(name: &str, stapel: usize, heap: &Heap, f: Box<dyn FnOnce() + Send>) -> Option<usize> {
    let stapel = stapel.max(8 * 1024) & !15;
    let st = heap.alloc(stapel, 16);
    if st.is_null() {
        return None;
    }
    // SAFETY: TCB muss intern liegen; Grösse aus den Bindungen.
    let tcb = unsafe {
        sys::heap_caps_malloc(
            core::mem::size_of::<sys::StaticTask_t>(),
            sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT,
        )
    }
    .cast::<sys::StaticTask_t>();
    if tcb.is_null() {
        // SAFETY: st stammt aus diesem Heap.
        unsafe { heap.free(st) };
        return None;
    }
    let mut cname = [0u8; 16];
    for (d, s) in cname.iter_mut().zip(name.bytes().take(15)) {
        *d = s;
    }
    let arg = Box::into_raw(Box::new(f)).cast::<c_void>();
    // SAFETY: Stapel und TCB gehören dem Task, bis er gelöscht und abgeräumt ist.
    let handle = unsafe {
        sys::xTaskCreateStaticPinnedToCore(
            Some(trampolin),
            cname.as_ptr().cast(),
            stapel as u32,
            arg,
            PRIO,
            st.cast(),
            tcb,
            i32::MAX, // tskNO_AFFINITY (Makro, nicht in den Bindungen)
        )
    };
    if handle.is_null() {
        // SAFETY: Task wurde nicht erzeugt, alles wieder frei.
        unsafe {
            drop(Box::from_raw(arg.cast::<Box<dyn FnOnce() + Send>>()));
            sys::heap_caps_free(tcb.cast());
            heap.free(st);
        }
        return None;
    }
    let g = Box::new(Griff { handle, tcb, stapel: st, heap: heap as *const Heap });
    Some(Box::into_raw(g) as usize)
}

fn ausfuehren(f: Box<dyn FnOnce() + Send>) -> Lauf {
    f();
    Lauf::Normal
}

fn beenden() -> ! {
    // Anhalten statt selbst löschen: der Aufräumer löscht den Task, sobald er
    // angehalten ist, und gibt dann TCB und Stapel frei.
    loop {
        // SAFETY: hält den laufenden Task an.
        unsafe { sys::vTaskSuspend(core::ptr::null_mut()) };
    }
}

fn weg(g: usize) -> bool {
    // SAFETY: Griff lebt bis `freigeben`; nur der Aufräumer fragt. Gefragt
    // wird erst, wenn der Faden sich beim Kern abgemeldet hat – angehalten
    // heisst dann: er steht in `beenden`.
    let g = unsafe { &*(g as *const Griff) };
    // SAFETY: TCB gehört uns (statisch), bleibt gültig.
    unsafe { sys::eTaskGetState(g.handle) == sys::eTaskState_eSuspended }
}

fn freigeben(g: usize) {
    // SAFETY: Griff aus `starten`, Task angehalten.
    let g = unsafe { Box::from_raw(g as *mut Griff) };
    // SAFETY: Der Task läuft nicht → vTaskDelete räumt sofort ab (TLS-
    // Rückrufe in diesem Faden); danach sind TCB und Stapel frei. Der Heap
    // des Programms lebt noch (Bereich wird danach freigegeben).
    unsafe {
        sys::vTaskDelete(g.handle);
        sys::heap_caps_free(g.tcb.cast());
        (*g.heap).free(g.stapel);
    }
}

/// Fremden Programm-Task anhalten (Zwangsbeenden, A6).
fn anhalten(g: usize) -> bool {
    // SAFETY: Griff lebt bis `freigeben` (nur der Kern ruft, unter seiner
    // Prozesstabelle).
    let g = unsafe { &*(g as *const Griff) };
    // SAFETY: gültiger Task; vTaskSuspend auf einen anderen Task ist erlaubt.
    // Läuft er gerade auf dem anderen Kern, nimmt vTaskSuspend ihn nur aus
    // der Bereitliste und stösst den Kern an – er rechnet noch, bis dieser
    // wechselt. Erst wenn er angehalten ist UND auf keinem Kern mehr läuft,
    // steht er (angehalten kann er nicht wieder an die Reihe kommen); erst
    // dann darf der Kern seinen Zustand (`im_kern`) lesen.
    unsafe {
        sys::vTaskSuspend(g.handle);
        for _ in 0..50 {
            let laeuft = (0..2).any(|k| sys::xTaskGetCurrentTaskHandleForCore(k) == g.handle);
            if !laeuft && sys::eTaskGetState(g.handle) == sys::eTaskState_eSuspended {
                return true;
            }
            sys::vTaskDelay(1);
        }
        // Steht nach 50 ms noch nicht: weiterlaufen lassen, nicht anhaltbar
        sys::vTaskResume(g.handle);
        false
    }
}

fn fortsetzen(g: usize) {
    // SAFETY: wie `anhalten`.
    let g = unsafe { &*(g as *const Griff) };
    // SAFETY: gültiger, angehaltener Task.
    unsafe { sys::vTaskResume(g.handle) };
}

fn holen(n: usize) -> *mut u8 {
    // SAFETY: einfache Belegung im PSRAM.
    unsafe { sys::heap_caps_malloc(n, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) }.cast()
}

fn frei(p: *mut u8, _n: usize) {
    // SAFETY: p aus `holen`.
    unsafe { sys::heap_caps_free(p.cast()) }
}

/// Plattform des Boards.
pub static BOARD: Plattform = Plattform {
    faden_lesen: lesen,
    faden_setzen: setzen,
    task_starten: starten,
    ausfuehren,
    task_beenden: beenden,
    task_weg: weg,
    task_freigeben: freigeben,
    task_anhalten: Some(anhalten),
    task_fortsetzen: fortsetzen,
    bereich_holen: holen,
    bereich_frei: frei,
    panik_beendet_task: true,
    system_frei,
    intern_frei,
};

fn intern_frei() -> usize {
    // SAFETY: reine Abfrage der Heap-Verwaltung.
    unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) }
}

/// D5: vom Ausnahme-Handler (absturz.c) gefragt – darf der Fehler nur das
/// Programm beenden? Im Handler: nur Atomics (siehe kern::hw_fehler_abfangen).
#[unsafe(no_mangle)]
pub extern "C" fn esp32os_programm_fehler_abfangen(ursache: u32, pc: u32, adresse: u32) -> bool {
    kern::hw_fehler_abfangen(ursache, pc, adresse)
}

/// D5: Ziel der Umleitung – läuft wieder im Faden des Programms.
#[unsafe(no_mangle)]
pub extern "C" fn esp32os_programm_fehler_ende() -> ! {
    // Ausgabe im Kern-Abschnitt (System-Heap), wie der Panik-Hook
    kern::kern_abschnitt(|| log::error!("Hardware-Fehler in Programm {:?} – nur dieses Programm wird beendet", kern::eigene_pid()));
    kern::hw_fehler_beenden()
}

fn system_frei() -> usize {
    // SAFETY: reine Abfrage der Heap-Verwaltung.
    unsafe { sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) }
}

/// Selbsttest K1 am Board (Build mit `KERN_SELBSTTEST=1`): startet Programme,
/// die normal enden, abstürzen, beendet werden und den Heap sprengen, und
/// meldet das Ergebnis im Log.
pub mod selbsttest {
    use core::sync::atomic::{AtomicBool, Ordering};
    use kern::{Ende, ProgrammEintrag};
    use std::time::{Duration, Instant};

    static LAEUFT: AtomicBool = AtomicBool::new(false);

    fn normal() {
        let v = vec![7u8; 100 * 1024];
        core::hint::black_box(&v);
    }
    fn absturz() {
        panic!("Absicht");
    }
    fn endlos() {
        LAEUFT.store(true, Ordering::SeqCst);
        loop {
            kern::pruefe_ende();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn gierig() {
        let mut v: Vec<Vec<u8>> = Vec::new();
        loop {
            v.push(vec![0u8; 32 * 1024]);
            kern::pruefe_ende();
        }
    }

    /// 20 Nebenfäden à 16 KB Stapel nacheinander in 256 KB Heap: geht nur,
    /// wenn beendete Fäden ihren Stapel sofort zurückgeben.
    fn faeden() {
        for _ in 0..20 {
            let fertig = std::sync::Arc::new(AtomicBool::new(false));
            let f2 = fertig.clone();
            if kern::faden_starten(16, Box::new(move || f2.store(true, Ordering::SeqCst))).is_none() {
                panic!("Faden startet nicht");
            }
            while !fertig.load(Ordering::SeqCst) {
                kern::pruefe_ende();
                std::thread::sleep(Duration::from_millis(2));
            }
            std::thread::sleep(Duration::from_millis(60));
            kern::pruefe_ende();
        }
    }

    pub static PROGRAMME: &[ProgrammEintrag] = &[
        ProgrammEintrag { name: "faeden", heap_kb: 256, stapel_kb: 16, start: faeden },
        ProgrammEintrag { name: "normal", heap_kb: 256, stapel_kb: 16, start: normal },
        ProgrammEintrag { name: "absturz", heap_kb: 32, stapel_kb: 16, start: absturz },
        ProgrammEintrag { name: "endlos", heap_kb: 32, stapel_kb: 16, start: endlos },
        ProgrammEintrag { name: "gierig", heap_kb: 256, stapel_kb: 16, start: gierig },
    ];

    fn warte(pid: u32) -> Option<Ende> {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) {
            if let Some(e) = kern::ende_von(pid) {
                return Some(e);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    fn psram_frei() -> usize {
        // SAFETY: reine Abfrage.
        unsafe { esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM) }
    }

    pub fn laufen() {
        let vorher = psram_frei();
        let mut nach_runde1 = vorher;
        let mut ok = true;
        for runde in 0..10 {
            for name in ["normal", "absturz", "gierig", "endlos", "faeden"] {
                let pid = match kern::starte_name(name) {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("K1: {name} startet nicht: {e:?}");
                        ok = false;
                        continue;
                    }
                };
                if name == "endlos" {
                    while !LAEUFT.swap(false, Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    kern::beende(pid);
                }
                let e = warte(pid);
                let erwartet = match (name, &e) {
                    ("normal" | "faeden", Some(Ende::Normal)) | ("endlos", Some(Ende::Beendet)) => true,
                    ("absturz", Some(Ende::Absturz(_))) | ("gierig", Some(Ende::SpeicherVoll)) => true,
                    _ => false,
                };
                if !erwartet {
                    ok = false;
                }
                if runde == 0 {
                    log::info!("K1: {name} pid {pid} -> {e:?}");
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            if runde == 0 {
                nach_runde1 = psram_frei();
            }
        }
        std::thread::sleep(Duration::from_millis(200));
        let nachher = psram_frei();
        // Ab Runde 2 darf nichts mehr verloren gehen (Runde 1: einmalige
        // Kern-Tabellen, Protokoll).
        let leck = nach_runde1.saturating_sub(nachher);
        log::info!(
            "K1-SELBSTTEST {}: 50 Starts (davon 10 mit je 20 Nebenfäden), PSRAM frei vorher {} KB, nach Runde 1 {} B, nachher {} B, Verlust ab Runde 2 {} B, Regionen {}, Notbelegungen {}",
            if ok && kern::regionen() == 0 && kern::notbelegungen() == 0 && leck < 4096 { "OK" } else { "FEHLER" },
            vorher / 1024,
            nach_runde1,
            nachher,
            leck,
            kern::regionen(),
            kern::notbelegungen()
        );
    }
}

/// Selbsttest K6 am Board (Build mit `KERN_DATEITEST=1`): Funktionen 70 auf
/// der internen Partition (/sys) und der SD-Karte (/sd0/1).
pub mod dateitest {
    use core::sync::atomic::{AtomicU32, Ordering};
    use kern::datei as d;
    use kern::ProgrammEintrag;
    use std::time::{Duration, Instant};

    static FEHLER: AtomicU32 = AtomicU32::new(0);
    static FERTIG: AtomicU32 = AtomicU32::new(0);

    fn pruefe(ok: bool, was: &str) {
        if ok {
            log::info!("K6 ok: {was}");
        } else {
            log::error!("K6 FEHLER: {was}");
            FEHLER.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn auf(basis: &str) {
        let p = |n: &str| format!("{basis}/{n}");
        let _ = d::loeschen(&p("k6test/b.txt"));
        let _ = d::loeschen(&p("k6test/a.txt"));
        let _ = d::loeschen(&p("k6test"));
        pruefe(d::ordner_erstellen(&p("k6test")) == 0, &format!("{basis}: Ordner anlegen"));
        let daten: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        pruefe(d::erstellen(&p("k6test/a.txt"), &daten) == (0, 20_000), &format!("{basis}: 20 KB schreiben"));
        let mut r = vec![0u8; 20_000];
        pruefe(d::lesen(&p("k6test/a.txt"), 0, &mut r) == (0, 20_000) && r == daten, &format!("{basis}: 20 KB lesen, gleich"));
        pruefe(d::umbenennen(&p("k6test/a.txt"), "b.txt") == 0, &format!("{basis}: umbenennen"));
        let mut b = vec![0u8; 32 + 8 * 304];
        let (_, n) = d::ordner_lesen(&p("k6test"), 0, 8, 1, &mut b);
        pruefe(n == 3, &format!("{basis}: Ordner lesen (., .., b.txt) = {n}"));
        pruefe(d::loeschen(&p("k6test")) == d::VERWEIGERT, &format!("{basis}: nicht leer → 10"));
        pruefe(d::loeschen(&p("k6test/b.txt")) == 0 && d::loeschen(&p("k6test")) == 0, &format!("{basis}: löschen"));
    }

    fn prog() {
        let mut b = vec![0u8; 32 + 8 * 304];
        let (_, n) = d::ordner_lesen("/", 0, 8, 1, &mut b);
        log::info!("K6: Wurzel hat {n} Einträge");
        auf("/sys");
        if crate::speicher::sd_mb() > 0 {
            auf("/sd0/1");
            let (_, n) = d::ordner_lesen("/sd0/1", 0, 8, 1, &mut b);
            log::info!("K6: /sd0/1 hat {n} Einträge (erste 8)");
        } else {
            pruefe(false, "SD-Karte fehlt");
        }
        FERTIG.store(1, Ordering::SeqCst);
    }

    pub static PROGRAMME: &[ProgrammEintrag] = &[ProgrammEintrag { name: "dateitest", heap_kb: 256, stapel_kb: 32, start: prog }];

    pub fn laufen() {
        let t = Instant::now();
        if kern::starte_name("dateitest").is_err() {
            log::error!("K6-SELBSTTEST FEHLER: Start");
            return;
        }
        while FERTIG.load(Ordering::SeqCst) == 0 && t.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let f = FEHLER.load(Ordering::SeqCst);
        log::info!(
            "K6-SELBSTTEST {}: {} Fehler, SD {} MB, {} ms",
            if f == 0 && FERTIG.load(Ordering::SeqCst) == 1 { "OK" } else { "FEHLER" },
            f,
            crate::speicher::sd_mb(),
            t.elapsed().as_millis()
        );
    }
}
