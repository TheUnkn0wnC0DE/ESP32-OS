//! libini nativ (KolibriOS `develop/libraries/libs-dev/libini`): INI-Dateien
//! über die Dateifunktionen des Kerns (KolibriOS-Pfade). Sektionen und
//! Schlüssel ohne Rücksicht auf Gross-/Kleinschreibung; beim Schreiben bleibt
//! der Rest der Datei (Kommentare, Reihenfolge) erhalten.
//!
//! Rust-Schnittstelle und C-Exporte mit den Original-Namen (`ini_get_str` …).
//!
//! Absturzsicher (V2, Auftrag 13.7). Ursachen des Verlusts in der Nacht
//! 22.11.: (1) Schreiben war nicht atomar (Datei gekürzt, dann geschrieben –
//! ein Absturz dazwischen hinterliess eine leere Datei); (2) zwei Schreiber
//! (Programm „Einstellungen“ und die Uhr-Sicherung der Firmware) arbeiteten
//! ohne Sperre auf derselben Datei und derselben Zwischendatei; (3) war die
//! Datei kurz nicht lesbar, schrieb `schreiben` eine Datei nur mit dem neuen
//! Schlüssel – alle anderen Einstellungen waren weg.
//!
//! Jetzt:
//! * Eine Sperre für das ganze Lesen–Ändern–Schreiben (im Kern-Abschnitt:
//!   ein zwangsweise beendetes Programm kann sie nie halten).
//! * Letzte Zeile: Kommentar `; esp32os-pruefsumme=<crc32> <länge>` (andere
//!   ini-Leser überspringen Kommentare). Eine Fassung gilt nur, wenn sie
//!   passt.
//! * Speichern: `<datei>.neu` schreiben und gegenlesen; alte Sicherung
//!   `<datei>.bak` löschen; `<datei>` → `<datei>.bak`; `.neu` → `<datei>`.
//!   Die vorige Fassung bleibt als Sicherungskopie.
//! * Laden: die erste gültige Fassung aus `<datei>`, `.neu`, `.bak`; ältere
//!   Dateien ohne Prüfsumme gelten, wenn nicht leer. Gibt es Dateien, aber
//!   keine gültige, schreibt `schreiben` nichts (statt Werte zu verlieren).

use kern::datei;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Sperre für alle Schreibvorgänge (siehe Moduldoku).
static SPERRE: Mutex<()> = Mutex::new(());

const PRUEF: &str = "; esp32os-pruefsumme=";

/// CRC-32 (IEEE, wie zlib).
fn crc32(d: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in d {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Inhalt mit Prüfsummen-Zeile.
fn mit_pruefsumme(inhalt: &str) -> String {
    format!("{inhalt}{PRUEF}{:08x} {}\r\n", crc32(inhalt.as_bytes()), inhalt.len())
}

/// Gültigkeit einer gelesenen Fassung: Some(inhalt ohne Prüfzeile) oder
/// None (beschädigt). Fassungen ohne Prüfzeile (älter) gelten, wenn nicht
/// leer.
fn pruefen(text: &str) -> Option<String> {
    match text.rfind(PRUEF) {
        Some(i) => {
            let (inhalt, rest) = text.split_at(i);
            let rest = rest[PRUEF.len()..].trim_end();
            let (c, l) = rest.split_once(' ')?;
            (u32::from_str_radix(c, 16).ok()? == crc32(inhalt.as_bytes()) && l.parse::<usize>().ok()? == inhalt.len())
                .then(|| inhalt.to_string())
        }
        None => (!text.trim().is_empty()).then(|| text.to_string()),
    }
}

/// Datei roh laden.
fn laden_roh(datei_pfad: &str) -> Option<String> {
    let mut info = [0u8; 40];
    if datei::info(datei_pfad, &mut info) != 0 {
        return None;
    }
    let n = u64::from_le_bytes(info[32..40].try_into().ok()?) as usize;
    let mut buf = vec![0u8; n];
    let (c, k) = datei::lesen(datei_pfad, 0, &mut buf);
    if c != 0 && c != datei::EOF {
        return None;
    }
    buf.truncate(k as usize);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Ergebnis des Ladens: gültiger Inhalt, gar keine Datei, oder nur
/// beschädigte Fassungen.
enum Stand {
    Gueltig(String),
    Keine,
    Beschaedigt,
}

fn stand(datei_pfad: &str) -> Stand {
    let mut irgendeine = false;
    for kandidat in [datei_pfad.to_string(), format!("{datei_pfad}.neu"), format!("{datei_pfad}.bak")] {
        if let Some(t) = laden_roh(&kandidat) {
            irgendeine = true;
            if let Some(g) = pruefen(&t) {
                return Stand::Gueltig(g);
            }
        }
    }
    if irgendeine { Stand::Beschaedigt } else { Stand::Keine }
}

fn laden(datei_pfad: &str) -> Option<String> {
    match stand(datei_pfad) {
        Stand::Gueltig(t) => Some(t),
        _ => None,
    }
}

/// Prüf-Haken (V2): wird nach jedem Schritt des Speicherns mit der
/// Schrittnummer gerufen. Tests stellen damit einen Stromausfall nach (PC:
/// Abbruch per Panik, Board-Testbuild: Neustart).
static HAKEN: AtomicUsize = AtomicUsize::new(0);

/// Haken setzen (nur Tests / Test-Builds).
pub fn stromausfall_haken(f: fn(usize)) {
    HAKEN.store(f as usize, Ordering::Release);
}

fn schritt(n: usize) {
    let h = HAKEN.load(Ordering::Acquire);
    if h != 0 {
        // SAFETY: nur aus `stromausfall_haken` gesetzt (fn(usize)).
        let f: fn(usize) = unsafe { core::mem::transmute::<usize, fn(usize)>(h) };
        f(n);
    }
}

/// Schritte des Speicherns (für den Prüf-Haken).
pub const SCHRITTE: usize = 6;

/// Datei absturzsicher ersetzen (siehe Moduldoku). Schritte: 1 `.neu` halb
/// geschrieben, 2 ganz, 3 gegengelesen, 4 alte `.bak` gelöscht,
/// 5 Datei → `.bak`, 6 `.neu` → Datei.
fn sicher_speichern(datei_pfad: &str, inhalt: &str) -> bool {
    let daten = mit_pruefsumme(inhalt);
    let neu = format!("{datei_pfad}.neu");
    let bak = format!("{datei_pfad}.bak");
    let name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let b = daten.as_bytes();
    let haelfte = b.len() / 2;
    if datei::erstellen(&neu, &b[..haelfte]).0 != 0 {
        return false;
    }
    schritt(1);
    if datei::schreiben(&neu, haelfte as u64, &b[haelfte..]).0 != 0 {
        return false;
    }
    schritt(2);
    // Gegenprobe: vollständig und gültig geschrieben?
    if laden_roh(&neu).as_deref() != Some(daten.as_str()) {
        return false;
    }
    schritt(3);
    // FAT überschreibt beim Umbenennen nicht: Platz für die Sicherung –
    // nur, wenn eine Datei da ist, die an ihre Stelle rückt (nach einem
    // Abbruch zwischen 5 und 6 bleibt die ältere Sicherung erhalten)
    let da = laden_roh(datei_pfad).is_some();
    if da {
        let _ = datei::loeschen(&bak);
    }
    schritt(4);
    if da && datei::umbenennen(datei_pfad, &name(&bak)) != 0 {
        return false;
    }
    schritt(5);
    let ok = datei::umbenennen(&neu, &name(datei_pfad)) == 0;
    schritt(6);
    ok
}

/// Lesen–Ändern–Schreiben unter der Sperre. `f` bekommt den gültigen Inhalt
/// (leer, wenn es noch keine Datei gibt) und liefert den neuen oder None.
fn aendern(datei_pfad: &str, f: impl FnOnce(&str) -> Option<String>) -> bool {
    kern::kern_abschnitt(|| {
        let _g = SPERRE.lock().unwrap_or_else(|e| e.into_inner());
        let alt = match stand(datei_pfad) {
            Stand::Gueltig(t) => t,
            Stand::Keine => String::new(),
            // nichts überschreiben, was vielleicht noch zu retten ist
            Stand::Beschaedigt => return false,
        };
        match f(&alt) {
            Some(neu) => sicher_speichern(datei_pfad, &neu),
            None => false,
        }
    })
}

fn sektion_von(zeile: &str) -> Option<&str> {
    let t = zeile.trim();
    t.strip_prefix('[')?.strip_suffix(']').map(str::trim)
}

fn eintrag_von(zeile: &str) -> Option<(&str, &str)> {
    let t = zeile.trim();
    if t.starts_with(';') || t.starts_with('#') || t.starts_with('[') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    Some((k.trim(), v.trim()))
}

/// Wert lesen.
pub fn lesen(datei_pfad: &str, sektion: &str, schluessel: &str) -> Option<String> {
    let text = laden(datei_pfad)?;
    let mut drin = false;
    for z in text.lines() {
        if let Some(s) = sektion_von(z) {
            drin = s.eq_ignore_ascii_case(sektion);
        } else if drin && let Some((k, v)) = eintrag_von(z) && k.eq_ignore_ascii_case(schluessel) {
            return Some(v.to_string());
        }
    }
    None
}

/// Wert schreiben (Datei/Sektion werden bei Bedarf angelegt).
pub fn schreiben(datei_pfad: &str, sektion: &str, schluessel: &str, wert: &str) -> bool {
    aendern(datei_pfad, |text| Some(wert_setzen(text, sektion, schluessel, wert)))
}

fn wert_setzen(text: &str, sektion: &str, schluessel: &str, wert: &str) -> String {
    let mut zeilen: Vec<String> = text.lines().map(String::from).collect();
    let mut drin = false;
    let mut sek_ende = None;
    let mut gesetzt = false;
    for i in 0..zeilen.len() {
        if let Some(s) = sektion_von(&zeilen[i]) {
            if drin {
                break;
            }
            drin = s.eq_ignore_ascii_case(sektion);
            if drin {
                sek_ende = Some(i + 1);
            }
        } else if drin {
            if let Some((k, _)) = eintrag_von(&zeilen[i])
                && k.eq_ignore_ascii_case(schluessel)
            {
                zeilen[i] = format!("{schluessel}={wert}");
                gesetzt = true;
                break;
            }
            if !zeilen[i].trim().is_empty() {
                sek_ende = Some(i + 1);
            }
        }
    }
    if !gesetzt {
        match sek_ende {
            Some(i) => zeilen.insert(i, format!("{schluessel}={wert}")),
            None => {
                if zeilen.last().is_some_and(|z| !z.trim().is_empty()) {
                    zeilen.push(String::new());
                }
                zeilen.push(format!("[{sektion}]"));
                zeilen.push(format!("{schluessel}={wert}"));
            }
        }
    }
    let mut out = zeilen.join("\r\n");
    out.push_str("\r\n");
    out
}

/// Ganzzahl lesen (Standard bei Fehlen oder Unlesbarkeit).
pub fn zahl(datei_pfad: &str, sektion: &str, schluessel: &str, standard: i32) -> i32 {
    lesen(datei_pfad, sektion, schluessel).and_then(|v| v.parse().ok()).unwrap_or(standard)
}

pub fn zahl_setzen(datei_pfad: &str, sektion: &str, schluessel: &str, wert: i32) -> bool {
    schreiben(datei_pfad, sektion, schluessel, &wert.to_string())
}

/// Farbe "r,g,b" (oder 0xRRGGBB) lesen.
pub fn farbe(datei_pfad: &str, sektion: &str, schluessel: &str, standard: u32) -> u32 {
    let Some(v) = lesen(datei_pfad, sektion, schluessel) else { return standard };
    if let Some(h) = v.strip_prefix("0x").or_else(|| v.strip_prefix('#')) {
        return u32::from_str_radix(h, 16).unwrap_or(standard);
    }
    let t: Vec<u32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if t.len() == 3 { (t[0] & 255) << 16 | (t[1] & 255) << 8 | (t[2] & 255) } else { standard }
}

pub fn farbe_setzen(datei_pfad: &str, sektion: &str, schluessel: &str, c: u32) -> bool {
    schreiben(datei_pfad, sektion, schluessel, &format!("{},{},{}", c >> 16 & 255, c >> 8 & 255, c & 255))
}

/// Alle Sektionen.
pub fn sektionen(datei_pfad: &str) -> Vec<String> {
    laden(datei_pfad).map_or_else(Vec::new, |t| t.lines().filter_map(sektion_von).map(String::from).collect())
}

/// Alle Einträge einer Sektion.
pub fn eintraege(datei_pfad: &str, sektion: &str) -> Vec<(String, String)> {
    let Some(text) = laden(datei_pfad) else { return Vec::new() };
    let mut drin = false;
    let mut v = Vec::new();
    for z in text.lines() {
        if let Some(s) = sektion_von(z) {
            drin = s.eq_ignore_ascii_case(sektion);
        } else if drin && let Some((k, w)) = eintrag_von(z) {
            v.push((k.to_string(), w.to_string()));
        }
    }
    v
}

/// Sektion samt Einträgen löschen.
pub fn sektion_loeschen(datei_pfad: &str, sektion: &str) -> bool {
    aendern(datei_pfad, |text| {
        let mut drin = false;
        let mut gefunden = false;
        let rest: Vec<&str> = text
            .lines()
            .filter(|z| {
                if let Some(s) = sektion_von(z) {
                    drin = s.eq_ignore_ascii_case(sektion);
                    gefunden |= drin;
                }
                !drin
            })
            .collect();
        let mut out = rest.join("
");
        out.push_str("
");
        gefunden.then_some(out)
    })
}

// ------------------------------------------------------------------ C-ABI

#[cfg(target_pointer_width = "32")]
mod c {
    use super::*;
    use core::ffi::c_char;

    unsafe fn s(p: *const c_char) -> String {
        if p.is_null() {
            return String::new();
        }
        // SAFETY: nullterminierter String des Programms.
        let b = unsafe { core::ffi::CStr::from_ptr(p) }.to_bytes();
        datei::pfad_text(b, 0)
    }

    unsafe fn kopieren(ziel: *mut u8, n: u32, text: &str) {
        if ziel.is_null() || n == 0 {
            return;
        }
        let b: Vec<u8> = text.chars().map(kern::schrift::uni_zu_cp866).collect();
        let k = b.len().min(n as usize - 1);
        // SAFETY: Puffer der Länge n vom Programm.
        unsafe {
            core::ptr::copy_nonoverlapping(b.as_ptr(), ziel, k);
            *ziel.add(k) = 0;
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_get_str(
        f: *const c_char,
        sek: *const c_char,
        key: *const c_char,
        buf: *mut u8,
        len: u32,
        def: *const c_char,
    ) -> i32 {
        unsafe {
            match lesen(&s(f), &s(sek), &s(key)) {
                Some(v) => {
                    kopieren(buf, len, &v);
                    0
                }
                None => {
                    kopieren(buf, len, &s(def));
                    -1
                }
            }
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_set_str(f: *const c_char, sek: *const c_char, key: *const c_char, buf: *const u8, len: u32) -> i32 {
        // SAFETY: len Bytes vom Programm.
        let w = unsafe { core::slice::from_raw_parts(buf, len as usize) };
        let w = datei::pfad_text(w, 0);
        if unsafe { schreiben(&s(f), &s(sek), &s(key), &w) } { 0 } else { -1 }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_get_int(f: *const c_char, sek: *const c_char, key: *const c_char, def: i32) -> i32 {
        unsafe { zahl(&s(f), &s(sek), &s(key), def) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_set_int(f: *const c_char, sek: *const c_char, key: *const c_char, v: i32) -> i32 {
        if unsafe { zahl_setzen(&s(f), &s(sek), &s(key), v) } { 0 } else { -1 }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_get_color(f: *const c_char, sek: *const c_char, key: *const c_char, def: u32) -> u32 {
        unsafe { farbe(&s(f), &s(sek), &s(key), def) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_set_color(f: *const c_char, sek: *const c_char, key: *const c_char, v: u32) -> i32 {
        if unsafe { farbe_setzen(&s(f), &s(sek), &s(key), v) } { 0 } else { -1 }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_del_section(f: *const c_char, sek: *const c_char) -> i32 {
        if unsafe { sektion_loeschen(&s(f), &s(sek)) } { 0 } else { -1 }
    }

    type SekRueckruf = unsafe extern "system" fn(*const c_char, *const c_char) -> i32;
    type KeyRueckruf = unsafe extern "system" fn(*const c_char, *const c_char, *const c_char, *const c_char) -> i32;

    fn cstr(t: &str) -> Vec<u8> {
        t.chars().map(kern::schrift::uni_zu_cp866).chain([0]).collect()
    }

    /// Ruft `cb(datei, sektion)` je Sektion; Rückgabe 0 beendet.
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_enum_sections(f: *const c_char, cb: SekRueckruf) -> i32 {
        for sek in sektionen(&unsafe { s(f) }) {
            let c = cstr(&sek);
            if unsafe { cb(f, c.as_ptr().cast()) } == 0 {
                break;
            }
        }
        0
    }

    /// Ruft `cb(datei, sektion, schlüssel, wert)` je Eintrag; Rückgabe 0 beendet.
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn ini_enum_keys(f: *const c_char, sek: *const c_char, cb: KeyRueckruf) -> i32 {
        for (k, v) in eintraege(&unsafe { s(f) }, &unsafe { s(sek) }) {
            let (ck, cv) = (cstr(&k), cstr(&v));
            if unsafe { cb(f, sek, ck.as_ptr().cast(), cv.as_ptr().cast()) } == 0 {
                break;
            }
        }
        0
    }
}
