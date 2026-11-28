//! D3: App-Store. Katalog (Vorgabe des Nutzers 27.11.): die Spiele, PasswordGen,
//! TinyHashView und die laufenden KolibriOS-Programme; das Grundsystem ist
//! fest. Alle GPL-2.0 bzw. eigener Code.
//!
//! * **Mit eigenem Abbild** (Doom, Quake): Installieren lädt die Datei aus
//!   dem neuesten GitHub-Release (HTTPS mit Zertifikatsprüfung, SHA-256 aus
//!   `SHA256SUMS`) in ihre Partition, ohne den Startplatz zu ändern;
//!   Entfernen löscht den Kopf des Abbilds ([`kern::aktualisierung`]).
//! * **In die Firmware einkompiliert** (alle anderen): ein Download bräuchte
//!   einen Lader für Programmcode zur Laufzeit (gibt es nicht, siehe PLAN
//!   „D3 einkompilierte Programme“) – Installieren/Entfernen blendet sie in
//!   Startmenü und Desktop ein bzw. aus.
//!
//! Der Zustand „ausgeblendet“ ist eine Bitmaske über [`KATALOG`] (keine
//! Belegung, gespeichert in den Einstellungen als `hidden_apps`).

use core::sync::atomic::{AtomicU32, Ordering};
use kern::sys;

/// Ein Programm im App-Store.
pub struct Eintrag {
    pub programm: &'static str,
    pub titel: &'static str,
    pub titel_de: &'static str,
    /// (Partition, Datei im Release) bei Programmen mit eigenem Abbild
    pub abbild: Option<(&'static str, &'static str)>,
}

const fn e(programm: &'static str, titel: &'static str, titel_de: &'static str, abbild: Option<(&'static str, &'static str)>) -> Eintrag {
    Eintrag { programm, titel, titel_de, abbild }
}

pub const KATALOG: [Eintrag; 12] = [
    e("@spiel_doom", "Doom", "Doom", Some(("doomapp", "doomapp.bin"))),
    e("@spiel_quake", "Quake", "Quake", Some(("quakeapp", "quakeapp.bin"))),
    e("@spiel_pudding", "Pudding Panic", "Pudding-Panik", None),
    e("@spiel_wuerfelwelt", "Cube World", "Würfelwelt", None),
    e("passwordgen", "PasswordGen", "PasswordGen", None),
    e("thashview", "TinyHashView", "TinyHashView", None),
    e("basic_gui", "Example GUI", "Beispiel GUI", None),
    e("boxlib_demo", "Controls", "Steuerelemente", None),
    e("libimg_demo", "Image demo", "Bild-Demo", None),
    e("http_tcp_demo", "Network test", "Netzwerk-Test", None),
    e("netz_laden", "Download test", "Download-Test", None),
    e("matrix", "Matrix", "Matrix", None),
];

/// Bit i = KATALOG[i] ausgeblendet (entfernt).
static AUSGEBLENDET: AtomicU32 = AtomicU32::new(0);

fn index(programm: &str) -> Option<usize> {
    KATALOG.iter().position(|e| e.programm == programm)
}

/// Erscheint das Programm in Startmenü und Desktop? (Programme ausserhalb
/// des Katalogs immer.)
pub fn sichtbar(programm: &str) -> bool {
    index(programm).is_none_or(|i| AUSGEBLENDET.load(Ordering::Relaxed) & (1 << i) == 0)
}

/// Ein-/ausblenden; liefert den Wert für die Einstellungen.
pub fn setzen(programm: &str, sichtbar: bool) -> String {
    if let Some(i) = index(programm) {
        if sichtbar {
            AUSGEBLENDET.fetch_and(!(1 << i), Ordering::Relaxed);
        } else {
            AUSGEBLENDET.fetch_or(1 << i, Ordering::Relaxed);
        }
    }
    text()
}

/// Ausgeblendete Programme als Liste für die Einstellungen.
pub fn text() -> String {
    let m = AUSGEBLENDET.load(Ordering::Relaxed);
    KATALOG.iter().enumerate().filter(|(i, _)| m & (1 << i) != 0).map(|(_, e)| e.programm).collect::<Vec<_>>().join(",")
}

/// Aus den Einstellungen laden (unbekannte Namen werden übergangen).
pub fn laden(wert: &str) {
    let m = wert.split(',').filter_map(|n| index(n.trim())).fold(0u32, |m, i| m | 1 << i);
    AUSGEBLENDET.store(m, Ordering::Relaxed);
}

// ------------------------------------------------------------ Fenster

const FX: i32 = 10;
const FY: i32 = 6;
const FW: i32 = 620;
const FH: i32 = 300;
const LISTE_Y: i32 = 4;
const ZEILE_H: i32 = 40;
const ZEILEN: usize = 5;
const KNOPF_X: i32 = 404;
const KNOPF_W: i32 = 150;
const K_ZEILE: u32 = 0x100;
const K_HOCH: u32 = 0x50;
const K_RUNTER: u32 = 0x51;

fn text_bei(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

/// Installiert? Bei Abbildern der Blick in die Partition (unbekannt am PC:
/// wie „ausgeblendet“ sagt).
fn installiert(e: &Eintrag, abbilder: &[Option<bool>]) -> bool {
    let i = index(e.programm).unwrap_or(0);
    match e.abbild {
        Some(_) => abbilder.get(i).copied().flatten().unwrap_or(true) && sichtbar(e.programm),
        None => sichtbar(e.programm),
    }
}

/// Mitte des Knopfs „Installieren/Entfernen“ in Zeile `zeile` der sichtbaren
/// Liste (Bildschirmkoordinaten; Tests).
pub fn knopf_mitte(zeile: usize) -> Option<(i32, i32)> {
    kern::fenster::mit_welt(|w| {
        let skin = w.skin.clone();
        let f = w.slots.iter().flatten().filter_map(|s| s.fenster.as_ref()).find(|f| f.titel == "App-Store" || f.titel == "App store")?;
        let (ax, ay, _, _) = f.arbeitsbereich(&skin);
        Some((f.x + ax + KNOPF_X + KNOPF_W / 2, f.y + ay + LISTE_Y + zeile as i32 * ZEILE_H + ZEILE_H / 2))
    })
}

struct Lage {
    oben: usize,
    abbilder: Vec<Option<bool>>,
    status: Option<(String, u32)>,
    /// Abbild, dessen Entfernen noch bestätigt werden muss (zweites Tippen;
    /// danach ist ein neuer Download nötig).
    bestaetigen: Option<usize>,
}

fn zeichnen(l: &Lage) {
    let t = kern::sprache::t;
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xF4F4F4, 0, Some(t("App store", "App-Store")));
    for (z, e) in KATALOG.iter().skip(l.oben).take(ZEILEN).enumerate() {
        let y = LISTE_Y + z as i32 * ZEILE_H;
        sys::rechteck(4 << 16 | (FW - 60) as u32, (y as u32) << 16 | (ZEILE_H - 4) as u32, 0xFFFFFF);
        let name = t(e.titel, e.titel_de);
        text_bei(12, y + 4, name, 0x101010);
        let da = installiert(e, &l.abbilder);
        let art = match (e.abbild.is_some(), da) {
            (true, true) => t("installed (own image)", "installiert (eigenes Abbild)"),
            (true, false) => t("not installed - download", "nicht installiert - Download"),
            (false, true) => t("installed", "installiert"),
            (false, false) => t("removed", "entfernt"),
        };
        text_bei(12, y + 20, art, 0x707070);
        sys::knopf((KNOPF_X as u32) << 16 | (KNOPF_W - 1) as u32, ((y + 2) as u32) << 16 | (ZEILE_H - 9) as u32, K_ZEILE + (l.oben + z) as u32, if da { 0xC0C0C0 } else { 0x3A7BD5 });
        let k = if da { t("Remove", "Entfernen") } else { t("Install", "Installieren") };
        text_bei(KNOPF_X + (KNOPF_W - k.chars().count() as i32 * 8) / 2, y + 12, k, if da { 0x202020 } else { 0xFFFFFF });
    }
    let x = FW - 52;
    sys::knopf((x as u32) << 16 | 33, (LISTE_Y as u32) << 16 | 70, K_HOCH, 0xD8D8D8);
    text_bei(x + 13, LISTE_Y + 30, "^", 0x202020);
    let y2 = LISTE_Y + ZEILEN as i32 * ZEILE_H - 72;
    sys::knopf((x as u32) << 16 | 33, (y2 as u32) << 16 | 70, K_RUNTER, 0xD8D8D8);
    text_bei(x + 13, y2 + 30, "v", 0x202020);
    status_zeichnen(l);
    sys::neuzeichnen(2);
}

fn status_zeichnen(l: &Lage) {
    let y = LISTE_Y + ZEILEN as i32 * ZEILE_H + 6;
    sys::rechteck(4 << 16 | (FW - 20) as u32, (y as u32) << 16 | 20, 0xF4F4F4);
    match &l.status {
        Some((s, f)) => text_bei(8, y + 2, &s.chars().take(70).collect::<String>(), *f),
        None => text_bei(8, y + 2, kern::sprache::t("Core system is always installed.", "Das Grundsystem ist immer installiert."), 0x707070),
    }
}

fn speichern() {
    let _ = crate::konfig::ausgeblendet_speichern(&text());
}

/// Installieren bzw. Entfernen von KATALOG[i].
fn umschalten(l: &mut Lage, i: usize) {
    let t = kern::sprache::t;
    let e = &KATALOG[i];
    let name = t(e.titel, e.titel_de);
    let da = installiert(e, &l.abbilder);
    let bestaetigt = l.bestaetigen.take() == Some(i);
    match (e.abbild, da) {
        (Some(_), true) if !bestaetigt => {
            l.bestaetigen = Some(i);
            l.status = Some((format!("{name}: {}", t("tap Remove again to delete (download needed later)", "nochmals Entfernen tippen (danach neuer Download)")), 0xB06010));
        }
        (Some((platz, _)), true) => match kern::aktualisierung::abbild_entfernen(platz) {
            Ok(()) => {
                l.abbilder[i] = Some(false);
                setzen(e.programm, false);
                speichern();
                l.status = Some((format!("{name}: {}", t("removed", "entfernt")), 0x207020));
            }
            Err(f) => l.status = Some((format!("{name}: {f}"), 0xB03020)),
        },
        (Some((platz, datei)), false) => {
            l.status = Some((format!("{name}: {}", t("downloading...", "wird geladen...")), 0x303030));
            status_zeichnen(l);
            let mut fortschritt = |p: u32| {
                let y = LISTE_Y + ZEILEN as i32 * ZEILE_H + 6;
                sys::rechteck(4 << 16 | (FW - 20) as u32, (y as u32) << 16 | 20, 0xF4F4F4);
                text_bei(8, y + 2, &format!("{name}: {p} %"), 0x303030);
            };
            match kern::aktualisierung::abbild_installieren(platz, datei, &mut fortschritt) {
                Ok(()) => {
                    l.abbilder[i] = Some(true);
                    setzen(e.programm, true);
                    speichern();
                    l.status = Some((format!("{name}: {}", t("installed", "installiert")), 0x207020));
                }
                Err(f) => l.status = Some((format!("{name}: {f}"), 0xB03020)),
            }
        }
        (None, true) => {
            setzen(e.programm, false);
            speichern();
            l.status = Some((format!("{name}: {}", t("removed", "entfernt")), 0x207020));
        }
        (None, false) => {
            setzen(e.programm, true);
            speichern();
            l.status = Some((format!("{name}: {}", t("installed", "installiert")), 0x207020));
        }
    }
    // Startmenü und Desktop neu aufbauen
    kern::sys::alle_neu_zeichnen();
}

/// Programm „appstore“.
pub fn programm() {
    let abbilder = KATALOG.iter().map(|e| e.abbild.and_then(|(platz, _)| kern::aktualisierung::abbild_vorhanden(platz))).collect();
    let mut l = Lage { oben: 0, abbilder, status: None, bestaetigen: None };
    loop {
        match sys::warte_ereignis() {
            1 => zeichnen(&l),
            3 => {
                let id = sys::knopf_id() >> 8;
                match id {
                    1 => return,
                    K_HOCH => (l.oben, l.bestaetigen) = (l.oben.saturating_sub(ZEILEN - 1), None),
                    K_RUNTER => (l.oben, l.bestaetigen) = ((l.oben + ZEILEN - 1).min(KATALOG.len() - ZEILEN), None),
                    k if k >= K_ZEILE && ((k - K_ZEILE) as usize) < KATALOG.len() => umschalten(&mut l, (k - K_ZEILE) as usize),
                    _ => continue,
                }
                zeichnen(&l);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ausblenden_und_laden() {
        laden("");
        assert!(sichtbar("matrix") && sichtbar("@spiel_doom") && sichtbar("info"));
        assert_eq!(setzen("matrix", false), "matrix");
        assert!(!sichtbar("matrix"));
        assert_eq!(setzen("@spiel_doom", false), "@spiel_doom,matrix");
        laden("matrix, unbekannt ,thashview");
        assert!(!sichtbar("matrix") && !sichtbar("thashview") && sichtbar("@spiel_doom"));
        assert!(sichtbar("einstellungen"), "Grundsystem nie ausgeblendet");
        assert_eq!(setzen("info", false), "thashview,matrix", "nicht im Katalog");
        laden("");
    }
}
