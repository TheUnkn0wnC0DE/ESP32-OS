// ESP32-OS: Belastungstest „Flash schreiben, während abgebildeter Flash
// gelesen wird“ (Befund 22.11., STATUS). Mit Code im PSRAM (XIP) sperrt
// ESP-IDF den Cache beim Flash-Schreiben nicht mehr; liest dann ein anderer
// Kern Flash über den Cache (esp_partition_mmap, z. B. esp_ota_* über
// read_otadata), stürzte das Board ab.
//
// Schreiber (Kern 0): löscht und schreibt 64-KB-Blöcke im Ende des zweiten
// ESP32-OS-Platzes (esp32os_b) – hinter jedem Abbild, nie im Dateisystem
// mit Nutzerdaten. Verweigert, wenn dieser Platz läuft oder Startplatz ist.
// Leser (Kern 1): esp_ota_get_state_partition in einer Schleife – ohne
// bzw. mit der Flash-Sperre (flash_sperre.c).
// Nur für Entwicklungs-Builds (ESP32OS_FLASHTEST, firmware/src/main.rs).
#include <stdbool.h>
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_partition.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

void esp32os_flash_sperren(void);
void esp32os_flash_freigeben(void);

static const char *TAG = "flashtest";

#define BEREICH (512 * 1024)
#define BLOCK (64 * 1024)

static volatile bool laufen;
static volatile bool mit_sperre;
static volatile uint32_t bloecke, abfragen, fehler;
static const esp_partition_t *ziel;

static void schreiber(void *arg) {
    (void)arg;
    uint8_t *puffer = heap_caps_malloc(4096, MALLOC_CAP_INTERNAL);
    if (!puffer) {
        ESP_LOGE(TAG, "kein Puffer");
        vTaskDelete(NULL);
    }
    size_t anfang = ziel->size - BEREICH;
    for (uint32_t runde = 0; laufen; runde++) {
        for (size_t off = anfang; off < ziel->size && laufen; off += BLOCK) {
            memset(puffer, (int)(runde + off / BLOCK), 4096);
            if (esp_partition_erase_range(ziel, off, BLOCK) != ESP_OK) fehler++;
            for (size_t i = 0; i < BLOCK; i += 4096)
                if (esp_partition_write(ziel, off + i, puffer, 4096) != ESP_OK) fehler++;
            bloecke++;
        }
    }
    heap_caps_free(puffer);
    vTaskDelete(NULL);
}

static void leser(void *arg) {
    (void)arg;
    const esp_partition_t *lauf = esp_ota_get_running_partition();
    while (laufen) {
        esp_ota_img_states_t z;
        if (mit_sperre) esp32os_flash_sperren();
        esp_ota_get_state_partition(lauf, &z);
        if (mit_sperre) esp32os_flash_freigeben();
        abfragen++;
        // kurze Pause wie bei echten Abfragen (sonst nähme der Leser die
        // Sperre sofort wieder und der Schreiber käme nie dran – erster
        // Lauf: 0 Blöcke geschrieben); in beiden Arten gleich
        vTaskDelay(1);
    }
    vTaskDelete(NULL);
}

// Startet den Test für `sekunden`; `gesperrt`: Leser nimmt die Flash-Sperre.
// Meldet alle 5 s die Zähler; 0 = gestartet.
int esp32os_flashtest(int sekunden, int gesperrt) {
    ziel = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_ANY, "esp32os_b");
    if (!ziel || ziel == esp_ota_get_running_partition() || ziel == esp_ota_get_boot_partition() || ziel->size < BEREICH) {
        ESP_LOGE(TAG, "esp32os_b nicht verwendbar (läuft, ist Startplatz oder fehlt) – kein Test");
        return -1;
    }
    mit_sperre = gesperrt != 0;
    laufen = true;
    ESP_LOGW(TAG, "Start %d s, Leser %s Sperre; schreibe 0x%lx..0x%lx in %s", sekunden, mit_sperre ? "mit" : "ohne",
             (unsigned long)(ziel->address + ziel->size - BEREICH), (unsigned long)(ziel->address + ziel->size), ziel->label);
    xTaskCreatePinnedToCore(schreiber, "ft_schreiber", 4096, NULL, 5, NULL, 0);
    xTaskCreatePinnedToCore(leser, "ft_leser", 4096, NULL, 5, NULL, 1);
    int64_t start = esp_timer_get_time();
    while ((esp_timer_get_time() - start) / 1000000 < sekunden) {
        vTaskDelay(pdMS_TO_TICKS(5000));
        ESP_LOGW(TAG, "%lld s: %lu Blöcke (64 KB) geschrieben, %lu otadata-Abfragen, %lu Fehler",
                 (long long)((esp_timer_get_time() - start) / 1000000), (unsigned long)bloecke, (unsigned long)abfragen,
                 (unsigned long)fehler);
    }
    laufen = false;
    vTaskDelay(pdMS_TO_TICKS(500));
    ESP_LOGW(TAG, "Ende: %lu Blöcke, %lu Abfragen, %lu Fehler – kein Absturz", (unsigned long)bloecke, (unsigned long)abfragen,
             (unsigned long)fehler);
    return 0;
}
