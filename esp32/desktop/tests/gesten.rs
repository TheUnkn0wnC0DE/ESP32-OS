//! D3–D7-Nachweis (PC): Gesten über die Gestenerkennung (wie am Board vom
//! Touch-Treiber bzw. Autopiloten), Wirkung beim Programm geprüft, Fotos.

use desktop::gesten::Gesten;
use kern::{KernAlloc, ProgrammEintrag, sys};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static KNOPF: AtomicU32 = AtomicU32::new(0);
static SCROLL: AtomicI32 = AtomicI32::new(0);
static ZOOM: AtomicU32 = AtomicU32::new(1000);
static SERIELL: Mutex<()> = Mutex::new(());

fn ziel() {
    sys::maske(0x27);
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster(100 << 16 | 299, 60 << 16 | 199, 0x33 << 24 | 0xE6E6E6, 0, Some("Gesten"));
                sys::knopf(20 << 16 | 99, 20 << 16 | 39, 7, 0x3A7AC0);
                sys::neuzeichnen(2);
            }
            3 => KNOPF.store(sys::knopf_id(), Ordering::SeqCst),
            6 => {
                let s = sys::maus(7) as i16 as i32;
                SCROLL.fetch_add(s, Ordering::SeqCst);
                let (z, _, _) = sys::zoom_holen();
                if z != 1000 {
                    ZOOM.store(ZOOM.load(Ordering::SeqCst) * z / 1000, Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }
}

fn liste() {
    let _ = kolibri::dialog::datei_dialog(kolibri::dialog::Modus::Oeffnen, "/sys", "", (420, 330), (10, 10));
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "ziel", heap_kb: 256, stapel_kb: 128, start: ziel },
    ProgrammEintrag { name: "liste", heap_kb: 512, stapel_kb: 128, start: liste },
    ProgrammEintrag { name: "@icon", heap_kb: 1024, stapel_kb: 128, start: desktop::shell::icon },
    ProgrammEintrag { name: "info", heap_kb: 256, stapel_kb: 128, start: desktop::shell::info },
];

fn init() -> std::sync::MutexGuard<'static, ()> {
    let g = SERIELL.lock().unwrap_or_else(|e| e.into_inner());
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    g
}

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) {
    std::thread::sleep(Duration::from_millis(200));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::create_dir_all(&dir).unwrap();
    let ist = format!("{:016x}", fnv(&px));
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

/// Finger-Folge abspielen: (ms, Finger), dazwischen kurz warten.
fn folge(g: &mut Gesten, t0: &mut u64, schritte: &[(u64, [Option<(i32, i32)>; 2])]) {
    for &(dt, f) in schritte {
        *t0 += dt;
        g.schritt(*t0, f);
        std::thread::sleep(Duration::from_millis(5));
    }
    // Leerlaufschritt: offenen Klick loslassen (am Board kommt jedes Bild einer)
    *t0 += 100;
    g.schritt(*t0, LOS);
    std::thread::sleep(Duration::from_millis(80));
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn fenster(titel: &str) -> Option<(i32, i32, i32, i32)> {
    kern::kern_abschnitt(|| {
        kern::fenster::mit_welt(|w| {
            let s = w.fenster_liste().into_iter().find(|f| f.titel == titel)?.slot;
            let f = w.slots[s].as_ref()?.fenster.as_ref()?;
            Some((f.x, f.y, f.w, f.h))
        })
    })
}

const AUF: fn(i32, i32) -> [Option<(i32, i32)>; 2] = |x, y| [Some((x, y)), None];
const LOS: [Option<(i32, i32)>; 2] = [None, None];

#[test]
fn tippen_halten_wischen_zoomen_ziehen() {
    let _g = init();
    let pid = kern::starte_name("ziel").unwrap();
    bis(|| fenster("Gesten").is_some(), "Fenster");
    std::thread::sleep(Duration::from_millis(100));
    let mut g = Gesten::default();
    let mut t = 1000u64;
    // Knopf 7: Arbeitsbereich (105, 108) + (20, 20) → Mitte (175, 148)
    // D3: Tippen = Linksklick
    folge(&mut g, &mut t, &[(0, AUF(175, 148)), (80, AUF(176, 149)), (60, LOS)]);
    bis(|| KNOPF.load(Ordering::SeqCst) == 7 << 8, "Tippen → Knopf 7 links");
    // D4: Halten = Rechtsklick (al Bit 1)
    KNOPF.store(0, Ordering::SeqCst);
    let mut s = vec![(0, AUF(175, 148))];
    s.extend((0..14).map(|_| (50, AUF(175, 148))));
    s.push((50, LOS));
    folge(&mut g, &mut t, &s);
    bis(|| KNOPF.load(Ordering::SeqCst) == (7 << 8 | 2), "Halten → Knopf 7 rechts");
    // D5: Wischen nach oben 120 px in 150 ms → +10 Scrollschritte
    SCROLL.store(0, Ordering::SeqCst);
    let mut s = vec![(0, AUF(300, 280))];
    s.extend((1..=6).map(|i| (25, AUF(300, 280 - 20 * i))));
    s.push((20, LOS));
    folge(&mut g, &mut t, &s);
    bis(|| SCROLL.load(Ordering::SeqCst) == 10, "Wischen → Scroll +10");
    // D6: zwei Finger 100 → 200 px auseinander → Zoom ×2
    ZOOM.store(1000, Ordering::SeqCst);
    let mut s = vec![(0, [Some((250, 200)), Some((350, 200))])];
    s.extend((1..=10).map(|i| (30, [Some((250 - 5 * i, 200)), Some((350 + 5 * i, 200))])));
    s.push((30, LOS));
    folge(&mut g, &mut t, &s);
    bis(|| (1900..=2100).contains(&ZOOM.load(Ordering::SeqCst)), "Zoom ×2");
    // D7: Titelleiste ziehen um (+120, +40)
    let (x0, y0, w0, h0) = fenster("Gesten").unwrap();
    let mut s = vec![(0, AUF(x0 + 60, y0 + 20))];
    s.extend((1..=8).map(|i| (30, AUF(x0 + 60 + 15 * i, y0 + 20 + 5 * i))));
    s.push((30, LOS));
    folge(&mut g, &mut t, &s);
    let (x1, y1, _, _) = fenster("Gesten").unwrap();
    assert_eq!((x1 - x0, y1 - y0), (120, 40), "verschoben");
    bild("d7_verschoben");
    // D7: Doppeltipp auf die Titelleiste → maximiert (Arbeitsfläche), nochmal → zurück
    folge(&mut g, &mut t, &[(0, AUF(x1 + 60, y1 + 20)), (60, LOS), (120, AUF(x1 + 60, y1 + 20)), (60, LOS)]);
    bis(|| fenster("Gesten").is_some_and(|f| f.2 == 640), "maximiert");
    bild("d7_maximiert");
    folge(&mut g, &mut t, &[(1000, AUF(200, 20)), (60, LOS), (120, AUF(200, 20)), (60, LOS)]);
    bis(|| fenster("Gesten") == Some((x1, y1, w0, h0)), "wiederhergestellt");
    kern::beende(pid);
}

#[test]
fn wischen_blaettert_liste_und_kontextmenue() {
    let _g = init();
    let dir = std::env::temp_dir().join(format!("esp32os_d5_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..30 {
        std::fs::write(dir.join(format!("datei_{i:02}.txt")), b"x").unwrap();
    }
    kern::datei::einhaengen("/sys", &dir);
    let pid = kern::starte_name("liste").unwrap();
    bis(|| fenster("Open file").is_some(), "Dateidialog");
    bild("d5_liste_vorher");
    let mut g = Gesten::default();
    let mut t = 50_000u64;
    let mut s = vec![(0, AUF(200, 300))];
    s.extend((1..=8).map(|i| (20, AUF(200, 300 - 20 * i))));
    s.push((20, LOS));
    folge(&mut g, &mut t, &s);
    bild("d5_liste_nachher");
    kern::beende(pid);

    // D4: langer Druck auf ein Desktop-Symbol → Kontextmenü → „Öffnen“
    desktop::shell::anwendungen_setzen(&[desktop::shell::Anwendung { programm: "info", titel: "Info", titel_de: "Info", symbol: 113, auf_desktop: true }]);
    let icon = kern::starte_name("@icon").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut s = vec![(0, AUF(50, 40))];
    s.extend((0..14).map(|_| (50, AUF(50, 40))));
    s.push((50, LOS));
    folge(&mut g, &mut t, &s);
    std::thread::sleep(Duration::from_millis(200));
    bild("d4_kontextmenue");
    // erster Eintrag „Öffnen“ (Menü an der Druckstelle, Zeile 40 px)
    folge(&mut g, &mut t, &[(500, AUF(50 + 40, 40 + 20)), (60, LOS)]);
    bis(|| fenster("About ESP32-OS").is_some(), "Info über Kontextmenü geöffnet");
    kern::beende(icon);
    // Info wieder schließen: sonst steht das Fenster im Bild des anderen
    // Tests (gleicher Prozess, Reihenfolge der Tests nicht fest)
    for p in kern::liste().into_iter().filter(|p| p.name == "info") {
        kern::beende(p.pid);
    }
    bis(|| fenster("About ESP32-OS").is_none(), "Info geschlossen");
    let _ = std::fs::remove_dir_all(&dir);
}
