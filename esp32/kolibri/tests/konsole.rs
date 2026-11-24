//! C1: console.obj nativ und die unveränderte KolibriOS-Shell – Eingabe
//! über die Tastatur-Ereignisse (wie die Bildschirmtastatur), Fotos der
//! Konsole, Schliessen über das Kreuz.
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
    std::thread::sleep(Duration::from_millis(300));
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

/// Text tippen wie die Bildschirmtastatur (ASCII in Bits 8–15).
fn tippe(t: &str) {
    for c in t.bytes() {
        sys::eingabe::taste(u32::from(if c == b'\n' { 13 } else { c }) << 8);
        std::thread::sleep(Duration::from_millis(15));
    }
    std::thread::sleep(Duration::from_millis(400));
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

/// Kreuz des obersten Fensters (Systemknopf 1).
fn kreuz() -> (i32, i32) {
    kern::fenster::mit_welt(|w| {
        let s = *w.stapel.last().expect("Fenster");
        let f = w.slots[s].as_ref().and_then(|z| z.fenster.as_ref()).expect("Fenster");
        let k = f.knoepfe.iter().find(|k| k.system && k.id == 1).expect("Kreuz");
        (f.x + k.x + k.w / 2, f.y + k.y + k.h / 2)
    })
}

#[test]
fn shell_in_der_konsole() {
    let sysdir = std::env::temp_dir().join(format!("esp32os_c1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sysdir);
    std::fs::create_dir_all(sysdir.join("ordner")).unwrap();
    std::fs::write(sysdir.join("a.txt"), b"Hallo").unwrap();
    std::fs::write(sysdir.join("b.txt"), b"Welt!!").unwrap();
    kern::init(&kern::PC, kolibri::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &sysdir);
    kern::datei::einhaengen("/rd/1", &sysdir);

    let pid = kern::starte_name("shell").unwrap();
    std::thread::sleep(Duration::from_millis(800));
    // Konsole fordert die Bildschirmtastatur an
    assert!(sys::textfeld_mit_fokus().is_some(), "Konsole meldet Textfokus");
    bild("c1_shell_start");
    tippe("echo Hallo ESP32-OS\n");
    tippe("ver\n");
    bild("c1_shell_echo_ver");
    tippe("ls\n");
    tippe("lsmod\n");
    bild("c1_shell_ls");
    // Zeilenbearbeitung: Tippfehler, zwei Mal links, Löschen, weiter
    tippe("echo abXc");
    sys::eingabe::taste(0xB0 << 8);
    std::thread::sleep(Duration::from_millis(50));
    sys::eingabe::taste(8 << 8);
    std::thread::sleep(Duration::from_millis(300));
    bild("c1_shell_vor_enter");
    tippe("\n");
    bild("c1_shell_bearbeitet");
    // Datei anlegen und wieder löschen (Dateifunktionen über Funktion 70)
    tippe("touch neu.txt\n");
    assert!(sysdir.join("neu.txt").exists(), "touch legt die Datei an");
    tippe("rm neu.txt\n");
    assert!(!sysdir.join("neu.txt").exists(), "rm löscht die Datei");
    // Kreuz: Programm endet
    let (x, y) = kreuz();
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(40));
    sys::eingabe::maus(x, y, 0);
    assert_eq!(ende(pid), Ende::Normal);
    assert!(sys::textfeld_mit_fokus().is_none(), "Textfokus weg");
    let _ = std::fs::remove_dir_all(&sysdir);
    matrix_in_der_konsole();
}

/// „Matrix“ (C1) nach der Shell im selben Kern.
fn matrix_in_der_konsole() {
    // 40 Schritte, fester Startwert: gleiches Bild bei jedem Lauf
    let pid = kern::starte_mit(kern::programme().iter().find(|e| e.name == "matrix").unwrap(), "40 7").unwrap();
    let t = Instant::now();
    while !sys::fenster_liste().iter().any(|f| f.titel.contains("[Finished]")) {
        assert!(t.elapsed() < Duration::from_secs(20), "Matrix wird nicht fertig");
        std::thread::sleep(Duration::from_millis(20));
    }
    bild("c1_matrix");
    let (x, y) = kreuz();
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(40));
    sys::eingabe::maus(x, y, 0);
    assert_eq!(ende(pid), Ende::Normal);
}
