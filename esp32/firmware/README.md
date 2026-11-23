# firmware

ESP-IDF firmware for the Waveshare ESP32-P4-WIFI6-Touch-LCD-5: display (MIPI-DSI, PPA scaling), touch, audio, Wi-Fi through the ESP32-C6 (`esp_hosted`), microSD, flash file systems, OTA slots and the hardware JPEG decoder. It hosts the ESP32-OS kernel (`../kern`) and desktop (`../desktop`); Doom and Quake are built as separate images (`doomapp`, `quakeapp`). This is its own Cargo workspace with a pinned toolchain (`rust-toolchain.toml`).

Build and flash (Windows, PowerShell):

```powershell
powershell -File esp32\firmware\bauen_flashen.ps1 -NurBauen              # build all images
powershell -File esp32\firmware\bauen_flashen.ps1 -Nur esp32os -Port COM3 # build and flash the OS only
powershell -File esp32\firmware\release_paket.ps1 -Version 1.0           # release package into release\
```

`-Release` refuses Wi-Fi test credentials and development switches and checks the finished images. Flash layout: `partitions.csv`. Serial tools: `seriell_log.py` (log), `bildschirmfoto.py` (framebuffer screenshots); further helpers are in `werkzeuge/`.
