//! Bildschirmfoto (P6, Auftrag 12.3): `@screenshot` – ausgelöst über das
//! Startmenü oder einen langen Druck auf die Uhr der Taskleiste. Wartet,
//! bis Menü/Kontextmenü weg sind, nimmt den ganzen Bildschirm auf und
//! speichert ihn als PNG unter `/sd0/1/Screenshots` (ohne SD-Karte:
//! `/rd/1/Screenshots`). Die Datei wird zur Kontrolle wieder gelesen und
//! dekodiert; danach meldet ein Dialog den Pfad.

use crate::shell::melden;
use kern::sprache::t;
use kern::{datei, sys};
use kolibri::dekoder::Rgba;

/// Zielordner: SD-Karte, wenn eingehängt, sonst intern.
pub fn ordner() -> &'static str {
    if datei::einhaengungen().iter().any(|(p, _)| p == "sd0/1") { "/sd0/1/Screenshots" } else { "/rd/1/Screenshots" }
}

/// Dateiname aus der Ortszeit: `ESP32-OS_2026-11-22_01-23-45.png`.
pub fn dateiname() -> String {
    let l = kern::zeit::lokal(kern::zeit::unix());
    let (y, m, d) = kern::zeit::datum_aus_tagen(l.div_euclid(86_400));
    let s = l.rem_euclid(86_400);
    format!("ESP32-OS_{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}.png", s / 3600, s / 60 % 60, s % 60)
}

/// Bildschirm aufnehmen, speichern, prüfen. Ergebnis: Pfad und Grösse.
pub fn aufnehmen() -> Result<(String, usize), String> {
    let (w, h, px) = sys::foto();
    let bild = Rgba { w: w as u32, h: h as u32, px: px.into_iter().map(|p| p | 0xFF00_0000).collect(), delay: 0 };
    let png = kolibri::kodierer::png(&bild);
    let o = ordner();
    let _ = datei::ordner_erstellen(o);
    let mut pfad = format!("{o}/{}", dateiname());
    // gleiche Sekunde (zweimal kurz hintereinander) → Zusatz
    let mut n = 2;
    while crate::dateimanager::art(&pfad).is_some() {
        pfad = format!("{o}/{}", dateiname().replace(".png", &format!("_{n}.png")));
        n += 1;
    }
    let (c, _) = datei::erstellen(&pfad, &png);
    if c != datei::OK {
        return Err(format!("{}: {}", pfad, crate::dateimanager::fehlertext(c)));
    }
    // Kontrolle: zurücklesen und dekodieren
    let mut zurueck = vec![0u8; png.len()];
    let (c, n) = datei::lesen(&pfad, 0, &mut zurueck);
    let ok = (c == datei::OK || c == datei::EOF)
        && n as usize == png.len()
        && kolibri::dekoder::dekodieren(&zurueck).is_ok_and(|(_, v)| v[0].w == bild.w && v[0].h == bild.h && v[0].px == bild.px);
    if !ok {
        return Err(format!("{pfad}: {}", t("check failed", "Prüfung fehlgeschlagen")));
    }
    Ok((pfad, png.len()))
}

/// Programm „@screenshot“.
pub fn programm() {
    // Menü bzw. Kontextmenü verschwinden lassen, dann aufnehmen
    sys::warten(60);
    // … und warten, bis das Bild ruhig steht (höchstens 1,5 s): unter Last
    // zeigte die Taskleiste sonst noch den gedrückten Startknopf
    let pruefsumme = || sys::foto().2.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &p| (h ^ u64::from(p)).wrapping_mul(0x100_0000_01b3));
    let mut alt = pruefsumme();
    for _ in 0..15 {
        sys::warten(10);
        let neu = pruefsumme();
        if neu == alt {
            break;
        }
        alt = neu;
    }
    match aufnehmen() {
        Ok((pfad, n)) => {
            sys::debug_text(&format!("Screenshot: {pfad} ({n} Bytes, gelesen und geprüft)\n"));
            melden(t("Screenshot", "Bildschirmfoto"), &format!("{}\n{pfad}", t("Saved:", "Gespeichert:")));
        }
        Err(f) => {
            sys::debug_text(&format!("Screenshot: Fehler {f}\n"));
            melden(t("Screenshot", "Bildschirmfoto"), &format!("{} {f}", t("Not saved:", "Nicht gespeichert:")));
        }
    }
}
