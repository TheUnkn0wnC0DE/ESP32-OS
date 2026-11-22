//! Gemeinsame Oberfläche aller Spiele: FPS-Anzeige und Home-Taste (zurück
//! zum Konsolen-Menü). Koordinaten in Bildpunkten des 640×360-Bildes.

use pudding_core::app::Perf;
use pudding_core::gfx::{Canvas, hex, text_width};
use pudding_core::input::Input;
use spatz::render::overlay::fmt;

/// Gezeichnete Home-Taste (x, y, Breite, Höhe).
pub const HOME_RECT: (i32, i32, i32, i32) = (8, 8, 44, 44);
/// Trefferfläche der Home-Taste (großzügiger als die Zeichnung).
pub const HOME_TREFFER: (u16, u16) = (72, 72);

fn im_home(x: u16, y: u16) -> bool {
    x < HOME_TREFFER.0 && y < HOME_TREFFER.1
}

/// Ergebnis eines Bildes für die Home-Taste.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HomeErgebnis {
    /// Losgelassen über der Taste → zurück zur Konsole.
    pub ausgeloest: bool,
    /// Der Finger gehört der Home-Taste – nicht ans Spiel weitergeben.
    pub verbraucht: bool,
}

/// Home-Taste wie ein Knopf: löst beim Loslassen über der Taste aus, und nur,
/// wenn der Finger auf ihr aufgesetzt hat.
#[derive(Clone, Copy, Debug, Default)]
pub struct HomeTaste {
    aktiv: bool,
    beruehrt: bool,
    drin: bool,
}

impl HomeTaste {
    pub fn update(&mut self, input: &Input) -> HomeErgebnis {
        match input.first_touch() {
            Some((x, y)) => {
                if !self.beruehrt {
                    self.aktiv = im_home(x, y);
                }
                self.beruehrt = true;
                self.drin = self.aktiv && im_home(x, y);
                HomeErgebnis { ausgeloest: false, verbraucht: self.aktiv }
            }
            None => {
                let ausgeloest = self.aktiv && self.drin;
                *self = Self::default();
                HomeErgebnis { ausgeloest, verbraucht: false }
            }
        }
    }

    /// Wird gerade gedrückt (Finger auf der Taste).
    pub fn gedrueckt(&self) -> bool {
        self.aktiv && self.drin
    }
}

/// Home-Taste zeichnen: runde Fläche mit Haus.
pub fn home_taste(c: &mut Canvas, gedrueckt: bool) {
    let (x, y, w, h) = HOME_RECT;
    let (cx, cy) = (x + w / 2, y + h / 2);
    let r = (w / 2) as f32;
    if gedrueckt {
        c.mix_disc(cx as f32, cy as f32, r, hex(0xFFFFFF), 14);
    } else {
        c.mix_disc(cx as f32, cy as f32, r, hex(0x101018), 20);
    }
    c.mix_ring(cx as f32, cy as f32, r, 1.5, hex(0xFFFFFF), 18);
    let weiss = hex(0xFFFFFF);
    // Dach (Dreieck aus Zeilen), Haus, Tür
    for i in 0..9 {
        c.fill_rect(cx - i - 1, cy - 12 + i, 2 * i + 2, 1, weiss);
    }
    c.fill_rect(cx - 8, cy - 3, 16, 12, weiss);
    c.fill_rect(cx - 2, cy + 2, 4, 7, if gedrueckt { hex(0x606070) } else { hex(0x202028) });
}

/// Linke Kante der FPS-Anzeige neben der Home-Taste.
pub const FPS_X: i32 = HOME_RECT.0 + HOME_RECT.2 + 8;

/// FPS-Anzeige oben links ab `x`.
pub fn fps(c: &mut Canvas, perf: &Perf, x: i32) {
    fps_mit_ziel(c, perf.fps, 55.0, x);
}

/// FPS-Anzeige für ein Spiel mit eigener Höchstrate (Doom: 35): grün ab
/// 90 % des Ziels, gelb ab 55 %, sonst rot.
pub fn fps_mit_ziel(c: &mut Canvas, fps: f32, ziel: f32, x: i32) {
    fps_einheit(c, fps, ziel, x, 1);
}

/// Wie [`fps_mit_ziel`] auf einer Zeichenfläche mit `einheit` Bildpunkten
/// je Koordinate (Pudding-Panik: 2) – gleiche Größe und Lage in Bildpunkten.
pub fn fps_einheit(c: &mut Canvas, fps: f32, ziel: f32, x: i32, einheit: i32) {
    let e = einheit.max(1);
    let t = fmt::<16>(format_args!("{:.0} FPS", fps));
    let w = text_width(t.as_str(), 2 / e) + 12 / e;
    c.blend_rect(x / e, 14 / e, w, 24 / e, hex(0x101018));
    let farbe = if fps >= ziel * 0.9 {
        hex(0x8CFF8C)
    } else if fps >= ziel * 0.55 {
        hex(0xFFE070)
    } else {
        hex(0xFF7070)
    };
    c.text((x + 6) / e, 19 / e, t.as_str(), farbe, 2 / e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pudding_core::input::Input;

    fn tipp(x: u16, y: u16) -> Input {
        Input::touch_at(x, y)
    }

    #[test]
    fn home_loest_beim_loslassen_aus_und_verbraucht_den_tipp() {
        let mut h = HomeTaste::default();
        let r = h.update(&tipp(30, 30));
        assert!(!r.ausgeloest && r.verbraucht, "beim Drücken: noch nicht, aber nicht ins Spiel");
        let r = h.update(&Input::default());
        assert!(r.ausgeloest, "beim Loslassen");
    }

    #[test]
    fn tipp_ausserhalb_geht_ins_spiel() {
        let mut h = HomeTaste::default();
        let r = h.update(&tipp(300, 200));
        assert!(!r.ausgeloest && !r.verbraucht);
        assert!(!h.update(&Input::default()).ausgeloest);
    }

    #[test]
    fn hineinwischen_loest_nicht_aus() {
        let mut h = HomeTaste::default();
        assert!(!h.update(&tipp(200, 150)).verbraucht);
        assert!(!h.update(&tipp(30, 30)).verbraucht, "Finger kam von außen");
        assert!(!h.update(&Input::default()).ausgeloest);
    }

    #[test]
    fn herauswischen_bricht_ab() {
        let mut h = HomeTaste::default();
        assert!(h.update(&tipp(30, 30)).verbraucht);
        assert!(h.update(&tipp(250, 200)).verbraucht, "derselbe Finger bleibt verbraucht");
        assert!(!h.update(&Input::default()).ausgeloest, "außerhalb losgelassen");
    }
}
