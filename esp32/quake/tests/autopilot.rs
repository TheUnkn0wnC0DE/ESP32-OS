//! Autopilot mit der echten PAK (falls vorhanden): Quake spielt sich selbst,
//! wechselt alle 120 s Spielzeit das Level (start, e1m1 … e1m8, dann wieder
//! start) bei „Screen size“ 120 und meldet je Wechsel eine Zeile;
//! ausgeschaltet kommt die vorige „Screen size“ zurück.
//!
//! Nur 32 Bit wie das Board (das Spiel selbst läuft in quakegeneric nur
//! dort, 64 Bit reicht für Vorführung und Menü):
//!
//! ```text
//! cargo test --profile sim -q -p quake --target i686-pc-windows-msvc --test autopilot
//! ```
#![cfg(target_pointer_width = "32")]

use core::ffi::{CStr, c_char};
use konsole::{Spiel, Wechsel};
use pudding_core::input::Input;
use quake::autopilot::{KARTEN, LEVELZEIT};
use quake::steuerung::Modus;
use quake::{Pak, Quake};
use std::sync::Mutex;

unsafe extern "C" {
    fn Cvar_VariableValue(name: *const c_char) -> f32;
    fn QG_Karte() -> *const c_char;
}

static ZEILEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn merken(zeile: &str) {
    ZEILEN.lock().unwrap().push(zeile.to_string());
}

fn viewsize() -> f32 {
    // SAFETY: Quake wartet zwischen zwei `update` auf seinen nächsten Auftrag.
    unsafe { Cvar_VariableValue(c"viewsize".as_ptr()) }
}

fn karte() -> String {
    // SAFETY: wie oben; Zeiger auf sv.name bzw. "".
    unsafe { CStr::from_ptr(QG_Karte()) }.to_string_lossy().into_owned()
}

#[test]
fn autopilot_spielt_alle_level_reihum() {
    let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("pak/pak0.pak");
    let Ok(pak) = std::fs::read(&pfad) else {
        eprintln!("pak0.pak fehlt – Test übersprungen");
        return;
    };
    let ordner = std::env::temp_dir().join("quake_autopilot_test");
    let _ = std::fs::remove_dir_all(&ordner);
    std::fs::create_dir_all(ordner.join("id1")).unwrap();
    quake::beim_bericht(merken);
    Quake::virtuelle_zeit();
    let mut q = Quake::starten(
        Pak::Speicher(Box::leak(pak.into_boxed_slice())),
        pfad.to_str().unwrap(),
        ordner.to_str().unwrap(),
    );
    let vorher = viewsize();
    q.autopilot(true);

    // Größter Zeitschritt: 1200 Bilder je Level.
    let dt = 0.1;
    let je_level = (LEVELZEIT / dt) as u32;
    let (mut bilder, mut im_spiel) = (0u32, 0u32);
    let mut besucht = Vec::new();
    for _ in 0..=KARTEN.len() {
        for i in 0..je_level {
            assert!(matches!(q.update(&Input::default(), dt), Wechsel::Bleiben), "Quake will beenden");
            bilder += 1;
            im_spiel += (q.modus() == Modus::Spiel) as u32;
            // 5 s nach dem Wechsel: geladen, aber noch nicht weitergelaufen.
            if i == 50 {
                besucht.push(karte());
                assert_eq!(viewsize(), 120.0, "Screen size im Autopiloten");
            }
        }
    }
    let erwartet: Vec<&str> = KARTEN.iter().chain(&KARTEN[..1]).copied().collect();
    assert_eq!(besucht, erwartet, "Level reihum");
    assert!(im_spiel * 10 >= bilder * 9, "meist im Spiel ({im_spiel} von {bilder})");

    let zeilen = ZEILEN.lock().unwrap().clone();
    for z in &zeilen {
        println!("{z}");
    }
    let wechsel: Vec<&String> = zeilen.iter().filter(|z| z.contains("-> changelevel")).collect();
    assert!(wechsel.len() >= KARTEN.len(), "eine Zeile je Wechsel: {zeilen:#?}");
    for (z, ziel) in wechsel.iter().zip(KARTEN.iter().skip(1).chain(&KARTEN[..1])) {
        assert!(z.ends_with(&format!("-> changelevel {ziel}")), "{z}");
        let hunk: u32 =
            z.split("Hunk frei ").nth(1).and_then(|r| r.split(' ').next()).and_then(|n| n.parse().ok()).expect(z);
        assert!(hunk > 1000, "Hunk frei in KB: {z}");
    }

    q.autopilot(false);
    q.update(&Input::default(), dt);
    assert_eq!(viewsize(), vorher, "Screen size wie vorher");
    assert!(ZEILEN.lock().unwrap().last().unwrap().starts_with("Quake-Autopilot aus"));
}
