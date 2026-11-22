//! Doom auf dem Board: Die WAD liegt seit B0 als Datei auf der internen
//! FAT-Partition (`/daten/doom/DOOM1.WAD`; beim Erststart von der SD-Karte
//! übernommen, Auftrag 12.8) und wird beim Start in den PSRAM kopiert;
//! Konfiguration und Spielstände liegen daneben unter `/daten/doom/`.


/// Ordner für Doom (Konfiguration, Spielstände), mit „/“ am Ende.
pub const ORDNER: &str = "/daten/doom/";

/// WAD auf der internen FAT-Partition (ab B0).
pub const WAD_DATEI: &str = "/daten/doom/DOOM1.WAD";
/// Erststart (Auftrag 12.8): von der SD-Karte übernehmen.
pub const WAD_SD: &str = "/sdcard/ESP32-OS/doom/DOOM1.WAD";

/// DOOM1.WAD in den PSRAM laden; fehlt sie intern, einmal von der SD-Karte
/// übernehmen. Prüft den WAD-Kopf.
pub fn wad_laden() -> Option<&'static [u8]> {
    if !crate::daten::einbinden() {
        return None;
    }
    let t0 = crate::hw::now_us();
    let v = match std::fs::read(WAD_DATEI) {
        Ok(v) => v,
        Err(_) => {
            log::warn!("Doom: {WAD_DATEI} fehlt – Erststart, suche {WAD_SD}");
            if crate::speicher::sd_einbinden() != 0 {
                log::error!("Doom: keine SD-Karte – DOOM1.WAD (Shareware) nach ESP32-OS/doom/ auf die SD legen");
                return None;
            }
            let v = std::fs::read(WAD_SD).inspect_err(|e| log::error!("Doom: {WAD_SD}: {e}")).ok()?;
            let _ = std::fs::create_dir_all("/daten/doom");
            match std::fs::write(WAD_DATEI, &v) {
                Ok(()) => log::info!("Doom: WAD von der SD übernommen ({} KB)", v.len() / 1024),
                Err(e) => log::error!("Doom: {WAD_DATEI} schreiben: {e}"),
            }
            v
        }
    };
    if v.len() < 12 || &v[..4] != b"IWAD" {
        log::error!("Doom: {WAD_DATEI} ist keine WAD");
        return None;
    }
    log::info!("Doom: WAD {} KB in {} ms geladen", v.len() / 1024, (crate::hw::now_us() - t0) / 1000);
    Some(Box::leak(v.into_boxed_slice()))
}

pub fn ordner_anlegen() -> bool {
    if !crate::daten::einbinden() {
        return false;
    }
    match std::fs::create_dir_all("/daten/doom/saves") {
        Ok(()) => true,
        Err(e) => {
            log::error!("Doom: Ordner /daten/doom: {e}");
            false
        }
    }
}
