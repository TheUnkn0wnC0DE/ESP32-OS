// ESP32-OS: Updates über WLAN (U1, Auftrag 12.7). HTTPS mit
// Zertifikatsprüfung (ESP-IDF-Zertifikatsbündel – nie abgeschaltet),
// Weiterleitungen (GitHub leitet Release-Dateien auf einen anderen Rechner
// um), Abbild direkt in den zweiten ESP32-OS-Platz schreiben, SHA-256 beim
// Schreiben, esp_ota_end prüft das Abbild (Kopf, Chip, Prüfsumme). Erst
// dann wird der Platz als Startplatz gesetzt; startet die neue Version nicht
// sauber, nimmt der Bootloader die alte (Rollback, B0).

#include <stdbool.h>
#include <string.h>
#include "esp_crt_bundle.h"
#include "esp_http_client.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_heap_caps.h"
#include "esp_partition.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "mbedtls/sha256.h"

static const char *TAG = "update";

// Flash-Sperre (flash_sperre.c): esp_ota_* lesen otadata bzw. das Abbild
// über abgebildeten Flash – nie gleichzeitig mit Schreiben anderer Tasks
void esp32os_flash_sperren(void);
void esp32os_flash_freigeben(void);

// Verbindung öffnen und Weiterleitungen folgen. Liefert den Griff (offen,
// Kopf gelesen) oder NULL; *status = HTTP-Status bzw. negativer Fehler.
static esp_http_client_handle_t oeffnen(const char *url, int json, int64_t *laenge, int *status) {
    esp_http_client_config_t c;
    memset(&c, 0, sizeof c);
    c.url = url;
    c.crt_bundle_attach = esp_crt_bundle_attach;
    c.timeout_ms = 20000;
    c.user_agent = "ESP32-OS";
    c.buffer_size = 2048;
    c.buffer_size_tx = 1024;
    c.max_redirection_count = 6;
    c.keep_alive_enable = false;
    esp_http_client_handle_t h = esp_http_client_init(&c);
    if (!h) {
        *status = -1;
        return NULL;
    }
    if (json) esp_http_client_set_header(h, "Accept", "application/vnd.github+json");
    for (int i = 0; i < 7; i++) {
        esp_err_t r = esp_http_client_open(h, 0);
        if (r != ESP_OK) {
            ESP_LOGW(TAG, "Verbindung fehlgeschlagen: %s", esp_err_to_name(r));
            *status = -(int)r;
            esp_http_client_cleanup(h);
            return NULL;
        }
        *laenge = esp_http_client_fetch_headers(h);
        *status = esp_http_client_get_status_code(h);
        if (*status == 301 || *status == 302 || *status == 303 || *status == 307 || *status == 308) {
            esp_http_client_set_redirection(h);
            esp_http_client_close(h);
            continue;
        }
        return h;
    }
    *status = -2;
    esp_http_client_cleanup(h);
    return NULL;
}

// Text holen (JSON der Release-Abfrage, Prüfsummendatei). Rückgabe: Länge
// (≥ 0) oder negativer Fehler; *status = HTTP-Status.
int esp32os_https_text(const char *url, int json, char *puffer, int max, int *status) {
    int64_t laenge = 0;
    esp_http_client_handle_t h = oeffnen(url, json, &laenge, status);
    if (!h) return *status < 0 ? *status : -1;
    int n = 0;
    while (n < max - 1) {
        int k = esp_http_client_read(h, puffer + n, max - 1 - n);
        if (k <= 0) break;
        n += k;
    }
    puffer[n] = 0;
    esp_http_client_close(h);
    esp_http_client_cleanup(h);
    return n;
}

// ---- Schreib-Task: Flash löschen/schreiben verlangt einen Stapel im
// internen RAM (cache_utils.c: esp_task_stack_is_sane_cache_disabled).
// Programme und der Update-Faden laufen mit Stapel im PSRAM – darum führt
// ein kleiner Task mit internem Stapel die OTA-Aufrufe aus.
enum { OTA_ANFANG, OTA_SCHREIBEN, OTA_ENDE, OTA_ABBRUCH, OTA_STOPP, OTA_KOPF_LOESCHEN, OTA_KOPF_LESEN };
typedef struct {
    int befehl;
    const esp_partition_t *p;
    const void *daten;
    size_t n;
    esp_ota_handle_t ota;
    esp_err_t ergebnis;
    int startplatz;      /* OTA_ENDE: auch Startplatz setzen (Firmware-Update) */
    unsigned char kopf;  /* OTA_KOPF_LESEN: erstes Byte */
} ota_auftrag_t;
static QueueHandle_t ota_schlange;
static SemaphoreHandle_t ota_fertig;

static void ota_schreiber(void *arg) {
    (void)arg;
    for (;;) {
        ota_auftrag_t *a;
        if (xQueueReceive(ota_schlange, &a, portMAX_DELAY) != pdTRUE) continue;
        int stopp = a->befehl == OTA_STOPP;
        esp32os_flash_sperren();
        switch (a->befehl) {
        case OTA_ANFANG: a->ergebnis = esp_ota_begin(a->p, a->n, &a->ota); break;
        case OTA_SCHREIBEN: a->ergebnis = esp_ota_write(a->ota, a->daten, a->n); break;
        case OTA_ENDE:
            a->ergebnis = esp_ota_end(a->ota);
            // D3: Spiele-Abbilder (App-Store) werden nur geschrieben, nicht
            // zum Startplatz
            if (a->ergebnis == ESP_OK && a->startplatz) a->ergebnis = esp_ota_set_boot_partition(a->p);
            break;
        case OTA_ABBRUCH: a->ergebnis = esp_ota_abort(a->ota); break;
        case OTA_KOPF_LOESCHEN: a->ergebnis = esp_partition_erase_range(a->p, 0, 4096); break;
        case OTA_KOPF_LESEN: a->ergebnis = esp_partition_read(a->p, 0, &a->kopf, 1); break;
        default: a->ergebnis = ESP_OK; break;
        }
        esp32os_flash_freigeben();
        xSemaphoreGive(ota_fertig);
        if (stopp) vTaskDelete(NULL);
    }
}

/* Eine Sperre für alle Nutzer des Schreib-Tasks (Update, App-Store:
 * Schreiben, Kopf löschen/lesen) – sonst liefen zwei Schreib-Tasks an
 * derselben Schlange und demselben Semaphor (Review Teil D). */
static SemaphoreHandle_t ota_sperre;
static StaticSemaphore_t ota_sperre_puffer;
static portMUX_TYPE ota_sperre_mux = portMUX_INITIALIZER_UNLOCKED;

static bool ota_sperren(TickType_t warten) {
    taskENTER_CRITICAL(&ota_sperre_mux);
    if (!ota_sperre) ota_sperre = xSemaphoreCreateMutexStatic(&ota_sperre_puffer);
    taskEXIT_CRITICAL(&ota_sperre_mux);
    return xSemaphoreTake(ota_sperre, warten) == pdTRUE;
}

static void ota_freigeben(void) { xSemaphoreGive(ota_sperre); }

int esp32os_boot_platz(char *aus, int n);

/* Ist `p` der Platz, aus dem beim nächsten Start gebootet wird? */
static bool ist_startplatz(const esp_partition_t *p) {
    char l[20];
    return esp32os_boot_platz(l, sizeof l) == 0 && strcmp(l, p->label) == 0;
}

static esp_err_t ota_ausfuehren(ota_auftrag_t *a) {
    xQueueSend(ota_schlange, &a, portMAX_DELAY);
    xSemaphoreTake(ota_fertig, portMAX_DELAY);
    return a->ergebnis;
}

static int ota_schreiber_starten(void) {
    if (!ota_schlange) ota_schlange = xQueueCreate(1, sizeof(ota_auftrag_t *));
    if (!ota_fertig) ota_fertig = xSemaphoreCreateBinary();
    if (!ota_schlange || !ota_fertig) return -1;
    // Stapel aus dem internen RAM (xTaskCreate ohne Caps)
    return xTaskCreate(ota_schreiber, "ota_schreiber", 3072, NULL, 5, NULL) == pdPASS ? 0 : -1;
}

// Abbild in den App-Platz `label` laden. `sha` = erwartete SHA-256 (32
// Byte). fortschritt(prozent, kontext) wird alle paar Prozent gerufen.
// Rückgabe 0 = geschrieben, geprüft, Startplatz gesetzt; sonst Fehler:
// -1 Platz fehlt, -2 Verbindung, -3 Länge, -4 Prüfsumme, sonst esp_err_t.
static int bild_von_url(const char *url, const char *label, const unsigned char *sha,
                        void (*fortschritt)(int, void *), void *kontext, int startplatz) {
    const esp_partition_t *p = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_ANY, label);
    if (!p) return -1;
    if (p == esp_ota_get_running_partition()) return -1;
    // App-Abbild (ohne Startplatz-Wechsel) nie in den Startplatz schreiben
    if (!startplatz && ist_startplatz(p)) return -1;
    int64_t laenge = 0;
    int status = 0;
    esp_http_client_handle_t h = oeffnen(url, 0, &laenge, &status);
    if (!h) return -2;
    if (status != 200 || laenge <= 0 || laenge > (int64_t)p->size) {
        ESP_LOGW(TAG, "Abbild: Status %d, Länge %lld (Platz %lu)", status, (long long)laenge, (unsigned long)p->size);
        esp_http_client_close(h);
        esp_http_client_cleanup(h);
        return -3;
    }
    // Puffer im PSRAM (esp_flash kopiert über einen eigenen Puffer)
    char *puffer = heap_caps_malloc(4096, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (!puffer || ota_schreiber_starten() != 0) {
        free(puffer);
        esp_http_client_close(h);
        esp_http_client_cleanup(h);
        return ESP_ERR_NO_MEM;
    }
    ota_auftrag_t a = { .befehl = OTA_ANFANG, .p = p, .n = (size_t)laenge, .startplatz = startplatz };
    esp_err_t r = ota_ausfuehren(&a);
    if (r != ESP_OK) {
        a.befehl = OTA_STOPP;
        ota_ausfuehren(&a);
        free(puffer);
        esp_http_client_close(h);
        esp_http_client_cleanup(h);
        return r;
    }
    mbedtls_sha256_context s;
    mbedtls_sha256_init(&s);
    mbedtls_sha256_starts(&s, 0);
    int64_t n = 0;
    int zuletzt = -1;
    while (n < laenge) {
        int k = esp_http_client_read(h, puffer, 4096);
        if (k <= 0) break;
        mbedtls_sha256_update(&s, (const unsigned char *)puffer, k);
        a.befehl = OTA_SCHREIBEN;
        a.daten = puffer;
        a.n = (size_t)k;
        r = ota_ausfuehren(&a);
        if (r != ESP_OK) break;
        n += k;
        int pz = (int)(n * 100 / laenge);
        if (fortschritt && pz / 5 != zuletzt) {
            zuletzt = pz / 5;
            fortschritt(pz, kontext);
        }
    }
    esp_http_client_close(h);
    esp_http_client_cleanup(h);
    free(puffer);
    unsigned char ist[32];
    mbedtls_sha256_finish(&s, ist);
    mbedtls_sha256_free(&s);
    int fehler = 0;
    if (r != ESP_OK || n != laenge) {
        ESP_LOGW(TAG, "Abbild unvollständig: %lld von %lld Byte (%s)", (long long)n, (long long)laenge, esp_err_to_name(r));
        fehler = r != ESP_OK ? r : -3;
    } else if (memcmp(ist, sha, 32) != 0) {
        ESP_LOGW(TAG, "Prüfsumme des Abbilds stimmt nicht");
        fehler = -4;
    }
    if (fehler) {
        a.befehl = OTA_ABBRUCH;
        ota_ausfuehren(&a);
    } else {
        // esp_ota_end prüft das Abbild (Kopf, Chip, Prüfsumme), dann Startplatz
        a.befehl = OTA_ENDE;
        r = ota_ausfuehren(&a);
        if (r != ESP_OK) {
            ESP_LOGW(TAG, "Abbild ungültig: %s", esp_err_to_name(r));
            fehler = r;
        }
    }
    // App-Store: ein halb geschriebenes Abbild darf nicht als installiert
    // gelten (erstes Byte 0xE9 stünde schon da) – Kopf löschen
    if (fehler && !startplatz) {
        a.befehl = OTA_KOPF_LOESCHEN;
        ota_ausfuehren(&a);
    }
    a.befehl = OTA_STOPP;
    ota_ausfuehren(&a);
    if (fehler) return fehler;
    ESP_LOGI(TAG, "Abbild %lld Byte in %s geschrieben, geprüft%s", (long long)n, label, startplatz ? ", Startplatz gesetzt" : "");
    return 0;
}

int esp32os_bild_von_url(const char *url, const char *label, const unsigned char *sha,
                         void (*fortschritt)(int, void *), void *kontext, int startplatz) {
    ota_sperren(portMAX_DELAY);
    int r = bild_von_url(url, label, sha, fortschritt, kontext, startplatz);
    ota_freigeben();
    return r;
}

/* Update der Firmware: Abbild schreiben und als Startplatz setzen. */
int esp32os_ota_von_url(const char *url, const char *label, const unsigned char *sha,
                        void (*fortschritt)(int, void *), void *kontext) {
    return esp32os_bild_von_url(url, label, sha, fortschritt, kontext, 1);
}

/* D3: Kopf eines App-Abbilds löschen (App-Store „Entfernen“): ohne gültigen
 * Kopf startet es nicht und gilt als nicht installiert. Nie der laufende
 * Platz. 0 = ok. */
static int kopf_auftrag(const esp_partition_t *p, int befehl, unsigned char *kopf) {
    if (ota_schreiber_starten() != 0) return -1;
    ota_auftrag_t a = { .befehl = befehl, .p = p };
    esp_err_t r = ota_ausfuehren(&a);
    if (kopf) *kopf = a.kopf;
    ota_auftrag_t stopp = { .befehl = OTA_STOPP };
    ota_ausfuehren(&stopp);
    return r == ESP_OK ? 0 : (int)r;
}

int esp32os_bild_loeschen(const char *label) {
    const esp_partition_t *p = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_ANY, label);
    if (!p) return -1;
    if (p == esp_ota_get_running_partition() || ist_startplatz(p)) return -1;
    // Flash nur aus dem Schreib-Task (interner Stapel, Flash gesperrt)
    ota_sperren(portMAX_DELAY);
    int r = kopf_auftrag(p, OTA_KOPF_LOESCHEN, NULL);
    ota_freigeben();
    return r;
}

/* D3: liegt ein Abbild im Platz? (erstes Byte 0xE9 = ESP-Abbild) – 1 ja,
 * 0 nein, -1 Platz fehlt. */
int esp32os_bild_vorhanden(const char *label) {
    const esp_partition_t *p = esp_partition_find_first(ESP_PARTITION_TYPE_APP, ESP_PARTITION_SUBTYPE_ANY, label);
    if (!p) return -1;
    unsigned char b = 0;
    // läuft gerade ein Update/Download: unbekannt statt warten
    if (!ota_sperren(0)) return -1;
    int r = kopf_auftrag(p, OTA_KOPF_LESEN, &b);
    ota_freigeben();
    if (r != 0) return -1;
    return b == 0xE9 ? 1 : 0;
}

// ---- Startplatz abfragen: esp_ota_get_boot_partition liest otadata über
// mmap (Cache wird gesperrt) – nur mit internem Stapel erlaubt. Für Aufrufer
// mit PSRAM-Stapel (Programme, Update-Faden) in einem kurzlebigen Task.
typedef struct {
    const esp_partition_t *p;
    SemaphoreHandle_t fertig;
} boot_abfrage_t;

static void boot_abfrage(void *arg) {
    boot_abfrage_t *b = arg;
    esp32os_flash_sperren();
    b->p = esp_ota_get_boot_partition();
    esp32os_flash_freigeben();
    xSemaphoreGive(b->fertig);
    vTaskDelete(NULL);
}

int esp32os_boot_platz(char *aus, int n) {
    boot_abfrage_t b = { NULL, xSemaphoreCreateBinary() };
    if (!b.fertig || n <= 0) return -1;
    if (xTaskCreate(boot_abfrage, "boot_abfrage", 3072, &b, 5, NULL) != pdPASS) {
        vSemaphoreDelete(b.fertig);
        return -1;
    }
    xSemaphoreTake(b.fertig, portMAX_DELAY);
    vSemaphoreDelete(b.fertig);
    strncpy(aus, b.p ? b.p->label : "?", (size_t)n - 1);
    aus[n - 1] = 0;
    return b.p ? 0 : -1;
}
