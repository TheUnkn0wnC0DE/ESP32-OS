//! Einstellungen des Systems in `/sys/settings/esp32os.ini` (bleiben nach dem
//! Neustart erhalten): Sprache und Tastaturlayout (Auftrag 12.1),
//! Helligkeit, Lautstärke, Touchpad-Modus, Hintergrund (P3) und die zuletzt
//! bekannte Uhrzeit (Rückfall, wenn die Uhr nach dem Ausschalten bei 1970
//! steht). Gelesen beim Start, geschrieben beim Ändern.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use kern::sprache::{self, Sprache};
use kolibri::libini;
use std::sync::Mutex;

pub const DATEI: &str = "/sys/settings/esp32os.ini";
const ORDNER: &str = "/sys/settings";
const SEKTION: &str = "system";

/// Helligkeit 1..=10 (Anzeigeschleife liest sie über `Spiel::helligkeit`).
pub static HELLIGKEIT: AtomicU8 = AtomicU8::new(8);
/// Lautstärke 0..=10 (Desktop schickt sie an den Synth).
pub static LAUTSTAERKE: AtomicU8 = AtomicU8::new(6);
/// Probeton nach dem Ändern der Lautstärke.
pub static TON_PROBE: AtomicBool = AtomicBool::new(false);
/// Angeforderte Signaltöne der Uhr (Wecker/Timer, A3); der Desktop schickt
/// sie an den Synth.
pub static SIGNAL: AtomicU32 = AtomicU32::new(0);
/// Wecker (A3): Minute des Tages (0..1440), Bit 16 = an. Standard 07:00 aus.
pub static WECKER: AtomicU32 = AtomicU32::new(7 * 60);
/// Sekunden ohne Eingabe bis Bildschirm aus (0 = nie).
pub static BILDSCHIRM_AUS: AtomicU32 = AtomicU32::new(120);
/// Sekunden nach „Bildschirm aus“ bis zum Ruhezustand (0 = nie).
pub static RUHE_NACH: AtomicU32 = AtomicU32::new(300);
/// Ruhezustand sofort (Startmenü).
pub static RUHE_JETZT: AtomicBool = AtomicBool::new(false);

/// Entwicklung: ein Autopilot spielt Berührungen ein (Desktop::autopilot).
/// Dann startet kein Spiel – ein Tipp, der wegen eines fehlenden Fensters
/// auf ein Desktop-Symbol fällt, würde den Messlauf in ein Spiel umleiten
/// (Dauertest M4: Pudding-Panik).
pub static AUTOPILOT_AKTIV: AtomicBool = AtomicBool::new(false);

/// Auswahl „Bildschirm aus nach“ (Sekunden, 0 = nie).
pub const AUS_WAHL: [u32; 6] = [30, 60, 120, 300, 600, 0];
/// Auswahl „Ruhezustand nach“ (Sekunden nach dem Abschalten, 0 = nie).
pub const RUHE_WAHL: [u32; 5] = [60, 300, 900, 1800, 0];

/// Touchpad-Modus beim Laden nicht einschalten (Autopilot-Läufe tippen
/// direkt; die gespeicherte Einstellung bleibt).
pub static TOUCHPAD_GESPERRT: AtomicBool = AtomicBool::new(false);

/// Akkustand in Prozent von der Firmware; 255 = nicht messbar.
pub static AKKU: AtomicU8 = AtomicU8::new(255);

/// Hintergrund des Desktops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hintergrund {
    /// Senkrechter Verlauf aus [`VERLAEUFE`].
    Verlauf(usize),
    /// Bilddatei (KolibriOS-Pfad), auf den Bildschirm skaliert.
    Bild(String),
}

/// Verläufe: (englisch, deutsch, oben, unten).
pub const VERLAEUFE: [(&str, &str, u32, u32); 4] = [
    ("Blue", "Blau", 0x3A6EA5, 0x12304F),
    ("Green", "Grün", 0x3F8F5A, 0x123F25),
    ("Grey", "Grau", 0x707A84, 0x2A3038),
    ("Purple", "Violett", 0x7A4FA0, 0x2A1640),
];

/// Liegt im System-Heap (überlebt das Programm, das ihn setzt).
static HINTERGRUND: Mutex<Option<Hintergrund>> = Mutex::new(None);

/// 2020-01-01: davor gilt die Uhr als nicht gestellt.
pub const UHR_GUELTIG_AB: i64 = 1_577_836_800;

fn lesen(schluessel: &str) -> Option<String> {
    libini::lesen(DATEI, SEKTION, schluessel)
}

fn zahl(schluessel: &str) -> Option<i64> {
    lesen(schluessel)?.trim().parse().ok()
}

/// D3: im App-Store entfernte Programme speichern.
pub fn ausgeblendet_speichern(wert: &str) -> bool {
    speichern("hidden_apps", wert)
}

fn speichern(schluessel: &str, wert: &str) -> bool {
    let _ = kern::datei::ordner_erstellen(ORDNER);
    libini::schreiben(DATEI, SEKTION, schluessel, wert)
}

/// Q2: Ort der Wetter-App (Name, Breite, Länge), `None` ohne Auswahl.
pub fn wetter_ort() -> Option<crate::wetter::Ort> {
    let name = lesen("wetter_ort")?;
    let lat = lesen("wetter_lat")?.trim().parse().ok()?;
    let lon = lesen("wetter_lon")?.trim().parse().ok()?;
    Some(crate::wetter::Ort { name, lat, lon })
}

/// Q2: Ort der Wetter-App speichern.
pub fn wetter_ort_setzen(o: &crate::wetter::Ort) -> bool {
    speichern("wetter_ort", &o.name) && speichern("wetter_lat", &format!("{:.4}", o.lat)) && speichern("wetter_lon", &format!("{:.4}", o.lon))
}

/// Gespeicherte Einstellungen übernehmen (Standard: Englisch, Layout CH,
/// Helligkeit 8, Lautstärke 6, Touchpad aus, blauer Verlauf).
pub fn laden() {
    // D3: im App-Store entfernte Programme
    crate::appstore::laden(&lesen("hidden_apps").unwrap_or_default());
    // Entwicklungs-Builds (`ESP32OS_SPRACHE=en`, z. B. Pressefotos): feste
    // Sprache, die gespeicherte Einstellung bleibt unverändert
    let s = option_env!("ESP32OS_SPRACHE")
        .and_then(sprache::aus_kuerzel)
        .or_else(|| lesen("language").and_then(|k| sprache::aus_kuerzel(&k)))
        .unwrap_or(Sprache::Englisch);
    sprache::setzen(s);
    let l = lesen("keyboard").and_then(|k| crate::tastatur::Layout::aus_kuerzel(&k)).unwrap_or(crate::tastatur::Layout::Ch);
    crate::tastatur::layout_setzen(l);
    if let Some(h) = zahl("brightness") {
        HELLIGKEIT.store(h.clamp(1, 10) as u8, Ordering::Relaxed);
    }
    if let Some(v) = zahl("volume") {
        LAUTSTAERKE.store(v.clamp(0, 10) as u8, Ordering::Relaxed);
    }
    if let Some(s) = zahl("screen_off") {
        BILDSCHIRM_AUS.store(s.clamp(0, 86_400) as u32, Ordering::Relaxed);
    }
    if let Some(s) = zahl("sleep_after") {
        RUHE_NACH.store(s.clamp(0, 86_400) as u32, Ordering::Relaxed);
    }
    // Wecker „HH:MM“ und an/aus (A3)
    if let Some((h, m)) = lesen("alarm").and_then(|w| {
        let (h, m) = w.trim().split_once(':')?;
        Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?))
    }) && h < 24
        && m < 60
    {
        let an = zahl("alarm_on").is_some_and(|a| a != 0);
        WECKER.store(h * 60 + m | u32::from(an) << 16, Ordering::Relaxed);
    }
    // Internetzeit (W3b), Standard an
    kern::zeit::ntp_an_setzen(zahl("ntp").is_none_or(|n| n != 0));
    if let Some(t) = zahl("touchpad") {
        kern::sys::eingabe::touchpad_setzen(t != 0 && !TOUCHPAD_GESPERRT.load(Ordering::Relaxed));
    }
    let hg = match lesen("wallpaper") {
        Some(w) if w.starts_with('/') => Hintergrund::Bild(w),
        Some(w) => Hintergrund::Verlauf(w.trim().parse::<usize>().unwrap_or(0).min(VERLAEUFE.len() - 1)),
        None => Hintergrund::Verlauf(0),
    };
    kern::kern_abschnitt(|| *HINTERGRUND.lock().unwrap_or_else(|e| e.into_inner()) = Some(hg));
    // Uhr nach dem Ausschalten (1970) → zuletzt gesicherte Zeit
    if kern::zeit::unix() < UHR_GUELTIG_AB
        && let Some(t) = zahl("clock")
        && t >= UHR_GUELTIG_AB
    {
        kern::zeit::unix_setzen(t);
    }
}

/// Internetzeit (NTP) ein/aus und speichern.
pub fn ntp_setzen(an: bool) -> bool {
    kern::zeit::ntp_an_setzen(an);
    speichern("ntp", if an { "1" } else { "0" })
}

/// Sprache umschalten, speichern, alle Fenster neu zeichnen.
pub fn sprache_setzen(s: Sprache) -> bool {
    sprache::setzen(s);
    let ok = speichern("language", sprache::kuerzel(s));
    kern::sys::alle_neu_zeichnen();
    ok
}

/// Tastaturlayout umschalten und speichern.
pub fn layout_setzen(l: crate::tastatur::Layout) -> bool {
    crate::tastatur::layout_setzen(l);
    speichern("keyboard", l.kuerzel())
}

pub fn helligkeit_setzen(h: u8) -> bool {
    let h = h.clamp(1, 10);
    HELLIGKEIT.store(h, Ordering::Relaxed);
    speichern("brightness", &h.to_string())
}

pub fn lautstaerke_setzen(v: u8) -> bool {
    let v = v.min(10);
    LAUTSTAERKE.store(v, Ordering::Relaxed);
    TON_PROBE.store(true, Ordering::Relaxed);
    speichern("volume", &v.to_string())
}

/// Wecker stellen (Minute des Tages) und speichern.
pub fn wecker_setzen(minute: u32, an: bool) -> bool {
    let minute = minute % (24 * 60);
    WECKER.store(minute | u32::from(an) << 16, Ordering::Relaxed);
    speichern("alarm", &format!("{:02}:{:02}", minute / 60, minute % 60)) && speichern("alarm_on", if an { "1" } else { "0" })
}

pub fn bildschirm_aus_setzen(s: u32) -> bool {
    BILDSCHIRM_AUS.store(s, Ordering::Relaxed);
    speichern("screen_off", &s.to_string())
}

pub fn ruhe_nach_setzen(s: u32) -> bool {
    RUHE_NACH.store(s, Ordering::Relaxed);
    speichern("sleep_after", &s.to_string())
}

pub fn touchpad_setzen(an: bool) -> bool {
    kern::sys::eingabe::touchpad_setzen(an);
    speichern("touchpad", if an { "1" } else { "0" })
}

pub fn hintergrund() -> Hintergrund {
    // Nur im Kern-Abschnitt sperren (pthread-Objekt im System-Heap; ein
    // zwangsweise angehaltenes Programm hält die Sperre nie)
    // Kopie im Heap des Aufrufers (M8, wie shell::symbol)
    crate::shell::sperre_vorbereiten(&HINTERGRUND);
    kern::geschuetzt(|| HINTERGRUND.lock().unwrap_or_else(|e| e.into_inner()).clone()).unwrap_or(Hintergrund::Verlauf(0))
}

/// Vorbereitetes Hintergrundbild: halbe Bildschirmgrösse in RGB565 (Kopf:
/// Breite, Höhe als u32 LE). 320×180 = 115 KB – die interne Partition hat
/// nur 1 MB (bis B0); `@icon` vergrössert es beim Zeichnen.
pub const HG_DATEI: &str = "/sys/settings/wallpaper.rgb";

/// Bild füllend auf halbe Bildschirmgrösse bringen und speichern.
pub fn hintergrund_vorbereiten(b: &kolibri::dekoder::Rgba, w: u32, h: u32) -> bool {
    let (w, h) = (w / 2, h / 2);
    let g = crate::shell::fuellend(b, w, h);
    let mut d = Vec::with_capacity(8 + (w * h * 2) as usize);
    d.extend_from_slice(&w.to_le_bytes());
    d.extend_from_slice(&h.to_le_bytes());
    for p in &g.px {
        d.extend_from_slice(&kern::grafik::zu565(*p).to_le_bytes());
    }
    let _ = kern::datei::ordner_erstellen(ORDNER);
    kern::datei::erstellen(HG_DATEI, &d).0 == kern::datei::OK
}

/// Hintergrundbild in Bildschirmgrösse, RGB565 (wie der Fensterpuffer).
pub struct Bild565 {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u16>,
}

/// Vorbereitetes Hintergrundbild lesen und auf w×h vergrössern (bilinear,
/// gleiche Rechnung wie `libimg::skalieren`). M6: direkt in RGB565, ohne
/// RGBA-Zwischenbilder – vorher hielt `@icon` 800 KB RGBA (mit dem kleinen
/// RGBA-Bild beim Vergrössern über 1 MB), jetzt 400 KB.
pub fn hintergrund_lesen(w: u32, h: u32) -> Option<Bild565> {
    let (bw, bh) = (w / 2, h / 2);
    let mut d = vec![0u8; 8 + (bw * bh * 2) as usize];
    let (c, n) = kern::datei::lesen(HG_DATEI, 0, &mut d);
    if (c != kern::datei::OK && c != kern::datei::EOF) || n as usize != d.len() {
        return None;
    }
    if u32::from_le_bytes(d[0..4].try_into().ok()?) != bw || u32::from_le_bytes(d[4..8].try_into().ok()?) != bh {
        return None;
    }
    let klein = |x: u32, y: u32| {
        let i = 8 + 2 * (y * bw + x) as usize;
        kern::grafik::von565(u16::from_le_bytes([d[i], d[i + 1]]))
    };
    let (w, h) = (w.max(1), h.max(1));
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            if bw < 2 || bh < 2 {
                px.push(kern::grafik::zu565(klein(x * bw / w, y * bh / h)));
                continue;
            }
            let fx = ((x as f32 + 0.5) * bw as f32 / w as f32 - 0.5).clamp(0.0, (bw - 1) as f32);
            let fy = ((y as f32 + 0.5) * bh as f32 / h as f32 - 0.5).clamp(0.0, (bh - 1) as f32);
            let (x0, y0) = (fx as u32, fy as u32);
            let (x1, y1) = ((x0 + 1).min(bw - 1), (y0 + 1).min(bh - 1));
            let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
            let (a, b, c, e) = (klein(x0, y0), klein(x1, y0), klein(x0, y1), klein(x1, y1));
            let mut v = 0u32;
            for s in 0..3 {
                let k = |q: u32| (q >> (8 * s) & 255) as f32;
                let o = k(a) * (1.0 - ax) + k(b) * ax;
                let u = k(c) * (1.0 - ax) + k(e) * ax;
                v |= ((o * (1.0 - ay) + u * ay).round() as u32 & 255) << (8 * s);
            }
            px.push(kern::grafik::zu565(v));
        }
    }
    Some(Bild565 { w, h, px })
}

/// Hintergrund setzen, speichern, Desktop neu zeichnen.
pub fn hintergrund_setzen(h: Hintergrund) -> bool {
    let wert = match &h {
        Hintergrund::Verlauf(i) => i.to_string(),
        Hintergrund::Bild(p) => p.clone(),
    };
    kern::kern_abschnitt(|| *HINTERGRUND.lock().unwrap_or_else(|e| e.into_inner()) = Some(h.clone()));
    let ok = speichern("wallpaper", &wert);
    kern::sys::alle_neu_zeichnen();
    ok
}

/// Ist die Ersteinrichtung (P4) schon abgeschlossen?
pub fn einrichtung_fertig() -> bool {
    zahl("setup") == Some(1)
}

/// Ersteinrichtung zeigen? Nur wenn `/sys` eingehängt ist (sonst lässt sich
/// nichts speichern) und sie noch nicht abgeschlossen wurde.
pub fn einrichtung_noetig() -> bool {
    kern::datei::einhaengungen().iter().any(|(p, _)| p == "sys") && !einrichtung_fertig()
}

/// Ersteinrichtung abschliessen (nicht mehr beim Start zeigen).
pub fn einrichtung_abschliessen() -> bool {
    speichern("setup", "1")
}

/// Uhr stellen (UTC) und die Zeit als Rückfall sichern.
pub fn uhr_setzen(utc: i64) -> bool {
    kern::zeit::unix_setzen(utc);
    uhr_sichern()
}

/// Aktuelle Zeit sichern (nur wenn gestellt).
pub fn uhr_sichern() -> bool {
    let t = kern::zeit::unix();
    t >= UHR_GUELTIG_AB && speichern("clock", &t.to_string())
}
