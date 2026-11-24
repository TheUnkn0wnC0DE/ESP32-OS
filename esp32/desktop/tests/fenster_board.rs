//! S5 am Board: jedes Programm über das Startmenü öffnen, Bildschirmfoto,
//! über das Kreuz schliessen. Am PC wird jeder Schritt geprüft (Fenster da,
//! Titelleiste und Rahmen sauber wie in `fenster_raender.rs`, wieder zu);
//! `S5_AUTOPILOT_NEU=1` schreibt `werkzeuge/board/s5_autopilot.txt` und die
//! Fotozeiten `s5_fotos.txt` (ein Foto je Programm, Reihenfolge in
//! `s5_programme.txt`).
//!
//! Spiele bleiben aussen vor (starten ein anderes Abbild), ebenso
//! Programme ohne Fenster. Bedient wird nichts ausser Menü und Kreuz.
#![cfg(target_pointer_width = "32")]
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

const START: (i32, i32) = (40, 340);
const BEGINN_MS: u64 = 25_000;
const OHNE_FENSTER: &[&str] = &["http_tcp_demo", "netz_laden"];

struct Lauf {
    g: Gesten,
    t: u64,
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
        std::thread::sleep(Duration::from_millis((board_warte / 5).clamp(40, 2500)));
    }
    fn foto(&mut self) {
        self.fotos.push(self.board_ms / 1000 + 1);
        self.board_ms += 9_000; // Foto-Übertragung blockiert ~7 s
    }
}

fn kreuz(slot: usize) -> Option<(i32, i32)> {
    kern::fenster::mit_welt(|w| {
        let f = w.slots.get(slot)?.as_ref()?.fenster.as_ref()?;
        let k = f.knoepfe.iter().find(|k| k.system && k.id == 1)?;
        Some((f.x + k.x + k.w / 2, f.y + k.y + k.h / 2))
    })
}

/// Titelleiste und Rahmen gleich der Neuzeichnung des Kerns?
fn sauber(slot: usize) -> bool {
    kern::fenster::mit_welt(|w| {
        let skin = w.skin.clone();
        let farbe = w.farben[4];
        let aktiv = w.aktiv() == Some(slot);
        let Some(f) = w.slots.get_mut(slot).and_then(|s| s.as_mut()).and_then(|s| s.fenster.as_mut()) else { return false };
        if !matches!(f.stil, 3 | 4) {
            return true;
        }
        let ist = f.buf.clone();
        kern::fenster::rahmen_zeichnen(f, &skin, aktiv);
        kern::fenster::kopf_zeichnen(f, &skin, aktiv, farbe);
        let soll = std::mem::replace(&mut f.buf, ist);
        let (bw, bh, k, r) = (f.w as usize, f.h as usize, skin.hoehe as usize, kern::fenster::RAHMEN as usize);
        (0..bh).all(|y| (0..bw).all(|x| (y >= k && x >= r && x < bw - r && y < bh - r) || f.buf.px[y * bw + x] == soll.px[y * bw + x]))
    })
}

#[test]
fn alle_programme_fuer_das_board() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_s5b_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    std::fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../kolibri/daten/logo.png"), wurzel.join("logo.png")).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    for p in ["/sys", "/rd/1", "/sd0/1"] {
        kern::datei::einhaengen(p, &wurzel);
    }
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    let anwendungen = desktop::anwendungen();
    desktop::shell::anwendungen_setzen(&anwendungen);
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    let _ = kern::neue_abstuerze();
    let mut l = Lauf { g: Gesten::default(), t: 5000, board: Vec::new(), board_ms: BEGINN_MS, fotos: Vec::new() };
    let mut liste: Vec<(&str, &str)> = anwendungen.iter().filter(|a| a.auf_desktop && !a.programm.starts_with('@')).map(|a| ("", a.programm)).collect();
    liste.extend(anwendungen.iter().filter(|a| !a.auf_desktop && !OHNE_FENSTER.contains(&a.programm)).map(|a| ("mehr", a.programm)));
    liste.push(("system", "taskmanager"));
    let mut namen = Vec::new();
    for (ordner, programm) in liste {
        let vorher: Vec<usize> = sys::fenster_liste().iter().map(|f| f.slot).collect();
        l.tipp(START.0, START.1, 1200);
        if !ordner.is_empty() {
            let (x, y) = desktop::shell::menue_eintrag_mitte(&format!("@{ordner}")).unwrap_or_else(|| panic!("Ordner {ordner}"));
            l.tipp(x, y, 1200);
        }
        let (x, y) = desktop::shell::menue_eintrag_mitte_in(ordner, programm).unwrap_or_else(|| panic!("{programm} im Menü"));
        l.tipp(x, y, if programm == "browser" { 20_000 } else { 5_000 });
        let neu: Vec<(usize, String)> = sys::fenster_liste().into_iter().filter(|f| !vorher.contains(&f.slot) && !f.titel.starts_with('@')).map(|f| (f.slot, f.titel)).collect();
        assert!(!neu.is_empty(), "{programm}: kein Fenster");
        for (s, t) in &neu {
            assert!(sauber(*s), "{programm} ({t}): Titelleiste oder Rahmen nicht sauber");
        }
        l.foto();
        namen.push(programm.to_string());
        // alle neuen Fenster über das Kreuz schliessen (zuletzt geöffnetes zuerst)
        for _ in 0..4 {
            let Some(s) = sys::fenster_liste().into_iter().filter(|f| !vorher.contains(&f.slot) && !f.titel.starts_with('@')).map(|f| f.slot).last() else { break };
            let Some((x, y)) = kreuz(s) else { break };
            l.tipp(x, y, 2500);
        }
        let rest: Vec<String> = sys::fenster_liste().into_iter().filter(|f| !vorher.contains(&f.slot) && !f.titel.starts_with('@')).map(|f| f.titel).collect();
        assert!(rest.is_empty(), "{programm}: nach dem Kreuz noch offen: {rest:?}");
        let a = kern::neue_abstuerze();
        assert!(a.is_empty(), "{programm}: Abstürze {a:?}");
    }
    eprintln!("S5-Board-Lauf {} s, {} Programme, {} Tipps", l.board_ms / 1000, namen.len(), l.board.len());
    if std::env::var_os("S5_AUTOPILOT_NEU").is_some() {
        let ap: Vec<String> = l.board.iter().flat_map(|&(t, x, y)| [format!("{t}:{x}:{y}:1"), format!("{}:{x}:{y}:0", t + 120)]).collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../werkzeuge/board");
        std::fs::write(dir.join("s5_autopilot.txt"), ap.join(",")).unwrap();
        std::fs::write(dir.join("s5_fotos.txt"), l.fotos.iter().map(u64::to_string).collect::<Vec<_>>().join(",")).unwrap();
        std::fs::write(dir.join("s5_programme.txt"), namen.join("\n") + "\n").unwrap();
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}
