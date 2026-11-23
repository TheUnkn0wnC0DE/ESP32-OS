//! A3: Uhr mit Wecker und Timer (Auftrag Abschnitt 5).
//!
//! Drei Reiter: „Uhr“ (Uhrzeit gross, Datum, Wecker-Stand), „Wecker“
//! (Stunde/Minute mit grossen +/−-Tasten, an/aus – gespeichert in den
//! Einstellungen) und „Timer“ (Minuten/Sekunden, Start/Pause,
//! Zurücksetzen). Läuft der Timer ab oder ist die Weckzeit erreicht,
//! klingelt die Uhr: grosser „Stopp“-Knopf, Signalton jede Sekunde
//! (höchstens eine Minute lang).
//!
//! Den Wecker prüft die Taskleiste zweimal je Sekunde ([`wecker_pruefen`]):
//! Er klingelt auch, wenn die Uhr nicht offen ist – sie wird dann gestartet.

use crate::konfig;
use core::sync::atomic::{AtomicU8, AtomicU32, Ordering::Relaxed};
use kern::sys;

// Fenster (Arbeitsbereich-Koordinaten)
pub const FX: i32 = 120;
pub const FY: i32 = 4;
pub const FW: i32 = 400;
pub const FH: i32 = 306;
/// Breite des Inhalts (Fenster ohne Rand)
const IW: i32 = FW - 10;

/// Knöpfe
pub const K_REITER: u32 = 2; // 2, 3, 4
pub const K_STOPP: u32 = 10;
pub const K_STUNDE_PLUS: u32 = 11;
pub const K_STUNDE_MINUS: u32 = 12;
pub const K_MINUTE_PLUS: u32 = 13;
pub const K_MINUTE_MINUS: u32 = 14;
pub const K_WECKER_AN: u32 = 15;
pub const K_T_MIN_PLUS: u32 = 20;
pub const K_T_MIN_MINUS: u32 = 21;
pub const K_T_SEK_PLUS: u32 = 22;
pub const K_T_SEK_MINUS: u32 = 23;
pub const K_T_START: u32 = 24;
pub const K_T_ZURUECK: u32 = 25;

/// Reiter: x = 6 + i · 128, Breite 122.
const REITER_Y: i32 = 6;
const REITER_H: i32 = 36;
/// Grosse Ziffern „HH:MM“ (Schrift ×4: 32 × 64 je Zeichen)
const ZIFFERN_Y: i32 = 98;
const ZIFFERN_X: i32 = 30;
const PLUS_Y: i32 = 52;
const MINUS_Y: i32 = 168;
const PM_H: i32 = 42;
/// Rechte Spalte (an/aus, Start, Zurücksetzen)
const RECHTS_X: i32 = 236;
const RECHTS_W: i32 = 146;

/// Was klingelt: 0 nichts, 1 Wecker, 2 Timer.
pub static KLINGELN: AtomicU8 = AtomicU8::new(0);
/// Minute (lokal, seit 1970), in der der Wecker zuletzt geklingelt hat.
static WECKER_MINUTE: AtomicU32 = AtomicU32::new(0);
/// Zuletzt gezeichneter Haupttext (für Tests und das Board-Log).
static ZULETZT: crate::Textmerker = crate::Textmerker::neu();

/// Text, den die Uhr zuletzt gross angezeigt hat.
pub fn anzeige_zuletzt() -> String {
    ZULETZT.holen()
}

fn anzeige_merken(s: &str) {
    ZULETZT.setzen(s);
}

/// Lage eines Knopfs (x, y, Breite, Höhe; Arbeitsbereich des Fensters).
pub fn knopf_lage(k: u32) -> (i32, i32, i32, i32) {
    match k {
        2..=4 => (6 + (k - K_REITER) as i32 * 128, REITER_Y, 122, REITER_H),
        K_STOPP => (IW / 2 - 110, 130, 220, 80),
        K_STUNDE_PLUS | K_T_MIN_PLUS => (ZIFFERN_X, PLUS_Y, 64, PM_H),
        K_STUNDE_MINUS | K_T_MIN_MINUS => (ZIFFERN_X, MINUS_Y, 64, PM_H),
        K_MINUTE_PLUS | K_T_SEK_PLUS => (ZIFFERN_X + 96, PLUS_Y, 64, PM_H),
        K_MINUTE_MINUS | K_T_SEK_MINUS => (ZIFFERN_X + 96, MINUS_Y, 64, PM_H),
        K_WECKER_AN | K_T_START => (RECHTS_X, 76, RECHTS_W, 56),
        K_T_ZURUECK => (RECHTS_X, 144, RECHTS_W, 56),
        _ => (0, 0, 0, 0),
    }
}

/// Mitte eines Knopfs (Arbeitsbereich) – für Tests und den Autopiloten.
pub fn knopf_mitte(k: u32) -> (i32, i32) {
    let (x, y, w, h) = knopf_lage(k);
    (x + w / 2, y + h / 2)
}

/// Lokale Zeit in Sekunden seit 1970 (None: Uhr nicht gestellt).
fn lokal_jetzt() -> Option<i64> {
    let t = kern::zeit::unix();
    (t >= konfig::UHR_GUELTIG_AB).then(|| kern::zeit::lokal(t))
}

/// Wecker prüfen (Taskleiste, alle 0,5 s): Weckzeit erreicht → klingeln,
/// die Uhr starten, wenn sie nicht läuft.
pub fn wecker_pruefen() {
    let w = konfig::WECKER.load(Relaxed);
    if w & 1 << 16 == 0 {
        return;
    }
    let Some(l) = lokal_jetzt() else { return };
    if (l.rem_euclid(86_400) / 60) as u32 != w & 0xFFFF {
        return;
    }
    let minute = l.div_euclid(60) as u32;
    if WECKER_MINUTE.swap(minute, Relaxed) == minute {
        return;
    }
    KLINGELN.store(1, Relaxed);
    sys::debug_text(&format!("Uhr: Wecker {:02}:{:02} klingelt", (w & 0xFFFF) / 60, (w & 0xFFFF) % 60));
    if !kern::liste().iter().any(|p| p.name == "uhr") {
        let r = kern::datei::starten("/sys/uhr", "");
        if r < 0 {
            sys::debug_text(&format!("Uhr: Start für den Wecker fehlgeschlagen ({})", -r));
        }
    }
}

/// Text mit Vergrösserung `mal` (1..=8; 8 × 16 Pixel je Zeichen bei 1).
fn text(x: i32, y: i32, s: &str, farbe: u32, mal: u32) {
    let f = 0x3000_0000 | (mal.clamp(1, 8) - 1) << 24;
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, f | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

/// Text waagrecht mittig in [x, x+w).
fn text_mittig(x: i32, w: i32, y: i32, s: &str, farbe: u32, mal: u32) {
    let breite = s.chars().count() as i32 * 8 * mal as i32;
    text(x + (w - breite) / 2, y, s, farbe, mal);
}

fn knopf(k: u32, beschriftung: &str, farbe: u32, schrift: u32, mal: u32) {
    let (x, y, w, h) = knopf_lage(k);
    sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, k, farbe);
    text_mittig(x, w, y + (h - 16 * mal as i32) / 2, beschriftung, schrift, mal);
}

const WOCHENTAGE: [(&str, &str); 7] = [
    ("Sunday", "Sonntag"),
    ("Monday", "Montag"),
    ("Tuesday", "Dienstag"),
    ("Wednesday", "Mittwoch"),
    ("Thursday", "Donnerstag"),
    ("Friday", "Freitag"),
    ("Saturday", "Samstag"),
];

/// Timer: eingestellte Dauer, Rest, Ende (läuft).
#[derive(Default)]
struct Timer {
    soll_s: u32,
    /// Rest in ms, solange er nicht läuft
    rest_ms: u64,
    /// Ende (zeit_ns), wenn er läuft
    ende_ns: Option<u64>,
}

impl Timer {
    fn rest_ms(&self, jetzt_ns: u64) -> u64 {
        match self.ende_ns {
            Some(e) => e.saturating_sub(jetzt_ns) / 1_000_000,
            None => self.rest_ms,
        }
    }

    /// Anzeige „MM:SS“ (Sekunden aufgerundet).
    fn text(&self, jetzt_ns: u64) -> String {
        let s = self.rest_ms(jetzt_ns).div_ceil(1000);
        format!("{:02}:{:02}", s / 60, s % 60)
    }

    fn stellen(&mut self, delta_s: i32) {
        if self.ende_ns.is_some() {
            return;
        }
        self.soll_s = (self.soll_s as i32 + delta_s).clamp(0, 99 * 60 + 59) as u32;
        self.rest_ms = u64::from(self.soll_s) * 1000;
    }
}

struct Zustand {
    reiter: u32,
    timer: Timer,
    /// Klingeln: seit (zeit_ns) und letzter Ton
    klingeln_seit: Option<u64>,
    letzter_ton: u64,
    /// zuletzt gezeichnete Zeitanzeige (nur bei Änderung neu zeichnen)
    gezeichnet: String,
}

fn wecker() -> (u32, bool) {
    let w = konfig::WECKER.load(Relaxed);
    (w & 0xFFFF, w & 1 << 16 != 0)
}

/// Grosse Anzeige des aktuellen Reiters.
fn anzeige_text(z: &Zustand, jetzt_ns: u64) -> String {
    if z.klingeln_seit.is_some() {
        return match KLINGELN.load(Relaxed) {
            1 => {
                let (m, _) = wecker();
                format!("{} {:02}:{:02}", kern::sprache::t("Alarm", "Wecker"), m / 60, m % 60)
            }
            _ => kern::sprache::t("Timer done", "Timer abgelaufen").to_string(),
        };
    }
    match z.reiter {
        0 => match lokal_jetzt() {
            Some(l) => {
                let s = l.rem_euclid(86_400);
                format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
            }
            None => "--:--:--".into(),
        },
        1 => {
            let (m, _) = wecker();
            format!("{:02}:{:02}", m / 60, m % 60)
        }
        _ => z.timer.text(jetzt_ns),
    }
}

fn zeichnen(z: &mut Zustand) {
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xE8EAEE, 0, Some(kern::sprache::t("Clock", "Uhr")));
    z.gezeichnet.clear();
    if z.klingeln_seit.is_some() {
        klingeln_zeichnen(z);
        sys::neuzeichnen(2);
        return;
    }
    let namen = [kern::sprache::t("Clock", "Uhr"), kern::sprache::t("Alarm", "Wecker"), "Timer"];
    for (i, n) in namen.iter().enumerate() {
        let aktiv = z.reiter == i as u32;
        knopf(K_REITER + i as u32, n, if aktiv { 0x3A7BD5 } else { 0xD8D8D8 }, if aktiv { 0xFFFFFF } else { 0x202020 }, 2);
    }
    match z.reiter {
        0 => {}
        1 => {
            knopf(K_STUNDE_PLUS, "+", 0xC9D6EA, 0x202020, 2);
            knopf(K_STUNDE_MINUS, "-", 0xC9D6EA, 0x202020, 2);
            knopf(K_MINUTE_PLUS, "+", 0xC9D6EA, 0x202020, 2);
            knopf(K_MINUTE_MINUS, "-", 0xC9D6EA, 0x202020, 2);
            let (_, an) = wecker();
            let t = if an { kern::sprache::t("On", "An") } else { kern::sprache::t("Off", "Aus") };
            knopf(K_WECKER_AN, t, if an { 0x3A9D5A } else { 0xD8D8D8 }, if an { 0xFFFFFF } else { 0x202020 }, 2);
        }
        _ => {
            knopf(K_T_MIN_PLUS, "+1m", 0xC9D6EA, 0x202020, 2);
            knopf(K_T_MIN_MINUS, "-1m", 0xC9D6EA, 0x202020, 2);
            knopf(K_T_SEK_PLUS, "+10s", 0xC9D6EA, 0x202020, 2);
            knopf(K_T_SEK_MINUS, "-10s", 0xC9D6EA, 0x202020, 2);
            let t = if z.timer.ende_ns.is_some() { kern::sprache::t("Pause", "Pause") } else { "Start" };
            knopf(K_T_START, t, 0x3A7BD5, 0xFFFFFF, 2);
            knopf(K_T_ZURUECK, kern::sprache::t("Reset", "Zurück"), 0xD8D8D8, 0x202020, 2);
        }
    }
    anzeige_zeichnen(z, sys::zeit_ns());
    sys::neuzeichnen(2);
}

/// Nur die Zeitanzeige neu (jede Sekunde bzw. nach +/−).
fn anzeige_zeichnen(z: &mut Zustand, jetzt_ns: u64) {
    let s = anzeige_text(z, jetzt_ns);
    if s == z.gezeichnet || z.klingeln_seit.is_some() {
        return;
    }
    anzeige_merken(&s);
    z.gezeichnet = s.clone();
    match z.reiter {
        0 => {
            // Uhrzeit ×4, Datum ×2, Wecker-Stand ×1
            sys::rechteck(IW as u32, 50 << 16 | 200, 0xE8EAEE);
            text_mittig(0, IW, 64, &s, 0x101010, 4);
            if let Some(l) = lokal_jetzt() {
                let tage = l.div_euclid(86_400);
                let (j, m, d) = kern::zeit::datum_aus_tagen(tage);
                let (en, de) = WOCHENTAGE[kern::zeit::wochentag(tage) as usize % 7];
                let datum = format!("{}, {d:02}.{m:02}.{j}", kern::sprache::t(en, de));
                text_mittig(0, IW, 150, &datum, 0x303030, 2);
            }
            let (m, an) = wecker();
            let w = if an {
                format!("{} {:02}:{:02}", kern::sprache::t("Alarm", "Wecker"), m / 60, m % 60)
            } else {
                kern::sprache::t("Alarm off", "Wecker aus").to_string()
            };
            text_mittig(0, IW, 210, &w, 0x505050, 1);
        }
        _ => {
            // „HH:MM“ bzw. „MM:SS“ ×4 zwischen den +/−-Tasten
            sys::rechteck((ZIFFERN_X as u32) << 16 | 160, (ZIFFERN_Y as u32) << 16 | 64, 0xE8EAEE);
            text(ZIFFERN_X, ZIFFERN_Y, &s, 0x101010, 4);
        }
    }
}

fn klingeln_zeichnen(z: &Zustand) {
    sys::rechteck(IW as u32, 250, 0xFFE6D8);
    let s = anzeige_text(z, 0);
    anzeige_merken(&s);
    text_mittig(0, IW, 50, &s, 0x802010, 3);
    knopf(K_STOPP, kern::sprache::t("Stop", "Stopp"), 0xD04030, 0xFFFFFF, 3);
}

/// Eigenes Fenster nach vorn (Wecker klingelt, Fenster liegt hinten).
fn nach_vorn() {
    let pid = kern::eigene_pid();
    if let Some(f) = sys::fenster_liste().iter().find(|f| Some(f.pid) == pid) {
        sys::system(3, f.slot as u32);
    }
}

/// Programm „uhr“.
pub fn programm() {
    let mut z = Zustand { reiter: 0, timer: Timer::default(), klingeln_seit: None, letzter_ton: 0, gezeichnet: String::new() };
    z.timer.stellen(5 * 60);
    loop {
        let e = sys::warte_ereignis_max(10);
        let jetzt = sys::zeit_ns();
        match e {
            1 => zeichnen(&mut z),
            3 => {
                let id = sys::knopf_id() >> 8;
                match id {
                    1 => {
                        // Schliessen beendet auch das Klingeln
                        KLINGELN.store(0, Relaxed);
                        return;
                    }
                    2..=4 => {
                        z.reiter = id - K_REITER;
                        zeichnen(&mut z);
                    }
                    K_STOPP => {
                        sys::debug_text("Uhr: Stopp");
                        KLINGELN.store(0, Relaxed);
                        z.klingeln_seit = None;
                        zeichnen(&mut z);
                    }
                    K_STUNDE_PLUS | K_STUNDE_MINUS | K_MINUTE_PLUS | K_MINUTE_MINUS | K_WECKER_AN => {
                        let (m, an) = wecker();
                        let (m, an) = match id {
                            K_STUNDE_PLUS => (m + 60, an),
                            K_STUNDE_MINUS => (m + 23 * 60, an),
                            K_MINUTE_PLUS => (m / 60 * 60 + (m % 60 + 1) % 60, an),
                            K_MINUTE_MINUS => (m / 60 * 60 + (m % 60 + 59) % 60, an),
                            _ => (m, !an),
                        };
                        if !konfig::wecker_setzen(m, an) {
                            sys::debug_text("Uhr: Wecker nicht gespeichert");
                        }
                        let (m, an) = wecker();
                        sys::debug_text(&format!("Uhr: Wecker {:02}:{:02} {}", m / 60, m % 60, if an { "an" } else { "aus" }));
                        if id == K_WECKER_AN {
                            zeichnen(&mut z);
                        } else {
                            anzeige_zeichnen(&mut z, jetzt);
                        }
                    }
                    K_T_MIN_PLUS | K_T_MIN_MINUS | K_T_SEK_PLUS | K_T_SEK_MINUS => {
                        z.timer.stellen(match id {
                            K_T_MIN_PLUS => 60,
                            K_T_MIN_MINUS => -60,
                            K_T_SEK_PLUS => 10,
                            _ => -10,
                        });
                        anzeige_zeichnen(&mut z, jetzt);
                    }
                    K_T_START => {
                        match z.timer.ende_ns {
                            Some(_) => {
                                z.timer.rest_ms = z.timer.rest_ms(jetzt);
                                z.timer.ende_ns = None;
                                sys::debug_text(&format!("Uhr: Timer Pause bei {}", z.timer.text(jetzt)));
                            }
                            None if z.timer.rest_ms > 0 => {
                                z.timer.ende_ns = Some(jetzt + z.timer.rest_ms * 1_000_000);
                                sys::debug_text(&format!("Uhr: Timer läuft ({})", z.timer.text(jetzt)));
                            }
                            None => {}
                        }
                        zeichnen(&mut z);
                    }
                    K_T_ZURUECK => {
                        z.timer.ende_ns = None;
                        z.timer.rest_ms = u64::from(z.timer.soll_s) * 1000;
                        zeichnen(&mut z);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Timer abgelaufen
        if z.timer.ende_ns.is_some_and(|e| jetzt >= e) {
            z.timer.ende_ns = None;
            z.timer.rest_ms = u64::from(z.timer.soll_s) * 1000;
            KLINGELN.store(2, Relaxed);
            sys::debug_text("Uhr: Timer abgelaufen");
        }
        // Klingeln beginnt (Timer oder Wecker aus der Taskleiste)
        let k = KLINGELN.load(Relaxed);
        if k != 0 && z.klingeln_seit.is_none() {
            z.klingeln_seit = Some(jetzt);
            z.letzter_ton = 0;
            z.reiter = if k == 1 { 1 } else { 2 };
            nach_vorn();
            zeichnen(&mut z);
        }
        if let Some(seit) = z.klingeln_seit {
            if jetzt.saturating_sub(seit) > 60_000_000_000 {
                // nach einer Minute von selbst still
                sys::debug_text("Uhr: Klingeln nach 60 s beendet");
                KLINGELN.store(0, Relaxed);
                z.klingeln_seit = None;
                zeichnen(&mut z);
            } else if z.letzter_ton == 0 || jetzt.saturating_sub(z.letzter_ton) >= 1_000_000_000 {
                z.letzter_ton = jetzt.max(1);
                konfig::SIGNAL.fetch_add(1, Relaxed);
            }
        } else {
            anzeige_zeichnen(&mut z, jetzt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_stellen_und_anzeigen() {
        let mut t = Timer::default();
        t.stellen(5 * 60);
        assert_eq!(t.text(0), "05:00");
        t.stellen(-60);
        t.stellen(10);
        assert_eq!(t.text(0), "04:10");
        t.stellen(-1000);
        assert_eq!(t.text(0), "00:00");
        t.stellen(10);
        // läuft: 10 s ab 0 → nach 0,5 s zeigt er aufgerundet 10, nach 9,2 s 1
        t.ende_ns = Some(10_000_000_000);
        assert_eq!(t.text(500_000_000), "00:10");
        assert_eq!(t.text(9_200_000_000), "00:01");
        assert_eq!(t.text(11_000_000_000), "00:00");
        // während er läuft, lässt er sich nicht verstellen
        t.stellen(60);
        assert_eq!(t.soll_s, 10);
    }
}
