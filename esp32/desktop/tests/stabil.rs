//! Stabilitätsrunde (SB/SC/SD, Gesamtauftrag 24.11.): alle Programme über
//! das Startmenü öffnen, bedienen (Fenster ziehen, im Browser Links
//! antippen), über das Kreuz schliessen, das Menü oft öffnen und schliessen.
//! Am PC wird jeder Schritt geprüft (Fenster da/weg, kein Absturz); dieselben
//! Schritte mit Board-Wartezeiten ergeben den Autopilot für die
//! Board-Runde (`STABIL_AUTOPILOT_NEU=1` schreibt
//! `werkzeuge/board/stabil_autopilot.txt`).
//!
//! Spiele bleiben aussen vor (Doom/Quake starten ein anderes Abbild neu),
//! ebenso „Bildschirmfoto“ (schreibt auf die SD des Nutzers) und der
//! Touchpad-Schalter. Bedient wird nur Harmloses: Titelleiste ziehen,
//! im Browser Inhalt antippen – nie Knöpfe, die Daten ändern.
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Startknopf der Taskleiste.
const START: (i32, i32) = (40, 340);
/// Programme ohne eigenes Fenster (Ausgabe nur im Protokoll).
const OHNE_FENSTER: &[&str] = &["http_tcp_demo", "netz_laden"];
/// Runden des Dauertests M4 (≥ 60 min, `dauertest_autopilot.txt`).
const RUNDEN_DAUER: usize = 10;
/// Runden für den Board-Lauf (je Runde etwa 6½ min → 38 min).
const RUNDEN: usize = 6;
/// Beginn am Board: Desktop steht, WLAN verbunden.
const BEGINN_MS: u64 = 25_000;

#[derive(Clone, Copy, Debug)]
enum Schritt {
    /// Antippen bei (x, y).
    Tipp(i32, i32),
    /// Ziehen von – nach (Finger bleibt liegen, dann los).
    Ziehen(i32, i32, i32, i32),
    /// Warten (Board-Zeit in ms).
    Warte(u64),
}

struct Lauf {
    g: Gesten,
    t: u64,
    schritte: Vec<Schritt>,
}

impl Lauf {
    fn takt(&mut self, f: [Option<(i32, i32)>; 2], ms: u64) {
        self.g.schritt(self.t, f);
        self.t += ms;
        std::thread::sleep(Duration::from_millis(ms.min(30)));
    }

    fn tipp(&mut self, x: i32, y: i32) {
        self.schritte.push(Schritt::Tipp(x, y));
        self.takt([Some((x, y)), None], 120);
        for _ in 0..10 {
            self.takt([None, None], 20);
        }
    }

    fn ziehen(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.schritte.push(Schritt::Ziehen(x0, y0, x1, y1));
        for i in 0..=8 {
            let (x, y) = (x0 + (x1 - x0) * i / 8, y0 + (y1 - y0) * i / 8);
            self.takt([Some((x, y)), None], 60);
        }
        for _ in 0..10 {
            self.takt([None, None], 20);
        }
    }

    /// Board-Wartezeit; am PC kürzer (Programme starten schneller).
    fn warte(&mut self, board_ms: u64) {
        self.schritte.push(Schritt::Warte(board_ms));
        std::thread::sleep(Duration::from_millis((board_ms / 4).clamp(250, 2500)));
    }
}

fn fenster_slots() -> Vec<(usize, String)> {
    sys::fenster_liste().into_iter().map(|f| (f.slot, f.titel)).collect()
}

/// Lage des Kreuzes (Systemknopf 1) und eines Punkts der Titelleiste.
fn kreuz_und_titel(slot: usize) -> Option<((i32, i32), (i32, i32), (i32, i32, i32, i32))> {
    kern::fenster::mit_welt(|w| {
        let f = w.slots.get(slot)?.as_ref()?.fenster.as_ref()?;
        let k = f.knoepfe.iter().find(|k| k.system && k.id == 1)?;
        Some(((f.x + k.x + k.w / 2, f.y + k.y + k.h / 2), (f.x + f.w / 3, f.y + 8), (f.x, f.y, f.w, f.h)))
    })
}

fn abstuerze() -> Vec<String> {
    kern::neue_abstuerze().into_iter().map(|(n, e)| format!("{n}: {e:?}")).collect()
}

/// Ein Programm aus dem Menü (Ordner "" oder "mehr"/"system") öffnen,
/// bedienen, schliessen.
fn programm(l: &mut Lauf, ordner: &str, programm: &str) {
    let vorher = fenster_slots();
    l.tipp(START.0, START.1);
    l.warte(1200);
    if !ordner.is_empty() {
        let (x, y) = desktop::shell::menue_eintrag_mitte(&format!("@{ordner}")).unwrap_or_else(|| panic!("Ordner {ordner}"));
        l.tipp(x, y);
        l.warte(1200);
    }
    let (x, y) = desktop::shell::menue_eintrag_mitte_in(ordner, programm).unwrap_or_else(|| panic!("{programm} im Menü"));
    l.tipp(x, y);
    let browser = programm == "browser";
    l.warte(if browser { 20_000 } else { 5_000 });
    let neu: Vec<(usize, String)> = fenster_slots().into_iter().filter(|f| !vorher.contains(f)).collect();
    // das Programmfenster (die Bildschirmtastatur kann mit erscheinen)
    let Some((slot, titel)) = neu.iter().rev().find(|(s, t)| !t.starts_with('@') && kreuz_und_titel(*s).is_some()).cloned() else {
        // Programme ohne Fenster (Netzwerk-Test, Download-Test: schreiben
        // nur ins Protokoll und enden): kein Absturz, nichts offen
        assert!(OHNE_FENSTER.contains(&programm), "{programm}: kein neues Fenster mit Kreuz (neu {neu:?})");
        let a = abstuerze();
        assert!(a.is_empty(), "{programm}: Abstürze {a:?}");
        // das Menü hat sich beim Start geschlossen
        l.warte(1500);
        return;
    };
    let (kreuz, titelpunkt, rect) = kreuz_und_titel(slot).unwrap_or_else(|| panic!("{programm} ({titel}): kein Kreuz"));
    // bedienen: Titelleiste ziehen und zurück (Neuzeichnen, Verschieben)
    l.ziehen(titelpunkt.0, titelpunkt.1, titelpunkt.0 + 30, titelpunkt.1 + 20);
    l.warte(1000);
    l.ziehen(titelpunkt.0 + 30, titelpunkt.1 + 20, titelpunkt.0, titelpunkt.1);
    l.warte(1000);
    if browser {
        // Inhalt antippen (Links der Startseite bzw. der geladenen Seite)
        for (fx, fy) in [(1, 3), (1, 2), (2, 3)] {
            l.tipp(rect.0 + rect.2 * fx / 3, rect.1 + rect.3 * fy / 4);
            l.warte(15_000);
        }
        // Startknopf („Home“) aus dem Browser (Fehlerbericht 24.11.): Menü
        // auf, mit einem Tipp in den Browser wieder zu – zwei Mal
        for _ in 0..2 {
            let vorher = fenster_slots().len();
            l.tipp(START.0, START.1);
            l.warte(1500);
            assert!(fenster_slots().len() > vorher, "Menü aus dem Browser");
            l.tipp(rect.0 + rect.2 / 2, rect.1 + rect.3 / 2);
            l.warte(2000);
            assert!(!sys::fenster_liste().iter().any(|f| f.titel == "@menu"), "Menü zu");
        }
    }
    let (kreuz, _, _) = kreuz_und_titel(slot).map_or((kreuz, titelpunkt, rect), |k| k);
    l.tipp(kreuz.0, kreuz.1);
    l.warte(if browser { 4_000 } else { 2_500 });
    // Folgefenster schliessen (Prüfsummen: nach dem Dateidialog das
    // Hauptfenster), höchstens drei
    for _ in 0..3 {
        let Some(k) = fenster_slots()
            .into_iter()
            .filter(|f| !vorher.contains(f) && !f.1.starts_with('@'))
            .find_map(|(s, _)| kreuz_und_titel(s).map(|k| k.0))
        else {
            break;
        };
        l.tipp(k.0, k.1);
        l.warte(2_500);
    }
    let offen = fenster_slots();
    let rest: Vec<_> = offen.iter().filter(|f| !vorher.contains(f) && !f.1.starts_with("@tastatur") && !f.1.starts_with("@notify")).collect();
    assert!(rest.is_empty(), "{programm} ({titel}): nach dem Kreuz noch offen: {rest:?}");
    let a = abstuerze();
    assert!(a.is_empty(), "{programm}: Abstürze {a:?}");
}

/// Startmenü n-mal öffnen und schliessen.
fn menue_auf_zu(l: &mut Lauf, n: usize) {
    for _ in 0..n {
        let vorher = fenster_slots().len();
        l.tipp(START.0, START.1);
        l.warte(800);
        assert!(fenster_slots().len() > vorher, "Menü öffnet nicht");
        // Startknopf noch einmal: Menü zu (unabhängig von der Lage der
        // Desktop-Symbole – am Board traf die „freie Stelle“ ein Symbol)
        l.tipp(START.0, START.1);
        l.warte(800);
        assert_eq!(fenster_slots().len(), vorher, "Menü schliesst nicht");
    }
}

fn board_autopilot(schritte: &[Schritt], runden: usize) -> String {
    let mut ev = Vec::new();
    let mut t = BEGINN_MS;
    for _ in 0..runden {
        for s in schritte {
            match *s {
                Schritt::Tipp(x, y) => {
                    ev.push(format!("{t}:{x}:{y}:1"));
                    ev.push(format!("{}:{x}:{y}:0", t + 120));
                    t += 300;
                }
                Schritt::Ziehen(x0, y0, x1, y1) => {
                    for i in 0..=8 {
                        let (x, y) = (x0 + (x1 - x0) * i / 8, y0 + (y1 - y0) * i / 8);
                        ev.push(format!("{}:{x}:{y}:1", t + i as u64 * 60));
                    }
                    ev.push(format!("{}:{x1}:{y1}:0", t + 600));
                    t += 900;
                }
                Schritt::Warte(ms) => t += ms,
            }
        }
    }
    ev.join(",")
}

#[test]
fn stabilitaetsrunde() {
    // /sys und /rd/1 wie am Board ein Ordner (dort /daten) mit logo.png
    let wurzel = std::env::temp_dir().join(format!("esp32os_stabil_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    std::fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../kolibri/daten/logo.png"), wurzel.join("logo.png")).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    let anwendungen = desktop::anwendungen();
    desktop::shell::anwendungen_setzen(&anwendungen);
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let _ = abstuerze();
    let mut l = Lauf { g: Gesten::default(), t: 5000, schritte: Vec::new() };
    menue_auf_zu(&mut l, 3);
    for a in anwendungen.iter().filter(|a| a.auf_desktop && !a.programm.starts_with('@')) {
        programm(&mut l, "", a.programm);
        menue_auf_zu(&mut l, 2);
    }
    for a in anwendungen.iter().filter(|a| !a.auf_desktop) {
        programm(&mut l, "mehr", a.programm);
        menue_auf_zu(&mut l, 1);
    }
    programm(&mut l, "system", "taskmanager");
    menue_auf_zu(&mut l, 5);
    let ap = board_autopilot(&l.schritte, RUNDEN);
    let dauer: u64 = l.schritte.iter().map(|s| match s {
        Schritt::Tipp(..) => 300,
        Schritt::Ziehen(..) => 900,
        Schritt::Warte(ms) => *ms,
    }).sum();
    eprintln!("Runde am Board: {} s, {RUNDEN} Runden = {} min, {} Ereignisse", dauer / 1000, dauer * RUNDEN as u64 / 60_000, ap.split(',').count());
    assert!(dauer * RUNDEN as u64 >= 30 * 60_000, "Board-Lauf unter 30 min");
    assert!(dauer * RUNDEN_DAUER as u64 >= 60 * 60_000, "Dauertest unter 60 min");
    eprintln!("Dauertest: {RUNDEN_DAUER} Runden = {} min", dauer * RUNDEN_DAUER as u64 / 60_000);
    if std::env::var_os("STABIL_AUTOPILOT_NEU").is_some() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../werkzeuge/board");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stabil_autopilot.txt"), &ap).unwrap();
        std::fs::write(dir.join("dauertest_autopilot.txt"), board_autopilot(&l.schritte, RUNDEN_DAUER)).unwrap();
    }
}
