//! Dateimanager (P1) – nativ im KolibriOS-Stil (Vorbild Eolite; KolibriOS
//! hat keinen Dateimanager in C: KFM/KFar sind FASM, Eolite ist C--).
//!
//! Touch: links die Geräte (SD-Karte, interner Speicher), rechts die Liste.
//! Tippen auf einen Ordner öffnet ihn, auf eine Datei öffnet sie mit dem
//! zugehörigen Programm. Langer Druck auf einen Eintrag = Kontextmenü
//! (Öffnen, Kopieren, Ausschneiden, Umbenennen, Löschen); oben die Knöpfe
//! „Hoch“, „Neuer Ordner“, „Einfügen“. Wischen blättert. Namen werden in
//! einem Dialog mit Eingabefeld eingegeben (Bildschirmtastatur öffnet sich).
//!
//! D4 Papierkorb: „Löschen“ verschiebt in den Papierkorb des Geräts (SD:
//! `ESP32-OS/Papierkorb` – auf der SD-Karte schreibt ESP32-OS nur in seinen
//! eigenen Ordner; intern: `Papierkorb`), der Herkunftspfad steht in der
//! Datei `.herkunft` im Papierkorb. Links unter den Geräten öffnet
//! „Papierkorb“ ihn; dort: Wiederherstellen, Endgültig löschen, Leeren.
//! Ohne Papierkorb (SD ohne Ordner ESP32-OS) wird wie bisher endgültig
//! gelöscht.
//! Jede Änderung schreibt die Dateiliste ins Systemprotokoll (`DM:`).

use crate::shell::{LEISTE_H, bild_auf, kontextmenue, symbol};
use kern::sprache::t;
use kern::{datei, sys};
use kolibri::boxlib::{EditBox, edit_box_draw, edit_box_key, edit_box_mouse, edit_box_set_text};
use kolibri::dekoder::Rgba;
use std::sync::{Arc, Mutex};

/// Breite der Geräteleiste, Höhe der Knopfleiste, Zeilenhöhe (42 px ≈ 7,3 mm).
pub const SEITE_W: i32 = 150;
pub const LEISTE: i32 = 46;
pub const ZEILE: i32 = 42;
const BLOCK: usize = 32 * 1024;

const K_HOCH: u32 = 2;
const K_NEU: u32 = 3;
const K_EINFUEGEN: u32 = 4;
const K_KORB: u32 = 5;
const K_LEEREN: u32 = 6;
/// Name des Papierkorbs und der Herkunftsliste darin.
pub const KORB: &str = "Papierkorb";
const HERKUNFT: &str = ".herkunft";
const K_GERAET: u32 = 10;
const K_EINTRAG: u32 = 100;

const HG: u32 = 0xF0F0F0;
/// Symbol des Papierkorbs (KolibriOS-Symbolsatz).
const SYMBOL_KORB: usize = 39;
const WAHL: u32 = 0xCFE0F5;

/// Eintrag eines Ordners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub name: String,
    pub ordner: bool,
    pub groesse: u64,
}

/// Ordner lesen (ohne „.“/„..“), Ordner zuerst, dann nach Namen.
pub fn lesen(pfad: &str) -> Result<Vec<Eintrag>, u32> {
    let mut v = Vec::new();
    let mut start = 0;
    loop {
        let mut b = vec![0u8; 32 + 64 * 560];
        let (c, n) = datei::ordner_lesen(pfad, start, 64, 3, &mut b);
        if c != datei::OK && c != datei::EOF {
            return Err(c);
        }
        for i in 0..n as usize {
            let o = 32 + i * 560;
            let attr = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
            let mut g = [0u8; 8];
            g.copy_from_slice(&b[o + 32..o + 40]);
            let z = &b[o + 40..o + 560];
            let name = String::from_utf8_lossy(&z[..z.iter().position(|&c| c == 0).unwrap_or(0)]).into_owned();
            if name != "." && name != ".." && !name.is_empty() {
                v.push(Eintrag { name, ordner: attr & 0x10 != 0, groesse: u64::from_le_bytes(g) });
            }
        }
        if c != datei::OK || n < 64 {
            break;
        }
        start += 64;
    }
    v.sort_by(|a, b| b.ordner.cmp(&a.ordner).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(v)
}

fn verbinden(ordner: &str, name: &str) -> String {
    format!("{}/{}", ordner.trim_end_matches('/'), name)
}

fn eltern(p: &str) -> String {
    let t = datei::normalisieren(p, "/");
    format!("/{}", t[..t.len().saturating_sub(1)].join("/"))
}

fn name_von(p: &str) -> &str {
    p.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// Ist `p` eine Datei (Some(false)), ein Ordner (Some(true)) oder fehlt sie?
pub fn art(p: &str) -> Option<bool> {
    let mut b = [0u8; 40];
    (datei::info(p, &mut b) == datei::OK).then_some(b[0] & 0x10 != 0)
}

/// Einhängung, zu der `p` gehört (längstes Präfix, Gross/Klein egal).
fn einhaengung(p: &str) -> Option<String> {
    let t: Vec<String> = datei::normalisieren(p, "/").iter().map(|s| s.to_lowercase()).collect();
    datei::einhaengungen()
        .into_iter()
        .map(|(e, _)| e)
        .filter(|e| {
            let et: Vec<&str> = e.trim_matches('/').split('/').collect();
            t.len() >= et.len() && t[..et.len()].iter().zip(&et).all(|(a, b)| a == b)
        })
        .max_by_key(|e| e.len())
}

/// Freier Name im Ordner: „x.png“, sonst „x (Kopie).png“, „x (Kopie 2).png“ …
/// Höchstens 100 Versuche (ein Dateisystem ohne lange Namen bildet alle
/// Varianten auf denselben Kurznamen ab – dann `None` statt Endlosschleife).
pub fn freier_name(ordner: &str, name: &str) -> Option<String> {
    if art(&verbinden(ordner, name)).is_none() {
        return Some(name.to_string());
    }
    let (stamm, endung) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let kopie = t("Copy", "Kopie");
    (1..=100).map(|n| if n == 1 { format!("{stamm} ({kopie}){endung}") } else { format!("{stamm} ({kopie} {n}){endung}") }).find(|k| art(&verbinden(ordner, k)).is_none())
}

fn liegt_in(kind: &str, eltern: &str) -> bool {
    let k = datei::normalisieren(kind, "/").iter().map(|s| s.to_lowercase()).collect::<Vec<_>>();
    let e = datei::normalisieren(eltern, "/").iter().map(|s| s.to_lowercase()).collect::<Vec<_>>();
    k.len() >= e.len() && k[..e.len()] == e[..]
}

/// Datei oder Ordner (rekursiv) kopieren. Fehlercode wie Funktion 70.
pub fn kopieren(q: &str, z: &str) -> Result<(), u32> {
    match art(q) {
        None => Err(datei::NICHT_GEFUNDEN),
        Some(true) => {
            if liegt_in(z, q) {
                return Err(datei::VERWEIGERT);
            }
            let c = datei::ordner_erstellen(z);
            if c != datei::OK {
                return Err(c);
            }
            for e in lesen(q)? {
                kopieren(&verbinden(q, &e.name), &verbinden(z, &e.name))?;
            }
            Ok(())
        }
        Some(false) => {
            let mut buf = vec![0u8; BLOCK];
            let mut off = 0u64;
            loop {
                let (c, n) = datei::lesen(q, off, &mut buf);
                if c != datei::OK && c != datei::EOF {
                    return Err(c);
                }
                let (w, _) = if off == 0 { datei::erstellen(z, &buf[..n as usize]) } else { datei::schreiben(z, off, &buf[..n as usize]) };
                if w != datei::OK {
                    return Err(w);
                }
                off += u64::from(n);
                if c == datei::EOF || n == 0 {
                    return Ok(());
                }
            }
        }
    }
}

/// Datei oder Ordner (rekursiv) löschen.
pub fn loeschen(p: &str) -> Result<(), u32> {
    if art(p) == Some(true) {
        for e in lesen(p)? {
            loeschen(&verbinden(p, &e.name))?;
        }
    }
    match datei::loeschen(p) {
        datei::OK => Ok(()),
        c => Err(c),
    }
}

/// Verschieben: auf derselben Partition umbenennen, sonst kopieren + löschen.
pub fn verschieben(q: &str, z: &str) -> Result<(), u32> {
    if liegt_in(z, q) {
        return Err(datei::VERWEIGERT);
    }
    let (eq, ez) = (einhaengung(q), einhaengung(z));
    if let (Some(a), Some(b)) = (&eq, &ez)
        && a == b
    {
        // Ziel relativ zur Wurzel der Partition (Funktion 70.10, absolut)
        let rel = datei::normalisieren(z, "/")[a.trim_matches('/').split('/').count()..].join("/");
        if datei::umbenennen(q, &format!("/{rel}")) == datei::OK {
            return Ok(());
        }
    }
    kopieren(q, z)?;
    loeschen(q)
}

/// Fehlertext zu einem Code von Funktion 70.
/// D4: Papierkorb des Geräts, zu dem `p` gehört (None: SD-Karte ohne
/// Ordner ESP32-OS – dann gibt es keinen, gelöscht wird endgültig).
pub fn papierkorb_von(p: &str) -> Option<String> {
    let wurzel = format!("/{}", einhaengung(p)?.trim_matches('/'));
    if liegt_in(&wurzel, "/sd0/1") {
        let eigen = verbinden(&wurzel, "ESP32-OS");
        (art(&eigen) == Some(true)).then(|| verbinden(&eigen, KORB))
    } else {
        Some(verbinden(&wurzel, KORB))
    }
}

/// Ist `p` selbst ein Papierkorb?
fn ist_korb(p: &str) -> bool {
    papierkorb_von(p).is_some_and(|k| liegt_in(p, &k) && liegt_in(&k, p))
}

/// Herkunftsliste (Name im Papierkorb, ursprünglicher Pfad).
fn herkunft_lesen(korb: &str) -> Vec<(String, String)> {
    let mut buf = vec![0u8; 64 * 1024];
    let (_, n) = datei::lesen(&verbinden(korb, HERKUNFT), 0, &mut buf);
    buf.truncate(n as usize);
    String::from_utf8_lossy(&buf)
        .lines()
        .filter_map(|z| z.split_once('\t').map(|(a, b)| (a.to_string(), b.to_string())))
        .collect()
}

fn herkunft_schreiben(korb: &str, liste: &[(String, String)]) -> Result<(), u32> {
    let text: String = liste.iter().map(|(n, p)| format!("{n}\t{p}\n")).collect();
    match datei::erstellen(&verbinden(korb, HERKUNFT), text.as_bytes()).0 {
        datei::OK => Ok(()),
        c => Err(c),
    }
}

/// Ordner samt fehlenden Elternordnern anlegen.
fn ordner_pfad_anlegen(p: &str) -> Result<(), u32> {
    if art(p) == Some(true) {
        return Ok(());
    }
    let e = eltern(p);
    if e != p && art(&e).is_none() {
        ordner_pfad_anlegen(&e)?;
    }
    match datei::ordner_erstellen(p) {
        datei::OK => Ok(()),
        c => Err(c),
    }
}

/// `p` in den Papierkorb `korb` verschieben (Name dort ggf. „(Kopie)“).
/// Liefert den Namen im Papierkorb.
pub fn in_papierkorb(p: &str, korb: &str) -> Result<String, u32> {
    ordner_pfad_anlegen(korb)?;
    let name = freier_name(korb, name_von(p)).ok_or(datei::VERWEIGERT)?;
    verschieben(p, &verbinden(korb, &name))?;
    let mut h = herkunft_lesen(korb);
    h.retain(|(n, _)| n != &name);
    h.push((name.clone(), p.to_string()));
    herkunft_schreiben(korb, &h)?;
    Ok(name)
}

/// Eintrag `name` aus dem Papierkorb an seinen alten Ort zurück (fehlende
/// Ordner werden angelegt, belegter Name → „(Kopie)“). Liefert den Pfad.
pub fn wiederherstellen(korb: &str, name: &str) -> Result<String, u32> {
    let mut h = herkunft_lesen(korb);
    let orig = h.iter().find(|(n, _)| n == name).map(|(_, p)| p.clone()).ok_or(datei::NICHT_GEFUNDEN)?;
    let ordner = eltern(&orig);
    ordner_pfad_anlegen(&ordner)?;
    let ziel = verbinden(&ordner, &freier_name(&ordner, name_von(&orig)).ok_or(datei::VERWEIGERT)?);
    verschieben(&verbinden(korb, name), &ziel)?;
    h.retain(|(n, _)| n != name);
    herkunft_schreiben(korb, &h)?;
    Ok(ziel)
}

/// Eintrag endgültig löschen (und aus der Herkunftsliste nehmen).
pub fn endgueltig_loeschen(korb: &str, name: &str) -> Result<(), u32> {
    loeschen(&verbinden(korb, name))?;
    let mut h = herkunft_lesen(korb);
    h.retain(|(n, _)| n != name);
    herkunft_schreiben(korb, &h)
}

/// Papierkorb leeren: alle Einträge und die Herkunftsliste löschen.
pub fn papierkorb_leeren(korb: &str) -> Result<(), u32> {
    for e in lesen(korb)? {
        loeschen(&verbinden(korb, &e.name))?;
    }
    Ok(())
}

pub fn fehlertext(c: u32) -> &'static str {
    match c {
        datei::NICHT_UNTERSTUETZT => t("not supported", "nicht unterstützt"),
        datei::NICHT_GEFUNDEN => t("not found", "nicht gefunden"),
        datei::VOLL => t("storage full", "Speicher voll"),
        datei::FS_FEHLER => t("file system error", "Dateisystemfehler"),
        datei::VERWEIGERT => t("access denied", "Zugriff verweigert"),
        datei::GERAET => t("device error", "Gerätefehler"),
        _ => t("error", "Fehler"),
    }
}

/// Programm zum Öffnen einer Datei (nach Endung).
pub fn programm_fuer(name: &str) -> Option<&'static str> {
    let e = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match e.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" => Some("bildbetrachter"),
        "txt" | "ini" | "log" | "md" | "cfg" => Some("notizen"),
        "mp3" => Some("musik"),
        "pdf" => Some("pdfbetrachter"),
        _ => None,
    }
}

fn symbol_fuer(e: &Eintrag) -> usize {
    if e.ordner {
        return 45;
    }
    match programm_fuer(&e.name) {
        Some("bildbetrachter") => 70,
        Some("notizen") => 9,
        Some("musik") => 64,
        Some("pdfbetrachter") => 73,
        _ => 95,
    }
}

fn groesse_text(g: u64) -> String {
    match g {
        g if g < 1024 => format!("{g} B"),
        g if g < 1024 * 1024 => format!("{} KB", g.div_ceil(1024)),
        g => format!("{:.1} MB", g as f64 / (1024.0 * 1024.0)),
    }
}

fn text(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

/// Text auf `max` Zeichen kürzen („…“ gibt es in der Schrift nicht: „..“).
/// Text auf höchstens zwei Zeilen zu `max` Zeichen umbrechen (an einem
/// Leerzeichen, sonst hart); die zweite Zeile kann länger sein (kürzt der
/// Aufrufer).
fn zwei_zeilen(t: &str, max: usize) -> (String, String) {
    let z: Vec<char> = t.chars().collect();
    if z.len() <= max {
        return (t.to_string(), String::new());
    }
    let schnitt = z[..=max].iter().rposition(|&c| c == ' ').filter(|&i| i > 0).unwrap_or(max);
    let erste: String = z[..schnitt].iter().collect();
    let zweite: String = z[schnitt..].iter().collect::<String>().trim_start().to_string();
    (erste, zweite)
}

fn kuerzen(t: &str, max: usize) -> String {
    if t.chars().count() <= max { t.to_string() } else { format!("{}..", t.chars().take(max.saturating_sub(2)).collect::<String>()) }
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, farbe: u32, beschriftung: &str, tf: u32) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, farbe);
    let n = beschriftung.chars().count() as i32 * 8;
    text(x + (w - n) / 2, y + (h - 16) / 2, beschriftung, tf);
}

/// Liste der Dateiliste ins Protokoll (Nachweis am Board).
fn protokoll(pfad: &str, liste: &[Eintrag]) {
    let namen: Vec<String> = liste.iter().map(|e| if e.ordner { format!("{}/", e.name) } else { format!("{} ({})", e.name, e.groesse) }).collect();
    sys::debug_text(&format!("DM: {pfad}: {}\n", namen.join(", ")));
}

// ------------------------------------------------------------ Dialoge

/// Läuft `f` in einem eigenen Faden (eigenes Fenster) und wartet darauf.
fn im_faden<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let erg: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
    let e2 = erg.clone();
    let fertig = Arc::new(Mutex::new(false));
    let f2 = fertig.clone();
    if sys::faden(
        96,
        Box::new(move || {
            let r = f();
            *e2.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            *f2.lock().unwrap_or_else(|e| e.into_inner()) = true;
        }),
    ) < 0
    {
        return None;
    }
    while !*fertig.lock().unwrap_or_else(|e| e.into_inner()) {
        sys::warten(2);
    }
    erg.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Namen eingeben (Umbenennen, neuer Ordner). Feld und Knöpfe liegen in
/// einer Zeile oben, damit die Bildschirmtastatur sie nicht verdeckt;
/// „Enter“ der Tastatur bestätigt.
pub fn name_dialog(titel: &str, vorgabe: &str) -> Option<String> {
    eingabe_dialog(titel, vorgabe, false).map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

/// Passwort eingeben (WLAN): verdeckt (`*`), „Zeigen“ schaltet um; nicht
/// gekürzt (Leerzeichen gehören zum Passwort). `None` = abgebrochen.
pub fn passwort_dialog(titel: &str, vorgabe: &str) -> Option<String> {
    eingabe_dialog(titel, vorgabe, true)
}

/// Eingabefeld-Dialog (gemeinsam für Namen und Passwörter).
fn eingabe_dialog(titel: &str, vorgabe: &str, passwort: bool) -> Option<String> {
    let (titel, vorgabe) = (titel.to_string(), vorgabe.to_string());
    im_faden(move || {
        let mut puffer = vec![0u8; 256];
        let mut feld = EditBox {
            width: if passwort { 268 } else { 360 },
            left: 8,
            top: 22,
            color: 0xFFFFFF,
            shift_color: 0x94AECE,
            focus_border_color: 0x3A7AC0,
            blur_border_color: 0x8A8A8A,
            text_color: 0x3000_0000,
            max: 255,
            text: puffer.as_mut_ptr(),
            mouse_variable: core::ptr::null_mut(),
            flags: if passwort { 2 | 1 } else { 2 },
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
        edit_box_set_text(&mut feld, vorgabe.as_bytes());
        if !passwort {
            // Namensstamm markieren: Tippen ersetzt ihn, die Endung bleibt
            let stamm = match vorgabe.rfind('.') {
                Some(i) if i > 0 => i,
                _ => vorgabe.len(),
            };
            feld.shift_old = 0;
            feld.shift = stamm as u32;
            feld.pos = stamm as u32;
        }
        sys::maske(0x27);
        let ergebnis = |feld: &EditBox, puffer: &[u8]| String::from_utf8_lossy(&puffer[..feld.size as usize]).into_owned();
        loop {
            match sys::warte_ereignis() {
                1 => {
                    sys::neuzeichnen(1);
                    sys::fenster(20 << 16 | 599, 48 + 5 + 50 - 1, 0x33 << 24 | HG, 0, Some(&titel));
                    edit_box_draw(&mut feld);
                    if passwort {
                        let verdeckt = feld.flags & 1 != 0;
                        knopf(4, 284, 3, 90, 42, 0xD8D8D8, if verdeckt { t("Show", "Zeigen") } else { t("Hide", "Verbergen") }, 0);
                    }
                    knopf(2, 380, 3, 90, 42, 0x3A7AC0, "OK", 0xFFFFFF);
                    knopf(3, 476, 3, 110, 42, 0xC8C8C8, t("Cancel", "Abbrechen"), 0);
                    sys::neuzeichnen(2);
                }
                2 => {
                    let k = sys::taste();
                    if k & 0xFF != 3 && (k >> 8 & 0xFF) == 13 {
                        let n = ergebnis(&feld, &puffer);
                        if passwort || !n.trim().is_empty() {
                            sys::textfeld_fokus(false);
                            return Some(n);
                        }
                    }
                    edit_box_key(&mut feld, k);
                }
                3 => match sys::knopf_id() >> 8 {
                    2 => {
                        let n = ergebnis(&feld, &puffer);
                        if passwort || !n.trim().is_empty() {
                            sys::textfeld_fokus(false);
                            return Some(n);
                        }
                    }
                    4 if passwort => {
                        feld.flags ^= 1;
                        edit_box_draw(&mut feld);
                        let verdeckt = feld.flags & 1 != 0;
                        knopf(4, 284, 3, 90, 42, 0xD8D8D8, if verdeckt { t("Show", "Zeigen") } else { t("Hide", "Verbergen") }, 0);
                    }
                    1 | 3 => {
                        sys::textfeld_fokus(false);
                        return None;
                    }
                    _ => {}
                },
                6 => edit_box_mouse(&mut feld),
                _ => {}
            }
        }
    })
    .flatten()
}

/// Ja/Nein-Frage mit Touch-Knöpfen.
pub fn frage(titel: &str, frage: &str, ja: &str) -> bool {
    let (titel, frage, ja) = (titel.to_string(), frage.to_string(), ja.to_string());
    im_faden(move || {
        let (w, h) = (420, 48 + 5 + 110);
        let (x, y) = ((640 - w) / 2, (360 - LEISTE_H - h) / 2);
        sys::maske(0x07);
        loop {
            match sys::warte_ereignis() {
                1 => {
                    sys::neuzeichnen(1);
                    sys::fenster((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, 0x33 << 24 | HG, 0, Some(&titel));
                    bild_auf(&symbol(14, 32), 10, 10, |_, _| HG);
                    // zwei Zeilen zu 42 Zeichen (lange Namen, D4-Texte)
                    let (z1, z2) = zwei_zeilen(&frage, 42);
                    text(54, if z2.is_empty() { 18 } else { 10 }, &z1, 0x202020);
                    if !z2.is_empty() {
                        text(54, 28, &kuerzen(&z2, 42), 0x202020);
                    }
                    knopf(2, w - 10 - 110 - 10 - 120 - 10, 56, 120, 42, 0xC03030, &ja, 0xFFFFFF);
                    knopf(3, w - 10 - 110 - 10, 56, 110, 42, 0xC8C8C8, t("Cancel", "Abbrechen"), 0);
                    sys::neuzeichnen(2);
                }
                3 => match sys::knopf_id() >> 8 {
                    2 => return true,
                    1 | 3 => return false,
                    _ => {}
                },
                _ => {}
            }
        }
    })
    .unwrap_or(false)
}

// ------------------------------------------------------------ Programm

struct Geraet {
    pfad: String,
    /// Titel englisch / deutsch.
    titel: (&'static str, &'static str),
    symbol: usize,
}

fn geraete() -> Vec<Geraet> {
    let e = datei::einhaengungen();
    let mut v: Vec<Geraet> = Vec::new();
    let mut gesehen = Vec::new();
    for (p, titel, sym) in [("/sd0/1", ("SD card", "SD-Karte"), 50), ("/rd/1", ("Internal", "Intern"), 37), ("/sys", ("System", "System"), 7)] {
        if let Some((_, host)) = e.iter().find(|(k, _)| k == p.trim_matches('/'))
            && !gesehen.contains(host)
        {
            gesehen.push(host.clone());
            v.push(Geraet { pfad: p.to_string(), titel, symbol: sym });
        }
    }
    v
}

/// Zwischenablage des Dateimanagers: Pfade, ausschneiden?
type Ablage = Option<(Vec<String>, bool)>;

struct Zustand {
    pfad: String,
    liste: Vec<Eintrag>,
    oben: usize,
    meldung: String,
    ablage: Ablage,
    geraete: Vec<Geraet>,
    symbole: Vec<(usize, Rgba)>,
    /// S2: Symbole fertig auf eine Hintergrundfarbe gemischt (Symbol, Farbe)
    gemischt: Vec<((usize, u32), Rgba)>,
    /// S2: oberste sichtbare Zeile beim letzten Zeichnen der Liste (deren
    /// Knöpfe werden beim Blättern entfernt)
    gezeichnet_oben: usize,
}

impl Zustand {
    fn neu_lesen(&mut self) {
        match lesen(&self.pfad) {
            Ok(mut l) => {
                if ist_korb(&self.pfad) {
                    l.retain(|e| e.name != HERKUNFT);
                }
                self.liste = l;
            }
            Err(c) => {
                self.liste.clear();
                self.meldung = format!("{}: {}", t("Read", "Lesen"), fehlertext(c));
            }
        }
        self.oben = self.oben.min(self.liste.len().saturating_sub(1));
        protokoll(&self.pfad, &self.liste);
    }

    fn oeffne_ordner(&mut self, p: String) {
        self.pfad = p;
        self.oben = 0;
        self.meldung.clear();
        self.neu_lesen();
    }

    /// S2: Symbol `i` auf die einfarbige Fläche `farbe` gemischt – einmal,
    /// danach aus dem Zwischenspeicher (vorher je Bild und Zeile neu gemischt
    /// und kopiert: Blättern ruckelte).
    fn sym_auf(&mut self, i: usize, farbe: u32) -> Rgba {
        if let Some((_, b)) = self.gemischt.iter().find(|(k, _)| *k == (i, farbe)) {
            return b.clone();
        }
        let roh = self.sym(i);
        let mut m = roh.clone();
        for (z, &p) in m.px.iter_mut().zip(&roh.px) {
            let a = p >> 24;
            let k = |s: u32| ((p >> s & 255) * a + (farbe >> s & 255) * (255 - a)) / 255;
            *z = 0xFF00_0000 | k(16) << 16 | k(8) << 8 | k(0);
        }
        self.gemischt.push(((i, farbe), m.clone()));
        m
    }

    fn sym(&mut self, i: usize) -> Rgba {
        if let Some((_, b)) = self.symbole.iter().find(|(k, _)| *k == i) {
            return b.clone();
        }
        let b = symbol(i, 32);
        self.symbole.push((i, b.clone()));
        b
    }
}

fn masse() -> (i32, i32) {
    let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
    (640 - 10, 360 - LEISTE_H - skin - 5)
}

fn sichtbar(h: i32) -> usize {
    ((h - LEISTE) / ZEILE).max(1) as usize
}

fn zeichnen(z: &mut Zustand) {
    let (w, h) = masse();
    sys::neuzeichnen(1);
    let titel = format!("{} - {}", t("File Manager", "Dateimanager"), z.pfad);
    sys::fenster(639, (360 - LEISTE_H - 1) as u32, 0x33 << 24 | HG, 0, Some(&titel));
    // Titel folgt dem Ordner (Funktion 0 übernimmt ihn nur beim ersten Mal)
    sys::titel(&titel);
    // Geräte
    sys::rechteck((SEITE_W - 2) as u32, h as u32, 0xE2E6EC);
    let geraete: Vec<(bool, usize, &'static str)> = z.geraete.iter().map(|g| (liegt_in(&z.pfad, &g.pfad), g.symbol, t(g.titel.0, g.titel.1))).collect();
    for (i, (aktiv, sym, titel)) in geraete.into_iter().enumerate() {
        let y = 4 + i as i32 * 48;
        let farbe = if aktiv { WAHL } else { 0xE2E6EC };
        sys::knopf(2 << 16 | (SEITE_W - 9) as u32, (y as u32) << 16 | 43, (K_GERAET + i as u32) | 0x4000_0000, farbe);
        sys::rechteck(2 << 16 | (SEITE_W - 8) as u32, (y as u32) << 16 | 44, farbe);
        let b = z.sym(sym);
        bild_auf(&b, 8, y + 6, |_, _| farbe);
        text(46, y + 14, titel, 0x202020);
    }
    // D4: Papierkorb unter den Geräten
    let im_korb = ist_korb(&z.pfad);
    let y = 4 + z.geraete.len() as i32 * 48;
    let farbe = if im_korb { WAHL } else { 0xE2E6EC };
    sys::knopf(2 << 16 | (SEITE_W - 9) as u32, (y as u32) << 16 | 43, K_KORB | 0x4000_0000, farbe);
    sys::rechteck(2 << 16 | (SEITE_W - 8) as u32, (y as u32) << 16 | 44, farbe);
    let b = z.sym(SYMBOL_KORB);
    bild_auf(&b, 8, y + 6, |_, _| farbe);
    text(46, y + 14, t("Recycle bin", "Papierkorb"), 0x202020);
    // Knopfleiste
    let x0 = SEITE_W + 4;
    knopf(K_HOCH, x0, 2, 80, 42, 0xD8D8D8, t("Up", "Hoch"), 0);
    if im_korb {
        knopf(K_LEEREN, x0 + 86, 2, 130, 42, 0xD8D8D8, t("Empty", "Leeren"), 0);
    } else {
        knopf(K_NEU, x0 + 86, 2, 130, 42, 0xD8D8D8, t("New folder", "Neuer Ordner"), 0);
    }
    let ab = z.ablage.as_ref().map_or(0, |a| a.0.len());
    if ab > 0 {
        knopf(K_EINFUEGEN, x0 + 222, 2, 100, 42, 0x3A7AC0, t("Paste", "Einfügen"), 0xFFFFFF);
    }
    let info = if z.meldung.is_empty() {
        match ab {
            0 => format!("{} {}", z.liste.len(), t("objects", "Objekte")),
            n => format!("{}: {n}", t("Clipboard", "Ablage")),
        }
    } else {
        z.meldung.clone()
    };
    let ix = x0 + if ab > 0 { 330 } else { 224 };
    text(ix, 15, &kuerzen(&info, ((w - ix - 4) / 8).max(0) as usize), 0x404040);
    liste_zeichnen(z);
    sys::neuzeichnen(2);
}

/// Dateiliste und Bildlaufanzeige (S2: beim Blättern nur dieser Teil).
fn liste_zeichnen(z: &mut Zustand) {
    let (w, h) = masse();
    let lx = SEITE_W + 4;
    let lw = w - lx - 10;
    let n = sichtbar(h);
    sys::rechteck((lx as u32) << 16 | lw as u32, (LEISTE as u32) << 16 | (h - LEISTE) as u32, 0xFFFFFF);
    if z.liste.is_empty() {
        text(lx + 12, LEISTE + 12, t("(empty)", "(leer)"), 0x808080);
    }
    let eintraege: Vec<Eintrag> = z.liste.iter().skip(z.oben).take(n).cloned().collect();
    for (i, e) in eintraege.iter().enumerate() {
        let y = LEISTE + i as i32 * ZEILE;
        let farbe = if i % 2 == 0 { 0xFFFFFF } else { 0xF4F6F8 };
        let markiert = z.ablage.as_ref().is_some_and(|(p, aus)| *aus && p.contains(&verbinden(&z.pfad, &e.name)));
        let farbe = if markiert { 0xE8E8E8 } else { farbe };
        sys::knopf((lx as u32) << 16 | (lw - 1) as u32, (y as u32) << 16 | (ZEILE - 1) as u32, (K_EINTRAG + (z.oben + i) as u32) | 0x4000_0000, farbe);
        sys::rechteck((lx as u32) << 16 | lw as u32, (y as u32) << 16 | ZEILE as u32, farbe);
        let b = z.sym_auf(symbol_fuer(e), farbe);
        kolibri::libimg::zeichnen(&b, lx + 6, y + (ZEILE - 32) / 2, b.w, b.h, 0, 0);
        let g = if e.ordner { String::new() } else { groesse_text(e.groesse) };
        let platz = ((lw - 46 - 90) / 8) as usize;
        text(lx + 46, y + (ZEILE - 16) / 2, &kuerzen(&e.name, platz), if markiert { 0x808080 } else if e.ordner { 0x1A4A8A } else { 0x101010 });
        text(lx + lw - 8 - g.chars().count() as i32 * 8, y + (ZEILE - 16) / 2, &g, 0x606060);
    }
    // Bildlaufanzeige (Spur erst leeren: beim Blättern bleibt das Fenster
    // sonst stehen und der alte Schieber sichtbar)
    sys::rechteck(((lx + lw) as u32) << 16 | 10, (LEISTE as u32) << 16 | (h - LEISTE) as u32, HG);
    if z.liste.len() > n {
        let bh = h - LEISTE;
        let dh = (bh * n as i32 / z.liste.len() as i32).max(12);
        let dy = LEISTE + (bh - dh) * z.oben as i32 / (z.liste.len() - n) as i32;
        sys::rechteck(((lx + lw + 2) as u32) << 16 | 6, (dy as u32) << 16 | dh as u32, 0x9AA4B0);
    }
    z.gezeichnet_oben = z.oben;
}

/// S2: Blättern – Knöpfe der bisher sichtbaren Zeilen entfernen, dann nur
/// die Liste neu zeichnen (nicht das ganze Fenster mit Geräten und Leiste).
fn liste_blaettern(z: &mut Zustand) {
    let (_, h) = masse();
    for i in z.gezeichnet_oben..z.gezeichnet_oben + sichtbar(h) {
        sys::knopf(0, 0, (K_EINTRAG + i as u32) | 0x8000_0000, 0);
    }
    liste_zeichnen(z);
}

/// Meldung, wenn ein Programm nicht startet (Rückgabe von `datei::starten`).
fn startfehler(prog: &str, r: i32) -> String {
    match r {
        -30 => format!("{prog}: {}", t("not enough memory – close other programs", "zu wenig Speicher – andere Programme schliessen")),
        -32 => format!("{prog}: {}", t("too many programs", "zu viele Programme")),
        r if r == -(datei::NICHT_GEFUNDEN as i32) => format!("{prog} {}", t("missing", "fehlt")),
        r => format!("{prog}: {} ({})", t("start failed", "Start fehlgeschlagen"), -r),
    }
}

fn oeffnen(z: &mut Zustand, e: &Eintrag) {
    let p = verbinden(&z.pfad, &e.name);
    if e.ordner {
        z.oeffne_ordner(p);
        return;
    }
    match programm_fuer(&e.name) {
        Some(prog) => {
            let r = datei::starten(&format!("/sys/{prog}"), &p);
            if r < 0 {
                z.meldung = startfehler(prog, r);
            }
        }
        None => {
            // Programmdatei? (Name = eingelinktes Programm)
            if datei::starten(&p, "") < 0 {
                z.meldung = t("No program for this", "Kein Programm dafür").to_string();
            }
        }
    }
}

fn einfuegen(z: &mut Zustand) {
    let Some((pfade, aus)) = z.ablage.clone() else { return };
    let mut fehler = None;
    for q in &pfade {
        let ziel_name = name_von(q).to_string();
        let gleicher_ordner = liegt_in(&eltern(q), &z.pfad) && liegt_in(&z.pfad, &eltern(q));
        if aus && gleicher_ordner {
            continue;
        }
        let Some(n) = freier_name(&z.pfad, &ziel_name) else {
            fehler = Some(t("no free name", "kein freier Name").to_string());
            continue;
        };
        let ziel = verbinden(&z.pfad, &n);
        let r = if aus { verschieben(q, &ziel) } else { kopieren(q, &ziel) };
        if let Err(c) = r {
            fehler = Some(fehlertext(c).to_string());
        }
    }
    if aus {
        z.ablage = None;
    }
    z.meldung = match fehler {
        Some(f) => format!("{}: {f}", t("Paste", "Einfügen")),
        None => format!("{} {}", pfade.len(), t("pasted", "eingefügt")),
    };
    z.neu_lesen();
}

/// Kontextmenü im Papierkorb: Wiederherstellen, Endgültig löschen.
fn kontext_korb(z: &mut Zustand, e: &Eintrag) {
    let m = sys::maus(0);
    let wahl = kontextmenue((m >> 16) as i32, (m & 0xFFFF) as i32, &[t("Restore", "Wiederherstellen"), t("Delete permanently", "Endgültig löschen")]);
    let korb = z.pfad.clone();
    match wahl {
        Some(0) => {
            z.meldung = match wiederherstellen(&korb, &e.name) {
                Ok(p) => format!("{}: {p}", t("Restored", "Wiederhergestellt")),
                Err(c) => format!("{}: {}", t("Restore", "Wiederherstellen"), fehlertext(c)),
            };
            z.neu_lesen();
        }
        Some(1) => {
            let f = match kern::sprache::aktuell() {
                kern::sprache::Sprache::Englisch => format!("Delete \"{}\" permanently?", e.name),
                kern::sprache::Sprache::Deutsch => format!("\"{}\" endgültig löschen?", e.name),
            };
            if frage(t("Delete", "Löschen"), &f, t("Delete", "Löschen")) {
                z.meldung = match endgueltig_loeschen(&korb, &e.name) {
                    Ok(()) => format!("{}: {}", t("Deleted", "Gelöscht"), e.name),
                    Err(c) => format!("{}: {}", t("Delete", "Löschen"), fehlertext(c)),
                };
                z.neu_lesen();
            }
        }
        _ => {}
    }
}

/// Papierkorb öffnen: der des aktuellen Geräts, sonst der interne.
fn korb_oeffnen(z: &mut Zustand) {
    let korb = papierkorb_von(&z.pfad).or_else(|| z.geraete.iter().find_map(|g| papierkorb_von(&g.pfad).filter(|_| !g.pfad.starts_with("/sd0"))));
    let Some(korb) = korb else {
        z.meldung = t("No recycle bin", "Kein Papierkorb").to_string();
        return;
    };
    if let Err(c) = ordner_pfad_anlegen(&korb) {
        z.meldung = format!("{}: {}", t("Recycle bin", "Papierkorb"), fehlertext(c));
        return;
    }
    z.oeffne_ordner(korb);
}

fn leeren(z: &mut Zustand) {
    if z.liste.is_empty() {
        return;
    }
    let f = match kern::sprache::aktuell() {
        kern::sprache::Sprache::Englisch => format!("Delete all {} objects permanently?", z.liste.len()),
        kern::sprache::Sprache::Deutsch => format!("Alle {} Objekte endgültig löschen?", z.liste.len()),
    };
    if frage(t("Empty", "Leeren"), &f, t("Empty", "Leeren")) {
        let korb = z.pfad.clone();
        z.meldung = match papierkorb_leeren(&korb) {
            Ok(()) => t("Recycle bin emptied", "Papierkorb geleert").to_string(),
            Err(c) => format!("{}: {}", t("Empty", "Leeren"), fehlertext(c)),
        };
        z.neu_lesen();
    }
}

fn kontext(z: &mut Zustand, i: usize) {
    let Some(e) = z.liste.get(i).cloned() else { return };
    if ist_korb(&z.pfad) {
        kontext_korb(z, &e);
        return;
    }
    let m = sys::maus(0);
    let wahl = kontextmenue((m >> 16) as i32, (m & 0xFFFF) as i32, &[t("Open", "Öffnen"), t("Copy", "Kopieren"), t("Cut", "Ausschneiden"), t("Rename", "Umbenennen"), t("Delete", "Löschen")]);
    let p = verbinden(&z.pfad, &e.name);
    match wahl {
        Some(0) => oeffnen(z, &e),
        Some(1) => {
            z.ablage = Some((vec![p], false));
            z.meldung = format!("{}: {}", t("Copied", "Kopiert"), e.name);
        }
        Some(2) => {
            z.ablage = Some((vec![p], true));
            z.meldung = format!("{}: {}", t("Cut", "Ausgeschnitten"), e.name);
        }
        Some(3) => {
            if let Some(neu) = name_dialog(t("Rename", "Umbenennen"), &e.name)
                && neu != e.name
            {
                z.meldung = if neu.contains('/') {
                    t("Name without /", "Name ohne /").to_string()
                } else if art(&verbinden(&z.pfad, &neu)).is_some() && neu.to_lowercase() != e.name.to_lowercase() {
                    format!("{neu} {}", t("already exists", "gibt es schon"))
                } else {
                    match datei::umbenennen(&p, &neu) {
                        datei::OK => format!("{}: {neu}", t("Renamed", "Umbenannt")),
                        c => format!("{}: {}", t("Rename", "Umbenennen"), fehlertext(c)),
                    }
                };
                z.neu_lesen();
            }
        }
        Some(4) => {
            let korb = papierkorb_von(&p).filter(|k| !liegt_in(&p, k) && !liegt_in(k, &p));
            if let Some(korb) = korb {
                // D4: in den Papierkorb (Rückfrage bleibt: langer Druck aus Versehen)
                let f = match kern::sprache::aktuell() {
                    kern::sprache::Sprache::Englisch => format!("Move \"{}\" to the recycle bin?", e.name),
                    kern::sprache::Sprache::Deutsch => format!("\"{}\" in den Papierkorb?", e.name),
                };
                if frage(t("Delete", "Löschen"), &f, t("Delete", "Löschen")) {
                    z.meldung = match in_papierkorb(&p, &korb) {
                        Ok(_) => format!("{}: {}", t("In the recycle bin", "Im Papierkorb"), e.name),
                        Err(c) => format!("{}: {}", t("Delete", "Löschen"), fehlertext(c)),
                    };
                    z.neu_lesen();
                }
                return;
            }
            let f = match (kern::sprache::aktuell(), e.ordner) {
                (kern::sprache::Sprache::Englisch, true) => format!("Delete folder \"{}\"?", e.name),
                (kern::sprache::Sprache::Englisch, false) => format!("Delete file \"{}\"?", e.name),
                (kern::sprache::Sprache::Deutsch, true) => format!("Ordner \"{}\" löschen?", e.name),
                (kern::sprache::Sprache::Deutsch, false) => format!("Datei \"{}\" löschen?", e.name),
            };
            if frage(t("Delete", "Löschen"), &f, t("Delete", "Löschen")) {
                z.meldung = match loeschen(&p) {
                    Ok(()) => format!("{}: {}", t("Deleted", "Gelöscht"), e.name),
                    Err(c) => format!("{}: {}", t("Delete", "Löschen"), fehlertext(c)),
                };
                z.neu_lesen();
            }
        }
        _ => {}
    }
}

fn neuer_ordner(z: &mut Zustand) {
    let vorschlag = freier_name(&z.pfad, t("New folder", "Neuer Ordner")).unwrap_or_default();
    if let Some(n) = name_dialog(t("New folder", "Neuer Ordner"), &vorschlag) {
        z.meldung = if n.contains('/') {
            t("Name without /", "Name ohne /").to_string()
        } else if art(&verbinden(&z.pfad, &n)).is_some() {
            format!("{n} gibt es schon")
        } else {
            match datei::ordner_erstellen(&verbinden(&z.pfad, &n)) {
                datei::OK => format!("{}: {n}", t("Created", "Angelegt")),
                c => format!("{}: {}", t("Create", "Anlegen"), fehlertext(c)),
            }
        };
        z.neu_lesen();
    }
}

/// Programm „dateimanager“ (Parameter: Startordner).
pub fn programm() {
    let geraete = geraete();
    let a = kern::argumente();
    let start = if !a.is_empty() && art(&a) == Some(true) {
        a
    } else {
        geraete.first().map_or_else(|| "/sys".to_string(), |g| g.pfad.clone())
    };
    let mut z = Zustand {
        pfad: String::new(),
        liste: Vec::new(),
        oben: 0,
        meldung: String::new(),
        ablage: None,
        geraete,
        symbole: Vec::new(),
        gemischt: Vec::new(),
        gezeichnet_oben: 0,
    };
    z.oeffne_ordner(start);
    sys::maske(0x27);
    loop {
        match sys::warte_ereignis() {
            1 => zeichnen(&mut z),
            3 => {
                let k = sys::knopf_id();
                let (id, rechts) = (k >> 8, k & 2 != 0);
                match id {
                    1 => sys::ende(),
                    K_HOCH => {
                        let e = eltern(&z.pfad);
                        // nicht über die Wurzel des Geräts hinaus
                        if z.geraete.iter().any(|g| liegt_in(&e, &g.pfad)) {
                            z.oeffne_ordner(e);
                        }
                    }
                    K_NEU => neuer_ordner(&mut z),
                    K_EINFUEGEN => einfuegen(&mut z),
                    K_KORB => korb_oeffnen(&mut z),
                    K_LEEREN => leeren(&mut z),
                    id if (K_GERAET..K_EINTRAG).contains(&id) => {
                        if let Some(g) = z.geraete.get((id - K_GERAET) as usize) {
                            let p = g.pfad.clone();
                            z.oeffne_ordner(p);
                        }
                    }
                    id if id >= K_EINTRAG => {
                        let i = (id - K_EINTRAG) as usize;
                        if rechts {
                            kontext(&mut z, i);
                        } else if let Some(e) = z.liste.get(i).cloned() {
                            oeffnen(&mut z, &e);
                        }
                    }
                    _ => {}
                }
                zeichnen(&mut z);
            }
            6 => {
                let s = sys::maus(7) as i16 as i32;
                if s != 0 {
                    let (_, h) = masse();
                    let max = z.liste.len().saturating_sub(sichtbar(h));
                    let neu = (z.oben as i32 + s.signum() * (s.abs() / 3).max(1)).clamp(0, max as i32) as usize;
                    if neu != z.oben {
                        z.oben = neu;
                        liste_blaettern(&mut z);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O4: Startet ein Programm nicht, nennt der Dateimanager den Grund
    /// (vorher hiess jeder Fehler „fehlt“, auch „zu wenig Speicher“).
    #[test]
    fn zwei_zeilen_bricht_am_leerzeichen() {
        let (a, b) = zwei_zeilen("Move \"notiz (Copy).txt\" to the recycle bin?", 42);
        assert_eq!(a, "Move \"notiz (Copy).txt\" to the recycle");
        assert_eq!(b, "bin?");
        assert_eq!(zwei_zeilen("kurz", 42), ("kurz".to_string(), String::new()));
    }

    #[test]
    fn startfehler_nennt_den_grund() {
        assert!(startfehler("bildbetrachter", -30).contains("Speicher") || startfehler("bildbetrachter", -30).contains("memory"));
        assert!(startfehler("bildbetrachter", -(datei::NICHT_GEFUNDEN as i32)).ends_with("fehlt") || startfehler("bildbetrachter", -5).ends_with("missing"));
        assert!(!startfehler("bildbetrachter", -30).contains("fehlt"));
        assert!(startfehler("x", -31).contains("31"));
    }
}
