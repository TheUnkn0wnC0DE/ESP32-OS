//! A2 (PC): Rückmeldung der Bildschirmtastatur. Solange der Finger auf einer
//! Taste liegt, ist sie kleiner und dunkler; nur ihre Fläche ändert sich.
//! Der Kern markiert dabei in der Tastatur nur die Zelle der Taste (Dirty
//! Rect). Nach dem Loslassen sieht die Tastatur wieder aus wie vorher. Geprüft für
//! ein Zeichen, Shift, Löschen, Leerzeichen und Enter. Foto der gedrückten
//! Taste.

use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, sondertaste, tastenmitte};
use kern::{KernAlloc, ProgrammEintrag, sys};
use kolibri::boxlib::{EditBox, edit_box_draw, edit_box_key, edit_box_mouse};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn editor() {
    let mut puffer = vec![0u8; 128];
    let mut ed = EditBox {
        width: 400, left: 10, top: 10, color: 0xFFFFFF, shift_color: 0x94AECE, focus_border_color: 0x3A7AC0,
        blur_border_color: 0x8A8A8A, text_color: 0x3000_0000, max: 127, text: puffer.as_mut_ptr(),
        mouse_variable: core::ptr::null_mut(), flags: 0, size: 0, pos: 0, offset: 0, cl_curs_x: 0, cl_curs_y: 0,
        shift: 0, shift_old: 0, height: 0, char_width: 0,
    };
    sys::maske(0x27);
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster(20 << 16 | 599, 10 << 16 | 99, 0x33 << 24 | 0xF0F0F0, 0, Some("Notiz"));
                edit_box_draw(&mut ed);
                sys::neuzeichnen(2);
            }
            2 => edit_box_key(&mut ed, sys::taste()),
            6 => edit_box_mouse(&mut ed),
            _ => {}
        }
    }
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "editor", heap_kb: 256, stapel_kb: 128, start: editor },
    ProgrammEintrag { name: "@tastatur", heap_kb: 512, stapel_kb: 128, start: desktop::tastatur::dienst },
    ProgrammEintrag { name: "@taskbar", heap_kb: 512, stapel_kb: 128, start: desktop::shell::taskbar },
];

const GRUND: u32 = 0x2C3444;

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Bildschirm (640×360) ohne Uhr der Taskleiste.
fn foto() -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(120));
    let (w, h, px) = sys::foto();
    assert_eq!((w, h), (640, 360));
    px
}

/// Vom Kern als geändert gemerkte Bereiche abholen (und leeren).
fn schmutz() -> Vec<kern::grafik::Clip> {
    kern::fenster::mit_welt(|w| w.schmutz_holen())
}

fn rgb(c: u32) -> u32 {
    c & 0xFF_FFFF
}

fn helligkeit(c: u32) -> u32 {
    (c >> 16 & 255) + (c >> 8 & 255) + (c & 255)
}

#[test]
fn gedrueckte_taste_kleiner_und_dunkler() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    for p in ["@taskbar", "@tastatur", "editor"] {
        kern::starte_name(p).unwrap();
    }
    std::thread::sleep(Duration::from_millis(400));
    let mut g = Gesten::default();
    let mut t = 1000u64;
    let mut tippen = |g: &mut Gesten, x: i32, y: i32| {
        g.schritt(t, [Some((x, y)), None]);
        t += 80;
        std::thread::sleep(Duration::from_millis(5));
        g.schritt(t, [None, None]);
        t += 100;
        std::thread::sleep(Duration::from_millis(30));
        g.schritt(t, [None, None]);
        t += 100;
    };
    tippen(&mut g, 20 + 5 + 10 + 50, 10 + 48 + 10 + 10);
    bis(|| sys::tastatur_soll().0, "Tastatur erscheint");
    std::thread::sleep(Duration::from_millis(300));
    // Lage wie in tastatur::dienst: Textfeld frei lassen
    let y_unten = 360 - desktop::shell::LEISTE_H - HOEHE;
    let oben = match sys::tastatur_soll().1 {
        Some(f) if f.y1 > y_unten && f.y0 >= HOEHE => 0,
        Some(f) if f.y1 > y_unten => (f.y1 + 2).min(360 - HOEHE),
        _ => y_unten,
    };
    let (qx, qy) = tastenmitte('q', 640).map(|(_, x, y)| (x, y)).unwrap();
    let (lx, ly) = tastenmitte(' ', 640).map(|(_, x, y)| (x, y)).unwrap();
    let tasten = [("q", (qx, qy)), ("rueck", sondertaste("rueck", 640)), ("leer", (lx, ly)), ("enter", sondertaste("enter", 640)), ("shift", sondertaste("shift", 640))];
    let mut t = 5000u64;
    for (name, (x, y)) in tasten {
        let (x, y) = (x, oben + y);
        let vorher = foto();
        // Finger aufsetzen und liegen lassen (kein Klick bis zum Loslassen);
        // die vom Kern als geändert gemerkten Bereiche mitlesen
        schmutz();
        g.schritt(t, [Some((x, y)), None]);
        std::thread::sleep(Duration::from_millis(150));
        let beim_druecken = schmutz();
        let gedrueckt = foto();
        // geändert: nur die Fläche dieser Taste (eine Zelle, ≤ 64×46)
        let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (i, (a, n)) in vorher.iter().zip(&gedrueckt).enumerate() {
            let (px, py) = ((i % 640) as i32, (i / 640) as i32);
            if a != n && !(px >= 580 && py >= 320) {
                b = (b.0.min(px), b.1.min(py), b.2.max(px), b.3.max(py));
            }
        }
        assert!(b.0 <= x && x <= b.2 && b.1 <= y && y <= b.3, "{name}: geänderte Fläche {b:?} enthält die Taste ({x}, {y})");
        assert!(b.3 - b.1 < desktop::tastatur::HOEHE / 5 + 2, "{name}: nur eine Tastenzeile geändert: {b:?}");
        // Dirty Rect: in der Tastatur nur die Zelle dieser Taste markiert
        let zelle = (b.0 - 2, b.1 - 2, b.2 + 3, b.3 + 3);
        let nur_taste = |rs: &[kern::grafik::Clip], wann: &str| {
            for r in rs.iter().filter(|r| r.y1 > oben && r.y0 < oben + HOEHE) {
                assert!(
                    r.x0 >= zelle.0 && r.y0 >= zelle.1 && r.x1 <= zelle.2 && r.y1 <= zelle.3,
                    "{name}, {wann}: markiert {r:?} – erwartet nur die Taste {zelle:?}"
                );
            }
        };
        nur_taste(&beim_druecken, "drücken");
        // Rand der Taste jetzt Tastaturgrund (kleiner), Mitte dunkler
        let rand = gedrueckt[(b.1 * 640 + b.0) as usize];
        // Bildschirm in RGB565: je Kanal bis 7 daneben
        let nah = |a: u32, b: u32| [16, 8, 0].iter().all(|&k| (a >> k & 255).abs_diff(b >> k & 255) <= 7);
        assert!(nah(rgb(rand), GRUND), "{name}: Rand zeigt den Grund ({:06x})", rgb(rand));
        let mitte_vorher = vorher[((y + 13) * 640 + x) as usize];
        let mitte = gedrueckt[((y + 13) * 640 + x) as usize];
        assert!(helligkeit(mitte) < helligkeit(mitte_vorher), "{name}: dunkler ({mitte:06x} < {mitte_vorher:06x})");
        if name == "q" {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
            desktop::bild_schreiben(&dir.join("a2_taste_gedrueckt.png"), &spatz::image::encode_png(640, 360, &gedrueckt));
        }
        // loslassen → Klick; danach wieder wie vorher (Shift wechselt die Ebene)
        schmutz();
        t += 80;
        g.schritt(t, [None, None]);
        t += 200;
        std::thread::sleep(Duration::from_millis(30));
        g.schritt(t, [None, None]);
        t += 1000;
        std::thread::sleep(Duration::from_millis(150));
        let beim_loslassen = schmutz();
        let nachher = foto();
        if name != "shift" {
            nur_taste(&beim_loslassen, "loslassen");
            let tastatur = (oben * 640) as usize..(oben + HOEHE) as usize * 640;
            let anders: Vec<(usize, u32, u32)> =
                tastatur.clone().filter(|&i| vorher[i] != nachher[i]).map(|i| (i, vorher[i], nachher[i])).take(3).collect();
            let n = tastatur.filter(|&i| vorher[i] != nachher[i]).count();
            assert!(n == 0, "{name}: Tastatur nach dem Loslassen wie vorher – {n} Punkte anders, z. B. {:?}",
                anders.iter().map(|&(i, a, b)| (i % 640, i / 640, a & 0xFF_FFFF, b & 0xFF_FFFF)).collect::<Vec<_>>());
        }
    }
}
