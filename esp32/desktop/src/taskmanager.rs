//! A6 Taskmanager: laufende Programme mit Fäden und Speicher (Heap des
//! Programms), Summen und freier Systemspeicher; „Programm beenden“ erst
//! kooperativ, nach [`WARTE_MS`] zwangsweise – auch Programme, die keine
//! Systemfunktion mehr rufen (Endlosschleife): `kern::zwangsbeenden` hält
//! ihre Tasks an, der Aufräumer gibt Task, Stapel, Heap, Fenster frei.
//!
//! Dazu das Testprogramm `endlostest` (nicht im Menü): Fenster, dann eine
//! Schleife ohne jeden Systemaufruf.

use kern::sys;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

/// Wartezeit auf das kooperative Ende, danach Zwang.
pub const WARTE_MS: u32 = 2000;

/// Systemspeicher [(frei, gesamt) intern, (frei, gesamt) PSRAM] in Byte.
pub type SpeicherQuelle = fn() -> [(usize, usize); 2];
static SPEICHER: OnceLock<SpeicherQuelle> = OnceLock::new();

/// Quelle des Systemspeichers setzen (Firmware; am PC gibt es keine).
pub fn speicher_quelle_setzen(f: SpeicherQuelle) {
    let _ = SPEICHER.set(f);
}

// Fenster und Aufbau (Arbeitsbereich-Koordinaten)
pub const FX: i32 = 20;
pub const FY: i32 = 4;
pub const FW: i32 = 600;
pub const FH: i32 = 306;
/// Erste Listenzeile, Zeilenhöhe, sichtbare Zeilen.
pub const LISTE_Y: i32 = 22;
pub const ZEILE_H: i32 = 24;
pub const ZEILEN: usize = 6;
/// Knopf „Programm beenden“ (Arbeitsbereich).
pub const BEENDEN: (i32, i32, i32, i32) = (440, 210, 140, 28);
const LISTE_W: i32 = 540;
const K_BEENDEN: u32 = 2;
const K_HOCH: u32 = 3;
const K_RUNTER: u32 = 4;
/// Zeilenknöpfe tragen die PID (nicht die Zeilennummer): Ändert sich die
/// Liste zwischen Zeichnen und Antippen, wird trotzdem das gezeigte
/// Programm gewählt.
const K_ZEILE: u32 = 0x10_0000;

fn text(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

fn kb(b: usize) -> String {
    b.div_ceil(1024).to_string()
}

fn mb(b: usize) -> String {
    format!("{:.1}", b as f64 / (1024.0 * 1024.0))
}

/// Programme in Anzeigereihenfolge: Anwendungen vor Systemdiensten („@…“),
/// sonst in Startreihenfolge.
pub fn liste() -> Vec<kern::ProzessInfo> {
    let mut v = kern::liste();
    v.sort_by_key(|p| p.name.starts_with('@'));
    v
}

/// Laufender Beenden-Auftrag (Hilfsfaden).
struct Auftrag {
    name: &'static str,
    ergebnis: Arc<AtomicU32>,
}

const LAEUFT: u32 = u32::MAX;

fn zwang_zahl(z: kern::Zwang) -> u32 {
    match z {
        kern::Zwang::Beendet => 0,
        kern::Zwang::Angehalten => 1,
        kern::Zwang::Verwaist => 2,
        kern::Zwang::UnbekanntePid => 3,
    }
}

fn status_text(name: &str, z: u32) -> (String, u32) {
    let t = kern::sprache::t;
    match z {
        LAEUFT => (format!("{} \"{name}\" ...", t("Ending", "Beende")), 0x404040),
        0 => (format!("\"{name}\" {}", t("was ended.", "wurde beendet.")), 0x1A6A1A),
        1 => (format!("\"{name}\" {}", t("hung - force-ended.", "hing - zwangsweise beendet.")), 0x1A6A1A),
        2 => (format!("\"{name}\" {}", t("could not be stopped.", "liess sich nicht anhalten.")), 0xB03020),
        _ => (format!("\"{name}\" {}", t("had already ended.", "war schon beendet.")), 0x404040),
    }
}

struct Lage {
    /// Ausgewählte PID.
    auswahl: Option<u32>,
    oben: usize,
    status: Option<(String, u32)>,
    auftrag: Option<Auftrag>,
    /// Gezeigter Aufbau (PIDs, Fäden, endet): ändert er sich, wird das
    /// Fenster neu aufgebaut (mit Knöpfen).
    aufbau: Vec<(u32, u32, bool)>,
    /// Gezeigte Zahlen (Speicher je Zeile, Summen): ändern sie sich, werden
    /// nur sie neu gezeichnet – höchstens einmal je Sekunde. Ein ständiger
    /// Neuaufbau nähme die Knöpfe immer wieder kurz weg (Tippen ginge
    /// verloren, Board-Lauf 22.11.).
    werte: Vec<String>,
    werte_zeit: u32,
}

fn aufbau(v: &[kern::ProzessInfo]) -> Vec<(u32, u32, bool)> {
    v.iter().map(|p| (p.pid, p.faeden, p.endet)).collect()
}

fn speicher_text(p: &kern::ProzessInfo) -> String {
    format!("{} / {}", kb(p.heap_belegt), kb(p.heap_groesse))
}

fn summen(v: &[kern::ProzessInfo]) -> [String; 2] {
    let t = kern::sprache::t;
    let belegt: usize = v.iter().map(|p| p.heap_belegt).sum();
    let groesse: usize = v.iter().map(|p| p.heap_groesse).sum();
    let eins = format!("{}: {}   {}: {} / {} MB", t("Programs", "Programme"), v.len(), t("Program memory", "Programmspeicher"), mb(belegt), mb(groesse));
    let zwei = SPEICHER
        .get()
        .map(|q| {
            let [(i_frei, i_ges), (p_frei, p_ges)] = q();
            format!("{}: {} / {} KB   PSRAM {}: {} / {} MB", t("Free RAM", "Frei RAM"), kb(i_frei), kb(i_ges), t("free", "frei"), mb(p_frei), mb(p_ges))
        })
        .unwrap_or_default();
    [eins, zwei]
}

fn werte(l: &Lage, v: &[kern::ProzessInfo]) -> Vec<String> {
    let mut w: Vec<String> = v.iter().skip(l.oben).take(ZEILEN).map(speicher_text).collect();
    w.extend(summen(v));
    w
}

/// Zeile `i` (Hintergrund und Texte; der Knopf darüber ist unsichtbar).
fn zeile_zeichnen(l: &Lage, i: usize, p: &kern::ProzessInfo) {
    let y = LISTE_Y + i as i32 * ZEILE_H;
    let gewaehlt = l.auswahl == Some(p.pid);
    let hg = if gewaehlt { 0xC8DCF4 } else if i % 2 == 0 { 0xFFFFFF } else { 0xECECEC };
    sys::rechteck((4u32) << 16 | LISTE_W as u32, (y as u32) << 16 | ZEILE_H as u32, hg);
    let farbe = if p.endet { 0x909090 } else if p.name.starts_with('@') { 0x505078 } else { 0x101010 };
    let name: String = p.name.chars().take(24).collect();
    text(10, y + 4, &name, farbe);
    text(210, y + 4, &p.pid.to_string(), farbe);
    text(262, y + 4, &p.faeden.to_string(), farbe);
    text(340, y + 4, &speicher_text(p), farbe);
}

const SUMME_Y: i32 = LISTE_Y + ZEILEN as i32 * ZEILE_H + 6;

fn summen_zeichnen(v: &[kern::ProzessInfo]) {
    sys::rechteck(4 << 16 | 430, (SUMME_Y as u32) << 16 | 38, 0xF4F4F4);
    let [eins, zwei] = summen(v);
    text(8, SUMME_Y, &eins, 0x202020);
    text(8, SUMME_Y + 20, &zwei, 0x202020);
}

/// Nur die Zahlen neu (ohne Fensteraufbau, Knöpfe bleiben).
fn werte_zeichnen(l: &mut Lage, v: &[kern::ProzessInfo]) {
    l.werte = werte(l, v);
    l.werte_zeit = sys::zeitzaehler();
    for (i, p) in v.iter().skip(l.oben).take(ZEILEN).enumerate() {
        zeile_zeichnen(l, i, p);
    }
    summen_zeichnen(v);
}

/// Ganzes Fenster mit Knöpfen.
fn zeichnen(l: &mut Lage, v: &[kern::ProzessInfo]) {
    let t = kern::sprache::t;
    l.oben = l.oben.min(v.len().saturating_sub(ZEILEN));
    l.aufbau = aufbau(v);
    l.werte = werte(l, v);
    l.werte_zeit = sys::zeitzaehler();
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xF4F4F4, 0, Some(t("Task manager", "Task-Manager")));
    let grau = 0x606060;
    text(8, 3, t("Program", "Programm"), grau);
    text(210, 3, "PID", grau);
    text(262, 3, t("Threads", "Faeden"), grau);
    text(340, 3, t("Memory (KB)", "Speicher (KB)"), grau);
    for (i, p) in v.iter().skip(l.oben).take(ZEILEN).enumerate() {
        let y = LISTE_Y + i as i32 * ZEILE_H;
        sys::knopf((4u32) << 16 | (LISTE_W - 1) as u32, (y as u32) << 16 | (ZEILE_H - 1) as u32, (K_ZEILE | p.pid) | 0x4000_0000, 0xFFFFFF);
        zeile_zeichnen(l, i, p);
    }
    if v.len() > ZEILEN {
        let x = 4 + LISTE_W + 6;
        sys::knopf((x as u32) << 16 | 33, (LISTE_Y as u32) << 16 | 39, K_HOCH, 0xD8D8D8);
        text(x + 13, LISTE_Y + 12, "^", 0x202020);
        let y2 = LISTE_Y + ZEILEN as i32 * ZEILE_H - 40;
        sys::knopf((x as u32) << 16 | 33, (y2 as u32) << 16 | 39, K_RUNTER, 0xD8D8D8);
        text(x + 13, y2 + 12, "v", 0x202020);
    }
    summen_zeichnen(v);
    let (bx, by, bw, bh) = BEENDEN;
    let aktiv = l.auswahl.is_some() && l.auftrag.is_none();
    sys::knopf((bx as u32) << 16 | (bw - 1) as u32, (by as u32) << 16 | (bh - 1) as u32, K_BEENDEN, if aktiv { 0xC04030 } else { 0xB0A8A8 });
    let b = t("End program", "Beenden");
    text(bx + (bw - b.chars().count() as i32 * 8) / 2, by + 6, b, 0xFFFFFF);
    if let Some((s, f)) = &l.status {
        let s: String = s.chars().take(52).collect();
        text(8, by + 6, &s, *f);
    }
    sys::neuzeichnen(2);
}

/// Programm „taskmanager“.
pub fn programm() {
    let eigen = kern::eigene_pid();
    let mut l = Lage { auswahl: None, oben: 0, status: None, auftrag: None, aufbau: Vec::new(), werte: Vec::new(), werte_zeit: 0 };
    loop {
        // Zeitüberschreitung 1 s: Liste auffrischen
        let ereignis = sys::warte_ereignis_max(100);
        let mut neu = false;
        match ereignis {
            1 => neu = true,
            3 => {
                let id = sys::knopf_id() >> 8;
                let v = liste();
                match id {
                    1 => return,
                    K_HOCH => {
                        l.oben = l.oben.saturating_sub(ZEILEN - 1);
                        neu = true;
                    }
                    K_RUNTER => {
                        l.oben = (l.oben + ZEILEN - 1).min(v.len().saturating_sub(ZEILEN));
                        neu = true;
                    }
                    K_BEENDEN if l.auftrag.is_none() => {
                        neu = true;
                        match l.auswahl.and_then(|pid| v.iter().find(|p| p.pid == pid)) {
                            None => l.status = Some((kern::sprache::t("Tap a program first.", "Erst ein Programm antippen.").into(), 0xB03020)),
                            Some(p) if Some(p.pid) == eigen => return,
                            Some(p) if p.name.starts_with('@') => {
                                l.status = Some((kern::sprache::t("System services cannot be ended.", "Systemdienste lassen sich nicht beenden.").into(), 0xB03020));
                            }
                            Some(p) => {
                                let pid = p.pid;
                                let ergebnis = Arc::new(AtomicU32::new(LAEUFT));
                                let e2 = ergebnis.clone();
                                sys::debug_text(&format!("Taskmanager: beende \"{}\" (PID {pid})", p.name));
                                let r = sys::faden(16, Box::new(move || e2.store(zwang_zahl(kern::zwangsbeenden(pid, WARTE_MS)), Ordering::Release)));
                                if r < 0 {
                                    ergebnis.store(zwang_zahl(kern::zwangsbeenden(pid, WARTE_MS)), Ordering::Release);
                                }
                                l.status = Some(status_text(p.name, LAEUFT));
                                l.auftrag = Some(Auftrag { name: p.name, ergebnis });
                            }
                        }
                    }
                    _ if id & K_ZEILE != 0 => {
                        let pid = id & !K_ZEILE;
                        if v.iter().any(|p| p.pid == pid) {
                            l.auswahl = Some(pid);
                            neu = true;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if let Some(a) = &l.auftrag {
            let z = a.ergebnis.load(Ordering::Acquire);
            if z != LAEUFT {
                sys::debug_text(&format!("Taskmanager: \"{}\" Ergebnis {z}", a.name));
                l.status = Some(status_text(a.name, z));
                l.auftrag = None;
                l.auswahl = None;
                neu = true;
            }
        }
        let v = liste();
        if neu || aufbau(&v) != l.aufbau {
            zeichnen(&mut l, &v);
        } else if sys::zeitzaehler().wrapping_sub(l.werte_zeit) >= 100 && werte(&l, &v) != l.werte {
            werte_zeichnen(&mut l, &v);
        }
    }
}

/// Nur für Tests am PC: lässt die Endlosschleife enden (am PC lassen sich
/// Threads nicht von aussen anhalten). Kein Systemaufruf – am Board nie gesetzt.
pub static ENDLOS_STOPP: AtomicBool = AtomicBool::new(false);

/// Testprogramm „endlostest“: Fenster, dann Endlosschleife ohne
/// Systemaufruf, die ständig im eigenen Heap belegt und freigibt (prüft,
/// dass nie mit gesetzter Heap-Sperre angehalten wird).
pub fn endlostest() {
    sys::pruefe_ereignis();
    sys::neuzeichnen(1);
    sys::fenster(150 << 16 | 299, 120 << 16 | 119, 0x33 << 24 | 0xFFE8D0, 0, Some(kern::sprache::t("Endless loop", "Endlosschleife")));
    text(10, 10, kern::sprache::t("Hangs on purpose (no system calls).", "Haengt absichtlich (keine Systemaufrufe)."), 0x802000);
    sys::neuzeichnen(2);
    sys::debug_text("endlostest: Schleife ohne Systemaufruf beginnt");
    let mut n = 0u32;
    let mut v: Vec<u32> = Vec::new();
    while !ENDLOS_STOPP.load(Ordering::Relaxed) {
        n = core::hint::black_box(n.wrapping_add(1));
        v.push(n);
        if v.len() >= 256 {
            v = core::hint::black_box(Vec::new());
        }
    }
}
