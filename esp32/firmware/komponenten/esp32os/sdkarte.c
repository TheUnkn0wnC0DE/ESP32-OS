/* ESP32-OS: microSD-Karte (SDMMC-Slot 0, 4 Bit, feste IO-MUX-Pins des P4)
 * als FAT einhängen. Versorgung optional über den internen LDO-Kanal 4. */
#include <string.h>
#include "esp_vfs_fat.h"
#include "sdmmc_cmd.h"
#include "driver/sdmmc_host.h"
#include "sd_pwr_ctrl_by_on_chip_ldo.h"

static sdmmc_card_t* karte;
static sd_pwr_ctrl_handle_t versorgung;

/* Liefert ESP_OK (0) oder einen esp_err_t. */
int esp32os_sd_einbinden(const char* ort, int mit_ldo)
{
    esp_vfs_fat_sdmmc_mount_config_t cfg = {
        .format_if_mount_failed = false,
        .max_files = 8,
        .allocation_unit_size = 16 * 1024,
    };
    sdmmc_host_t host = SDMMC_HOST_DEFAULT();
    host.slot = SDMMC_HOST_SLOT_0;
    host.max_freq_khz = SDMMC_FREQ_HIGHSPEED;
    if (mit_ldo && !versorgung) {
        sd_pwr_ctrl_ldo_config_t ldo = { .ldo_chan_id = 4 };
        esp_err_t e = sd_pwr_ctrl_new_on_chip_ldo(&ldo, &versorgung);
        if (e != ESP_OK) {
            return e;
        }
    }
    host.pwr_ctrl_handle = versorgung;
    sdmmc_slot_config_t slot = SDMMC_SLOT_CONFIG_DEFAULT();
    slot.width = 4;
    return esp_vfs_fat_sdmmc_mount(ort, &host, &slot, &cfg, &karte);
}

/* Kartengrösse in MB (0 = keine Karte). */
unsigned esp32os_sd_mb(void)
{
    if (!karte) {
        return 0;
    }
    return (unsigned)(((unsigned long long)karte->csd.capacity * karte->csd.sector_size) >> 20);
}

/* Speicherplatz eines FAT-Einhängepunkts (SD oder interne Partition) für die
 * Einstellungen (P3): Gesamt und frei in Bytes. 0 = ok. */
int esp32os_fat_info(const char* ort, uint64_t* gesamt, uint64_t* frei)
{
    return esp_vfs_fat_info(ort, gesamt, frei);
}
