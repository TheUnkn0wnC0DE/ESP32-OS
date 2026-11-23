//! S0-Nachweis (PC): Spiele als Programme im Desktop – je ein Symbol auf
//! dem Desktop, im Startmenü der Ordner „Spiele...“ mit den vier Spielen,
//! kein Eintrag für den alten Launcher. Antippen startet das Spiel über den
//! Haken der Firmware (am Board: Startziel ins NVS, Neustart).
//! S5: Fehlen die Spieldaten von Doom/Quake (Shareware, nicht in ESP32-OS),
//! kommt statt des Starts ein Hinweis mit Lizenz und Ablageort.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use konsole::SpielId;
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Zuletzt gestartetes Spiel (+1; 0 = keins).
static GESTARTET: AtomicU32 = AtomicU32::new(0);

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}; Fenster {:?}", sys::fenster_liste().iter().map(|w| w.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn tippen(&mut self, (x, y): (i32, i32)) {
        let mut d = 0;
        while d <= 80 {
            self.g.schritt(self.t + d, [Some((x, y)), None]);
            d += 50;
            std::thread::sleep(Duration::from_millis(5));
        }
        self.t += 130;
        self.g.schritt(self.t, [None, None]);
        std::thread::sleep(Duration::from_millis(60));
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        self.t += 600;
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn menue_offen() -> bool {
    sys::fenster_liste().iter().any(|w| w.titel == "ESP32-OS")
}

#[test]
fn spiele_im_desktop() {
    let wurzel = std::env::temp_dir().join("esp32os_s0_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    let sd = wurzel.join("sd");
    std::fs::create_dir_all(&sd).unwrap();
    kern::datei::einhaengen("/sd0/1", &sd);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    desktop::shell::haken_setzen(desktop::shell::Haken { neustart: || {}, spiel: |id| GESTARTET.store(u32::from(id) + 1, Relaxed) });
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));

    // vier Symbole auf dem Desktop, kein Launcher-Eintrag
    let alle = desktop::anwendungen();
    for p in ["@spiel_pudding", "@spiel_wuerfelwelt", "@spiel_doom", "@spiel_quake"] {
        assert!(desktop::shell::symbol_mitte(p).is_some(), "Desktop-Symbol {p}");
    }
    assert!(!alle.iter().any(|a| a.programm == "@konsole"), "kein Launcher mehr");
    foto("s0_desktop");

    // Startmenü: Spiele nur im Ordner
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    assert!(desktop::shell::menue_eintrag_mitte("@spiel_doom").is_none(), "Spiele nicht im Hauptmenü");
    f.tippen((40, 340));
    bis(menue_offen, "Startmenü");
    foto("s0_menue");
    f.tippen(desktop::shell::menue_eintrag_mitte("@spiele").unwrap());
    std::thread::sleep(Duration::from_millis(600));
    bis(menue_offen, "Ordner Spiele");
    foto("s0_ordner_spiele");
    // S5: ohne DOOM1.WAD kein Start, sondern der Hinweis
    f.tippen(desktop::shell::menue_eintrag_mitte_in("spiele", "@spiel_doom").unwrap());
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Game data missing"), "Hinweis Spieldaten");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(GESTARTET.load(Relaxed), 0, "ohne Spieldaten kein Start");
    let (_, text) = desktop::shell::spieldaten_fehlen(SpielId::Doom).unwrap();
    assert!(text.contains("DOOM1.WAD") && text.contains("id Software") && text.contains("ESP32-OS/doom/"), "{text}");
    assert!(desktop::shell::spieldaten_fehlen(SpielId::Quake).is_some());
    assert!(desktop::shell::spieldaten_fehlen(SpielId::Pudding).is_none(), "Pudding braucht nichts");
    foto("s5_hinweis");
    // Hinweis schliessen (Knopf 1 des Fensters)
    let zu = kern::kern_abschnitt(|| {
        kern::fenster::mit_welt(|w| {
            let s = w.fenster_liste().into_iter().find(|f| f.titel == "Game data missing").unwrap().slot;
            let fe = w.slots[s].as_ref().unwrap().fenster.as_ref().unwrap();
            let k = fe.knoepfe.iter().find(|k| k.id == 1).unwrap();
            (fe.x + k.x + k.w / 2, fe.y + k.y + k.h / 2)
        })
    });
    f.tippen(zu);
    bis(|| !sys::fenster_liste().iter().any(|w| w.titel == "Game data missing"), "Hinweis zu");

    // Spieldaten auf der SD-Karte → Doom startet über das Menü
    std::fs::create_dir_all(sd.join("ESP32-OS/doom")).unwrap();
    std::fs::create_dir_all(sd.join("ESP32-OS/quake")).unwrap();
    std::fs::write(sd.join("ESP32-OS/doom/DOOM1.WAD"), b"IWAD").unwrap();
    std::fs::write(sd.join("ESP32-OS/quake/pak0.pak"), b"PACK").unwrap();
    assert!(desktop::shell::spieldaten_fehlen(SpielId::Doom).is_none() && desktop::shell::spieldaten_fehlen(SpielId::Quake).is_none());
    f.tippen((40, 340));
    bis(menue_offen, "Startmenü");
    f.tippen(desktop::shell::menue_eintrag_mitte("@spiele").unwrap());
    std::thread::sleep(Duration::from_millis(600));
    bis(menue_offen, "Ordner Spiele");
    f.tippen(desktop::shell::menue_eintrag_mitte_in("spiele", "@spiel_doom").unwrap());
    bis(|| GESTARTET.load(Relaxed) == u32::from(SpielId::Doom.als_u8()) + 1, "Doom über das Menü");

    // Desktop-Symbole: jedes Spiel startet
    for (p, id) in [("@spiel_pudding", SpielId::Pudding), ("@spiel_wuerfelwelt", SpielId::Wuerfelwelt), ("@spiel_quake", SpielId::Quake)] {
        GESTARTET.store(0, Relaxed);
        bis(|| !menue_offen(), "Menü zu");
        f.tippen(desktop::shell::symbol_mitte(p).unwrap());
        bis(|| GESTARTET.load(Relaxed) == u32::from(id.als_u8()) + 1, p);
    }
}
