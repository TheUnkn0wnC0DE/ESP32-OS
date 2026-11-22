// ESP32-OS: Hardware-JPEG-Dekoder des ESP32-P4 (esp_driver_jpeg) für den
// Bildbetrachter (P2). Ausgabe YUV444 (Graustufen: 1 Byte), die Umrechnung
// nach RGB macht Rust mit vollem JFIF-Bereich – die Farbumrechnung der
// DMA2D-Einheit (BT.601/709) rechnet mit TV-Bereich und übersättigt.
// Nur Baseline-JPEG: der Treiber lehnt progressive Bilder ab (kein SOF0).

#include <string.h>
#include "driver/jpeg_decode.h"
#include "esp_heap_caps.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

static jpeg_decoder_handle_t dekoder;
static SemaphoreHandle_t sperre;

static int ausrichten(int v, int a) { return (v + a - 1) / a * a; }

// Dekodiert `daten` und ruft für das Ergebnis `fertig(ziel, bytes, breite,
// hoehe, zeilenbreite, bytes_je_pixel)` auf (3 = YUV444 gepackt, 1 = grau;
// Zeilen mit `zeilenbreite` Pixeln). Rückgabe: 0 ok, sonst ESP-Fehlercode
// (progressive JPEGs: kein SOF0 → ESP_ERR_NOT_FOUND/NOT_SUPPORTED).
int esp32os_jpeg_dekodieren(const uint8_t *daten, uint32_t laenge, void *ziel,
                            void (*fertig)(void *, const uint8_t *, uint32_t, uint32_t, uint32_t, uint32_t)) {
    if (sperre == NULL) {
        SemaphoreHandle_t s = xSemaphoreCreateMutex();
        if (s == NULL) return ESP_ERR_NO_MEM;
        sperre = s;
    }
    xSemaphoreTake(sperre, portMAX_DELAY);
    int r = ESP_OK;
    uint8_t *ein = NULL, *aus = NULL;
    if (dekoder == NULL) {
        jpeg_decode_engine_cfg_t cfg = {.intr_priority = 0, .timeout_ms = 2000};
        r = jpeg_new_decoder_engine(&cfg, &dekoder);
        if (r != ESP_OK) goto ende;
    }
    jpeg_decode_picture_info_t info;
    r = jpeg_decoder_get_info(daten, laenge, &info);
    if (r != ESP_OK) goto ende;
    // Ausgabe auf ganze MCUs (16×16 bei 4:2:0, 16×8 bei 4:2:2, sonst 8×8)
    int mw = 8, mh = 8;
    if (info.sample_method == JPEG_DOWN_SAMPLING_YUV420) { mw = 16; mh = 16; }
    else if (info.sample_method == JPEG_DOWN_SAMPLING_YUV422) { mw = 16; }
    int zb = ausrichten(info.width, mw), zh = ausrichten(info.height, mh);
    int grau = info.sample_method == JPEG_DOWN_SAMPLING_GRAY;
    int bpp = grau ? 1 : 3;
    jpeg_decode_memory_alloc_cfg_t ein_cfg = {.buffer_direction = JPEG_DEC_ALLOC_INPUT_BUFFER};
    jpeg_decode_memory_alloc_cfg_t aus_cfg = {.buffer_direction = JPEG_DEC_ALLOC_OUTPUT_BUFFER};
    size_t ein_n = 0, aus_n = 0;
    ein = jpeg_alloc_decoder_mem(laenge, &ein_cfg, &ein_n);
    aus = jpeg_alloc_decoder_mem((size_t)zb * zh * bpp, &aus_cfg, &aus_n);
    if (ein == NULL || aus == NULL) { r = ESP_ERR_NO_MEM; goto ende; }
    memcpy(ein, daten, laenge);
    jpeg_decode_cfg_t dcfg = {
        .output_format = grau ? JPEG_DECODE_OUT_FORMAT_GRAY : JPEG_DECODE_OUT_FORMAT_YUV444,
        .rgb_order = JPEG_DEC_RGB_ELEMENT_ORDER_BGR,
        .conv_std = JPEG_YUV_RGB_CONV_STD_BT601,
    };
    uint32_t n = 0;
    r = jpeg_decoder_process(dekoder, &dcfg, ein, laenge, aus, aus_n, &n);
    if (r != ESP_OK) goto ende;
    fertig(ziel, aus, info.width, info.height, zb, bpp);
ende:
    if (ein) heap_caps_free(ein);
    if (aus) heap_caps_free(aus);
    xSemaphoreGive(sperre);
    return r;
}
