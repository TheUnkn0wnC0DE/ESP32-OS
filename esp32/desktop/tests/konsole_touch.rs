//! C1 per Touch: Startmenü → Mehr Programme → Konsole, Befehle über die
//! Bildschirmtastatur tippen (Fingertipps über die Gesten), Ergebnis an der
//! Datei prüfen (touch/rm), Konsole und Matrix über das Kreuz schliessen.
//! Dieselben Tipps mit Board-Wartezeiten ergeben den Autopilot für den
//! Board-Nachweis (`C1_AUTOPILOT_NEU=1` schreibt
//! `werkzeuge/board/c1_autopilot.txt` und die Fotozeiten
//! `werkzeuge/board/c1_fotos.txt`).
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, sondertaste, tastenmitte};
use kern::{KernAlloc, sys};
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

const START: (i32, i32) = (40, 340);
const BEGINN_MS: u64 = 25_000;

struct Lauf {
    g: Gesten,
    t: u64,
    /// Board-Zeitleiste: (ms, x, y) je Tipp; Fotos (s)
    board: Vec<(u64, i32, i32)>,
    board_ms: u64,
    fotos: Vec<u64>,
}

impl Lauf {
    fn tipp(&mut self, x: i32, y: i32, board_warte: u64) {
        self.board.push((self.board_ms, x, y));
        self.board_ms += board_warte;
        self.g.schritt(self.t, [Some((x, y)), None]);
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        for _ in 0..8 {
            self.t += 20;
            self.g.schritt(self.t, [None, None]);
            std::thread::sleep(Duration::from_millis(15));
        }
        std::thread::sleep(Duration::from_millis((board_warte / 6).clamp(40, 1200)));
    }

    fn foto(&mut self) {
        self.fotos.push(self.board_ms / 1000 + 1);
        self.board_ms += 9_000; // Foto-Übertragung blockiert ~7 s
    }
}

/// Lage der Bildschirmtastatur (wie tastatur::dienst).
fn tastatur_oben() -> i32 {
    let y_unten = 360 - desktop::shell::LEISTE_H - HOEHE;
    match sys::tastatur_soll().1 {
        Some(f) if f.y1 > y_unten && f.y0 >= HOEHE => 0,
        Some(f) if f.y1 > y_unten => (f.y1 + 2).min(360 - HOEHE),
        _ => y_unten,
    }
}

fn tippe(l: &mut Lauf, text: &str) {
    for c in text.chars() {
        let oben = tastatur_oben();
        let (x, y) = if c == '\n' {
            sondertaste("enter", 640)
        } else {
            let (ebene, x, y) = tastenmitte(c, 640).unwrap_or_else(|| panic!("Taste {c:?}"));
            // D1: Sonderzeichen (|) über die Ebene „123“, danach zurück
            assert!(ebene == "normal" || ebene == "sonder", "Kleinbuchstaben, Leerzeichen, Sonderzeichen: {c:?}");
            if ebene == "sonder" {
                let (sx, sy) = sondertaste("123", 640);
                l.tipp(sx, oben + sy, 300);
                l.tipp(x, oben + y, 300);
                let (sx, sy) = sondertaste("123", 640);
                l.tipp(sx, oben + sy, 300);
                continue;
            }
            (x, y)
        };
        l.tipp(x, oben + y, if c == '\n' { 1500 } else { 300 });
    }
}

fn kreuz_von(titel: &str) -> (i32, i32) {
    kern::fenster::mit_welt(|w| {
        for &s in w.stapel.iter().rev() {
            let Some(f) = w.slots[s].as_ref().and_then(|z| z.fenster.as_ref()) else { continue };
            if f.titel.starts_with(titel)
                && let Some(k) = f.knoepfe.iter().find(|k| k.system && k.id == 1)
            {
                return (f.x + k.x + k.w / 2, f.y + k.y + k.h / 2);
            }
        }
        panic!("Fenster {titel} mit Kreuz")
    })
}

fn titel() -> Vec<String> {
    sys::fenster_liste().into_iter().map(|f| f.titel).collect()
}

fn aus_mehr(l: &mut Lauf, programm: &str) {
    l.tipp(START.0, START.1, 1200);
    let (x, y) = desktop::shell::menue_eintrag_mitte("@mehr").expect("Mehr Programme");
    l.tipp(x, y, 1200);
    let (x, y) = desktop::shell::menue_eintrag_mitte_in("mehr", programm).expect(programm);
    l.tipp(x, y, 4000);
}

#[test]
fn konsole_per_touch() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_c1t_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let _ = kern::neue_abstuerze();
    let mut l = Lauf { g: Gesten::default(), t: 5000, board: Vec::new(), board_ms: BEGINN_MS, fotos: Vec::new() };

    aus_mehr(&mut l, "shell");
    assert!(titel().iter().any(|t| t.starts_with("SHELL")), "Konsole offen: {:?}", titel());
    assert!(sys::tastatur_soll().0, "Bildschirmtastatur erscheint");
    tippe(&mut l, "ver\n");
    tippe(&mut l, "touch neu\n");
    assert!(wurzel.join("neu").exists(), "per Touch getipptes touch legt die Datei an");
    tippe(&mut l, "ls\n");
    l.foto();
    tippe(&mut l, "rm neu\n");
    assert!(!wurzel.join("neu").exists(), "per Touch getipptes rm löscht die Datei");
    // D1 am Board: Pipe und neue Befehle (nur lesen, nichts schreiben)
    tippe(&mut l, "clear\n");
    tippe(&mut l, "ls | wc\n");
    tippe(&mut l, "help sort\n");
    l.foto();
    let (x, y) = kreuz_von("SHELL");
    l.tipp(x, y, 2500);
    assert!(!titel().iter().any(|t| t.starts_with("SHELL")), "Konsole zu: {:?}", titel());

    aus_mehr(&mut l, "matrix");
    assert!(titel().iter().any(|t| t.starts_with("Matrix")), "Matrix offen: {:?}", titel());
    l.board_ms += 4000;
    l.foto();
    let (x, y) = kreuz_von("Matrix");
    l.tipp(x, y, 2500);
    assert!(!titel().iter().any(|t| t.starts_with("Matrix")), "Matrix zu: {:?}", titel());
    let a = kern::neue_abstuerze();
    assert!(a.is_empty(), "Abstürze {a:?}");

    if std::env::var_os("C1_AUTOPILOT_NEU").is_some() {
        let ap: Vec<String> = l.board.iter().flat_map(|&(t, x, y)| [format!("{t}:{x}:{y}:1"), format!("{}:{x}:{y}:0", t + 120)]).collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../werkzeuge/board");
        std::fs::write(dir.join("c1_autopilot.txt"), ap.join(",")).unwrap();
        std::fs::write(dir.join("c1_fotos.txt"), l.fotos.iter().map(u64::to_string).collect::<Vec<_>>().join(",")).unwrap();
        eprintln!("Board-Lauf {} s, {} Tipps, Fotos {:?}", l.board_ms / 1000, l.board.len(), l.fotos);
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}
