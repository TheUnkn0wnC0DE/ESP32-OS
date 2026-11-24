/* ESP32-OS: Absturzmerker (D12). Der Panic-Handler von ESP-IDF wird per
 * Linker (--wrap=esp_panic_handler) umhüllt: vor dem Neustart Name des
 * abstürzenden Tasks (= Programmname) und Grund im RTC-Speicher ablegen,
 * der einen Software-Neustart übersteht. Nach dem Neustart zeigt der
 * Desktop die Meldung. */
#include <string.h>
#include "esp_attr.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include <stdbool.h>
#include <stdint.h>
#include "esp_private/panic_internal.h"
#include "esp_private/cache_utils.h"
#include "riscv/rvruntime-frames.h"

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

/* D5 (Absturzschutz): CPU-Ausnahmen kommen über vectors.S zu
 * panic_from_exception = xt_unhandled_exception (per Linker umhüllt, vor dem
 * Anhalten des zweiten Kerns und dem Umstellen der Wächter). Kommt der
 * Fehler aus einem Programm-Faden (nicht aus Kern-Code, nicht geschützt,
 * kein abort(), nicht aus einer Unterbrechung, Cache an), lenkt der Handler
 * den Faden auf esp32os_programm_fehler_ende um und kehrt zurück
 * (_return_from_exception) – nur das Programm endet. Sonst wie bisher:
 * Panik, Neustart, Meldung. Grenzen (PLAN, Abweichungen): kein Schutz des
 * Speichers – was das Programm vorher überschrieben hat, bleibt kaputt. */
extern void __real_xt_unhandled_exception(void* frame);
bool esp32os_programm_fehler_abfangen(uint32_t ursache, uint32_t pc, uint32_t adresse);
void esp32os_programm_fehler_ende(void);

void IRAM_ATTR __wrap_xt_unhandled_exception(void* frame)
{
    RvExcFrame* f = (RvExcFrame*)frame;
    uint32_t c = f->mcause;
    /* Adress-/Zugriffsfehler und unerlaubter Befehl; nicht ecall/ebreak */
    bool art = c == 0 || c == 1 || c == 2 || c == 4 || c == 5 || c == 6 || c == 7;
    if (art && !g_panic_abort && spi_flash_cache_enabled() && !xPortInterruptedFromISRContext()
        && esp32os_programm_fehler_abfangen(c, f->mepc, f->mtval)) {
        f->mepc = (uint32_t)(uintptr_t)esp32os_programm_fehler_ende;
        return;
    }
    __real_xt_unhandled_exception(frame);
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
