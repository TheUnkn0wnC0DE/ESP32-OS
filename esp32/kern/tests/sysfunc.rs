//! K2-Nachweis: Systemfunktionen je Gruppe mit Testprogrammen.
//! Bildvergleich: `tests/erwartet/<name>.png` (zum Ansehen) + `.fnv`
//! (Prüfsumme der Pixel). Neu erzeugen mit `ERWARTET_NEU=1`.

use kern::{Ende, KernAlloc, ProgrammEintrag, sys};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static SERIELL: Mutex<()> = Mutex::new(());
static BEREIT: AtomicBool = AtomicBool::new(false);
static KNOPF: AtomicU32 = AtomicU32::new(0);
static TASTE: AtomicU32 = AtomicU32::new(0);
static MAUS_EV: AtomicU32 = AtomicU32::new(0);
static WERT: AtomicU32 = AtomicU32::new(0);
static WERT2: AtomicU32 = AtomicU32::new(0);
static ZAHL: AtomicUsize = AtomicUsize::new(0);

const BLAU: u32 = 0x3060A0;

// ------------------------------------------------------------ Programme

fn zeichnen_inhalt() {
    sys::neuzeichnen(1);
    // Fenster 20,10 400×300, Stil 3 mit Titel, Koordinaten relativ zum Arbeitsbereich
    sys::fenster(20 << 16 | 399, 10 << 16 | 299, 0x33_FFFFFF, 0, Some("Zeichnen – Grüezi"));
    sys::rechteck(10 << 16 | 60, 10 << 16 | 40, 0xFF0000);
    sys::rechteck(80 << 16 | 60, 10 << 16 | 40, 0x8000_80FF);
    sys::linie(10 << 16 | 200, 60 << 16 | 120, 0x000000);
    sys::linie(10 << 16 | 200, 120 << 16 | 60, 0x00A000);
    for i in 0..50 {
        sys::pixel(220 + i, 60 + i / 2, 0x0000FF);
    }
    // Bild 7: 16×8 Verlauf BGR
    let mut bgr = Vec::new();
    for y in 0..8u8 {
        for x in 0..16u8 {
            bgr.extend_from_slice(&[x * 16, y * 32, 200]);
        }
    }
    sys::bild(&bgr, 16 << 16 | 8, 290 << 16 | 10);
    // 65: 1 bpp Schachbrett mit Palette
    sys::palettenbild(&[0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55], 8 << 16 | 8, 320 << 16 | 10, 1, &[0xFFFFFF, 0x000000], 0);
    // 73: 4×4 rot/grün 32 bpp
    let mut bgrx = Vec::new();
    for i in 0..16 {
        bgrx.extend_from_slice(if i % 2 == 0 { &[0, 0, 255, 255] } else { &[0, 255, 0, 255] });
    }
    sys::blit(0, [340, 10, 4, 4, 0, 0, 4, 4], &bgrx, 16);
    // Text: alle vier Schriften, Faktor 2, Hintergrund
    sys::text(10 << 16 | 130, 0x80_000000, b"6x9 cp866: KolibriOS\0", 0, 0);
    sys::text(10 << 16 | 142, 0x90_000080, b"8x16 cp866\0", 0, 0);
    let u16: Vec<u8> = "UTF-16: äöü".encode_utf16().flat_map(u16::to_le_bytes).chain([0, 0]).collect();
    sys::text(10 << 16 | 160, 0xA0_800000, &u16, 0, 0);
    sys::text(10 << 16 | 178, 0xB0_006000, "UTF-8: Grüezi ÄÖÜ ß €\0".as_bytes(), 0, 0);
    sys::text(10 << 16 | 198, 0xF1_FFFFFF, "Faktor 2\0".as_bytes(), 0, BLAU);
    sys::zahl(6 << 16 | 0x8000_0000, 4711, 250 << 16 | 130, 0x10_000000, 0);
    sys::zahl(4 << 16 | 1 << 8, 0xBEEF, 250 << 16 | 150, 0x10_C00000, 0);
    sys::knopf(250 << 16 | 99, 190 << 16 | 29, 10, 0x40A040);
    sys::text(262 << 16 | 197, 0xB0_FFFFFF, "Knopf\0".as_bytes(), 0, 0);
    sys::neuzeichnen(2);
}

fn prog_zeichnen() {
    loop {
        match sys::warte_ereignis() {
            1 => {
                zeichnen_inhalt();
                BEREIT.store(true, Ordering::SeqCst);
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                KNOPF.store(id, Ordering::SeqCst);
                if id == 1 {
                    sys::ende();
                }
            }
            2 => TASTE.store(sys::taste(), Ordering::SeqCst),
            _ => {}
        }
    }
}

fn prog_maus() {
    sys::maske(0x27); // Redraw, Taste, Knopf, Maus
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::fenster(100 << 16 | 199, 100 << 16 | 149, 0x34_E0E0E0, 0, Some("Maus"));
                sys::knopf(20 << 16 | 59, 20 << 16 | 29, 7, 0x8080C0);
                BEREIT.store(true, Ordering::SeqCst);
            }
            3 => KNOPF.store(sys::knopf_id() >> 8, Ordering::SeqCst),
            6 => {
                MAUS_EV.fetch_add(1, Ordering::SeqCst);
                WERT.store(sys::maus(1), Ordering::SeqCst);
                WERT2.store(sys::maus(2), Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

fn prog_zeit() {
    let t0 = sys::zeitzaehler();
    let n0 = sys::zeit_ns();
    let start = Instant::now();
    assert_eq!(sys::warte_ereignis_max(15), 0); // kein Fenster-Ereignis? Redraw kommt!
    let _ = start;
    let _ = (t0, n0);
}

fn prog_zeit2() {
    // Redraw-Ereignis zuerst abholen (neues Programm bekommt immer eins)
    assert_eq!(sys::pruefe_ereignis(), 1);
    sys::fenster(0, 0, 0x01_000000, 0, None);
    let t0 = sys::zeitzaehler();
    let n0 = sys::zeit_ns();
    let s = Instant::now();
    let e = sys::warte_ereignis_max(15);
    let d = s.elapsed();
    sys::warten(5);
    let t1 = sys::zeitzaehler();
    let n1 = sys::zeit_ns();
    let ok = e == 0 && d >= Duration::from_millis(140) && d < Duration::from_millis(400) && t1 >= t0 + 19 && n1 > n0;
    WERT.store(u32::from(ok), Ordering::SeqCst);
    let uhr = sys::uhrzeit();
    let datum = sys::datum();
    // BCD-Ziffern gültig
    let bcd_ok = |v: u32| (0..6).all(|i| (v >> (4 * i)) & 0xF <= 9);
    WERT2.store(u32::from(bcd_ok(uhr) && bcd_ok(datum) && uhr & 0xFF <= 0x23), Ordering::SeqCst);
}

fn prog_info() {
    sys::fenster(30 << 16 | 99, 40 << 16 | 79, 0x34_FFFFFF, 0, Some("Info"));
    let mut b = [0u8; 1024];
    let max = sys::faden_info(-1, &mut b);
    let name = String::from_utf8_lossy(&b[10..21]).to_string();
    let tid = u32::from_le_bytes(b[30..34].try_into().unwrap());
    let x = u32::from_le_bytes(b[34..38].try_into().unwrap());
    let w = u32::from_le_bytes(b[42..46].try_into().unwrap());
    let ok = name == "info       " && tid == kern::eigene_tid().unwrap() && x == 30 && w == 99 && max >= 2;
    WERT.store(u32::from(ok), Ordering::SeqCst);
    // Speicher 68
    let p = sys::speicher_holen(1000);
    let ok2 = !p.is_null() && unsafe {
        p.write_bytes(0x5A, 1000);
        let q = sys::speicher_neu(p, 1000, 5000);
        let gut = !q.is_null() && *q.add(999) == 0x5A;
        gut && sys::speicher_frei(q)
    };
    // Stil 48
    let mut f = [0u8; 40];
    sys::stil(3, 0, 40, &mut f);
    let arbeit = u32::from_le_bytes(f[20..24].try_into().unwrap());
    let (h, _) = sys::stil(4, 0, 0, &mut []);
    WERT2.store(u32::from(ok2 && arbeit == 0xE6E6E6 && h == 22), Ordering::SeqCst);
    // zweiter Faden mit eigenem Fenster
    let tid2 = sys::faden(64, Box::new(|| {
        sys::fenster(200 << 16 | 99, 40 << 16 | 79, 0x34_FFFFFF, 0, Some("Faden 2"));
        ZAHL.store(sys::system(7, 0) as usize, Ordering::SeqCst);
        loop {
            sys::warte_ereignis_max(2);
        }
    }));
    assert!(tid2 > 0);
    loop {
        sys::warte_ereignis_max(2);
    }
}

static PROGRAMME: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "zeichnen", heap_kb: 512, stapel_kb: 128, start: prog_zeichnen },
    ProgrammEintrag { name: "maus", heap_kb: 256, stapel_kb: 128, start: prog_maus },
    ProgrammEintrag { name: "zeit", heap_kb: 64, stapel_kb: 64, start: prog_zeit },
    ProgrammEintrag { name: "zeit2", heap_kb: 64, stapel_kb: 64, start: prog_zeit2 },
    ProgrammEintrag { name: "info", heap_kb: 256, stapel_kb: 128, start: prog_info },
    ProgrammEintrag { name: "zeiger", heap_kb: 256, stapel_kb: 128, start: prog_zeiger },
];

// ------------------------------------------------------------ Hilfen

fn init() -> std::sync::MutexGuard<'static, ()> {
    let g = SERIELL.lock().unwrap_or_else(|e| e.into_inner());
    kern::init(&kern::PC, PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    BEREIT.store(false, Ordering::SeqCst);
    KNOPF.store(0, Ordering::SeqCst);
    TASTE.store(0, Ordering::SeqCst);
    MAUS_EV.store(0, Ordering::SeqCst);
    WERT.store(0, Ordering::SeqCst);
    WERT2.store(0, Ordering::SeqCst);
    g
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn ende(pid: u32) -> Ende {
    let t = Instant::now();
    loop {
        kern::aufraeumen();
        if let Some(e) = kern::ende_von(pid) {
            return e;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "Programm {pid} endet nicht");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn beenden(pid: u32) {
    kern::beende(pid);
    ende(pid);
}

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bildvergleich(name: &str) {
    let (w, h, px) = sys::foto();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    let png = dir.join(format!("{name}.png"));
    let sum = dir.join(format!("{name}.fnv"));
    let ist = format!("{:016x}", fnv(&px));
    if std::env::var_os("ERWARTET_NEU").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&png, spatz::image::encode_png(w, h, &px)).unwrap();
        std::fs::write(&sum, &ist).unwrap();
        return;
    }
    let soll = std::fs::read_to_string(&sum).unwrap_or_else(|_| panic!("{sum:?} fehlt – ERWARTET_NEU=1"));
    if soll.trim() != ist {
        let neu = dir.join(format!("{name}.ist.png"));
        std::fs::write(&neu, spatz::image::encode_png(w, h, &px)).unwrap();
        panic!("Bild {name} weicht ab, siehe {neu:?}");
    }
}

// ------------------------------------------------------------ Tests

#[test]
fn zeichnen_text_bilder_erwartungsbild() {
    let _g = init();
    let pid = kern::starte_name("zeichnen").unwrap();
    bis(|| BEREIT.load(Ordering::SeqCst), "Fenster gezeichnet");
    std::thread::sleep(Duration::from_millis(20));
    bildvergleich("k2_zeichnen");
    // Pixelproben: rotes Rechteck im Arbeitsbereich (Fenster 20,10; Arbeitsbereich +5,+22)
    let (w, _, px) = sys::foto();
    assert_eq!(px[(10 + 22 + 20) * w + 20 + 5 + 30], 0xFF0000);
    // Knopf 10 antippen (Arbeitsbereich-relativ 250,190 → Bildschirm 275,222)
    sys::eingabe::maus(300, 230, 1);
    sys::eingabe::maus(300, 230, 0);
    bis(|| KNOPF.load(Ordering::SeqCst) == 10, "Knopf 10");
    // Taste
    sys::eingabe::taste(u32::from(b'a') << 8);
    bis(|| TASTE.load(Ordering::SeqCst) == u32::from(b'a') << 8, "Taste a");
    // Schliessen-Knopf (Fenster rechts oben: x = 20+400-20+8, y = 10+3+8)
    sys::eingabe::maus(20 + 400 - 12, 10 + 11, 1);
    sys::eingabe::maus(20 + 400 - 12, 10 + 11, 0);
    assert_eq!(ende(pid), Ende::Normal);
    let (_, _, px) = sys::foto();
    assert!(px.iter().all(|&p| p == kern::grafik::von565(kern::grafik::zu565(0x3A6EA5))), "Fenster nicht weg");
}

#[test]
fn maus_knopf_und_maske() {
    let _g = init();
    let pid = kern::starte_name("maus").unwrap();
    bis(|| BEREIT.load(Ordering::SeqCst), "Fenster");
    sys::eingabe::maus(150, 160, 0);
    bis(|| MAUS_EV.load(Ordering::SeqCst) > 0, "Mausereignis");
    // relativ zum Arbeitsbereich (100+5, 100+22) wie KolibriOS gui/mouse.inc
    bis(|| WERT.load(Ordering::SeqCst) == (45 << 16 | 38), "relative Mauskoordinaten");
    // Knopf 7 im Arbeitsbereich (Fenster 100,100 + 5,22 + 20,20)
    sys::eingabe::maus(140, 150, 1);
    bis(|| WERT2.load(Ordering::SeqCst) == 1, "Taste gehalten");
    sys::eingabe::maus(140, 150, 0);
    bis(|| KNOPF.load(Ordering::SeqCst) == 7, "Knopf 7");
    // Drücken auf Knopf, Loslassen daneben → kein Knopfereignis
    KNOPF.store(0, Ordering::SeqCst);
    sys::eingabe::maus(140, 150, 1);
    sys::eingabe::maus(250, 200, 0);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(KNOPF.load(Ordering::SeqCst), 0);
    // Minimieren (0xFFFF) behandelt das System
    sys::eingabe::maus(100 + 200 - 38 + 8, 100 + 11, 1);
    sys::eingabe::maus(100 + 200 - 38 + 8, 100 + 11, 0);
    std::thread::sleep(Duration::from_millis(20));
    let (_, _, px) = sys::foto();
    assert!(px.iter().all(|&p| p == kern::grafik::von565(kern::grafik::zu565(0x3A6EA5))), "nicht minimiert");
    assert_eq!(KNOPF.load(Ordering::SeqCst), 0);
    beenden(pid);
}

#[test]
fn zeit_und_warten() {
    let _g = init();
    let pid = kern::starte_name("zeit2").unwrap();
    assert_eq!(ende(pid), Ende::Normal);
    assert_eq!(WERT.load(Ordering::SeqCst), 1, "Zeitzähler/Wartezeiten");
    assert_eq!(WERT2.load(Ordering::SeqCst), 1, "Uhrzeit/Datum BCD");
    let _ = prog_zeit;
}

#[test]
fn info_speicher_stil_faeden() {
    let _g = init();
    ZAHL.store(0, Ordering::SeqCst);
    let pid = kern::starte_name("info").unwrap();
    bis(|| ZAHL.load(Ordering::SeqCst) != 0, "zweiter Faden");
    assert_eq!(WERT.load(Ordering::SeqCst), 1, "Funktion 9");
    assert_eq!(WERT2.load(Ordering::SeqCst), 1, "Funktion 68/48");
    std::thread::sleep(Duration::from_millis(20));
    bildvergleich("k2_zwei_fenster");
    // 18.7: aktives Fenster ist das des zweiten Fadens; 18.3 aktiviert das erste
    let aktiv = ZAHL.load(Ordering::SeqCst) as u32;
    let erster = sys::system(21, pid);
    assert_ne!(aktiv, erster);
    sys::system(3, erster);
    assert_eq!(sys::system(7, 0), erster);
    // 18.2: Programm über Slot beenden – beide Fäden enden
    assert_eq!(sys::system(2, erster), 0);
    assert_eq!(ende(pid), Ende::Beendet);
    assert!(kern::liste().iter().all(|p| p.pid != pid));
}

static ZEIGER_OK: AtomicU32 = AtomicU32::new(0);

fn prog_zeiger() {
    sys::pruefe_ereignis();
    sys::fenster(300 << 16 | 199, 100 << 16 | 149, 0x34_FFFFFF, 0, Some("Zeiger"));
    // 37.4 (ARGB, Hotspot 0,0): grünes Quadrat; 37.5 setzen
    let gruen: Vec<u8> = (0..1024).flat_map(|_| 0xFF00_C000u32.to_le_bytes()).collect();
    let griff = sys::zeiger_laden(kern::zeiger::argb(&gruen, (0, 0)));
    let alt = sys::zeiger_setzen(griff);
    ZEIGER_OK.store(u32::from(griff > 0 && alt == 0), Ordering::SeqCst);
    loop {
        sys::warte_ereignis_max(5);
    }
}

#[test]
fn eigener_zeiger_37_4_37_5() {
    let _g = init();
    ZEIGER_OK.store(0, Ordering::SeqCst);
    let pid = kern::starte_name("zeiger").unwrap();
    bis(|| ZEIGER_OK.load(Ordering::SeqCst) == 1, "Zeiger geladen und gesetzt");
    sys::eingabe::touchpad_setzen(true);
    // über dem Fenster: grüner Zeiger; daneben: Pfeil (Spitze schwarz/weiss)
    sys::eingabe::maus(400, 200, 0);
    std::thread::sleep(Duration::from_millis(30));
    let (w, _, px) = sys::foto();
    assert_eq!(px[200 * w + 400 + 5], kern::grafik::von565(kern::grafik::zu565(0x00C000)), "eigener Zeiger über dem Fenster");
    sys::eingabe::maus(50, 50, 0);
    std::thread::sleep(Duration::from_millis(30));
    let (w, _, px) = sys::foto();
    assert_ne!(px[50 * w + 50 + 5], kern::grafik::von565(kern::grafik::zu565(0x00C000)), "Pfeil ausserhalb");
    sys::eingabe::touchpad_setzen(false);
    beenden(pid);
}
