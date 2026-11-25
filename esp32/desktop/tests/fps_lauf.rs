//! S3: Bildrate der Oberfläche messen. Am PC wird jeder Schritt geprüft
//! (Fenster da/weg, Liste blättert, Tastatur erscheint, kein Absturz);
//! dieselben Schritte mit Board-Wartezeiten ergeben den Autopiloten für den
//! Board-Lauf (`S3_AUTOPILOT_NEU=1` schreibt `werkzeuge/board/s3_autopilot.txt`
//! und die Phasen `s3_phasen.txt`: Name, Beginn, Ende in ms ab Desktop-Start).
//! Am Board liefert das Log alle 2 s „Desktop: … Bilder/s neu“; die
//! Auswertung (`s3_auswertung.py`) mittelt das je Phase.
//!
//! Bedient wird nur Harmloses: Menü, Rechner, Blättern im Dateimanager
//! (Wischen, nie Antippen eines Eintrags), Tippen in den Notizen (Schliessen
//! mit dem X speichert nichts), Blättern im Browser.
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

const START: (i32, i32) = (40, 340);
/// Beginn am Board: Desktop steht, WLAN verbunden.
const BEGINN_MS: u64 = 25_000;
/// Abstand der Fingerpunkte beim Wischen (Board-Zeit).
const TAKT_MS: u64 = 16;

#[derive(Clone, Debug)]
enum Schritt {
    Tipp(i32, i32),
    /// Q1: schneller Tipp (80 ms gedrückt, alle 150 ms) – Tastatur-Bildrate
    TippSchnell(i32, i32),
    /// Senkrecht wischen bei x von y0 nach y1 in `ms`.
    Wischen(i32, i32, i32, u64),
    Warte(u64),
    Phase(&'static str),
    PhaseEnde,
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
    fn tipp_schnell(&mut self, x: i32, y: i32) {
        self.schritte.push(Schritt::TippSchnell(x, y));
        self.takt([Some((x, y)), None], 80);
        for _ in 0..4 {
            self.takt([None, None], 20);
        }
    }

    fn tipp(&mut self, x: i32, y: i32) {
        self.schritte.push(Schritt::Tipp(x, y));
        self.takt([Some((x, y)), None], 120);
        for _ in 0..10 {
            self.takt([None, None], 20);
        }
    }
    fn wischen(&mut self, x: i32, y0: i32, y1: i32, ms: u64) {
        self.schritte.push(Schritt::Wischen(x, y0, y1, ms));
        let n = (ms / TAKT_MS).max(1) as i32;
        for i in 0..=n {
            self.takt([Some((x, y0 + (y1 - y0) * i / n)), None], TAKT_MS);
        }
        for _ in 0..10 {
            self.takt([None, None], 20);
        }
    }
    fn warte(&mut self, board_ms: u64) {
        self.schritte.push(Schritt::Warte(board_ms));
        std::thread::sleep(Duration::from_millis((board_ms / 4).clamp(200, 2500)));
    }
    fn phase(&mut self, name: &'static str) {
        self.schritte.push(Schritt::Phase(name));
    }
    fn phase_ende(&mut self) {
        self.schritte.push(Schritt::PhaseEnde);
    }
}

fn fenster_slots() -> Vec<(usize, String)> {
    sys::fenster_liste().into_iter().map(|f| (f.slot, f.titel)).collect()
}

/// Kreuz, Rechteck und grösster Knopf (Inhalt, z. B. Textfeld) eines Fensters.
fn fenster_teile(slot: usize) -> Option<((i32, i32), (i32, i32, i32, i32), Option<(i32, i32)>)> {
    kern::fenster::mit_welt(|w| {
        let f = w.slots.get(slot)?.as_ref()?.fenster.as_ref()?;
        let k = f.knoepfe.iter().find(|k| k.system && k.id == 1)?;
        let gross = f.knoepfe.iter().filter(|k| !k.system).max_by_key(|k| k.w * k.h).map(|k| (f.x + k.x + k.w / 2, f.y + k.y + k.h / 2));
        Some(((f.x + k.x + k.w / 2, f.y + k.y + k.h / 2), (f.x, f.y, f.w, f.h), gross))
    })
}

fn abstuerze() -> Vec<String> {
    kern::neue_abstuerze().into_iter().map(|(n, e)| format!("{n}: {e:?}")).collect()
}

/// Programm aus dem Startmenü öffnen; Slot seines Fensters.
fn oeffnen(l: &mut Lauf, programm: &str, board_ms: u64) -> usize {
    let vorher = fenster_slots();
    l.tipp(START.0, START.1);
    l.warte(1200);
    let (x, y) = desktop::shell::menue_eintrag_mitte_in("", programm).unwrap_or_else(|| panic!("{programm} im Menü"));
    l.tipp(x, y);
    l.warte(board_ms);
    let neu: Vec<(usize, String)> = fenster_slots().into_iter().filter(|f| !vorher.contains(f)).collect();
    let (slot, _) = neu.iter().rev().find(|(s, t)| !t.starts_with('@') && fenster_teile(*s).is_some()).cloned().unwrap_or_else(|| panic!("{programm}: kein Fenster (neu {neu:?})"));
    slot
}

fn schliessen(l: &mut Lauf, slot: usize, programm: &str) {
    let (kreuz, _, _) = fenster_teile(slot).unwrap_or_else(|| panic!("{programm}: kein Kreuz"));
    l.tipp(kreuz.0, kreuz.1);
    l.warte(2500);
    assert!(!fenster_slots().iter().any(|f| f.0 == slot && !f.1.starts_with('@')), "{programm}: nach dem Kreuz noch offen");
    let a = abstuerze();
    assert!(a.is_empty(), "{programm}: Abstürze {a:?}");
}

fn board_autopilot(schritte: &[Schritt]) -> (String, Vec<(String, u64, u64)>) {
    let mut ev = Vec::new();
    let mut phasen = Vec::new();
    let mut offen: Option<(&str, u64)> = None;
    let mut t = BEGINN_MS;
    for s in schritte {
        match *s {
            Schritt::Tipp(x, y) => {
                ev.push(format!("{t}:{x}:{y}:1"));
                ev.push(format!("{}:{x}:{y}:0", t + 120));
                t += 300;
            }
            Schritt::TippSchnell(x, y) => {
                ev.push(format!("{t}:{x}:{y}:1"));
                ev.push(format!("{}:{x}:{y}:0", t + 80));
                t += 150;
            }
            Schritt::Wischen(x, y0, y1, ms) => {
                let n = (ms / TAKT_MS).max(1);
                for i in 0..=n {
                    let y = y0 + (y1 - y0) * i as i32 / n as i32;
                    ev.push(format!("{}:{x}:{y}:1", t + i * TAKT_MS));
                }
                ev.push(format!("{}:{x}:{y1}:0", t + (n + 1) * TAKT_MS));
                t += (n + 1) * TAKT_MS + 250;
            }
            Schritt::Warte(ms) => t += ms,
            Schritt::Phase(name) => offen = Some((name, t)),
            Schritt::PhaseEnde => {
                let (name, a) = offen.take().expect("Phase ohne Anfang");
                phasen.push((name.to_string(), a, t));
            }
        }
    }
    (ev.join(","), phasen)
}

#[test]
fn bildrate_ablauf() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_s3_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let sd = wurzel.join("sd");
    std::fs::create_dir_all(&sd).unwrap();
    for i in 0..8 {
        std::fs::create_dir_all(sd.join(format!("ordner_{i}"))).unwrap();
    }
    for i in 0..40 {
        let endung = ["txt", "png", "mp3", "pdf", "bmp"][i % 5];
        std::fs::write(sd.join(format!("datei_{i:02}.{endung}")), b"x").unwrap();
    }
    std::fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../kolibri/daten/logo.png"), wurzel.join("logo.png")).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    kern::datei::einhaengen("/sd0/1", &sd);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let _ = abstuerze();
    let mut l = Lauf { g: Gesten::default(), t: 5000, schritte: Vec::new() };

    // Ruhe (Vergleich: keine neuen Bilder)
    l.phase("ruhe");
    l.warte(6000);
    l.phase_ende();

    // Startmenü auf und zu
    l.phase("menue");
    for _ in 0..6 {
        let n = fenster_slots().len();
        l.tipp(START.0, START.1);
        l.warte(700);
        assert!(fenster_slots().len() > n, "Menü öffnet nicht");
        l.tipp(START.0, START.1);
        l.warte(700);
        assert_eq!(fenster_slots().len(), n, "Menü schliesst nicht");
    }
    l.phase_ende();

    // Fenster öffnen und schliessen (Rechner)
    l.phase("fenster");
    for _ in 0..3 {
        let s = oeffnen(&mut l, "rechner", 2500);
        schliessen(&mut l, s, "rechner");
    }
    l.phase_ende();

    // Dateimanager: Liste wischen (runter und rauf), nie antippen
    let s = oeffnen(&mut l, "dateimanager", 3000);
    let (_, (fx, fy, fw, fh), _) = fenster_teile(s).unwrap();
    let titel_vorher = fenster_slots().into_iter().find(|f| f.0 == s).map(|f| f.1);
    let lx = fx + fw * 2 / 3;
    l.phase("dateimanager");
    for _ in 0..6 {
        l.wischen(lx, fy + fh - 40, fy + 110, 500);
        l.wischen(lx, fy + 110, fy + fh - 40, 500);
    }
    l.phase_ende();
    assert_eq!(fenster_slots().into_iter().find(|f| f.0 == s).map(|f| f.1), titel_vorher, "Wischen hat einen Ordner geöffnet");
    assert!(fenster_slots().len() == fenster_slots().iter().filter(|f| f.0 != s).count() + 1, "Wischen hat etwas geöffnet");
    schliessen(&mut l, s, "dateimanager");

    // Notizen: Textfeld antippen, auf der Bildschirmtastatur tippen
    let s = oeffnen(&mut l, "notizen", 3000);
    let (_, _, text) = fenster_teile(s).unwrap();
    let text = text.expect("Textfeld der Notizen");
    l.tipp(text.0, text.1);
    l.warte(1500);
    assert!(sys::tastatur_soll().0, "Tastatur erscheint");
    let y_unten = 360 - desktop::shell::LEISTE_H - desktop::tastatur::HOEHE;
    let oben = match sys::tastatur_soll().1 {
        Some(c) if c.y1 > y_unten && c.y0 >= desktop::tastatur::HOEHE => 0,
        Some(c) if c.y1 > y_unten => (c.y1 + 2).min(360 - desktop::tastatur::HOEHE),
        _ => y_unten,
    };
    l.phase("tastatur");
    for c in "das ist ein test der tastatur".chars() {
        let (ebene, x, y) = desktop::tastatur::tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        assert_eq!(ebene, "normal");
        l.tipp(x, oben + y);
        l.warte(100);
    }
    l.phase_ende();
    // Q1: schnelles Tippen (6–7 Tasten/s) – Bildrate der Tastatur selbst
    l.phase("tastatur_schnell");
    for c in "das ist ein test der tastatur das ist ein test der tastatur".chars() {
        let (_, x, y) = desktop::tastatur::tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        l.tipp_schnell(x, oben + y);
    }
    l.phase_ende();
    schliessen(&mut l, s, "notizen");
    l.warte(1500);

    // Browser: Startseite, dann wischen
    let s = oeffnen(&mut l, "browser", 20_000);
    let (_, (bx, by, bw, bh), _) = fenster_teile(s).unwrap();
    l.phase("browser");
    for _ in 0..6 {
        l.wischen(bx + bw / 2, by + bh - 40, by + 90, 500);
        l.wischen(bx + bw / 2, by + 90, by + bh - 40, 500);
    }
    l.phase_ende();
    schliessen(&mut l, s, "browser");
    let a = abstuerze();
    assert!(a.is_empty(), "Abstürze {a:?}");

    let (ap, phasen) = board_autopilot(&l.schritte);
    let ende = phasen.last().map_or(0, |p| p.2);
    eprintln!("S3-Lauf am Board: {} s, {} Ereignisse", ende / 1000, ap.split(',').count());
    for (n, a, e) in &phasen {
        eprintln!("  {n}: {a}–{e} ms");
        assert!(e - a >= 6000, "Phase {n} kürzer als 6 s (Log alle 2 s)");
    }
    if std::env::var_os("S3_AUTOPILOT_NEU").is_some() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../werkzeuge/board");
        std::fs::write(dir.join("s3_autopilot.txt"), &ap).unwrap();
        let p: Vec<String> = phasen.iter().map(|(n, a, e)| format!("{n} {a} {e}")).collect();
        std::fs::write(dir.join("s3_phasen.txt"), p.join("\n") + "\n").unwrap();
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}
