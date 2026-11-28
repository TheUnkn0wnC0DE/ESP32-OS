//! D3-Nachweis (PC): App-Store per Touch – Doom (eigenes Abbild) entfernen
//! (mit Bestätigung) und wieder installieren (Download simuliert, mit
//! Fortschritt; am Board GitHub-Release mit SHA-256), PasswordGen
//! (einkompiliert) entfernen; der Zustand steht in den Einstellungen und gilt
//! nach dem Neuladen, Startmenü-Liste folgt. Fotos.

use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

/// Simulierte Abbilder: Zustand je Platz, Liste der Vorgänge.
static ABBILD_DA: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::new());
static VORGAENGE: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Simuliert;

impl kern::aktualisierung::Abbilder for Simuliert {
    fn vorhanden(&self, platz: &str) -> Option<bool> {
        ABBILD_DA.lock().unwrap().iter().find(|(p, _)| p == platz).map(|(_, d)| *d)
    }
    fn installieren(&self, platz: &str, datei: &str, fortschritt: &mut dyn FnMut(u32)) -> Result<(), String> {
        for p in [0, 50, 100] {
            fortschritt(p);
        }
        VORGAENGE.lock().unwrap().push(format!("installieren {platz} {datei}"));
        for e in ABBILD_DA.lock().unwrap().iter_mut().filter(|(p, _)| p == platz) {
            e.1 = true;
        }
        Ok(())
    }
    fn entfernen(&self, platz: &str) -> Result<(), String> {
        VORGAENGE.lock().unwrap().push(format!("entfernen {platz}"));
        for e in ABBILD_DA.lock().unwrap().iter_mut().filter(|(p, _)| p == platz) {
            e.1 = false;
        }
        Ok(())
    }
}

fn vorgaenge() -> Vec<String> {
    VORGAENGE.lock().unwrap().clone()
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::create_dir_all(&dir).unwrap();
    // Uhr der Taskleiste ausblenden
    let maske: Vec<u32> = px.iter().enumerate().map(|(i, &p)| if i % w >= w - 60 && i / w >= h - 40 { 0 } else { p }).collect();
    let ist = format!("{:016x}", fnv(&maske));
    if std::env::var_os("ERWARTET_NEU").is_some() {
        desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
        std::fs::write(dir.join(format!("{name}.fnv")), &ist).unwrap();
    } else {
        let soll = std::fs::read_to_string(dir.join(format!("{name}.fnv"))).expect("ERWARTET_NEU=1");
        if soll.trim() != ist {
            desktop::bild_schreiben(&dir.join(format!("{name}.ist.png")), &spatz::image::encode_png(w, h, &px));
            panic!("Bild {name} weicht ab");
        }
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn tippen(&mut self, (x, y): (i32, i32)) {
        self.g.schritt(self.t, [Some((x, y)), None]);
        self.t += 80;
        std::thread::sleep(Duration::from_millis(5));
        self.g.schritt(self.t, [None, None]);
        self.t += 100;
        std::thread::sleep(Duration::from_millis(30));
        self.g.schritt(self.t, [None, None]);
        self.t += 100;
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn im_startmenue(name: &str) -> bool {
    desktop::shell::anwendungen().iter().any(|a| a.programm == name)
}

#[test]
fn app_store_entfernen_und_installieren() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_appstore_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    *ABBILD_DA.lock().unwrap() = vec![("doomapp".into(), true), ("quakeapp".into(), true)];
    assert!(kern::aktualisierung::abbilder_setzen(Box::new(Simuliert)));
    desktop::konfig::laden();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@taskbar").unwrap();
    let pid = kern::starte_name("appstore").unwrap();
    bis(|| sys::fenster_liste().iter().any(|f| f.pid == pid), "App-Store offen");
    bild("d3_appstore");
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    let knopf = |z| desktop::appstore::knopf_mitte(z).expect("Knopf");

    // Doom (Zeile 0, eigenes Abbild): erstes Tippen fragt nur nach
    assert!(im_startmenue("@spiel_doom"));
    f.tippen(knopf(0));
    assert!(vorgaenge().is_empty(), "ohne Bestätigung nichts gelöscht");
    assert!(desktop::appstore::sichtbar("@spiel_doom"));
    bild("d3_doom_bestaetigen");
    f.tippen(knopf(0));
    bis(|| !vorgaenge().is_empty(), "Doom entfernt");
    assert_eq!(vorgaenge(), ["entfernen doomapp"]);
    assert!(!desktop::appstore::sichtbar("@spiel_doom") && !im_startmenue("@spiel_doom"), "Doom weg");
    bild("d3_doom_entfernt");
    // wieder installieren (Download simuliert)
    f.tippen(knopf(0));
    bis(|| vorgaenge().len() == 2, "Doom installiert");
    assert_eq!(vorgaenge()[1], "installieren doomapp doomapp.bin");
    assert!(desktop::appstore::sichtbar("@spiel_doom") && im_startmenue("@spiel_doom"), "Doom wieder da");
    bild("d3_doom_installiert");

    // PasswordGen (Zeile 4, einkompiliert): sofort entfernt (im Startmenü
    // nur am Board – die C-Programme gibt es nur auf 32 Bit)
    f.tippen(knopf(4));
    bis(|| !desktop::appstore::sichtbar("passwordgen"), "PasswordGen entfernt");
    bild("d3_passwordgen_entfernt");
    let ini = std::fs::read_to_string(wurzel.join("settings/esp32os.ini")).unwrap();
    assert!(ini.contains("hidden_apps=passwordgen"), "{ini}");
    // „Neustart“: Zustand kommt aus den Einstellungen
    desktop::appstore::laden("");
    assert!(desktop::appstore::sichtbar("passwordgen"));
    desktop::konfig::laden();
    assert!(!desktop::appstore::sichtbar("passwordgen") && desktop::appstore::sichtbar("@spiel_doom"));

    f.tippen(knopf(4));
    bis(|| desktop::appstore::sichtbar("passwordgen"), "PasswordGen wieder installiert");
    assert_eq!(vorgaenge().len(), 2, "einkompilierte Programme laden nichts");
    kern::zwangsbeenden(pid, 2000);
    let _ = std::fs::remove_dir_all(&wurzel);
}
