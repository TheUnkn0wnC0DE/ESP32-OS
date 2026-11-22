//! Flash-Sperre (komponenten/esp32os/flash_sperre.c, Befund 22.11.): Mit
//! Code im PSRAM sperrt ESP-IDF beim Flash-Schreiben den Cache nicht; wer
//! abgebildeten Flash liest (esp_ota_* über read_otadata, Prüfung eines
//! Abbilds), muss deshalb die Sperre halten, die jede SPI1-Operation nimmt.

unsafe extern "C" {
    fn esp32os_flash_sperre_einrichten();
    fn esp32os_flash_sperren();
    fn esp32os_flash_freigeben();
    fn esp32os_flashtest(sekunden: i32, gesperrt: i32) -> i32;
}

/// Einmal beim Start, bevor andere Tasks den Flash benutzen.
pub fn einrichten() {
    // SAFETY: legt die Sperre an und hüllt die Haken des Standard-Flashs.
    unsafe { esp32os_flash_sperre_einrichten() }
}

/// `f` unter der Flash-Sperre (kein gleichzeitiges Schreiben/Löschen).
pub fn gesperrt<R>(f: impl FnOnce() -> R) -> R {
    struct Frei;
    impl Drop for Frei {
        fn drop(&mut self) {
            // SAFETY: gibt die oben genommene (rekursive) Sperre zurück.
            unsafe { esp32os_flash_freigeben() }
        }
    }
    // SAFETY: rekursive Sperre, im eigenen Task genommen und freigegeben.
    unsafe { esp32os_flash_sperren() };
    let _frei = Frei;
    f()
}

/// Entwicklungs-Build `ESP32OS_FLASHTEST=ohne:SEK` bzw. `mit:SEK`:
/// Belastungstest (komponenten/esp32os/flashtest.c) – schreibt nur ins Ende
/// des nicht laufenden Platzes esp32os_b, nie ins Dateisystem.
pub fn test(vorgabe: &str) {
    let (art, sek) = vorgabe.split_once(':').unwrap_or((vorgabe, "60"));
    let sek: i32 = sek.trim().parse().unwrap_or(60);
    let mit = art.trim() == "mit";
    // SAFETY: blockiert für `sek` Sekunden, eigene Tasks mit internem Stapel.
    let r = unsafe { esp32os_flashtest(sek, i32::from(mit)) };
    if r != 0 {
        log::error!("Flashtest nicht gestartet ({r})");
    }
}
