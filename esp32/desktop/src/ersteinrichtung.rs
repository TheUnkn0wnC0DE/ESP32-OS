//! Ersteinrichtung (P4): startet von `@start`, solange in den Einstellungen
//! `setup=1` fehlt. Schritte per Touch: Sprache → Tastaturlayout → WLAN
//! (W3, überspringbar) → Datum & Uhrzeit → fertig.

use crate::einstellungen::{FELDER, feld_aendern, felder_zu_utc, feld_x, ortszeit_felder};
use crate::konfig;
use crate::shell::{LEISTE_H, bild_auf, symbol};
use crate::tastatur::{Layout, layout};
use kern::sprache::{Sprache, aktuell, t};
use kern::sys;

pub const WEITER: u32 = 2;
pub const ZURUECK: u32 = 3;
/// Sprache: 10 Englisch, 11 Deutsch; Layout: 20 CH, 21 US.
pub const SPRACHE: u32 = 10;
pub const LAYOUT: u32 = 20;
/// Uhr: Plus 30 + Feld, Minus 40 + Feld.
pub const PLUS: u32 = 30;
pub const MINUS: u32 = 40;
/// WLAN-Liste: Knöpfe ab 100 (siehe `wlan`).
pub const WLAN: u32 = 100;
pub const WLAN_ZEILEN: usize = 3;

const HG: u32 = 0xF0F0F0;
const BLAU: u32 = 0x3A7AC0;

/// Schritte des Assistenten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schritt {
    Sprache,
    Tastatur,
    Wlan,
    Uhr,
    Fertig,
}

pub const SCHRITTE: [Schritt; 5] = [Schritt::Sprache, Schritt::Tastatur, Schritt::Wlan, Schritt::Uhr, Schritt::Fertig];

fn text(x: i32, y: i32, s: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, an: bool, beschriftung: &str) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, if an { BLAU } else { 0xD8D8D8 });
    let n = beschriftung.chars().count() as i32 * 8;
    text(x + (w - n) / 2, y + (h - 16) / 2, beschriftung, if an { 0xFFFFFF } else { 0 });
}

/// Arbeitsbereich 630 × (312 − Kopf − 5); „Weiter“ unten rechts.
pub fn weiter_lage() -> (i32, i32, i32, i32) {
    let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
    let h = 360 - LEISTE_H - skin - 5;
    (630 - 8 - 160, h - 8 - 44, 160, 44)
}

fn zeichnen(i: usize, zeit: &[i64; FELDER], logo: &kolibri::dekoder::Rgba, wl: &crate::wlan::Liste) {
    sys::neuzeichnen(1);
    let titel = t("Welcome to ESP32-OS", "Willkommen bei ESP32-OS");
    sys::fenster(639, (360 - LEISTE_H - 1) as u32, 0x33 << 24 | HG, 0, Some(titel));
    sys::titel(titel);
    let (wx, wy, ww, wh) = weiter_lage();
    sys::rechteck(630, (wy + wh + 8) as u32, HG);
    text(8, 10, &format!("{} {}/{}", t("Step", "Schritt"), i + 1, SCHRITTE.len()), 0x707070);
    match SCHRITTE[i] {
        Schritt::Sprache => {
            bild_auf(logo, 8, 36, |_, _| HG);
            text(64, 50, "Choose your language / Sprache wählen", 0x1A3A6A);
            for (k, (name, s)) in [("English", Sprache::Englisch), ("Deutsch", Sprache::Deutsch)].iter().enumerate() {
                knopf(SPRACHE + k as u32, 20 + k as i32 * 220, 100, 200, 56, aktuell() == *s, name);
            }
        }
        Schritt::Tastatur => {
            text(20, 50, t("Choose the on-screen keyboard layout", "Layout der Bildschirmtastatur wählen"), 0x1A3A6A);
            let l = layout();
            knopf(LAYOUT, 20, 100, 260, 56, l == Layout::Ch, t("Swiss German (QWERTZ)", "Schweizerdeutsch (QWERTZ)"));
            knopf(LAYOUT + 1, 300, 100, 200, 56, l == Layout::Us, "US (QWERTY)");
            text(20, 172, t("You can change this later in Settings > Touch.", "Später änderbar unter Einstellungen > Touch."), 0x606060);
        }
        Schritt::Wlan => {
            text(116, 10, t("Connect Wi-Fi (optional)", "WLAN verbinden (optional)"), 0x1A3A6A);
            crate::wlan::zeichnen(wl, 4, 54, WLAN_ZEILEN, WLAN);
            let m: String = wl.meldung.chars().take(35).collect();
            text(176, wy + 14, &m, crate::wlan::meldung_farbe(wl));
        }
        Schritt::Uhr => {
            text(20, 34, t("Set date and time (Internet time follows with Wi-Fi)", "Datum und Uhrzeit (Internetzeit folgt mit WLAN)"), 0x1A3A6A);
            let namen = [t("Day", "Tag"), t("Month", "Monat"), t("Year", "Jahr"), t("Hour", "Stunde"), t("Minute", "Minute")];
            for (k, n) in namen.iter().enumerate() {
                let x = feld_x(k);
                text(x + (100 - 8 * n.chars().count() as i32) / 2, 58, n, 0x404040);
                knopf(PLUS + k as u32, x, 78, 100, 42, false, "+");
                let v = if k == 2 { format!("{}", zeit[k]) } else { format!("{:02}", zeit[k]) };
                text(x + (100 - 8 * v.chars().count() as i32) / 2, 128, &v, 0x101010);
                knopf(MINUS + k as u32, x, 150, 100, 42, false, "-");
            }
        }
        Schritt::Fertig => {
            bild_auf(logo, 8, 36, |_, _| HG);
            text(64, 50, t("ESP32-OS is ready.", "ESP32-OS ist bereit."), 0x1A3A6A);
            text(64, 80, t("Tap and hold = right click, swipe = scroll,", "Halten = Rechtsklick, Wischen = Blättern,"), 0x404040);
            text(64, 100, t("two fingers = zoom. Settings are on the desktop.", "zwei Finger = Zoomen. Einstellungen auf dem Desktop."), 0x404040);
        }
    }
    if i > 0 {
        knopf(ZURUECK, 8, wy, 160, wh, false, t("< Back", "< Zurück"));
    }
    let w = match SCHRITTE[i] {
        Schritt::Fertig => t("Finish", "Fertig"),
        Schritt::Uhr => t("Set clock >", "Uhr stellen >"),
        Schritt::Wlan if !kern::wlan::zustand().verbunden => t("Skip >", "Überspringen >"),
        _ => t("Next >", "Weiter >"),
    };
    knopf(WEITER, wx, wy, ww, wh, true, w);
    sys::neuzeichnen(2);
}

/// Programm „ersteinrichtung“.
pub fn programm() {
    let logo = symbol(49, 48);
    let mut i = 0usize;
    let mut zeit = ortszeit_felder();
    let mut wl = crate::wlan::Liste::default();
    let mut wlan_zustand = kern::wlan::zustand();
    sys::maske(0x07);
    loop {
        // WLAN-Schritt: beim ersten Mal Netze suchen, danach den Zustand
        // alle 0,5 s prüfen
        let im_wlan = SCHRITTE[i] == Schritt::Wlan;
        if im_wlan && !wl.gesucht && kern::wlan::vorhanden() {
            zeichnen(i, &zeit, &logo, &wl);
            crate::wlan::suchen(&mut wl, &mut |l| zeichnen(i, &zeit, &logo, l));
            zeichnen(i, &zeit, &logo, &wl);
        }
        let e = if im_wlan { sys::warte_ereignis_max(50) } else { sys::warte_ereignis() };
        match e {
            0 if im_wlan => {
                let neu = kern::wlan::zustand();
                if neu != wlan_zustand {
                    wlan_zustand = neu;
                    zeichnen(i, &zeit, &logo, &wl);
                }
            }
            1 => zeichnen(i, &zeit, &logo, &wl),
            3 => {
                let id = sys::knopf_id() >> 8;
                match id {
                    // Schliessen ohne Abschluss: beim nächsten Start wieder
                    1 => sys::ende(),
                    ZURUECK => i = i.saturating_sub(1),
                    WEITER => match SCHRITTE[i] {
                        Schritt::Uhr => {
                            let _ = konfig::uhr_setzen(felder_zu_utc(&zeit));
                            i += 1;
                        }
                        Schritt::Fertig => {
                            if !konfig::einrichtung_abschliessen() {
                                sys::debug_text("Ersteinrichtung: nicht gespeichert\n");
                            }
                            sys::ende();
                        }
                        _ => i += 1,
                    },
                    SPRACHE => {
                        let _ = konfig::sprache_setzen(Sprache::Englisch);
                    }
                    id if id == SPRACHE + 1 => {
                        let _ = konfig::sprache_setzen(Sprache::Deutsch);
                    }
                    LAYOUT => {
                        let _ = konfig::layout_setzen(Layout::Ch);
                    }
                    id if id == LAYOUT + 1 => {
                        let _ = konfig::layout_setzen(Layout::Us);
                    }
                    id if (PLUS..PLUS + FELDER as u32).contains(&id) => feld_aendern(&mut zeit, (id - PLUS) as usize, 1),
                    id if (MINUS..MINUS + FELDER as u32).contains(&id) => feld_aendern(&mut zeit, (id - MINUS) as usize, -1),
                    id if SCHRITTE[i] == Schritt::Wlan && id >= WLAN => {
                        let schritt = i;
                        crate::wlan::knopf_behandeln(&mut wl, id, WLAN, WLAN_ZEILEN, &mut |l| zeichnen(schritt, &zeit, &logo, l));
                    }
                    _ => {}
                }
                zeichnen(i, &zeit, &logo, &wl);
            }
            _ => {}
        }
    }
}
