//! Bildschirmtastatur (`@tastatur`) – Systemdienst (Auftrag 4).
//!
//! * Erscheint automatisch, sobald ein Textfeld (box_lib edit_box) den
//!   Fokus hat, und zusätzlich über den Tastaturknopf der Taskleiste.
//! * Deutsch-Schweizer Belegung (QWERTZ, ü ö ä, Umschalt è é à) oder
//!   US-Belegung (QWERTY), umschaltbar über die Taste „CH“/„US“ und
//!   gespeichert (Auftrag 12.1); Sonderzeichen-Ebene (@ # € [ ] { } \ | …).
//! * Fenster der obersten Ebene: nimmt dem Programm nie den Fokus; Tasten
//!   gehen an das aktive Fenster (Funktion 2). Zeichen ausserhalb cp866
//!   als Unicode-Taste (al = 3, ESP32-OS-Erweiterung, versteht box_lib).
//! * Liegt das Textfeld unten, sitzt die Tastatur oben.

use crate::shell::LEISTE_H;
use core::sync::atomic::{AtomicU8, Ordering};
use kern::sprache::t;
use kern::sys;

/// Tastaturlayout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Deutsch-Schweiz (QWERTZ).
    Ch,
    /// US (QWERTY).
    Us,
}

impl Layout {
    pub fn kuerzel(self) -> &'static str {
        match self {
            Layout::Ch => "ch",
            Layout::Us => "us",
        }
    }
    pub fn aus_kuerzel(k: &str) -> Option<Layout> {
        match k.trim().to_ascii_lowercase().as_str() {
            "ch" => Some(Layout::Ch),
            "us" => Some(Layout::Us),
            _ => None,
        }
    }
}

static LAYOUT: AtomicU8 = AtomicU8::new(0);

pub fn layout() -> Layout {
    if LAYOUT.load(Ordering::Relaxed) == 1 { Layout::Us } else { Layout::Ch }
}

/// Layout setzen (ohne Speichern; dafür [`crate::konfig::layout_setzen`]).
pub fn layout_setzen(l: Layout) {
    LAYOUT.store(u8::from(l == Layout::Us), Ordering::Relaxed);
}

const EINHEIT: i32 = 49;
/// Tastenhöhe: sichtbar 42 px ≈ 7,3 mm.
const TASTE_H: i32 = 46;
const ZEILEN: i32 = 5;
pub const HOEHE: i32 = ZEILEN * TASTE_H + 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Aktion {
    Zeichen(char),
    Rueck,
    Eingabe,
    Umschalt,
    Sonder,
    Links,
    Rechts,
    Leer,
    Weg,
    /// Layout CH ↔ US umschalten.
    Layout,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ebene {
    Normal,
    Gross,
    Sonder,
}

/// Zeile: (Beschriftungen ohne Sonderknöpfe) je Ebene.
const NORMAL: [&str; 4] = ["1234567890'^", "qwertzuiopü¨", "asdfghjklöä$", "<yxcvbnm,.-"];
const GROSS: [&str; 4] = ["+\"*ç%&/()=?`", "QWERTZUIOPè!", "ASDFGHJKLéà£", ">YXCVBNM;:_"];
const NORMAL_US: [&str; 4] = ["1234567890-=", "qwertyuiop[]", "asdfghjkl;'\\", "`zxcvbnm,./"];
const GROSS_US: [&str; 4] = ["!@#$%^&*()_+", "QWERTYUIOP{}", "ASDFGHJKL:\"|", "~ZXCVBNM<>?"];
const SONDER: [&str; 4] = ["¦@#°§¬|¢´~{}", "[]€\\<>=+*/«»", "&%!?;:\"'`^-_", "…·µ²³½¼¿¡ß"];

/// Tasten einer Ebene: (Aktion, Beschriftung, Breite in Einheiten).
fn tasten(e: Ebene) -> Vec<Vec<(Aktion, String, i32)>> {
    let us = layout() == Layout::Us;
    let z = match e {
        Ebene::Normal if us => NORMAL_US,
        Ebene::Gross if us => GROSS_US,
        Ebene::Normal => NORMAL,
        Ebene::Gross => GROSS,
        Ebene::Sonder => SONDER,
    };
    let zeichen = |s: &str| s.chars().map(|c| (Aktion::Zeichen(c), c.to_string(), 1)).collect::<Vec<_>>();
    let mut r0 = zeichen(z[0]);
    r0.push((Aktion::Rueck, t("Back", "Rück").into(), 1));
    let mut r1 = zeichen(z[1]);
    r1.push((Aktion::Eingabe, "Enter".into(), 1));
    let r2 = zeichen(z[2]);
    let mut r3 = vec![(Aktion::Umschalt, "Shift".into(), 1)];
    r3.extend(zeichen(z[3]));
    r3.push((Aktion::Umschalt, "Shift".into(), 1));
    let sonder = if e == Ebene::Sonder { "abc" } else { "123" };
    let r4 = vec![
        (Aktion::Sonder, sonder.into(), 2),
        (Aktion::Layout, if us { "US" } else { "CH" }.into(), 1),
        (Aktion::Links, "<-".into(), 1),
        (Aktion::Leer, String::new(), 5),
        (Aktion::Rechts, "->".into(), 1),
        (Aktion::Weg, t("Hide", "Weg").into(), 2),
    ];
    vec![r0, r1, r2, r3, r4]
}

/// Zeichen an das aktive Fenster schicken (Funktion-2-Format).
pub fn senden(c: char) {
    let k = kern::schrift::uni_zu_cp866(c);
    let code = if c.is_ascii() || (k != b'?' && c != '?') { u32::from(k) << 8 } else { (c as u32 & 0xFFFF) << 16 | 3 };
    sys::eingabe::taste(code);
}

fn zeichnen(e: Ebene, w: i32) -> Vec<(i32, i32, i32, i32, Aktion)> {
    let mut lage = Vec::new();
    sys::neuzeichnen(1);
    sys::rechteck(w as u32, HOEHE as u32, 0x2C3444);
    for (zi, zeile) in tasten(e).iter().enumerate() {
        let breite: i32 = zeile.iter().map(|t| t.2).sum::<i32>() * EINHEIT;
        let mut x = (w - breite) / 2;
        let y = 2 + zi as i32 * TASTE_H;
        for (i, (a, t, b)) in zeile.iter().enumerate() {
            let bw = b * EINHEIT;
            let id = 10 + (zi * 20 + i) as u32;
            let farbe = match a {
                Aktion::Zeichen(_) | Aktion::Leer => 0x5A6478,
                Aktion::Umschalt if e == Ebene::Gross => 0x3A7AC0,
                _ => 0x434C5E,
            };
            sys::knopf((x as u32) << 16 | (bw - 3) as u32, (y as u32) << 16 | (TASTE_H - 4) as u32, id, farbe);
            let n = t.chars().count() as i32 * 8;
            sys::text(((x + (bw - 2 - n) / 2) as u32) << 16 | (y + (TASTE_H - 4 - 16) / 2) as u32, 0x30FF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
            lage.push((x, y, bw, id as i32, *a));
            x += bw;
        }
    }
    sys::neuzeichnen(2);
    lage
}

/// Der Dienst: läuft ständig, Fenster nur bei Bedarf sichtbar.
pub fn dienst() {
    let g = sys::bildschirmgroesse();
    let (w, h) = ((g >> 16) as i32 + 1, (g & 0xFFFF) as i32 + 1);
    let y_unten = h - LEISTE_H - HOEHE;
    let eigen = kern::eigene_tid().unwrap_or(0);
    let mut ebene = Ebene::Normal;
    let mut sichtbar = false;
    let mut erstellt = false;
    let mut oben = false;
    let mut lage_y = y_unten;
    let mut lage = Vec::new();
    // Redraw erst nach dem ersten Fenster (sonst bleibt Ereignis 1 stehen und
    // die Schleife dreht leer)
    sys::maske(0x04);
    loop {
        let e = sys::warte_ereignis_max(5);
        let (soll, feld) = sys::tastatur_soll();
        // Textfeld frei lassen: unten, sonst oben, sonst direkt unter dem Feld
        // (dann notfalls über der Taskleiste).
        let y = match feld {
            Some(f) if f.y1 > y_unten && f.y0 >= HOEHE => 0,
            Some(f) if f.y1 > y_unten => (f.y1 + 2).min(h - HOEHE),
            _ => y_unten,
        };
        let neu_oben = y != y_unten;
        if soll && !erstellt {
            sys::fenster((w - 1) as u32, (y as u32) << 16 | (HOEHE - 1) as u32, 0x01_000000, 0, None);
            sys::fensterebene(2, u32::MAX, 1);
            lage = zeichnen(ebene, w);
            erstellt = true;
            sys::maske(0x05);
            sichtbar = true;
            oben = neu_oben;
        } else if erstellt && soll != sichtbar {
            let slot = sys::system(21, eigen);
            sys::minimieren(if soll { 2 } else { 0 }, slot);
            sichtbar = soll;
        }
        if erstellt && sichtbar && (neu_oben != oben || lage_y != y) {
            sys::fenster_bewegen(0, y, -1, -1);
            oben = neu_oben;
            lage_y = y;
        }
        match e {
            1 if erstellt => {
                sys::fenster((w - 1) as u32, (y as u32) << 16 | (HOEHE - 1) as u32, 0x01_000000, 0, None);
                lage = zeichnen(ebene, w);
            }
            3 => {
                let id = (sys::knopf_id() >> 8) as i32;
                let Some(&(_, _, _, _, a)) = lage.iter().find(|l| l.3 == id) else { continue };
                match a {
                    Aktion::Zeichen(c) => {
                        senden(c);
                        if ebene == Ebene::Gross {
                            ebene = Ebene::Normal;
                            lage = zeichnen(ebene, w);
                        }
                    }
                    Aktion::Leer => senden(' '),
                    Aktion::Rueck => sys::eingabe::taste(8 << 8),
                    Aktion::Eingabe => sys::eingabe::taste(13 << 8),
                    Aktion::Links => sys::eingabe::taste(0xB0 << 8),
                    Aktion::Rechts => sys::eingabe::taste(0xB3 << 8),
                    Aktion::Umschalt => {
                        ebene = if ebene == Ebene::Gross { Ebene::Normal } else { Ebene::Gross };
                        lage = zeichnen(ebene, w);
                    }
                    Aktion::Sonder => {
                        ebene = if ebene == Ebene::Sonder { Ebene::Normal } else { Ebene::Sonder };
                        lage = zeichnen(ebene, w);
                    }
                    Aktion::Weg => sys::tastatur_umschalten(),
                    Aktion::Layout => {
                        let neu = if layout() == Layout::Ch { Layout::Us } else { Layout::Ch };
                        if !crate::konfig::layout_setzen(neu) {
                            sys::debug_text("Tastatur: Layout nicht gespeichert\n");
                        }
                        lage = zeichnen(ebene, w);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Lage der Taste für Zeichen `c` (Bildschirm, Mitte) – für Tests und
/// Autopilot: (Ebene-Wechsel nötig?, x, y relativ zur Tastatur).
pub fn tastenmitte(c: char, w: i32) -> Option<(&'static str, i32, i32)> {
    for (name, e) in [("normal", Ebene::Normal), ("gross", Ebene::Gross), ("sonder", Ebene::Sonder)] {
        for (zi, zeile) in tasten(e).iter().enumerate() {
            let breite: i32 = zeile.iter().map(|t| t.2).sum::<i32>() * EINHEIT;
            let mut x = (w - breite) / 2;
            for (a, _, b) in zeile {
                if *a == Aktion::Zeichen(c) || (c == ' ' && *a == Aktion::Leer) {
                    return Some((name, x + b * EINHEIT / 2, 2 + zi as i32 * TASTE_H + TASTE_H / 2));
                }
                x += b * EINHEIT;
            }
        }
    }
    None
}

/// Mitte einer Sondertaste („shift“, „123“, „layout“) relativ zur Tastatur.
pub fn sondertaste(name: &str, w: i32) -> (i32, i32) {
    let e = Ebene::Normal;
    let (zi, i) = match name {
        "shift" => (3, 0),
        "layout" => (4, 1),
        _ => (4, 0),
    };
    let zeile = &tasten(e)[zi];
    let breite: i32 = zeile.iter().map(|t| t.2).sum::<i32>() * EINHEIT;
    let x = (w - breite) / 2 + zeile[..i].iter().map(|t| t.2).sum::<i32>() * EINHEIT + zeile[i].2 * EINHEIT / 2;
    (x, 2 + zi as i32 * TASTE_H + TASTE_H / 2)
}
