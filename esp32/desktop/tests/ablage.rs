//! D9-Nachweis (PC): Kopieren und Einfügen per Kontextmenü (langer Druck)
//! zwischen zwei Textfeldern über die Zwischenablage (Funktion 54).

use desktop::gesten::Gesten;
use kern::{KernAlloc, ProgrammEintrag, sys};
use kolibri::boxlib::{EditBox, edit_box_draw, edit_box_key, edit_box_mouse, unicode_taste};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static TEXTE: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

fn feld(p: &mut [u8], top: u32) -> EditBox {
    EditBox {
        width: 400, left: 10, top, color: 0xFFFFFF, shift_color: 0x94AECE, focus_border_color: 0x3A7AC0,
        blur_border_color: 0x8A8A8A, text_color: 0x3000_0000, max: (p.len() - 1) as u32, text: p.as_mut_ptr(),
        mouse_variable: core::ptr::null_mut(), flags: 0, size: 0, pos: 0, offset: 0, cl_curs_x: 0, cl_curs_y: 0,
        shift: 0, shift_old: 0, height: 0, char_width: 0,
    }
}

fn editor() {
    let (mut pa, mut pb) = (vec![0u8; 128], vec![0u8; 128]);
    let (mut a, mut b) = (feld(&mut pa, 10), feld(&mut pb, 60));
    sys::maske(0x27);
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster(20 << 16 | 599, 10 << 16 | 159, 0x33 << 24 | 0xF0F0F0, 0, Some("Ablage"));
                edit_box_draw(&mut a);
                edit_box_draw(&mut b);
                sys::neuzeichnen(2);
            }
            2 => {
                let k = sys::taste();
                edit_box_key(&mut a, k);
                edit_box_key(&mut b, k);
            }
            6 => {
                edit_box_mouse(&mut a);
                edit_box_mouse(&mut b);
            }
            _ => {}
        }
        *TEXTE.lock().unwrap() = (String::from_utf8_lossy(&pa[..a.size as usize]).into_owned(), String::from_utf8_lossy(&pb[..b.size as usize]).into_owned());
    }
}

static P: &[ProgrammEintrag] = &[ProgrammEintrag { name: "editor", heap_kb: 256, stapel_kb: 128, start: editor }];

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn halten(&mut self, x: i32, y: i32, ms: u64) {
        let mut d = 0;
        while d <= ms {
            self.g.schritt(self.t + d, [Some((x, y)), None]);
            d += 50;
            std::thread::sleep(Duration::from_millis(5));
        }
        self.t += ms + 50;
        self.g.schritt(self.t, [None, None]);
        // echte Zeit wie am Board: das Programm liest den Druck (37.2)
        std::thread::sleep(Duration::from_millis(60));
        self.t += 100;
        self.g.schritt(self.t, [None, None]);
        self.t += 100;
        std::thread::sleep(Duration::from_millis(150));
    }
    fn tippen(&mut self, x: i32, y: i32) {
        self.halten(x, y, 80);
    }
}

#[test]
fn kopieren_einfuegen_per_kontextmenue() {
    kern::init(&kern::PC, P);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::starte_name("editor").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    // Feld A: Fenster 20,10 + 5,48 + 10,10
    let (ax, ay) = (20 + 5 + 10 + 60, 10 + 48 + 10 + 10);
    let (bx, by) = (ax, 10 + 48 + 60 + 10);
    f.tippen(ax, ay);
    for c in "Grüezi Wält".chars() {
        sys::eingabe::taste(if c.is_ascii() { u32::from(c as u8) << 8 } else { unicode_taste(c) });
    }
    bis(|| TEXTE.lock().unwrap().0 == "Grüezi Wält", "Text in A");
    // langer Druck in A → Kontextmenü → „Kopieren“ (2. Eintrag)
    f.halten(ax, ay, 700);
    std::thread::sleep(Duration::from_millis(200));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join("d9_kontextmenue.png"), spatz::image::encode_png(w, h, &px)).unwrap();
    f.tippen(ax + 40, ay + 2 + 44 + 22);
    bis(|| sys::ablage_text().as_deref() == Some("Grüezi Wält"), "in der Zwischenablage");
    // Feld B antippen, langer Druck → „Einfügen“ (3. Eintrag)
    f.tippen(bx, by);
    f.halten(bx, by, 700);
    f.tippen(bx + 40, by + 2 + 2 * 44 + 22);
    bis(|| TEXTE.lock().unwrap().1 == "Grüezi Wält", "in B eingefügt");
    let (w, h, px) = sys::foto();
    std::fs::write(dir.join("d9_eingefuegt.png"), spatz::image::encode_png(w, h, &px)).unwrap();
}
