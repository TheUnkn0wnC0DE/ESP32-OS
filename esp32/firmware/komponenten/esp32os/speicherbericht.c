/* ESP32-OS, M1 (Messen statt raten): Bericht, wer wie viel Speicher hält –
 * Tasks mit Lage des Stapels (intern/PSRAM) und Restplatz, Heap je Task
 * (CONFIG_HEAP_TASK_TRACKING), Heap-Bereiche des internen RAM. Nur im
 * Mess-Build vollständig (werkzeuge/board/m1_lauf.ps1 schaltet die beiden
 * Optionen vorübergehend ein – Task-Verfolgung ist im normalen Betrieb zu
 * riskant: sie stürzt ab, wenn der Planer angehalten ist). */
#include <stdio.h>
#include <stdlib.h>
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_memory_utils.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "sdkconfig.h"
#if CONFIG_HEAP_TASK_TRACKING
#include "esp_heap_task_info.h"
#endif

static const char *TAG = "speicher";

void esp32os_speicherbericht(void)
{
    ESP_LOGW(TAG, "BERICHT intern frei %u KB (Block %u KB, Min %u KB), PSRAM frei %u KB (Block %u KB, Min %u KB)",
             (unsigned)(heap_caps_get_free_size(MALLOC_CAP_INTERNAL) / 1024),
             (unsigned)(heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL) / 1024),
             (unsigned)(heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL) / 1024),
             (unsigned)(heap_caps_get_free_size(MALLOC_CAP_SPIRAM) / 1024),
             (unsigned)(heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM) / 1024),
             (unsigned)(heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM) / 1024));
#if CONFIG_FREERTOS_USE_TRACE_FACILITY
    UBaseType_t n = uxTaskGetNumberOfTasks() + 4;
    TaskStatus_t *s = heap_caps_malloc(n * sizeof(TaskStatus_t), MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (s != NULL) {
        n = uxTaskGetSystemState(s, n, NULL);
        unsigned intern = 0;
        for (UBaseType_t i = 0; i < n; i++) {
            void *basis = (void *)s[i].pxStackBase;
            bool ist_intern = esp_ptr_internal(basis);
            ESP_LOGW(TAG, "TASK %-16s Prio %2u Stapel %s bei %p, Rest %u Byte", s[i].pcTaskName,
                     (unsigned)s[i].uxCurrentPriority, ist_intern ? "INTERN" : "PSRAM ", basis,
                     (unsigned)s[i].usStackHighWaterMark * sizeof(StackType_t));
            intern += ist_intern;
        }
        ESP_LOGW(TAG, "TASKS: %u, davon %u mit internem Stapel", (unsigned)n, intern);
        heap_caps_free(s);
    }
#else
    ESP_LOGW(TAG, "Task-Liste nur im Mess-Build (CONFIG_FREERTOS_USE_TRACE_FACILITY)");
#endif
#if CONFIG_HEAP_TASK_TRACKING
    /* Heap je Task, getrennt nach internem RAM und PSRAM (Bytes, Blöcke);
     * Task 0 = vor dem Planer bzw. ohne Task */
    static heap_task_totals_t summen[48];
    size_t anzahl = 0;
    heap_task_info_params_t q = { 0 };
    q.caps[0] = MALLOC_CAP_INTERNAL;
    q.mask[0] = MALLOC_CAP_INTERNAL;
    q.caps[1] = MALLOC_CAP_SPIRAM;
    q.mask[1] = MALLOC_CAP_SPIRAM;
    q.totals = summen;
    q.num_totals = &anzahl;
    q.max_totals = sizeof summen / sizeof summen[0];
    heap_caps_get_per_task_info(&q);
    size_t intern_summe = 0;
    /* Namen nur für lebende Tasks: die Liste enthält auch Handles beendeter
     * Tasks – pcTaskGetName darauf läse freigegebenen Speicher (Messlauf 2) */
#if CONFIG_FREERTOS_USE_TRACE_FACILITY
    UBaseType_t nl = uxTaskGetNumberOfTasks() + 4;
    TaskStatus_t *leben = heap_caps_malloc(nl * sizeof(TaskStatus_t), MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    nl = leben ? uxTaskGetSystemState(leben, nl, NULL) : 0;
#endif
    for (size_t i = 0; i < anzahl; i++) {
        const char *name = summen[i].task ? "(beendet)" : "(vor Planer)";
#if CONFIG_FREERTOS_USE_TRACE_FACILITY
        for (UBaseType_t k = 0; k < nl && summen[i].task; k++) {
            if (leben[k].xHandle == summen[i].task) {
                name = leben[k].pcTaskName;
            }
        }
#endif
        ESP_LOGW(TAG, "HEAPTASK %-16s intern %7u B (%3u Blöcke)  PSRAM %8u B (%4u Blöcke)", name,
                 (unsigned)summen[i].size[0], (unsigned)summen[i].count[0], (unsigned)summen[i].size[1],
                 (unsigned)summen[i].count[1]);
        intern_summe += summen[i].size[0];
    }
    ESP_LOGW(TAG, "HEAPTASK Summe intern %u B in %u Tasks", (unsigned)intern_summe, (unsigned)anzahl);
    /* M6: die grössten Blöcke des Tasks „main“ (Bildspeicher, Caches …) */
    TaskHandle_t haupt = xTaskGetHandle("main");
    if (haupt != NULL) {
        static heap_task_block_t bloecke[700];
        static heap_task_totals_t summe_main[2];
        size_t n_main = 0;
        heap_task_info_params_t b = { 0 };
        b.caps[0] = MALLOC_CAP_INTERNAL;
        b.mask[0] = MALLOC_CAP_INTERNAL;
        b.caps[1] = MALLOC_CAP_SPIRAM;
        b.mask[1] = MALLOC_CAP_SPIRAM;
        b.tasks = &haupt;
        b.num_tasks = 1;
        b.totals = summe_main;
        b.num_totals = &n_main;
        b.max_totals = 2;
        b.blocks = bloecke;
        b.max_blocks = sizeof bloecke / sizeof bloecke[0];
        size_t nb = heap_caps_get_per_task_info(&b);
        /* die 25 grössten ausgeben (einfache Auswahl, nb ist klein) */
        for (int k = 0; k < 25; k++) {
            size_t beste = nb;
            for (size_t i = 0; i < nb; i++) {
                if (bloecke[i].size > 0 && (beste == nb || bloecke[i].size > bloecke[beste].size)) {
                    beste = i;
                }
            }
            if (beste == nb) {
                break;
            }
            ESP_LOGW(TAG, "MAINBLOCK %8u B bei %p (%s)", (unsigned)bloecke[beste].size, bloecke[beste].address,
                     esp_ptr_internal(bloecke[beste].address) ? "intern" : "PSRAM");
            bloecke[beste].size = 0;
        }
    }
#if CONFIG_FREERTOS_USE_TRACE_FACILITY
    heap_caps_free(leben);
#endif
#else
    ESP_LOGW(TAG, "Heap je Task nur im Mess-Build (CONFIG_HEAP_TASK_TRACKING)");
#endif
    heap_caps_print_heap_info(MALLOC_CAP_INTERNAL);
}
