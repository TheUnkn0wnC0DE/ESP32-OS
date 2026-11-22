//! K6-Nachweis (PC): Funktionen 30/70 auf eingehängten Ordnern.

use kern::datei as d;
use kern::{Ende, KernAlloc, ProgrammEintrag};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static FEHLER: Mutex<Vec<String>> = Mutex::new(Vec::new());
static KIND: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn pruefe(ok: bool, was: &str) {
    if !ok {
        FEHLER.lock().unwrap().push(was.to_string());
    }
}

fn name_bei(buf: &[u8], i: usize, lang: usize) -> String {
    let o = 32 + i * lang + 40;
    let z = &buf[o..o + 264];
    String::from_utf8_lossy(&z[..z.iter().position(|&c| c == 0).unwrap()]).into_owned()
}

fn prog() {
    // virtuelle Wurzel
    let mut b = vec![0u8; 32 + 10 * 304];
    let (c, n) = d::ordner_lesen("/", 0, 10, 1, &mut b);
    pruefe(c == d::EOF && n == 3, "Wurzel: rd, sd0, sys");
    pruefe(name_bei(&b, 0, 304) == "rd" && name_bei(&b, 1, 304) == "sd0" && name_bei(&b, 2, 304) == "sys", "Wurzelnamen");
    let (c, n) = d::ordner_lesen("/sd0", 0, 10, 1, &mut b);
    pruefe(c == d::EOF && n == 1 && name_bei(&b, 0, 304) == "1", "/sd0 → 1");
    // anlegen, lesen, schreiben mit Lücke, Grösse
    pruefe(d::erstellen("/sd0/1/notiz.txt", b"Gruezi") == (0, 6), "70.2 anlegen");
    let mut r = [0u8; 16];
    pruefe(d::lesen("/SD0/1/NOTIZ.TXT".to_lowercase().as_str(), 0, &mut r[..6]) == (0, 6) && &r[..6] == b"Gruezi", "70.0 lesen");
    pruefe(d::lesen("/sd0/1/notiz.txt", 2, &mut r) == (d::EOF, 4), "70.0 EOF");
    pruefe(d::schreiben("/sd0/1/notiz.txt", 8, b"!") == (0, 1), "70.3 schreiben mit Lücke");
    let (_, n) = d::lesen("/sd0/1/notiz.txt", 0, &mut r);
    pruefe(n == 9 && r[6] == 0 && r[8] == b'!', "Lücke mit 0");
    pruefe(d::schreiben("/sd0/1/fehlt.txt", 0, b"x").0 == d::NICHT_GEFUNDEN, "70.3 fehlende Datei → 5");
    pruefe(d::groesse_setzen("/sd0/1/notiz.txt", 3) == 0, "70.4");
    let mut info = [0u8; 40];
    pruefe(d::info("/sd0/1/notiz.txt", &mut info) == 0 && u64::from_le_bytes(info[32..40].try_into().unwrap()) == 3, "70.5 Grösse 3");
    pruefe(info[0] & 0x10 == 0 && u16::from_le_bytes([info[30], info[31]]) >= 2026, "70.5 Datei, Datum");
    // Ordner, relativ, umbenennen, verschieben, löschen
    pruefe(d::ordner_erstellen("/sd0/1/bilder") == 0 && d::ordner_erstellen("/sd0/1/bilder") == 0, "70.9 (doppelt ok)");
    d::ordner_setzen("/sd0/1/bilder");
    pruefe(d::aktueller_ordner() == "/sd0/1/bilder", "30.1/30.2");
    pruefe(d::erstellen("a.txt", b"A").0 == 0 && d::lesen("../bilder/a.txt", 0, &mut r[..1]).1 == 1, "relativ + ..");
    pruefe(d::umbenennen("a.txt", "b.txt") == 0, "70.10 umbenennen relativ");
    pruefe(d::umbenennen("/sd0/1/bilder/b.txt", "/c.txt") == 0, "70.10 verschieben absolut");
    pruefe(d::info("/sd0/1/c.txt", &mut info) == 0, "verschoben nach Wurzel");
    let (c, n) = d::ordner_lesen("/sd0/1/bilder", 0, 10, 3, &mut vec![0u8; 32 + 10 * 560]);
    pruefe(c == d::EOF && n == 2, "leerer Ordner: . und ..");
    pruefe(d::loeschen("/sd0/1") == d::VERWEIGERT, "Wurzel nicht löschbar");
    pruefe(d::ordner_erstellen("/sd0/1/voll") == 0 && d::erstellen("/sd0/1/voll/x", b"1").0 == 0, "voller Ordner");
    pruefe(d::loeschen("/sd0/1/voll") == d::VERWEIGERT, "70.8 nicht leer → 10");
    pruefe(d::loeschen("/sd0/1/voll/x") == 0 && d::loeschen("/sd0/1/voll") == 0, "70.8 löschen");
    pruefe(d::lesen("/sd0/1/bilder", 0, &mut r).0 == d::VERWEIGERT, "Ordner lesen → 10");
    pruefe(d::lesen("/hd0/1/x", 0, &mut r).0 == d::NICHT_GEFUNDEN, "unbekanntes Gerät → 5");
    // Ordnerliste UTF-8 mit Umlaut, Attribute
    pruefe(d::erstellen("/sd0/1/Grüße.txt", b"x").0 == 0, "Umlaut-Name");
    let mut b8 = vec![0u8; 32 + 10 * 560];
    let (_, n) = d::ordner_lesen("/sd0/1", 0, 10, 3, &mut b8);
    let namen: Vec<String> = (0..n as usize)
        .map(|i| {
            let z = &b8[32 + i * 560 + 40..32 + i * 560 + 560];
            String::from_utf8_lossy(&z[..z.iter().position(|&c| c == 0).unwrap()]).into_owned()
        })
        .collect();
    pruefe(namen == ["bilder", "c.txt", "Grüße.txt", "notiz.txt"], &format!("Liste UTF-8: {namen:?}"));
    pruefe(u32::from_le_bytes(b8[8..12].try_into().unwrap()) == 4, "Gesamtzahl");
    let mut attr = [0u8; 32];
    attr[0] = 1;
    pruefe(d::attribute_setzen("/sd0/1/c.txt", &attr) == 0, "70.6");
    d::info("/sd0/1/c.txt", &mut info);
    pruefe(info[0] & 1 == 1, "Schreibschutz gesetzt");
    attr[0] = 0;
    d::attribute_setzen("/sd0/1/c.txt", &attr);
    pruefe(d::verknuepfung() == d::NICHT_UNTERSTUETZT, "70.11/12 FAT → 2");
    // 70.7 Programm starten mit Parametern
    let pid = d::starten("/sys/Kind", "hallo welt");
    pruefe(pid > 0, "70.7 starten");
    KIND.store(pid, std::sync::atomic::Ordering::SeqCst);
    pruefe(d::starten("/sys/gibtsnicht", "") == -5, "70.7 unbekannt → −5");
}

fn kind() {
    let a = kern::argumente();
    if a != "hallo welt" {
        panic!("Argumente: {a}");
    }
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "datei", heap_kb: 512, stapel_kb: 256, start: prog },
    ProgrammEintrag { name: "kind", heap_kb: 64, stapel_kb: 64, start: kind },
];

#[test]
fn funktion_70_auf_eingehaengten_ordnern() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_k6_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("sd")).unwrap();
    std::fs::create_dir_all(wurzel.join("sys")).unwrap();
    kern::init(&kern::PC, P);
    d::einhaengen("/sys", wurzel.join("sys"));
    d::einhaengen("/rd/1", wurzel.join("sys"));
    d::einhaengen("/sd0/1", wurzel.join("sd"));
    let pid = kern::starte_name("datei").unwrap();
    let t = Instant::now();
    let ende = loop {
        kern::aufraeumen();
        if let Some(e) = kern::ende_von(pid) {
            break e;
        }
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(ende, Ende::Normal);
    let f = FEHLER.lock().unwrap().clone();
    assert!(f.is_empty(), "Fehlgeschlagen: {f:#?}");
    // Kind lief mit Parametern ohne Panik
    let kind = KIND.load(std::sync::atomic::Ordering::SeqCst) as u32;
    let e = loop {
        kern::aufraeumen();
        if let Some(e) = kern::ende_von(kind) {
            break e;
        }
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(e, Ende::Normal, "Kind-Programm: Argumente");
    let _ = std::fs::remove_dir_all(&wurzel);
}
