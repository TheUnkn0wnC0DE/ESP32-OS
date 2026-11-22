//! Autopilot für Messläufe ([`crate::Quake::autopilot`]): Quake spielt sich
//! selbst – mit simulierten Touch-Eingaben wie eine Spielerin mit zwei
//! Daumen (laufen, drehen, schießen, springen, Waffe wechseln) – und
//! wechselt alle [`LEVELZEIT`] Sekunden Spielzeit per `changelevel` zum
//! nächsten Shareware-Level, danach wieder von vorn. Die „Screen size“
//! steht dabei auf 120 (volle 3D-Sicht, größter Warp-Puffer unter Wasser);
//! beim Ausschalten kommt der vorige Wert zurück. Gespeichert wird nichts.
//!
//! Je Levelwechsel eine Zeile über [`crate::beim_bericht`]: Karte, freier
//! Hunk und (am Board) der kleinste freie Stapelrest des Quake-Threads.
//! Dieselbe Eingabe ([`eingabe`]) spielt der Langlauf-Test am PC.

use crate::ffi::{self, befehl, berichten};
use crate::steuerung::Modus;
use pudding_core::input::{Input, MAX_TOUCH};

/// Shareware-Level in Spielreihenfolge.
pub const KARTEN: [&str; 9] = ["start", "e1m1", "e1m2", "e1m3", "e1m4", "e1m5", "e1m6", "e1m7", "e1m8"];
/// Spielzeit je Level (s).
pub const LEVELZEIT: f32 = 120.0;
/// Bilder ohne Eingabe nach einem Kartenbefehl (Laden und Anmelden).
const LADEPAUSE: u32 = 40;
/// So lange (s) außerhalb des Spiels – etwa in der Konsole nach einem
/// Host_Error –, dann lädt der Autopilot das Level neu.
const NICHT_IM_SPIEL: f32 = 10.0;
/// „Screen size“ während des Autopiloten.
const SICHT: f32 = 120.0;

/// Eingabe für Bild `f` im Level: linker Daumen auf dem Knüppel, rechter
/// abwechselnd Feuer (dabei seitlich wischen = drehen), Springen, Waffe,
/// Umsehen.
pub fn eingabe(f: u32) -> Input {
    let phase = (f / 110) % 6;
    let knueppel = if f % 330 < 2 {
        None
    } else if f % 330 == 2 {
        Some((150u16, 250u16))
    } else {
        let (dx, dy): (i32, i32) = [(0, -50), (35, -35), (0, 50), (-50, 0), (0, -50), (50, 0)][phase as usize];
        Some(((150 + dx) as u16, (250 + dy) as u16))
    };
    let z = f % 60;
    let rechts_dreht = (f / 60) % 3 != 1;
    let rechter = match z {
        0..40 => {
            let x = if rechts_dreht { 560 + 2 * z } else { 639 - 2 * z };
            Some((x as u16, 310u16))
        }
        42..50 => Some((600, 214)),
        52..55 if (f / 60) % 5 == 4 => Some((600, 120)),
        55..60 => Some((450, (190 + (z - 55) * 6) as u16)),
        _ => None,
    };
    Input { held: 0, touch: [knueppel, rechter] }
}

pub(crate) struct Autopilot {
    /// Index in [`KARTEN`] des laufenden Levels.
    karte: usize,
    /// Spielzeit im laufenden Level (s).
    zeit: f32,
    /// Bilder seit dem letzten Kartenbefehl.
    bild: u32,
    /// Spielzeit (s) außerhalb des Spiels am Stück.
    ausserhalb: f32,
    /// Spielzeit (s) seit dem Einschalten.
    gesamt: f32,
    /// Levelwechsel seit dem Einschalten.
    wechsel: u32,
    /// „Screen size“ vor dem Einschalten.
    sicht_vorher: f32,
}

impl Autopilot {
    /// Schaltet ein: „Screen size“ 120, neues Spiel ab „start“.
    pub fn starten() -> Self {
        let sicht_vorher = ffi::variable("viewsize");
        befehl(&format!("viewsize {SICHT}\nmaxplayers 1\nskill 1\nmap {}\n", KARTEN[0]));
        berichten(&format!(
            "Quake-Autopilot an: viewsize {SICHT} (vorher {sicht_vorher}), map {}, Levelwechsel alle {LEVELZEIT} s",
            KARTEN[0]
        ));
        Self { karte: 0, zeit: 0.0, bild: 0, ausserhalb: 0.0, gesamt: 0.0, wechsel: 0, sicht_vorher }
    }

    /// Schaltet aus: „Screen size“ wie vorher.
    pub fn beenden(self) {
        befehl(&format!("viewsize {}\n", self.sicht_vorher));
        berichten(&format!("Quake-Autopilot aus nach {:.0} s, {} Levelwechsel", self.gesamt, self.wechsel));
    }

    /// Ein Bild: ersetzt die Touch-Eingabe, schickt bei Bedarf den nächsten
    /// Kartenbefehl.
    pub fn schritt(&mut self, echt: &Input, modus: Modus, dt: f32) -> Input {
        self.zeit += dt;
        self.gesamt += dt;
        self.ausserhalb = if modus == Modus::Spiel { 0.0 } else { self.ausserhalb + dt };
        if self.zeit >= LEVELZEIT {
            self.naechstes_level();
        } else if self.ausserhalb >= NICHT_IM_SPIEL {
            berichten(&format!("Quake-Autopilot: {NICHT_IM_SPIEL} s nicht im Spiel – lade {} neu", KARTEN[self.karte]));
            befehl(&format!("map {}\n", KARTEN[self.karte]));
            self.ausserhalb = 0.0;
            self.bild = 0;
        }
        let touch = if modus == Modus::Spiel && self.bild >= LADEPAUSE {
            eingabe(self.bild - LADEPAUSE).touch
        } else {
            [None; MAX_TOUCH]
        };
        self.bild += 1;
        Input { held: echt.held, touch }
    }

    fn naechstes_level(&mut self) {
        let naechste = (self.karte + 1) % KARTEN.len();
        let karte = ffi::karte();
        // changelevel behält Waffen und Munition wie am Levelende; ohne
        // laufenden Server geht nur map.
        let wort = if karte.is_empty() { "map" } else { "changelevel" };
        self.wechsel += 1;
        let stapel = ffi::stapel_frei().map(|b| format!(", Stapel min. frei {} KB", b / 1024)).unwrap_or_default();
        berichten(&format!(
            "Quake-Autopilot: Wechsel {}, Karte {karte} nach {:.0} s, Hunk frei {} KB{stapel}, Quake-Bilder {}, Spielzeit {:.0} s -> {wort} {}",
            self.wechsel,
            self.zeit,
            ffi::hunk_frei() / 1024,
            ffi::gemeinsam().bilder,
            self.gesamt,
            KARTEN[naechste]
        ));
        befehl(&format!("{wort} {}\n", KARTEN[naechste]));
        self.karte = naechste;
        self.zeit = 0.0;
        self.bild = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eingabe_haelt_knueppel_und_drueckt_feuer() {
        // Bild 3 im ersten Abschnitt: Knüppel nach vorn, rechter Daumen auf Feuer.
        let e = eingabe(3);
        assert_eq!(e.touch[0], Some((150, 200)));
        assert_eq!(e.touch[1], Some((566, 310)));
        // Zwischendurch Springen und Umsehen.
        assert_eq!(eingabe(45).touch[1], Some((600, 214)));
        assert_eq!(eingabe(57).touch[1], Some((450, 202)));
    }
}
