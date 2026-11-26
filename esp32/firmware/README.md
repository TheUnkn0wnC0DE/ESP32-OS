# firmware

ESP-IDF firmware for the Guition JC4880P443C_I_W: display (MIPI-DSI, PPA scaling), touch, audio, Wi-Fi through the ESP32-C6 (`esp_hosted`), microSD, flash file systems, OTA slots and the hardware JPEG decoder. It hosts the ESP32-OS kernel (`../kern`) and desktop (`../desktop`); Doom and Quake are built as separate images (`doomapp`, `quakeapp`). This is its own Cargo workspace with a pinned toolchain (`rust-toolchain.toml`).

## Guition JC4880P443C_I_W port

Differences from the original Waveshare ESP32-P4-WIFI6-Touch-LCD-5 build:

* **Display:** ST7701, 480×800 (`komponenten/esp_lcd_st7701`). The 640×360 desktop is scaled ×1.25 to 800×450, with 15-line black bars.
* **Console:** the P4's USB Serial/JTAG port (log, `bildschirmfoto.py`, flashing).
* **Flash:** 16 MB. One OS slot; `daten` is about 5.9 MB.
* **No Wi-Fi updates:** upstream releases are built for the Waveshare board. Setting `ESP32OS_UPDATE_REPO` at build time enables updates from your own repository, which also needs a second OTA slot.

Build and flash (macOS or Linux; needs rustup, `cargo install ldproxy espflash --locked`, LLVM, perl and Python 3 with Pillow):

```sh
./bauen_flashen.sh                                # build all images
./bauen_flashen.sh --port /dev/cu.usbmodem101     # build and flash
```

Build and flash (Windows, PowerShell):

```powershell
powershell -File esp32\firmware\bauen_flashen.ps1 -NurBauen              # build all images
powershell -File esp32\firmware\bauen_flashen.ps1 -Nur esp32os -Port COM3 # build and flash the OS only
powershell -File esp32\firmware\release_paket.ps1 -Version 1.0           # release package into release\
```

`-Release` refuses Wi-Fi test credentials and development switches and checks the finished images. Flash layout: `partitions.csv`. Serial tools: `seriell_log.py` (log), `bildschirmfoto.py` (framebuffer screenshots); further helpers are in `werkzeuge/`.
