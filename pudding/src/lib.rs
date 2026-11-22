//! Pudding-Panik als Bibliothek: Spiel, Level, Werkzeuge.
//!
//! Das PC-Programm (`main.rs`) nutzt diesen Spielkern.

pub mod alloc_count;
pub mod assets;
pub mod autopilot;
pub mod bench;
pub mod camera_rig;
pub mod game;
pub mod hud;
pub mod jelly;
pub mod level;
pub mod particles;
pub mod player;
pub mod verify;
pub mod world;

/// Tests der Bibliothek zählen Allokationen ebenso wie das PC-Programm.
#[cfg(test)]
#[global_allocator]
static GLOBAL: alloc_count::Counting = alloc_count::Counting;
