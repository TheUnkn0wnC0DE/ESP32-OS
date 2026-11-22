//! Kleinstes Fenster-Beispiel: ein rotierender Würfel mit dem App-Runner.
//! Zeigt die Grundstruktur eines Spiels auf `spatz` (Game-Trait, fester
//! Simulationstakt, Rendern mit Interpolation, eingebautes Perf-Overlay).
//!
//! Aufruf (Windows): `cargo run --release --example hello_window`

use spatz::app::{AppConfig, Control, Game, run};
use spatz::prelude::*;
use spatz::render::{RenderStats, shapes};

struct Spinner {
    store: MeshStore,
    cube: MeshId,
    renderer: Renderer,
    list: DrawList,
    angle: f32,
    prev_angle: f32,
}

impl Game for Spinner {
    fn update(&mut self, input: &Input, dt: f32, ctl: &mut Control) {
        if input.pressed(Key::Escape) {
            ctl.quit = true;
        }
        self.prev_angle = self.angle;
        self.angle += dt * 1.5;
    }

    fn render(&mut self, fb: &mut Framebuffer, alpha: f32) -> RenderStats {
        let a = self.prev_angle + (self.angle - self.prev_angle) * alpha;
        let mut cam = Camera::default();
        cam.look_at(Vec3::new(0.0, 1.5, 4.0), Vec3::ZERO);
        self.list.clear();
        self.list.add(self.cube, Mat4::from_trs(Vec3::ZERO, Quat::from_yaw_pitch_roll(a, a * 0.7, 0.0), Vec3::ONE));
        self.renderer.begin(fb, &cam, &Environment::default());
        self.renderer.draw_list(fb, &self.store, &self.list);
        self.renderer.finish(fb);
        self.renderer.stats
    }
}

#[cfg(windows)]
fn main() {
    let cfg = AppConfig { title: "spatz – Hallo Würfel".into(), width: 320, height: 180, ..AppConfig::default() };
    let mut window =
        spatz::platform::win32::Win32Window::new(&cfg.title, cfg.width, cfg.height, None).expect("Fenster");
    let mut store = MeshStore::new();
    let cube = store.add(shapes::box_centered(Vec3::ONE, Color::hex(0xFFB030)));
    let mut game =
        Spinner { store, cube, renderer: Renderer::new(), list: DrawList::new(), angle: 0.0, prev_angle: 0.0 };
    run(&mut window, &mut game, &cfg);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Dieses Beispiel braucht das Win32-Backend.");
}
