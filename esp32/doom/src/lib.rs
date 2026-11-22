//! Doom als Spiel der Konsole: doomgeneric (Chocolate Doom, GPL-2) mit der
//! Shareware-WAD, Bild als vergrößertes Palettenbild in der Mitte (4:3),
//! Touch-Steuerung ([`steuerung`]), Ton über den eigenen Mischer
//! ([`klang`]), FPS-Anzeige wie in jedem Spiel der Konsole.
//!
//! Doom läuft im Takt seiner Tics (35 je Sekunde): Ein Bild der Konsole
//! (55 Hz) ruft `doomgeneric_Tick` nur, wenn ein neuer Tic fällig ist –
//! sonst bleibt das letzte Doom-Bild stehen und Kern 0 hilft beim Zeichnen.

pub mod ffi;
pub mod klang;
pub mod musik;
pub mod opl;
pub mod steuerung;

use core::ffi::c_char;
use ffi::{BREITE, HOEHE};
use konsole::{Spiel, SpielId, Wechsel, ui};
use pudding_core::app::{FrameData, Perf};
use pudding_core::audio::AudioCmd;
use pudding_core::gfx::{Canvas, hex};
use pudding_core::input::Input;
use std::sync::atomic::Ordering::Relaxed;
use steuerung::{BILD, Modus, Steuerung, taste};

pub use klang::DoomKlang;

/// Raster der Konsole.
pub const W: usize = 640;
pub const H: usize = 360;

pub struct Doom {
    st: Steuerung,
    modus: Modus,
    letzter_tic: i32,
    /// Doom-Bilder je Sekunde (gemessen über ~0,5 s).
    fps: f32,
    fps_zeit: f32,
    fps_bilder: u32,
    name_getippt: bool,
    pal: [u16; 256],
}

/// Doom-Palette → RGB565.
fn palette() -> [u16; 256] {
    let mut p = [0u16; 256];
    // SAFETY: `colors` ist ein festes C-Feld; Doom schreibt es nur während
    // `doomgeneric_Tick` auf demselben Kern.
    let c = unsafe { &*core::ptr::addr_of!(ffi::colors) };
    for (o, &v) in p.iter_mut().zip(c) {
        let (r, g, b) = ((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF);
        *o = (((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)) as u16;
    }
    p
}

impl Doom {
    /// Startet Doom (einmal je Programmlauf): `wad` = DOOM1.WAD im Speicher,
    /// `ordner` = Ordner für Konfiguration und Spielstände (mit „/“ am Ende).
    /// Lädt die WAD und zeigt das erste Bild – dauert einen Moment.
    pub fn starten(wad: &'static [u8], ordner: &str) -> Self {
        ffi::einrichten(wad, ordner);
        let args: Vec<*mut c_char> = ["doom", "-iwad", "doom1.wad"]
            .iter()
            .map(|a| std::ffi::CString::new(*a).expect("Argument").into_raw())
            .chain([core::ptr::null_mut()])
            .collect();
        let argv = Box::leak(args.into_boxed_slice());
        // SAFETY: argv bleibt für immer gültig (Doom merkt sich die Zeiger).
        unsafe {
            ffi::doomgeneric_Create(3, argv.as_mut_ptr());
            ffi::DG_Einstellen();
        }
        Self {
            st: Steuerung::new(),
            modus: Modus::Zwischen,
            // SAFETY: reine Abfrage.
            letzter_tic: unsafe { ffi::I_GetTime() },
            fps: 0.0,
            fps_zeit: 0.0,
            fps_bilder: ffi::gemeinsam().bilder,
            name_getippt: false,
            pal: palette(),
        }
    }

    /// Simulation/Tests: Doom-Zeit läuft nur mit `update` weiter.
    pub fn virtuelle_zeit() {
        ffi::VIRTUELL.store(true, Relaxed);
    }

    /// Aktueller Modus (Spiel, Menü, …).
    pub fn modus(&self) -> Modus {
        self.modus
    }

    fn taste(&mut self, unten: bool, k: u8) {
        ffi::gemeinsam().tasten.push_back((unten, k));
    }

    fn druecken(&mut self, k: u8) {
        self.taste(true, k);
        self.taste(false, k);
    }

    /// Spielstand benennen: leerer Name → Episode/Karte eintippen, dann
    /// bestätigen (ein Tipp auf den Platz speichert sofort).
    fn name_eingeben(&mut self) {
        if self.modus != Modus::Name {
            self.name_getippt = false;
            return;
        }
        if self.name_getippt {
            return;
        }
        self.name_getippt = true;
        // SAFETY: reine Abfragen.
        if unsafe { ffi::DG_SpeicherNameLeer() } != 0 {
            let l = unsafe { ffi::DG_Level() };
            let name = format!("e{}m{}", l / 10, l % 10);
            for b in name.bytes() {
                self.druecken(b);
            }
        }
        self.druecken(taste::ENTER);
    }
}

impl Spiel for Doom {
    fn update(&mut self, input: &Input, dt: f32) -> Wechsel {
        if ffi::VIRTUELL.load(Relaxed) {
            ffi::ZEIT_MS.fetch_add((dt * 1000.0).round() as u32, Relaxed);
        }
        // SAFETY: reine Abfrage.
        self.modus = Modus::aus_c(unsafe { ffi::DG_Modus() });
        let a = self.st.update(input, self.modus, dt);
        for (unten, k) in a.tasten {
            self.taste(unten, k);
        }
        for (x, y) in a.tipps {
            match self.modus {
                // SAFETY: Doom läuft auf diesem Kern, nicht gleichzeitig.
                Modus::Menue => match unsafe { ffi::DG_MenueTipp(x, y) } {
                    1 => self.druecken(taste::ENTER),
                    2 => self.druecken(taste::LINKS),
                    3 => self.druecken(taste::RECHTS),
                    _ => {}
                },
                Modus::Meldung => self.druecken(taste::ENTER),
                _ => {}
            }
        }
        self.name_eingeben();

        // Neuer Tic fällig? Sonst bleibt das letzte Bild stehen.
        // SAFETY: reine Abfrage.
        let tic = unsafe { ffi::I_GetTime() };
        if tic != self.letzter_tic {
            self.letzter_tic = tic;
            if self.modus == Modus::Spiel {
                let (dx, dy) = (self.st.dreh_nehmen(), self.st.lauf());
                // SAFETY: Doom-Ereignis auf dem Doom-Kern.
                unsafe { ffi::DG_Maus(dx, dy) };
            }
            // SAFETY: Doom läuft nur hier (Spiel-Kern).
            unsafe { ffi::doomgeneric_Tick() };
            self.pal = palette();
        }

        let (bilder, beendet) = {
            let g = ffi::gemeinsam();
            (g.bilder, g.beendet)
        };
        self.fps_zeit += dt;
        if self.fps_zeit >= 0.5 {
            self.fps = bilder.wrapping_sub(self.fps_bilder) as f32 / self.fps_zeit;
            self.fps_bilder = bilder;
            self.fps_zeit = 0.0;
        }
        if beendet { Wechsel::Starte(SpielId::Konsole) } else { Wechsel::Bleiben }
    }

    fn render_size(&self) -> (usize, usize) {
        (W, H)
    }

    fn bild(&mut self, fd: &mut FrameData, _perf: &Perf) {
        let st = &self.st;
        let fps = self.fps;
        fd.flach(W, H, &rand_farbe, &mut |c: &mut Canvas| {
            rahmen_zeichnen(c);
            st.zeichne(c);
            ui::fps_mit_ziel(c, fps, 35.0, ui::FPS_X);
        });
        // SAFETY: Doom schreibt den Puffer nur in `doomgeneric_Tick` (gleicher Kern).
        let px = unsafe { core::slice::from_raw_parts(ffi::DG_ScreenBuffer, BREITE * HOEHE) };
        let nummer = ffi::gemeinsam().bilder as u64;
        fd.palettenbild(px, BREITE, HOEHE, &self.pal, BILD, nummer);
    }

    fn ton(&mut self, _f: &mut dyn FnMut(AudioCmd)) {}

    fn speichern(&mut self) -> Option<Vec<u8>> {
        None
    }

    fn eigene_fps(&self) -> bool {
        true
    }

    fn beenden(&mut self) {
        // SAFETY: Doom läuft auf diesem Kern; schreibt default.cfg.
        unsafe { ffi::M_SaveDefaults() };
    }
}

/// Hintergrund der Ränder neben dem Doom-Bild: dunkler Verlauf.
fn rand_farbe(y: usize) -> u16 {
    let t = y as u32 * 12 / H as u32;
    let (r, g, b) = (10 + t, 8 + t / 2, 12 + t / 2);
    (((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)) as u16
}

/// Feine Kanten um das Doom-Bild.
fn rahmen_zeichnen(c: &mut Canvas) {
    let (x, y, w, h) = BILD;
    c.fill_rect(x - 2, y, 2, h, hex(0x2A2026));
    c.fill_rect(x + w, y, 2, h, hex(0x2A2026));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Ganzer Ablauf mit der echten WAD (falls vorhanden): Titel → Menü per
    /// Tipp → neues Spiel → laufen, schießen. Ein Test: Doom gibt es nur
    /// einmal je Prozess.
    #[test]
    fn doom_startet_und_laesst_sich_per_touch_bedienen() {
        let pfad = Path::new(env!("CARGO_MANIFEST_DIR")).join("wad/DOOM1.WAD");
        let Ok(wad) = std::fs::read(&pfad) else {
            eprintln!("DOOM1.WAD fehlt ({}) – Test übersprungen", pfad.display());
            return;
        };
        let ordner = std::env::temp_dir().join("doom_konsole_test/");
        std::fs::create_dir_all(&ordner).unwrap();
        Doom::virtuelle_zeit();
        let mut d = Doom::starten(Box::leak(wad.into_boxed_slice()), ordner.to_str().unwrap());
        let dt = 1.0 / 55.0;
        let schritt = |d: &mut Doom, i: Input, n: usize| {
            for _ in 0..n {
                d.update(&i, dt);
            }
        };
        schritt(&mut d, Input::default(), 55);
        assert_eq!(d.modus(), Modus::Zwischen, "Titel/Demo");
        assert!(d.fps > 30.0 && d.fps < 37.0, "Doom zeichnet ~35 Bilder/s ({})", d.fps);
        // Antippen: Menü geht auf.
        schritt(&mut d, Input::touch_at(320, 180), 1);
        schritt(&mut d, Input::default(), 20);
        assert_eq!(d.modus(), Modus::Menue);
        // „New Game“ (erster Eintrag, Doom-y 64..80 → Raster-y ~124).
        for _ in 0..3 {
            schritt(&mut d, Input::touch_at(320, 128), 1);
            schritt(&mut d, Input::default(), 20);
        }
        assert_eq!(d.modus(), Modus::Spiel, "Episode und Schwierigkeit gewählt");
        schritt(&mut d, Input::default(), 40);
        let mut fd = FrameData::new();
        d.bild(&mut fd, &Perf::default());
    }
}
