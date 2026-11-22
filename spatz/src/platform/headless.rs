//! Kopflose Plattform für Tests und Benchmarks: kein Fenster, keine Ausgabe.

use super::Platform;
use crate::input::Input;
use crate::render::Framebuffer;

/// Plattform ohne Fenster. Zählt nur die präsentierten Frames.
#[derive(Default)]
pub struct Headless {
    pub frames_presented: u64,
    /// Nach so vielen Frames meldet `pump` „Beenden“ (0 = nie).
    pub max_frames: u64,
}

impl Platform for Headless {
    fn pump(&mut self, _input: &mut Input) -> bool {
        self.max_frames == 0 || self.frames_presented < self.max_frames
    }
    fn present(&mut self, _fb: &Framebuffer) {
        self.frames_presented += 1;
    }
}
