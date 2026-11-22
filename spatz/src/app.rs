//! App-Runner: Spielschleife mit festem Simulationstakt, Render-Interpolation,
//! Zeitmessung pro Phase, eingebautem Performance-Overlay und Hotkeys.
//!
//! Engine-Tasten: `F1` Overlay, `F2` VSync, `F11`/`Alt+Enter` Vollbild.

use crate::input::{Input, Key};
use crate::platform::Platform;
use crate::render::overlay::{PerfOverlay, fmt};
use crate::render::{Framebuffer, RenderStats};
use crate::time::{FixedTimestep, FrameStats};
use std::string::String;
use std::time::{Duration, Instant};

/// Konfiguration des Runners.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub title: String,
    pub width: usize,
    pub height: usize,
    /// Simulationsfrequenz in Hz.
    pub sim_hz: f32,
    pub vsync: bool,
    pub show_perf: bool,
    /// Bildratenbegrenzung ohne VSync (0 = unbegrenzt).
    pub fps_cap: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { title: "spatz".into(), width: 480, height: 270, sim_hz: 120.0, vsync: true, show_perf: true, fps_cap: 0 }
    }
}

/// Steuerbefehle vom Spiel an den Runner.
#[derive(Clone, Copy, Debug, Default)]
pub struct Control {
    pub quit: bool,
    /// Neue interne Auflösung anfordern.
    pub resize: Option<(usize, usize)>,
}

/// Ein Spiel für den Runner.
pub trait Game {
    /// Ein fester Simulationsschritt der Länge `dt`.
    fn update(&mut self, input: &Input, dt: f32, ctl: &mut Control);
    /// Zeichnet den Zustand, interpoliert mit `alpha` (0..1) zwischen den
    /// letzten beiden Simulationsschritten.
    fn render(&mut self, fb: &mut Framebuffer, alpha: f32) -> RenderStats;
    /// Optionaler Fenstertitel-Zusatz.
    fn title_suffix(&self) -> Option<&str> {
        None
    }
}

/// Startet die Spielschleife, bis Fenster geschlossen oder `ctl.quit` gesetzt.
pub fn run<P: Platform, G: Game>(platform: &mut P, game: &mut G, cfg: &AppConfig) {
    let mut fb = Framebuffer::new(cfg.width, cfg.height);
    let mut input = Input::new();
    let mut ts = FixedTimestep::new(cfg.sim_hz);
    let mut stats = FrameStats::new();
    let mut ctl = Control::default();
    let mut show_perf = cfg.show_perf;
    let mut vsync = cfg.vsync;
    let mut prev_keys = [false; 3];
    let mut overlay = PerfOverlay::new();
    // Texte nur neu formatieren, wenn sich ihr Inhalt ändern kann.
    let mut mode_key = None;
    let mut mode = fmt::<32>(format_args!(""));
    let mut title_key = None;
    let mut last = Instant::now();

    loop {
        let frame_start = Instant::now();
        let dt = (frame_start - last).as_secs_f32();
        last = frame_start;

        if !platform.pump(&mut input) {
            break;
        }
        // Engine-Hotkeys mit eigener Flankenerkennung (unabhängig vom Simulationstakt).
        let keys = [input.down(Key::F1), input.down(Key::F2), input.down(Key::F11)];
        let edge = |i: usize| keys[i] && !prev_keys[i];
        if edge(0) {
            show_perf = !show_perf;
        }
        if edge(1) {
            vsync = !vsync;
        }
        if edge(2) {
            platform.toggle_fullscreen();
        }
        prev_keys = keys;

        let t0 = Instant::now();
        for _ in 0..ts.advance(dt) {
            game.update(&input, ts.step, &mut ctl);
            input.end_step();
        }
        if ctl.quit {
            break;
        }
        if let Some((w, h)) = ctl.resize.take() {
            fb.resize(w, h);
        }
        let t1 = Instant::now();
        let rstats = game.render(&mut fb, ts.alpha());
        if show_perf {
            let key = (fb.width(), fb.height(), vsync);
            if mode_key != Some(key) {
                mode = fmt::<32>(format_args!(
                    "{}X{} VSYNC {} (F2)",
                    fb.width(),
                    fb.height(),
                    if vsync { "AN" } else { "AUS" }
                ));
                mode_key = Some(key);
            }
            overlay.draw(&mut fb, &stats, &rstats, &[mode.as_str()]);
        }
        let t2 = Instant::now();
        platform.present(&fb);
        let t3 = Instant::now();

        // Fenstertitel: nur bei geänderter Statistik (≈4×/s) neu formatieren.
        let s = stats.summary();
        if title_key != Some(s) {
            let title = fmt::<128>(format_args!(
                "{}  |  {:.0} FPS  {:.2} ms  (CPU {:.2} ms){}",
                cfg.title,
                s.fps,
                s.avg_ms,
                s.cpu_ms,
                game.title_suffix().unwrap_or("")
            ));
            platform.set_title(title.as_str());
            title_key = Some(s);
        }

        if vsync {
            if !platform.wait_vsync() {
                sleep_until(frame_start + Duration::from_micros(16_667));
            }
        } else if cfg.fps_cap > 0 {
            sleep_until(frame_start + Duration::from_secs_f64(1.0 / cfg.fps_cap as f64));
        }

        let ms = |a: Instant, b: Instant| (b - a).as_secs_f32() * 1000.0;
        stats.record(dt * 1000.0, ms(t0, t1), ms(t1, t2), ms(t2, t3));
    }
}

/// Schläft bis kurz vor `deadline` und wartet den Rest aktiv ab.
fn sleep_until(deadline: Instant) {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let left = deadline - now;
        if left > Duration::from_millis(2) {
            std::thread::sleep(left - Duration::from_millis(1));
        } else {
            std::hint::spin_loop();
        }
    }
}
