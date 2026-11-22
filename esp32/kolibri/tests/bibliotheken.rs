//! K7-Nachweis: Original-KolibriOS-Beispiele (box_lib, libimg) und der
//! proc_lib-Test laufen gegen die nativen Bibliotheken, per Touch bedient.
#![cfg(target_pointer_width = "32")]

use kern::{Ende, KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(150));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    let ist = format!("{:016x}", fnv(&px));
    if std::env::var_os("ERWARTET_NEU").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
        std::fs::write(dir.join(format!("{name}.fnv")), &ist).unwrap();
    } else {
        let soll = std::fs::read_to_string(dir.join(format!("{name}.fnv"))).expect("ERWARTET_NEU=1");
        if soll.trim() != ist {
            std::fs::write(dir.join(format!("{name}.ist.png")), spatz::image::encode_png(w, h, &px)).unwrap();
            panic!("Bild {name} weicht ab");
        }
    }
    px
}

fn tippen(x: i32, y: i32) {
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(40));
    sys::eingabe::maus(x, y, 0);
    std::thread::sleep(Duration::from_millis(40));
}

fn ende(pid: u32) -> Ende {
    let t = Instant::now();
    loop {
        kern::aufraeumen();
        if let Some(e) = kern::ende_von(pid) {
            return e;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "Programm {pid} endet nicht");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn clayer_beispiele_per_touch() {
    let sysdir = std::env::temp_dir().join(format!("esp32os_k7_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sysdir);
    std::fs::create_dir_all(&sysdir).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("daten/logo.png"), sysdir.join("logo.png")).unwrap();
    std::fs::write(sysdir.join("a.txt"), b"Hallo").unwrap();
    kern::init(&kern::PC, kolibri::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::datei::einhaengen("/sys", &sysdir);

    // ---- box_lib: Fenster 0,0 (passt nicht), Stil 0x34: relativ zum Arbeitsbereich (+5,+22)
    let pid = kern::starte_name("boxlib_demo").unwrap();
    bild("k7_boxlib_start");
    tippen(5 + 30, 22 + 66); // edit_box bei (10,60)
    assert!(sys::textfeld_mit_fokus().is_some(), "Textfeld meldet Fokus (Bildschirmtastatur)");
    // cp866-Feld (Schrift 1): ASCII; Umlaute testet `boxlib_umlaute` (UTF-8-Feld)
    for c in "Gruezi 42".chars() {
        let code = u32::from(kern::schrift::uni_zu_cp866(c));
        sys::eingabe::taste(code << 8);
        std::thread::sleep(Duration::from_millis(30));
    }
    let mit_text = bild("k7_boxlib_text");
    tippen(5 + 10 + 7, 22 + 120 + 7); // check_box an → Text unten aus
    let ohne = bild("k7_boxlib_check");
    assert_ne!(mit_text, ohne, "Checkbox schaltet Ausgabe");
    // Scrollbar (x 614 im Arbeitsbereich) nach unten ziehen → Fortschrittsbalken
    sys::eingabe::maus(5 + 621, 22 + 5, 1);
    // Druck erst lesen lassen (sonst greift das Programm den Regler an einer
    // späteren Stelle – Mausereignisse werden zusammengefasst wie in KolibriOS)
    std::thread::sleep(Duration::from_millis(100));
    for y in (10..200).step_by(10) {
        sys::eingabe::maus(5 + 621, 22 + y, 1);
        std::thread::sleep(Duration::from_millis(15));
    }
    // Endlage gedrückt halten, bis das Programm sie sicher gelesen hat
    sys::eingabe::maus(5 + 621, 22 + 200, 1);
    std::thread::sleep(Duration::from_millis(100));
    sys::eingabe::maus(5 + 621, 22 + 200, 0);
    bild("k7_boxlib_scroll");
    tippen(640 - 12, 11); // Schliessen
    assert_eq!(ende(pid), Ende::Normal);

    // ---- libimg: logo.png aus /sys (aktueller Ordner), mischen, skalieren
    let pid = kern::starte_name("libimg_demo").unwrap();
    bild("k7_libimg");
    tippen(10 + 435 - 12, 40 + 11); // Fenster 10,40, 435 breit
    assert_eq!(ende(pid), Ende::Normal);

    // ---- proc_lib: Datei wählen, Farbe wählen
    let pid = kern::starte_name("dialog_demo").unwrap();
    bild("k7_dialog_datei");
    tippen(10 + 5 + 60, 10 + 22 + 36 + 14); // erster Eintrag: a.txt
    bild("k7_dialog_farbe");
    tippen(10 + 5 + 5 + 20, 10 + 22 + 5 + 20); // erste Farbe
    tippen(10 + 5 + 250 + 50, 10 + 22 + 233 + 20); // OK
    assert_eq!(ende(pid), Ende::Normal);
    let erg = std::fs::read_to_string(sysdir.join("dialog_ergebnis.txt")).unwrap();
    assert!(erg.contains("datei=/sys/a.txt"), "{erg}");
    assert!(erg.contains("farbe=3F0000"), "{erg}");
    let _ = std::fs::remove_dir_all(&sysdir);
}
