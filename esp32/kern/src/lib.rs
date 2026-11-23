//! ESP32-OS – native Kernschicht.
//!
//! Baut die KolibriOS-Systemfunktionen (sysfuncs) mit gleichem Verhalten für
//! die ESP32-P4-Konsole nach. Dieses Crate ist plattformunabhängig (std):
//! am Board liefert die Firmware die [`Plattform`], am PC [`PC`].
//!
//! * K1: Prozess- und Speichermodell ([`prozess`], [`heap`], [`KernAlloc`]).
//! * K2: Systemfunktionen ([`sys`]): Fenster, Zeichnen, Text, Knöpfe,
//!   Ereignisse, Maus, Zeit, Fäden, Speicher ([`fenster`], [`grafik`],
//!   [`schrift`], [`zeit`]).
//! * K4: Original-Look: `.skn`-Skins ([`skin`]), für Touch vergrössert.
//! * K6: Dateisystem ([`datei`], Funktionen 30/70/80).
//! * D: WLAN ([`wlan`]): Treiber der Firmware bzw. Simulation am PC;
//!   Netzwerk der Funktionen 74/75/76 ([`netz`], auf `std::net`).
//! * K3: C-Schnittstelle `kos_syscall` (`abi`, nur 32 Bit) für die native ksys.h.

#[cfg(target_pointer_width = "32")]
pub mod abi;
pub mod aktualisierung;
mod alloc;
pub mod datei;
pub mod fenster;
pub mod grafik;
pub mod heap;
pub mod netz;
mod plattform;
pub mod prozess;
pub mod schrift;
pub mod skin;
pub mod sprache;
pub mod sys;
pub mod ton;
pub mod wlan;
pub mod zeit;
pub mod zeiger;

pub use alloc::{KernAlloc, notbelegungen, regionen};
#[cfg(not(target_os = "espidf"))]
pub use plattform::PC;
pub use plattform::{Lauf, Plattform, ProzessEnde};
pub use prozess::{
    Ende, Faden, ProgrammEintrag, ProzessInfo, StartFehler, Zwang, aufraeumen, beende, beende_selbst, eigene_pid, geschuetzt, zwangsbeenden,
    argumente, eigene_tid, ende_von, neue_abstuerze, starte_mit, faden_starten, init, kern_abschnitt, liste, programme, pruefe_ende, starte, starte_name,
};
