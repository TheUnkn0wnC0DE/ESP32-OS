//! A6-Nachweis (PC): Taskmanager aus dem Startmenü per Touch, Liste mit
//! Fäden und Speicher, Programm antippen, „Beenden“. Das Testprogramm
//! `endlostest` ruft keine Systemfunktion mehr: kooperativ endet es nie,
//! nach 2 s greift das Zwangsbeenden. Am PC lassen sich Threads nicht
//! anhalten – das Fenster verschwindet, der Thread läuft weiter, bis der Test
//! ihn freigibt (Meldung „liess sich nicht anhalten“); das echte Anhalten
//! prüft `kern/tests/zwang.rs` und am Board der Autopilot. Ausserdem:
//! Systemdienste lassen sich nicht beenden, Beenden ohne Auswahl.

use desktop::gesten::Gesten;
use desktop::taskmanager::{BEENDEN, FX, FY, LISTE_Y, ZEILE_H};
use kern::{KernAlloc, sys};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}; Fenster: {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn tippen(&mut self, x: i32, y: i32) {
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
        std::thread::sleep(Duration::from_millis(220));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn da(titel: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel == titel)
}

fn laeuft(name: &str) -> bool {
    kern::liste().iter().any(|p| p.name == name)
}

/// Arbeitsbereich des Taskmanagers (Rand 5, Kopf 48).
const AX: i32 = FX + 5;
const AY: i32 = FY + 48;

fn zeile(i: usize) -> (i32, i32) {
    (AX + 120, AY + LISTE_Y + i as i32 * ZEILE_H + ZEILE_H / 2)
}

fn beenden() -> (i32, i32) {
    (AX + BEENDEN.0 + BEENDEN.2 / 2, AY + BEENDEN.1 + BEENDEN.3 / 2)
}

/// Statuszeile (links vom Knopf) zeigt roten Text (Hinweis/Fehler)?
fn status_rot() -> bool {
    let (w, _, px) = sys::foto();
    let y0 = (AY + BEENDEN.1) as usize;
    (y0..y0 + BEENDEN.3 as usize).any(|y| {
        (AX as usize + 8..AX as usize + 420).any(|x| {
            let p = px[y * w as usize + x];
            let (r, g, b) = ((p >> 16 & 255) as i32, (p >> 8 & 255) as i32, (p & 255) as i32);
            r > g + 80 && r > b + 80
        })
    })
}

#[test]
fn taskmanager_beendet_haengendes_programm() {
    let wurzel = std::env::temp_dir().join("esp32os_a6_test");
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    std::fs::write(wurzel.join("settings/esp32os.ini"), "[system]\nsetup=1\n").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(2500));

    // Hängendes Programm (nicht im Menü)
    kern::starte_name("endlostest").unwrap();
    bis(|| da("Endless loop"), "Endlostest offen");

    // Taskmanager über das Startmenü
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    f.tippen(40, 340);
    bis(|| da("ESP32-OS"), "Startmenü");
    // Ordner „System“ im Startmenü, darin der Eintrag
    let (x, y) = desktop::shell::menue_eintrag_mitte("@system").unwrap();
    f.tippen(x, y);
    std::thread::sleep(Duration::from_millis(600));
    let (x, y) = desktop::shell::menue_eintrag_mitte_in("system", "taskmanager").unwrap();
    f.tippen(x, y);
    bis(|| da("Task manager"), "Taskmanager offen");
    let liste = desktop::taskmanager::liste();
    assert_eq!(liste[0].name, "endlostest", "Anwendungen zuerst: {:?}", liste.iter().map(|p| p.name).collect::<Vec<_>>());
    assert_eq!(liste[1].name, "taskmanager");
    assert!(liste.iter().any(|p| p.name == "@taskbar"));
    assert!(liste.iter().all(|p| p.heap_groesse > 0));
    foto("a6_liste");

    // Beenden ohne Auswahl: Hinweis
    let (bx, by) = beenden();
    f.tippen(bx, by);
    bis(status_rot, "Hinweis ohne Auswahl");
    foto("a6_ohne_auswahl");

    // Systemdienst: lässt sich nicht beenden
    let i = desktop::taskmanager::liste().iter().position(|p| p.name == "@taskbar").unwrap();
    assert!(i < desktop::taskmanager::ZEILEN);
    let (x, y) = zeile(i);
    f.tippen(x, y);
    f.tippen(bx, by);
    foto("a6_systemdienst");
    // länger als Wartezeit + Zwang: wäre sie beendet worden, wäre sie jetzt weg
    std::thread::sleep(Duration::from_millis(desktop::taskmanager::WARTE_MS as u64 + 800));
    assert!(laeuft("@taskbar") && da("@taskbar"), "Taskleiste läuft weiter");

    // Endlostest antippen, Beenden: kooperativ geht nichts (keine
    // Systemaufrufe), nach 2 s zwangsweise → Fenster weg
    let (x, y) = zeile(0);
    f.tippen(x, y);
    foto("a6_ausgewaehlt");
    let t = Instant::now();
    f.tippen(bx, by);
    bis(|| !da("Endless loop"), "Fenster des Hängers weg");
    let dauer = t.elapsed();
    assert!(dauer >= Duration::from_millis(1800), "erst kooperativ gewartet: {dauer:?}");
    assert!(da("Task manager"), "Taskmanager läuft weiter");
    std::thread::sleep(Duration::from_millis(600));
    foto("a6_beendet");
    // PC: Thread lief weiter → freigeben, dann ganz aufgeräumt
    assert!(laeuft("endlostest"));
    desktop::taskmanager::ENDLOS_STOPP.store(true, Ordering::Relaxed);
    bis(|| !laeuft("endlostest"), "Endlostest aufgeräumt");
    assert!(laeuft("@taskbar") && laeuft("taskmanager"));
    std::thread::sleep(Duration::from_millis(1200));
    foto("a6_liste_nachher");
    assert_eq!(desktop::taskmanager::liste()[0].name, "taskmanager");
}
