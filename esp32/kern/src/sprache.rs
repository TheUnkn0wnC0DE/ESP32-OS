//! Sprache der Oberfläche (Auftrag 12.1): Englisch ist Standard, Deutsch
//! umschaltbar. Texte stehen beim Aufrufer als Paar: `t("Open", "Öffnen")`.
//! Gespeichert wird die Wahl vom Desktop (`/sys/settings/esp32os.ini`).

use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sprache {
    Englisch,
    Deutsch,
}

static AKTUELL: AtomicU8 = AtomicU8::new(0);

pub fn aktuell() -> Sprache {
    if AKTUELL.load(Ordering::Relaxed) == 1 { Sprache::Deutsch } else { Sprache::Englisch }
}

pub fn setzen(s: Sprache) {
    AKTUELL.store(u8::from(s == Sprache::Deutsch), Ordering::Relaxed);
}

/// Text in der aktuellen Sprache.
pub fn t(englisch: &'static str, deutsch: &'static str) -> &'static str {
    match aktuell() {
        Sprache::Englisch => englisch,
        Sprache::Deutsch => deutsch,
    }
}

/// Kürzel für die Einstellungsdatei („en“/„de“).
pub fn kuerzel(s: Sprache) -> &'static str {
    match s {
        Sprache::Englisch => "en",
        Sprache::Deutsch => "de",
    }
}

pub fn aus_kuerzel(k: &str) -> Option<Sprache> {
    match k.trim().to_ascii_lowercase().as_str() {
        "en" => Some(Sprache::Englisch),
        "de" => Some(Sprache::Deutsch),
        _ => None,
    }
}
