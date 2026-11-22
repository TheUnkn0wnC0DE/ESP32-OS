//! FAT-Partition „daten“ (16 MB im Flash ab B0, mit Verschleißausgleich)
//! unter `/daten`: ESP32-OS-Einstellungen (`/sys`), Doom-WAD,
//! -Konfiguration und -Spielstände, Quake-Konfiguration, die Würfelwelt.

use crate::hw::zeroed;
use esp_idf_svc::sys::{self, esp};
use std::sync::atomic::{AtomicBool, Ordering};

static EINGEBUNDEN: AtomicBool = AtomicBool::new(false);

/// Einbinden (beim ersten Mal formatieren); mehrfacher Aufruf schadet nicht.
pub fn einbinden() -> bool {
    if EINGEBUNDEN.load(Ordering::Relaxed) {
        return true;
    }
    let mut cfg: sys::esp_vfs_fat_mount_config_t = zeroed();
    cfg.format_if_mount_failed = true;
    cfg.max_files = 8;
    let mut wl: sys::wl_handle_t = 0;
    // SAFETY: gültige Zeichenketten und Konfiguration.
    let r = unsafe { esp!(sys::esp_vfs_fat_spiflash_mount_rw_wl(c"/daten".as_ptr(), c"daten".as_ptr(), &cfg, &mut wl)) };
    match r {
        Ok(()) => {
            EINGEBUNDEN.store(true, Ordering::Relaxed);
            true
        }
        Err(e) => {
            log::error!("FAT-Partition „daten“: {e} – es wird nichts gespeichert");
            false
        }
    }
}
