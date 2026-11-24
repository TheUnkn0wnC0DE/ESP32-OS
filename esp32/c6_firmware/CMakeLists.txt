# ESP32-OS, C2 (W2): eigene Firmware für den WLAN-Chip ESP32-C6 – esp_hosted
# 3.1.0 (Koprozessor), versionsgleich mit dem Host auf dem P4, Transport
# SDIO. Bauen: werkzeuge/c6_bauen.ps1 (Ziel esp32c6, Bauordner ausserhalb
# des Repos).
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(esp32os_c6)
