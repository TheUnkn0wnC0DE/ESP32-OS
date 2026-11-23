//! D1-Nachweis (PC): Startbild → Desktop mit Symbolen und Taskleiste,
//! Startmenü und Info-Dialog per Touch.

use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::create_dir_all(&dir).unwrap();
    // Uhrzeit in der Taskleiste ändert sich: Vergleich ohne die rechten 60 px der Leiste
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
    px
}

fn tippen(x: i32, y: i32) {
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(50));
    sys::eingabe::maus(x, y, 0);
    std::thread::sleep(Duration::from_millis(100));
}

fn titel_da(t: &str) -> bool {
    kern::kern_abschnitt(|| kern::fenster::mit_welt(|w| w.fenster_liste().iter().any(|f| f.titel == t)))
}

#[test]
fn desktop_startet_mit_taskleiste_menue_und_info() {
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(0));
    bild("d1_startbild");
    let t = Instant::now();
    while !titel_da("") && kern::liste().iter().filter(|p| p.name.starts_with('@')).count() < 2 {
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(1500));
    bild("d1_desktop");
    // Startknopf (links in der Leiste)
    tippen(40, 360 - 20);
    std::thread::sleep(Duration::from_millis(300));
    assert!(titel_da("ESP32-OS"), "Startmenü offen");
    bild("d1_menue");
    // „Über ESP32-OS“ ist vorletzter Eintrag; Menü-Fenster liegt über der Leiste
    let (my, n) = kern::kern_abschnitt(|| {
        kern::fenster::mit_welt(|w| {
            let s = w.fenster_liste().into_iter().find(|f| f.titel == "ESP32-OS").unwrap().slot;
            let f = w.slots[s].as_ref().unwrap().fenster.as_ref().unwrap();
            (f.y, f.knoepfe.iter().filter(|k| !k.system).count())
        })
    });
    let skin = 48;
    // Einträge: Über, Spiele, Neustart → „Über“ ist der erste
    let _ = n;
    let y = my + skin + 2 + 20;
    tippen(5 + 60, y);
    let t = Instant::now();
    while !titel_da("About ESP32-OS") {
        assert!(t.elapsed() < Duration::from_secs(5), "Info-Dialog erscheint nicht");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!titel_da("ESP32-OS"), "Menü schliesst nach Auswahl");
    bild("d1_info");
    // OK (Fenster 100,20; Arbeitsbereich +5,+48; Knopf bei 320,182 100×40)
    tippen(100 + 5 + 370, 20 + 48 + 200);
    let t = Instant::now();
    while titel_da("About ESP32-OS") {
        assert!(t.elapsed() < Duration::from_secs(5), "Info-Dialog schliesst nicht");
        std::thread::sleep(Duration::from_millis(20));
    }
}
