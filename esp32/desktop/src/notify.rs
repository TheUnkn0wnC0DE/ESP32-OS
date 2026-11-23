//! Benachrichtigung „@notify“ wie in KolibriOS (`programs/system/notify3`):
//! kleines Fenster oben rechts mit Symbol und Text; schliesst sich nach
//! 4 s oder beim Antippen. KolibriOS-Programme rufen es so auf:
//! `_ksys_exec("/sys/@notify", "'Text' -I")`.
//!
//! Aufruf (readme von notify3): `Text`, `"Text" -Schalter` oder
//! `'Text' -Schalter`; im Text `\"`, `\'` und `\n` (neue Zeile).
//! Schalter: d = schliesst sich nicht selbst, c = nicht durch Antippen,
//! t = erste Zeile ist der Titel, p = Fortschrittsbalken (fehlt: der
//! Steuerkanal dafür ist nicht nachgebaut, Abweichung Z3), Symbole
//! A E W O N I F C M D S.

use crate::shell::bildschirm;
use kern::sys;
use std::sync::atomic::{AtomicU32, Ordering};

/// Belegte Plätze (je ein Bit): mehrere Meldungen stehen untereinander.
static PLAETZE: AtomicU32 = AtomicU32::new(0);
const BREITE: i32 = 300;
/// Höhe eines Platzes (Abstand zur nächsten Meldung).
const PLATZ_H: i32 = 84;
/// Zeichen je Zeile (Text ab x = 56, 8 Bildpunkte je Zeichen).
const ZEICHEN: usize = 29;
/// Selbst schliessen nach (Hundertstel).
const ZU_NACH: u32 = 400;

#[derive(Debug, Default, PartialEq)]
pub struct Notiz {
    pub zeilen: Vec<String>,
    /// erste Zeile als Titel (fett)
    pub titel: bool,
    /// schliesst sich nicht selbst (d)
    pub bleibt: bool,
    /// schliesst sich nicht beim Antippen (c)
    pub klick_fest: bool,
    pub symbol: Option<char>,
}

/// Aufruf zerlegen (Format von notify3).
pub fn zerlegen(a: &str) -> Notiz {
    let a = a.trim();
    let mut n = Notiz::default();
    let (text, rest) = match a.chars().next() {
        Some(q @ ('"' | '\'')) => {
            let mut t = String::new();
            let mut ende = a.len();
            let mut it = a[1..].char_indices();
            while let Some((i, c)) = it.next() {
                match c {
                    '\\' => match it.next() {
                        Some((_, 'n')) => t.push('\n'),
                        Some((_, c)) => t.push(c),
                        None => {}
                    },
                    c if c == q => {
                        ende = 1 + i + 1;
                        break;
                    }
                    c => t.push(c),
                }
            }
            (t, &a[ende..])
        }
        _ => (a.replace("\\n", "\n"), ""),
    };
    for w in rest.split_whitespace() {
        let Some(s) = w.strip_prefix('-') else { continue };
        for c in s.chars() {
            match c {
                'd' => n.bleibt = true,
                'c' => n.klick_fest = true,
                't' => n.titel = true,
                'A' | 'E' | 'W' | 'O' | 'N' | 'I' | 'F' | 'C' | 'M' | 'D' | 'S' => n.symbol = Some(c),
                _ => {}
            }
        }
    }
    // „-dc“ schliesst im Original nur der Steuerkanal, den es hier nicht
    // gibt (Abweichung Z3) – Antippen schliesst dann trotzdem
    if n.bleibt && n.klick_fest {
        n.klick_fest = false;
    }
    // Zeilen umbrechen (an Wortgrenzen, sonst hart)
    for absatz in text.split('\n') {
        let mut z = String::new();
        for wort in absatz.split(' ') {
            if !z.is_empty() && z.chars().count() + 1 + wort.chars().count() > ZEICHEN {
                n.zeilen.push(std::mem::take(&mut z));
            }
            if !z.is_empty() {
                z.push(' ');
            }
            z.push_str(wort);
            while z.chars().count() > ZEICHEN {
                let rest: String = z.chars().skip(ZEICHEN).collect();
                n.zeilen.push(z.chars().take(ZEICHEN).collect());
                z = rest;
            }
        }
        n.zeilen.push(z);
    }
    n
}

/// Farbe und Zeichen des Symbols.
fn symbol(s: char) -> (u32, &'static str) {
    match s {
        'I' => (0x2A6F_D6, "i"),
        'E' => (0xD63A_2A, "!"),
        'W' => (0xE89A_1C, "!"),
        'O' => (0x2E9E_44, "OK"),
        'N' => (0x3A8A_C0, "N"),
        'F' => (0xC8A0_30, "F"),
        'M' => (0x6A5A_C8, "@"),
        'D' => (0x2E9E_44, "v"),
        'S' => (0x9A4A_B0, "S"),
        'C' => (0x6A70_80, "C"),
        _ => (0x6A70_80, "A"),
    }
}

fn text(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

fn hoehe(n: &Notiz) -> i32 {
    (24 + n.zeilen.len() as i32 * 18).clamp(56, 4 * PLATZ_H)
}

fn zeichnen(n: &Notiz, x: i32, y: i32) {
    let h = hoehe(n);
    sys::neuzeichnen(1);
    // Stil 1: nur der Bereich, gezeichnet wird alles selbst
    sys::fenster((x as u32) << 16 | (BREITE - 1) as u32, (y as u32) << 16 | (h - 1) as u32, 0x01 << 24, 0, None);
    // Titel mit dem Text (für Tests und den Task-Manager); „@“ hält die
    // Meldung aus der Taskleiste
    sys::titel(&format!("@notify: {}", n.zeilen.join(" ")));
    sys::rechteck(BREITE as u32, h as u32, 0x40_4450);
    sys::rechteck(1 << 16 | (BREITE - 2) as u32, 1 << 16 | (h - 2) as u32, 0xF8_F8F8);
    let tx = if let Some(s) = n.symbol {
        let (farbe, zeichen) = symbol(s);
        sys::rechteck(12 << 16 | 32, 12 << 16 | 32, farbe);
        let w = zeichen.chars().count() as i32 * 8;
        text(12 + (32 - w) / 2, 20, zeichen, 0xFF_FFFF);
        56
    } else {
        14
    };
    for (i, z) in n.zeilen.iter().enumerate() {
        let y = 12 + i as i32 * 18;
        text(tx, y, z, 0x20_2020);
        if i == 0 && n.titel {
            text(tx + 1, y, z, 0x20_2020);
        }
    }
    sys::neuzeichnen(2);
}

/// Programm „@notify“ (Parameter wie notify3).
pub fn programm() {
    let n = zerlegen(&kern::argumente());
    if n.zeilen.iter().all(|z| z.trim().is_empty()) {
        sys::ende();
    }
    // Plätze zurücksetzen, wenn keine Meldung mehr offen ist (eine von aussen
    // beendete Meldung gibt ihr Bit sonst nie frei)
    if !sys::fenster_liste().iter().any(|f| f.titel.starts_with("@notify")) {
        PLAETZE.store(0, Ordering::Release);
    }
    let platz = loop {
        let alt = PLAETZE.load(Ordering::Acquire);
        let frei = (!alt).trailing_zeros().min(31);
        if PLAETZE.compare_exchange(alt, alt | 1 << frei, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            break frei;
        }
    };
    let (sw, sh) = bildschirm();
    let x = sw - BREITE - 8;
    let y = (8 + platz as i32 * PLATZ_H).min(sh - crate::shell::LEISTE_H - hoehe(&n)).max(0);
    let start = sys::zeitzaehler();
    sys::maske(0x27);
    sys::fensterebene(2, u32::MAX, 1);
    loop {
        match sys::warte_ereignis_max(10) {
            1 => zeichnen(&n, x, y),
            6 if !n.klick_fest => {
                // Antippen im Fenster schliesst
                let p = sys::maus(1);
                let (mx, my) = ((p >> 16) as i16 as i32, p as i16 as i32);
                if sys::maus(2) & 1 != 0 && (0..BREITE).contains(&mx) && (0..hoehe(&n)).contains(&my) {
                    break;
                }
            }
            _ => {}
        }
        if !n.bleibt && sys::zeitzaehler().wrapping_sub(start) >= ZU_NACH {
            break;
        }
    }
    PLAETZE.fetch_and(!(1 << platz), Ordering::AcqRel);
    sys::ende();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aufruf_wie_notify3() {
        let n = zerlegen("'Copied to clipboard!' -I");
        assert_eq!(n.zeilen, ["Copied to clipboard!"]);
        assert_eq!(n.symbol, Some('I'));
        assert!(!n.bleibt && !n.klick_fest && !n.titel);
        let n = zerlegen(r#""Titel\nZeile mit \"Zitat\" und \'x\'" -tcE"#);
        assert_eq!(n.zeilen, ["Titel", "Zeile mit \"Zitat\" und 'x'"]);
        assert!(n.titel && !n.bleibt && n.klick_fest);
        assert_eq!(n.symbol, Some('E'));
        // ohne Anführungszeichen: ganzer Text, keine Schalter
        assert_eq!(zerlegen("Hallo Welt").zeilen, ["Hallo Welt"]);
        // langer Text wird umbrochen, überlanges Wort hart
        let n = zerlegen("'eins zwei drei vier fünf sechs sieben acht neun zehn' -O");
        assert!(n.zeilen.len() == 2 && n.zeilen.iter().all(|z| z.chars().count() <= ZEICHEN), "{:?}", n.zeilen);
        let n = zerlegen(&format!("'{}'", "x".repeat(70)));
        assert_eq!(n.zeilen.iter().map(|z| z.chars().count()).collect::<Vec<_>>(), [29, 29, 12]);
        // -dc: ohne Steuerkanal bliebe die Meldung für immer → Antippen geht
        let n = zerlegen("'x' -dc");
        assert!(n.bleibt && !n.klick_fest);
    }
}
