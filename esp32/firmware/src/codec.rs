//! ES8311 Audio-Codec (Wiedergabe) und Lautsprecher-Verstärker.

use crate::board::{ADDR_CODEC, I2C_HZ, PA_EN};
use crate::hw::{delay_ms, output};
use crate::i2c::Bus;

/// Konfiguriert den ES8311 als I²S-Slave, 16 Bit, Takt aus BCLK
/// (BCLK = 32·fs → interner Takt ×8 = 256·fs, unabhängig vom MCLK-Pin),
/// DAC an, Lautstärke −12 dB; danach den Verstärker einschalten.
/// Registerfolge nach dem Espressif-Treiber (esp_codec_dev).
pub fn init(bus: &Bus) -> bool {
    let Ok(dev) = bus.device(ADDR_CODEC, I2C_HZ) else { return false };
    let seq: &[(u8, u8)] = &[
        (0x45, 0x00),
        (0x01, 0x30),
        (0x02, 0x00),
        (0x03, 0x10),
        (0x16, 0x24),
        (0x04, 0x10),
        (0x05, 0x00),
        (0x0B, 0x00),
        (0x0C, 0x00),
        (0x10, 0x1F),
        (0x11, 0x7F),
        (0x00, 0x80), // Zustandsmaschine an, Slave-Betrieb (Bit 6 = 0)
        (0x01, 0xBF), // interner MCLK aus SCLK (Bit 7), alle Takte an
        (0x06, 0x03), // SCLK nicht invertiert
        (0x02, 0x18), // Vorteiler 1, Vervielfacher ×8 (32·fs → 256·fs)
        (0x03, 0x10), // Single-Speed, ADC-OSR
        (0x04, 0x10), // DAC-OSR
        (0x05, 0x00), // ADC/DAC-Teiler 1
        (0x07, 0x00),
        (0x08, 0xFF), // LRCK-Teiler 256 (nur im Master-Betrieb relevant)
        (0x09, 0x0C), // DAC-Eingang: I²S, 16 Bit, nicht stumm
        (0x0A, 0x0C), // ADC-Ausgang: I²S, 16 Bit
        (0x13, 0x10),
        (0x1B, 0x0A),
        (0x1C, 0x6A),
        (0x17, 0xBF),
        (0x0E, 0x02),
        (0x12, 0x00), // DAC einschalten
        (0x14, 0x1A),
        (0x0D, 0x01), // Analogteil einschalten
        (0x15, 0x40),
        (0x37, 0x08),
        (0x45, 0x00),
        (0x31, 0x00), // DAC nicht stumm
        // DAC-Lautstärke: 0xBF = 0 dB, 0,5 dB je Schritt. −12 dB (0xA7):
        // 0 dB und −6 dB waren am Lautsprecher zu laut.
        (0x32, 0xA7),
    ];
    let mut ok = true;
    for &(r, v) in seq {
        ok &= dev.write(&[r, v]);
        if r == 0x00 || r == 0x0D {
            delay_ms(5);
        }
    }
    // Verstärker erst nach dem Codec an – kein Knacken beim Einschalten.
    ok &= output(PA_EN, true).is_ok();
    ok
}
