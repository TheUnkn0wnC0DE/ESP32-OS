//! Plattform-Layer: Fenster, Eingabe, Bildausgabe.
//!
//! Neue Plattformen (SDL, X11, ein Taschenrechner-LCD, ...) implementieren nur
//! [`Platform`]; Engine-Kern und Spiel bleiben unverändert.

use crate::input::Input;
use crate::render::Framebuffer;

pub mod headless;
#[cfg(windows)]
pub mod win32;

/// Schnittstelle zwischen Engine und Betriebssystem.
pub trait Platform {
    /// Verarbeitet OS-Ereignisse und aktualisiert `input`.
    /// `false` = Beenden wurde angefordert (Fenster geschlossen).
    fn pump(&mut self, input: &mut Input) -> bool;
    /// Zeigt den Framebuffer an (skaliert, mit Letterbox).
    fn present(&mut self, fb: &Framebuffer);
    /// Setzt den Fenstertitel.
    fn set_title(&mut self, _title: &str) {}
    /// Wartet auf den nächsten Bildaufbau. `false` = nicht unterstützt.
    fn wait_vsync(&mut self) -> bool {
        false
    }
    /// Schaltet Vollbild um (falls unterstützt).
    fn toggle_fullscreen(&mut self) {}
}

#[cfg(windows)]
pub use win32::Win32Window as DefaultWindow;
