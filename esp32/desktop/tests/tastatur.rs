//! D8-Nachweis (PC): Textfeld antippen → Bildschirmtastatur erscheint
//! automatisch, „Grüezi äöü @#“ per Touch getippt, Ausblenden über die
//! Taskleiste. Fotos.

use desktop::gesten::Gesten;
use desktop::tastatur::{HOEHE, sondertaste, tastenmitte};
use kern::{KernAlloc, ProgrammEintrag, sys};
use kolibri::boxlib::{EditBox, edit_box_draw, edit_box_key, edit_box_mouse};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static TEXT: Mutex<String> = Mutex::new(String::new());

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
            3 => {
                if sys::knopf_id() >> 8 == 1 {
                    return;
                }
            }
            _ => {}
        }
        *TEXT.lock().unwrap() = String::from_utf8_lossy(&puffer[..ed.size as usize]).into_owned();
    }
}

static P: &[ProgrammEintrag] = &[
    ProgrammEintrag { name: "editor", heap_kb: 256, stapel_kb: 128, start: editor },
    ProgrammEintrag { name: "@tastatur", heap_kb: 512, stapel_kb: 128, start: desktop::tastatur::dienst },
    ProgrammEintrag { name: "@taskbar", heap_kb: 512, stapel_kb: 128, start: desktop::shell::taskbar },
];

fn fnv(px: &[u32]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3))
}

fn bild(name: &str) {
    std::thread::sleep(Duration::from_millis(250));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::create_dir_all(&dir).unwrap();
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
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn tippen(&mut self, x: i32, y: i32) {
        self.g.schritt(self.t, [Some((x, y)), None]);
        self.t += 80;
        std::thread::sleep(Duration::from_millis(5));
        self.g.schritt(self.t, [None, None]);
        self.t += 100;
        std::thread::sleep(Duration::from_millis(30));
        self.g.schritt(self.t, [None, None]);
        self.t += 100;
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn bildschirmtastatur_tippt_umlaute_und_sonderzeichen() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    for p in ["@taskbar", "@tastatur", "editor"] {
        kern::starte_name(p).unwrap();
    }
    std::thread::sleep(Duration::from_millis(400));
    assert!(!sys::tastatur_soll().0, "ohne Fokus keine Tastatur");
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    // Textfeld: Fenster 20,10 + Arbeitsbereich 5,48 + Feld 10,10
    f.tippen(20 + 5 + 10 + 50, 10 + 48 + 10 + 10);
    bis(|| sys::tastatur_soll().0, "Tastatur erscheint bei Fokus");
    std::thread::sleep(Duration::from_millis(200));
    let oben = 360 - desktop::shell::LEISTE_H - HOEHE;
    let taste = |f: &mut Finger, c: char| {
        let (ebene, x, y) = tastenmitte(c, 640).unwrap_or_else(|| panic!("keine Taste für {c}"));
        match ebene {
            "gross" => {
                let (sx, sy) = sondertaste("shift", 640);
                f.tippen(sx, oben + sy);
            }
            "sonder" => {
                let (sx, sy) = sondertaste("123", 640);
                f.tippen(sx, oben + sy);
            }
            _ => {}
        }
        f.tippen(x, oben + y);
        if ebene == "sonder" {
            // zurück zu „abc“ (gleiche Taste)
            let (sx, sy) = sondertaste("123", 640);
            f.tippen(sx, oben + sy);
        }
    };
    for c in "Grüezi äöü @#".chars() {
        taste(&mut f, c);
    }
    std::thread::sleep(Duration::from_millis(300));
    let ist = TEXT.lock().unwrap().clone();
    if ist != "Grüezi äöü @#" {
        bild("d8_fehler");
    }
    assert_eq!(ist, "Grüezi äöü @#", "Text angekommen");
    bild("d8_tastatur");
    // Ausblenden über den Tastaturknopf der Taskleiste (rechts, vor der Uhr)
    f.tippen(640 - 112 + 2 + 21, 360 - 20);
    bis(|| !sys::tastatur_soll().0, "Tastatur über Taskleiste aus");
    bild("d8_ausgeblendet");
    f.tippen(640 - 112 + 2 + 21, 360 - 20);
    bis(|| sys::tastatur_soll().0, "Tastatur über Taskleiste ein");
}
