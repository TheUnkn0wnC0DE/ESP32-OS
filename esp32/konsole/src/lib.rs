//! Spielekonsole: gemeinsamer Rahmen für alle Spiele auf dem Board.

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod kunst;
pub mod launcher;
pub mod pudding;
pub mod start;
pub mod ui;

use alloc::boxed::Box;
use alloc::vec::Vec;
use pudding_core::app::{FrameData, Perf};
use pudding_core::audio::AudioCmd;
use pudding_core::input::{Input, MAX_TOUCH};

/// Was nach einem Spielschritt passieren soll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wechsel {
    Bleiben,
    /// Dieses Spiel (bzw. das Konsolen-Menü) starten – auf dem Board per Neustart.
    Starte(SpielId),
}

/// Ein Spiel der Konsole. Die Firmware ruft je Bild `update` (fester oder
/// gemessener Zeitschritt) und `bild` (Bild in `fd` vorbereiten; `fd` muss
/// danach unabhängig vom Spielzustand sein – es wird gezeichnet, während
/// das Spiel weiterläuft).
pub trait Spiel {
    fn update(&mut self, input: &Input, dt: f32) -> Wechsel;
    /// Renderauflösung (quer), z. B. 640×360.
    fn render_size(&self) -> (usize, usize);
    /// Raster der Touch-Koordinaten in `Input` (Standard: Renderauflösung).
    fn touch_raster(&self) -> (u16, u16) {
        let (w, h) = self.render_size();
        (w as u16, h as u16)
    }
    fn bild(&mut self, fd: &mut FrameData, perf: &Perf);
    /// Tonbefehle dieses Bildes abholen.
    fn ton(&mut self, f: &mut dyn FnMut(AudioCmd));
    /// Spielstand, falls er gespeichert werden soll.
    fn speichern(&mut self) -> Option<Vec<u8>>;
    /// Das Spiel bringt eine eigene Rückkehr zur Konsole mit (Menüeintrag)
    /// bzw. ist selbst die Konsole – dann keine Home-Taste vom Rahmen.
    fn eigene_home_taste(&self) -> bool {
        false
    }
    /// Das Spiel zeichnet seine FPS-Anzeige selbst.
    fn eigene_fps(&self) -> bool {
        false
    }
    /// Display-Helligkeit 1..=10.
    fn helligkeit(&self) -> u8 {
        8
    }
    /// Vergrößern weich (bilinear, PPA) statt scharf.
    fn glatt(&self) -> bool {
        true
    }
    /// Bild (und Touch) um 180° gedreht.
    fn gedreht(&self) -> bool {
        false
    }
    /// Akkustand in Prozent, falls die Firmware ihn kennt (alle ~2 s).
    fn akku(&mut self, _prozent: Option<u8>) {}
    /// Sekunden ohne Eingabe, bis das Hintergrundlicht ausgeht (None = nie).
    fn bildschirm_aus_s(&self) -> Option<u32> {
        Some(300)
    }
    /// Sekunden nach dem Abschalten des Lichts bis zum Ruhezustand
    /// (Panel schläft, Schleife gedrosselt; None = nie).
    fn ruhe_nach_s(&self) -> Option<u32> {
        None
    }
    /// Ruhezustand sofort (z. B. Menüeintrag); einmal true je Wunsch.
    fn ruhe_jetzt(&mut self) -> bool {
        false
    }
    /// Bildschirm an lassen bzw. aufwecken, ohne die Eingabe zu sperren
    /// (z. B. Wecker klingelt); zählt wie eine Eingabe für Schoner/Ruhe.
    fn wach_halten(&mut self) -> bool {
        false
    }
    /// Eigene Eingabequelle des Spiels (Touch-Autopilot): einen Schritt
    /// vorrücken; true = ein Finger liegt. Die Firmware ruft das in jedem
    /// Durchlauf (auch im Ruhezustand) und zählt es für Schoner/Aufwecken.
    fn fremde_eingabe(&mut self) -> bool {
        false
    }
    /// Die laufende Berührung dient nur dem Aufwecken (nicht weitergeben).
    fn eingabe_sperren(&mut self, _an: bool) {}
    /// Vor dem Neustart in ein anderes Spiel (Einstellungen sichern o. ä.).
    fn beenden(&mut self) {}
    /// Jedes Bild so viele Bildwechsel des Displays lang zeigen (1 = volle
    /// Bildrate). Ein Spiel, das die volle Rate nicht schafft, wirkt mit
    /// festem Takt (2 = halbe Rate) gleichmäßiger als mit schwankendem.
    fn bildrate_teiler(&self) -> u32 {
        1
    }
    /// Das zuletzt vorbereitete Bild ist gleich dem angezeigten (ESP32-OS-
    /// Desktop ohne Änderung): die Firmware zeichnet dann nichts neu.
    fn unveraendert(&self) -> bool {
        false
    }
}

/// Rahmen um jedes Spiel: Home-Taste (Finger auf der Taste erreicht das
/// Spiel nie) und FPS-Anzeige über jedem Bild.
pub struct Rahmen {
    spiel: Box<dyn Spiel>,
    home: ui::HomeTaste,
    #[cfg(test)]
    weitergegeben: usize,
}

impl Rahmen {
    pub fn new(spiel: Box<dyn Spiel>) -> Self {
        Self {
            spiel,
            home: ui::HomeTaste::default(),
            #[cfg(test)]
            weitergegeben: 0,
        }
    }

    pub fn update(&mut self, input: &Input, dt: f32) -> Wechsel {
        if self.spiel.eigene_home_taste() {
            return self.weiter(*input, dt);
        }
        let h = self.home.update(input);
        let fuer_spiel = if h.verbraucht { Input { touch: [None; MAX_TOUCH], ..*input } } else { *input };
        let w = self.weiter(fuer_spiel, dt);
        if h.ausgeloest { Wechsel::Starte(SpielId::Konsole) } else { w }
    }

    fn weiter(&mut self, input: Input, dt: f32) -> Wechsel {
        #[cfg(test)]
        {
            self.weitergegeben += input.touching() as usize;
        }
        self.spiel.update(&input, dt)
    }

    #[cfg(test)]
    fn beruehrungen_im_spiel(&self) -> usize {
        self.weitergegeben
    }

    pub fn bild(&mut self, fd: &mut FrameData, perf: &Perf) {
        self.spiel.bild(fd, perf);
        let home = !self.spiel.eigene_home_taste();
        let fps = !self.spiel.eigene_fps();
        if home || fps {
            let gedrueckt = self.home.gedrueckt();
            let einheit = fd.overlay_massstab() as i32;
            let teiler = self.spiel.bildrate_teiler().max(1);
            fd.dazu(&mut |c| {
                if home {
                    ui::home_taste(c, gedrueckt);
                }
                if fps {
                    ui::fps_einheit(c, perf.fps, 55.0 / teiler as f32, if home { ui::FPS_X } else { 8 }, einheit);
                }
            });
        }
    }

    pub fn render_size(&self) -> (usize, usize) {
        self.spiel.render_size()
    }

    pub fn touch_raster(&self) -> (u16, u16) {
        self.spiel.touch_raster()
    }

    pub fn ton(&mut self, f: &mut dyn FnMut(AudioCmd)) {
        self.spiel.ton(f);
    }

    pub fn speichern(&mut self) -> Option<Vec<u8>> {
        self.spiel.speichern()
    }

    pub fn helligkeit(&self) -> u8 {
        self.spiel.helligkeit()
    }

    pub fn glatt(&self) -> bool {
        self.spiel.glatt()
    }

    pub fn gedreht(&self) -> bool {
        self.spiel.gedreht()
    }

    pub fn akku(&mut self, prozent: Option<u8>) {
        self.spiel.akku(prozent);
    }

    pub fn bildschirm_aus_s(&self) -> Option<u32> {
        self.spiel.bildschirm_aus_s()
    }

    pub fn ruhe_nach_s(&self) -> Option<u32> {
        self.spiel.ruhe_nach_s()
    }

    pub fn ruhe_jetzt(&mut self) -> bool {
        self.spiel.ruhe_jetzt()
    }

    pub fn wach_halten(&mut self) -> bool {
        self.spiel.wach_halten()
    }

    pub fn fremde_eingabe(&mut self) -> bool {
        self.spiel.fremde_eingabe()
    }

    pub fn eingabe_sperren(&mut self, an: bool) {
        self.spiel.eingabe_sperren(an);
    }

    pub fn beenden(&mut self) {
        self.spiel.beenden();
    }

    pub fn bildrate_teiler(&self) -> u32 {
        self.spiel.bildrate_teiler().max(1)
    }

    /// Siehe [`Spiel::unveraendert`].
    pub fn unveraendert(&self) -> bool {
        self.spiel.unveraendert()
    }
}

/// Die Spiele der Konsole. Die Zahl (`als_u8`) steht im NVS als Startziel –
/// Reihenfolge nicht ändern, nur hinten anfügen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpielId {
    Konsole,
    Pudding,
    Doom,
    Wuerfelwelt,
    Quake,
}

impl SpielId {
    pub const ALLE: [SpielId; 5] = [SpielId::Konsole, SpielId::Pudding, SpielId::Doom, SpielId::Wuerfelwelt, SpielId::Quake];
    /// Die Spiele im Menü (ohne die Konsole selbst).
    pub const SPIELE: [SpielId; 4] = [SpielId::Pudding, SpielId::Doom, SpielId::Quake, SpielId::Wuerfelwelt];

    pub fn als_u8(self) -> u8 {
        self as u8
    }

    pub fn aus_u8(v: u8) -> Option<Self> {
        Self::ALLE.get(v as usize).copied()
    }

    /// Schlüssel des Spielstands im NVS (Namensraum „pudding“); Pudding-Panik
    /// behält seinen bisherigen Schlüssel.
    pub fn nvs_schluessel(self) -> &'static str {
        match self {
            SpielId::Konsole => "konsole",
            SpielId::Pudding => "save",
            SpielId::Doom => "doom",
            SpielId::Wuerfelwelt => "wuerfelwelt",
            SpielId::Quake => "quake",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use alloc::vec;
    use pudding_core::app::FrameData;
    use pudding_core::render::{Scratch, Target};
    use pudding_core::app::Perf;
    use pudding_core::audio::AudioCmd;
    use pudding_core::input::Input;

    #[test]
    fn schluessel_sind_eindeutig() {
        let mut gesehen = BTreeSet::new();
        for id in SpielId::ALLE {
            let k = id.nvs_schluessel();
            assert!(!k.is_empty() && k.len() <= 15, "NVS-Schlüssel {k:?}");
            assert!(gesehen.insert(k), "doppelt: {k}");
        }
    }

    #[test]
    fn ids_hin_und_zurueck() {
        for id in SpielId::ALLE {
            assert_eq!(SpielId::aus_u8(id.als_u8()), Some(id));
        }
        assert_eq!(SpielId::aus_u8(200), None);
    }

    fn zeichnen(fd: &FrameData, w: usize, h: usize) -> alloc::vec::Vec<u16> {
        let (mut farbe, mut tiefe) = (vec![0u16; w * h], vec![0u16; w * h]);
        let mut s = Scratch::default();
        let mut rest = Target::new(&mut farbe, &mut tiefe, w, h);
        for y in (8..h).step_by(8).chain([h]) {
            let (mut band, tail) = rest.split(y);
            fd.render_band(&mut s, &mut band);
            rest = tail;
        }
        farbe
    }

    /// Spiel-Attrappe: merkt sich die Berührungen, zeichnet Grau.
    #[derive(Default)]
    struct Attrappe {
        beruehrungen: usize,
        eigene_home: bool,
    }

    impl Spiel for Attrappe {
        fn update(&mut self, input: &Input, _dt: f32) -> Wechsel {
            self.beruehrungen += input.touching() as usize;
            Wechsel::Bleiben
        }
        fn render_size(&self) -> (usize, usize) {
            (640, 360)
        }
        fn bild(&mut self, fd: &mut FrameData, _perf: &Perf) {
            fd.flach(640, 360, &|_| 0x4208, &mut |_| {});
        }
        fn ton(&mut self, _f: &mut dyn FnMut(AudioCmd)) {}
        fn speichern(&mut self) -> Option<alloc::vec::Vec<u8>> {
            None
        }
        fn eigene_home_taste(&self) -> bool {
            self.eigene_home
        }
    }

    #[test]
    fn dazu_zeichnet_ueber_das_flache_bild() {
        let mut fd = FrameData::new();
        fd.flach(640, 360, &|_| 1, &mut |_| {});
        fd.dazu(&mut |c| c.fill_rect(300, 100, 10, 10, 0x07E0));
        let farbe = zeichnen(&fd, 640, 360);
        assert_eq!(farbe[105 * 640 + 305], 0x07E0);
        assert_eq!(farbe[200 * 640 + 305], 1);
    }

    #[test]
    fn home_tipp_geht_nicht_ins_spiel_und_fuehrt_zur_konsole() {
        let mut r = Rahmen::new(alloc::boxed::Box::new(Attrappe::default()));
        assert_eq!(r.update(&Input::touch_at(30, 30), 0.02), Wechsel::Bleiben);
        assert_eq!(r.update(&Input::default(), 0.02), Wechsel::Starte(SpielId::Konsole));
        let mut r = Rahmen::new(alloc::boxed::Box::new(Attrappe::default()));
        r.update(&Input::touch_at(30, 30), 0.02);
        r.update(&Input::touch_at(400, 200), 0.02);
        assert_eq!(r.beruehrungen_im_spiel(), 0, "Home-Finger erreicht das Spiel nie");
    }

    #[test]
    fn tipp_ins_spiel_kommt_an() {
        let mut r = Rahmen::new(alloc::boxed::Box::new(Attrappe::default()));
        r.update(&Input::touch_at(400, 200), 0.02);
        assert_eq!(r.beruehrungen_im_spiel(), 1);
    }

    #[test]
    fn spiel_mit_eigener_home_taste_bekommt_alles() {
        let mut r = Rahmen::new(alloc::boxed::Box::new(Attrappe { eigene_home: true, ..Default::default() }));
        assert_eq!(r.update(&Input::touch_at(30, 30), 0.02), Wechsel::Bleiben);
        assert_eq!(r.update(&Input::default(), 0.02), Wechsel::Bleiben);
        assert_eq!(r.beruehrungen_im_spiel(), 1);
    }

    #[test]
    fn rahmen_zeichnet_fps_und_home_dazu() {
        let mut r = Rahmen::new(alloc::boxed::Box::new(Attrappe::default()));
        let mut fd = FrameData::new();
        r.bild(&mut fd, &Perf { fps: 55.0, ..Perf::default() });
        let farbe = zeichnen(&fd, 640, 360);
        let anders = |x0: usize, x1: usize, y0: usize, y1: usize| {
            (y0..y1).any(|y| (x0..x1).any(|x| farbe[y * 640 + x] != 0x4208))
        };
        assert!(anders(8, 52, 8, 52), "Home-Taste");
        assert!(anders(60, 160, 14, 38), "FPS");
        assert!(!anders(200, 600, 100, 300), "Spielfläche unberührt");
    }

    #[test]
    fn flaches_bild_zeichnet_himmel_und_overlay() {
        let (w, h) = (640, 360);
        let mut fd = FrameData::new();
        fd.flach(w, h, &|y| y as u16, &mut |c| c.fill_rect(100, 50, 20, 10, 0xF800));
        let (mut farbe, mut tiefe) = (vec![0u16; w * h], vec![0u16; w * h]);
        let mut s = Scratch::default();
        let mut rest = Target::new(&mut farbe, &mut tiefe, w, h);
        for y in (8..h).step_by(8).chain([h]) {
            let (mut band, tail) = rest.split(y);
            fd.render_band(&mut s, &mut band);
            rest = tail;
        }
        assert_eq!(farbe[10 * w + 5], 10, "Himmel der Zeile 10");
        assert_eq!(farbe[55 * w + 110], 0xF800, "im Rechteck");
        assert_eq!(farbe[200 * w + 300], 200, "Himmel der Zeile 200");
    }
}
