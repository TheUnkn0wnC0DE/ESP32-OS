//! Touch-Gesten → Kern-Eingaben (Auftrag Abschnitt 4).
//!
//! * Tippen (kurz, kaum bewegt) = Linksklick an der Stelle.
//! * Halten ≥ 600 ms ohne Bewegung = Rechtsklick (Kontextmenü).
//! * Bewegung in den ersten 300 ms = Wischen → Scrollen (Funktion 37.7,
//!   Finger nach oben = Inhalt nach unten, wie das Mausrad).
//! * Halten 300–600 ms, dann bewegen = Ziehen mit gedrückter linker Taste
//!   (Schieberegler, Markieren).
//! * Titelleiste ziehen = Fenster verschieben; Doppeltipp auf die
//!   Titelleiste = maximieren/wiederherstellen.
//! * Zwei Finger auseinander/zusammen = Zoom (an das aktive Fenster).
//!
//! Touchpad-Modus (zuschaltbar, für winzige Ziele in alten Programmen):
//! Finger bewegt den Zeiger relativ (×1,5), Tippen klickt am Zeiger, Halten
//! = Rechtsklick, Tippen und gleich wieder aufsetzen = Ziehen mit gedrückter
//! Taste, zwei Finger = Scrollen.
//!
//! Koordinaten logisch (640×360). Zeit in ms von aussen (testbar).

use kern::sys::eingabe;

pub const TIPP_MS: u64 = 300;
pub const HALTEN_MS: u64 = 600;
pub const DOPPEL_MS: u64 = 450;
/// Bewegung, ab der kein Tippen mehr vorliegt (logische px, ≈ 1,4 mm).
pub const SCHWELLE: i32 = 8;
/// Wischweg je Scrollschritt (logische px).
pub const SCROLL_SCHRITT: i32 = 12;
/// So lange bleibt die Taste bei Tippen/Halten gedrückt (wie ein Mausklick;
/// Programme fragen den Zustand mit 37.2 ab).
pub const KLICK_MS: u64 = 80;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Zustand {
    Frei,
    Unklar { start: (i32, i32), t0: u64 },
    Wischen { letzt: (i32, i32), rest: (i32, i32) },
    Ziehen { letzt: (i32, i32) },
    Kopf { slot: usize, start: (i32, i32), letzt: (i32, i32), bewegt: bool },
    Gehalten,
    Zwei { letzter: f32 },
    /// Nach zwei Fingern: warten, bis alle los sind.
    Auslaufen,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tp {
    Frei,
    Auf { letzt: (i32, i32), start: (i32, i32), t0: u64, ziehen: bool, bewegt: bool, rest: (i32, i32) },
    Gehalten,
    Zwei { mitte: (i32, i32), rest: (i32, i32) },
}

/// Zeigerbewegung je Fingerweg (Touchpad-Modus), in Zehnteln.
pub const TP_FAKTOR: i32 = 15;

pub struct Gesten {
    zustand: Zustand,
    tp: Tp,
    letzter_tp_tipp: u64,
    letzter_kopf_tipp: Option<(u64, usize)>,
    /// Noch loszulassender Klick: (ab Zeit, Stelle).
    offen: Option<(u64, (i32, i32))>,
    /// Lag im letzten Schritt ein Finger auf?
    vorher: bool,
}

impl Default for Gesten {
    fn default() -> Gesten {
        Gesten { zustand: Zustand::Frei, tp: Tp::Frei, letzter_tp_tipp: 0, letzter_kopf_tipp: None, offen: None, vorher: false }
    }
}

fn abstand(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

impl Gesten {
    /// Ein Abtastschritt: `t` in ms, bis zu zwei Finger.
    pub fn schritt(&mut self, t: u64, p: [Option<(i32, i32)>; 2]) {
        use Zustand::*;
        // Klick loslassen, sobald er lange genug gedrückt war oder ein Finger neu
        // aufsetzt (nicht, solange derselbe Finger noch liegt: Halten im Touchpad-Modus)
        let jetzt = p[0].is_some() || p[1].is_some();
        let neu_auf = jetzt && !self.vorher;
        self.vorher = jetzt;
        if let Some((ab, q)) = self.offen
            && (t >= ab || neu_auf)
        {
            eingabe::maus(q.0, q.1, 0);
            self.offen = None;
        }
        if eingabe::touchpad() {
            self.touchpad(t, p);
            return;
        }
        // Zwei Finger: Zoom
        if let (Some(a), Some(b)) = (p[0], p[1]) {
            let d = (((a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)) as f32).sqrt().max(1.0);
            match self.zustand {
                Zwei { letzter } => {
                    let r = d / letzter;
                    if (r - 1.0).abs() > 0.02 {
                        eingabe::zoom((r * 1000.0) as u32, (a.0 + b.0) / 2, (a.1 + b.1) / 2);
                        self.zustand = Zwei { letzter: d };
                    }
                }
                Ziehen { letzt } => {
                    eingabe::maus(letzt.0, letzt.1, 0);
                    self.zustand = Zwei { letzter: d };
                }
                _ => self.zustand = Zwei { letzter: d },
            }
            return;
        }
        let f = p[0].or(p[1]);
        self.zustand = match (self.zustand, f) {
            (Frei, None) => Frei,
            (Frei, Some(q)) => match eingabe::kopf_bei(q.0, q.1) {
                Some(slot) => {
                    // Titelleiste berühren aktiviert das Fenster (wie ein Klick darauf)
                    eingabe::aktivieren(slot);
                    Kopf { slot, start: q, letzt: q, bewegt: false }
                }
                None => {
                    eingabe::maus(q.0, q.1, 0);
                    Unklar { start: q, t0: t }
                }
            },
            (Unklar { start, t0 }, Some(q)) => {
                let el = t.saturating_sub(t0);
                if abstand(start, q) > SCHWELLE {
                    if el < TIPP_MS {
                        self.wischen(start, (0, 0), q)
                    } else {
                        eingabe::maus(start.0, start.1, 1);
                        eingabe::maus(q.0, q.1, 1);
                        Ziehen { letzt: q }
                    }
                } else if el >= HALTEN_MS {
                    eingabe::maus(start.0, start.1, 2);
                    self.offen = Some((t + KLICK_MS, start));
                    Gehalten
                } else {
                    Unklar { start, t0 }
                }
            }
            (Unklar { start, .. }, None) => {
                eingabe::maus(start.0, start.1, 1);
                self.offen = Some((t + KLICK_MS, start));
                Frei
            }
            (Wischen { letzt, rest }, Some(q)) => self.wischen(letzt, rest, q),
            (Wischen { .. }, None) => Frei,
            (Ziehen { .. }, Some(q)) => {
                eingabe::maus(q.0, q.1, 1);
                Ziehen { letzt: q }
            }
            (Ziehen { letzt }, None) => {
                eingabe::maus(letzt.0, letzt.1, 0);
                Frei
            }
            (Kopf { slot, start, letzt, bewegt }, Some(q)) => {
                if bewegt || abstand(start, q) > SCHWELLE {
                    eingabe::verschieben(slot, q.0 - letzt.0, q.1 - letzt.1);
                    Kopf { slot, start, letzt: q, bewegt: true }
                } else {
                    Kopf { slot, start, letzt, bewegt }
                }
            }
            (Kopf { slot, start, bewegt, .. }, None) => {
                if !bewegt {
                    // Tippen auf den Kopf aktiviert; zweimal kurz = maximieren
                    eingabe::maus(start.0, start.1, 1);
                    self.offen = Some((t + KLICK_MS, start));
                    match self.letzter_kopf_tipp {
                        Some((t1, s1)) if s1 == slot && t.saturating_sub(t1) <= DOPPEL_MS => {
                            eingabe::maximieren(slot);
                            self.letzter_kopf_tipp = None;
                        }
                        _ => self.letzter_kopf_tipp = Some((t, slot)),
                    }
                }
                Frei
            }
            (Gehalten, Some(_)) => Gehalten,
            (Gehalten, None) => Frei,
            (Zwei { .. } | Auslaufen, Some(_)) => Auslaufen,
            (Zwei { .. } | Auslaufen, None) => Frei,
        };
    }

    fn touchpad(&mut self, t: u64, p: [Option<(i32, i32)>; 2]) {
        let (zx, zy) = eingabe::zeiger();
        if let (Some(a), Some(b)) = (p[0], p[1]) {
            let m = ((a.0 + b.0) / 2, (a.1 + b.1) / 2);
            self.tp = match self.tp {
                Tp::Zwei { mitte, rest } => {
                    let dy = m.1 - mitte.1 + rest.1;
                    let s = -dy / SCROLL_SCHRITT;
                    if s != 0 {
                        eingabe::scroll(0, s);
                    }
                    Tp::Zwei { mitte: m, rest: (0, dy + s * SCROLL_SCHRITT) }
                }
                Tp::Auf { ziehen: true, .. } => {
                    eingabe::maus(zx, zy, 0);
                    Tp::Zwei { mitte: m, rest: (0, 0) }
                }
                _ => Tp::Zwei { mitte: m, rest: (0, 0) },
            };
            return;
        }
        let f = p[0].or(p[1]);
        self.tp = match (self.tp, f) {
            (Tp::Frei, None) => Tp::Frei,
            (Tp::Frei, Some(q)) => {
                // gleich nach einem Tippen wieder aufsetzen = Ziehen
                let ziehen = t.saturating_sub(self.letzter_tp_tipp) <= DOPPEL_MS && self.letzter_tp_tipp > 0;
                if ziehen {
                    if let Some((_, o)) = self.offen.take() {
                        eingabe::maus(o.0, o.1, 0);
                    }
                    eingabe::maus(zx, zy, 1);
                }
                Tp::Auf { letzt: q, start: q, t0: t, ziehen, bewegt: false, rest: (0, 0) }
            }
            (Tp::Auf { letzt, start, t0, ziehen, bewegt, rest }, Some(q)) => {
                let (dx, dy) = ((q.0 - letzt.0) * TP_FAKTOR + rest.0, (q.1 - letzt.1) * TP_FAKTOR + rest.1);
                let (mx, my) = (dx / 10, dy / 10);
                if mx != 0 || my != 0 {
                    eingabe::maus(zx + mx, zy + my, u32::from(ziehen));
                }
                let bewegt = bewegt || abstand(start, q) > SCHWELLE;
                if !bewegt && !ziehen && t.saturating_sub(t0) >= HALTEN_MS {
                    eingabe::maus(zx, zy, 2);
                    self.offen = Some((t + KLICK_MS, (zx, zy)));
                    Tp::Gehalten
                } else {
                    Tp::Auf { letzt: q, start, t0, ziehen, bewegt, rest: (dx - mx * 10, dy - my * 10) }
                }
            }
            (Tp::Auf { ziehen: true, .. }, None) => {
                eingabe::maus(zx, zy, 0);
                self.letzter_tp_tipp = 0;
                Tp::Frei
            }
            (Tp::Auf { bewegt, t0, .. }, None) => {
                if !bewegt && t.saturating_sub(t0) < HALTEN_MS {
                    eingabe::maus(zx, zy, 1);
                    self.offen = Some((t + KLICK_MS, (zx, zy)));
                    self.letzter_tp_tipp = t;
                }
                Tp::Frei
            }
            (Tp::Gehalten, Some(_)) => Tp::Gehalten,
            (Tp::Gehalten | Tp::Zwei { .. }, None) => Tp::Frei,
            (Tp::Zwei { .. }, Some(_)) => Tp::Gehalten,
        };
    }

    fn wischen(&self, letzt: (i32, i32), rest: (i32, i32), q: (i32, i32)) -> Zustand {
        let (dx, dy) = (q.0 - letzt.0 + rest.0, q.1 - letzt.1 + rest.1);
        let (sx, sy) = (-dx / SCROLL_SCHRITT, -dy / SCROLL_SCHRITT);
        if sx != 0 || sy != 0 {
            eingabe::scroll(sx, sy);
        }
        Zustand::Wischen { letzt: q, rest: (dx + sx * SCROLL_SCHRITT, dy + sy * SCROLL_SCHRITT) }
    }
}
