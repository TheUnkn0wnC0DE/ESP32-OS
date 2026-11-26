//! Einstellungen (P3) – nativ im KolibriOS-Stil. Startseite mit Kacheln,
//! je Bereich eine Seite mit „<“ zurück: Anzeige (Helligkeit), Ton
//! (Lautstärke + Probeton), Akku, Datum/Uhrzeit (manuell; NTP folgt mit
//! WLAN), Touch (Touchpad-Modus, Tastaturlayout), Hintergrund (Verläufe,
//! Bild aus Datei), Speicherplatz (SD, intern), Sprache (Auftrag 12.1),
//! WLAN (W3: Netze suchen, verbinden, gemerkte Netze vergessen), System
//! (U1: Version, nach Updates suchen, installieren).
//! Alles wird sofort in `/sys/settings/esp32os.ini` gespeichert (`konfig`).

use crate::konfig::{self, Hintergrund, VERLAEUFE};
use crate::shell::{LEISTE_H, bild_auf, symbol};
use core::sync::atomic::Ordering::Relaxed;
use kern::sprache::{Sprache, t};
use kern::{sys, zeit};

pub const ZURUECK: u32 = 2;
/// Kacheln: 200 + Index (eigener Bereich, kollidiert mit keinem anderen Knopf).
pub const KACHEL: u32 = 200;
pub const MINUS: u32 = 20;
pub const PLUS: u32 = 21;
/// Datum/Uhrzeit: Plus = 30 + Feld, Minus = 40 + Feld, Übernehmen 50.
pub const FELD_PLUS: u32 = 30;
pub const FELD_MINUS: u32 = 40;
pub const UEBERNEHMEN: u32 = 50;
/// Datum/Uhrzeit: Internetzeit ein/aus.
pub const NTP: u32 = 51;
pub const TOUCHPAD: u32 = 60;
pub const LAYOUT: u32 = 61;
/// Hintergrund: Verlauf 70 + i, Bild 79.
pub const VERLAUF: u32 = 70;
pub const BILD: u32 = 79;
/// Sprache: 80 Englisch, 81 Deutsch.
pub const SPRACHE: u32 = 80;
/// WLAN-Liste: Knöpfe ab 110 (siehe `wlan`).
pub const WLAN: u32 = 110;
/// Sichtbare Netze auf der WLAN-Seite.
pub const WLAN_ZEILEN: usize = 4;
/// System (U1): nach Updates suchen, installieren.
pub const UPDATE_SUCHEN: u32 = 160;
pub const UPDATE_INSTALLIEREN: u32 = 161;

const HG: u32 = 0xF0F0F0;
const BLAU: u32 = 0x3A7AC0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seite {
    Start,
    Anzeige,
    Ton,
    Akku,
    Zeit,
    Touch,
    Hintergrund,
    Speicher,
    Sprache,
    Energie,
    Wlan,
    System,
    /// Q2: Ort der Wetter-App (startet die Ortssuche im Programm „wetter“)
    Wetter,
}

/// Kacheln der Startseite: (Seite, englisch, deutsch, Symbol in icons32).
pub const KACHELN: [(Seite, &str, &str, usize); 12] = [
    (Seite::Anzeige, "Display", "Anzeige", 7),
    (Seite::Ton, "Sound", "Ton", 64),
    (Seite::Akku, "Battery", "Akku", 121),
    (Seite::Zeit, "Date & time", "Datum & Zeit", 52),
    (Seite::Touch, "Touch", "Touch", 54),
    (Seite::Hintergrund, "Wallpaper", "Hintergrund", 46),
    (Seite::Speicher, "Storage", "Speicher", 50),
    (Seite::Sprache, "Language", "Sprache", 69),
    (Seite::Energie, "Power", "Energie", 119),
    (Seite::Wlan, "Wi-Fi", "WLAN", 33),
    (Seite::System, "System", "System", 113),
    (Seite::Wetter, "Weather place", "Wetter-Ort", 128),
];

/// Kachelgrösse und -lage (Arbeitsbereich 630×259, 6 × 2).
pub const KW: i32 = 98;
pub const KH: i32 = 117;
pub fn kachel_lage(i: usize) -> (i32, i32) {
    (6 + (i as i32 % 6) * (KW + 6), 8 + (i as i32 / 6) * (KH + 8))
}

/// Energie: „Bildschirm aus“ 90 + i, „Ruhezustand“ 100 + i.
pub const AUS: u32 = 90;
pub const RUHE: u32 = 100;

fn dauer(s: u32) -> String {
    match s {
        0 => t("Never", "Nie").to_string(),
        s if s < 60 => format!("{s} s"),
        s => format!("{} min", s / 60),
    }
}

/// Datum/Uhrzeit-Felder: Tag, Monat, Jahr, Stunde, Minute.
pub const FELDER: usize = 5;
pub fn feld_x(i: usize) -> i32 {
    10 + i as i32 * 120
}

struct Zustand {
    /// Kachelsymbole 48 px, einmal skaliert (schnelles Neuzeichnen: während
    /// des Zeichnens sind die Knöpfe kurz weg, ein Tipp ginge verloren).
    symbole: Vec<kolibri::dekoder::Rgba>,
    seite: Seite,
    /// Eingestellte Ortszeit (Tag, Monat, Jahr, Stunde, Minute).
    zeit: [i64; FELDER],
    meldung: String,
    wlan: crate::wlan::Liste,
    /// System (U1): gefundene neuere Version, Stand der Suche/Installation.
    update: Option<kern::aktualisierung::Freigabe>,
    update_text: String,
}

fn text(x: i32, y: i32, s: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, farbe: u32, beschriftung: &str, tf: u32) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, farbe);
    let n = beschriftung.chars().count() as i32 * 8;
    text(x + (w - n) / 2, y + (h - 16) / 2, beschriftung, tf);
}

/// Ortszeit als Felder; Uhr nicht gestellt (vor 2020) → 01.01.2026 12:00.
pub fn ortszeit_felder() -> [i64; FELDER] {
    let l = zeit::lokal(zeit::unix());
    let (y, m, d) = zeit::datum_aus_tagen(l.div_euclid(86_400));
    if y < 2020 {
        return [1, 1, 2026, 12, 0];
    }
    let s = l.rem_euclid(86_400);
    [i64::from(d), i64::from(m), y, s / 3600, s / 60 % 60]
}

fn monatstage(y: i64, m: i64) -> i64 {
    let erster = zeit::tage_aus_datum(y, m as u32, 1);
    let naechster = if m == 12 { zeit::tage_aus_datum(y + 1, 1, 1) } else { zeit::tage_aus_datum(y, m as u32 + 1, 1) };
    naechster - erster
}

/// Feld `i` um `d` ändern (mit Umlauf, Tag passend zum Monat).
pub fn feld_aendern(z: &mut [i64; FELDER], i: usize, d: i64) {
    let (min, max) = match i {
        0 => (1, monatstage(z[2], z[1])),
        1 => (1, 12),
        2 => (2020, 2099),
        3 => (0, 23),
        _ => (0, 59),
    };
    let n = max - min + 1;
    z[i] = (z[i] - min + d).rem_euclid(n) + min;
    z[0] = z[0].min(monatstage(z[2], z[1]));
}

/// Ortszeit-Felder → UTC (Zeitzone/Sommerzeit wie `zeit::lokal`).
pub fn felder_zu_utc(z: &[i64; FELDER]) -> i64 {
    let lokal = zeit::tage_aus_datum(z[2], z[1] as u32, z[0] as u32) * 86_400 + z[3] * 3600 + z[4] * 60;
    // Versatz an der gesuchten Stelle (zweimal: Sommerzeit-Grenze)
    let mut utc = lokal - (zeit::lokal(lokal) - lokal);
    utc = lokal - (zeit::lokal(utc) - utc);
    utc
}

fn mb(b: u64) -> String {
    match b {
        b if b >= 10 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 10 << 20 => format!("{} MB", b >> 20),
        b => format!("{} KB", b >> 10),
    }
}

fn regler(wert: u8, min: u8, max: u8, y: i32) {
    knopf(MINUS, 20, y, 64, 48, 0xD8D8D8, "-", 0);
    let n = i32::from(max - min) + 1;
    let bw = 400 / n;
    for i in 0..n {
        let an = i <= i32::from(wert - min);
        sys::rechteck(((100 + i * bw) as u32) << 16 | (bw - 4) as u32, ((y + 8) as u32) << 16 | 32, if an { BLAU } else { 0xC8C8C8 });
    }
    knopf(PLUS, 100 + n * bw + 16, y, 64, 48, 0xD8D8D8, "+", 0);
    text(100, y + 60, &format!("{wert} / {max}"), 0x303030);
}

fn zeichnen(z: &Zustand) {
    let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
    let (w, h) = (630, 360 - LEISTE_H - skin - 5);
    sys::neuzeichnen(1);
    let titel = t("Settings", "Einstellungen");
    sys::fenster(639, (360 - LEISTE_H - 1) as u32, 0x33 << 24 | HG, 0, Some(titel));
    sys::titel(titel);
    sys::rechteck(w as u32, h as u32, HG);
    if z.seite != Seite::Start {
        knopf(ZURUECK, 8, 6, 56, 42, 0xD8D8D8, "<", 0);
        if let Some((_, en, de, _)) = KACHELN.iter().find(|k| k.0 == z.seite) {
            text(76, 19, t(en, de), 0x1A3A6A);
        }
    }
    match z.seite {
        Seite::Start => {
            for (i, (_, en, de, sym)) in KACHELN.iter().enumerate() {
                let (x, y) = kachel_lage(i);
                sys::knopf((x as u32) << 16 | (KW - 1) as u32, (y as u32) << 16 | (KH - 1) as u32, KACHEL + i as u32, 0xFFFFFF);
                let _ = sym;
                bild_auf(&z.symbole[i], x + (KW - 48) / 2, y + 16, |_, _| 0xFFFFFF);
                let name = t(en, de);
                text(x + (KW - 8 * name.chars().count() as i32) / 2, y + 78, name, 0x202020);
            }
        }
        Seite::Anzeige => {
            text(20, 70, t("Brightness", "Helligkeit"), 0x202020);
            regler(konfig::HELLIGKEIT.load(Relaxed), 1, 10, 96);
        }
        Seite::Ton => {
            text(20, 70, t("Volume (a test tone plays on change)", "Lautstärke (Probeton beim Ändern)"), 0x202020);
            regler(konfig::LAUTSTAERKE.load(Relaxed), 0, 10, 96);
        }
        Seite::Akku => match konfig::AKKU.load(Relaxed) {
            255 => {
                text(20, 70, t("Battery level: not measurable", "Akkustand: nicht messbar"), 0x202020);
                text(20, 100, t("The board charges the battery but has no", "Die Platine lädt den Akku, misst die"), 0x505050);
                text(20, 120, t("voltage measurement. A voltage divider at", "Spannung aber nicht. Dafür braucht es"), 0x505050);
                text(20, 140, t("GPIO 21 is required (see README).", "einen Spannungsteiler an GPIO 21 (README)."), 0x505050);
            }
            p => {
                text(20, 70, &format!("{}: {p} %", t("Battery level", "Akkustand")), 0x202020);
                sys::rechteck(20 << 16 | 400, 100 << 16 | 40, 0xC8C8C8);
                sys::rechteck(20 << 16 | (4 * u32::from(p)).max(1), 100 << 16 | 40, if p < 20 { 0xC03030 } else { 0x3A9A4A });
            }
        },
        Seite::Zeit => {
            let namen = [t("Day", "Tag"), t("Month", "Monat"), t("Year", "Jahr"), t("Hour", "Stunde"), t("Minute", "Minute")];
            for (i, n) in namen.iter().enumerate() {
                let x = feld_x(i);
                text(x + (100 - 8 * n.chars().count() as i32) / 2, 58, n, 0x404040);
                knopf(FELD_PLUS + i as u32, x, 78, 100, 42, 0xD8D8D8, "+", 0);
                let v = if i == 2 { format!("{}", z.zeit[i]) } else { format!("{:02}", z.zeit[i]) };
                text(x + (100 - 8 * v.chars().count() as i32) / 2, 128, &v, 0x101010);
                knopf(FELD_MINUS + i as u32, x, 150, 100, 42, 0xD8D8D8, "-", 0);
            }
            knopf(UEBERNEHMEN, 10, 204, 220, 44, BLAU, t("Set clock", "Uhr stellen"), 0xFFFFFF);
            let jetzt = ortszeit_felder();
            let s = format!("{}: {:02}.{:02}.{} {:02}:{:02}", t("Now", "Jetzt"), jetzt[0], jetzt[1], jetzt[2], jetzt[3], jetzt[4]);
            text(246, 19, &s, 0x404040);
            // Internetzeit (W3b): ein/aus, letzter Abgleich
            let an = kern::zeit::ntp_an();
            let status = match (an, kern::zeit::ntp_gestellt_um()) {
                (false, _) => t("Internet time: off", "Internetzeit: aus").to_string(),
                (true, Some(u)) => {
                    let l = zeit::lokal(u).rem_euclid(86_400);
                    format!("{} {:02}:{:02}", t("Internet time: set", "Internetzeit: gestellt"), l / 3600, l / 60 % 60)
                }
                (true, None) if kern::wlan::zustand().verbunden => t("Internet time: on, waiting", "Internetzeit: an, wartet").to_string(),
                (true, None) => t("Internet time: on (needs Wi-Fi)", "Internetzeit: an (braucht WLAN)").to_string(),
            };
            knopf(NTP, 246, 204, 374, 44, if an { BLAU } else { 0xD8D8D8 }, &status, if an { 0xFFFFFF } else { 0 });
        }
        Seite::Touch => {
            let tp = sys::eingabe::touchpad();
            let a = if tp { t("Touchpad mode: on", "Touchpad-Modus: an") } else { t("Touchpad mode: off", "Touchpad-Modus: aus") };
            knopf(TOUCHPAD, 20, 66, 420, 48, if tp { BLAU } else { 0xD8D8D8 }, a, if tp { 0xFFFFFF } else { 0 });
            let l = match crate::tastatur::layout() {
                crate::tastatur::Layout::Ch => t("Keyboard layout: Swiss German (CH)", "Tastaturlayout: Schweizerdeutsch (CH)"),
                crate::tastatur::Layout::Us => t("Keyboard layout: US", "Tastaturlayout: US"),
            };
            knopf(LAYOUT, 20, 126, 420, 48, 0xD8D8D8, l, 0);
            text(20, 190, t("Touchpad mode: finger moves a pointer, tap = click.", "Touchpad-Modus: Finger bewegt einen Zeiger, Tippen = Klick."), 0x505050);
        }
        Seite::Hintergrund => {
            let akt = konfig::hintergrund();
            for (i, (en, de, oben, unten)) in VERLAEUFE.iter().enumerate() {
                let x = 8 + i as i32 * 154;
                let gewaehlt = akt == Hintergrund::Verlauf(i);
                sys::knopf((x as u32) << 16 | 145, 60 << 16 | 99, (VERLAUF + i as u32) | 0x4000_0000, 0);
                if gewaehlt {
                    sys::rechteck(((x - 3) as u32) << 16 | 152, 57 << 16 | 106, BLAU);
                }
                for y in 0..70 {
                    let tt = y as u32 * 255 / 70;
                    let m = |s: u32| ((oben >> s & 255) * (255 - tt) + (unten >> s & 255) * tt) / 255;
                    sys::rechteck((x as u32) << 16 | 146, ((60 + y) as u32) << 16 | 1, m(16) << 16 | m(8) << 8 | m(0));
                }
                sys::rechteck((x as u32) << 16 | 146, 130 << 16 | 30, 0xFFFFFF);
                let n = t(en, de);
                text(x + (146 - 8 * n.chars().count() as i32) / 2, 137, n, 0x202020);
            }
            knopf(BILD, 8, 176, 300, 48, 0xD8D8D8, t("Image from file...", "Bild aus Datei..."), 0);
            if let Hintergrund::Bild(p) = &akt {
                let n: String = p.chars().rev().take(36).collect::<Vec<_>>().into_iter().rev().collect();
                text(320, 192, &n, BLAU);
            }
        }
        Seite::Speicher => {
            for (i, (pfad, en, de)) in [("/sd0/1", "SD card", "SD-Karte"), ("/rd/1", "Internal", "Intern")].iter().enumerate() {
                let y = 64 + i as i32 * 80;
                text(20, y, t(en, de), 0x202020);
                match kern::datei::speicherplatz(pfad) {
                    Some((ges, frei)) if ges > 0 => {
                        let belegt = ges.saturating_sub(frei);
                        let bw = (500 * belegt / ges) as u32;
                        sys::rechteck(20 << 16 | 500, ((y + 22) as u32) << 16 | 22, 0xC8E0C8);
                        sys::rechteck(20 << 16 | bw.max(1), ((y + 22) as u32) << 16 | 22, BLAU);
                        let s = format!("{} {} {}", mb(frei), t("free of", "frei von"), mb(ges));
                        text(20, y + 50, &s, 0x404040);
                    }
                    _ => text(20, y + 26, t("not available", "nicht verfügbar"), 0x808080),
                }
            }
        }
        Seite::Energie => {
            text(20, 60, t("Screen off after (no touch):", "Bildschirm aus nach (ohne Berührung):"), 0x202020);
            let aus = konfig::BILDSCHIRM_AUS.load(Relaxed);
            for (i, s) in konfig::AUS_WAHL.iter().enumerate() {
                let an = aus == *s;
                knopf(AUS + i as u32, 10 + i as i32 * 102, 82, 96, 44, if an { BLAU } else { 0xD8D8D8 }, &dauer(*s), if an { 0xFFFFFF } else { 0 });
            }
            text(20, 142, t("Sleep after screen off (panel off):", "Ruhezustand nach Bildschirm aus (Panel aus):"), 0x202020);
            let ruhe = konfig::RUHE_NACH.load(Relaxed);
            for (i, s) in konfig::RUHE_WAHL.iter().enumerate() {
                let an = ruhe == *s;
                knopf(RUHE + i as u32, 10 + i as i32 * 102, 164, 96, 44, if an { BLAU } else { 0xD8D8D8 }, &dauer(*s), if an { 0xFFFFFF } else { 0 });
            }
            text(20, 220, t("Touch wakes the device.", "Berühren weckt das Gerät."), 0x606060);
        }
        Seite::Wlan => crate::wlan::zeichnen(&z.wlan, 6, 58, WLAN_ZEILEN, WLAN),
        // Q2: keine eigene Seite – die Kachel startet die Ortssuche der
        // Wetter-App (die Seite bleibt die Startseite)
        Seite::Wetter => {}
        Seite::System => {
            text(20, 64, &format!("{} {}", crate::shell::NAME, crate::shell::VERSION), 0x202020);
            text(20, 88, t("Updates come from the GitHub release (HTTPS, checksum checked).", "Updates kommen vom GitHub-Release (HTTPS, Prüfsumme geprüft)."), 0x606060);
            let geht = kern::aktualisierung::vorhanden();
            knopf(UPDATE_SUCHEN, 20, 116, 300, 48, if geht { 0xD8D8D8 } else { 0xE8E8E8 }, t("Check for updates", "Nach Updates suchen"), if geht { 0 } else { 0x909090 });
            if let Some(f) = &z.update {
                let b = format!("{} {}", t("Install", "Installieren:"), f.version);
                knopf(UPDATE_INSTALLIEREN, 330, 116, 290, 48, BLAU, &b, 0xFFFFFF);
            }
            if !geht {
                text(20, 176, t("Updates need Wi-Fi.", "Updates brauchen WLAN."), 0x808080);
            }
            text(20, 200, &z.update_text, 0x1A3A6A);
        }
        Seite::Sprache => {
            let s = kern::sprache::aktuell();
            for (i, (name, sp)) in [("English", Sprache::Englisch), ("Deutsch", Sprache::Deutsch)].iter().enumerate() {
                let an = s == *sp;
                knopf(SPRACHE + i as u32, 20 + i as i32 * 220, 70, 200, 48, if an { BLAU } else { 0xD8D8D8 }, name, if an { 0xFFFFFF } else { 0 });
            }
        }
    }
    let (meldung, farbe) = if z.seite == Seite::Wlan && z.meldung.is_empty() { (&z.wlan.meldung, crate::wlan::meldung_farbe(&z.wlan)) } else { (&z.meldung, 0xB03030) };
    if !meldung.is_empty() {
        text(8, h - 20, meldung, farbe);
    }
    sys::neuzeichnen(2);
}

/// Zwischenstand der WLAN-Liste zeichnen (Suchen, Verbinden).
fn wlan_neu(z: &mut Zustand, l: &crate::wlan::Liste) {
    let alt = core::mem::replace(
        &mut z.wlan,
        crate::wlan::Liste { netze: l.netze.clone(), erste: l.erste, meldung: l.meldung.clone(), meldung_ok: l.meldung_ok, gesucht: l.gesucht },
    );
    zeichnen(z);
    z.wlan = alt;
}

fn gespeichert(ok: bool) -> String {
    if ok { String::new() } else { t("Could not save the setting", "Einstellung nicht gespeichert").to_string() }
}

/// Programm „einstellungen“ (Parameter: Seite, z. B. „language“).
pub fn programm() {
    let start = match kern::argumente().as_str() {
        "display" => Seite::Anzeige,
        "sound" => Seite::Ton,
        "battery" => Seite::Akku,
        "time" => Seite::Zeit,
        "touch" => Seite::Touch,
        "wallpaper" => Seite::Hintergrund,
        "storage" => Seite::Speicher,
        "language" => Seite::Sprache,
        "power" => Seite::Energie,
        "wifi" => Seite::Wlan,
        "system" => Seite::System,
        _ => Seite::Start,
    };
    let symbole = KACHELN.iter().map(|k| symbol(k.3, 48)).collect();
    let mut z = Zustand {
        symbole,
        seite: start,
        zeit: ortszeit_felder(),
        meldung: String::new(),
        wlan: Default::default(),
        update: None,
        update_text: String::new(),
    };
    sys::maske(0x07);
    let mut wlan_zustand = kern::wlan::zustand();
    loop {
        // WLAN: erstes Öffnen sucht Netze
        if z.seite == Seite::Wlan && !z.wlan.gesucht && kern::wlan::vorhanden() {
            zeichnen(&z);
            let mut wl = core::mem::take(&mut z.wlan);
            crate::wlan::suchen(&mut wl, &mut |l| wlan_neu(&mut z, l));
            z.wlan = wl;
            zeichnen(&z);
        }
        // Datum/Zeit-Seite: „Jetzt“ jede Sekunde aktuell halten; WLAN-Seite:
        // Zustand (Verbindung, Signal) alle 0,5 s prüfen
        let e = match z.seite {
            Seite::Zeit => sys::warte_ereignis_max(100),
            Seite::Wlan => sys::warte_ereignis_max(50),
            _ => sys::warte_ereignis(),
        };
        match e {
            0 if z.seite == Seite::Wlan => {
                let neu = kern::wlan::zustand();
                if neu != wlan_zustand {
                    wlan_zustand = neu;
                    zeichnen(&z);
                }
            }
            0 => zeichnen(&z),
            1 => zeichnen(&z),
            3 => {
                let id = sys::knopf_id() >> 8;
                z.meldung.clear();
                match id {
                    1 => sys::ende(),
                    ZURUECK => z.seite = Seite::Start,
                    id if (KACHEL..KACHEL + KACHELN.len() as u32).contains(&id) => {
                        z.seite = KACHELN[(id - KACHEL) as usize].0;
                        if z.seite == Seite::Wetter {
                            // Ortssuche der Wetter-App (Touch, Bildschirmtastatur)
                            z.seite = Seite::Start;
                            let r = kern::datei::starten("/sys/wetter", "ort");
                            if r <= 0 {
                                // Start gescheitert (z. B. Speicher): sichtbar melden (Q1 Fund 1)
                                z.meldung = crate::dateimanager::startfehler(t("Weather", "Wetter"), r);
                            }
                        }
                        if z.seite == Seite::Zeit {
                            z.zeit = ortszeit_felder();
                        }
                    }
                    MINUS | PLUS => {
                        let d: i16 = if id == PLUS { 1 } else { -1 };
                        match z.seite {
                            Seite::Anzeige => {
                                let v = (i16::from(konfig::HELLIGKEIT.load(Relaxed)) + d).clamp(1, 10) as u8;
                                z.meldung = gespeichert(konfig::helligkeit_setzen(v));
                            }
                            Seite::Ton => {
                                let v = (i16::from(konfig::LAUTSTAERKE.load(Relaxed)) + d).clamp(0, 10) as u8;
                                z.meldung = gespeichert(konfig::lautstaerke_setzen(v));
                            }
                            _ => {}
                        }
                    }
                    id if (FELD_PLUS..FELD_PLUS + FELDER as u32).contains(&id) => feld_aendern(&mut z.zeit, (id - FELD_PLUS) as usize, 1),
                    id if (FELD_MINUS..FELD_MINUS + FELDER as u32).contains(&id) => feld_aendern(&mut z.zeit, (id - FELD_MINUS) as usize, -1),
                    UEBERNEHMEN => {
                        // Die Taskleiste zeigt die neue Zeit beim nächsten Takt (0,5 s)
                        let ok = konfig::uhr_setzen(felder_zu_utc(&z.zeit));
                        z.meldung = gespeichert(ok);
                    }
                    NTP => z.meldung = gespeichert(konfig::ntp_setzen(!kern::zeit::ntp_an())),
                    TOUCHPAD => z.meldung = gespeichert(konfig::touchpad_setzen(!sys::eingabe::touchpad())),
                    LAYOUT => {
                        let neu = match crate::tastatur::layout() {
                            crate::tastatur::Layout::Ch => crate::tastatur::Layout::Us,
                            crate::tastatur::Layout::Us => crate::tastatur::Layout::Ch,
                        };
                        z.meldung = gespeichert(konfig::layout_setzen(neu));
                    }
                    id if (VERLAUF..VERLAUF + VERLAEUFE.len() as u32).contains(&id) => {
                        z.meldung = gespeichert(konfig::hintergrund_setzen(Hintergrund::Verlauf((id - VERLAUF) as usize)));
                    }
                    BILD => {
                        let start = if kern::datei::speicherplatz("/sd0/1").is_some() { "/sd0/1" } else { "/rd/1" };
                        if let Some(p) = kolibri::dialog::datei_dialog(kolibri::dialog::Modus::Oeffnen, start, "", (420, 300), (100, 10)) {
                            let (sw, sh) = crate::shell::bildschirm();
                            z.meldung = match crate::bildbetrachter::laden(&p) {
                                Ok(v) if konfig::hintergrund_vorbereiten(&v[0], sw as u32, sh as u32) => gespeichert(konfig::hintergrund_setzen(Hintergrund::Bild(p))),
                                Ok(_) => gespeichert(false),
                                Err(f) => f,
                            };
                        }
                    }
                    id if (AUS..AUS + konfig::AUS_WAHL.len() as u32).contains(&id) => {
                        z.meldung = gespeichert(konfig::bildschirm_aus_setzen(konfig::AUS_WAHL[(id - AUS) as usize]));
                    }
                    id if (RUHE..RUHE + konfig::RUHE_WAHL.len() as u32).contains(&id) => {
                        z.meldung = gespeichert(konfig::ruhe_nach_setzen(konfig::RUHE_WAHL[(id - RUHE) as usize]));
                    }
                    id if z.seite == Seite::Wlan && id >= WLAN => {
                        let mut wl = core::mem::take(&mut z.wlan);
                        crate::wlan::knopf_behandeln(&mut wl, id, WLAN, WLAN_ZEILEN, &mut |l| wlan_neu(&mut z, l));
                        z.wlan = wl;
                    }
                    UPDATE_SUCHEN if kern::aktualisierung::vorhanden() => {
                        z.update = None;
                        z.update_text = t("Checking for updates...", "Suche nach Updates...").to_string();
                        zeichnen(&z);
                        z.update_text = match kern::aktualisierung::pruefen(crate::shell::VERSION) {
                            Ok(Some(f)) => {
                                let s = format!("{} {}", t("New version available:", "Neue Version verfügbar:"), f.version);
                                z.update = Some(f);
                                s
                            }
                            Ok(None) => t("ESP32-OS is up to date.", "ESP32-OS ist aktuell.").to_string(),
                            Err(e) => e,
                        };
                        sys::debug_text(&format!("Update: {}\n", z.update_text));
                    }
                    UPDATE_INSTALLIEREN => {
                        if let Some(f) = z.update.clone() {
                            let laden = t("Downloading", "Lade");
                            let r = kern::aktualisierung::installieren(&f, &mut |p| {
                                z.update_text = format!("{laden} {} ... {p} %", f.version);
                                zeichnen(&z);
                            });
                            let ok = r.is_ok();
                            z.update_text = match r {
                                Ok(()) => t("Installed - restarting...", "Installiert - Neustart...").to_string(),
                                Err(e) => e,
                            };
                            sys::debug_text(&format!("Update: {}\n", z.update_text));
                            zeichnen(&z);
                            if ok {
                                crate::shell::starten("@neustart");
                            }
                        }
                    }
                    SPRACHE => z.meldung = gespeichert(konfig::sprache_setzen(Sprache::Englisch)),
                    id if id == SPRACHE + 1 => z.meldung = gespeichert(konfig::sprache_setzen(Sprache::Deutsch)),
                    _ => {}
                }
                if !z.meldung.is_empty() {
                    sys::debug_text(&format!("Einstellungen: {}\n", z.meldung));
                }
                zeichnen(&z);
            }
            _ => {}
        }
    }
}
