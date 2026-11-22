// ESP32-OS: Flash-Sperre (Befund 22.11., STATUS „Flash schreiben“).
//
// Code und Konstanten laufen aus dem PSRAM (CONFIG_SPIRAM_XIP_FROM_PSRAM).
// Damit sperrt ESP-IDF beim Flash-Schreiben/-Löschen über SPI1 den Cache
// nicht mehr und hält den anderen Kern nicht an (spi_flash_os_func_app.c,
// SPI_FLASH_CACHE_NO_DISABLE) – nur ein Mutex schützt SPI1. Liest aber ein
// anderer Kern zur selben Zeit Flash über den Cache (abgebildeter Flash:
// esp_partition_mmap, etwa esp_ota_* über read_otadata oder die Prüfung
// des Abbilds in esp_ota_end), stürzt das Board ab bzw. hängt (am Board
// beobachtet: Store-/Load-Fehler auf beiden Kernen, Watchdog-Neustarts).
//
// Abhilfe: Eine rekursive Sperre, die jede SPI1-Operation des Standard-
// Flashs hält (die start/end-Haken von esp_flash_default_chip werden
// umhüllt) und die jeder nimmt, der abgebildeten Flash liest
// (esp32os_flash_sperren/freigeben, z. B. um esp_ota_*-Aufrufe). So laufen
// Schreiben und abgebildetes Lesen nie gleichzeitig.
#include <stdbool.h>
#include "esp_flash.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

static const char *TAG = "flash_sperre";

static SemaphoreHandle_t sperre;
static StaticSemaphore_t sperre_speicher;
static const esp_flash_os_functions_t *alt;
static esp_flash_os_functions_t neu;

static esp_err_t start(void *arg, uint32_t flags) {
    xSemaphoreTakeRecursive(sperre, portMAX_DELAY);
    esp_err_t r = alt->start(arg, flags);
    if (r != ESP_OK) xSemaphoreGiveRecursive(sperre);
    return r;
}

static esp_err_t ende(void *arg) {
    esp_err_t r = alt->end(arg);
    xSemaphoreGiveRecursive(sperre);
    return r;
}

// Einmal beim Start, bevor andere Tasks den Flash benutzen.
void esp32os_flash_sperre_einrichten(void) {
    if (sperre) return;
    sperre = xSemaphoreCreateRecursiveMutexStatic(&sperre_speicher);
    alt = esp_flash_default_chip ? esp_flash_default_chip->os_func : NULL;
    if (!alt || !alt->start || !alt->end) {
        ESP_LOGE(TAG, "Standard-Flash ohne start/end-Haken – Sperre nicht eingerichtet");
        return;
    }
    neu = *alt;
    neu.start = start;
    neu.end = ende;
    esp_flash_default_chip->os_func = &neu;
    ESP_LOGI(TAG, "eingerichtet (SPI1-Operationen und abgebildetes Lesen schliessen sich aus)");
}

// Vor dem Lesen von abgebildetem Flash (esp_ota_* usw.) nehmen.
void esp32os_flash_sperren(void) {
    if (sperre) xSemaphoreTakeRecursive(sperre, portMAX_DELAY);
}

void esp32os_flash_freigeben(void) {
    if (sperre) xSemaphoreGiveRecursive(sperre);
}
