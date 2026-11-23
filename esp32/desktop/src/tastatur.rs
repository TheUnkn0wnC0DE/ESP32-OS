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

/// Hintergrund der Tastatur.
const GRUND: u32 = 0x2C3444;
/// Gedrückte Taste: Grösse in Prozent.
const GEDRUECKT_PROZENT: i32 = 87;

/// Eine gezeichnete Taste (Fensterkoordinaten).
struct Taste {
    x: i32,
    y: i32,
    /// Breite der Zelle (Knopf 2 px schmaler)
    b: i32,
    id: u32,
    a: Aktion,
    text: String,
    farbe: u32,
}

impl Taste {
    /// Knopffläche (Funktion 8 zeichnet Breite/Höhe + 1).
    fn flaeche(&self) -> (i32, i32) {
        (self.b - 2, TASTE_H - 3)
    }

    fn enthaelt(&self, x: i32, y: i32) -> bool {
        let (w, h) = self.flaeche();
        (self.x..self.x + w).contains(&x) && (self.y..self.y + h).contains(&y)
    }
}

/// Farbe ~20 % dunkler.
fn dunkler(c: u32) -> u32 {
    let k = |s: u32| ((c >> s & 0xFF) * 4 / 5) << s;
    k(16) | k(8) | k(0)
}

/// Eine Taste zeichnen: normal als Knopf, gedrückt kleiner (87 %) und
/// dunkler auf dem Grund. Markiert nur die Fläche dieser Taste.
fn taste_zeichnen(t: &Taste, gedrueckt: bool) {
    let (w, h) = t.flaeche();
    if gedrueckt {
        sys::rechteck((t.x as u32) << 16 | w as u32, (t.y as u32) << 16 | h as u32, GRUND);
        let (sw, sh) = (w * GEDRUECKT_PROZENT / 100, h * GEDRUECKT_PROZENT / 100);
        let (sx, sy) = (t.x + (w - sw) / 2, t.y + (h - sh) / 2);
        sys::rechteck((sx as u32) << 16 | sw as u32, (sy as u32) << 16 | sh as u32, dunkler(t.farbe));
    } else {
        // gleicher Knopf neu (der Kern ersetzt ihn in einem Schritt)
        sys::knopf((t.x as u32) << 16 | (t.b - 3) as u32, (t.y as u32) << 16 | (TASTE_H - 4) as u32, t.id, t.farbe);
    }
    let n = t.text.chars().count() as i32 * 8;
    sys::text(
        ((t.x + (t.b - 2 - n) / 2) as u32) << 16 | (t.y + (TASTE_H - 4 - 16) / 2) as u32,
        0x30FF_FFFF,
        t.text.as_bytes(),
        t.text.chars().count() as u32,
        0,
    );
}

fn zeichnen(e: Ebene, w: i32) -> Vec<Taste> {
    let mut lage = Vec::new();
    sys::neuzeichnen(1);
    sys::rechteck(w as u32, HOEHE as u32, GRUND);
    for (zi, zeile) in tasten(e).iter().enumerate() {
        let breite: i32 = zeile.iter().map(|t| t.2).sum::<i32>() * EINHEIT;
        let mut x = (w - breite) / 2;
        let y = 2 + zi as i32 * TASTE_H;
        for (i, (a, text, b)) in zeile.iter().enumerate() {
            let bw = b * EINHEIT;
            let farbe = match a {
                Aktion::Zeichen(_) | Aktion::Leer => 0x5A6478,
                Aktion::Umschalt if e == Ebene::Gross => 0x3A7AC0,
                _ => 0x434C5E,
            };
            let t = Taste { x, y, b: bw, id: 10 + (zi * 20 + i) as u32, a: *a, text: text.clone(), farbe };
            sys::knopf((x as u32) << 16 | (bw - 3) as u32, (y as u32) << 16 | (TASTE_H - 4) as u32, t.id, farbe);
            let n = text.chars().count() as i32 * 8;
            sys::text(
                ((x + (bw - 2 - n) / 2) as u32) << 16 | (y + (TASTE_H - 4 - 16) / 2) as u32,
                0x30FF_FFFF,
                text.as_bytes(),
                text.chars().count() as u32,
                0,
            );
            lage.push(t);
            x += bw;
        }
    }
    sys::neuzeichnen(2);
    lage
}

/// Taste unter dem Finger, solange er aufliegt (direkter Touch: Lage aus
/// den Gesten, Fenster bei (0, `fenster_y`)), sonst unter der gedrückten
/// Maustaste (Touchpad-Modus, Maus).
fn taste_unter_finger(lage: &[Taste], fenster_y: i32) -> Option<usize> {
    let (x, y) = match sys::eingabe::beruehrung().filter(|_| !sys::eingabe::touchpad()) {
        Some((x, y)) => (x, y - fenster_y),
        None if sys::maus(2) & 1 != 0 => {
            let m = sys::maus(1);
            ((m >> 16) as i16 as i32, (m & 0xFFFF) as i16 as i32)
        }
        None => return None,
    };
    lage.iter().position(|t| t.enthaelt(x, y))
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
    let mut lage: Vec<Taste> = Vec::new();
    // Gedrückt gezeichnete Taste (Index in `lage`)
    let mut gedrueckt: Option<usize> = None;
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
            gedrueckt = None;
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
        // Rückmeldung: Taste unter dem Finger kleiner und dunkler, beim
        // Loslassen (oder Wegrutschen) wieder normal – nur diese Taste
        let jetzt = if erstellt && sichtbar { taste_unter_finger(&lage, lage_y) } else { None };
        if jetzt != gedrueckt {
            if let Some(t) = gedrueckt.and_then(|i| lage.get(i)) {
                taste_zeichnen(t, false);
            }
            if let Some(t) = jetzt.and_then(|i| lage.get(i)) {
                taste_zeichnen(t, true);
            }
            gedrueckt = jetzt;
        }
        match e {
            1 if erstellt => {
                sys::fenster((w - 1) as u32, (y as u32) << 16 | (HOEHE - 1) as u32, 0x01_000000, 0, None);
                lage = zeichnen(ebene, w);
                gedrueckt = None;
            }
            3 => {
                let id = (sys::knopf_id() >> 8) as i32;
                let Some(a) = lage.iter().find(|l| l.id as i32 == id).map(|l| l.a) else { continue };
                match a {
                    Aktion::Zeichen(c) => {
                        senden(c);
                        if ebene == Ebene::Gross {
                            ebene = Ebene::Normal;
                            lage = zeichnen(ebene, w);
                            gedrueckt = None;
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
                        gedrueckt = None;
                    }
                    Aktion::Sonder => {
                        ebene = if ebene == Ebene::Sonder { Ebene::Normal } else { Ebene::Sonder };
                        lage = zeichnen(ebene, w);
                        gedrueckt = None;
                    }
                    Aktion::Weg => sys::tastatur_umschalten(),
                    Aktion::Layout => {
                        let neu = if layout() == Layout::Ch { Layout::Us } else { Layout::Ch };
                        if !crate::konfig::layout_setzen(neu) {
                            sys::debug_text("Tastatur: Layout nicht gespeichert\n");
                        }
                        lage = zeichnen(ebene, w);
                        gedrueckt = None;
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

/// Mitte einer Sondertaste („shift“, „123“, „layout“, „enter“, „rueck“)
/// relativ zur Tastatur.
pub fn sondertaste(name: &str, w: i32) -> (i32, i32) {
    let e = Ebene::Normal;
    let (zi, i) = match name {
        "shift" => (3, 0),
        "layout" => (4, 1),
        "enter" => (1, usize::MAX),
        "rueck" => (0, usize::MAX),
        _ => (4, 0),
    };
    let zeile = &tasten(e)[zi];
    // usize::MAX: letzte Taste der Reihe
    let i = i.min(zeile.len() - 1);
    let breite: i32 = zeile.iter().map(|t| t.2).sum::<i32>() * EINHEIT;
    let x = (w - breite) / 2 + zeile[..i].iter().map(|t| t.2).sum::<i32>() * EINHEIT + zeile[i].2 * EINHEIT / 2;
    (x, 2 + zi as i32 * TASTE_H + TASTE_H / 2)
}
