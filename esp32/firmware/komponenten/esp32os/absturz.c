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
#include "riscv/rv_utils.h"
#include "soc/soc_caps.h"
#include "esp_cpu.h"

#define MERKER 0x4F534352u /* "OSCR" */

/* M1: Speicherstand (KB) und Laufzeit (s) – laufend von der Speicherwacht
 * (speicherwacht.rs, jede Sekunde), beim Absturz in den Merker kopiert */
typedef struct {
    uint32_t intern, intern_block, psram, psram_block, sekunden;
} speicherstand_t;

typedef struct {
    uint32_t merker;
    char name[24];
    char grund[64];
    uint32_t stand_gueltig;
    speicherstand_t stand;
} absturz_t;

RTC_NOINIT_ATTR static absturz_t absturz;
/* nicht im RTC: nach jedem Start neu (Wert vom letzten Lauf zählt nicht) */
static volatile speicherstand_t stand;
static volatile uint32_t stand_da;

void esp32os_speicher_merken(uint32_t intern, uint32_t intern_block, uint32_t psram, uint32_t psram_block, uint32_t sekunden)
{
    stand.intern = intern;
    stand.intern_block = intern_block;
    stand.psram = psram;
    stand.psram_block = psram_block;
    stand.sekunden = sekunden;
    stand_da = 1;
}

/* Speicherstand beim letzten Absturz (vor esp32os_absturz_holen lesen).
 * 1 = vorhanden. */
int esp32os_absturz_speicher(uint32_t* aus)
{
    if (absturz.merker != MERKER || absturz.stand_gueltig != MERKER) {
        return 0;
    }
    aus[0] = absturz.stand.intern;
    aus[1] = absturz.stand.intern_block;
    aus[2] = absturz.stand.psram;
    aus[3] = absturz.stand.psram_block;
    aus[4] = absturz.stand.sekunden;
    return 1;
}

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
    if (stand_da) {
        absturz.stand.intern = stand.intern;
        absturz.stand.intern_block = stand.intern_block;
        absturz.stand.psram = stand.psram;
        absturz.stand.psram_block = stand.psram_block;
        absturz.stand.sekunden = stand.sekunden;
        absturz.stand_gueltig = MERKER;
    } else {
        absturz.stand_gueltig = 0;
    }
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

/* Fehler im Handler selbst (z. B. kaputte Faden-Daten): nicht noch einmal
 * abfangen, sondern Panik wie bisher (Review 24.11.) */
static volatile bool im_handler[SOC_CPU_CORES_NUM];

/* Waren Unterbrechungen beim Fehler frei? (MPIE, Schwelle auf Grundwert.)
 * Sonst lief das Programm in einem kritischen Abschnitt oder unter einer
 * Sperre – das Ende würde hängen bleiben (Review 24.11.). */
static bool IRAM_ATTR unterbrechungen_frei(const RvExcFrame* f)
{
    if ((f->mstatus & (1u << 7)) == 0) { /* MPIE */
        return false;
    }
#if SOC_INT_CLIC_SUPPORTED
    if (rv_utils_get_interrupt_threshold() != RVHAL_INTR_ENABLE_THRESH_CLIC) {
        return false;
    }
#endif
    return true;
}

void IRAM_ATTR __wrap_xt_unhandled_exception(void* frame)
{
    RvExcFrame* f = (RvExcFrame*)frame;
    uint32_t c = f->mcause;
    int kern = esp_cpu_get_core_id();
    /* Adress-/Zugriffsfehler und unerlaubter Befehl; nicht ecall/ebreak */
    bool art = c == 0 || c == 1 || c == 2 || c == 4 || c == 5 || c == 6 || c == 7;
    if (art && !im_handler[kern] && !g_panic_abort && spi_flash_cache_enabled() && !xPortInterruptedFromISRContext()
        && unterbrechungen_frei(f)) {
        im_handler[kern] = true;
        bool abgefangen = esp32os_programm_fehler_abfangen(c, f->mepc, f->mtval);
        im_handler[kern] = false;
        if (abgefangen) {
            f->mepc = (uint32_t)(uintptr_t)esp32os_programm_fehler_ende;
            return;
        }
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
