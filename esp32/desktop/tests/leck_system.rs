//! M8 (Dauertest 26.11.): Bleibt nach dem Schliessen eines Programms
//! Speicher im System-Heap belegt? Am Board sank der freie System-PSRAM im
//! Dauertest um etwa 110 KB je Runde (alle Programme einmal). Jedes Programm
//! wird mehrmals gestartet und beendet; der erste Durchgang füllt nur
//! Zwischenspeicher (Symbole, Schriften), danach darf je Durchgang nichts
//! mehr im System-Heap bleiben (`kern::system_belegt`).
use kern::{KernAlloc, sys};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Durchgänge je Programm (der erste zählt nicht).
const DURCHGAENGE: usize = 4;
/// Erlaubter mittlerer Zuwachs je Durchgang nach dem ersten (Protokoll-
/// Einträge des Kerns: höchstens 256, dann fest).
const ERLAUBT: isize = 512;

fn ruhig() -> isize {
    // Bereiche beendeter Programme gehen beim Aufräumen frei (zweimal:
    // frei_spaeter wird erst beim nächsten Aufräumen zurückgegeben)
    for _ in 0..4 {
        kern::aufraeumen();
        std::thread::sleep(Duration::from_millis(60));
    }
    kern::system_belegt()
}

/// Kreuz des Fensters im Slot `slot` (Bildschirmkoordinaten).
fn kreuz(slot: usize) -> Option<(i32, i32)> {
    kern::fenster::mit_welt(|w| {
        let f = w.slots.get(slot)?.as_ref()?.fenster.as_ref()?;
        let k = f.knoepfe.iter().find(|k| k.system && k.id == 1)?;
        Some((f.x + k.x + k.w / 2, f.y + k.y + k.h / 2))
    })
}

fn tippen(x: i32, y: i32) {
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(50));
    sys::eingabe::maus(x, y, 0);
    std::thread::sleep(Duration::from_millis(150));
}

/// Starten, Fenster abwarten, über das Kreuz schliessen (wie ein Nutzer;
/// auch Folgefenster), notfalls kooperativ beenden. false = nicht
/// gestartet oder nicht zu beenden.
fn einmal(name: &str) -> bool {
    let Ok(pid) = kern::starte_name(name) else { return false };
    let t = Instant::now();
    while !sys::fenster_liste().iter().any(|f| f.pid == pid) && kern::ende_von(pid).is_none() && t.elapsed() < Duration::from_secs(4) {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(400));
    for _ in 0..4 {
        let Some(slot) = sys::fenster_liste().iter().rev().find(|f| f.pid == pid).map(|f| f.slot) else { break };
        let Some((x, y)) = kreuz(slot) else { break };
        tippen(x, y);
        std::thread::sleep(Duration::from_millis(400));
    }
    let t = Instant::now();
    while kern::liste().iter().any(|p| p.pid == pid) && t.elapsed() < Duration::from_secs(4) {
        kern::aufraeumen();
        std::thread::sleep(Duration::from_millis(20));
    }
    kern::liste().iter().all(|p| p.pid != pid)
}

#[test]
fn kein_leck_im_system_heap() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_leck_{}", std::process::id()));
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
    let mut programme: Vec<&str> = anwendungen.iter().map(|a| a.programm).filter(|p| !p.starts_with('@')).collect();
    programme.extend(["@menu", "@meldung"]);
    let mut zeilen = Vec::new();
    let mut lecks = Vec::new();
    for name in programme {
        let mut stand = Vec::new();
        for _ in 0..DURCHGAENGE {
            let vorher = ruhig();
            if !einmal(name) {
                break;
            }
            stand.push(ruhig() - vorher);
        }
        if stand.len() < DURCHGAENGE {
            zeilen.push(format!("{name:16} nicht gestartet"));
            continue;
        }
        let danach: isize = stand[1..].iter().sum::<isize>() / (DURCHGAENGE as isize - 1);
        zeilen.push(format!("{name:16} erster {:>8} B, danach je Durchgang {:>7} B  {stand:?}", stand[0], danach));
        // Leck: jeder Durchgang nach dem ersten lässt etwas zurück (ein
        // einzelner Wert ist ein spät gefüllter Zwischenspeicher)
        if stand[1..].iter().all(|&d| d > 0) && danach > ERLAUBT {
            lecks.push(format!("{name}: {danach} B je Durchgang"));
        }
    }
    println!("System-Heap je Programm (Start und Ende):\n{}", zeilen.join("\n"));
    let a = kern::neue_abstuerze();
    assert!(a.is_empty(), "Abstürze {a:?}");
    assert!(lecks.is_empty(), "Leck im System-Heap: {lecks:?}\n{}", zeilen.join("\n"));
    let _ = std::fs::remove_dir_all(&wurzel);
}
