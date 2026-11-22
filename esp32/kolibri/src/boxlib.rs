//! box_lib nativ (KolibriOS `develop/libraries/box_lib`): edit_box,
//! check_box, scrollbar, progressbar, option_box, frame – Strukturen mit
//! demselben Speicherlayout wie `clayer/boxlib.h`, Zeichnen über die
//! Systemfunktionen des Kerns (Fenster- bzw. Arbeitsbereichskoordinaten wie
//! beim Original).
//!
//! Touch: Ein edit_box mit Fokus meldet das dem Kern
//! (`kern::sys::textfeld_fokus`) – daran hängt die Bildschirmtastatur (D8).
//! Kodierung des Puffers nach der Schrift in `text_color` (wie Funktion 4):
//! Schrift 3 = UTF-8, sonst cp866. Tasten aus Funktion 2: al = 0 mit
//! cp866-Code in ah, oder (ESP32-OS-Erweiterung, Bildschirmtastatur) al = 3
//! mit Unicode in den oberen 16 Bit – so kommen auch ä ö ü € an.

use kern::sys;

const ED_PASS: u32 = 1;
const ED_FOCUS: u32 = 2;
const ED_FIGURE_ONLY: u32 = 0x8000;
const ED_ALWAYS_FOCUS: u32 = 0x4000;
const ED_DISABLED: u32 = 0x800;
const ED_MOUSE_ON: u32 = 0x100;
const ED_INSERT: u32 = 0x80;
/// ESP32-OS: Rechte Taste (langer Druck) schon behandelt.
const ED_RECHTS: u32 = 0x1_0000;

#[repr(C, packed)]
pub struct EditBox {
    pub width: u32,
    pub left: u32,
    pub top: u32,
    pub color: u32,
    pub shift_color: u32,
    pub focus_border_color: u32,
    pub blur_border_color: u32,
    pub text_color: u32,
    pub max: u32,
    pub text: *mut u8,
    pub mouse_variable: *mut *mut EditBox,
    pub flags: u32,
    pub size: u32,
    pub pos: u32,
    pub offset: u32,
    pub cl_curs_x: u32,
    pub cl_curs_y: u32,
    pub shift: u32,
    pub shift_old: u32,
    pub height: u32,
    pub char_width: u32,
}

fn klein(text_color: u32) -> bool {
    text_color & 0x3000_0000 == 0
}

fn utf8(text_color: u32) -> bool {
    text_color & 0x3000_0000 == 0x3000_0000
}

/// Funktion-2-Wert für ein Unicode-Zeichen (ESP32-OS-Erweiterung al = 3).
pub fn unicode_taste(c: char) -> u32 {
    (c as u32 & 0xFFFF) << 16 | 3
}

impl EditBox {
    fn bytes(&self) -> &[u8] {
        if self.text.is_null() {
            return &[];
        }
        // SAFETY: Programm verwaltet `max`+1 Bytes, `size` davon belegt.
        unsafe { core::slice::from_raw_parts(self.text, (self.size as usize).min(self.max as usize)) }
    }
    fn setze(&mut self, neu: &[u8]) {
        if self.text.is_null() {
            return;
        }
        let n = neu.len().min(self.max as usize);
        // Nicht mitten in einem UTF-8-Zeichen abschneiden
        let n = if utf8(self.text_color) {
            (0..=n).rev().find(|&i| core::str::from_utf8(&neu[..i]).is_ok()).unwrap_or(0)
        } else {
            n
        };
        // SAFETY: Puffer hat max+1 Bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(neu.as_ptr(), self.text, n);
            *self.text.add(n) = 0;
        }
        self.size = n as u32;
        self.pos = self.pos.min(self.size);
    }
    fn masse(&mut self) {
        let (cw, ch) = if klein(self.text_color) { (6, 9) } else { (8, 16) };
        self.char_width = cw;
        self.height = ch + 6;
    }
    fn dekodiere(&self, b: &[u8]) -> Vec<char> {
        if utf8(self.text_color) {
            String::from_utf8_lossy(b).chars().collect()
        } else {
            b.iter().map(|&c| char::from_u32(kern::schrift::cp866_zu_uni(c)).unwrap_or('?')).collect()
        }
    }
    fn zeichen(&self) -> Vec<char> {
        self.dekodiere(self.bytes())
    }
    fn zeichen_pos(&self) -> usize {
        self.dekodiere(&self.bytes()[..(self.pos as usize).min(self.size as usize)]).len()
    }
    fn laenge(&self, c: char) -> usize {
        if utf8(self.text_color) { c.len_utf8() } else { 1 }
    }
    fn byte_pos(&self, z: usize) -> u32 {
        self.zeichen().iter().take(z).map(|&c| self.laenge(c)).sum::<usize>() as u32
    }
    /// Zeichen in der Kodierung des Puffers (None = nicht darstellbar).
    fn kodiere(&self, c: char) -> Option<Vec<u8>> {
        if utf8(self.text_color) {
            let mut b = [0u8; 4];
            return Some(c.encode_utf8(&mut b).as_bytes().to_vec());
        }
        let k = kern::schrift::uni_zu_cp866(c);
        (k != b'?' || c == '?').then(|| vec![k])
    }
}

/// Text in der Schrift des Feldes (Bits 28/29 von text_color, Faktor 1).
fn text_feld(x: i32, y: i32, text: &str, farbe: u32) {
    if text.is_empty() {
        return;
    }
    let pos = (x as u32) << 16 | y as u32 & 0xFFFF;
    if utf8(farbe) {
        sys::text(pos, 0x3000_0000 | farbe & 0x00FF_FFFF, text.as_bytes(), text.chars().count() as u32, 0);
    } else {
        let b: Vec<u8> = text.chars().map(kern::schrift::uni_zu_cp866).collect();
        sys::text(pos, farbe & 0x1000_0000 | farbe & 0x00FF_FFFF, &b, b.len() as u32, 0);
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32, c: u32) {
    if w > 0 && h > 0 {
        sys::rechteck((x as u32) << 16 | w as u32 & 0xFFFF, (y as u32) << 16 | h as u32 & 0xFFFF, c);
    }
}

fn rahmen(x: i32, y: i32, w: i32, h: i32, c: u32) {
    rect(x, y, w, 1, c);
    rect(x, y + h - 1, w, 1, c);
    rect(x, y, 1, h, c);
    rect(x + w - 1, y, 1, h, c);
}

/// Mausposition relativ zum Fenster bzw. Arbeitsbereich (wie Funktion 37.1).
fn maus() -> (i32, i32, bool) {
    let m = sys::maus(1);
    ((m >> 16) as i16 as i32, m as i16 as i32, sys::maus(2) & 1 != 0)
}

pub fn edit_box_draw(ed: &mut EditBox) {
    ed.masse();
    let (x, y, w, h) = (ed.left as i32, ed.top as i32, ed.width as i32, ed.height as i32);
    let fokus = ed.flags & (ED_FOCUS | ED_ALWAYS_FOCUS) != 0;
    rahmen(x, y, w, h, if fokus { ed.focus_border_color } else { ed.blur_border_color });
    rect(x + 1, y + 1, w - 2, h - 2, if ed.flags & ED_DISABLED != 0 { 0xCACACA } else { ed.color });
    let cw = ed.char_width as i32;
    let platz = ((w - 6) / cw).max(1) as usize;
    let zeichen: Vec<char> = if ed.flags & ED_PASS != 0 { vec!['*'; ed.zeichen().len()] } else { ed.zeichen() };
    let zp = ed.zeichen_pos();
    let mut off = ed.offset as usize;
    if zp < off {
        off = zp;
    }
    if zp > off + platz {
        off = zp - platz;
    }
    ed.offset = off as u32;
    let ty = y + (h - if klein(ed.text_color) { 9 } else { 16 }) / 2;
    // Auswahl
    if ed.shift != ed.shift_old && fokus {
        let (a, b) = (ed.shift.min(ed.shift_old) as usize, ed.shift.max(ed.shift_old) as usize);
        let za = String::from_utf8_lossy(&ed.bytes()[..a.min(ed.size as usize)]).chars().count();
        let zb = String::from_utf8_lossy(&ed.bytes()[..b.min(ed.size as usize)]).chars().count();
        let (va, vb) = (za.max(off), zb.min(off + platz));
        if vb > va {
            rect(x + 3 + (va - off) as i32 * cw, ty - 1, (vb - va) as i32 * cw, h - 4, ed.shift_color);
        }
    }
    let sichtbar: String = zeichen.iter().skip(off).take(platz).collect();
    text_feld(x + 3, ty, &sichtbar, ed.text_color);
    if fokus && ed.flags & ED_DISABLED == 0 {
        // Auch ein von Anfang an fokussiertes Feld meldet sich (Bildschirmtastatur),
        // samt Lage, damit die Tastatur es nicht verdeckt.
        if ed.flags & ED_FOCUS != 0 {
            sys::textfeld_fokus(true);
            sys::textfeld_lage(x, y, w, h);
        }
        let cx = x + 3 + (zp - off) as i32 * cw;
        rect(cx, y + 2, 1, h - 4, ed.text_color & 0xFF_FFFF);
        ed.cl_curs_x = cx as u32;
        ed.cl_curs_y = (y + 2) as u32;
    }
}

fn fokus_setzen(ed: &mut EditBox, an: bool) {
    let vorher = ed.flags & ED_FOCUS != 0;
    if an {
        ed.flags |= ED_FOCUS;
        if !ed.mouse_variable.is_null() {
            // SAFETY: Programm übergibt edit_box** als Fokus-Merker.
            unsafe { *ed.mouse_variable = ed as *mut EditBox };
        }
    } else {
        ed.flags &= !ED_FOCUS;
    }
    if vorher != an {
        sys::textfeld_fokus(an);
    }
}

/// Markierter Text (oder alles, wenn nichts markiert).
fn auswahl_text(ed: &EditBox) -> (usize, usize) {
    let (a, b) = (ed.shift.min(ed.shift_old) as usize, (ed.shift.max(ed.shift_old) as usize).min(ed.size as usize));
    if a == b { (0, ed.size as usize) } else { (a, b) }
}

/// Kontextmenü des Feldes (langer Druck): Ausschneiden, Kopieren,
/// Einfügen, Alles markieren – über die Zwischenablage (Funktion 54).
fn kontext(ed: &mut EditBox) {
    let m = sys::maus(0);
    let passwort = ed.flags & ED_PASS != 0;
    let wahl = crate::dialog::kontextmenue((m >> 16) as i32, (m & 0xFFFF) as i32, &[kern::sprache::t("Cut", "Ausschneiden"), kern::sprache::t("Copy", "Kopieren"), kern::sprache::t("Paste", "Einfügen"), kern::sprache::t("Select all", "Alles markieren")]);
    match wahl {
        // Ausschneiden nur mit Markierung (sonst ginge der ganze Text verloren)
        Some(0) if ed.shift == ed.shift_old => {}
        Some(0 | 1) if !passwort => {
            let (a, b) = auswahl_text(ed);
            let t: String = ed.dekodiere(&ed.bytes()[a..b]).into_iter().collect();
            sys::ablage_text_setzen(&t);
            if wahl == Some(0) {
                let mut v = ed.bytes().to_vec();
                v.drain(a..b);
                ed.pos = a as u32;
                ed.setze(&v);
                ed.shift = ed.pos;
                ed.shift_old = ed.pos;
            }
        }
        Some(2) => {
            if let Some(t) = sys::ablage_text() {
                edit_box_einfuegen(ed, &t);
            }
        }
        Some(3) => {
            ed.shift_old = 0;
            ed.shift = ed.size;
            ed.pos = ed.size;
        }
        _ => {}
    }
    edit_box_draw(ed);
}

pub fn edit_box_mouse(ed: &mut EditBox) {
    if ed.flags & ED_DISABLED != 0 {
        return;
    }
    ed.masse();
    let (mx, my, links) = maus();
    let (x, y, w, h) = (ed.left as i32, ed.top as i32, ed.width as i32, ed.height as i32);
    let drin = mx >= x && my >= y && mx < x + w && my < y + h;
    let rechts = sys::maus(2) & 2 != 0;
    if rechts && drin && ed.flags & ED_RECHTS == 0 {
        ed.flags |= ED_RECHTS;
        fokus_setzen(ed, true);
        kontext(ed);
        return;
    }
    if !rechts {
        ed.flags &= !ED_RECHTS;
    }
    if links {
        if drin {
            let z = (((mx - x - 3) + ed.char_width as i32 / 2) / ed.char_width as i32).max(0) as usize + ed.offset as usize;
            let z = z.min(ed.zeichen().len());
            let p = ed.byte_pos(z);
            if ed.flags & ED_MOUSE_ON == 0 {
                // Druck beginnt: Cursor setzen, Auswahl leeren
                ed.flags |= ED_MOUSE_ON;
                ed.shift_old = p;
            }
            ed.pos = p;
            ed.shift = p;
            fokus_setzen(ed, true);
            edit_box_draw(ed);
        } else if ed.flags & ED_MOUSE_ON == 0 && ed.flags & ED_ALWAYS_FOCUS == 0 && ed.flags & ED_FOCUS != 0 {
            fokus_setzen(ed, false);
            edit_box_draw(ed);
        }
    } else {
        ed.flags &= !ED_MOUSE_ON;
    }
}

fn auswahl_loeschen(ed: &mut EditBox) -> bool {
    if ed.shift == ed.shift_old {
        return false;
    }
    let (a, b) = (ed.shift.min(ed.shift_old) as usize, (ed.shift.max(ed.shift_old) as usize).min(ed.size as usize));
    let mut v = ed.bytes().to_vec();
    v.drain(a..b);
    ed.pos = a as u32;
    ed.setze(&v);
    ed.shift = ed.pos;
    ed.shift_old = ed.pos;
    true
}

/// Taste aus Funktion 2 (`key_val`: ah = Code, Bits 16–23 Scancode).
pub fn edit_box_key(ed: &mut EditBox, key_val: u32) {
    if ed.flags & (ED_FOCUS | ED_ALWAYS_FOCUS) == 0 || ed.flags & ED_DISABLED != 0 {
        return;
    }
    if key_val & 0xFF == 1 {
        return; // Puffer war leer
    }
    if key_val & 0xFF == 3 {
        if let Some(c) = char::from_u32(key_val >> 16) {
            edit_box_einfuegen(ed, &c.to_string());
            edit_box_draw(ed);
        }
        return;
    }
    let code = (key_val >> 8 & 0xFF) as u8;
    let z = ed.zeichen();
    let zp = ed.zeichen_pos();
    match code {
        8 => {
            if !auswahl_loeschen(ed) && zp > 0 {
                let a = ed.byte_pos(zp - 1);
                let mut v = ed.bytes().to_vec();
                v.drain(a as usize..ed.pos as usize);
                ed.pos = a;
                ed.setze(&v);
            }
        }
        0xB6 => {
            if !auswahl_loeschen(ed) && zp < z.len() {
                let b = ed.byte_pos(zp + 1);
                let mut v = ed.bytes().to_vec();
                v.drain(ed.pos as usize..b as usize);
                ed.setze(&v);
            }
        }
        0xB0 => ed.pos = ed.byte_pos(zp.saturating_sub(1)),
        0xB3 => ed.pos = ed.byte_pos((zp + 1).min(z.len())),
        0xB4 => ed.pos = 0,
        0xB5 => ed.pos = ed.size,
        0xB9 => ed.flags ^= ED_INSERT,
        13 | 9 | 27 | 0xB1 | 0xB2 => {}
        c if c >= 32 => {
            let ch = char::from_u32(kern::schrift::cp866_zu_uni(c)).unwrap_or('?');
            if ed.flags & ED_FIGURE_ONLY != 0 && !ch.is_ascii_digit() {
                return;
            }
            let Some(s) = ed.kodiere(ch) else { return };
            auswahl_loeschen(ed);
            let mut v = ed.bytes().to_vec();
            let s = s.as_slice();
            let p = ed.pos as usize;
            if ed.flags & ED_INSERT != 0 && p < v.len() {
                let e = ed.byte_pos(ed.zeichen_pos() + 1) as usize;
                v.drain(p..e);
            }
            if v.len() + s.len() > ed.max as usize {
                return;
            }
            v.splice(p..p, s.iter().copied());
            ed.pos = (p + s.len()) as u32;
            ed.setze(&v);
        }
        _ => {}
    }
    ed.shift = ed.pos;
    ed.shift_old = ed.pos;
    edit_box_draw(ed);
}

/// Unicode-Text einfügen (Bildschirmtastatur, Einfügen aus der
/// Zwischenablage); nicht darstellbare Zeichen (cp866-Feld) entfallen.
pub fn edit_box_einfuegen(ed: &mut EditBox, text: &str) {
    if ed.flags & ED_FIGURE_ONLY != 0 && !text.chars().all(|c| c.is_ascii_digit()) {
        return;
    }
    auswahl_loeschen(ed);
    let mut v = ed.bytes().to_vec();
    let p = ed.pos as usize;
    let rest = ed.max as usize - v.len().min(ed.max as usize);
    let mut t: Vec<u8> = Vec::new();
    for c in text.chars() {
        if let Some(b) = ed.kodiere(c) {
            if t.len() + b.len() > rest {
                break;
            }
            t.extend_from_slice(&b);
        }
    }
    v.splice(p..p, t.iter().copied());
    ed.pos = (p + t.len()) as u32;
    ed.setze(&v);
    ed.shift = ed.pos;
    ed.shift_old = ed.pos;
}

pub fn edit_box_set_text(ed: &mut EditBox, text: &[u8]) {
    let n = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    ed.setze(&text[..n]);
    ed.pos = ed.size;
    ed.offset = 0;
    ed.shift = ed.pos;
    ed.shift_old = ed.pos;
}

// ---------------------------------------------------------- check_box

#[repr(C, packed)]
pub struct CheckBox {
    pub left_s: u32,
    pub top_s: u32,
    pub ch_text_margin: u32,
    pub color: u32,
    pub border_color: u32,
    pub text_color: u32,
    pub text: *const u8,
    pub flags: u32,
    pub size_of_str: u32,
}

const CH_AN: u32 = 2;

fn ctext(p: *const u8) -> &'static [u8] {
    if p.is_null() {
        return &[];
    }
    // SAFETY: nullterminierter String des Programms.
    unsafe { core::ffi::CStr::from_ptr(p.cast()) }.to_bytes()
}

/// Programmtext in Unicode, Kodierung nach Schrift (3 = UTF-8, sonst cp866).
fn als_text(b: &[u8], farbe: u32) -> String {
    if utf8(farbe) {
        String::from_utf8_lossy(b).into_owned()
    } else {
        b.iter().map(|&c| char::from_u32(kern::schrift::cp866_zu_uni(c)).unwrap_or('?')).collect()
    }
}

pub fn init_checkbox2(cb: &mut CheckBox) {
    let n = als_text(ctext(cb.text), cb.text_color).chars().count() as u32;
    cb.size_of_str = n * if klein(cb.text_color) { 6 } else { 8 };
}

pub fn check_box_draw2(cb: &mut CheckBox) {
    let (x, w) = ((cb.left_s >> 16) as i32, (cb.left_s & 0xFFFF) as i32);
    let (y, h) = ((cb.top_s >> 16) as i32, (cb.top_s & 0xFFFF) as i32);
    rect(x, y, w, h, cb.color);
    rahmen(x, y, w, h, cb.border_color);
    if cb.flags & CH_AN != 0 {
        rect(x + 3, y + 3, w - 6, h - 6, cb.border_color);
    }
    let th = if klein(cb.text_color) { 9 } else { 16 };
    let t = als_text(ctext(cb.text), cb.text_color);
    text_feld(x + w + cb.ch_text_margin as i32, y + (h - th) / 2, &t, cb.text_color);
}

pub fn check_box_mouse2(cb: &mut CheckBox) {
    let (mx, my, links) = maus();
    let (x, w) = ((cb.left_s >> 16) as i32, (cb.left_s & 0xFFFF) as i32);
    let (y, h) = ((cb.top_s >> 16) as i32, (cb.top_s & 0xFFFF) as i32);
    let breit = w + cb.ch_text_margin as i32 + cb.size_of_str as i32;
    let drin = mx >= x && my >= y && mx < x + breit && my < y + h;
    // Bit 31: Taste war beim letzten Aufruf gedrückt (Umschalten beim Loslassen)
    if links {
        if drin {
            cb.flags |= 0x8000_0000;
        }
    } else if cb.flags & 0x8000_0000 != 0 {
        cb.flags &= !0x8000_0000;
        if drin {
            cb.flags ^= CH_AN;
            check_box_draw2(cb);
        }
    }
}

// ---------------------------------------------------------- scrollbar

#[repr(C, packed)]
pub struct Scrollbar {
    pub xsize: u16,
    pub xpos: u16,
    pub ysize: u16,
    pub ypos: u16,
    pub btn_height: u32,
    pub typ: u32,
    pub max_area: u32,
    pub cur_area: u32,
    pub position: u32,
    pub back_color: u32,
    pub front_color: u32,
    pub line_color: u32,
    pub redraw: u32,
    pub delta: u16,
    pub delta2: u16,
    pub r_size_x: u16,
    pub r_start_x: u16,
    pub r_size_y: u16,
    pub r_start_y: u16,
    pub m_pos: u32,
    pub m_pos2: u32,
    pub m_keys: u32,
    pub run_size: u32,
    pub position2: u32,
    pub work_size: u32,
    pub all_redraw: u32,
    pub ar_offset: u32,
}

/// (Bahnanfang, Bahnlänge, Griffanfang, Grifflänge) entlang der Achse.
fn bahn(s: &Scrollbar, laenge: i32, anfang: i32) -> (i32, i32, i32, i32) {
    let btn = s.btn_height as i32;
    let (b0, bl) = (anfang + btn, (laenge - 2 * btn).max(1));
    let max = s.max_area.max(1) as i32;
    let cur = (s.cur_area as i32).clamp(0, max);
    let gl = (bl * cur / max).max(8.min(bl));
    let spiel = (max - cur).max(1);
    let pos = (s.position as i32).clamp(0, spiel);
    let g0 = b0 + (bl - gl) * pos / spiel;
    (b0, bl, g0, gl)
}

fn scroll_zeichnen(s: &mut Scrollbar, vertikal: bool) {
    let (x, y, w, h) = (s.xpos as i32, s.ypos as i32, s.xsize as i32, s.ysize as i32);
    rect(x, y, w, h, s.back_color);
    rahmen(x, y, w, h, s.line_color);
    let (laenge, anfang) = if vertikal { (h, y) } else { (w, x) };
    let btn = s.btn_height as i32;
    if btn > 0 {
        for p in [anfang, anfang + laenge - btn] {
            if vertikal {
                rect(x + 1, p + 1, w - 2, btn - 2, s.front_color);
            } else {
                rect(p + 1, y + 1, btn - 2, h - 2, s.front_color);
            }
        }
    }
    let (_, _, g0, gl) = bahn(s, laenge, anfang);
    if vertikal {
        rect(x + 1, g0, w - 2, gl, s.front_color);
        rahmen(x + 1, g0, w - 2, gl, s.line_color);
    } else {
        rect(g0, y + 1, gl, h - 2, s.front_color);
        rahmen(g0, y + 1, gl, h - 2, s.line_color);
    }
}

fn scroll_maus(s: &mut Scrollbar, vertikal: bool) {
    let (mx, my, links) = maus();
    let (x, y, w, h) = (s.xpos as i32, s.ypos as i32, s.xsize as i32, s.ysize as i32);
    let (laenge, anfang, m) = if vertikal { (h, y, my) } else { (w, x, mx) };
    let drin = mx >= x && my >= y && mx < x + w && my < y + h;
    if !links {
        s.m_keys = 0;
        return;
    }
    if s.m_keys == 0 && !drin {
        return;
    }
    let (b0, bl, g0, gl) = bahn(s, laenge, anfang);
    let spiel = s.max_area.saturating_sub(s.cur_area);
    let btn = s.btn_height as i32;
    if s.m_keys == 0 {
        s.m_keys = 1;
        if btn > 0 && m < anfang + btn {
            s.position = s.position.saturating_sub(1);
            s.m_keys = 2;
        } else if btn > 0 && m >= anfang + laenge - btn {
            s.position = (s.position + 1).min(spiel);
            s.m_keys = 2;
        } else if m >= g0 && m < g0 + gl {
            s.m_pos = (m - g0) as u32;
        } else {
            // Klick in die Bahn: Griff mittig dorthin
            s.m_pos = (gl / 2) as u32;
        }
    }
    if s.m_keys == 1 && bl > gl {
        let neu = (m - s.m_pos as i32 - b0).clamp(0, bl - gl);
        s.position = (neu as i64 * i64::from(spiel) / i64::from(bl - gl)) as u32;
    }
    s.redraw = 1;
    scroll_zeichnen(s, vertikal);
}

pub fn scrollbar_v_draw(s: &mut Scrollbar) {
    scroll_zeichnen(s, true);
}
pub fn scrollbar_h_draw(s: &mut Scrollbar) {
    scroll_zeichnen(s, false);
}
pub fn scrollbar_v_mouse(s: &mut Scrollbar) {
    scroll_maus(s, true);
}
pub fn scrollbar_h_mouse(s: &mut Scrollbar) {
    scroll_maus(s, false);
}

// ---------------------------------------------------------- progressbar

#[repr(C)]
pub struct Progressbar {
    pub value: u32,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub style: u32,
    pub min: u32,
    pub max: u32,
    pub back_color: u32,
    pub progress_color: u32,
    pub frame_color: u32,
}

pub fn progressbar_draw(p: &mut Progressbar) {
    let (x, y, w, h) = (p.left as i32, p.top as i32, p.width as i32, p.height as i32);
    rahmen(x, y, w, h, p.frame_color);
    let span = p.max.saturating_sub(p.min).max(1);
    let v = p.value.clamp(p.min, p.max) - p.min;
    let fw = ((w - 2) as i64 * i64::from(v) / i64::from(span)) as i32;
    rect(x + 1, y + 1, fw, h - 2, p.progress_color);
    rect(x + 1 + fw, y + 1, w - 2 - fw, h - 2, p.back_color);
}

pub fn progressbar_progress(p: &mut Progressbar) {
    if p.value < p.max {
        p.value += 1;
    }
    progressbar_draw(p);
}

// ---------------------------------------------------------- option_box

#[repr(C, packed)]
pub struct OptionBox {
    pub selected: *mut *mut OptionBox,
    pub posx: u16,
    pub posy: u16,
    pub text_margin: u32,
    pub size: u32,
    pub color: u32,
    pub border_color: u32,
    pub text_color: u32,
    pub text: *const u8,
    pub text_len: u32,
    pub flags: u32,
}

/// `liste`: nullterminierte Liste von Zeigern (wie im Original).
fn optionen(liste: *mut *mut OptionBox) -> Vec<*mut OptionBox> {
    let mut v = Vec::new();
    if liste.is_null() {
        return v;
    }
    // SAFETY: nullterminierte Zeigerliste des Programms.
    unsafe {
        let mut p = liste;
        while !(*p).is_null() {
            v.push(*p);
            p = p.add(1);
        }
    }
    v
}

/// # Safety
/// `liste` ist eine gültige, nullterminierte Liste.
pub unsafe fn option_box_draw(liste: *mut *mut OptionBox) {
    for o in optionen(liste) {
        // SAFETY: Element der Liste.
        let o = unsafe { &mut *o };
        let (x, y, s) = (i32::from(o.posx), i32::from(o.posy), o.size as i32);
        rect(x, y, s, s, o.color);
        rahmen(x, y, s, s, o.border_color);
        // SAFETY: `selected` zeigt auf den Merker der Gruppe.
        let gewaehlt = !o.selected.is_null() && unsafe { *o.selected } == o as *mut OptionBox;
        if gewaehlt {
            rect(x + s / 4, y + s / 4, s - s / 2, s - s / 2, o.border_color);
        }
        let t = ctext(o.text);
        let t = if o.text_len > 0 { &t[..(o.text_len as usize).min(t.len())] } else { t };
        let th = if klein(o.text_color) { 9 } else { 16 };
        text_feld(x + s + o.text_margin as i32, y + (s - th) / 2, &als_text(t, o.text_color), o.text_color);
    }
}

/// # Safety
/// wie [`option_box_draw`].
pub unsafe fn option_box_mouse(liste: *mut *mut OptionBox) {
    let (mx, my, links) = maus();
    if !links {
        return;
    }
    for o in optionen(liste) {
        // SAFETY: Element der Liste.
        let r = unsafe { &mut *o };
        let (x, y, s) = (i32::from(r.posx), i32::from(r.posy), r.size as i32);
        let breite = s + r.text_margin as i32 + ctext(r.text).len() as i32 * 8;
        if mx >= x && my >= y && mx < x + breite && my < y + s && !r.selected.is_null() {
            // SAFETY: Merker der Gruppe.
            unsafe { *r.selected = o };
            unsafe { option_box_draw(liste) };
            return;
        }
    }
}

// ---------------------------------------------------------- frame

#[repr(C)]
pub struct Frame {
    pub typ: u32,
    pub x_w: u32,
    pub y_h: u32,
    pub ext_col: u32,
    pub int_col: u32,
    pub flags: u32,
    pub text_pointer: *const u8,
    pub text_position: u32,
    pub font_number: u32,
    pub font_size_y: u32,
    pub font_color: u32,
    pub font_bg_color: u32,
}

pub fn frame_draw(f: &mut Frame) {
    let (x, w) = ((f.x_w >> 16) as i32, (f.x_w & 0xFFFF) as i32);
    let (y, h) = ((f.y_h >> 16) as i32, (f.y_h & 0xFFFF) as i32);
    rahmen(x, y, w, h, f.ext_col);
    rahmen(x + 1, y + 1, w - 2, h - 2, f.int_col);
    let t = String::from_utf8_lossy(ctext(f.text_pointer)).into_owned();
    if !t.is_empty() {
        let fh = if f.font_number == 0 { 9 } else { 16 };
        let ty = if f.text_position == 0 { y - fh / 2 } else { y + h - fh / 2 };
        let farbe = if f.font_number == 0 { f.font_color & 0xFF_FFFF } else { f.font_color | 0x1000_0000 };
        let n = t.chars().count() as i32 * if f.font_number == 0 { 6 } else { 8 };
        rect(x + 8, ty, n + 4, fh, f.font_bg_color);
        text_feld(x + 10, ty, &t, farbe);
    }
}

// ---------------------------------------------------------- C-ABI

#[cfg(target_pointer_width = "32")]
mod c {
    use super::*;

    macro_rules! export {
        ($name:ident, $typ:ty, $f:path) => {
            #[unsafe(no_mangle)]
            pub unsafe extern "system" fn $name(p: *mut $typ) {
                if let Some(p) = unsafe { p.as_mut() } {
                    $f(p);
                }
            }
        };
    }
    export!(edit_box_draw, EditBox, super::edit_box_draw);
    export!(edit_box_mouse, EditBox, super::edit_box_mouse);
    export!(check_box_draw2, CheckBox, super::check_box_draw2);
    export!(check_box_mouse2, CheckBox, super::check_box_mouse2);
    export!(init_checkbox2, CheckBox, super::init_checkbox2);
    export!(scrollbar_v_draw, Scrollbar, super::scrollbar_v_draw);
    export!(scrollbar_h_draw, Scrollbar, super::scrollbar_h_draw);
    export!(scrollbar_v_mouse, Scrollbar, super::scrollbar_v_mouse);
    export!(scrollbar_h_mouse, Scrollbar, super::scrollbar_h_mouse);
    export!(progressbar_draw, Progressbar, super::progressbar_draw);
    export!(progressbar_progress, Progressbar, super::progressbar_progress);
    export!(frame_draw, Frame, super::frame_draw);

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn edit_box_key(p: *mut EditBox, key: u32) {
        if let Some(p) = unsafe { p.as_mut() } {
            super::edit_box_key(p, key);
        }
    }

    /// `ksys_oskey_t` ist ein 32-Bit-Wert wie Funktion 2.
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn edit_box_key_safe(p: *mut EditBox, key: u32) {
        if let Some(p) = unsafe { p.as_mut() } {
            super::edit_box_key(p, key);
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn edit_box_set_text(p: *mut EditBox, t: *const u8) {
        if let Some(p) = unsafe { p.as_mut() } {
            super::edit_box_set_text(p, ctext(t));
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn option_box_draw(l: *mut *mut OptionBox) {
        unsafe { super::option_box_draw(l) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn option_box_mouse(l: *mut *mut OptionBox) {
        unsafe { super::option_box_mouse(l) }
    }
}
