//! Bildbetrachter (P2) – nativ im KolibriOS-Stil (Vorbild kiv; KolibriOS hat
//! keinen Bildbetrachter in C). JPEG über die Hardware-JPEG-Einheit (Board,
//! Haken in `kolibri::dekoder`), PNG, GIF (animiert), BMP über libimg.
//!
//! Touch: Wischen nach links/rechts = nächstes/voriges Bild im selben
//! Ordner; zwei Finger = Zoom um ihren Mittelpunkt; vergrössert verschiebt
//! Wischen den Ausschnitt. Rechts Knöpfe „<“, „>“ und „Fit“/„1:1“.
//! Parameter: Pfad des Bildes (vom Dateimanager); ohne Parameter Dateidialog.

use crate::dateimanager::{lesen as ordner_lesen, programm_fuer};
use crate::gesten::SCROLL_SCHRITT;
use crate::shell::LEISTE_H;
use kern::sprache::t;
use kern::{datei, sys};
use kolibri::dekoder::{self, Rgba};

/// Breite der Knopfspalte rechts.
pub const SPALTE: i32 = 64;
/// Grösstes Bild (Pixel): RGBA im Programm-Heap (12 MB).
pub const MAX_PIXEL: u64 = 2_100_000;
const MAX_DATEI: u64 = 16 << 20;

const K_ZURUECK: u32 = 2;
const K_WEITER: u32 = 3;
const K_FIT: u32 = 4;
const HG: u32 = 0x202428;

/// Geladenes Bild mit Ansicht.
struct Ansicht {
    bilder: Vec<Rgba>,
    bild: usize,
    /// Anzeigepixel je Bildpixel.
    s: f32,
    /// Bildkoordinate links oben im Bildbereich.
    vx: f32,
    vy: f32,
    /// Auf „Fit“ geglättet verkleinertes Bild (Qualität), sonst nächster Nachbar.
    fit_bild: Option<Rgba>,
}

struct Zustand {
    ordner: String,
    dateien: Vec<String>,
    i: usize,
    ansicht: Option<Ansicht>,
    fehler: String,
    /// Wischen: Summe der Scroll-Schritte, Zeit des letzten (1/100 s).
    wisch: i32,
    wisch_t: u32,
    wisch_fertig: bool,
    /// Nach dem Umblättern keine weitere Seite vor dieser Zeit (1/100 s):
    /// der Rest derselben Wischbewegung kommt erst nach Laden und Zeichnen.
    wisch_sperre: u32,
}

fn bereich() -> (i32, i32) {
    let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
    (640 - 10 - SPALTE, 360 - LEISTE_H - skin - 5)
}

fn fit(w: u32, h: u32) -> f32 {
    let (aw, ah) = bereich();
    (aw as f32 / w as f32).min(ah as f32 / h as f32)
}

/// Datei lesen und dekodieren; Fehlertext bei Problemen.
pub fn laden(pfad: &str) -> Result<Vec<Rgba>, String> {
    let mut info = [0u8; 40];
    if datei::info(pfad, &mut info) != datei::OK {
        return Err(t("File not found", "Datei nicht gefunden").to_string());
    }
    let groesse = u64::from_le_bytes(info[32..40].try_into().unwrap_or([0; 8]));
    if groesse > MAX_DATEI {
        return Err(t("File too large", "Datei zu gross").to_string());
    }
    let mut d = vec![0u8; groesse as usize];
    let (c, n) = datei::lesen(pfad, 0, &mut d);
    if (c != datei::OK && c != datei::EOF) || u64::from(n) != groesse {
        return Err(t("Read error", "Lesefehler").to_string());
    }
    match dekoder::masse(&d) {
        Some((w, h)) if u64::from(w) * u64::from(h) > MAX_PIXEL => {
            return Err(format!("{w}×{h}: {}", t("image too large (max. 2.1 MP)", "Bild zu gross (max. 2,1 MP)")));
        }
        None => return Err(t("Unknown image format", "Unbekanntes Bildformat").to_string()),
        _ => {}
    }
    match dekoder::dekodieren(&d) {
        Ok((_, v)) if !v.is_empty() => Ok(v),
        Ok(_) | Err(dekoder::Fehler::Format) => Err(t("Damaged image", "Bild beschädigt").to_string()),
        Err(dekoder::Fehler::NichtUnterstuetzt) => Err(t("Format variant not supported (e.g. progressive JPEG)", "Formatvariante nicht unterstützt (z. B. progressives JPEG)").to_string()),
    }
}

impl Zustand {
    fn oeffnen(&mut self) {
        self.ansicht = None;
        self.fehler.clear();
        let Some(name) = self.dateien.get(self.i).cloned() else {
            self.fehler = t("No images", "Keine Bilder").to_string();
            return;
        };
        let pfad = format!("{}/{}", self.ordner.trim_end_matches('/'), name);
        let t0 = sys::zeit_ns();
        match laden(&pfad) {
            Ok(bilder) => {
                let (w, h) = (bilder[0].w, bilder[0].h);
                sys::debug_text(&format!("BV: {pfad} {w}x{h} in {} ms\n", (sys::zeit_ns() - t0) / 1_000_000));
                let mut a = Ansicht { bilder, bild: 0, s: 1.0, vx: 0.0, vy: 0.0, fit_bild: None };
                a.fit_setzen();
                self.ansicht = Some(a);
            }
            Err(f) => {
                sys::debug_text(&format!("BV: {pfad}: {f}\n"));
                self.fehler = f;
            }
        }
    }

    fn blaettern(&mut self, d: i32) {
        if self.dateien.is_empty() {
            return;
        }
        let n = self.dateien.len() as i32;
        self.i = (self.i as i32 + d).rem_euclid(n) as usize;
        self.oeffnen();
    }
}

impl Ansicht {
    fn wh(&self) -> (u32, u32) {
        (self.bilder[0].w, self.bilder[0].h)
    }

    fn ist_fit(&self) -> bool {
        let (w, h) = self.wh();
        (self.s - fit(w, h)).abs() < 1e-4
    }

    fn fit_setzen(&mut self) {
        let (w, h) = self.wh();
        self.s = fit(w, h);
        self.begrenzen();
        if self.fit_bild.is_none() && self.bilder.len() == 1 {
            let (nw, nh) = (((w as f32 * self.s).round() as u32).max(1), ((h as f32 * self.s).round() as u32).max(1));
            self.fit_bild = Some(kolibri::libimg::skalieren(&self.bilder[0], nw, nh, true));
        }
    }

    /// Zoom um Faktor `z` mit festem Punkt (mx, my) im Bildbereich.
    fn zoomen(&mut self, z: f32, mx: f32, my: f32) {
        let (w, h) = self.wh();
        let f = fit(w, h);
        let neu = (self.s * z).clamp(f, (f * 8.0).max(4.0));
        let (ix, iy) = (self.vx + mx / self.s, self.vy + my / self.s);
        self.s = neu;
        self.vx = ix - mx / neu;
        self.vy = iy - my / neu;
        self.begrenzen();
    }

    /// Ausschnitt im Bild halten; kleiner als der Bereich = zentriert.
    fn begrenzen(&mut self) {
        let (aw, ah) = bereich();
        let (w, h) = self.wh();
        let (sw, sh) = (aw as f32 / self.s, ah as f32 / self.s);
        self.vx = if (w as f32) <= sw { -(sw - w as f32) / 2.0 } else { self.vx.clamp(0.0, w as f32 - sw) };
        self.vy = if (h as f32) <= sh { -(sh - h as f32) / 2.0 } else { self.vy.clamp(0.0, h as f32 - sh) };
    }

    /// Bildbereich berechnen (Schachbrett unter Transparenz).
    fn rendern(&self) -> Rgba {
        let (aw, ah) = bereich();
        let (aw, ah) = (aw.max(1) as u32, ah.max(1) as u32);
        let mut px = vec![0u32; (aw * ah) as usize];
        let b = &self.bilder[self.bild];
        let fitb = if self.ist_fit() { self.fit_bild.as_ref() } else { None };
        // Lage des Bildes auf dem Bereich
        let (bw, bh) = (b.w as f32 * self.s, b.h as f32 * self.s);
        let (ox, oy) = (-self.vx * self.s, -self.vy * self.s);
        for y in 0..ah {
            for x in 0..aw {
                let (fx, fy) = (x as f32 - ox, y as f32 - oy);
                let i = (y * aw + x) as usize;
                if fx < 0.0 || fy < 0.0 || fx >= bw || fy >= bh {
                    px[i] = HG;
                    continue;
                }
                let p = match fitb {
                    Some(f) => f.px[((fy as u32).min(f.h - 1) * f.w + (fx as u32).min(f.w - 1)) as usize],
                    None => {
                        let (sx, sy) = (((fx / self.s) as u32).min(b.w - 1), ((fy / self.s) as u32).min(b.h - 1));
                        b.px[(sy * b.w + sx) as usize]
                    }
                };
                let a = p >> 24;
                px[i] = if a == 255 {
                    p & 0xFF_FFFF
                } else {
                    let k = if (x / 8 + y / 8) % 2 == 0 { 0xC8 } else { 0x98 };
                    let m = |s: u32| ((p >> s & 255) * a + k * (255 - a)) / 255;
                    m(16) << 16 | m(8) << 8 | m(0)
                };
            }
        }
        Rgba { w: aw, h: ah, px, delay: 0 }
    }
}

fn text(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

fn knopf(id: u32, x: i32, y: i32, w: i32, h: i32, beschriftung: &str) {
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, id, 0x3C4250);
    let n = beschriftung.chars().count() as i32 * 8;
    text(x + (w - n) / 2, y + (h - 16) / 2, beschriftung, 0xFFFFFF);
}

fn titel(z: &Zustand) -> String {
    let name = z.dateien.get(z.i).map_or("", String::as_str);
    let mut t = format!("{} - {name}", t("Image Viewer", "Bildbetrachter"));
    if let Some(a) = &z.ansicht {
        let (w, h) = a.wh();
        t += &format!(" ({}/{}, {w}x{h}, {}%)", z.i + 1, z.dateien.len(), (a.s * 100.0).round());
    }
    t
}

fn bild_zeichnen(z: &Zustand) {
    let (aw, ah) = bereich();
    match &z.ansicht {
        Some(a) => {
            let r = a.rendern();
            kolibri::libimg::zeichnen(&r, 0, 0, r.w, r.h, 0, 0);
        }
        None => {
            sys::rechteck(aw as u32, ah as u32, HG);
            let f = if z.fehler.is_empty() { t("Loading…", "Lädt…") } else { z.fehler.as_str() };
            let n = f.chars().count() as i32 * 8;
            text(((aw - n) / 2).max(4), ah / 2 - 8, f, 0xE0E0E0);
        }
    }
}

fn zeichnen(z: &Zustand) {
    let (aw, ah) = bereich();
    sys::neuzeichnen(1);
    let t = titel(z);
    sys::fenster(639, (360 - LEISTE_H - 1) as u32, 0x33 << 24 | HG, 0, Some(&t));
    sys::titel(&t);
    let t0 = sys::zeit_ns();
    bild_zeichnen(z);
    if z.ansicht.is_some() {
        sys::debug_text(&format!("BV: Zeichnen {} ms\n", (sys::zeit_ns() - t0) / 1_000_000));
    }
    let x = aw + 4;
    sys::rechteck((aw as u32) << 16 | SPALTE as u32, ah as u32, 0x2C3038);
    knopf(K_ZURUECK, x, 4, SPALTE - 8, 48, "<");
    knopf(K_WEITER, x, 58, SPALTE - 8, 48, ">");
    let fit_text = if z.ansicht.as_ref().is_some_and(Ansicht::ist_fit) { "1:1" } else { "Fit" };
    knopf(K_FIT, x, 112, SPALTE - 8, 48, fit_text);
    sys::neuzeichnen(2);
}

/// Bilder (nach Endung) im Ordner, sortiert wie im Dateimanager.
fn bilder_im_ordner(ordner: &str) -> Vec<String> {
    ordner_lesen(ordner)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.ordner && programm_fuer(&e.name) == Some("bildbetrachter"))
        .map(|e| e.name)
        .collect()
}

/// Programm „bildbetrachter“ (Parameter: Bildpfad).
pub fn programm() {
    let mut pfad = kern::argumente();
    if pfad.is_empty() {
        match kolibri::dialog::datei_dialog(kolibri::dialog::Modus::Oeffnen, "/sd0/1", "", (420, 300), (10, 10)) {
            Some(p) => pfad = p,
            None => sys::ende(),
        }
    }
    let (ordner, name) = match pfad.rsplit_once('/') {
        Some((o, n)) => (if o.is_empty() { "/".to_string() } else { o.to_string() }, n.to_string()),
        None => ("/".to_string(), pfad.clone()),
    };
    let mut dateien = bilder_im_ordner(&ordner);
    let i = match dateien.iter().position(|d| d.eq_ignore_ascii_case(&name)) {
        Some(i) => i,
        None => {
            dateien.insert(0, name);
            0
        }
    };
    let mut z = Zustand { ordner, dateien, i, ansicht: None, fehler: String::new(), wisch: 0, wisch_t: 0, wisch_fertig: false, wisch_sperre: 0 };
    z.oeffnen();
    sys::maske(0x27);
    loop {
        // GIF-Animation: nächstes Einzelbild nach seiner Anzeigedauer
        let warten = z.ansicht.as_ref().filter(|a| a.bilder.len() > 1).map(|a| a.bilder[a.bild].delay.max(2));
        let e = match warten {
            Some(d) => sys::warte_ereignis_max(d),
            None => sys::warte_ereignis(),
        };
        match e {
            0 => {
                if let Some(a) = z.ansicht.as_mut()
                    && a.bilder.len() > 1
                {
                    a.bild = (a.bild + 1) % a.bilder.len();
                    bild_zeichnen(&z);
                }
            }
            1 => zeichnen(&z),
            3 => {
                match sys::knopf_id() >> 8 {
                    1 => sys::ende(),
                    K_ZURUECK => z.blaettern(-1),
                    K_WEITER => z.blaettern(1),
                    K_FIT => {
                        if let Some(a) = z.ansicht.as_mut() {
                            if a.ist_fit() {
                                let (aw, ah) = bereich();
                                let f = 1.0 / a.s;
                                a.zoomen(f, aw as f32 / 2.0, ah as f32 / 2.0);
                            } else {
                                a.fit_setzen();
                            }
                        }
                    }
                    _ => {}
                }
                zeichnen(&z);
            }
            6 => {
                let mut neu = false;
                let (zf, mx, my) = sys::zoom_holen();
                if zf != 1000
                    && let Some(a) = z.ansicht.as_mut()
                {
                    a.zoomen(zf as f32 / 1000.0, mx as f32, my as f32);
                    neu = true;
                }
                let s = sys::maus(7);
                let (sx, sy) = ((s >> 16) as i16 as i32, s as i16 as i32);
                if sx != 0 || sy != 0 {
                    let jetzt = sys::zeitzaehler();
                    if jetzt.wrapping_sub(z.wisch_t) > 30 {
                        z.wisch = 0;
                        z.wisch_fertig = false;
                    }
                    z.wisch_t = jetzt;
                    match z.ansicht.as_mut() {
                        Some(a) if !a.ist_fit() => {
                            a.vx += (sx * SCROLL_SCHRITT) as f32 / a.s;
                            a.vy += (sy * SCROLL_SCHRITT) as f32 / a.s;
                            a.begrenzen();
                            neu = true;
                        }
                        _ => {
                            z.wisch += sx;
                            let gesperrt = (jetzt.wrapping_sub(z.wisch_sperre) as i32) < 0;
                            if !z.wisch_fertig && !gesperrt && z.wisch.abs() >= 5 {
                                z.wisch_fertig = true;
                                z.blaettern(z.wisch.signum());
                                zeichnen(&z);
                                z.wisch_sperre = sys::zeitzaehler().wrapping_add(30);
                                z.wisch_t = sys::zeitzaehler();
                            }
                        }
                    }
                }
                if neu {
                    zeichnen(&z);
                }
            }
            _ => {}
        }
    }
}
