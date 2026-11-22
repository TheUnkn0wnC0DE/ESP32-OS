/* ESP32-OS: Absturzmerker (D12). Der Panic-Handler von ESP-IDF wird per
 * Linker (--wrap=esp_panic_handler) umhüllt: vor dem Neustart Name des
 * abstürzenden Tasks (= Programmname) und Grund im RTC-Speicher ablegen,
 * der einen Software-Neustart übersteht. Nach dem Neustart zeigt der
 * Desktop die Meldung. */
#include <string.h>
#include "esp_attr.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_private/panic_internal.h"

#define MERKER 0x4F534352u /* "OSCR" */

typedef struct {
    uint32_t merker;
    char name[24];
    char grund[64];
} absturz_t;

RTC_NOINIT_ATTR static absturz_t absturz;

extern void __real_esp_panic_handler(panic_info_t* info);

static void kopieren(char* z, size_t n, const char* q)
{
    size_t i = 0;
    if (q) {
        for (; i + 1 < n && q[i]; i++) {
            z[i] = q[i];
        }
    }
    z[i] = 0;
}

void __wrap_esp_panic_handler(panic_info_t* info)
{
    const char* n = pcTaskGetName(NULL);
    kopieren(absturz.name, sizeof absturz.name, n ? n : "?");
    kopieren(absturz.grund, sizeof absturz.grund, info && info->reason ? info->reason : "?");
    absturz.merker = MERKER;
    __real_esp_panic_handler(info);
}

/* 1 = es gab einen Absturz (Name, Grund kopiert, Merker gelöscht), sonst 0. */
int esp32os_absturz_holen(char* name, int nl, char* grund, int gl)
{
    if (absturz.merker != MERKER) {
        return 0;
    }
    kopieren(name, (size_t)nl, absturz.name);
    kopieren(grund, (size_t)gl, absturz.grund);
    absturz.merker = 0;
    return 1;
}
