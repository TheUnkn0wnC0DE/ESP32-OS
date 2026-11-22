//! B0-Umzug (Entwicklungs-Build `ESP32OS_PAK_RETTEN=1`): Nach dem Flashen
//! der neuen Aufteilung liegt die Quake-PAK noch roh im Flash, wo früher
//! die Partition „quakepak“ war (0xB90000). Bevor doomapp/quakeapp/daten
//! diesen Bereich überschreiben, wird sie auf die SD-Karte kopiert
//! (`ESP32-OS/quake/pak0.pak`) und per CRC32 geprüft (Quelle, geschriebene
//! Datei); den Wert vergleicht `bauen` mit der Datei auf dem PC.

use esp_idf_svc::sys;
use std::io::{Read, Write};

const ALT_OFFSET: u32 = 0xB9_0000;
const STUECK: usize = 64 * 1024;

fn crc32(mut c: u32, d: &[u8]) -> u32 {
    c = !c;
    for &x in d {
        c ^= u32::from(x);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn roh_lesen(adr: u32, buf: &mut [u8]) -> bool {
    // SAFETY: Standard-Flash, Puffer passt zur Länge.
    unsafe { sys::esp_flash_read(sys::esp_flash_default_chip, buf.as_mut_ptr().cast(), adr, buf.len() as u32) == 0 }
}

fn datei_crc(pfad: &str, buf: &mut [u8]) -> Option<(u32, usize)> {
    let mut f = std::fs::File::open(pfad).ok()?;
    let (mut c, mut n) = (0u32, 0usize);
    loop {
        match f.read(buf) {
            Ok(0) => return Some((c, n)),
            Ok(k) => {
                c = crc32(c, &buf[..k]);
                n += k;
            }
            Err(_) => return None,
        }
    }
}

/// PAK aus dem alten Flash-Bereich auf die SD-Karte kopieren und prüfen.
/// `erwartet` = CRC32 der PAK auf dem PC: Kopiert wird nur eine Quelle mit
/// genau dieser Prüfsumme, und eine schon richtige Kopie bleibt (das Board
/// startet nach dem Flashen zweimal; beim zweiten Mal kann der alte Bereich
/// schon von der neuen Daten-Partition überschrieben sein).
/// `groesse` = Dateilänge auf dem PC (hinter dem PAK-Verzeichnis liegen
/// noch Daten, die Länge aus dem Kopf wäre zu kurz).
pub fn pak_retten(erwartet: u32, groesse: usize) {
    let mut kopf = [0u8; 12];
    if !roh_lesen(ALT_OFFSET, &mut kopf) || &kopf[..4] != b"PACK" {
        log::error!("UMZUG: keine PAK bei 0x{ALT_OFFSET:X}");
        return;
    }
    if crate::speicher::sd_einbinden() != 0 {
        log::error!("UMZUG: keine SD-Karte");
        return;
    }
    let ziel = "/sdcard/ESP32-OS/quake/pak0.pak";
    let mut buf = vec![0u8; STUECK];
    if let Some((c, n)) = datei_crc(ziel, &mut buf)
        && c == erwartet
        && n == groesse
    {
        log::warn!("UMZUG: {ziel} schon vorhanden, CRC32 {c:08x} = erwartet – nichts zu tun");
        return;
    }
    // Quelle prüfen, bevor etwas geschrieben wird
    let (mut c, mut pos) = (0u32, 0usize);
    while pos < groesse {
        let n = STUECK.min(groesse - pos);
        if !roh_lesen(ALT_OFFSET + pos as u32, &mut buf[..n]) {
            log::error!("UMZUG: Lesefehler bei {pos}");
            return;
        }
        c = crc32(c, &buf[..n]);
        pos += n;
    }
    if c != erwartet {
        log::error!("UMZUG: Quelle CRC32 {c:08x} ≠ erwartet {erwartet:08x} – nichts kopiert");
        return;
    }
    if let Err(e) = std::fs::create_dir_all("/sdcard/ESP32-OS/quake") {
        log::error!("UMZUG: Ordner: {e}");
        return;
    }
    let t0 = crate::hw::now_us();
    let mut f = match std::fs::File::create(ziel) {
        Ok(f) => f,
        Err(e) => {
            log::error!("UMZUG: {ziel}: {e}");
            return;
        }
    };
    let (mut crc_quelle, mut pos) = (0u32, 0usize);
    while pos < groesse {
        let n = STUECK.min(groesse - pos);
        if !roh_lesen(ALT_OFFSET + pos as u32, &mut buf[..n]) || f.write_all(&buf[..n]).is_err() {
            log::error!("UMZUG: Fehler bei {pos}");
            return;
        }
        crc_quelle = crc32(crc_quelle, &buf[..n]);
        pos += n;
    }
    drop(f);
    // Gegenprobe: Datei zurücklesen
    let (crc_datei, gelesen) = datei_crc(ziel, &mut buf).unwrap_or((0, 0));
    log::warn!(
        "UMZUG: PAK {groesse} Bytes nach {ziel} in {} ms, CRC32 Quelle {crc_quelle:08x}, Datei {crc_datei:08x} ({gelesen} Bytes) – {}",
        (crate::hw::now_us() - t0) / 1000,
        if crc_quelle == erwartet && crc_datei == erwartet && gelesen == groesse { "GLEICH" } else { "ABWEICHUNG" }
    );
}
