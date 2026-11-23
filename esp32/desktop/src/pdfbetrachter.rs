//! PDF-Betrachter („Wenn Zeit bleibt“) – Seiten aus `crate::pdf`, im Stil
//! des Bildbetrachters.
//!
//! Touch: Wischen nach oben/unten rollt die Seite (am Ende weiter auf die
//! nächste bzw. vorige Seite), nach links/rechts blättert; zwei Finger
//! zoomen. Rechts Knöpfe „<“, „>“ (Seite), „+“, „−“ (Zoom). Parameter: Pfad
//! der PDF-Datei (vom Dateimanager); ohne Parameter Dateidialog.

use crate::gesten::SCROLL_SCHRITT;
use crate::pdf::{self, Leinwand, Pdf};
use crate::shell::LEISTE_H;
use kern::sprache::t;
use kern::{datei, sys};
use kolibri::dekoder::Rgba;

const SPALTE: i32 = 64;
const K_ZURUECK: u32 = 2;
const K_WEITER: u32 = 3;
const K_GROSS: u32 = 4;
const K_KLEIN: u32 = 5;
const HG: u32 = 0x50545C;
/// Grösste Datei (bleibt ganz im Speicher) und grösste gezeichnete Seite
/// (Bildpunkte, 4 Byte je Punkt). Heap 8 MB: Datei 4 MB, Seite in
/// Seitenbreite (A4) 1,8 MB, der Rest für Bilder und Objektströme.
const MAX_DATEI: u64 = 4 << 20;
const MAX_SEITE_PX: f64 = 1_200_000.0;
/// Frei zu haltender Speicher für die Bilder einer Seite.
const RESERVE_BILDER: f64 = 1_500_000.0;

/// Grösste Seite (Bildpunkte), die gerade in den Speicher passt.
fn seiten_grenze() -> f64 {
    let am_stueck = sys::programm_speicher_groesster_block().map_or(f64::MAX, |g| g as f64);
    let frei = sys::programm_speicher_frei().map_or(f64::MAX, |f| f as f64 - RESERVE_BILDER);
    MAX_SEITE_PX.min(am_stueck.min(frei) / 4.0).max(100_000.0)
}

fn bereich() -> (i32, i32) {
    let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
    (640 - 10 - SPALTE, 360 - LEISTE_H - skin - 5)
}

struct Zustand {
    name: String,
    pdf: Option<Pdf>,
    seite: usize,
    /// Zoom gegenüber „Seitenbreite = Bereich“ (1.0)
    zoom: f64,
    bild: Option<Leinwand>,
    /// Ausschnitt (Bildpunkte der gezeichneten Seite)
    vx: f64,
    vy: f64,
    fehler: String,
    wisch_x: i32,
    wisch_t: u32,
    wisch_fertig: bool,
    /// Nach dem Blättern kurz keine weitere Wischgeste (Nachlauf, langsames
    /// Zeichnen der Seite) – wie im Bildbetrachter.
    wisch_sperre: u32,
    /// Zwei-Finger-Zoom läuft: gesammelter Faktor (Vorschau aus der
    /// gezeichneten Seite) und Zeit des letzten Schritts; neu gezeichnet wird
    /// erst, wenn die Finger ruhen
    zoom_offen: f64,
    zoom_t: u32,
}

/// So lange (Hundertstel) ohne Zoom-Schritt, dann wird neu gezeichnet.
const ZOOM_RUHE: u32 = 25;

/// Glyphen der Systemschrift (8×16) für `pdf`.
fn glyphe(c: char) -> [u8; 16] {
    kern::schrift::glyphe(c)
}

impl Zustand {
    fn seite_zeichnen(&mut self) {
        self.bild = None;
        let Some(p) = &self.pdf else { return };
        let Some(s) = p.seiten.get(self.seite) else { return };
        let (aw, _) = bereich();
        let quer = s.drehung == 90 || s.drehung == 270;
        let (bw, bh) = (s.kasten[2] - s.kasten[0], s.kasten[3] - s.kasten[1]);
        let (pw, ph) = if quer { (bh, bw) } else { (bw, bh) };
        // Seitenbreite = Bereich, mal Zoom; nicht grösser als der Speicher
        // erlaubt (dann bleibt der Zoom kleiner)
        let mut skala = aw as f64 / pw.max(1.0) * self.zoom;
        let flaeche = pw * ph * skala * skala;
        let grenze = seiten_grenze();
        if flaeche > grenze {
            skala *= (grenze / flaeche).sqrt();
            self.zoom = skala * pw.max(1.0) / aw as f64;
        }
        let t0 = sys::zeit_ns();
        match pdf::seite_zeichnen(p, self.seite, skala, &glyphe) {
            Ok((l, texte)) => {
                sys::debug_text(&format!(
                    "PDF: Seite {}/{} {}x{} in {} ms, {} Textstücke\n",
                    self.seite + 1,
                    p.seiten.len(),
                    l.w,
                    l.h,
                    (sys::zeit_ns() - t0) / 1_000_000,
                    texte.len()
                ));
                self.bild = Some(l);
                self.fehler.clear();
            }
            Err(f) => self.fehler = f,
        }
        self.begrenzen();
    }

    fn begrenzen(&mut self) {
        let (aw, ah) = bereich();
        if let Some(b) = &self.bild {
            let (w, h) = (b.w as f64, b.h as f64);
            self.vx = if w <= aw as f64 { -(aw as f64 - w) / 2.0 } else { self.vx.clamp(0.0, w - aw as f64) };
            self.vy = if h <= ah as f64 { -(ah as f64 - h) / 2.0 } else { self.vy.clamp(0.0, h - ah as f64) };
        }
    }

    fn seiten(&self) -> usize {
        self.pdf.as_ref().map_or(0, |p| p.seiten.len())
    }

    /// Zur Seite `i` (oben bzw. unten anfangen).
    fn gehe_zu(&mut self, i: isize, unten: bool) {
        let n = self.seiten() as isize;
        if n == 0 || i < 0 || i >= n {
            return;
        }
        self.seite = i as usize;
        self.seite_zeichnen();
        self.vy = if unten { f64::MAX } else { 0.0 };
        self.begrenzen();
    }

    fn zoomen(&mut self, f: f64) {
        let alt = self.zoom;
        self.zoom = (self.zoom * f).clamp(0.5, 3.0);
        if (self.zoom - alt).abs() < 1e-6 {
            return;
        }
        // Mitte des Ausschnitts festhalten
        let (aw, ah) = bereich();
        let (mx, my) = (self.vx + aw as f64 / 2.0, self.vy + ah as f64 / 2.0);
        self.seite_zeichnen();
        let k = self.zoom / alt;
        self.vx = mx * k - aw as f64 / 2.0;
        self.vy = my * k - ah as f64 / 2.0;
        self.begrenzen();
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
    let mut s = format!("{} - {}", t("PDF Viewer", "PDF-Betrachter"), z.name);
    if z.pdf.is_some() {
        s += &format!(" ({}/{}, {}%)", z.seite + 1, z.seiten(), (z.zoom * 100.0).round());
    }
    s
}

/// Ausschnitt der Seite (grau daneben) in den Bildbereich.
fn seite_zeigen(z: &Zustand) {
    let (aw, ah) = bereich();
    match &z.bild {
        // Zwei-Finger-Zoom läuft: Vorschau aus der gezeichneten Seite
        Some(b) if z.zoom_offen != 1.0 => {
            let (aw, ah) = (aw.max(1) as usize, ah.max(1) as usize);
            let f = z.zoom_offen as f32;
            let (mx, my) = (z.vx as f32 + aw as f32 / 2.0, z.vy as f32 + ah as f32 / 2.0);
            let mut px = vec![HG; aw * ah];
            for y in 0..ah {
                let sy = my + (y as f32 - ah as f32 / 2.0) / f;
                if sy < 0.0 || sy >= b.h as f32 {
                    continue;
                }
                let zeile = &b.px[sy as usize * b.w..(sy as usize + 1) * b.w];
                for (x, p) in px[y * aw..(y + 1) * aw].iter_mut().enumerate() {
                    let sx = mx + (x as f32 - aw as f32 / 2.0) / f;
                    if sx >= 0.0 && sx < b.w as f32 {
                        *p = zeile[sx as usize] & 0xFF_FFFF;
                    }
                }
            }
            let r = Rgba { w: aw as u32, h: ah as u32, px, delay: 0 };
            kolibri::libimg::zeichnen(&r, 0, 0, r.w, r.h, 0, 0);
        }
        // Ausschnitt direkt aus der Seite (Funktion 73 mit Zeilenlänge),
        // ohne Zwischenkopie; grau, wo die Seite nicht hinreicht
        Some(b) => {
            let (ox, oy) = (z.vx.round() as i32, z.vy.round() as i32);
            let (zx, qx) = if ox < 0 { (-ox, 0) } else { (0, ox) };
            let (zy, qy) = if oy < 0 { (-oy, 0) } else { (0, oy) };
            let w = (aw - zx).min(b.w as i32 - qx).max(0);
            let h = (ah - zy).min(b.h as i32 - qy).max(0);
            if zx > 0 || zy > 0 || w < aw || h < ah {
                sys::rechteck(aw as u32, ah as u32, HG);
            }
            // SAFETY: u32-Bildpunkte als Bytes lesen (gleiche Speicherstelle,
            // Länge × 4); Funktion 73 erwartet 32 bpp BGRX, die Seite ist
            // 0xAARRGGBB in Little Endian – Byte 0 = Blau.
            let daten = unsafe { std::slice::from_raw_parts(b.px.as_ptr().cast::<u8>(), b.px.len() * 4) };
            sys::blit(0x2000_0000, [zx, zy, w, h, qx, qy, w, h], daten, b.w * 4);
        }
        None => {
            sys::rechteck(aw as u32, ah as u32, HG);
            let f = if z.fehler.is_empty() { t("Loading…", "Lädt…") } else { z.fehler.as_str() };
            let n = f.chars().count() as i32 * 8;
            text(((aw - n) / 2).max(4), ah / 2 - 8, f, 0xF0F0F0);
        }
    }
}

fn zeichnen(z: &Zustand) {
    let (aw, ah) = bereich();
    sys::neuzeichnen(1);
    let t = titel(z);
    sys::fenster(639, (360 - LEISTE_H - 1) as u32, 0x33 << 24 | HG, 0, Some(&t));
    sys::titel(&t);
    seite_zeigen(z);
    let x = aw + 4;
    sys::rechteck((aw as u32) << 16 | SPALTE as u32, ah as u32, 0x2C3038);
    knopf(K_ZURUECK, x, 4, SPALTE - 8, 48, "<");
    knopf(K_WEITER, x, 58, SPALTE - 8, 48, ">");
    knopf(K_GROSS, x, 112, SPALTE - 8, 48, "+");
    knopf(K_KLEIN, x, 166, SPALTE - 8, 48, "-");
    sys::neuzeichnen(2);
}

/// PDF-Datei lesen und öffnen.
pub fn laden(pfad: &str) -> Result<Pdf, String> {
    let mut info = [0u8; 40];
    if datei::info(pfad, &mut info) != datei::OK {
        return Err(t("File not found", "Datei nicht gefunden").to_string());
    }
    let groesse = u64::from_le_bytes(info[32..40].try_into().unwrap_or([0; 8]));
    if groesse > MAX_DATEI {
        return Err(t("File too large (max. 4 MB)", "Datei zu gross (max. 4 MB)").to_string());
    }
    let mut d = vec![0u8; groesse as usize];
    let (c, n) = datei::lesen(pfad, 0, &mut d);
    if (c != datei::OK && c != datei::EOF) || u64::from(n) != groesse {
        return Err(t("Read error", "Lesefehler").to_string());
    }
    Pdf::oeffnen(d).map_err(|f| format!("PDF: {f}"))
}

/// Programm „pdfbetrachter“ (Parameter: Pfad der PDF-Datei).
pub fn programm() {
    let mut pfad = kern::argumente();
    if pfad.is_empty() {
        match kolibri::dialog::datei_dialog(kolibri::dialog::Modus::Oeffnen, "/sd0/1", "", (420, 300), (10, 10)) {
            Some(p) => pfad = p,
            None => sys::ende(),
        }
    }
    let name = pfad.rsplit('/').next().unwrap_or(&pfad).to_string();
    let mut z = Zustand { name, pdf: None, seite: 0, zoom: 1.0, bild: None, vx: 0.0, vy: 0.0, fehler: String::new(), wisch_x: 0, wisch_t: 0, wisch_fertig: false, wisch_sperre: 0, zoom_offen: 1.0, zoom_t: 0 };
    let t0 = sys::zeit_ns();
    match laden(&pfad) {
        Ok(p) => {
            sys::debug_text(&format!("PDF: {pfad}: {} Seiten, geöffnet in {} ms\n", p.seiten.len(), (sys::zeit_ns() - t0) / 1_000_000));
            z.pdf = Some(p);
            z.seite_zeichnen();
        }
        Err(f) => {
            sys::debug_text(&format!("PDF: {pfad}: {f}\n"));
            z.fehler = f;
        }
    }
    sys::maske(0x27);
    loop {
        let ereignis = if z.zoom_offen != 1.0 { sys::warte_ereignis_max(5) } else { sys::warte_ereignis() };
        // Finger ruhen: Zoom übernehmen und die Seite neu zeichnen
        if z.zoom_offen != 1.0 && sys::zeitzaehler().wrapping_sub(z.zoom_t) >= ZOOM_RUHE {
            let f = std::mem::replace(&mut z.zoom_offen, 1.0);
            z.zoomen(f);
            zeichnen(&z);
        }
        match ereignis {
            1 => zeichnen(&z),
            3 => {
                match sys::knopf_id() >> 8 {
                    1 => sys::ende(),
                    K_ZURUECK => z.gehe_zu(z.seite as isize - 1, false),
                    K_WEITER => z.gehe_zu(z.seite as isize + 1, false),
                    K_GROSS => z.zoomen(1.25),
                    K_KLEIN => z.zoomen(0.8),
                    _ => {}
                }
                zeichnen(&z);
            }
            6 => {
                let mut neu = false;
                let (zf, _, _) = sys::zoom_holen();
                if zf != 1000 && z.bild.is_some() {
                    // nur sammeln (Vorschau); gezeichnet wird, wenn die
                    // Finger ruhen – sonst je Schritt eine ganze Seite
                    z.zoom_offen = (z.zoom_offen * f64::from(zf) / 1000.0).clamp(0.5 / z.zoom, 3.0 / z.zoom);
                    z.zoom_t = sys::zeitzaehler();
                    neu = true;
                }
                let s = sys::maus(7);
                let (sx, sy) = ((s >> 16) as i16 as i32, s as i16 as i32);
                if sy != 0 && z.bild.is_some() {
                    let (_, ah) = bereich();
                    let alt = z.vy;
                    z.vy += f64::from(sy * SCROLL_SCHRITT);
                    z.begrenzen();
                    // am Ende der Seite weiter auf die nächste bzw. vorige
                    let h = z.bild.as_ref().map_or(0.0, |b| b.h as f64);
                    if (z.vy - alt).abs() < 0.5 && h > 0.0 {
                        if sy > 0 && z.vy >= h - f64::from(ah) - 0.5 {
                            z.gehe_zu(z.seite as isize + 1, false);
                        } else if sy < 0 && z.vy <= 0.5 {
                            z.gehe_zu(z.seite as isize - 1, true);
                        }
                    }
                    neu = true;
                }
                if sx != 0 && z.bild.is_some() {
                    let jetzt = sys::zeitzaehler();
                    if jetzt.wrapping_sub(z.wisch_t) > 30 {
                        z.wisch_x = 0;
                        z.wisch_fertig = false;
                    }
                    z.wisch_t = jetzt;
                    let breit = z.bild.as_ref().is_some_and(|b| b.w as i32 > bereich().0);
                    if breit {
                        z.vx += f64::from(sx * SCROLL_SCHRITT);
                        z.begrenzen();
                        neu = true;
                    } else {
                        z.wisch_x += sx;
                        let gesperrt = (jetzt.wrapping_sub(z.wisch_sperre) as i32) < 0;
                        if !z.wisch_fertig && !gesperrt && z.wisch_x.abs() >= 5 {
                            z.wisch_fertig = true;
                            z.gehe_zu(z.seite as isize + z.wisch_x.signum() as isize, false);
                            zeichnen(&z);
                            z.wisch_sperre = sys::zeitzaehler().wrapping_add(30);
                            z.wisch_t = sys::zeitzaehler();
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
