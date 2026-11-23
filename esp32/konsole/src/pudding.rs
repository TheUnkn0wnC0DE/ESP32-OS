//! Pudding-Panik als Spiel der Konsole: dieselbe App wie bisher, die
//! Rückkehr in den Desktop steht im Hauptmenü („ZUM DESKTOP“), die FPS zeigt
//! der Rahmen wie in jedem Spiel.

use crate::{Spiel, SpielId, Wechsel};
use alloc::vec::Vec;
use pudding_core::app::{App, FrameData, Perf};
use pudding_core::audio::AudioCmd;
use pudding_core::gfx::{H, W};
use pudding_core::input::Input;
use pudding_core::save::SaveData;

/// Pudding-Panik mit Spielstand (Bytes aus dem NVS) für die Konsole.
pub fn neu(spielstand: Option<&[u8]>) -> App {
    let mut app = App::new(spielstand.and_then(SaveData::from_bytes));
    app.konsole = true;
    app
}

impl Spiel for App {
    fn update(&mut self, input: &Input, dt: f32) -> Wechsel {
        App::update(self, *input, dt);
        if self.zur_konsole() { Wechsel::Starte(SpielId::Konsole) } else { Wechsel::Bleiben }
    }

    fn render_size(&self) -> (usize, usize) {
        App::render_size(self)
    }

    fn touch_raster(&self) -> (u16, u16) {
        (W as u16, H as u16)
    }

    fn bild(&mut self, fd: &mut FrameData, perf: &Perf) {
        self.perf = *perf;
        let (w, h) = App::render_size(self);
        self.prepare_scene(w, h);
        self.build_triangles();
        self.swap_frame(fd);
    }

    fn ton(&mut self, f: &mut dyn FnMut(AudioCmd)) {
        self.drain_audio(f);
    }

    fn speichern(&mut self) -> Option<Vec<u8>> {
        self.take_save().map(|b| b.to_vec())
    }

    fn eigene_home_taste(&self) -> bool {
        true
    }

    fn helligkeit(&self) -> u8 {
        self.settings().brightness
    }

    fn glatt(&self) -> bool {
        self.settings().smooth
    }

    fn gedreht(&self) -> bool {
        self.settings().flip
    }

    fn akku(&mut self, prozent: Option<u8>) {
        self.battery = prozent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rahmen;
    use alloc::boxed::Box;
    use pudding_core::input::btn;

    fn druecke(r: &mut Rahmen, b: u8) -> Wechsel {
        let w = r.update(&Input::buttons(b), 1.0 / 55.0);
        let w2 = r.update(&Input::default(), 1.0 / 55.0);
        if w == Wechsel::Bleiben { w2 } else { w }
    }

    /// Hauptmenü → ganz unten „ZUM DESKTOP“ → das Spiel will zur Konsole.
    #[test]
    fn hauptmenue_fuehrt_zur_konsole() {
        let mut r = Rahmen::new(Box::new(neu(None)));
        assert_eq!(druecke(&mut r, btn::JUMP), Wechsel::Bleiben); // Titel → Hauptmenü
        // Ohne Spielstand: NEUES SPIEL, OPTIONEN, REKORDE, ZUM DESKTOP.
        for _ in 0..3 {
            assert_eq!(druecke(&mut r, btn::DOWN), Wechsel::Bleiben);
        }
        assert_eq!(druecke(&mut r, btn::JUMP), Wechsel::Starte(SpielId::Konsole));
        assert!(r.speichern().is_some(), "Spielstand vor dem Wechsel");
    }

    /// Spielstand aus dem NVS kommt an (Einstellungen bleiben erhalten).
    #[test]
    fn spielstand_wird_geladen() {
        let mut save = SaveData::default();
        save.settings.show_fps = false;
        save.settings.brightness = 3;
        let app = neu(Some(&save.to_bytes()));
        assert!(!app.settings().show_fps);
        assert_eq!(Spiel::helligkeit(&app), 3);
        assert_eq!(Spiel::touch_raster(&app), (320, 180));
    }

    /// Ein Bild über den Rahmen: Dreiecke sind vorbereitet, Pudding zeichnet
    /// sein HUD selbst (keine Home-Taste vom Rahmen).
    #[test]
    fn bild_kommt_in_den_bilddaten_an() {
        let mut r = Rahmen::new(Box::new(neu(None)));
        r.update(&Input::default(), 1.0 / 55.0);
        let mut fd = FrameData::new();
        r.bild(&mut fd, &Perf { fps: 55.0, ..Perf::default() });
        assert!(fd.prims() > 0, "3D-Szene vorbereitet");
    }
}
