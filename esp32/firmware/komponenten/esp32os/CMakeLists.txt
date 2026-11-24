# ESP32-OS: C-Helfer für ESP-IDF-Makros, die in den Rust-Bindungen fehlen.
idf_component_register(SRCS "sdkarte.c" "absturz.c" "jpeg.c" "wlan.c" "update.c" "eingebettet.c" "flash_sperre.c" "flashtest.c" "speicherbericht.c"
                       INCLUDE_DIRS "."
                       REQUIRES fatfs sdmmc esp_driver_sdmmc esp_system freertos esp_driver_jpeg vfs
                                esp_netif esp_event esp_timer esp_wifi_remote esp_hosted
                                esp_http_client esp-tls mbedtls app_update esp_partition spi_flash)
