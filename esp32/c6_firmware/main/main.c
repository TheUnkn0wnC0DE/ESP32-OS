/* ESP32-OS, C2 (W2): Firmware des WLAN-Chips ESP32-C6. esp_hosted startet
 * sich selbst (Koprozessor-Kern, eh_cp_auto_init_task).
 *
 * Rückweg nach dem Update vom P4 aus (OTA): Die alte Firmware bleibt im
 * anderen App-Platz. Diese Firmware gilt erst als bestätigt, wenn sich der
 * P4 gemeldet hat (esp_hosted-Handshake, EH_CP_EVT_PRIVATE_RPC_READY) und
 * sie danach STABIL_S ohne Absturz lief (dann auch WLAN-Start auf dem C6).
 * Bis dahin (Probezeit):
 *  - meldet sich der P4 nicht innerhalb von PROBEZEIT_S → zurück zur alten
 *    Firmware;
 *  - startete der C6 nach einem Absturz/Wächter neu → sofort zurück (ein
 *    Absturz beim Start darf keine Endlosschleife ohne Rückweg werden).
 * Bestätigt wird über die SHA-256 des ELF dieser Firmware (NVS), und die
 * Rücknahme-Sperre des Bootloaders wird aufgehoben
 * (esp_ota_mark_app_valid_cancel_rollback; harmlos ohne Rollback). */
#include <string.h>
#include "esp_app_desc.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_partition.h"
#include "esp_system.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs.h"
#include "nvs_flash.h"
#include "eh_cp_event.h"

#define PROBEZEIT_S 90
#define STABIL_S 60

static const char *TAG = "esp32os_c6";
static volatile bool host_da;

static void ereignis(void *arg, esp_event_base_t basis, int32_t id, void *daten)
{
    (void)arg;
    (void)daten;
    if (basis == EH_CP_EVENT && id == EH_CP_EVT_PRIVATE_RPC_READY) {
        host_da = true;
    }
}

static bool gueltig(const esp_partition_t *p)
{
    esp_app_desc_t d;
    return p != NULL && esp_ota_get_partition_description(p, &d) == ESP_OK;
}

/* Vorherige Firmware: OTA schreibt immer in den Platz nach dem laufenden
 * (factory → ota_0 → ota_1 → ota_0 …). Läuft diese Firmware in ota_1, war
 * die vorherige in ota_0; läuft sie in ota_0, war es factory (oder ota_1,
 * wenn es kein factory gibt). NULL, wenn keine gültige da ist. */
static const esp_partition_t *alte_firmware(const esp_partition_t *lauf)
{
    const esp_partition_t *ota0 = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_APP_OTA_0, NULL);
    const esp_partition_t *ota1 = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_APP_OTA_1, NULL);
    const esp_partition_t *fab = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_APP_FACTORY, NULL);
    const esp_partition_t *wahl[2] = { NULL, NULL };
    if (lauf->subtype == ESP_PARTITION_SUBTYPE_APP_OTA_1) {
        wahl[0] = ota0;
        wahl[1] = fab;
    } else if (lauf->subtype == ESP_PARTITION_SUBTYPE_APP_OTA_0) {
        wahl[0] = fab;
        wahl[1] = ota1;
    } else {
        wahl[0] = ota0;
        wahl[1] = ota1;
    }
    for (int i = 0; i < 2; i++) {
        if (wahl[i] != NULL && wahl[i]->address != lauf->address && gueltig(wahl[i])) {
            return wahl[i];
        }
    }
    return NULL;
}

static void zurueck(const esp_partition_t *lauf, const char *grund)
{
    const esp_partition_t *alt = alte_firmware(lauf);
    if (alt == NULL) {
        ESP_LOGE(TAG, "%s, aber keine alte Firmware gefunden – bleibe", grund);
        return;
    }
    ESP_LOGE(TAG, "%s – zurück zur alten Firmware in %s", grund, alt->label);
    if (esp_ota_set_boot_partition(alt) == ESP_OK) {
        esp_restart();
    }
    ESP_LOGE(TAG, "Umschalten gescheitert – bleibe");
}

void app_main(void)
{
    const esp_partition_t *lauf = esp_ota_get_running_partition();
    uint8_t sha[32] = { 0 };
    esp_partition_get_sha256(lauf, sha);
    ESP_LOGI(TAG, "ESP32-OS C6-Firmware (esp_hosted 3.1.0, SDIO) in %s (0x%lx)", lauf->label, (unsigned long)lauf->address);
    esp_err_t r = nvs_flash_init();
    if (r != ESP_OK) {
        /* NVS nie löschen (Kalibrierung, Daten der alten Firmware): ohne
         * NVS gibt es keine Probezeit – nichts wird umgeschaltet */
        ESP_LOGW(TAG, "NVS nicht nutzbar (%s) – keine Probezeit", esp_err_to_name(r));
        return;
    }
    nvs_handle_t h;
    if (nvs_open("esp32os", NVS_READWRITE, &h) != ESP_OK) {
        ESP_LOGW(TAG, "NVS-Bereich nicht offen – keine Probezeit");
        return;
    }
    uint8_t ok[32] = { 0 };
    size_t n = sizeof ok;
    if (nvs_get_blob(h, "c6_ok", ok, &n) == ESP_OK && n == sizeof ok && memcmp(ok, sha, sizeof ok) == 0) {
        ESP_LOGI(TAG, "Firmware bestätigt");
        nvs_close(h);
        return;
    }
    /* Probezeit: Neustart nach Absturz oder Wächter → sofort zurück. Ein
     * Rücksetzen durch den P4 (EN) gilt als Einschalten, nicht als Absturz. */
    esp_reset_reason_t grund = esp_reset_reason();
    if (grund == ESP_RST_PANIC || grund == ESP_RST_INT_WDT || grund == ESP_RST_TASK_WDT || grund == ESP_RST_WDT) {
        nvs_close(h);
        zurueck(lauf, "Neustart nach Absturz in der Probezeit");
        return;
    }
    r = esp_event_handler_register(EH_CP_EVENT, EH_CP_EVT_PRIVATE_RPC_READY, ereignis, NULL);
    if (r != ESP_OK) {
        ESP_LOGW(TAG, "Ereignis nicht angemeldet (%s) – keine Probezeit", esp_err_to_name(r));
        nvs_close(h);
        return;
    }
    ESP_LOGI(TAG, "Probezeit: bis %d s auf den P4 warten, dann %d s stabil", PROBEZEIT_S, STABIL_S);
    for (int i = 0; i < PROBEZEIT_S * 10 && !host_da; i++) {
        vTaskDelay(pdMS_TO_TICKS(100));
    }
    if (!host_da) {
        nvs_close(h);
        zurueck(lauf, "P4 meldet sich nicht");
        return;
    }
    vTaskDelay(pdMS_TO_TICKS(STABIL_S * 1000));
    for (int versuch = 0; versuch < 3; versuch++) {
        if (nvs_set_blob(h, "c6_ok", sha, sizeof sha) == ESP_OK && nvs_commit(h) == ESP_OK) {
            esp_ota_mark_app_valid_cancel_rollback();
            ESP_LOGI(TAG, "P4 gemeldet und %d s stabil – Firmware bestätigt", STABIL_S);
            nvs_close(h);
            return;
        }
        vTaskDelay(pdMS_TO_TICKS(500));
    }
    /* Nicht gespeichert: beim nächsten Start wieder Probezeit (der P4
     * meldet sich dann erneut) – nicht umschalten */
    ESP_LOGE(TAG, "Bestätigung nicht gespeichert – nächster Start wieder mit Probezeit");
    nvs_close(h);
}
