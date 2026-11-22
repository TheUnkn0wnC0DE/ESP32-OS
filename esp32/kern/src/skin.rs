//! KolibriOS-Skins (`.skn`) laden – Format wie `skins/skin.inc` bzw.
//! `kernel/trunk/gui/skincode.inc` (parse_skin_data) – und für Touch
//! vergrössern. Standard ist der Original-Skin „Shkvorka“ (`daten/`,
//! GPL-2.0), gebaut mit `werkzeuge/skn_bauen.py`.

use crate::fenster::{self, KopfBilder, Skin};
use std::sync::Arc;

/// Eingebauter Standard-Skin (KolibriOS DEFAULT.SKN = Leency/Shkvorka).
pub static STANDARD: &[u8] = include_bytes!("../daten/shkvorka.skn");

/// Bitmap des Skins, 0x00RRGGBB.
#[derive(Clone, Debug, Default)]
pub struct Bild {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Bild {
    /// Ganzzahlig vergrössern (nächster Nachbar).
    pub fn skaliert(&self, s: i32) -> Bild {
        if s <= 1 {
            return self.clone();
        }
        let (w, h) = (self.w * s, self.h * s);
        let mut px = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                px.push(self.px[((y / s) * self.w + x / s) as usize]);
            }
        }
        Bild { w, h, px }
    }
}

/// Inhalt einer `.skn`-Datei.
#[derive(Clone, Debug)]
pub struct SkinDatei {
    pub hoehe: i32,
    /// links, oben, rechts, unten (Titelbereich, Funktion 48.7).
    pub raender: (i32, i32, i32, i32),
    /// (innen, aussen, rahmen) aktiv / inaktiv.
    pub rahmen_aktiv: (u32, u32, u32),
    pub rahmen_inaktiv: (u32, u32, u32),
    /// Systemfarben (dtp, Funktion 48.3), bis 10 Einträge.
    pub farben: Vec<u32>,
    /// (links, oben, breite, höhe); links < 0 = vom rechten Rand.
    pub knopf_zu: Option<(i32, i32, i32, i32)>,
    pub knopf_min: Option<(i32, i32, i32, i32)>,
    /// [links, oper, basis] aktiv / inaktiv.
    pub aktiv: [Bild; 3],
    pub inaktiv: [Bild; 3],
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, &'static str> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or("Skin zu kurz")
}

fn i16_at(d: &[u8], o: usize) -> Result<i32, &'static str> {
    d.get(o..o + 2).map(|b| i32::from(i16::from_le_bytes([b[0], b[1]]))).ok_or("Skin zu kurz")
}

/// `.skn` lesen (unkomprimiert; kpack-gepackte Dateien werden abgelehnt).
pub fn lesen(d: &[u8]) -> Result<SkinDatei, &'static str> {
    if d.starts_with(b"KPCK") {
        return Err("kpack-gepackter Skin wird nicht unterstützt");
    }
    if !d.starts_with(b"SKIN") || u32_at(d, 4)? != 1 {
        return Err("keine Skin-Datei (Version 1)");
    }
    let p = u32_at(d, 8)? as usize;
    let b = u32_at(d, 12)? as usize;
    let bm = u32_at(d, 16)? as usize;
    let hoehe = u32_at(d, p)? as i32;
    // dw right, left, bottom, top
    let (r, l, u, o) = (i16_at(d, p + 4)?, i16_at(d, p + 6)?, i16_at(d, p + 8)?, i16_at(d, p + 10)?);
    let f = |i: usize| u32_at(d, p + 12 + 4 * i);
    let rahmen_aktiv = (f(0)?, f(1)?, f(2)?);
    let rahmen_inaktiv = (f(3)?, f(4)?, f(5)?);
    let dtp_len = u32_at(d, p + 36)? as usize;
    let farben = (0..(dtp_len / 4).min(10)).map(|i| u32_at(d, p + 40 + 4 * i)).collect::<Result<Vec<_>, _>>()?;
    let (mut knopf_zu, mut knopf_min) = (None, None);
    let mut o2 = b;
    loop {
        let typ = u32_at(d, o2)?;
        if typ == 0 {
            break;
        }
        let k = (i16_at(d, o2 + 4)?, i16_at(d, o2 + 6)?, i16_at(d, o2 + 8)?, i16_at(d, o2 + 10)?);
        match typ {
            1 => knopf_zu = Some(k),
            2 => knopf_min = Some(k),
            _ => {}
        }
        o2 += 12;
    }
    let mut aktiv: [Bild; 3] = Default::default();
    let mut inaktiv: [Bild; 3] = Default::default();
    let mut o3 = bm;
    loop {
        let art = i16_at(d, o3)?;
        if art == 0 {
            break;
        }
        let ist_aktiv = i16_at(d, o3 + 2)? == 1;
        let off = u32_at(d, o3 + 4)? as usize;
        let (w, h) = (u32_at(d, off)? as i32, u32_at(d, off + 4)? as i32);
        let roh = d.get(off + 8..off + 8 + (w * h * 3) as usize).ok_or("Bitmap zu kurz")?;
        // Bytes wie in der BMP: B, G, R
        let px = roh.chunks_exact(3).map(|c| u32::from(c[2]) << 16 | u32::from(c[1]) << 8 | u32::from(c[0])).collect();
        let bild = Bild { w, h, px };
        let i = match art {
            1 => 0,
            2 => 1,
            3 => 2,
            _ => return Err("unbekannte Bitmap-Art"),
        };
        if ist_aktiv {
            aktiv[i] = bild;
        } else {
            inaktiv[i] = bild;
        }
        o3 += 8;
    }
    Ok(SkinDatei { hoehe, raender: (l, o, r, u), rahmen_aktiv, rahmen_inaktiv, farben, knopf_zu, knopf_min, aktiv, inaktiv })
}

/// Skin mit Touch-Faktor `s` (ganzzahlig) aus einer gelesenen Datei.
pub fn als_skin(sd: &SkinDatei, s: i32) -> Skin {
    let sk = |k: (i32, i32, i32, i32)| (k.0 * s, k.1 * s, k.2 * s, k.3 * s);
    let (l, o, r, u) = sd.raender;
    let std = Skin::default();
    Skin {
        hoehe: sd.hoehe * s,
        rahmen_aktiv: sd.rahmen_aktiv,
        rahmen_inaktiv: sd.rahmen_inaktiv,
        kopf_aktiv: std.kopf_aktiv,
        kopf_inaktiv: std.kopf_inaktiv,
        knopf_zu: sd.knopf_zu.map_or(std.knopf_zu, sk),
        knopf_min: sd.knopf_min.map_or(std.knopf_min, sk),
        raender: (l * s, o * s, r * s, u * s),
        bilder: Some(Arc::new(KopfBilder {
            aktiv: sd.aktiv.clone().map(|b| b.skaliert(s)),
            inaktiv: sd.inaktiv.clone().map(|b| b.skaliert(s)),
        })),
        skala: s,
    }
}

/// Skin systemweit setzen (Funktion 48.8): Systemfarben übernehmen, alle
/// Fenster neu zeichnen lassen.
pub fn anwenden(d: &[u8], s: i32) -> Result<(), &'static str> {
    // Alles, was das Programm überlebt (Bitmaps im Welt-Skin), im System-Heap.
    let (sd, skin) = crate::prozess::kern_abschnitt(|| -> Result<_, &'static str> {
        let sd = lesen(d)?;
        let skin = als_skin(&sd, s);
        Ok((sd, skin))
    })?;
    crate::prozess::kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            w.skin = skin;
            for (i, &c) in sd.farben.iter().enumerate() {
                w.farben[i] = c;
            }
            let slots: Vec<usize> = w.stapel.clone();
            for s in slots {
                w.rahmen_neu(s);
            }
            for fz in w.slots.iter_mut().flatten() {
                fz.redraw = true;
            }
            w.alles();
        })
    });
    fenster::wecken();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shkvorka_lesen() {
        let s = lesen(STANDARD).unwrap();
        assert_eq!(s.hoehe, 24);
        assert_eq!(s.raender, (6, 5, 65, 3));
        assert_eq!(s.knopf_zu, Some((-31, 1, 27, 18)));
        assert_eq!(s.knopf_min, Some((-58, 1, 27, 18)));
        assert_eq!(s.rahmen_aktiv, (0xDDD7CF, 0x646464, 0xDDD7CF));
        assert_eq!(s.farben.len(), 10);
        assert_eq!(s.farben[4], 0x333333);
        assert_eq!(s.aktiv[1].w, 60);
        assert_eq!(s.aktiv[2].h, 24);
        let sk = als_skin(&s, 2);
        assert_eq!(sk.hoehe, 48);
        assert_eq!(sk.knopf_zu, (-62, 2, 54, 36));
    }

    #[test]
    fn kpack_abgelehnt() {
        assert!(lesen(b"KPCK....").is_err());
    }
}
