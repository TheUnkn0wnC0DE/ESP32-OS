//! Quake als Spiel der Konsole: quakegeneric (WinQuake, GPL-2) mit der
//! Shareware-Episode, Bild 320×240 als vergrößertes Palettenbild (4:3) in
//! der Mitte, Touch-Steuerung ([`steuerung`]), Ton über den WinQuake-Mischer
//! ([`klang`]); für Messläufe ein Autopilot ([`autopilot`]).

pub mod autopilot;
pub mod ffi;
pub mod klang;
pub mod steuerung;

use autopilot::Autopilot;
use core::ffi::c_char;
use ffi::{BREITE, HOEHE};
pub use ffi::{Pak, befehl, beim_beenden, beim_bericht, beim_fehler, fehlermeldung};
use konsole::{Spiel, SpielId, Wechsel};
use pudding_core::app::{FrameData, Perf};
use pudding_core::audio::AudioCmd;
use pudding_core::gfx::{Canvas, hex};
use pudding_core::input::Input;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use steuerung::{BILD, Modus, Steuerung};

pub use klang::QuakeKlang;

pub const W: usize = 640;
pub const H: usize = 360;

/// Einstellungen für die Touch-Steuerung (Achsen des Knüppels, Maus-Blick,
/// immer rennen).
const EINSTELLUNGEN: &str = "joystick 1\njoyadvanced 1\njoyadvaxisx 3\njoyadvaxisy 1\n\
joyforwardsensitivity -1\njoysidesensitivity 1\njoyadvancedupdate\n+mlook\nlookspring 0\n\
cl_forwardspeed 400\ncl_backspeed 400\ncl_sidespeed 350\nsensitivity 5\nvolume 0.8\ngamma 0.8\n";

/// Startet einen Thread mit großem Stapel und führt `f` darin aus (Board:
/// Stapel im PSRAM). Quake legt in einigen Funktionen über 100 KB lokal an
/// und verschachtelt sie (~250 KB beim Zeichnen).
pub type Starter = fn(Box<dyn FnOnce() + Send + 'static>);

/// Standard (PC): Thread mit 4 MB Stapel.
fn std_starter(f: Box<dyn FnOnce() + Send + 'static>) {
    std::thread::Builder::new().name("quake".into()).stack_size(4 << 20).spawn(f).expect("Quake-Thread");
}

/// Stapelgröße, die ein [`Starter`] mindestens bereitstellen soll.
pub const STAPEL: usize = 512 * 1024;

pub struct Quake {
    st: Steuerung,
    modus: Modus,
    pal: [u16; 256],
    /// Quake läuft ganz im eigenen Thread: Auftrag (Zeitschritt) hin,
    /// Fertigmeldung zurück – nie gleichzeitig mit der Bildschleife.
    auftrag: SyncSender<f64>,
    fertig: Receiver<()>,
    autopilot: Option<Autopilot>,
}

impl Quake {
    /// Startet Quake (einmal je Programmlauf): Spieldaten `pak` (dazu ein mit
    /// `fopen` lesbarer Pfad `pak_datei` für die Demos), Ordner für
    /// Konfiguration und Spielstände `ordner` (ohne „/“ am Ende).
    pub fn starten(pak: Pak, pak_datei: &str, ordner: &str) -> Self {
        Self::starten_mit(pak, pak_datei, ordner, std_starter)
    }

    /// Wie [`Quake::starten`], Thread über `starter` (mindestens [`STAPEL`]).
    pub fn starten_mit(pak: Pak, pak_datei: &str, ordner: &str, starter: Starter) -> Self {
        ffi::einrichten(pak, pak_datei);
        let ordner = ordner.to_string();
        let (auftrag_tx, auftrag_rx) = sync_channel::<f64>(1);
        let (fertig_tx, fertig_rx) = sync_channel::<()>(1);
        starter(Box::new(move || {
            let args: Vec<*mut c_char> = ["quake", "-basedir", &ordner, "-nocdaudio"]
                .iter()
                .map(|a| std::ffi::CString::new(*a).expect("Argument").into_raw())
                .chain([core::ptr::null_mut()])
                .collect();
            let argv = Box::leak(args.into_boxed_slice());
            ffi::stapel_merken();
            // SAFETY: argv bleibt für immer gültig; Quake läuft nur in diesem Thread.
            unsafe { ffi::QG_Create(4, argv.as_mut_ptr()) };
            befehl(EINSTELLUNGEN);
            let _ = fertig_tx.send(());
            while let Ok(dt) = auftrag_rx.recv() {
                // SAFETY: wie oben.
                unsafe { ffi::QG_Tick(dt) };
                if fertig_tx.send(()).is_err() {
                    break;
                }
            }
        }));
        let _ = fertig_rx.recv();
        Self {
            st: Steuerung::new(),
            modus: Modus::Vorfuehrung,
            pal: ffi::gemeinsam().pal,
            auftrag: auftrag_tx,
            fertig: fertig_rx,
            autopilot: None,
        }
    }

    /// Messlauf (Board-Langlauf): Quake spielt sich selbst per simulierter
    /// Touch-Eingabe und wechselt alle 2 min Spielzeit das Level (siehe
    /// [`autopilot`]); „Screen size“ steht solange auf 120. Zeilen je
    /// Levelwechsel über [`beim_bericht`].
    pub fn autopilot(&mut self, an: bool) {
        match (an, self.autopilot.take()) {
            (true, laufend) => self.autopilot = Some(laufend.unwrap_or_else(Autopilot::starten)),
            (false, Some(a)) => a.beenden(),
            (false, None) => {}
        }
    }

    /// Simulation/Tests: Zeit läuft nur mit `update` weiter.
    pub fn virtuelle_zeit() {
        ffi::VIRTUELL.store(true, Relaxed);
    }

    pub fn modus(&self) -> Modus {
        self.modus
    }

    /// Gezeichnete Quake-Bilder seit dem Start.
    pub fn bilder(&self) -> u32 {
        ffi::gemeinsam().bilder
    }
}

impl Spiel for Quake {
    fn update(&mut self, input: &Input, dt: f32) -> Wechsel {
        let dt = dt.clamp(0.001, 0.1);
        if ffi::VIRTUELL.load(Relaxed) {
            ffi::ZEIT_US.fetch_add((dt * 1e6) as u32, Relaxed);
        }
        // SAFETY: reine Abfrage.
        self.modus = Modus::aus_c(unsafe { ffi::QG_Modus() });
        let bot;
        let input = match &mut self.autopilot {
            Some(a) => {
                bot = a.schritt(input, self.modus, dt);
                &bot
            }
            None => input,
        };
        let a = self.st.update(input, self.modus, dt);
        {
            let mut g = ffi::gemeinsam();
            g.tasten.extend(a.tasten);
            g.maus.0 += a.maus.0;
            g.maus.1 += a.maus.1;
            g.achsen = a.achsen;
        }
        // Ein Schritt im Quake-Thread; währenddessen wartet die Bildschleife.
        if self.auftrag.send(dt as f64).is_ok() {
            let _ = self.fertig.recv();
        }
        let g = ffi::gemeinsam();
        self.pal = g.pal;
        if g.beendet { Wechsel::Starte(SpielId::Konsole) } else { Wechsel::Bleiben }
    }

    fn render_size(&self) -> (usize, usize) {
        (W, H)
    }

    fn bild(&mut self, fd: &mut FrameData, _perf: &Perf) {
        let st = &self.st;
        fd.flach(W, H, &rand_farbe, &mut |c: &mut Canvas| {
            let (x, y, w, h) = BILD;
            c.fill_rect(x - 2, y, 2, h, hex(0x262028));
            c.fill_rect(x + w, y, 2, h, hex(0x262028));
            st.zeichne(c);
        });
        let (bild, nummer) = {
            let g = ffi::gemeinsam();
            (g.bild, g.bilder)
        };
        if bild != 0 {
            // SAFETY: Quakes Bildpuffer (statisch), beschrieben nur in `QG_Tick` (gleicher Kern).
            let px = unsafe { core::slice::from_raw_parts(bild as *const u8, BREITE * HOEHE) };
            fd.palettenbild(px, BREITE, HOEHE, &self.pal, BILD, nummer as u64);
        }
    }

    fn ton(&mut self, _f: &mut dyn FnMut(AudioCmd)) {}

    fn speichern(&mut self) -> Option<Vec<u8>> {
        None
    }

    /// Quake braucht je Bild 20–40 ms: fester Takt (27,5 Bilder/s) wirkt
    /// gleichmäßiger als schwankende 25–48.
    fn bildrate_teiler(&self) -> u32 {
        2
    }
}

fn rand_farbe(y: usize) -> u16 {
    let t = y as u32 * 12 / H as u32;
    let (r, g, b) = (12 + t, 9 + t / 2, 8 + t / 3);
    (((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Ganzer Ablauf mit der echten PAK (falls vorhanden): Vorführung →
    /// Menü per Tipp → Bild kommt an. Ein Test: Quake gibt es nur einmal je Prozess.
    #[test]
    fn quake_startet_und_zeigt_bilder() {
        let pfad = Path::new(env!("CARGO_MANIFEST_DIR")).join("pak/pak0.pak");
        let Ok(pak) = std::fs::read(&pfad) else {
            eprintln!("pak0.pak fehlt – Test übersprungen");
            return;
        };
        let ordner = std::env::temp_dir().join("quake_konsole_test");
        std::fs::create_dir_all(ordner.join("id1")).unwrap();
        Quake::virtuelle_zeit();
        let mut q = Quake::starten(Pak::Speicher(Box::leak(pak.into_boxed_slice())), pfad.to_str().unwrap(), ordner.to_str().unwrap());
        let dt = 1.0 / 30.0;
        for _ in 0..90 {
            q.update(&Input::default(), dt);
        }
        assert!(q.bilder() > 30, "Quake zeichnet ({})", q.bilder());
        let mut fd = FrameData::new();
        q.bild(&mut fd, &Perf::default());
        // Antippen öffnet das Menü.
        q.update(&Input::touch_at(320, 180), dt);
        for _ in 0..10 {
            q.update(&Input::default(), dt);
        }
        assert_eq!(q.modus(), Modus::Menue);
        // Bilder für die Durchsicht (nur mit QUAKE_BILDER=Ordner).
        let Ok(ziel) = std::env::var("QUAKE_BILDER") else { return };
        let ziel = Path::new(&ziel);
        std::fs::create_dir_all(ziel).unwrap();
        let mut r = konsole::Rahmen::new(Box::new(q));
        let warte = |r: &mut konsole::Rahmen, n: usize, i: Input| {
            for _ in 0..n {
                r.update(&i, dt);
            }
        };
        let zwei = |a: (u16, u16), b: (u16, u16)| Input { held: 0, touch: [Some(a), Some(b)] };
        bild_png(&mut r, &ziel.join("2_menue.png"));
        for _ in 0..2 {
            warte(&mut r, 1, Input::touch_at(600, 304));
            warte(&mut r, 15, Input::default());
        }
        warte(&mut r, 90, Input::default());
        warte(&mut r, 1, zwei((150, 250), (420, 200)));
        for i in 0..30u16 {
            warte(&mut r, 1, zwei((150, 205), (420 + i * 2, 200)));
        }
        warte(&mut r, 1, Input::touch_at(150, 205));
        warte(&mut r, 8, zwei((150, 205), (600, 310)));
        bild_png(&mut r, &ziel.join("3_spiel.png"));
    }

    fn bild_png(r: &mut konsole::Rahmen, pfad: &Path) {
        use pudding_core::render::{Scratch, Target};
        let (w, h) = r.render_size();
        let mut fd = FrameData::new();
        r.bild(&mut fd, &Perf { fps: 30.0, ..Perf::default() });
        let (mut farbe, mut tiefe) = (vec![0u16; w * h], vec![0u16; w * h]);
        let mut s = Scratch::default();
        let mut rest = Target::new(&mut farbe, &mut tiefe, w, h);
        for y in (8..h).step_by(8).chain([h]) {
            let (mut band, tail) = rest.split(y);
            fd.render_band(&mut s, &mut band);
            rest = tail;
        }
        let rgb: Vec<u32> = farbe.iter().map(|&c| pudding_core::gfx::to_rgb888(c)).collect();
        std::fs::write(pfad, spatz::image::encode_png(w, h, &rgb)).unwrap();
    }
}
