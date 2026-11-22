//! Quake auf dem Board: Die Spieldaten (pak0.pak, 18,7 MB) liegen seit B0
//! auf der SD-Karte unter `ESP32-OS/quake/pak0.pak` (Auftrag 12.8: nicht in
//! der Firmware; für zwei OTA-Plätze ist im Flash kein Platz mehr). Gelesen
//! wird stückweise; Demos öffnen dieselbe Datei mit `fopen`. Konfiguration
//! liegt auf der FAT-Partition unter `/daten/quake/id1`.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

/// Ordner für Quake (Konfiguration), ohne „/“ am Ende.
pub const ORDNER: &str = "/daten/quake";
/// Spieldaten auf der SD-Karte (mit `fopen` lesbar).
pub const PAK_DATEI: &str = "/sdcard/ESP32-OS/quake/pak0.pak";

static DATEI: Mutex<Option<std::fs::File>> = Mutex::new(None);

fn lesen(pos: usize, ziel: &mut [u8]) -> bool {
    let mut g = DATEI.lock().unwrap_or_else(|e| e.into_inner());
    let Some(f) = g.as_mut() else { return false };
    f.seek(SeekFrom::Start(pos as u64)).is_ok() && f.read_exact(ziel).is_ok()
}

/// Spieldaten öffnen (Größe aus dem PAK-Kopf: Verzeichnis am Ende).
pub fn pak() -> Option<quake::Pak> {
    if crate::speicher::sd_einbinden() != 0 {
        log::error!("Quake: keine SD-Karte – pak0.pak gehört nach ESP32-OS/quake/ auf die SD");
        return None;
    }
    let f = std::fs::File::open(PAK_DATEI).inspect_err(|e| log::error!("Quake: {PAK_DATEI}: {e}")).ok()?;
    let laenge = f.metadata().map(|m| m.len()).unwrap_or(0) as usize;
    *DATEI.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    let mut kopf = [0u8; 12];
    if !lesen(0, &mut kopf) || &kopf[..4] != b"PACK" {
        log::error!("Quake: {PAK_DATEI} ist keine PAK");
        return None;
    }
    let ofs = u32::from_le_bytes([kopf[4], kopf[5], kopf[6], kopf[7]]) as usize;
    let len = u32::from_le_bytes([kopf[8], kopf[9], kopf[10], kopf[11]]) as usize;
    let groesse = ofs + len;
    if groesse > laenge {
        log::error!("Quake: PAK-Kopf passt nicht zur Datei ({groesse} > {laenge})");
        return None;
    }
    log::info!("Quake: PAK {} KB auf der SD-Karte", groesse / 1024);
    Some(quake::Pak::Lesen { groesse, lesen })
}

/// FAT-Partition einbinden und Konfigurationsordner anlegen.
pub fn ordner_anlegen() {
    if crate::daten::einbinden()
        && let Err(e) = std::fs::create_dir_all("/daten/quake/id1")
    {
        log::error!("Quake: Ordner /daten/quake: {e}");
    }
}
