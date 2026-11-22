//! Bildschirmfotos aus dem angezeigten Bildspeicher über die serielle
//! Schnittstelle – für die Entwicklung (`KONSOLE_BILDER=10,25` beim Bauen:
//! Sekunden nach dem Start). Gegenstück am PC: `bildschirmfoto.py`.
//!
//! Ablauf: Ankündigung `BILD <nr> <breite> <höhe> <baud> <gedreht>` mit
//! 115200 Baud, dann kurz auf [`BAUD_SCHNELL`] umschalten, `BILDDATA`, die
//! Pixel (RGB565, little endian, Panel hochkant), eine 32-Bit-Summe aller
//! Bytes, und zurück auf 115200 Baud. Andere Logausgaben schweigen solange.

use crate::board::{LCD_H, LCD_W};
use crate::display::FB_BYTES;
use esp_idf_svc::sys;

/// 1,8 MB in ~6 s (bei 115200 Baud wären es 160 s).
pub const BAUD_SCHNELL: u32 = 3_000_000;
const BAUD_NORMAL: u32 = 115_200;

/// Sendet den Bildspeicher `fb` (blockiert einige Sekunden).
pub fn senden(fb: *const u16, nummer: u32, gedreht: bool) {
    log::info!("BILD {nummer} {LCD_W} {LCD_H} {BAUD_SCHNELL} {}", gedreht as u8);
    crate::hw::LOG_STUMM.store(true, core::sync::atomic::Ordering::Release);
    // SAFETY: UART0 mit Treiber (hw::log_einrichten); `fb` ist ein ganzer
    // Bildspeicher, den der PPA gerade nicht beschreibt (angezeigtes Bild).
    unsafe {
        sys::uart_wait_tx_done(0, 1000);
        sys::vTaskDelay(300);
        sys::uart_set_baudrate(0, BAUD_SCHNELL);
        sys::vTaskDelay(50);
        // Der PPA schrieb per DMA: veraltete Cache-Zeilen verwerfen.
        sys::esp_cache_msync(fb as *mut _, FB_BYTES, crate::hw::CACHE_MSYNC_DIR_M2C as _);
        let daten = core::slice::from_raw_parts(fb as *const u8, FB_BYTES);
        sys::uart_write_bytes(0, b"BILDDATA".as_ptr().cast(), 8);
        let mut summe = 0u32;
        for stueck in daten.chunks(4096) {
            summe = stueck.iter().fold(summe, |s, &b| s.wrapping_add(b as u32));
            sys::uart_write_bytes(0, stueck.as_ptr().cast(), stueck.len());
        }
        sys::uart_write_bytes(0, summe.to_le_bytes().as_ptr().cast(), 4);
        sys::uart_wait_tx_done(0, 5000);
        sys::vTaskDelay(100);
        sys::uart_set_baudrate(0, BAUD_NORMAL);
        sys::vTaskDelay(300);
    }
    crate::hw::LOG_STUMM.store(false, core::sync::atomic::Ordering::Release);
    log::info!("BILD {nummer} gesendet");
}
