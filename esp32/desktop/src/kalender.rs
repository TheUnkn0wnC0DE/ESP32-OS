//! A4: Kalender (Auftrag Abschnitt 5) – Monatsansicht, Woche ab Montag,
//! heute hervorgehoben, Wochenende grau. Monat wechseln: nach links/oben
//! wischen = nächster Monat, nach rechts/unten = voriger (ein Wechsel je
//! Wischbewegung), oder die Knöpfe „<“ und „>“; der Monatsname springt
//! zurück auf heute.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use kern::sys;

// Fenster (Arbeitsbereich-Koordinaten)
pub const FX: i32 = 120;
pub const FY: i32 = 4;
pub const FW: i32 = 400;
pub const FH: i32 = 306;
const IW: i32 = FW - 10;

pub const K_ZURUECK: u32 = 2;
pub const K_VOR: u32 = 3;
pub const K_HEUTE: u32 = 4;

/// Kopfzeile, Wochentage, Raster (6 Wochen × 7 Tage)
const KOPF_Y: i32 = 6;
const KOPF_H: i32 = 36;
const TAGE_Y: i32 = 48;
pub const RASTER_X: i32 = 6;
pub const RASTER_Y: i32 = 70;
pub const ZELLE_W: i32 = 54;
pub const ZELLE_H: i32 = 30;

/// Angezeigter Monat (Jahr · 12 + Monat − 1) – für Tests und das Board-Log.
static ANGEZEIGT: AtomicU32 = AtomicU32::new(0);

/// Zuletzt angezeigter Monat (Jahr, Monat 1..=12); (0, 0) vor dem ersten Bild.
pub fn monat_zuletzt() -> (i64, u32) {
    let a = ANGEZEIGT.load(Relaxed);
    if a == 0 { (0, 0) } else { (i64::from(a / 12), a % 12 + 1) }
}

/// Lage eines Knopfs (x, y, Breite, Höhe; Arbeitsbereich).
pub fn knopf_lage(k: u32) -> (i32, i32, i32, i32) {
    match k {
        K_ZURUECK => (6, KOPF_Y, 64, KOPF_H),
        K_VOR => (IW - 70, KOPF_Y, 64, KOPF_H),
        K_HEUTE => (76, KOPF_Y, IW - 152, KOPF_H),
        _ => (0, 0, 0, 0),
    }
}

/// Mitte eines Knopfs (Arbeitsbereich) – für Tests und den Autopiloten.
pub fn knopf_mitte(k: u32) -> (i32, i32) {
    let (x, y, w, h) = knopf_lage(k);
    (x + w / 2, y + h / 2)
}

const MONATE: [(&str, &str); 12] = [
    ("January", "Januar"),
    ("February", "Februar"),
    ("March", "März"),
    ("April", "April"),
    ("May", "Mai"),
    ("June", "Juni"),
    ("July", "Juli"),
    ("August", "August"),
    ("September", "September"),
    ("October", "Oktober"),
    ("November", "November"),
    ("December", "Dezember"),
];

const WOCHENTAGE: [(&str, &str); 7] = [("Mo", "Mo"), ("Tu", "Di"), ("We", "Mi"), ("Th", "Do"), ("Fr", "Fr"), ("Sa", "Sa"), ("Su", "So")];

/// Tage im Monat.
pub fn tage_im_monat(j: i64, m: u32) -> u32 {
    let (nj, nm) = if m == 12 { (j + 1, 1) } else { (j, m + 1) };
    (kern::zeit::tage_aus_datum(nj, nm, 1) - kern::zeit::tage_aus_datum(j, m, 1)) as u32
}

/// Spalte des Ersten (0 = Montag).
pub fn erster_spalte(j: i64, m: u32) -> u32 {
    (kern::zeit::wochentag(kern::zeit::tage_aus_datum(j, m, 1)) + 6) % 7
}

/// Monat verschieben.
fn verschieben(j: i64, m: u32, d: i32) -> (i64, u32) {
    let i = j * 12 + i64::from(m) - 1 + i64::from(d);
    (i.div_euclid(12), i.rem_euclid(12) as u32 + 1)
}

/// Heute (lokal); vor dem Stellen der Uhr: None.
fn heute() -> Option<(i64, u32, u32)> {
    let t = kern::zeit::unix();
    (t >= crate::konfig::UHR_GUELTIG_AB).then(|| kern::zeit::datum_aus_tagen(kern::zeit::lokal(t).div_euclid(86_400)))
}

fn text(x: i32, y: i32, s: &str, farbe: u32, mal: u32) {
    let f = 0x3000_0000 | (mal.clamp(1, 8) - 1) << 24;
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, f | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

fn text_mittig(x: i32, w: i32, y: i32, s: &str, farbe: u32, mal: u32) {
    let breite = s.chars().count() as i32 * 8 * mal as i32;
    text(x + (w - breite) / 2, y, s, farbe, mal);
}

fn knopf(k: u32, beschriftung: &str, farbe: u32) {
    let (x, y, w, h) = knopf_lage(k);
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, k, farbe);
    text_mittig(x, w, y + (h - 32) / 2, beschriftung, 0x202020, 2);
}

fn zeichnen(j: i64, m: u32) {
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xF4F4F4, 0, Some(kern::sprache::t("Calendar", "Kalender")));
    knopf(K_ZURUECK, "<", 0xC9D6EA);
    knopf(K_VOR, ">", 0xC9D6EA);
    let (en, de) = MONATE[(m - 1) as usize];
    knopf(K_HEUTE, &format!("{} {j}", kern::sprache::t(en, de)), 0xF4F4F4);
    for (i, (en, de)) in WOCHENTAGE.iter().enumerate() {
        let farbe = if i >= 5 { 0x909090 } else { 0x404040 };
        text_mittig(RASTER_X + i as i32 * ZELLE_W, ZELLE_W, TAGE_Y, kern::sprache::t(en, de), farbe, 1);
    }
    let heute = heute();
    let erster = erster_spalte(j, m) as i32;
    for d in 1..=tage_im_monat(j, m) as i32 {
        let p = erster + d - 1;
        let (sp, ze) = (p % 7, p / 7);
        let (x, y) = (RASTER_X + sp * ZELLE_W, RASTER_Y + ze * ZELLE_H);
        let ist_heute = heute == Some((j, m, d as u32));
        if ist_heute {
            sys::rechteck(((x + 6) as u32) << 16 | (ZELLE_W - 12) as u32, ((y + 1) as u32) << 16 | (ZELLE_H - 2) as u32, 0x3A7BD5);
        }
        let farbe = if ist_heute {
            0xFFFFFF
        } else if sp >= 5 {
            0x909090
        } else {
            0x101010
        };
        text_mittig(x, ZELLE_W, y + (ZELLE_H - 16) / 2, &d.to_string(), farbe, 1);
    }
    sys::neuzeichnen(2);
    ANGEZEIGT.store((j * 12 + i64::from(m) - 1) as u32, Relaxed);
    sys::debug_text(&format!("Kalender: {} {j}", kern::sprache::t(en, de)));
}

/// Programm „kalender“.
pub fn programm() {
    // Ereignisse: Neuzeichnen, Tasten, Knöpfe, Maus (Wischen = Scrollen)
    sys::maske(0x27);
    let (mut j, mut m) = heute().map_or((2026, 1), |(j, m, _)| (j, m));
    // Wischen: Summe der Scrollschritte einer Bewegung, ein Wechsel je Bewegung
    let (mut wisch, mut wisch_t, mut fertig) = (0i32, 0u32, false);
    loop {
        let mut neu = None;
        match sys::warte_ereignis() {
            1 => zeichnen(j, m),
            2 => {
                // Pfeiltasten/Bild auf/ab wechseln den Monat
                loop {
                    let c = sys::taste();
                    if c & 0xFF == 1 {
                        break;
                    }
                    match (c >> 8) & 0xFF {
                        // links, oben, Bild auf
                        176 | 178 | 184 => neu = Some(verschieben(j, m, -1)),
                        // rechts, unten, Bild ab
                        179 | 177 | 183 => neu = Some(verschieben(j, m, 1)),
                        _ => {}
                    }
                }
            }
            3 => match sys::knopf_id() >> 8 {
                1 => return,
                K_ZURUECK => neu = Some(verschieben(j, m, -1)),
                K_VOR => neu = Some(verschieben(j, m, 1)),
                K_HEUTE => neu = heute().map(|(j, m, _)| (j, m)),
                _ => {}
            },
            6 => {
                let s = sys::maus(7);
                let (sx, sy) = ((s >> 16) as i16 as i32, s as i16 as i32);
                if sx != 0 || sy != 0 {
                    let jetzt = sys::zeitzaehler();
                    if jetzt.wrapping_sub(wisch_t) > 30 {
                        wisch = 0;
                        fertig = false;
                    }
                    wisch_t = jetzt;
                    // nach links/oben wischen (Inhalt weiter) = nächster Monat
                    wisch += if sx.abs() >= sy.abs() { sx } else { sy };
                    if !fertig && wisch.abs() >= 5 {
                        fertig = true;
                        neu = Some(verschieben(j, m, wisch.signum()));
                    }
                }
            }
            _ => {}
        }
        if let Some((nj, nm)) = neu
            && (nj, nm) != (j, m)
        {
            (j, m) = (nj, nm);
            zeichnen(j, m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monate_und_wochentage() {
        assert_eq!(tage_im_monat(2026, 2), 28);
        assert_eq!(tage_im_monat(2028, 2), 29);
        assert_eq!(tage_im_monat(2026, 11), 30);
        assert_eq!(tage_im_monat(2026, 12), 31);
        // 1.11.2026 ist ein Sonntag, 1.12.2026 ein Dienstag
        assert_eq!(erster_spalte(2026, 11), 6);
        assert_eq!(erster_spalte(2026, 12), 1);
        assert_eq!(verschieben(2026, 12, 1), (2027, 1));
        assert_eq!(verschieben(2026, 1, -1), (2025, 12));
        assert_eq!(verschieben(2026, 11, -13), (2025, 10));
    }
}
