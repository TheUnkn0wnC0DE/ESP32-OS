/* ESP32-OS, C2 (W2): Firmware des WLAN-Chips ESP32-C6. esp_hosted startet
 * sich selbst (Koprozessor-Kern, eh_cp_auto_init_task); hier nur ein
 * Lebenszeichen im Log des C6. */
#include "esp_log.h"

void app_main(void)
{
    ESP_LOGI("esp32os_c6", "ESP32-OS C6-Firmware (esp_hosted 3.1.0, SDIO)");
}
