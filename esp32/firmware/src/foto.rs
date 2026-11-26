//! Bildschirmfotos aus dem angezeigten Bildspeicher über die serielle
//! Schnittstelle – für die Entwicklung (`KONSOLE_BILDER=10,25` beim Bauen:
//! Sekunden nach dem Start). Gegenstück am PC: `bildschirmfoto.py`.
//!
//! Ablauf: Ankündigung `BILD <nr> <breite> <höhe> <baud> <gedreht>`, dann
//! `BILDDATA`, die Pixel (RGB565, little endian, Panel hochkant) und eine
//! 32-Bit-Summe aller Bytes. Andere Logausgaben schweigen solange.

use crate::board::{LCD_H, LCD_W};
use crate::display::FB_BYTES;
use esp_idf_svc::sys;

/// USB überträgt unabhängig von der Baudrate; die Ankündigung nennt sie nur,
/// weil `bildschirmfoto.py` sie erwartet.
const BAUD: u32 = 115_200;

/// Sendet den Bildspeicher `fb` (blockiert einige Sekunden).
pub fn senden(fb: *const u16, nummer: u32, gedreht: bool) {
    log::info!("BILD {nummer} {LCD_W} {LCD_H} {BAUD} {}", gedreht as u8);
    crate::hw::LOG_STUMM.store(true, core::sync::atomic::Ordering::Release);
    // SAFETY: USB-Treiber installiert (hw::log_einrichten); `fb` ist ein
    // ganzer Bildspeicher, den der PPA gerade nicht beschreibt.
    unsafe {
        sys::usb_serial_jtag_wait_tx_done(1000);
        sys::vTaskDelay(300);
        // Der PPA schrieb per DMA: veraltete Cache-Zeilen verwerfen.
        sys::esp_cache_msync(fb as *mut _, FB_BYTES, crate::hw::CACHE_MSYNC_DIR_M2C as _);
        let daten = core::slice::from_raw_parts(fb as *const u8, FB_BYTES);
        let mut summe = 0u32;
        let senden = |teil: &[u8]| {
            let mut rest = teil;
            while !rest.is_empty() {
                // höchstens 1 s warten: liest niemand mit, wird abgebrochen
                let n = sys::usb_serial_jtag_write_bytes(rest.as_ptr().cast(), rest.len() as _, 1000);
                if n <= 0 {
                    return false;
                }
                rest = &rest[n as usize..];
            }
            true
        };
        let mut ok = senden(b"BILDDATA");
        for stueck in daten.chunks(2048) {
            if !ok {
                break;
            }
            summe = stueck.iter().fold(summe, |s, &b| s.wrapping_add(b as u32));
            ok = senden(stueck);
        }
        if ok {
            senden(&summe.to_le_bytes());
        }
        sys::usb_serial_jtag_wait_tx_done(5000);
        sys::vTaskDelay(300);
    }
    crate::hw::LOG_STUMM.store(false, core::sync::atomic::Ordering::Release);
    log::info!("BILD {nummer} gesendet");
}
