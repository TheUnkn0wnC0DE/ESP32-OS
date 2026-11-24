//! D1: Shell – Umleitungen (`>`, `>>`, `<`), Pipes (`|`), neue Befehle
//! (cat, grep, wc, head, tail, sort) und Skripte (#SHS). Eingabe über die
//! Tastatur-Ereignisse wie die Bildschirmtastatur; geprüft werden die
//! geschriebenen Dateien und ein Foto der Konsole.
#![cfg(target_pointer_width = "32")]
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) {
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
}

fn tippe(t: &str) {
    for c in t.bytes() {
        sys::eingabe::taste(u32::from(if c == b'\n' { 13 } else { c }) << 8);
        std::thread::sleep(Duration::from_millis(15));
    }
    std::thread::sleep(Duration::from_millis(400));
}

fn datei(p: &Path) -> String {
    let t = Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(p) {
            return s;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "{} fehlt", p.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn umleitung_pipes_befehle_skript() {
    let d = std::env::temp_dir().join(format!("esp32os_d1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("ordner")).unwrap();
    std::fs::write(d.join("obst.txt"), "Birne\napfel\nKirsche\nApfelsaft\nbanane\n").unwrap();
    std::fs::write(d.join("test.shs"), "#SHS\necho aus dem Skript > skript.txt\nls -1 | grep obst >> skript.txt\n").unwrap();
    kern::init(&kern::PC, kolibri::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sys", &d);
    kern::datei::einhaengen("/rd/1", &d);
    let pid = kern::starte_name("shell").unwrap();
    std::thread::sleep(Duration::from_millis(800));

    // > und >>
    tippe("echo Hallo Welt > o1.txt\n");
    assert_eq!(datei(&d.join("o1.txt")), "Hallo Welt\n");
    tippe("echo zweite Zeile >> o1.txt\n");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(datei(&d.join("o1.txt")), "Hallo Welt\nzweite Zeile\n");
    // Pipe in eine Datei, grep ohne Gross/Klein
    tippe("cat obst.txt | grep -i apfel > o2.txt\n");
    assert_eq!(datei(&d.join("o2.txt")), "apfel\nApfelsaft\n");
    // mehrere Stufen, sort, head
    tippe("cat obst.txt | sort | head -n 2 > o3.txt\n");
    assert_eq!(datei(&d.join("o3.txt")), "Apfelsaft\nBirne\n");
    // < als Eingabe, wc, tail, sort -r, grep -v/-c
    tippe("wc -l < obst.txt > o4.txt\n");
    assert_eq!(datei(&d.join("o4.txt")), "5\n");
    tippe("tail -n 1 obst.txt > o5.txt\n");
    assert_eq!(datei(&d.join("o5.txt")), "banane\n");
    tippe("sort -r obst.txt | grep -v a > o6.txt\n");
    assert_eq!(datei(&d.join("o6.txt")), "Kirsche\nBirne\n");
    tippe("grep -c i obst.txt > o7.txt\n");
    assert_eq!(datei(&d.join("o7.txt")), "2\n");
    // eingebaute Befehle der Shell in der Pipe
    tippe("ls -1 > o9.txt\n");
    let o9 = datei(&d.join("o9.txt"));
    tippe("ls -1 | grep ordner > o8.txt\n");
    let o8 = datei(&d.join("o8.txt"));
    assert!(o9.contains("  obst.txt\n") && o9.contains("  ordner/\n"), "ls -1 in eine Datei: {o9:?}");
    assert!(o8.contains("ordner"), "ls in der Pipe: {o8:?}");
    // Skript (Signatur #SHS) mit Umleitungen
    tippe("test.shs\n");
    std::thread::sleep(Duration::from_millis(500));
    let s = datei(&d.join("skript.txt"));
    assert!(s.starts_with("aus dem Skript\n") && s.contains("obst.txt"), "Skript: {s:?}");
    // Anzeige: Pipe ohne Umleitung zeigt das Ergebnis in der Konsole
    tippe("clear\n");
    tippe("cat obst.txt | grep -i apfel | wc\n");
    tippe("help grep\n");
    bild("d1_shell_pipe");
    // Review 24.11.: grosse Eingaben – zu grosse Datei wird gemeldet, eine
    // grosse Pipe läuft ohne Absturz, Umleitung an falscher Stelle abgelehnt
    std::fs::write(d.join("gross.txt"), vec![b'x'; 300 * 1024]).unwrap();
    let zeilen: String = (0..10_000).map(|i| format!("zeile{i:04}\n")).collect();
    std::fs::write(d.join("viel.txt"), zeilen).unwrap();
    tippe("wc gross.txt > o10.txt\n");
    assert!(datei(&d.join("o10.txt")).contains("too large"), "zu grosse Datei gemeldet");
    tippe("cat viel.txt | grep zeile9 | wc -l > o11.txt\n");
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(datei(&d.join("o11.txt")), "1000\n");
    tippe("ls > o12.txt | wc\n");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!d.join("o12.txt").exists(), "> vor einer Pipe abgelehnt");
    tippe("wc -l <obst.txt|cat >o13.txt\n");
    assert_eq!(datei(&d.join("o13.txt")), "5\n", "Dateiname nach < endet an |");
    // fehlende Datei hinter > meldet einen Fehler, kein Absturz
    tippe("echo x >\n");
    assert!(kern::ende_von(pid).is_none(), "Shell läuft weiter");
    kern::zwangsbeenden(pid, 2000);
    let _ = std::fs::remove_dir_all(&d);
}
