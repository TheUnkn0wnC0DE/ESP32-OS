//! # Pudding-Panik für den ESP32-P4
//!
//! Plattformunabhängiger Kern der Mikrocontroller-Fassung (`no_std` +
//! `alloc`): Spiel, 3D-Renderer für RGB565, HUD, Menüs, Touch-Steuerung,
//! Spielstand und Synthesizer. Die Firmware (`../firmware`) liefert nur
//! Knöpfe, Touch, Zeit, Display, Lautsprecher und Flash; die PC-Simulation
//! (`../sim`) dieselben Dinge kopflos – so lässt sich alles am PC testen.
//!
//! ```text
//!   app      – Titel, Menüs, Spiel, Pause, Ergebnis, Musik-Schalter,
//!              Touch-Steuerung, Speichern
//!   play     – laufende Partie (Welt, Pudding, Weichkörper, Kamera, Partikel)
//!   render   – 3D-Renderer: RGB565, 16-Bit-Tiefe, Streifen für zwei Kerne
//!   gfx/hud  – 2D-Zeichnen (320×180-Layout, ×2 skalierbar), HUD, FPS-Anzeige
//!   audio    – Synthesizer, Soundeffekte, Musik (für den zweiten Kern)
//!   input    – Knöpfe, Touch, Umrechnung Panel → Spielbild
//!   present  – Streifen, PPA-Blocklage, Nearest-Neighbor-Vergrößerung
//!   testbild – Testbild für das Hardware-Testprogramm
//!   save     – Spielstand mit Prüfsumme
//! ```

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod app;
pub mod assets;
pub mod audio;
pub mod autopilot;
pub mod battery;
pub mod camera_rig;
pub mod campaign;
pub mod gfx;
pub mod hud;
pub mod input;
pub mod jelly;
pub mod level;
pub mod particles;
pub mod play;
pub mod player;
pub mod present;
pub mod render;
pub mod save;
pub mod testbild;
pub mod touchpad;
pub mod world;
