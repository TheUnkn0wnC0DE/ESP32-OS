//! proc_lib nativ (KolibriOS `develop/libraries/proc_lib`): Datei-Dialog
//! (Öffnen/Speichern/Ordner wählen) und Farbdialog. Im Original startet
//! proc_lib die Programme `opendial`/`colrdial`; hier läuft der Dialog als
//! eigener Faden mit eigenem Touch-Fenster im aufrufenden Programm, der
//! Aufrufer wartet wie beim Original auf das Ergebnis.
//!
//! Touch: grosse Zeilen (32 px), Wischen/Pfeiltasten zum Blättern, Tippen auf
//! Ordner öffnet ihn, auf Datei wählt sie; beim Speichern Eingabefeld
//! (Bildschirmtastatur über box_lib).

use crate::boxlib::{EditBox, edit_box_draw, edit_box_key, edit_box_mouse};
use kern::{datei, sys};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modus {
    Oeffnen = 0,
    Speichern = 1,
    Ordner = 2,
}

const ZEILE: i32 = 32;
const KNOPF_OK: u32 = 2;
const KNOPF_AB: u32 = 3;
const KNOPF_HOCH: u32 = 4;
const KNOPF_RUNTER: u32 = 5;
const KNOPF_ELTERN: u32 = 6;
const KNOPF_EINTRAG: u32 = 100;

fn eintraege(pfad: &str) -> Vec<(String, bool)> {
    let mut v = Vec::new();
    let mut start = 0;
    loop {
        let mut b = vec![0u8; 32 + 64 * 560];
        let (c, n) = datei::ordner_lesen(pfad, start, 64, 3, &mut b);
        for i in 0..n as usize {
            let o = 32 + i * 560;
            let attr = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
            let z = &b[o + 40..o + 560];
            let name = String::from_utf8_lossy(&z[..z.iter().position(|&c| c == 0).unwrap_or(0)]).into_owned();
            if name != "." && name != ".." {
                v.push((name, attr & 0x10 != 0));
            }
        }
        if c != 0 || n < 64 {
            break;
        }
        start += 64;
    }
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    v
}

fn eltern(p: &str) -> String {
    let t = datei::normalisieren(p, "/");
    format!("/{}", t[..t.len().saturating_sub(1)].join("/"))
}

fn text(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32, 0x3000_0000 | farbe, t.as_bytes(), t.chars().count() as u32, 0);
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, farbe: u32, beschriftung: &str, tf: u32) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, farbe);
    let n = beschriftung.chars().count() as i32 * 8;
    text(x + (w - n) / 2, y + (h - 16) / 2, beschriftung, tf);
}

/// Datei-Dialog (blockiert, bis gewählt oder abgebrochen). Liefert den
/// vollständigen KolibriOS-Pfad.
pub fn datei_dialog(modus: Modus, start: &str, name: &str, groesse: (u32, u32), lage: (u32, u32)) -> Option<String> {
    let ergebnis: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
    let e2 = ergebnis.clone();
    let start = if start.is_empty() { "/sys".to_string() } else { start.to_string() };
    let name = name.to_string();
    let tid = sys::faden(
        128,
        Box::new(move || {
            let r = dialog_lauf(modus, start, name, groesse, lage);
            *e2.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        }),
    );
    if tid < 0 {
        return None;
    }
    loop {
        if let Some(r) = ergebnis.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return r;
        }
        sys::warten(2);
    }
}

fn dialog_lauf(modus: Modus, mut pfad: String, name: String, groesse: (u32, u32), lage: (u32, u32)) -> Option<String> {
    let (w, h) = (groesse.0.max(300) as i32, groesse.1.max(240) as i32);
    let titel = match modus {
        Modus::Oeffnen => kern::sprache::t("Open file", "Datei öffnen"),
        Modus::Speichern => kern::sprache::t("Save file", "Datei speichern"),
        Modus::Ordner => kern::sprache::t("Select folder", "Ordner wählen"),
    };
    let mut namenpuffer = vec![0u8; 256];
    let mut feld = EditBox {
        width: (w - 20) as u32,
        left: 5,
        top: 0,
        color: 0xFFFFFF,
        shift_color: 0x94AECE,
        focus_border_color: 0x3A7AC0,
        blur_border_color: 0x8A8A8A,
        text_color: 0x1000_0000,
        max: 255,
        text: namenpuffer.as_mut_ptr(),
        mouse_variable: core::ptr::null_mut(),
        flags: 0,
        size: 0,
        pos: 0,
        offset: 0,
        cl_curs_x: 0,
        cl_curs_y: 0,
        shift: 0,
        shift_old: 0,
        height: 0,
        char_width: 0,
    };
    crate::boxlib::edit_box_set_text(&mut feld, name.as_bytes());
    let mut liste = eintraege(&pfad);
    let mut oben = 0usize;
    sys::maske(0x27);
    loop {
        let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
        let innen_h = h - skin - 5;
        let unten_h = if modus == Modus::Speichern { 2 * 40 + 10 } else { 50 };
        let liste_y = 4 + ZEILE;
        let sichtbar = ((innen_h - liste_y - unten_h) / ZEILE).max(1) as usize;
        let zeichnen = |liste: &[(String, bool)], oben: usize, pfad: &str, feld: &mut EditBox| {
            sys::neuzeichnen(1);
            sys::fenster(lage.0 << 16 | (w - 1) as u32, lage.1 << 16 | (h - 1) as u32, 0x33 << 24 | 0xF0F0F0, 0, Some(titel));
            knopf(KNOPF_ELTERN, 4, 2, 40, ZEILE - 4, 0xD8D8D8, "..", 0);
            text(52, 2 + (ZEILE - 4 - 16) / 2, pfad, 0x202020);
            for (i, (n, ordner)) in liste.iter().skip(oben).take(sichtbar).enumerate() {
                let y = liste_y + i as i32 * ZEILE;
                let farbe = if i % 2 == 0 { 0xFFFFFF } else { 0xF4F4F4 };
                sys::knopf(4 << 16 | (w - 10 - 50 - 1) as u32, (y as u32) << 16 | (ZEILE - 2) as u32, (KNOPF_EINTRAG + oben as u32 + i as u32) | 0x2000_0000, farbe);
                text(12, y + (ZEILE - 16) / 2, &format!("{}{}", if *ordner { "[+] " } else { "" }, n), if *ordner { 0x1A4A8A } else { 0x101010 });
            }
            knopf(KNOPF_HOCH, w - 10 - 46, liste_y, 44, 44, 0xD8D8D8, "^", 0);
            knopf(KNOPF_RUNTER, w - 10 - 46, liste_y + sichtbar as i32 * ZEILE - 44, 44, 44, 0xD8D8D8, "v", 0);
            let uy = innen_h - unten_h + 5;
            if modus == Modus::Speichern {
                feld.top = uy as u32;
                edit_box_draw(feld);
            }
            let by = innen_h - 45;
            knopf(KNOPF_AB, w - 10 - 2 * 110 - 10, by, 110, 40, 0xC8C8C8, kern::sprache::t("Cancel", "Abbrechen"), 0);
            if modus != Modus::Oeffnen {
                knopf(KNOPF_OK, w - 10 - 110, by, 110, 40, 0x3A7AC0, if modus == Modus::Speichern { kern::sprache::t("Save", "Speichern") } else { kern::sprache::t("Select", "Wählen") }, 0xFFFFFF);
            }
            sys::neuzeichnen(2);
        };
        match sys::warte_ereignis() {
            1 => zeichnen(&liste, oben, &pfad, &mut feld),
            2 => {
                let k = sys::taste();
                if modus == Modus::Speichern {
                    edit_box_key(&mut feld, k);
                }
            }
            6 => {
                if modus == Modus::Speichern {
                    edit_box_mouse(&mut feld);
                }
                let s = sys::maus(7) as i16 as i32;
                if s != 0 {
                    oben = (oben as i32 + s.signum() * 3).clamp(0, liste.len().saturating_sub(sichtbar) as i32) as usize;
                    zeichnen(&liste, oben, &pfad, &mut feld);
                }
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                match id {
                    1 | KNOPF_AB => return None,
                    KNOPF_OK => {
                        let n = String::from_utf8_lossy(&namenpuffer[..feld.size as usize]).into_owned();
                        return match modus {
                            Modus::Speichern if !n.is_empty() => Some(format!("{}/{}", pfad.trim_end_matches('/'), n)),
                            Modus::Ordner => Some(pfad),
                            _ => continue,
                        };
                    }
                    KNOPF_HOCH => oben = oben.saturating_sub(sichtbar),
                    KNOPF_RUNTER => oben = (oben + sichtbar).min(liste.len().saturating_sub(sichtbar)),
                    KNOPF_ELTERN => {
                        pfad = eltern(&pfad);
                        liste = eintraege(&pfad);
                        oben = 0;
                    }
                    id if id >= KNOPF_EINTRAG => {
                        if let Some((n, ordner)) = liste.get((id - KNOPF_EINTRAG) as usize).cloned() {
                            let ganz = format!("{}/{}", pfad.trim_end_matches('/'), n);
                            if ordner {
                                pfad = ganz;
                                liste = eintraege(&pfad);
                                oben = 0;
                            } else if modus == Modus::Oeffnen {
                                return Some(ganz);
                            } else if modus == Modus::Speichern {
                                crate::boxlib::edit_box_set_text(&mut feld, n.as_bytes());
                            }
                        }
                    }
                    _ => {}
                }
                zeichnen(&liste, oben, &pfad, &mut feld);
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------ Kontextmenü

/// Zeile 44 px, Knopf 42 px ≈ 7,3 mm.
const KM_ZEILE: i32 = 44;
const KM_BREITE: i32 = 180;

/// Kontextmenü an (x, y) (Bildschirm): blockiert, bis ein Eintrag gewählt
/// oder daneben getippt wurde. Läuft als eigener Faden mit eigenem Fenster
/// der obersten Ebene; nutzbar von jedem Programm (Desktop, box_lib).
pub fn kontextmenue(x: i32, y: i32, eintraege: &[&str]) -> Option<usize> {
    use std::sync::Arc;
    let namen: Vec<String> = eintraege.iter().map(|s| s.to_string()).collect();
    let ergebnis: Arc<Mutex<Option<Option<usize>>>> = Arc::new(Mutex::new(None));
    let e2 = ergebnis.clone();
    if sys::faden(48, Box::new(move || *e2.lock().unwrap_or_else(|e| e.into_inner()) = Some(km_lauf(x, y, &namen)))) < 0 {
        return None;
    }
    loop {
        if let Some(r) = ergebnis.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return r;
        }
        sys::warten(2);
    }
}

fn km_lauf(x: i32, y: i32, namen: &[String]) -> Option<usize> {
    let g = sys::bildschirmgroesse();
    let (w, h) = ((g >> 16) as i32 + 1, (g & 0xFFFF) as i32 + 1);
    let mh = namen.len() as i32 * KM_ZEILE + 4;
    let x = x.clamp(0, w - KM_BREITE);
    let y = y.clamp(0, h - mh);
    sys::maske(0x27);
    let mut gezeigt = false;
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster((x as u32) << 16 | (KM_BREITE - 1) as u32, (y as u32) << 16 | (mh - 1) as u32, 0x01_000000, 0, None);
                if !gezeigt {
                    sys::fensterebene(2, u32::MAX, 1);
                }
                sys::rechteck(KM_BREITE as u32, mh as u32, 0x6B6B6B);
                sys::rechteck(1 << 16 | (KM_BREITE - 2) as u32, 1 << 16 | (mh - 2) as u32, 0xF8F8F8);
                for (i, n) in namen.iter().enumerate() {
                    let yy = 2 + i as i32 * KM_ZEILE;
                    sys::knopf(2 << 16 | (KM_BREITE - 5) as u32, (yy as u32) << 16 | (KM_ZEILE - 2) as u32, 10 + i as u32, 0xF8F8F8);
                    text(14, yy + (KM_ZEILE - 16) / 2, n, 0x101010);
                }
                sys::neuzeichnen(2);
                gezeigt = true;
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                if let Some(i) = id.checked_sub(10).filter(|&i| (i as usize) < namen.len()) {
                    return Some(i as usize);
                }
            }
            6 => {
                // Antippen ausserhalb schliesst das Menü
                let m = sys::maus(0);
                let (mx, my) = ((m >> 16) as i32, (m & 0xFFFF) as i32);
                let gedrueckt = sys::maus(2) & 3 != 0;
                if gezeigt && gedrueckt && !(mx >= x && my >= y && mx < x + KM_BREITE && my < y + mh) {
                    return None;
                }
            }
            _ => {}
        }
    }
}


/// Grundfarben des Farbdialogs (6 Helligkeiten × 8 Farbtöne + Grautöne).
fn farben() -> Vec<u32> {
    let mut v = Vec::new();
    let toene = [0xFF0000, 0xFF8000, 0xFFFF00, 0x00C000, 0x00C0C0, 0x0060FF, 0x8000FF, 0xFF00A0];
    for stufe in [0.25f32, 0.5, 0.75, 1.0] {
        for &t in &toene {
            let k = |s: u32| ((((t >> s) & 255) as f32) * stufe) as u32;
            v.push(k(16) << 16 | k(8) << 8 | k(0));
        }
    }
    for g in 0..8u32 {
        let x = g * 255 / 7;
        v.push(x << 16 | x << 8 | x);
    }
    v
}

/// Farbdialog (blockiert). Liefert 0x00RRGGBB.
pub fn farb_dialog(lage: (u32, u32)) -> Option<u32> {
    let ergebnis: Arc<Mutex<Option<Option<u32>>>> = Arc::new(Mutex::new(None));
    let e2 = ergebnis.clone();
    if sys::faden(64, Box::new(move || *e2.lock().unwrap_or_else(|e| e.into_inner()) = Some(farb_lauf(lage)))) < 0 {
        return None;
    }
    loop {
        if let Some(r) = ergebnis.lock().unwrap_or_else(|e| e.into_inner()).take() {
            return r;
        }
        sys::warten(2);
    }
}

fn farb_lauf(lage: (u32, u32)) -> Option<u32> {
    let f = farben();
    let mut gewaehlt = None;
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster(lage.0 << 16 | 8 * 44 + 20, lage.1 << 16 | 5 * 44 + 110, 0x33 << 24 | 0xF0F0F0, 0, Some(kern::sprache::t("Select colour", "Farbe wählen")));
                for (i, &c) in f.iter().enumerate() {
                    let (x, y) = (5 + (i % 8) as i32 * 44, 5 + (i / 8) as i32 * 44);
                    sys::knopf((x as u32) << 16 | 41, (y as u32) << 16 | 41, 10 + i as u32, c);
                }
                let y = 5 + 5 * 44 + 8;
                if let Some(c) = gewaehlt {
                    sys::rechteck(5 << 16 | 120, (y as u32) << 16 | 40, c);
                }
                knopf(KNOPF_AB, 135, y, 110, 40, 0xC8C8C8, kern::sprache::t("Cancel", "Abbrechen"), 0);
                knopf(KNOPF_OK, 250, y, 110, 40, 0x3A7AC0, "OK", 0xFFFFFF);
                sys::neuzeichnen(2);
            }
            3 => match sys::knopf_id() >> 8 {
                1 | KNOPF_AB => return None,
                KNOPF_OK if gewaehlt.is_some() => return gewaehlt,
                id if id >= 10 => {
                    gewaehlt = f.get((id - 10) as usize).copied();
                    if let Some(c) = gewaehlt {
                        sys::rechteck(5 << 16 | 120, ((5 + 5 * 44 + 8) as u32) << 16 | 40, c);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(target_pointer_width = "32")]
mod c {
    use super::*;
    use core::ffi::c_char;

    #[repr(C, packed)]
    pub struct OpenDialog {
        mode: u32,
        procinfo: *mut c_char,
        com_area_name: *mut c_char,
        com_area: u32,
        opendir_path: *mut c_char,
        dir_default_path: *mut c_char,
        start_path: *mut c_char,
        draw_window: Option<extern "C" fn()>,
        status: u32,
        openfile_path: *mut c_char,
        filename_area: *mut c_char,
        filter_area: *mut u8,
        x_size: u16,
        x_start: u16,
        y_size: u16,
        y_start: u16,
    }

    #[repr(C, packed)]
    pub struct ColorDialog {
        typ: u32,
        procinfo: *mut c_char,
        com_area_name: *mut c_char,
        com_area: u32,
        start_path: *mut c_char,
        draw_window: Option<extern "C" fn()>,
        status: u32,
        x_size: u16,
        x_start: u16,
        y_size: u16,
        y_start: u16,
        color_type: u32,
        color: u32,
    }

    unsafe fn s(p: *const c_char) -> String {
        if p.is_null() {
            return String::new();
        }
        datei::pfad_text(unsafe { core::ffi::CStr::from_ptr(p) }.to_bytes(), 0)
    }

    unsafe fn kopieren(z: *mut c_char, n: usize, t: &str) {
        if z.is_null() {
            return;
        }
        let b = t.as_bytes();
        let k = b.len().min(n - 1);
        unsafe {
            core::ptr::copy_nonoverlapping(b.as_ptr(), z.cast(), k);
            *z.add(k) = 0;
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn OpenDialog_init(_d: *mut OpenDialog) {}

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn OpenDialog_start(d: *mut OpenDialog) {
        let Some(d) = (unsafe { d.as_mut() }) else { return };
        let modus = match { d.mode } {
            1 => Modus::Speichern,
            2 => Modus::Ordner,
            _ => Modus::Oeffnen,
        };
        let start = unsafe { s(d.opendir_path) };
        let start = if start.is_empty() { unsafe { s(d.dir_default_path) } } else { start };
        let name = unsafe { s(d.filename_area) };
        let r = datei_dialog(modus, &start, &name, (u32::from(d.x_size), u32::from(d.y_size)), (u32::from(d.x_start), u32::from(d.y_start)));
        match r {
            Some(p) => {
                unsafe {
                    kopieren(d.openfile_path, 4096, &p);
                    kopieren(d.filename_area, 256, p.rsplit('/').next().unwrap_or(""));
                    kopieren(d.opendir_path, 4096, &eltern(&p));
                }
                d.status = 1;
            }
            None => d.status = 0,
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ColorDialog_init(_d: *mut ColorDialog) {}

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ColorDialog_start(d: *mut ColorDialog) {
        let Some(d) = (unsafe { d.as_mut() }) else { return };
        match farb_dialog((u32::from(d.x_start), u32::from(d.y_start))) {
            Some(c) => {
                d.color = c;
                d.status = 1;
            }
            None => d.status = 0,
        }
    }
}
