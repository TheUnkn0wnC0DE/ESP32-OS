//! Zeit: Zähler seit Systemstart (Funktion 26.9/26.10), Uhrzeit und Datum
//! in BCD (Funktionen 3, 29) mit Zeitzone und EU-Sommerzeit (Schweiz:
//! MEZ/MESZ), Stellen der Uhr (Funktion 22).

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static START: OnceLock<Instant> = OnceLock::new();
/// Zeitzone in Sekunden östlich von UTC (Standard: MEZ +1 h).
static ZONE: AtomicI32 = AtomicI32::new(3600);
static SOMMERZEIT: AtomicBool = AtomicBool::new(true);
/// Korrektur der Systemuhr (Funktion 22), Sekunden (kein 64-Bit-Atomic auf
/// RISC-V-32, daher Mutex).
static KORREKTUR: Mutex<i64> = Mutex::new(0);

/// Sperre nur im Kern-Abschnitt (pthread-Objekt im System-Heap, siehe
/// `fenster::wecken`).
fn korrektur() -> i64 {
    crate::prozess::kern_abschnitt(|| *KORREKTUR.lock().unwrap_or_else(|e| e.into_inner()))
}

fn korrektur_setzen(k: i64) {
    crate::prozess::kern_abschnitt(|| *KORREKTUR.lock().unwrap_or_else(|e| e.into_inner()) = k);
}

pub(crate) fn start() {
    START.get_or_init(Instant::now);
}

fn seit_start() -> std::time::Duration {
    START.get_or_init(Instant::now).elapsed()
}

/// Hundertstelsekunden seit Start (Funktion 26.9), modulo 2³².
pub fn hundertstel() -> u32 {
    (seit_start().as_millis() / 10) as u32
}

/// Nanosekunden seit Start (Funktion 26.10).
pub fn nanos() -> u64 {
    seit_start().as_nanos() as u64
}

/// Internetzeit (W3b): eingeschaltet (Einstellungen „Datum & Zeit“).
static NTP_AN: AtomicBool = AtomicBool::new(true);
/// Zeitpunkt (Unix) des letzten NTP-Abgleichs, 0 = noch keiner. 32 Bit
/// (RISC-V-32 hat keine 64-Bit-Atomics; reicht bis 2106).
static NTP_GESTELLT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Soll die Uhr per NTP gestellt werden?
pub fn ntp_an() -> bool {
    NTP_AN.load(Ordering::Relaxed)
}

pub fn ntp_an_setzen(an: bool) {
    NTP_AN.store(an, Ordering::Relaxed);
}

/// Letzter NTP-Abgleich (Unix-Zeit), falls es einen gab.
pub fn ntp_gestellt_um() -> Option<i64> {
    match NTP_GESTELLT.load(Ordering::Relaxed) {
        0 => None,
        t => Some(i64::from(t)),
    }
}

/// Die Plattform meldet einen NTP-Abgleich (Systemuhr schon gestellt):
/// Korrektur zurücksetzen, Zeitpunkt merken.
pub fn ntp_gemeldet() {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    unix_setzen(s);
    NTP_GESTELLT.store(unix().clamp(1, i64::from(u32::MAX)) as u32, Ordering::Relaxed);
}

/// Zeitzone (Sekunden östlich UTC) und ob EU-Sommerzeit gilt.
pub fn zone_setzen(sekunden: i64, eu_sommerzeit: bool) {
    ZONE.store(sekunden as i32, Ordering::Relaxed);
    SOMMERZEIT.store(eu_sommerzeit, Ordering::Relaxed);
}

/// Unix-Zeit (UTC, Sekunden) inkl. Korrektur.
pub fn unix() -> i64 {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    s + korrektur()
}

/// Systemuhr so verschieben, dass `unix()` jetzt `ziel` liefert. Am Board
/// wird die Systemuhr selbst gestellt (sie läuft über einen Neustart
/// weiter); am PC bleibt die Rechneruhr unberührt (Korrektur im Kern).
pub fn unix_setzen(ziel: i64) {
    #[cfg(target_os = "espidf")]
    {
        #[repr(C)]
        struct Zeitwert {
            sek: i64,
            usek: i32,
        }
        unsafe extern "C" {
            fn settimeofday(tv: *const Zeitwert, tz: *const core::ffi::c_void) -> i32;
        }
        let tv = Zeitwert { sek: ziel, usek: 0 };
        // SAFETY: gültiger Zeitwert, keine Zeitzone (newlib).
        if unsafe { settimeofday(&tv, core::ptr::null()) } == 0 {
            korrektur_setzen(0);
            return;
        }
    }
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    korrektur_setzen(ziel - s);
}

/// Tage seit 1970-01-01 → (Jahr, Monat, Tag) (Algorithmus von H. Hinnant).
pub fn datum_aus_tagen(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (Jahr, Monat, Tag) → Tage seit 1970-01-01.
pub fn tage_aus_datum(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Wochentag 0 = Sonntag.
pub fn wochentag(tage: i64) -> u32 {
    (tage + 4).rem_euclid(7) as u32
}

fn letzter_sonntag(y: i64, m: u32) -> i64 {
    let letzter = tage_aus_datum(y, m, 31);
    letzter - i64::from(wochentag(letzter))
}

/// Lokale Zeit als Sekunden seit 1970 (Ortszeit), aus UTC.
pub fn lokal(utc: i64) -> i64 {
    let mut t = utc + i64::from(ZONE.load(Ordering::Relaxed));
    if SOMMERZEIT.load(Ordering::Relaxed) {
        let (y, _, _) = datum_aus_tagen(utc.div_euclid(86_400));
        // EU: letzter Sonntag März 01:00 UTC bis letzter Sonntag Oktober 01:00 UTC
        let beginn = letzter_sonntag(y, 3) * 86_400 + 3600;
        let ende = letzter_sonntag(y, 10) * 86_400 + 3600;
        if utc >= beginn && utc < ende {
            t += 3600;
        }
    }
    t
}

fn bcd(v: u32) -> u32 {
    (v / 10) << 4 | (v % 10)
}

/// Funktion 3: 0x00SSMMHH (BCD), Ortszeit.
pub fn uhrzeit_bcd() -> u32 {
    let t = lokal(unix()).rem_euclid(86_400) as u32;
    bcd(t % 60) << 16 | bcd(t / 60 % 60) << 8 | bcd(t / 3600)
}

/// Funktion 29: 0x00DDMMYY (BCD), Ortszeit.
pub fn datum_bcd() -> u32 {
    let (y, m, d) = datum_aus_tagen(lokal(unix()).div_euclid(86_400));
    bcd(d) << 16 | bcd(m) << 8 | bcd(y.rem_euclid(100) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kalender() {
        assert_eq!(datum_aus_tagen(0), (1970, 1, 1));
        assert_eq!(datum_aus_tagen(tage_aus_datum(2026, 11, 21)), (2026, 11, 21));
        assert_eq!(wochentag(tage_aus_datum(2026, 11, 21)), 6); // Samstag
        assert_eq!(datum_aus_tagen(tage_aus_datum(2024, 2, 29)), (2024, 2, 29));
    }

    #[test]
    fn sommerzeit_schweiz() {
        zone_setzen(3600, true);
        // 2026-07-01 12:00 UTC → 14:00 MESZ
        let s = tage_aus_datum(2026, 7, 1) * 86_400 + 12 * 3600;
        assert_eq!(lokal(s).rem_euclid(86_400), 14 * 3600);
        // 2026-12-01 12:00 UTC → 13:00 MEZ
        let w = tage_aus_datum(2026, 12, 1) * 86_400 + 12 * 3600;
        assert_eq!(lokal(w).rem_euclid(86_400), 13 * 3600);
        // Umstellung 29.3.2026 01:00 UTC
        let u = tage_aus_datum(2026, 3, 29) * 86_400 + 3600;
        assert_eq!(lokal(u - 1) - (u - 1), 3600);
        assert_eq!(lokal(u) - u, 7200);
    }

    #[test]
    fn bcd_kodierung() {
        assert_eq!(bcd(59), 0x59);
        assert_eq!(bcd(7), 0x07);
    }
}
