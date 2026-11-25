//! Q1 Randfälle: keine SD-Karte, kein Netz, sehr lange Dateinamen, leere
//! Ordner. Jedes Programm öffnet ein Fenster, nichts stürzt ab; Fotos je
//! Fall in `tests/erwartet/q1_rand_*.png` zum Ansehen.
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(400));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn offen(name: &str) -> u32 {
    let pid = kern::starte_name(name).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let t = Instant::now();
    while !sys::fenster_liste().iter().any(|f| f.pid == pid) {
        assert!(t.elapsed() < Duration::from_secs(5), "{name}: kein Fenster");
        std::thread::sleep(Duration::from_millis(20));
    }
    pid
}

fn zu(pid: u32) {
    kern::zwangsbeenden(pid, 2000);
    let t = Instant::now();
    while sys::fenster_liste().iter().any(|f| f.pid == pid) && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn randfaelle() {
    // nur interner Speicher, keine SD-Karte (/sd0/1 nicht eingehängt), kein
    // Netz (kein Abrufer, kein WLAN-Treiber)
    let intern = std::env::temp_dir().join(format!("esp32os_q1r_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&intern);
    std::fs::create_dir_all(intern.join("leer")).unwrap();
    let lang = format!("{}.txt", "ein_sehr_langer_dateiname_".repeat(8));
    std::fs::write(intern.join(&lang), b"x").unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    for p in ["/sys", "/rd/1"] {
        kern::datei::einhaengen(p, &intern);
    }
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    desktop::shell::anwendungen_setzen(&desktop::anwendungen());
    kern::starte_name("@taskbar").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = kern::neue_abstuerze();
    for (name, foto_name) in [
        ("dateimanager", "q1_rand_ohne_sd_dateimanager"),
        ("notizen", "q1_rand_ohne_sd_notizen"),
        ("musik", "q1_rand_ohne_sd_musik"),
        ("wetter", "q1_rand_ohne_netz_wetter"),
        ("einstellungen", "q1_rand_ohne_sd_einstellungen"),
    ] {
        let pid = offen(name);
        foto(foto_name);
        let a = kern::neue_abstuerze();
        assert!(a.is_empty(), "{name}: Abstürze {a:?}");
        assert!(kern::ende_von(pid).is_none(), "{name} beendet sich ohne SD/Netz");
        zu(pid);
    }
    // langer Dateiname im Dateimanager (auf dem internen Speicher)
    let pid = kern::datei::starten("/sys/dateimanager", "/rd/1");
    assert!(pid > 0, "Dateimanager mit Ordner");
    std::thread::sleep(Duration::from_millis(800));
    foto("q1_rand_langer_name");
    let a = kern::neue_abstuerze();
    assert!(a.is_empty(), "langer Name: Abstürze {a:?}");
    let _ = std::fs::remove_dir_all(&intern);
}
