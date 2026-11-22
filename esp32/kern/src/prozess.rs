//! Prozessmodell: Programme laufen als Tasks (Fäden) im gemeinsamen
//! Adressraum, jedes Programm mit eigenem Heap in einem eigenen Bereich.
//! Ende (normal, Systemfunktion −1, Beenden von aussen, Absturz, Speicher
//! voll) gibt den Bereich als Ganzes frei, das System läuft weiter.

use crate::alloc::{PL, not_freigeben, region_austragen, region_eintragen};
use crate::heap::{Heap, MIN_BEREICH};
use crate::plattform::{Lauf, Plattform};
use core::cell::{Cell, RefCell};
use core::sync::atomic::Ordering::Relaxed;
use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Statisch eingelinktes Programm.
#[derive(Clone, Copy)]
pub struct ProgrammEintrag {
    pub name: &'static str,
    /// Heap des Programms in KB.
    pub heap_kb: usize,
    /// Stapel des Hauptfadens in KB.
    pub stapel_kb: usize,
    pub start: fn(),
}

/// Warum ein Programm endete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ende {
    Normal,
    /// Von aussen beendet (Taskmanager, Systemfunktion 18.2).
    Beendet,
    /// Rust-Panik mit Meldung.
    Absturz(String),
    /// Heap des Programms erschöpft.
    SpeicherVoll,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StartFehler {
    Unbekannt,
    KeinSpeicher,
    ZuVieleProgramme,
    TaskFehler,
}

/// Kontext eines Programm-Fadens (Thread-lokal über die Plattform).
pub struct Faden {
    pub pid: u32,
    /// Faden-Kennung (KolibriOS TID); beim Hauptfaden = PID.
    pub tid: u32,
    pub heap: *const Heap,
    /// > 0, solange Kern-Code auf diesem Faden läuft (belegt dann aus dem
    /// System). Atomar: das Zwangsbeenden liest es von einem anderen Faden.
    pub im_kern: AtomicU32,
    /// > 0 in [`geschuetzt`]: Treiber, Netz, Update – hier wird nie
    /// zwangsweise angehalten und nicht an `pruefe_ende` beendet.
    pub geschuetzt: AtomicU32,
    pub speicher_voll: Cell<bool>,
    pub panik: RefCell<Option<String>>,
}

struct Prozess {
    pid: u32,
    name: &'static str,
    basis: *mut u8,
    laenge: usize,
    heap: *const Heap,
    faeden: u32,
    soll_enden: bool,
    /// Fäden des Programms: Task-Griff (sobald erzeugt), schon beendet?
    faeden_liste: Vec<FadenEintrag>,
    /// Start läuft noch – nicht aufräumen.
    startet: bool,
    ende: Option<Ende>,
    /// Startparameter (Funktion 70.7).
    argumente: String,
}

// SAFETY: Zeiger werden nur unter der Prozesstabellen-Sperre benutzt.
unsafe impl Send for Prozess {}

struct FadenEintrag {
    tid: u32,
    griff: Option<usize>,
    beendet: bool,
    /// Kontext des Fadens (`*mut Faden`), für das Zwangsbeenden (läuft er
    /// gerade Kern-Code?).
    kontext: usize,
    /// Zwangsweise angehalten: Kontext gibt der Aufräumer frei (nach dem
    /// Löschen des Tasks).
    zwang: bool,
}

/// Öffentliche Sicht auf ein Programm (Taskmanager, Systemfunktion 9).
#[derive(Clone, Debug)]
pub struct ProzessInfo {
    pub pid: u32,
    pub name: &'static str,
    pub faeden: u32,
    pub heap_belegt: usize,
    pub heap_spitze: usize,
    pub heap_groesse: usize,
    pub endet: bool,
}

struct Kern {
    pl: &'static Plattform,
    programme: &'static [ProgrammEintrag],
    prozesse: Mutex<Vec<Prozess>>,
    protokoll: Mutex<Vec<(u32, &'static str, Ende)>>,
    /// Laufende Nummer des letzten Protokolleintrags / des zuletzt gemeldeten Absturzes.
    protokoll_nr: AtomicU32,
    gemeldet: AtomicU32,
    /// Bereiche beendeter Programme, frei beim nächsten Aufräumen (ein
    /// gleichzeitiges Freigeben kann ihren Heap-Kopf noch kurz lesen).
    frei_spaeter: Mutex<Vec<(usize, usize)>>,
    naechste_pid: AtomicU32,
}

static KERN: OnceLock<Kern> = OnceLock::new();

fn kern() -> &'static Kern {
    KERN.get().expect("kern::init fehlt")
}

fn sperre<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Aktueller Faden-Kontext (null = Kern/System-Thread).
pub(crate) fn faden() -> *mut Faden {
    KERN.get().map_or(core::ptr::null_mut(), |k| (k.pl.faden_lesen)())
}

struct KernWache(*mut Faden);

impl Drop for KernWache {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: eigener Faden.
            unsafe { (*self.0).im_kern.fetch_sub(1, Relaxed) };
        }
    }
}

struct SchutzWache(*mut Faden);

impl Drop for SchutzWache {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: eigener Faden.
            unsafe { (*self.0).geschuetzt.fetch_sub(1, Relaxed) };
        }
    }
}

/// Geschützter Abschnitt: Der Faden darf hier nicht zwangsweise angehalten
/// werden (Taskmanager, A6) und endet nicht an `pruefe_ende` – er kann
/// Sperren, Treiber oder Verbindungen halten, die nicht dem Programm
/// gehören (WLAN-Suche, Sockets, Update, JPEG-Einheit). Belegungen bleiben
/// im Heap des Programms (anders als [`kern_abschnitt`]).
pub fn geschuetzt<R>(f: impl FnOnce() -> R) -> R {
    let fd = faden();
    if !fd.is_null() {
        // SAFETY: eigener Faden.
        unsafe { (*fd).geschuetzt.fetch_add(1, Relaxed) };
    }
    let _w = SchutzWache(fd);
    f()
}

/// Soll der laufende Faden enden? Für Warteschleifen in [`geschuetzt`]en
/// Abschnitten: Sie geben dann einen Fehler zurück, das Programm endet beim
/// nächsten Systemaufruf.
pub(crate) fn ende_angefordert() -> bool {
    let fd = faden();
    if fd.is_null() {
        return false;
    }
    // SAFETY: eigener Faden.
    let (pid, voll) = unsafe { ((*fd).pid, (*fd).speicher_voll.get()) };
    voll || kern_abschnitt(|| sperre(&kern().prozesse).iter().any(|p| p.pid == pid && p.soll_enden))
}

/// Führt Kern-Code aus: Belegungen gehen in den System-Heap, auch wenn ein
/// Programm-Faden aufruft. Regel: Alles, was ein Programm überlebt (Kern-
/// Tabellen, Warteschlangen, globale Puffer), wird nur hier belegt – der
/// Programm-Heap verschwindet beim Ende als Ganzes.
pub fn kern_abschnitt<R>(f: impl FnOnce() -> R) -> R {
    let fd = faden();
    if !fd.is_null() {
        // SAFETY: eigener Faden.
        unsafe { (*fd).im_kern.fetch_add(1, Relaxed) };
    }
    let _w = KernWache(fd);
    f()
}

/// Startet den Kern. Einmal beim Systemstart; weitere Aufrufe werden ignoriert.
pub fn init(pl: &'static Plattform, programme: &'static [ProgrammEintrag]) {
    let mut neu = false;
    KERN.get_or_init(|| {
        neu = true;
        Kern {
            pl,
            programme,
            prozesse: Mutex::new(Vec::new()),
            protokoll: Mutex::new(Vec::new()),
            protokoll_nr: AtomicU32::new(0),
            gemeldet: AtomicU32::new(0),
            frei_spaeter: Mutex::new(Vec::new()),
            naechste_pid: AtomicU32::new(2),
        }
    });
    if !neu {
        return;
    }
    PL.store((pl as *const Plattform).cast_mut(), Ordering::Release);
    crate::zeit::start();
    // Gemeinsame Bedingungsvariable jetzt (System-Heap) anlegen.
    crate::fenster::wecken();
    let alt = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let fd = faden();
        // SAFETY: eigener Faden.
        let im_programm = !fd.is_null() && unsafe { (*fd).im_kern.load(Relaxed) } == 0;
        if im_programm {
            let msg = kern_abschnitt(|| info.to_string());
            // SAFETY: eigener Faden.
            // Kopie ebenfalls im System-Heap: sie landet im Protokoll und
            // überlebt die Speicherregion des Programms.
            unsafe { *(*fd).panik.borrow_mut() = Some(kern_abschnitt(|| msg.clone())) };
            // Ausgabe im Kern-Abschnitt: Puffer der Ausgabe (stdout, Test-
            // Mitschnitt) dürfen nicht im Programm-Heap wachsen.
            kern_abschnitt(|| alt(info));
            if kern().pl.panik_beendet_task {
                beende_selbst(Ende::Absturz(msg));
            }
            return;
        }
        alt(info);
    }));
    // Globale Ausgabepuffer jetzt (im System-Heap) anlegen, nicht später
    // zufällig aus einem Programm heraus.
    {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
    }
    std::thread::Builder::new()
        .name("aufraeumer".into())
        .stack_size(if cfg!(target_os = "espidf") { 12 * 1024 } else { 256 * 1024 })
        .spawn(|| {
            loop {
                aufraeumen();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .expect("Aufräum-Thread");
}

/// Programmtabelle.
pub fn programme() -> &'static [ProgrammEintrag] {
    kern().programme
}

/// Startet ein Programm aus der Tabelle nach Namen.
pub fn starte_name(name: &str) -> Result<u32, StartFehler> {
    let e = kern().programme.iter().find(|e| e.name == name).ok_or(StartFehler::Unbekannt)?;
    starte(e)
}

/// Startet ein Programm: eigener Bereich + Heap, Hauptfaden.
pub fn starte(e: &'static ProgrammEintrag) -> Result<u32, StartFehler> {
    starte_mit(e, "")
}

/// Wie [`starte`], mit Startparametern (Funktion 70.7).
pub fn starte_mit(e: &'static ProgrammEintrag, argumente: &str) -> Result<u32, StartFehler> {
    kern_abschnitt(|| {
        let k = kern();
        let laenge = (e.heap_kb + e.stapel_kb) * 1024 + MIN_BEREICH + 4096;
        let basis = (k.pl.bereich_holen)(laenge);
        if basis.is_null() {
            return Err(StartFehler::KeinSpeicher);
        }
        // SAFETY: frischer, exklusiver Bereich.
        let heap = unsafe { Heap::anlegen(basis, laenge) };
        if !region_eintragen(heap) {
            (k.pl.bereich_frei)(basis, laenge);
            return Err(StartFehler::ZuVieleProgramme);
        }
        let pid = k.naechste_pid.fetch_add(1, Ordering::Relaxed);
        sperre(&k.prozesse).push(Prozess {
            pid,
            name: e.name,
            basis,
            laenge,
            heap,
            faeden: 0,
            soll_enden: false,
            faeden_liste: Vec::new(),
            startet: true,
            ende: None,
            argumente: argumente.chars().take(256).collect(),
        });
        let start = e.start;
        if starte_faden_fuer(pid, Some(pid), e.name, e.stapel_kb, Box::new(move || start())).is_none() {
            let mut l = sperre(&k.prozesse);
            if let Some(i) = l.iter().position(|p| p.pid == pid) {
                let p = l.remove(i);
                region_austragen(p.heap);
                (k.pl.bereich_frei)(p.basis, p.laenge);
            }
            return Err(StartFehler::TaskFehler);
        }
        if let Some(p) = sperre(&k.prozesse).iter_mut().find(|p| p.pid == pid) {
            p.startet = false;
        }
        Ok(pid)
    })
}

/// Weiterer Faden im laufenden Programm (Systemfunktion 51). Liefert die
/// Faden-Kennung (TID) oder None.
pub fn faden_starten(stapel_kb: usize, f: Box<dyn FnOnce() + Send>) -> Option<u32> {
    let fd = faden();
    if fd.is_null() {
        return None;
    }
    // SAFETY: eigener Faden.
    let pid = unsafe { (*fd).pid };
    kern_abschnitt(|| {
        let name = sperre(&kern().prozesse).iter().find(|p| p.pid == pid)?.name;
        starte_faden_fuer(pid, None, name, stapel_kb, f)
    })
}

fn starte_faden_fuer(
    pid: u32,
    tid: Option<u32>,
    name: &str,
    stapel_kb: usize,
    start: Box<dyn FnOnce() + Send>,
) -> Option<u32> {
    let k = kern();
    let tid = tid.unwrap_or_else(|| k.naechste_pid.fetch_add(1, Ordering::Relaxed));
    let heap = {
        let mut l = sperre(&k.prozesse);
        let p = l.iter_mut().find(|p| p.pid == pid)?;
        p.faeden += 1;
        // Eintrag vor dem Task: endet der Faden, bevor sein Griff da ist,
        // wartet der Aufräumer auf den Griff.
        p.faeden_liste.push(FadenEintrag { tid, griff: None, beendet: false, kontext: 0, zwang: false });
        p.heap
    };
    crate::fenster::mit_welt(|w| w.faden_neu(tid, pid, name));
    let fd = Box::into_raw(Box::new(Faden {
        pid,
        tid,
        heap,
        im_kern: AtomicU32::new(0),
        geschuetzt: AtomicU32::new(0),
        speicher_voll: Cell::new(false),
        panik: RefCell::new(None),
    })) as usize;
    if let Some(e) = sperre(&k.prozesse).iter_mut().find(|p| p.pid == pid).and_then(|p| p.faeden_liste.iter_mut().find(|e| e.tid == tid)) {
        e.kontext = fd;
    }
    let pl = k.pl;
    let lauf = Box::new(move || {
        (pl.faden_setzen)(fd as *mut Faden);
        match (pl.ausfuehren)(start) {
            Lauf::SchonBeendet => {}
            Lauf::Normal => faden_ende(Ende::Normal),
            Lauf::Panik => {
                // SAFETY: eigener Faden, noch gesetzt.
                let msg = unsafe { (*(fd as *mut Faden)).panik.borrow_mut().take() };
                let grund = kern_abschnitt(|| Ende::Absturz(msg.unwrap_or_else(|| "Panik".into())));
                faden_ende(grund);
            }
        }
    });
    // SAFETY: Heap lebt, bis der Prozess aufgeräumt ist.
    let griff = (pl.task_starten)(name, stapel_kb * 1024, unsafe { &*heap }, lauf);
    let mut l = sperre(&k.prozesse);
    let p = l.iter_mut().find(|p| p.pid == pid)?;
    match griff {
        Some(g) => {
            if let Some(e) = p.faeden_liste.iter_mut().find(|e| e.tid == tid) {
                e.griff = Some(g);
            }
            Some(tid)
        }
        None => {
            p.faeden -= 1;
            p.faeden_liste.retain(|e| e.tid != tid);
            drop(l);
            crate::fenster::mit_welt(|w| w.faden_weg(tid));
            // Der Faden lief nie: Kontext selbst freigeben.
            // SAFETY: nie an einen Thread übergeben.
            drop(unsafe { Box::from_raw(fd as *mut Faden) });
            None
        }
    }
}

/// Meldet den laufenden Faden ab und entfernt seinen Kontext.
fn faden_ende(grund: Ende) {
    let fd = faden();
    if fd.is_null() {
        return;
    }
    kern_abschnitt(|| {
        // SAFETY: eigener Faden.
        let (pid, tid) = unsafe { ((*fd).pid, (*fd).tid) };
        crate::fenster::mit_welt(|w| w.faden_weg(tid));
        crate::datei::faden_weg(tid);
        let mut l = sperre(&kern().prozesse);
        if let Some(p) = l.iter_mut().find(|p| p.pid == pid) {
            // Schon ausgetragen (Zwangsbeenden am PC: lief weiter)? Dann
            // nicht noch einmal zählen.
            if let Some(e) = p.faeden_liste.iter_mut().find(|e| e.tid == tid && !e.beendet) {
                e.beendet = true;
                p.faeden = p.faeden.saturating_sub(1);
            }
            if grund != Ende::Normal {
                p.soll_enden = true;
            }
            if p.ende.is_none() || (p.ende == Some(Ende::Normal) && grund != Ende::Normal) {
                p.ende = Some(grund);
            }
        }
    });
    (kern().pl.faden_setzen)(core::ptr::null_mut());
    // SAFETY: Kontext stammt aus `Box::into_raw`, ist jetzt nicht mehr gesetzt.
    drop(unsafe { Box::from_raw(fd) });
}

/// Beendet den laufenden Programm-Faden sofort (Systemfunktion −1).
pub fn beende_selbst(grund: Ende) -> ! {
    faden_ende(grund);
    (kern().pl.task_beenden)()
}

/// Wird von jeder Systemfunktion eines Programms aufgerufen: beendet den
/// Faden, wenn das Programm beendet werden soll oder sein Heap voll ist.
pub fn pruefe_ende() {
    let fd = faden();
    if fd.is_null() {
        return;
    }
    // In einem geschützten Abschnitt (Treiber, Update) nicht: Am Board
    // bliebe der Faden dort mit Sperren stehen. Er endet beim nächsten
    // Systemaufruf danach.
    // SAFETY: eigener Faden.
    if unsafe { (*fd).geschuetzt.load(Relaxed) } > 0 {
        return;
    }
    // SAFETY: eigener Faden.
    if unsafe { (*fd).speicher_voll.get() } {
        beende_selbst(Ende::SpeicherVoll);
    }
    // SAFETY: eigener Faden.
    let pid = unsafe { (*fd).pid };
    let soll = kern_abschnitt(|| sperre(&kern().prozesse).iter().any(|p| p.pid == pid && p.soll_enden));
    if soll {
        beende_selbst(Ende::Beendet);
    }
}

/// Beendet ein Programm von aussen (kooperativ: bei seiner nächsten
/// Systemfunktion). false = unbekannte PID.
pub fn beende(pid: u32) -> bool {
    kern_abschnitt(|| {
        let mut l = sperre(&kern().prozesse);
        match l.iter_mut().find(|p| p.pid == pid) {
            Some(p) => {
                p.soll_enden = true;
                if p.ende.is_none() {
                    p.ende = Some(Ende::Beendet);
                }
                crate::fenster::wecken();
                true
            }
            None => false,
        }
    })
}

/// Ergebnis von [`zwangsbeenden`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zwang {
    /// Programm hat sich (kooperativ) beendet.
    Beendet,
    /// Fäden angehalten und ausgetragen (Board: Task wird gelöscht, Heap frei).
    Angehalten,
    /// Nicht (ganz) angehalten: Plattform kann es nicht (PC: Fenster weg,
    /// Thread läuft weiter, sein Speicher bleibt), oder ein Faden blieb
    /// dauerhaft im Kern bzw. in einem geschützten Abschnitt (endet dann
    /// beim nächsten Systemaufruf).
    Verwaist,
    UnbekanntePid,
}

/// Darf der angehaltene bzw. laufende Faden ausgetragen werden? Nicht, wenn
/// er Kern-Code oder einen geschützten Abschnitt ausführt oder die Sperre
/// seines Programm-Heaps gesetzt ist (der Aufräumer gibt dort seinen Stapel
/// frei).
///
/// # Safety
/// `kontext` ist 0 oder ein lebender `Faden`, `heap` lebt.
unsafe fn frei_anhaltbar(kontext: usize, heap: *const Heap) -> bool {
    if kontext == 0 {
        return false;
    }
    // SAFETY: Vertrag des Aufrufers.
    let f = unsafe { &*(kontext as *const Faden) };
    // SAFETY: Vertrag des Aufrufers.
    f.im_kern.load(Ordering::Acquire) == 0 && f.geschuetzt.load(Ordering::Acquire) == 0 && !unsafe { &*heap }.gesperrt()
}

/// Beendet ein Programm auch dann, wenn es keine Systemfunktion mehr ruft
/// (Endlosschleife; Taskmanager, A6). Erst kooperativ (bis `warte_ms`),
/// dann wird jeder noch laufende Faden angehalten – aber nur, wenn er
/// gerade keinen Kern-Code und keinen [`geschuetzt`]en Abschnitt ausführt
/// und die Sperre seines Heaps nicht gesetzt ist: Alles, was ein Programm
/// mit dem System teilt (Kern-Tabellen, Sperren, Treiber, Ausgabe), wird
/// nur dort angefasst (stdout/stderr der C-Programme gehen deshalb über das
/// Debug-Board). Geprüft wird vor dem Anhalten (dann wird ein Faden, der im
/// Kern wartet, gar nicht angefasst) und danach noch einmal (er könnte
/// gerade eingetreten sein → fortsetzen, später erneut).
///
/// Anhalten, Prüfen und Austragen geschehen unter der Prozesstabelle: Der
/// Faden kann nicht zugleich selbst enden, der Aufräumer seinen Griff nicht
/// freigeben, ein zweiter Aufruf ihn nicht noch einmal austragen. Fenster
/// und Dateien werden gleich danach entfernt; Task, Stapel, Kontext, Heap
/// und Sockets gibt [`aufraeumen`] frei (räumt auch die Fenster ab, falls
/// der Aufrufer selbst vorher endet).
pub fn zwangsbeenden(pid: u32, warte_ms: u32) -> Zwang {
    if eigene_pid() == Some(pid) {
        beende_selbst(Ende::Beendet);
    }
    if !beende(pid) {
        return Zwang::UnbekanntePid;
    }
    let k = kern();
    let laeuft = || {
        kern_abschnitt(|| {
            sperre(&k.prozesse).iter().find(|p| p.pid == pid).is_some_and(|p| p.startet || p.faeden_liste.iter().any(|e| !e.beendet))
        })
    };
    let mut gewartet = 0;
    while gewartet < warte_ms {
        if !laeuft() {
            return Zwang::Beendet;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        gewartet += 20;
    }
    let mut verwaist = false;
    for _versuch in 0..250 {
        let (weg, offen) = kern_abschnitt(|| {
            let mut l = sperre(&k.prozesse);
            let Some(p) = l.iter_mut().find(|p| p.pid == pid) else { return (Vec::new(), false) };
            let heap = p.heap;
            let mut weg = Vec::new();
            let mut offen = p.startet;
            for e in p.faeden_liste.iter_mut().filter(|e| !e.beendet) {
                let Some(g) = e.griff else {
                    offen = true; // Task startet gerade
                    continue;
                };
                let Some(anhalten) = k.pl.task_anhalten else {
                    // Plattform kann nicht anhalten (PC): nur austragen. Der
                    // Eintrag bleibt, bis der Thread wirklich endet – so wird
                    // kein Speicher unter laufendem Code freigegeben.
                    e.beendet = true;
                    e.kontext = 0;
                    weg.push(e.tid);
                    verwaist = true;
                    continue;
                };
                // SAFETY: nicht beendet → Kontext lebt; Heap lebt mit dem Prozess.
                if !unsafe { frei_anhaltbar(e.kontext, heap) } {
                    offen = true; // im Kern / geschützt: nicht anfassen
                    continue;
                }
                if !anhalten(g) {
                    offen = true; // stand nicht rechtzeitig: später erneut
                    continue;
                }
                // SAFETY: wie oben; Faden steht.
                if !unsafe { frei_anhaltbar(e.kontext, heap) } {
                    (k.pl.task_fortsetzen)(g);
                    offen = true;
                    continue;
                }
                // Steht für immer: `task_weg` meldet ihn ab jetzt als weg
                e.beendet = true;
                e.zwang = true;
                weg.push(e.tid);
            }
            p.faeden = p.faeden.saturating_sub(weg.len() as u32);
            if p.ende.is_none() {
                p.ende = Some(Ende::Beendet);
            }
            (weg, offen)
        });
        for &tid in &weg {
            kern_abschnitt(|| {
                crate::fenster::mit_welt(|w| w.faden_weg(tid));
                crate::datei::faden_weg(tid);
            });
        }
        if !offen {
            return if verwaist { Zwang::Verwaist } else { Zwang::Angehalten };
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Zwang::Verwaist
}

/// PID des laufenden Programms (None = Kern/System).
pub fn eigene_pid() -> Option<u32> {
    let fd = faden();
    // SAFETY: eigener Faden.
    (!fd.is_null()).then(|| unsafe { (*fd).pid })
}

/// Startparameter des laufenden Programms.
pub fn argumente() -> String {
    let Some(pid) = eigene_pid() else { return String::new() };
    kern_abschnitt(|| {
        sperre(&kern().prozesse).iter().find(|p| p.pid == pid).map(|p| p.argumente.clone()).unwrap_or_default()
    })
}

/// TID des laufenden Fadens (None = Kern/System).
pub fn eigene_tid() -> Option<u32> {
    let fd = faden();
    // SAFETY: eigener Faden.
    (!fd.is_null()).then(|| unsafe { (*fd).tid })
}

/// Heap des laufenden Programm-Fadens (Funktion 68.12).
pub(crate) fn eigener_heap() -> Option<&'static Heap> {
    let fd = faden();
    // SAFETY: eigener Faden; Heap lebt, solange der Faden läuft.
    (!fd.is_null()).then(|| unsafe { &*(*fd).heap })
}

/// Laufende Programme.
pub fn liste() -> Vec<ProzessInfo> {
    kern_abschnitt(|| {
        sperre(&kern().prozesse)
            .iter()
            .map(|p| {
                // SAFETY: Heap lebt, solange der Prozess in der Tabelle steht.
                let h = unsafe { &*p.heap };
                ProzessInfo {
                    pid: p.pid,
                    name: p.name,
                    faeden: p.faeden,
                    heap_belegt: h.belegt(),
                    heap_spitze: h.spitze(),
                    heap_groesse: h.groesse(),
                    endet: p.soll_enden || p.faeden == 0,
                }
            })
            .collect()
    })
}

/// Wie ein beendetes Programm endete (None = läuft noch oder unbekannt).
pub fn ende_von(pid: u32) -> Option<Ende> {
    kern_abschnitt(|| {
        sperre(&kern().protokoll)
            .iter()
            .rev()
            .find(|(p, _, _)| *p == pid)
            .map(|(_, _, e)| e.clone())
    })
}

/// Abstürze (Panik, Speicher voll) seit dem letzten Aufruf – für die
/// Meldung der Shell (D12). (Name, Grund)
pub fn neue_abstuerze() -> Vec<(&'static str, Ende)> {
    kern_abschnitt(|| {
        let k = kern();
        // Zähler unter der Protokollsperre lesen (aufraeumen erhöht ihn dort)
        let prot = sperre(&k.protokoll);
        let nr = k.protokoll_nr.load(Ordering::Acquire);
        let alt = k.gemeldet.swap(nr, Ordering::AcqRel);
        let neu = nr.wrapping_sub(alt) as usize;
        prot.iter()
            .rev()
            .take(neu.min(prot.len()))
            .filter(|(_, _, e)| matches!(e, Ende::Absturz(_) | Ende::SpeicherVoll))
            .map(|(_, n, e)| (*n, e.clone()))
            .collect()
    })
}

/// Räumt ab: beendete Fäden einzeln (Stapel und Task-Kopf frei, auch
/// solange das Programm weiterläuft), beendete Programme ganz (Bereich wird
/// erst beim nächsten Durchlauf freigegeben). Läuft periodisch im
/// Aufräum-Thread.
pub fn aufraeumen() {
    kern_abschnitt(|| {
        let k = kern();
        let alt_frei = core::mem::take(&mut *sperre(&k.frei_spaeter));
        let mut fertig = Vec::new();
        let mut griffe = Vec::new();
        // (TID, Kontext eines zwangsweise angehaltenen Fadens)
        let mut weg = Vec::new();
        {
            let mut l = sperre(&k.prozesse);
            for p in l.iter_mut() {
                p.faeden_liste.retain(|e| match e.griff {
                    Some(g) if e.beendet && (k.pl.task_weg)(g) => {
                        griffe.push(g);
                        weg.push((e.tid, if e.zwang { e.kontext } else { 0 }));
                        false
                    }
                    _ => true,
                });
            }
            let mut i = 0;
            while i < l.len() {
                let p = &l[i];
                if p.faeden == 0 && !p.startet && p.faeden_liste.is_empty() {
                    fertig.push(l.remove(i));
                } else {
                    i += 1;
                }
            }
        }
        for g in griffe {
            (k.pl.task_freigeben)(g);
        }
        for (tid, kontext) in weg {
            // Fenster/Dateien: meist schon weg (faden_ende, zwangsbeenden);
            // hier nur, falls der Beendende vorher selbst endete.
            crate::fenster::mit_welt(|w| w.faden_weg(tid));
            crate::datei::faden_weg(tid);
            if kontext != 0 {
                // SAFETY: Task ist gelöscht; Kontext aus Box::into_raw, nie
                // anderswo freigegeben (zwangsweise angehalten).
                drop(unsafe { Box::from_raw(kontext as *mut Faden) });
            }
        }
        let mut neu_frei = Vec::new();
        for p in fertig {
            // Offene Sockets des Programms schließen (W9)
            crate::netz::prozess_ende(p.pid);
            region_austragen(p.heap);
            not_freigeben(p.pid);
            neu_frei.push((p.basis as usize, p.laenge));
            let mut prot = sperre(&k.protokoll);
            if prot.len() >= 256 {
                prot.remove(0);
            }
            prot.push((p.pid, p.name, p.ende.unwrap_or(Ende::Normal)));
            k.protokoll_nr.fetch_add(1, Ordering::AcqRel);
            drop(prot);
        }
        for (b, n) in alt_frei {
            (k.pl.bereich_frei)(b as *mut u8, n);
        }
        sperre(&k.frei_spaeter).extend(neu_frei);
    });
}
