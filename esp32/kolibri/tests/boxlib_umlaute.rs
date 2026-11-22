//! K7: edit_box mit UTF-8-Schrift nimmt Umlaute über die Unicode-Taste
//! (Bildschirmtastatur) an; cp866-Feld nimmt Kyrillisch, lehnt ü ab.

use kern::{KernAlloc, ProgrammEintrag, sys};
use kolibri::boxlib::{EditBox, edit_box_draw, edit_box_key, unicode_taste};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static ERGEBNIS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn feld(puffer: &mut [u8], farbe: u32) -> EditBox {
    EditBox {
        width: 300, left: 10, top: 10, color: 0xFFFFFF, shift_color: 0xA0A0FF, focus_border_color: 0, blur_border_color: 0x808080,
        text_color: farbe, max: (puffer.len() - 1) as u32, text: puffer.as_mut_ptr(), mouse_variable: core::ptr::null_mut(),
        flags: 2, size: 0, pos: 0, offset: 0, cl_curs_x: 0, cl_curs_y: 0, shift: 0, shift_old: 0, height: 0, char_width: 0,
    }
}

fn prog() {
    sys::pruefe_ereignis();
    sys::fenster(0 << 16 | 399, 0 << 16 | 199, 0x34_FFFFFF, 0, Some("Umlaute"));
    let mut p1 = [0u8; 64];
    let mut e = feld(&mut p1, 0x3000_0000);
    edit_box_draw(&mut e);
    for c in "Grüezi äöü €".chars() {
        let k = if c.is_ascii() { u32::from(c as u8) << 8 } else { unicode_taste(c) };
        edit_box_key(&mut e, k);
    }
    edit_box_key(&mut e, 8 << 8); // Rücktaste löscht €
    let s1 = String::from_utf8_lossy(&p1[..e.size as usize]).into_owned();
    let mut p2 = [0u8; 64];
    let mut c = feld(&mut p2, 0x1000_0000);
    for ch in ['Ж', 'ü', 'A'] {
        edit_box_key(&mut c, unicode_taste(ch));
    }
    let s2: String = p2[..c.size as usize].iter().map(|&b| char::from_u32(kern::schrift::cp866_zu_uni(b)).unwrap()).collect();
    *ERGEBNIS.lock().unwrap() = vec![s1, s2];
}

static P: &[ProgrammEintrag] = &[ProgrammEintrag { name: "umlaute", heap_kb: 256, stapel_kb: 128, start: prog }];

#[test]
fn umlaute_ueber_unicode_taste() {
    kern::init(&kern::PC, P);
    let pid = kern::starte_name("umlaute").unwrap();
    let t = Instant::now();
    while kern::ende_von(pid).is_none() {
        kern::aufraeumen();
        assert!(t.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    let e = ERGEBNIS.lock().unwrap().clone();
    assert_eq!(e, ["Grüezi äöü ", "ЖA"]);
}
