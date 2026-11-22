//! # spatz – eine winzige, schnelle 3D-Engine
//!
//! `spatz` rendert komplett auf der CPU (Software-Rasterizer) und braucht
//! weder GPU noch externe Bibliotheken. Der Kern ist `#![no_std]` (+ `alloc`)
//! und damit auch auf sehr schwacher Hardware (Mikrocontroller,
//! Taschenrechner) lauffähig. Mit dem Feature `std` (Standard) kommen
//! Plattform-Layer (Win32-Fenster, Headless) und ein App-Runner dazu.
//!
//! Architektur (jede Schicht kennt nur die darunterliegenden):
//!
//! ```text
//!   app        – Spielschleife: fester Physik-Takt, Interpolation, Perf-HUD
//!   platform   – Fenster/Eingabe/Present (Win32, Headless)          [std]
//!   render     – Kamera, Meshes, Pipeline, Rasterizer, Text, Overlay
//!   anim       – gedämpfte Federn für sekundäre Bewegung
//!   softbody   – XPBD-Weichkörper (Wackelpudding) mit Volumenerhalt
//!   physics    – Kollisionsformen, Charakter-Controller, Raycasts
//!   input/time – Tastenzustand, Frame-Statistik, Fixed-Timestep
//!   math/color – Vektoren, Matrizen, Quaternionen, Geometrie, Farben
//! ```
//!
//! Performance-Leitlinien:
//! * Geometrie in `f32` (wenige Vertices), alles pro Pixel in Ganzzahlen.
//! * Keine Allokationen pro Frame (Scratch-Puffer werden wiederverwendet).
//! * Frustum-Culling pro Objekt, Backface-Culling, Guard-Band-Clipping.
//! * Nebel + Himmel in einem einzigen Nachbearbeitungs-Durchlauf.

#![no_std]

extern crate alloc;
#[cfg(any(feature = "std", test))]
extern crate std;

pub mod anim;
pub mod color;
pub mod cpu;
pub mod image;
pub mod input;
pub mod math;
pub mod physics;
pub mod render;
pub mod rng;
pub mod softbody;
pub mod time;

#[cfg(feature = "std")]
pub mod app;
#[cfg(feature = "std")]
pub mod platform;

/// Häufig benötigte Typen auf einen Blick.
pub mod prelude {
    pub use crate::color::Color;
    pub use crate::input::{Input, Key};
    pub use crate::math::{Aabb, Mat4, Quat, Sphere, Vec2, Vec3, Vec4};
    pub use crate::physics::{CharacterBody, Collider, CollisionWorld, Shape};
    pub use crate::render::{
        Camera, DrawList, DrawMode, DrawParams, Environment, Framebuffer, Mesh, MeshId, MeshStore, Renderer,
    };
    pub use crate::rng::Rng;
    pub use crate::time::{FixedTimestep, FrameStats};
}
