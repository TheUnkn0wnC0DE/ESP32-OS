//! Dateisystem der KolibriOS-Funktionen 70/80 (und 30, aktueller Ordner).
//!
//! KolibriOS-Pfade (`/sys/…`, `/rd/1/…`, `/sd0/1/…`, relativ zum aktuellen
//! Ordner, `..`) werden auf eingehängte Ordner der Plattform abgebildet
//! (Board: interne FAT-Partition `/daten`, SD-Karte `/sdcard`; PC: beliebige
//! Ordner). `/`, `/rd`, `/sd0` sind virtuelle Ordner wie im Original.
//! Fehlercodes nach `sysfuncs.txt` („Error codes of the file system“).

use crate::prozess::{eigene_tid, kern_abschnitt, pruefe_ende};
use crate::schrift::{cp866_zu_uni, uni_zu_cp866};
use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const OK: u32 = 0;
pub const NICHT_UNTERSTUETZT: u32 = 2;
pub const NICHT_GEFUNDEN: u32 = 5;
pub const EOF: u32 = 6;
pub const VOLL: u32 = 8;
pub const FS_FEHLER: u32 = 9;
pub const VERWEIGERT: u32 = 10;
pub const GERAET: u32 = 11;

/// (KolibriOS-Präfix in Kleinbuchstaben ohne Schrägstrich, z. B. "sd0/1", Host-Ordner)
static EINHAENGUNGEN: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());
static ORDNER: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);

fn sperre<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// KolibriOS-Ordner `kolibri` (z. B. "/sd0/1") auf `host` abbilden.
pub fn einhaengen(kolibri: &str, host: impl Into<PathBuf>) {
    let host = host.into();
    kern_abschnitt(|| {
        let k = kolibri.trim_matches('/').to_ascii_lowercase();
        let host = host.clone();
        let mut e = sperre(&EINHAENGUNGEN);
        e.retain(|(p, _)| *p != k);
        e.push((k, host));
        // längere Präfixe zuerst
        e.sort_by_key(|(p, _)| std::cmp::Reverse(p.matches('/').count()));
    });
}

/// Plattform: (Gesamt, frei) in Bytes für einen Host-Pfad (Board: FAT-Info).
pub type SpeicherInfo = fn(&std::path::Path) -> Option<(u64, u64)>;
static SPEICHER_INFO: OnceLock<SpeicherInfo> = OnceLock::new();

/// Abfrage des Speicherplatzes festlegen (einmal beim Systemstart).
pub fn speicher_info_setzen(f: SpeicherInfo) {
    let _ = SPEICHER_INFO.set(f);
}

/// Speicherplatz (Gesamt, frei) der Einhängung `kolibri` (z. B. „/sd0/1“).
pub fn speicherplatz(kolibri: &str) -> Option<(u64, u64)> {
    let k = kolibri.trim_matches('/').to_ascii_lowercase();
    let host = kern_abschnitt(|| sperre(&EINHAENGUNGEN).iter().find(|(p, _)| *p == k).map(|(_, h)| h.clone()))?;
    SPEICHER_INFO.get().and_then(|f| f(&host))
}

/// Liste der Einhängungen (Dateimanager, Speicherplatz).
pub fn einhaengungen() -> Vec<(String, PathBuf)> {
    kern_abschnitt(|| sperre(&EINHAENGUNGEN).clone())
}

pub(crate) fn faden_weg(tid: u32) {
    if let Some(m) = sperre(&ORDNER).as_mut() {
        m.remove(&tid);
    }
}

/// Aktueller Ordner des Fadens (Standard "/sys").
pub fn aktueller_ordner() -> String {
    let tid = eigene_tid().unwrap_or(0);
    kern_abschnitt(|| sperre(&ORDNER).as_ref().and_then(|m| m.get(&tid).cloned()).unwrap_or_else(|| "/sys".into()))
}

/// Funktion 30.1/30.4: aktuellen Ordner setzen.
pub fn ordner_setzen(pfad: &str) {
    pruefe_ende();
    let tid = eigene_tid().unwrap_or(0);
    let neu = normalisieren(pfad, &aktueller_ordner());
    kern_abschnitt(|| {
        sperre(&ORDNER).get_or_insert_with(HashMap::new).insert(tid, format!("/{}", neu.join("/")));
    });
}

/// Pfadbytes nach Kodierung (0 = Kennbyte 1/2/3 am Anfang, sonst cp866)
/// in Text umsetzen; nullterminiert.
pub fn pfad_text(roh: &[u8], kodierung: u32) -> String {
    let ende16 = |b: &[u8]| b.chunks_exact(2).position(|c| c == [0, 0]).map_or(b.len(), |i| 2 * i);
    let (k, b) = match kodierung {
        1..=3 => (kodierung, roh),
        _ => match roh.first() {
            Some(&k @ 1..=3) => (u32::from(k), &roh[1..]),
            // Kennbyte nach dem ersten '/' (oder '//' davor)
            Some(b'/') if matches!(roh.get(1), Some(1..=3)) => {
                let k = u32::from(roh[1]);
                let mut v = vec![b'/'];
                v.extend_from_slice(&roh[2..]);
                return pfad_text(&v, k);
            }
            _ => (1, roh),
        },
    };
    match k {
        2 => {
            let n = ende16(b);
            String::from_utf16_lossy(&b[..n].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
        }
        3 => String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned(),
        _ => b.iter().take_while(|&&c| c != 0).map(|&c| char::from_u32(cp866_zu_uni(c)).unwrap_or('?')).collect(),
    }
}

/// Absoluter Pfad als Bestandteile ('.' und '..' aufgelöst).
pub fn normalisieren(pfad: &str, aktuell: &str) -> Vec<String> {
    let ganz = if pfad.starts_with('/') { pfad.to_string() } else { format!("{aktuell}/{pfad}") };
    let mut v: Vec<String> = Vec::new();
    for t in ganz.split('/') {
        match t {
            "" | "." => {}
            ".." => {
                v.pop();
            }
            t => v.push(t.into()),
        }
    }
    v
}

enum Ziel {
    Host(PathBuf, bool),
    Virtuell(Vec<String>),
}

fn aufloesen(pfad: &str) -> Result<Ziel, u32> {
    let teile = normalisieren(pfad, &aktueller_ordner());
    let klein: Vec<String> = teile.iter().map(|t| t.to_lowercase()).collect();
    let e = sperre(&EINHAENGUNGEN);
    for (p, host) in e.iter() {
        let pt: Vec<&str> = p.split('/').collect();
        if klein.len() >= pt.len() && klein[..pt.len()].iter().zip(&pt).all(|(a, b)| a == b) {
            let mut h = host.clone();
            for t in &teile[pt.len()..] {
                h.push(t);
            }
            return Ok(Ziel::Host(h, teile.len() == pt.len()));
        }
    }
    let mut namen: Vec<String> = Vec::new();
    for (p, _) in e.iter() {
        let pt: Vec<&str> = p.split('/').collect();
        if pt.len() > klein.len() && klein.iter().zip(&pt).all(|(a, b)| a == b) {
            let n = pt[klein.len()].to_string();
            if !namen.contains(&n) {
                namen.push(n);
            }
        }
    }
    if namen.is_empty() {
        return Err(NICHT_GEFUNDEN);
    }
    namen.sort();
    Ok(Ziel::Virtuell(namen))
}

fn fehler(e: &std::io::Error) -> u32 {
    match e.kind() {
        ErrorKind::NotFound => NICHT_GEFUNDEN,
        ErrorKind::PermissionDenied | ErrorKind::DirectoryNotEmpty | ErrorKind::IsADirectory => VERWEIGERT,
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => VOLL,
        ErrorKind::Unsupported => NICHT_UNTERSTUETZT,
        _ => FS_FEHLER,
    }
}

fn host(pfad: &str) -> Result<(PathBuf, bool), u32> {
    match aufloesen(pfad)? {
        Ziel::Host(h, wurzel) => Ok((h, wurzel)),
        Ziel::Virtuell(_) => Err(VERWEIGERT),
    }
}

/// Unterfunktion 0: Datei ab `offset` in `buf` lesen → (Code, gelesen).
pub fn lesen(pfad: &str, offset: u64, buf: &mut [u8]) -> (u32, u32) {
    pruefe_ende();
    kern_abschnitt(|| {
        let (h, _) = match host(pfad) {
            Ok(h) => h,
            Err(c) => return (c, 0),
        };
        if h.is_dir() {
            return (VERWEIGERT, 0);
        }
        let mut f = match fs::File::open(&h) {
            Ok(f) => f,
            Err(e) => return (fehler(&e), 0),
        };
        if let Err(e) = f.seek(SeekFrom::Start(offset)) {
            return (fehler(&e), 0);
        }
        let mut n = 0;
        while n < buf.len() {
            match f.read(&mut buf[n..]) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(e) => return (fehler(&e), n as u32),
            }
        }
        (if n < buf.len() { EOF } else { OK }, n as u32)
    })
}

fn zeit_bytes(t: Option<std::time::SystemTime>) -> ([u8; 4], [u8; 4]) {
    let Some(t) = t else { return ([0; 4], [0; 4]) };
    let s = t.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let l = crate::zeit::lokal(s);
    let (y, m, d) = crate::zeit::datum_aus_tagen(l.div_euclid(86_400));
    let sek = l.rem_euclid(86_400);
    let yb = (y as u16).to_le_bytes();
    ([(sek % 60) as u8, (sek / 60 % 60) as u8, (sek / 3600) as u8, 0], [d as u8, m as u8, yb[0], yb[1]])
}

/// BDFE-Eintrag schreiben; liefert seine Länge (304 cp866, sonst 560).
fn bdfe(out: &mut [u8], attr: u32, kod: u32, meta: Option<&fs::Metadata>, groesse: u64, name: Option<&str>) -> usize {
    let lang = if matches!(kod, 2 | 3) { 560 } else { 304 };
    let n = lang.min(out.len());
    out[..n].fill(0);
    if n < 40 {
        return n;
    }
    out[0..4].copy_from_slice(&attr.to_le_bytes());
    out[4..8].copy_from_slice(&kod.to_le_bytes());
    let (ez, ed) = zeit_bytes(meta.and_then(|m| m.created().ok()));
    let (az, ad) = zeit_bytes(meta.and_then(|m| m.accessed().ok()));
    let (mz, md) = zeit_bytes(meta.and_then(|m| m.modified().ok()));
    for (o, b) in [(8, ez), (12, ed), (16, az), (20, ad), (24, mz), (28, md)] {
        out[o..o + 4].copy_from_slice(&b);
    }
    out[32..40].copy_from_slice(&groesse.to_le_bytes());
    if let Some(name) = name {
        let z = &mut out[40..n];
        match kod {
            2 => {
                for (i, c) in name.encode_utf16().take(z.len() / 2 - 1).enumerate() {
                    z[2 * i..2 * i + 2].copy_from_slice(&c.to_le_bytes());
                }
            }
            3 => {
                let b = name.as_bytes();
                let k = b.len().min(z.len() - 1);
                z[..k].copy_from_slice(&b[..k]);
            }
            _ => {
                for (i, c) in name.chars().take(z.len() - 1).enumerate() {
                    z[i] = uni_zu_cp866(c);
                }
            }
        }
    }
    lang
}

fn attribute(m: &fs::Metadata) -> u32 {
    let mut a = 0x20;
    if m.is_dir() {
        a |= 0x10;
    }
    if m.permissions().readonly() {
        a |= 1;
    }
    a
}

/// Unterfunktion 1: Ordner lesen → (Code, Einträge). `buf` erhält 32 Byte
/// Kopf und BDFE-Einträge ab Index `start`.
pub fn ordner_lesen(pfad: &str, start: u32, anzahl: u32, kod: u32, buf: &mut [u8]) -> (u32, u32) {
    pruefe_ende();
    kern_abschnitt(|| {
        // (Name, Attribute, Grösse, Metadaten)
        let mut eintraege: Vec<(String, u32, u64, Option<fs::Metadata>)> = Vec::new();
        match aufloesen(pfad) {
            Err(c) => return (c, 0),
            Ok(Ziel::Virtuell(namen)) => {
                for n in namen {
                    eintraege.push((n, 0x10, 0, None));
                }
            }
            Ok(Ziel::Host(h, wurzel)) => {
                let rd = match fs::read_dir(&h) {
                    Ok(r) => r,
                    Err(e) => return (if h.is_file() { VERWEIGERT } else { fehler(&e) }, 0),
                };
                if !wurzel {
                    eintraege.push((".".into(), 0x10, 0, None));
                    eintraege.push(("..".into(), 0x10, 0, None));
                }
                let mut v: Vec<_> = rd
                    .flatten()
                    .filter_map(|e| {
                        let m = e.metadata().ok()?;
                        Some((e.file_name().to_string_lossy().into_owned(), attribute(&m), if m.is_dir() { 0 } else { m.len() }, Some(m)))
                    })
                    .collect();
                v.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                eintraege.extend(v);
            }
        }
        if buf.len() < 32 {
            return (FS_FEHLER, 0);
        }
        buf[..32].fill(0);
        let gesamt = eintraege.len() as u32;
        let mut o = 32;
        let mut n = 0u32;
        for (name, attr, gr, meta) in eintraege.iter().skip(start as usize).take(anzahl as usize) {
            let lang = if matches!(kod, 2 | 3) { 560 } else { 304 };
            if o + lang > buf.len() {
                break;
            }
            bdfe(&mut buf[o..], *attr, kod, meta.as_ref(), *gr, Some(name));
            o += lang;
            n += 1;
        }
        buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        buf[4..8].copy_from_slice(&n.to_le_bytes());
        buf[8..12].copy_from_slice(&gesamt.to_le_bytes());
        (if n < anzahl { EOF } else { OK }, n)
    })
}

/// Unterfunktion 2: Datei anlegen/überschreiben → (Code, geschrieben).
pub fn erstellen(pfad: &str, daten: &[u8]) -> (u32, u32) {
    pruefe_ende();
    kern_abschnitt(|| {
        let (h, _) = match host(pfad) {
            Ok(h) => h,
            Err(c) => return (c, 0),
        };
        match fs::File::create(&h).and_then(|mut f| f.write_all(daten)) {
            Ok(()) => (OK, daten.len() as u32),
            Err(e) => (fehler(&e), 0),
        }
    })
}

/// Unterfunktion 3: in bestehende Datei ab `offset` schreiben (Lücken mit 0).
pub fn schreiben(pfad: &str, offset: u64, daten: &[u8]) -> (u32, u32) {
    pruefe_ende();
    kern_abschnitt(|| {
        let (h, _) = match host(pfad) {
            Ok(h) => h,
            Err(c) => return (c, 0),
        };
        if !h.is_file() {
            return (NICHT_GEFUNDEN, 0);
        }
        let r = fs::OpenOptions::new().write(true).open(&h).and_then(|mut f| {
            if f.metadata()?.len() < offset {
                f.set_len(offset)?;
            }
            f.seek(SeekFrom::Start(offset))?;
            f.write_all(daten)
        });
        match r {
            Ok(()) => (OK, daten.len() as u32),
            Err(e) => (fehler(&e), 0),
        }
    })
}

/// Unterfunktion 4: Dateigrösse setzen.
pub fn groesse_setzen(pfad: &str, groesse: u64) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| match host(pfad) {
        Err(c) => c,
        Ok((h, _)) => match fs::OpenOptions::new().write(true).open(&h).and_then(|f| f.set_len(groesse)) {
            Ok(()) => OK,
            Err(e) => fehler(&e),
        },
    })
}

/// Unterfunktion 5: Information (BDFE ohne Namen, 40 Byte).
pub fn info(pfad: &str, buf: &mut [u8]) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| match aufloesen(pfad) {
        Err(c) => c,
        Ok(Ziel::Virtuell(_)) => {
            bdfe(buf, 0x10, 0, None, 0, None);
            OK
        }
        Ok(Ziel::Host(h, _)) => match fs::metadata(&h) {
            Ok(m) => {
                let mut tmp = [0u8; 304];
                bdfe(&mut tmp, attribute(&m), 0, Some(&m), if m.is_dir() { 0 } else { m.len() }, None);
                let n = buf.len().min(40);
                buf[..n].copy_from_slice(&tmp[..n]);
                OK
            }
            Err(e) => fehler(&e),
        },
    })
}

/// Unterfunktion 6: Attribute setzen (Schreibschutz-Bit 0, Änderungszeit
/// aus +24/+28); Ordner-/Kennungs-Bits bleiben.
pub fn attribute_setzen(pfad: &str, attr: &[u8]) -> u32 {
    pruefe_ende();
    if attr.len() < 32 {
        return FS_FEHLER;
    }
    kern_abschnitt(|| {
        let (h, wurzel) = match host(pfad) {
            Ok(h) => h,
            Err(c) => return c,
        };
        if wurzel {
            return VERWEIGERT;
        }
        let r = (|| -> std::io::Result<()> {
            let mut p = fs::metadata(&h)?.permissions();
            p.set_readonly(attr[0] & 1 != 0);
            fs::set_permissions(&h, p)?;
            let (z, d) = (&attr[24..28], &attr[28..32]);
            let jahr = u16::from_le_bytes([d[2], d[3]]);
            if jahr >= 1970 && h.is_file() {
                let tage = crate::zeit::tage_aus_datum(i64::from(jahr), u32::from(d[1]), u32::from(d[0]));
                let lokal = tage * 86_400 + i64::from(z[2]) * 3600 + i64::from(z[1]) * 60 + i64::from(z[0]);
                let utc = lokal - (crate::zeit::lokal(lokal) - lokal);
                let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(utc.max(0) as u64);
                fs::OpenOptions::new().write(true).open(&h)?.set_modified(t)?;
            }
            Ok(())
        })();
        r.map_or_else(|e| fehler(&e), |()| OK)
    })
}

/// Unterfunktion 7: Programm starten. Der Dateiname (ohne Endung, Gross/Klein
/// egal) wählt das eingelinkte Programm; Parameter abrufbar mit
/// [`crate::prozess::argumente`]. Liefert PID oder −Fehlercode.
pub fn starten(pfad: &str, parameter: &str) -> i32 {
    pruefe_ende();
    let name = pfad.rsplit('/').next().unwrap_or("");
    let name = name.split('.').next().unwrap_or("").to_lowercase();
    let e = crate::prozess::programme().iter().find(|e| e.name.to_lowercase() == name);
    match e {
        None => -(NICHT_GEFUNDEN as i32),
        Some(e) => match crate::prozess::starte_mit(e, parameter) {
            Ok(pid) => pid as i32,
            Err(crate::prozess::StartFehler::KeinSpeicher) => -30,
            Err(crate::prozess::StartFehler::ZuVieleProgramme) => -32,
            Err(_) => -31,
        },
    }
}

/// Unterfunktion 8: Datei oder leeren Ordner löschen.
pub fn loeschen(pfad: &str) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| match host(pfad) {
        Err(c) => c,
        Ok((_, true)) => VERWEIGERT,
        Ok((h, _)) => {
            let r = if h.is_dir() { fs::remove_dir(&h) } else { fs::remove_file(&h) };
            match r {
                Ok(()) => OK,
                Err(e) if h.is_dir() && e.kind() != ErrorKind::NotFound => VERWEIGERT,
                Err(e) => fehler(&e),
            }
        }
    })
}

/// Unterfunktion 9: Ordner anlegen (vorhanden = Erfolg).
pub fn ordner_erstellen(pfad: &str) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| match host(pfad) {
        Err(c) => c,
        Ok((h, _)) if h.is_dir() => OK,
        Ok((h, _)) => match fs::create_dir(&h) {
            Ok(()) => OK,
            Err(e) => fehler(&e),
        },
    })
}

/// Unterfunktion 10: umbenennen/verschieben. `neu` relativ = zum Elternordner
/// des Ziels, absolut = zur Wurzel der Partition.
pub fn umbenennen(pfad: &str, neu: &str) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| {
        let teile = normalisieren(pfad, &aktueller_ordner());
        let (h, wurzel) = match host(pfad) {
            Ok(h) => h,
            Err(c) => return c,
        };
        if wurzel {
            return VERWEIGERT;
        }
        // Wurzel der Partition = Präfix der Einhängung
        let e = sperre(&EINHAENGUNGEN);
        let klein: Vec<String> = teile.iter().map(|t| t.to_lowercase()).collect();
        let Some((p, basis)) = e.iter().find(|(p, _)| {
            let pt: Vec<&str> = p.split('/').collect();
            klein.len() >= pt.len() && klein[..pt.len()].iter().zip(&pt).all(|(a, b)| a == b)
        }) else {
            return NICHT_GEFUNDEN;
        };
        let tiefe = p.split('/').count();
        let ziel_teile = if neu.starts_with('/') {
            normalisieren(neu, "/")
        } else {
            let mut eltern = teile[tiefe..teile.len() - 1].to_vec();
            eltern.extend(normalisieren(neu, "/"));
            normalisieren(&eltern.join("/"), "/")
        };
        let mut z = basis.clone();
        for t in &ziel_teile {
            z.push(t);
        }
        drop(e);
        match fs::rename(&h, &z) {
            Ok(()) => OK,
            Err(e) => fehler(&e),
        }
    })
}

/// Unterfunktionen 11/12 (symbolische Verknüpfungen): Die abgebildeten
/// Dateisysteme sind FAT – wie KolibriOS' FAT-Treiber Code 2.
pub fn verknuepfung() -> u32 {
    pruefe_ende();
    NICHT_UNTERSTUETZT
}
