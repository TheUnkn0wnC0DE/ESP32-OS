//! WLAN-Liste (W3) für die Einstellungen und die Ersteinrichtung: Zustand
//! oben, darunter die gefundenen Netze (Signalbalken, Name, Sicherheit,
//! Wi-Fi 6, gemerkt). Tippen auf ein Netz verbindet: gemerkte Netze sofort,
//! sonst nach dem Passwort-Dialog (Bildschirmtastatur). Nach Erfolg wird
//! das Netz gemerkt (W4); „×“ vergisst es. Die Knöpfe liegen ab `basis`.

use kern::sprache::t;
use kern::sys;
use kern::wlan::{self, Netz};

/// Knopf-Versätze ab `basis`.
pub const SUCHEN: u32 = 0;
pub const TRENNEN: u32 = 1;
pub const HOCH: u32 = 2;
pub const RUNTER: u32 = 3;
pub const ZEILE: u32 = 10;
pub const VERGESSEN: u32 = 30;
/// Zeilenhöhe der Netzliste.
pub const ZEILE_H: i32 = 40;

const BLAU: u32 = 0x3A7AC0;

/// Farbe der Meldung.
pub fn meldung_farbe(l: &Liste) -> u32 {
    if l.meldung_ok { 0x2A7A3A } else { 0xB03030 }
}

/// Zustand der Liste in einem Programm.
#[derive(Default)]
pub struct Liste {
    pub netze: Vec<Netz>,
    /// Erste sichtbare Zeile (Blättern).
    pub erste: usize,
    pub meldung: String,
    /// Meldung ist ein Erfolg (grün statt rot).
    pub meldung_ok: bool,
    /// Mindestens einmal gesucht.
    pub gesucht: bool,
}

fn text(x: i32, y: i32, s: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, farbe: u32, beschriftung: &str, tf: u32) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, farbe);
    let n = beschriftung.chars().count() as i32 * 8;
    text(x + (w - n) / 2, y + (h - 16) / 2, beschriftung, tf);
}

/// Signalbalken (4 Stück, 18 px hoch) bei (x, y).
pub fn balken_zeichnen(x: i32, y: i32, rssi: i32, an: u32, aus: u32) {
    let b = wlan::balken(rssi);
    for i in 0..4u8 {
        let h = 6 + i32::from(i) * 4;
        sys::rechteck(((x + i32::from(i) * 6) as u32) << 16 | 4, ((y + 18 - h) as u32) << 16 | h as u32, if i < b { an } else { aus });
    }
}

/// Zustandszeile.
pub fn zustand_text() -> String {
    if !wlan::vorhanden() {
        return t("Wi-Fi not available", "WLAN nicht verfügbar").to_string();
    }
    let z = wlan::zustand();
    if z.verbunden {
        let ip = z.ip;
        format!(
            "{} · {}.{}.{}.{} · {} dBm{}",
            t("Connected", "Verbunden"),
            ip[0],
            ip[1],
            ip[2],
            ip[3],
            z.rssi,
            if z.he { " · Wi-Fi 6" } else { "" }
        )
    } else if z.verbindet {
        t("Connecting...", "Verbinde...").to_string()
    } else if !z.bereit {
        t("Wi-Fi is starting...", "WLAN startet...").to_string()
    } else {
        t("Not connected", "Nicht verbunden").to_string()
    }
}

/// Liste zeichnen: Kopfknöpfe rechts oben (`kopf_y`), Zustand bei `y0`,
/// darunter `zeilen` Netze, Breite 630 (Arbeitsbereich).
pub fn zeichnen(l: &Liste, kopf_y: i32, y0: i32, zeilen: usize, basis: u32) {
    let z = wlan::zustand();
    if wlan::vorhanden() {
        knopf(basis + SUCHEN, 476, kopf_y, 146, 42, 0xD8D8D8, t("Search", "Suchen"), 0);
        if z.verbunden || z.verbindet {
            knopf(basis + TRENNEN, 322, kopf_y, 146, 42, 0xD8D8D8, t("Disconnect", "Trennen"), 0);
        }
    }
    text(12, y0, &zustand_text(), if z.verbunden { 0x2A7A3A } else { 0x404040 });
    let bekannt = wlan::bekannte();
    let mit = wlan::verbunden_mit();
    let ly = y0 + 22;
    for k in 0..zeilen {
        let y = ly + k as i32 * ZEILE_H;
        let Some(n) = l.netze.get(l.erste + k) else { break };
        let ist = mit.as_deref() == Some(n.ssid.as_str());
        let hg = if ist { 0xD6E6F6 } else { 0xFFFFFF };
        sys::knopf(8 << 16 | 563, (y as u32) << 16 | (ZEILE_H - 5) as u32, basis + ZEILE + k as u32, hg);
        balken_zeichnen(18, y + 9, n.rssi, if ist { BLAU } else { 0x404040 }, 0xC8C8C8);
        let name: String = wlan::anzeigename(&n.ssid).chars().take(26).collect();
        text(52, y + 11, &name, if n.sicherheit.unterstuetzt() { 0x101010 } else { 0x909090 });
        let mut info = n.sicherheit.name().to_string();
        if n.ax {
            info.push_str(" · Wi-Fi 6");
        }
        text(270, y + 11, &info, 0x606060);
        if ist {
            text(430, y + 11, t("connected", "verbunden"), BLAU);
        } else if bekannt.contains(&n.ssid) {
            text(430, y + 11, t("saved", "gemerkt"), 0x606060);
        }
        if bekannt.contains(&n.ssid) {
            knopf(basis + VERGESSEN + k as u32, 528, y + 2, 40, ZEILE_H - 9, 0xE8D0D0, "x", 0x802020);
        }
    }
    if l.netze.len() > zeilen {
        let h = zeilen as i32 * ZEILE_H / 2 - 4;
        knopf(basis + HOCH, 578, ly, 44, h, 0xD8D8D8, "^", 0);
        knopf(basis + RUNTER, 578, ly + h + 4, 44, h, 0xD8D8D8, "v", 0);
    }
    if l.gesucht && l.netze.is_empty() && wlan::vorhanden() {
        text(12, ly + 12, t("No networks found.", "Keine Netze gefunden."), 0x808080);
    }
}

/// Suchen (blockiert einige Sekunden; vorher „Suche...“ anzeigen).
pub fn suchen(l: &mut Liste, neu: &mut dyn FnMut(&Liste)) {
    l.meldung = t("Searching for networks...", "Suche Netze...").to_string();
    neu(l);
    match wlan::suchen() {
        Ok(n) => {
            l.netze = n;
            l.erste = 0;
            l.meldung.clear();
            l.meldung_ok = false;
        }
        Err(e) => l.meldung = format!("{} ({e})", t("Search failed", "Suche fehlgeschlagen")),
    }
    l.gesucht = true;
}

/// Auf das Ergebnis eines Verbindungsauftrags warten (höchstens 25 s),
/// dabei neu zeichnen. Schliessen (Knopf 1) beendet das Programm.
fn warten(l: &mut Liste, name: &str, neu: &mut dyn FnMut(&Liste)) {
    l.meldung = format!("{} {name}...", t("Connecting to", "Verbinde mit"));
    neu(l);
    for _ in 0..250 {
        match sys::warte_ereignis_max(10) {
            1 => neu(l),
            3 if sys::knopf_id() >> 8 == 1 => sys::ende(),
            _ => {}
        }
        let z = wlan::zustand();
        if z.verbunden {
            l.meldung = format!("{} {name}", t("Connected to", "Verbunden mit"));
            l.meldung_ok = true;
            return;
        }
        if !z.verbindet {
            l.meldung = format!("{}: {}", t("Not connected", "Nicht verbunden"), wlan::grund_text(z.grund));
            return;
        }
    }
    l.meldung = t("Connection timed out", "Zeitüberschreitung beim Verbinden").to_string();
}

/// Knopf behandeln (`true` = gehörte zur Liste).
pub fn knopf_behandeln(l: &mut Liste, id: u32, basis: u32, zeilen: usize, neu: &mut dyn FnMut(&Liste)) -> bool {
    let Some(r) = id.checked_sub(basis) else { return false };
    l.meldung_ok = false;
    match r {
        SUCHEN => suchen(l, neu),
        TRENNEN => {
            wlan::trennen();
            l.meldung = t("Disconnected", "Getrennt").to_string();
            l.meldung_ok = true;
        }
        HOCH => l.erste = l.erste.saturating_sub(zeilen),
        RUNTER => {
            if l.erste + zeilen < l.netze.len() {
                l.erste += zeilen;
            }
        }
        r if (ZEILE..ZEILE + zeilen as u32).contains(&r) => {
            let Some(n) = l.netze.get(l.erste + (r - ZEILE) as usize).cloned() else { return true };
            let name = wlan::anzeigename(&n.ssid);
            if !n.sicherheit.unterstuetzt() {
                l.meldung = format!("{name}: {} ({})", t("not supported", "nicht unterstützt"), n.sicherheit.name());
                return true;
            }
            if wlan::verbunden_mit().as_deref() == Some(n.ssid.as_str()) {
                l.meldung = format!("{} {name}", t("Already connected to", "Schon verbunden mit"));
                l.meldung_ok = true;
                return true;
            }
            let passwort = if let Some(p) = wlan::passwort_von(&n.ssid) {
                p
            } else if n.sicherheit.braucht_passwort() {
                let titel = format!("{} {name}", t("Password for", "Passwort für"));
                let vorgabe = wlan::test_passwort(&n.ssid).unwrap_or_default();
                match crate::dateimanager::passwort_dialog(&titel, &vorgabe) {
                    Some(p) => p,
                    None => {
                        neu(l);
                        return true;
                    }
                }
            } else {
                String::new()
            };
            match wlan::verbinden(&n.ssid, &passwort, true) {
                Ok(()) => warten(l, &name, neu),
                Err(e) => l.meldung = format!("{} ({e})", t("Could not connect", "Verbinden nicht möglich")),
            }
        }
        r if (VERGESSEN..VERGESSEN + zeilen as u32).contains(&r) => {
            if let Some(n) = l.netze.get(l.erste + (r - VERGESSEN) as usize).cloned() {
                let ok = wlan::vergessen(&n.ssid);
                l.meldung_ok = ok;
                l.meldung = if ok {
                    format!("{}: {}", wlan::anzeigename(&n.ssid), t("forgotten", "vergessen"))
                } else {
                    t("Could not save", "Nicht gespeichert").to_string()
                };
            }
        }
        _ => return false,
    }
    true
}
