//! A1: Notizen – kleiner Texteditor (Auftrag Abschnitt 5).
//!
//! Links die Notizen des Ordners (`*.txt`), rechts der Text. „Neu“ beginnt
//! eine leere Notiz, „Speichern“ schreibt sie (Name aus der ersten Zeile,
//! vorhandene Dateien werden nicht überschrieben, ausser es ist dieselbe
//! Notiz). Antippen des Textes öffnet die Bildschirmtastatur (Textfeld-Fokus
//! samt Lage der Zeile mit der Schreibmarke, die sie nicht verdeckt). Tasten: Zeichen (auch
//! Umlaute), Rücktaste, Eingabe, Pfeil links/rechts.
//!
//! Ordner: Argument des Programms (Ordner oder Datei, die geöffnet wird –
//! der Dateimanager öffnet so .txt/.ini/.log/.md/.cfg), sonst `/sd0/1/ESP32-OS/Notizen` (eigener Ordner von ESP32-OS auf
//! der SD-Karte), ohne SD-Karte `/daten/Notizen`.

use crate::dateimanager;
use kern::{datei, sys};

// Fenster (Arbeitsbereich-Koordinaten)
pub const FX: i32 = 20;
pub const FY: i32 = 4;
pub const FW: i32 = 600;
pub const FH: i32 = 306;
/// Liste links: x, Breite, erste Zeile, Zeilenhöhe.
pub const LISTE_X: i32 = 6;
pub const LISTE_W: i32 = 150;
pub const LISTE_Y: i32 = 50;
/// Zeilen ≥ 7 mm (1 px = 0,173 mm), wie Menü- und Kontextmenüzeilen
pub const ZEILE_H: i32 = 42;
/// Ende der Liste (Arbeitsbereich y)
const LISTE_ENDE: i32 = 250;
/// Textbereich rechts.
pub const TEXT_X: i32 = 164;
pub const TEXT_Y: i32 = 50;
pub const TEXT_W: i32 = 424;
pub const TEXT_H: i32 = 172;
/// Knöpfe oben (x, Breite; y 4, Höhe 40).
pub const NEU: (i32, i32) = (6, 70);
pub const SPEICHERN: (i32, i32) = (82, 100);
pub const KNOPF_Y: i32 = 4;
pub const KNOPF_H: i32 = 40;
/// Grösste Datei, die sich bearbeiten lässt; grössere nur lesen (sonst
/// würde „Speichern“ sie gekürzt zurückschreiben); der Text liegt als
/// `Vec<char>` (4 Byte je Zeichen) im Heap des Programms, Tippen endet
/// ebenfalls bei 64 K Zeichen.
const MAX_DATEI: usize = 64 * 1024;

const K_NEU: u32 = 2;
const K_SPEICHERN: u32 = 3;
const K_TEXT: u32 = 4;
const K_EINTRAG: u32 = 100;

const ZEICHEN_B: i32 = 8;
const ZEILE_T: i32 = 18;
const SPALTEN: usize = ((TEXT_W - 12) / ZEICHEN_B) as usize;
const ZEILEN_T: usize = ((TEXT_H - 8) / ZEILE_T) as usize;
const LISTE_ZEILEN: usize = ((LISTE_ENDE - LISTE_Y) / ZEILE_H) as usize;

struct Zustand {
    ordner: String,
    notizen: Vec<String>,
    /// Name der geöffneten Notiz (None = neu, noch nicht gespeichert)
    name: Option<String>,
    text: Vec<char>,
    marke: usize,
    geaendert: bool,
    status: Option<(String, u32)>,
    /// Tastatur offen: die Zeile der Schreibmarke steht oben (die Tastatur
    /// liegt direkt darunter und ist höher als der Platz darunter)
    fokus: bool,
    /// erste sichtbare Notiz der Liste (Wischen blättert)
    oben: usize,
    /// Datei zu gross: nur lesen
    nur_lesen: bool,
    /// Ungespeichertes verwerfen: erst beim zweiten Antippen desselben Knopfs
    verwerfen: Option<u32>,
}

/// Erste sichtbare Zeile: mit Tastatur die Zeile der Schreibmarke, sonst so,
/// dass sie die letzte sichtbare ist.
fn erste_zeile(z: &Zustand, marke_zeile: usize) -> usize {
    if z.fokus { marke_zeile } else { marke_zeile.saturating_sub(ZEILEN_T - 1) }
}

fn t(en: &'static str, de: &'static str) -> &'static str {
    kern::sprache::t(en, de)
}

fn text(x: i32, y: i32, s: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, farbe: u32) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, farbe);
}

fn verbinden(ordner: &str, name: &str) -> String {
    if ordner.ends_with('/') { format!("{ordner}{name}") } else { format!("{ordner}/{name}") }
}

/// Standardordner: eigener Ordner auf der SD-Karte, sonst intern.
fn standard_ordner() -> String {
    if dateimanager::art("/sd0/1/ESP32-OS") == Some(true) {
        "/sd0/1/ESP32-OS/Notizen".into()
    } else {
        "/daten/Notizen".into()
    }
}

/// Notizen des Ordners (`*.txt`, nach Namen).
fn notizen_lesen(ordner: &str) -> Vec<String> {
    dateimanager::lesen(ordner)
        .map(|v| v.into_iter().filter(|e| !e.ordner && e.name.to_lowercase().ends_with(".txt")).map(|e| e.name).collect())
        .unwrap_or_default()
}

/// Datei ganz lesen (UTF-8, Fehler ersetzt).
fn datei_lesen(pfad: &str) -> Option<(String, bool)> {
    let mut v = Vec::new();
    let mut puffer = vec![0u8; 4096];
    loop {
        let (c, n) = datei::lesen(pfad, v.len() as u64, &mut puffer);
        if c != datei::OK && c != datei::EOF {
            return None;
        }
        v.extend_from_slice(&puffer[..n as usize]);
        if c == datei::EOF || n == 0 {
            break;
        }
        // grösser als MAX_DATEI: abgeschnitten (zweiter Wert), nur lesen
        if v.len() > MAX_DATEI {
            v.truncate(MAX_DATEI);
            return Some((String::from_utf8_lossy(&v).into_owned(), true));
        }
    }
    Some((String::from_utf8_lossy(&v).into_owned(), false))
}

/// Name aus der ersten Zeile: höchstens 24 Zeichen, nur unbedenkliche.
fn name_aus_text(text: &[char]) -> String {
    let erste: String = text.iter().take_while(|&&c| c != '\n').collect();
    let mut n: String = erste
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .take(24)
        .collect::<String>()
        .trim()
        .to_string();
    if n.is_empty() {
        n = t("Note", "Notiz").into();
    }
    format!("{n}.txt")
}

/// Text in Zeilen umbrechen (an Zeilenenden und nach SPALTEN Zeichen);
/// liefert je Bildschirmzeile den Index ihres ersten Zeichens.
fn zeilen(text: &[char]) -> Vec<usize> {
    let mut v = vec![0];
    let mut spalte = 0;
    for (i, &c) in text.iter().enumerate() {
        if c == '\n' {
            v.push(i + 1);
            spalte = 0;
        } else {
            spalte += 1;
            if spalte == SPALTEN {
                v.push(i + 1);
                spalte = 0;
            }
        }
    }
    v
}

/// Bildschirmzeile der Schreibmarke (Arbeitsbereich-Koordinate y).
fn marke_y(z: &Zustand) -> i32 {
    let z_anfang = zeilen(&z.text);
    let marke_zeile = z_anfang.iter().rposition(|&a| a <= z.marke).unwrap_or(0);
    let erste = erste_zeile(z, marke_zeile);
    TEXT_Y + 4 + (marke_zeile - erste) as i32 * ZEILE_T
}

/// Lage des Textfelds für die Bildschirmtastatur: die Zeile der
/// Schreibmarke – die Tastatur verdeckt sie nicht.
fn feld_melden(z: &Zustand) {
    sys::textfeld_lage(TEXT_X, marke_y(z) - 2, TEXT_W, ZEILE_T + 2);
}

fn zeichnen(z: &Zustand) {
    sys::neuzeichnen(1);
    let titel = match &z.name {
        Some(n) => format!("{} - {n}{}", t("Notes", "Notizen"), if z.geaendert { " *" } else { "" }),
        None => format!("{} - {}{}", t("Notes", "Notizen"), t("new", "neu"), if z.geaendert { " *" } else { "" }),
    };
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xF2F0EA, 0, Some(&titel));
    // Funktion 0 setzt den Titel nur beim Anlegen (71.1 danach)
    sys::titel(&titel);
    knopf(K_NEU, NEU.0, KNOPF_Y, NEU.1, KNOPF_H, 0xD8D8D8);
    text(NEU.0 + 8, KNOPF_Y + 12, t("New", "Neu"), 0x202020);
    let speicherbar = z.geaendert && !z.nur_lesen;
    knopf(K_SPEICHERN, SPEICHERN.0, KNOPF_Y, SPEICHERN.1, KNOPF_H, if speicherbar { 0x3A7BD5 } else { 0xD8D8D8 });
    text(SPEICHERN.0 + 8, KNOPF_Y + 12, t("Save", "Speichern"), if speicherbar { 0xFFFFFF } else { 0x202020 });
    // Liste (ab `oben`; Wischen blättert)
    for (i, n) in z.notizen.iter().enumerate().skip(z.oben).take(LISTE_ZEILEN) {
        let y = LISTE_Y + (i - z.oben) as i32 * ZEILE_H;
        let gewaehlt = z.name.as_deref() == Some(n.as_str());
        knopf(K_EINTRAG + i as u32, LISTE_X, y, LISTE_W, ZEILE_H - 3, if gewaehlt { 0xCFE0F5 } else { 0xFFFFFF });
        let kurz: String = n.trim_end_matches(".txt").chars().take(17).collect();
        text(LISTE_X + 6, y + (ZEILE_H - 3 - 16) / 2, &kurz, 0x202020);
    }
    if z.notizen.is_empty() {
        text(LISTE_X + 4, LISTE_Y + 6, t("(no notes)", "(keine Notizen)"), 0x808080);
    }
    // Textbereich (als Knopf: Antippen öffnet die Tastatur)
    knopf(K_TEXT | 0x4000_0000, TEXT_X, TEXT_Y, TEXT_W, TEXT_H, 0xFFFFFF);
    sys::rechteck((TEXT_X as u32) << 16 | TEXT_W as u32, (TEXT_Y as u32) << 16 | TEXT_H as u32, 0xFFFFFF);
    let z_anfang = zeilen(&z.text);
    // Zeile der Schreibmarke sichtbar halten
    let marke_zeile = z_anfang.iter().rposition(|&a| a <= z.marke).unwrap_or(0);
    let erste = erste_zeile(z, marke_zeile);
    for (j, &a) in z_anfang.iter().enumerate().skip(erste).take(ZEILEN_T) {
        let ende = z_anfang.get(j + 1).copied().unwrap_or(z.text.len());
        let s: String = z.text[a..ende].iter().filter(|&&c| c != '\n').collect();
        let y = TEXT_Y + 4 + (j - erste) as i32 * ZEILE_T;
        text(TEXT_X + 6, y, &s, 0x101010);
        if j == marke_zeile {
            let spalte = z.text[a..z.marke].iter().filter(|&&c| c != '\n').count() as i32;
            sys::rechteck(((TEXT_X + 6 + spalte * ZEICHEN_B) as u32) << 16 | 2, (y as u32) << 16 | 16, 0xC03020);
        }
    }
    if let Some((s, f)) = &z.status {
        let s: String = s.chars().take(70).collect();
        text(TEXT_X, TEXT_Y + TEXT_H + 8, &s, *f);
    }
    sys::neuzeichnen(2);
}

fn oeffnen(z: &mut Zustand, name: &str) {
    match datei_lesen(&verbinden(&z.ordner, name)) {
        Some((s, abgeschnitten)) => {
            z.text = s.chars().filter(|&c| c != '\r').collect();
            z.marke = z.text.len();
            z.name = Some(name.into());
            z.geaendert = false;
            z.nur_lesen = abgeschnitten;
            z.status = abgeschnitten.then(|| (t("File too large (over 64 KB) - read only", "Datei zu gross (über 64 KB) - nur lesen").to_string(), 0xB03020));
        }
        None => z.status = Some((format!("{} {name}", t("Cannot open", "Nicht zu öffnen:")), 0xB03020)),
    }
}

fn speichern(z: &mut Zustand) {
    if dateimanager::art(&z.ordner).is_none() && datei::ordner_erstellen(&z.ordner) != datei::OK {
        z.status = Some((format!("{} {}", t("Cannot create folder", "Ordner nicht anlegbar:"), z.ordner), 0xB03020));
        return;
    }
    let name = match &z.name {
        Some(n) => n.clone(),
        None => {
            let wunsch = name_aus_text(&z.text);
            match dateimanager::freier_name(&z.ordner, &wunsch) {
                Some(n) => n,
                None => {
                    z.status = Some((t("No free file name", "Kein freier Dateiname").into(), 0xB03020));
                    return;
                }
            }
        }
    };
    let pfad = verbinden(&z.ordner, &name);
    let inhalt: String = z.text.iter().collect();
    let (c, _) = datei::erstellen(&pfad, inhalt.as_bytes());
    if c != datei::OK {
        z.status = Some((format!("{} ({})", t("Save failed", "Speichern fehlgeschlagen"), dateimanager::fehlertext(c)), 0xB03020));
        return;
    }
    sys::debug_text(&format!("Notizen: gespeichert {pfad} ({} Byte)", inhalt.len()));
    z.name = Some(name.clone());
    z.geaendert = false;
    z.notizen = notizen_lesen(&z.ordner);
    z.status = Some((format!("{} {name}", t("Saved:", "Gespeichert:")), 0x207020));
}

/// Taste (Funktion 2): Zeichen, Rücktaste, Eingabe, Pfeile.
fn taste(z: &mut Zustand, code: u32) -> bool {
    // zu grosse Datei: nur Pfeile (lesen), keine Änderung
    if z.nur_lesen && !matches!((code >> 8) & 0xFF, 0xB0 | 0xB3) {
        return false;
    }
    z.verwerfen = None;
    let zeichen = if code & 0xFF == 3 {
        char::from_u32(code >> 16)
    } else {
        match (code >> 8) & 0xFF {
            8 => {
                if z.marke > 0 {
                    z.marke -= 1;
                    z.text.remove(z.marke);
                    z.geaendert = true;
                }
                return true;
            }
            13 => Some('\n'),
            0xB0 => {
                z.marke = z.marke.saturating_sub(1);
                return true;
            }
            0xB3 => {
                z.marke = (z.marke + 1).min(z.text.len());
                return true;
            }
            c if c >= 32 => char::from_u32(kern::schrift::cp866_zu_uni(c as u8)),
            _ => None,
        }
    };
    match zeichen {
        Some(c) if z.text.len() < 64 * 1024 => {
            z.text.insert(z.marke, c);
            z.marke += 1;
            z.geaendert = true;
            true
        }
        _ => false,
    }
}

/// Programm „notizen“.
pub fn programm() {
    let arg = kern::argumente();
    let arg = arg.trim();
    // Argument: Datei (aus dem Dateimanager, auch .ini/.log/.md/.cfg) oder Ordner
    let ist_datei = dateimanager::art(arg) == Some(false) || arg.to_lowercase().ends_with(".txt");
    let (ordner, datei_name) = if ist_datei {
        match arg.rfind('/') {
            Some(i) => (arg[..i].to_string(), Some(arg[i + 1..].to_string())),
            None => (standard_ordner(), Some(arg.to_string())),
        }
    } else if !arg.is_empty() {
        (arg.to_string(), None)
    } else {
        (standard_ordner(), None)
    };
    let mut z = Zustand { notizen: notizen_lesen(&ordner), ordner, name: None, text: Vec::new(), marke: 0, geaendert: false, status: None, fokus: false, oben: 0, nur_lesen: false, verwerfen: None };
    // Ereignisse: Neuzeichnen, Tasten, Knöpfe, Maus (Wischen = Scrollen)
    sys::maske(0x27);
    if let Some(n) = datei_name {
        oeffnen(&mut z, &n);
    }
    sys::debug_text(&format!("Notizen: Ordner {}, {} Notizen", z.ordner, z.notizen.len()));
    loop {
        match sys::warte_ereignis() {
            1 => zeichnen(&z),
            2 => {
                let mut neu = false;
                loop {
                    let c = sys::taste();
                    if c & 0xFF == 1 {
                        break;
                    }
                    neu |= taste(&mut z, c);
                }
                if neu {
                    zeichnen(&z);
                    feld_melden(&z);
                }
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                match id {
                    1 => {
                        sys::textfeld_fokus(false);
                        return;
                    }
                    K_NEU | K_EINTRAG.. if z.geaendert && !z.nur_lesen && z.verwerfen != Some(id) => {
                        // Ungespeichertes nicht stillschweigend verwerfen
                        z.verwerfen = Some(id);
                        z.status = Some((t("Not saved - tap again to discard", "Nicht gespeichert - nochmal tippen zum Verwerfen").into(), 0xB03020));
                    }
                    K_NEU => {
                        z.verwerfen = None;
                        z.nur_lesen = false;
                        z.name = None;
                        z.text.clear();
                        z.marke = 0;
                        z.geaendert = false;
                        z.status = None;
                        sys::textfeld_fokus(false);
                        z.fokus = false;
                    }
                    K_SPEICHERN if z.nur_lesen => {
                        z.status = Some((t("File too large (over 64 KB) - read only", "Datei zu gross (über 64 KB) - nur lesen").into(), 0xB03020));
                        sys::textfeld_fokus(false);
                        z.fokus = false;
                    }
                    K_SPEICHERN => {
                        z.verwerfen = None;
                        speichern(&mut z);
                        sys::textfeld_fokus(false);
                        z.fokus = false;
                    }
                    K_TEXT => {
                        // Tastatur auf, ohne den Text zu verdecken
                        sys::textfeld_fokus(true);
                        z.fokus = true;
                        zeichnen(&z);
                        feld_melden(&z);
                    }
                    i if i >= K_EINTRAG => {
                        z.verwerfen = None;
                        if let Some(n) = z.notizen.get((i - K_EINTRAG) as usize).cloned() {
                            oeffnen(&mut z, &n);
                        }
                        sys::textfeld_fokus(false);
                        z.fokus = false;
                    }
                    _ => {}
                }
                zeichnen(&z);
            }
            6 => {
                // Wischen blättert die Liste (mehr Notizen als Zeilen)
                let s = sys::maus(7) as i16 as i32;
                let max = z.notizen.len().saturating_sub(LISTE_ZEILEN);
                let neu = (z.oben as i32 + s.signum()).clamp(0, max as i32) as usize;
                if s != 0 && neu != z.oben {
                    z.oben = neu;
                    zeichnen(&z);
                }
            }
            _ => {}
        }
    }
}
