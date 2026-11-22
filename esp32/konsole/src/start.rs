//! Spielwechsel per Neustart: Vor dem Neustart steht das Ziel im Speicher
//! (NVS), beim Start wird es gelesen und sofort gelöscht – ein Kaltstart
//! (Strom an) oder ein Absturz landet so immer im Konsolen-Menü.

use crate::SpielId;
use alloc::vec::Vec;

/// Dauerhafter Schlüssel-Wert-Speicher (auf dem Board: NVS).
pub trait Speicher {
    fn lesen(&self, schluessel: &str) -> Option<Vec<u8>>;
    fn schreiben(&self, schluessel: &str, daten: &[u8]);
    fn loeschen(&self, schluessel: &str);
}

/// NVS-Schlüssel des Startziels (kein Spielstand-Schlüssel, siehe Test).
pub const START_SCHLUESSEL: &str = "start";

/// Startziel lesen und löschen; ohne (gültiges) Ziel: die Konsole.
pub fn startziel(sp: &dyn Speicher) -> SpielId {
    let ziel = sp.lesen(START_SCHLUESSEL);
    if ziel.is_some() {
        sp.loeschen(START_SCHLUESSEL);
    }
    ziel.and_then(|b| (b.len() == 1).then(|| SpielId::aus_u8(b[0])).flatten()).unwrap_or(SpielId::Konsole)
}

/// Vor dem Neustart: Ziel merken. Zurück zur Konsole → dort ist das Spiel
/// ausgewählt, aus dem man kommt.
pub fn wechsel_vorbereiten(sp: &dyn Speicher, von: SpielId, nach: SpielId) {
    if nach == SpielId::Konsole && von != SpielId::Konsole {
        sp.schreiben(SpielId::Konsole.nvs_schluessel(), &[von.als_u8()]);
    }
    sp.schreiben(START_SCHLUESSEL, &[nach.als_u8()]);
}

/// Die Karte, die das Konsolen-Menü beim Start zeigt (zuletzt gewählt).
pub fn launcher_auswahl(sp: &dyn Speicher) -> SpielId {
    sp.lesen(SpielId::Konsole.nvs_schluessel())
        .and_then(|b| b.first().copied().and_then(SpielId::aus_u8))
        .filter(|id| SpielId::SPIELE.contains(id))
        .unwrap_or(SpielId::SPIELE[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};
    use core::cell::RefCell;

    #[derive(Default)]
    struct Attrappe(RefCell<BTreeMap<String, Vec<u8>>>);

    impl Speicher for Attrappe {
        fn lesen(&self, k: &str) -> Option<Vec<u8>> {
            self.0.borrow().get(k).cloned()
        }
        fn schreiben(&self, k: &str, d: &[u8]) {
            self.0.borrow_mut().insert(k.to_string(), d.to_vec());
        }
        fn loeschen(&self, k: &str) {
            self.0.borrow_mut().remove(k);
        }
    }

    #[test]
    fn start_ziel_wird_nach_dem_lesen_geloescht() {
        let sp = Attrappe::default();
        assert_eq!(startziel(&sp), SpielId::Konsole, "ohne Ziel: Konsole");
        wechsel_vorbereiten(&sp, SpielId::Konsole, SpielId::Doom);
        assert_eq!(startziel(&sp), SpielId::Doom);
        assert_eq!(startziel(&sp), SpielId::Konsole, "nächster Start (Strom an): wieder Konsole");
    }

    #[test]
    fn kaputtes_startziel_fuehrt_zur_konsole() {
        let sp = Attrappe::default();
        sp.schreiben(START_SCHLUESSEL, &[200]);
        assert_eq!(startziel(&sp), SpielId::Konsole);
        sp.schreiben(START_SCHLUESSEL, &[1, 2]);
        assert_eq!(startziel(&sp), SpielId::Konsole);
        assert!(sp.lesen(START_SCHLUESSEL).is_none());
    }

    #[test]
    fn rueckkehr_waehlt_das_letzte_spiel() {
        let sp = Attrappe::default();
        assert_eq!(launcher_auswahl(&sp), SpielId::Pudding, "Standard: erste Karte");
        wechsel_vorbereiten(&sp, SpielId::Wuerfelwelt, SpielId::Konsole);
        assert_eq!(startziel(&sp), SpielId::Konsole);
        assert_eq!(launcher_auswahl(&sp), SpielId::Wuerfelwelt);
        sp.schreiben(SpielId::Konsole.nvs_schluessel(), &[SpielId::Konsole.als_u8()]);
        assert_eq!(launcher_auswahl(&sp), SpielId::Pudding, "Konsole selbst ist keine Karte");
    }

    #[test]
    fn startschluessel_ist_kein_spielstand() {
        for id in SpielId::ALLE {
            assert_ne!(id.nvs_schluessel(), START_SCHLUESSEL);
        }
    }
}
