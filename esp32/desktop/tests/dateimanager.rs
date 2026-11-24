//! P1-Nachweis (PC): Dateimanager per Touch – Ordner öffnen/Hoch, Kopieren
//! und Verschieben (SD → intern, innerhalb SD), Umbenennen und Neuer Ordner
//! über die Bildschirmtastatur, Löschen (auch Ordner mit Inhalt) mit
//! Rückfrage. Alles über die Gestenerkennung (Tippen, langer Druck =
//! Kontextmenü); geprüft wird das echte Dateisystem.

use desktop::dateimanager::{LEISTE, SEITE_W, ZEILE};
use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, tastenmitte};
use kern::{KernAlloc, sys};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}; Fenster: {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn halten(&mut self, x: i32, y: i32, ms: u64) {
        let mut d = 0;
        while d <= ms {
            self.g.schritt(self.t + d, [Some((x, y)), None]);
            d += 50;
            std::thread::sleep(Duration::from_millis(5));
            anzeigen();
        }
        self.t += ms + 50;
        self.g.schritt(self.t, [None, None]);
        std::thread::sleep(Duration::from_millis(60));
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        self.t += 600;
        std::thread::sleep(Duration::from_millis(200));
        anzeigen();
    }
    fn tippen(&mut self, x: i32, y: i32) {
        self.halten(x, y, 80);
    }
}

/// Anzeige wie am Board: Dirty-Rects in einen Doppelpuffer (Schirm). Jedes
/// Foto prüft, dass das Teilbild dem Vollbild entspricht.
static SCHIRM: std::sync::Mutex<Option<kern::sys::Schirm>> = std::sync::Mutex::new(None);

fn anzeigen() {
    let mut g = SCHIRM.lock().unwrap();
    let s = g.get_or_insert_with(kern::sys::Schirm::default);
    s.aktualisieren();
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(200));
    for _ in 0..2 {
        let mut g = SCHIRM.lock().unwrap();
        let s = g.get_or_insert_with(kern::sys::Schirm::default);
        if let Some(i) = s.aktualisieren() {
            let (w, h, voll) = sys::foto();
            let teil: Vec<u32> = s.puffer[i].px.iter().map(|&c| kern::grafik::von565(c)).collect();
            if teil != voll {
                let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
                desktop::bild_schreiben(&dir.join(format!("{name}.teil.png")), &spatz::image::encode_png(w, h, &teil));
                panic!("{name}: Teilbild (Dirty-Rects) weicht vom Vollbild ab");
            }
        }
    }
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn titel() -> String {
    sys::fenster_liste().into_iter().find(|f| f.titel.starts_with("File Manager")).map(|f| f.titel).unwrap_or_default()
}

// Fenster (0, 0), Arbeitsbereich ab (5, 48)
const LX: i32 = 5 + SEITE_W + 4 + 100;
fn zeile(i: i32) -> i32 {
    48 + LEISTE + i * ZEILE + ZEILE / 2
}
fn geraet(i: i32) -> (i32, i32) {
    (5 + SEITE_W / 2, 48 + 4 + i * 48 + 22)
}
const HOCH: (i32, i32) = (5 + SEITE_W + 4 + 40, 48 + 2 + 21);
const NEU: (i32, i32) = (5 + SEITE_W + 4 + 86 + 65, 48 + 2 + 21);
const EINFUEGEN: (i32, i32) = (5 + SEITE_W + 4 + 222 + 50, 48 + 2 + 21);
/// Knöpfe der Dialoge: Name (OK), Rückfrage (Löschen).
const NAME_OK: (i32, i32) = (20 + 5 + 380 + 45, 48 + 3 + 21);
const FRAGE_JA: (i32, i32) = (110 + 5 + 160 + 60, 74 + 48 + 56 + 21);

/// Langer Druck auf Zeile `i`, dann Menüeintrag `k` (Öffnen 0, Kopieren 1,
/// Ausschneiden 2, Umbenennen 3, Löschen 4).
fn menue(f: &mut Finger, i: i32, k: i32, bild: Option<&str>) {
    let y = zeile(i);
    let vorher = sys::fenster_liste().len();
    f.halten(LX, y, 700);
    bis(|| sys::fenster_liste().len() > vorher, "Kontextmenü");
    std::thread::sleep(Duration::from_millis(100));
    if let Some(b) = bild {
        foto(b);
    }
    // Menü: 5 × 44 + 4 hoch, nach oben verschoben, wenn es unten nicht passt
    let my = y.min(360 - (5 * 44 + 4));
    f.tippen(LX + 40, my + 2 + k * 44 + 22);
}

/// Wie `menue` für ein Menü mit `n` Einträgen (Papierkorb: 2).
fn menue_n(f: &mut Finger, i: i32, k: i32, n: i32) {
    let y = zeile(i);
    let vorher = sys::fenster_liste().len();
    f.halten(LX, y, 700);
    bis(|| sys::fenster_liste().len() > vorher, "Kontextmenü");
    std::thread::sleep(Duration::from_millis(100));
    let my = y.min(360 - (n * 44 + 4));
    f.tippen(LX + 40, my + 2 + k * 44 + 22);
}

/// Zeile eines Namens in der Liste des Dateimanagers (gleiche Sortierung).
fn zeile_von(ordner: &str, name: &str) -> i32 {
    let l = desktop::dateimanager::lesen(ordner).expect("Liste");
    l.iter().filter(|e| e.name != ".herkunft").position(|e| e.name == name).unwrap_or_else(|| panic!("{name} in {ordner}")) as i32
}

/// Text über die Bildschirmtastatur tippen (nur Kleinbuchstaben).
fn tippe(f: &mut Finger, text: &str) {
    bis(|| sys::tastatur_soll().0, "Tastatur erscheint");
    std::thread::sleep(Duration::from_millis(300));
    let y_unten = 360 - desktop::shell::LEISTE_H - HOEHE;
    let oben = match sys::tastatur_soll().1 {
        Some(c) if c.y1 > y_unten && c.y0 >= HOEHE => 0,
        Some(c) if c.y1 > y_unten => (c.y1 + 2).min(360 - HOEHE),
        _ => y_unten,
    };
    for c in text.chars() {
        let (ebene, x, y) = tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        assert_eq!(ebene, "normal", "{c}");
        f.tippen(x, oben + y);
    }
}

fn baum(wurzel: &Path) -> (PathBuf, PathBuf) {
    let _ = std::fs::remove_dir_all(wurzel);
    let (sd, intern) = (wurzel.join("sd"), wurzel.join("intern"));
    std::fs::create_dir_all(sd.join("ordner/tief")).unwrap();
    std::fs::create_dir_all(&intern).unwrap();
    std::fs::write(sd.join("notiz.txt"), "Hallo vom Dateimanager").unwrap();
    // grösser als ein Kopierblock (32 KB)
    let bild: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::write(sd.join("bild.png"), &bild).unwrap();
    std::fs::write(sd.join("ordner/a.txt"), "a").unwrap();
    std::fs::write(sd.join("ordner/tief/b.txt"), "b").unwrap();
    (sd, intern)
}

#[test]
fn dateimanager_per_touch() {
    let wurzel = std::env::temp_dir().join("esp32os_p1_test");
    let (sd, intern) = baum(&wurzel);
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sd0/1", &sd);
    kern::datei::einhaengen("/rd/1", &intern);
    for p in ["@taskbar", "@tastatur", "dateimanager"] {
        kern::starte_name(p).unwrap();
    }
    bis(|| titel() == "File Manager - /sd0/1", "Start auf der SD-Karte");
    std::thread::sleep(Duration::from_millis(300));
    foto("p1_liste");
    let mut f = Finger { g: Gesten::default(), t: 1000 };

    // Liste: ordner/, bild.png, notiz.txt – Ordner öffnen und zurück
    f.tippen(LX, zeile(0));
    bis(|| titel() == "File Manager - /sd0/1/ordner", "Ordner geöffnet");
    f.tippen(HOCH.0, HOCH.1);
    bis(|| titel() == "File Manager - /sd0/1", "Hoch");

    // Kopieren SD → intern
    menue(&mut f, 2, 1, Some("p1_kontextmenue"));
    let (gx, gy) = geraet(1);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /rd/1", "Gerät intern");
    f.tippen(EINFUEGEN.0, EINFUEGEN.1);
    bis(|| intern.join("notiz.txt").exists(), "kopiert");
    assert_eq!(std::fs::read(intern.join("notiz.txt")).unwrap(), b"Hallo vom Dateimanager");
    assert!(sd.join("notiz.txt").exists(), "Quelle bleibt beim Kopieren");
    // nochmals einfügen → freier Name
    f.tippen(EINFUEGEN.0, EINFUEGEN.1);
    bis(|| intern.join("notiz (Copy).txt").exists(), "Kopie mit freiem Namen");

    // Verschieben SD → intern (andere Partition: kopieren + löschen)
    let (gx, gy) = geraet(0);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /sd0/1", "Gerät SD");
    menue(&mut f, 1, 2, None);
    let (gx, gy) = geraet(1);
    f.tippen(gx, gy);
    f.tippen(EINFUEGEN.0, EINFUEGEN.1);
    bis(|| intern.join("bild.png").exists() && !sd.join("bild.png").exists(), "verschoben");
    assert_eq!(std::fs::metadata(intern.join("bild.png")).unwrap().len(), 100_000);
    assert_eq!(std::fs::read(intern.join("bild.png")).unwrap()[99_999], (99_999u32 * 7 % 251) as u8);

    // Verschieben innerhalb der SD: notiz.txt → ordner/
    let (gx, gy) = geraet(0);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /sd0/1", "Gerät SD");
    // Liste jetzt: ordner/, notiz.txt
    menue(&mut f, 1, 2, None);
    f.tippen(LX, zeile(0));
    bis(|| titel() == "File Manager - /sd0/1/ordner", "Zielordner");
    f.tippen(EINFUEGEN.0, EINFUEGEN.1);
    bis(|| sd.join("ordner/notiz.txt").exists() && !sd.join("notiz.txt").exists(), "innerhalb verschoben");
    f.tippen(HOCH.0, HOCH.1);
    bis(|| titel() == "File Manager - /sd0/1", "Hoch");

    // Umbenennen (intern: bild.png, notiz (Copy).txt, notiz.txt)
    let (gx, gy) = geraet(1);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /rd/1", "Gerät intern");
    menue(&mut f, 2, 3, None);
    tippe(&mut f, "brief");
    foto("p1_umbenennen");
    f.tippen(NAME_OK.0, NAME_OK.1);
    bis(|| intern.join("brief.txt").exists() && !intern.join("notiz.txt").exists(), "umbenannt");

    // Neuer Ordner
    f.tippen(NEU.0, NEU.1);
    tippe(&mut f, "bilder");
    foto("p1_neuer_ordner");
    f.tippen(NAME_OK.0, NAME_OK.1);
    bis(|| intern.join("bilder").is_dir(), "Ordner angelegt");
    bis(|| !sys::tastatur_soll().0, "Tastatur weg nach OK");

    // Löschen einer Datei (intern: bilder/, bild.png, brief.txt, notiz (Copy).txt)
    // – D4: in den Papierkorb
    menue(&mut f, 3, 4, None);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Delete"), "Rückfrage");
    foto("p1_loeschen");
    f.tippen(FRAGE_JA.0, FRAGE_JA.1);
    bis(|| !intern.join("notiz (Copy).txt").exists(), "Datei gelöscht");
    assert!(intern.join("Papierkorb/notiz (Copy).txt").exists(), "im Papierkorb");

    // D4: Papierkorb öffnen (links unter den Geräten), wiederherstellen
    let (kx, ky) = geraet(2);
    f.tippen(kx, ky);
    bis(|| titel() == "File Manager - /rd/1/Papierkorb", "Papierkorb offen");
    foto("d4_papierkorb");
    menue_n(&mut f, 0, 0, 2);
    bis(|| intern.join("notiz (Copy).txt").exists(), "wiederhergestellt");
    assert!(!intern.join("Papierkorb/notiz (Copy).txt").exists());
    // wieder löschen und im Papierkorb endgültig löschen
    let (gx, gy) = geraet(1);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /rd/1", "Gerät intern");
    menue(&mut f, zeile_von("/rd/1", "notiz (Copy).txt"), 4, None);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Delete"), "Rückfrage 2");
    f.tippen(FRAGE_JA.0, FRAGE_JA.1);
    bis(|| intern.join("Papierkorb/notiz (Copy).txt").exists(), "wieder im Papierkorb");
    f.tippen(kx, ky);
    bis(|| titel() == "File Manager - /rd/1/Papierkorb", "Papierkorb offen 2");
    menue_n(&mut f, 0, 1, 2);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Delete"), "Rückfrage endgültig");
    f.tippen(FRAGE_JA.0, FRAGE_JA.1);
    bis(|| !intern.join("Papierkorb/notiz (Copy).txt").exists(), "endgültig gelöscht");
    // Leeren: brief.txt in den Papierkorb, dann „Leeren“
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /rd/1", "Gerät intern 2");
    std::fs::copy(intern.join("brief.txt"), intern.join("weg.txt")).unwrap();
    // Gerät erneut antippen: Liste neu lesen
    f.tippen(gx, gy);
    std::thread::sleep(Duration::from_millis(300));
    menue(&mut f, zeile_von("/rd/1", "weg.txt"), 4, None);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Delete"), "Rückfrage 3");
    f.tippen(FRAGE_JA.0, FRAGE_JA.1);
    bis(|| intern.join("Papierkorb/weg.txt").exists(), "weg.txt im Papierkorb");
    f.tippen(kx, ky);
    bis(|| titel() == "File Manager - /rd/1/Papierkorb", "Papierkorb offen 3");
    f.tippen(NEU.0, NEU.1); // im Papierkorb: „Leeren“ an der Stelle von „Neuer Ordner“
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Empty"), "Rückfrage Leeren");
    f.tippen(FRAGE_JA.0, FRAGE_JA.1);
    bis(|| std::fs::read_dir(intern.join("Papierkorb")).unwrap().count() == 0, "Papierkorb leer");

    // Löschen eines Ordners mit Inhalt (SD: ordner/ mit a.txt, notiz.txt, tief/b.txt)
    let (gx, gy) = geraet(0);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /sd0/1", "Gerät SD");
    menue(&mut f, 0, 4, None);
    bis(|| sys::fenster_liste().iter().any(|w| w.titel == "Delete"), "Rückfrage Ordner");
    f.tippen(FRAGE_JA.0, FRAGE_JA.1);
    bis(|| !sd.join("ordner").exists(), "Ordner mit Inhalt gelöscht");

    // Ergebnis
    let (gx, gy) = geraet(1);
    f.tippen(gx, gy);
    bis(|| titel() == "File Manager - /rd/1", "Gerät intern");
    foto("p1_ergebnis");
    let mut namen: Vec<String> = std::fs::read_dir(&intern).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    namen.sort();
    assert_eq!(namen, ["Papierkorb", "bild.png", "bilder", "brief.txt"]);
    assert_eq!(std::fs::read_dir(&sd).unwrap().count(), 0, "SD leer");
}
