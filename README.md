# ESP32-OS

**KolibriOS, running natively on an ESP32-P4 touch console.**

> **Work in progress.** ESP32-OS is an experimental hobby project. Many parts
> work on real hardware, many are still missing (see below). Expect bugs,
> breaking changes and German comments in the code.
>
> This project was **vibecoded with [Claude Code](https://claude.com/claude-code)**:
> the code, tests and documentation were written by an AI assistant under
> human direction and checked on the real board.

ESP32-OS brings the look and the programming interface of
[KolibriOS](https://kolibrios.org) to a microcontroller. There is no x86
emulator: the KolibriOS system functions (windows, drawing, text, buttons,
events, files, threads, memory, sockets, …) are re-implemented natively in
Rust, and original KolibriOS C programs are compiled for the RISC-V CPU and
run unchanged. On top of that sits a touch-friendly desktop with taskbar,
start menu, on-screen keyboard and a set of system programs, a Wi-Fi stack,
the NetSurf web browser and a few games.

![Desktop](docs/nachweise/p4_board_desktop.png)

## Hardware

**Waveshare ESP32-P4-WIFI6-Touch-LCD-5**
([documentation](https://docs.waveshare.com/ESP32-P4-WIFI6-Touch-LCD-5)):

- ESP32-P4: dual-core RISC-V (360 MHz on chip revision v1.x, 400 MHz on v3.x),
  32 MB PSRAM, 32 MB flash, hardware JPEG decoder and 2D accelerator (PPA)
- ESP32-C6 as Wi-Fi 6 co-processor (connected over SDIO, `esp_hosted`)
- 5" IPS 720×1280 (MIPI-DSI), capacitive touch GT911
- ES8311 audio codec with speaker, microSD slot, LiPo connector

The desktop is drawn at 640×360 logical pixels and scaled ×2 to the panel
(landscape, 1280×720), so every control is large enough for a finger.

## What works today

Everything below has been verified on the board (screenshots and logs in
[`docs/nachweise/`](docs/nachweise/)); details per item are in
[`PLAN.md`](PLAN.md) (German).

**Kernel layer**
- Processes with their own PSRAM heap; a crashing or killed program is
  cleaned up without taking the system down (crash dialog, also after a
  CPU exception).
- KolibriOS system functions implemented natively, original skin
  *Shkvorka*, original system fonts.
- Original KolibriOS C programs (e.g. `basic_gui.c`, box_lib, libimg,
  dialogs) compiled natively; system libraries box_lib, libimg, proc_lib and
  libini as native code.
- Compositor with dirty rectangles (55 FPS while dragging a window).
- File system: internal flash (`/sys` = `/rd/1`) and microSD (`/sd0/1`).
- Two firmware slots with automatic rollback (OTA-ready flash layout).

**Desktop and touch**
- Boots straight into the desktop (icons, taskbar with Wi-Fi signal and
  clock, start menu) – about 3.4 s after power-on.
- Tap = click, long press = right click / context menu, swipe = scroll,
  two-finger pinch = zoom, drag the title bar to move, double tap to
  maximise. No hardware buttons needed.
- On-screen keyboard as a system service (Swiss German and US layout),
  copy & paste through the context menu, optional touchpad mode.

**Programs**
- File manager, image viewer (BMP, GIF, PNG, JPEG – baseline JPEG in
  hardware, progressive in software), settings, first-run wizard,
  screenshot tool, task manager (including force-quitting a hung program).
- Notes (text editor), calculator, clock with alarm and timer, calendar,
  MP3 player (files from the microSD card).
- PDF viewer (own PDF reader: scroll, page through, pinch-zoom; text is
  shown in the system font, embedded fonts are not rasterised).
- User interface in English and German.

**Wi-Fi and network**
- Scan and connect by touch, saved networks with automatic reconnect,
  signal strength in the taskbar, time via NTP.
- KolibriOS socket API on top of lwIP (original network samples run).
- Measured throughput 10–25 Mbit/s (TCP/UDP, iperf).

**Browser**
- NetSurf 3.11 (HTML5 parser, CSS 2.1 + parts of CSS 3) with QuickJS-ng as
  JavaScript engine, running as an ESP32-OS program: HTTPS with certificate
  check, address bar with search, tabs, bookmarks, history, downloads,
  zoom, text selection and copy, forms with the on-screen keyboard.
- JavaScript on the board: DOM, events, timers, fetch/XMLHttpRequest,
  cookies, localStorage, layout metrics; pages whose scripts are too heavy
  are reloaded without JavaScript automatically.
- Touch scrolling at about 32 frames per second.

**Games** – started from the desktop, back with the home button:
*Pudding-Panik* (own 3D jump'n'run, 55 FPS), *Würfelwelt* (voxel world,
27.5 FPS), Doom (doomgeneric, 55 FPS) and Quake (quakegeneric, 27.5 FPS).
Game data for Doom and Quake is not included (shareware files from the
microSD card).

**Updates over Wi-Fi** – *Settings → System* checks the latest GitHub
release, downloads it over HTTPS (certificate and SHA-256 check) into the
second firmware slot and rolls back automatically if it does not start.

| | | |
|---|---|---|
| ![First-run wizard](docs/nachweise/p4_board_sprache.png) | ![Image viewer](docs/nachweise/p2_board_jpeg.png) | ![On-screen keyboard](docs/nachweise/d8_board_tastatur.png) |
| ![NetSurf](docs/nachweise/b1/b1_board_2.png) | ![Task manager](docs/nachweise/a6_board_liste.png) | ![Context menu and About](docs/nachweise/d4_board_kontextmenue.png) |
| ![Calculator](docs/nachweise/o4/o4_1_02_rechner.png) | ![Notes with keyboard](docs/nachweise/o4/o4_1_03_notizen_tastatur.png) | ![Start menu](docs/nachweise/o4/o4_1_08_startmenue.png) |

![Pudding-Panik](esp32/docs/esp32_vorschau.gif)

## Planned (v1.1)

- Standby with wake-up by the power button, battery level (needs a voltage
  divider) and measured standby current.
- Update of the Wi-Fi co-processor firmware and iTWT power saving.
- Camera app for the OV5647 module (MIPI-CSI).
- Known gaps of the browser and other details are listed under
  "Abweichungen" in [`PLAN.md`](PLAN.md) (German).

## Known limitations

- **The Wi-Fi password is stored unencrypted** on the device (NVS in
  flash). Flash encryption is not used and no eFuses are burned – anyone
  with physical access to the board can read saved Wi-Fi credentials.
- Firmware built with the `-Wlan` developer switch contains Wi-Fi test
  credentials. Such builds must never be shared.
- All programs share one address space (no MMU protection between
  programs). A faulty C program can corrupt memory of the system; CPU
  exceptions are caught and reported.
- The battery level cannot be measured without an external voltage divider
  (GPIO21, see `STATUS.md`).
- The ESP32-C6 ships with an old `esp_hosted` firmware: Wi-Fi works, but
  iTWT and co-processor updates over SDIO need a one-time update by cable.
- The browser handles simple and medium pages; large single-page apps,
  CSS Grid and Web Components are out of reach with its 10 MB of memory.
  Custom request headers, `pushState` and a few other web features are
  missing (list in `PLAN.md`).
- The image viewer opens images up to about 0.85 megapixels.
- The PDF viewer opens files up to 4 MB; scanned pages using CCITT fax,
  JBIG2 or JPEG 2000 images, encrypted files, shadings and clipping paths
  are not supported (details in `PLAN.md`).
- **Binary releases are not cleared yet:** the firmware combines GPL-2.0
  code (NetSurf, KolibriOS parts) with Apache-2.0 code (ESP-IDF,
  esp_hosted). Whether a firmware image may be distributed in this form has
  to be clarified before the first binary release. This does not affect the
  source code in this repository.
- The build currently only works on **Windows** (short fixed paths such as
  `C:\eo`, `C:\ec`, `C:\ns` and a `subst` drive work around Windows path
  limits).

## Installing

Releases are published on the GitHub release page together with a web
flasher (Chrome or Edge, [`docs/flasher/`](docs/flasher/index.html)) and
the files for flashing by hand with `espflash` – see
[`docs/release/v1.0.md`](docs/release/v1.0.md). **No binary release has
been published yet** (see "Binary releases are not cleared yet" above);
until then, build from source as described below.

## Building

Requirements (Windows, PowerShell): Git (Git for Windows also provides the
Perl used below), Python 3, [rustup](https://rustup.rs), LLVM
(`winget install LLVM.LLVM`) and

```powershell
cargo install ldproxy espflash --locked
```

The Rust toolchain (a pinned nightly with `rust-src`) is selected
automatically by `esp32/firmware/rust-toolchain.toml`. ESP-IDF v5.5.5 is
downloaded on the first build (1–2 GB, takes a while).

1. **Fetch the browser sources** (NetSurf 3.11, QuickJS-ng 0.18.0, zlib 1.4;
   checked with SHA-256, stored in `C:\ns`):

   ```powershell
   python esp32\netsurf\werkzeuge\quellen_holen.py
   ```

2. **Build** the firmware (ESP32-OS, Doom and Quake programs):

   ```powershell
   powershell -File esp32\firmware\bauen_flashen.ps1 -NurBauen
   ```

   `-Nur esp32os` builds only the operating system. Add `-Release` for a
   firmware that is meant to be shared: it refuses Wi-Fi test credentials
   and checks the finished images for them.
   `esp32\firmware\release_paket.ps1 -Version 1.0` builds a complete
   release package (images, checksums, web flasher manifest) into
   `release\` – locally, nothing is published. The default profile is
   for chip revision v1.x; for v3.x boards change
   `esp_idf_sdkconfig_defaults` in `esp32/firmware/Cargo.toml` to
   `sdkconfig.defaults.rev3_x` and run `cargo clean` once.
   The script reaches the repository through a virtual drive `O:` (short
   paths for ESP-IDF). If `O:` already points to a different folder, it
   stops instead of silently building that folder's code – remove the old
   mapping with `subst O: /D`.

3. **Flash:** connect the board's **USB TO UART** port and run

   ```powershell
   powershell -File esp32\firmware\bauen_flashen.ps1 -Port COM3
   ```

   (use your COM port; `-Monitor 60` records 60 s of serial log).

4. **Optional game data:** `python esp32\doom\wad\holen.py` and
   `python esp32\quake\pak\holen.py` download the shareware files for local
   use. Quake reads `ESP32-OS\quake\pak0.pak` from the microSD card; Doom
   reads `DOOM1.WAD` from the internal data partition.

### Tests on the PC

The kernel, the KolibriOS layer, the desktop and the browser have PC tests
with reference images (in `esp32`):

```powershell
cargo test --profile sim -q -p kern
cargo test --profile sim -q -p kolibri --target i686-pc-windows-msvc
cargo test --profile sim -q -p desktop
cargo test --profile sim -q -p netsurf
```

## Repository layout

| Path | Content |
|---|---|
| `esp32/kern` | kernel layer: processes, heaps, windows, KolibriOS system functions |
| `esp32/kolibri` | KolibriOS C programs, native system libraries, image decoders |
| `esp32/desktop` | shell (desktop, taskbar, start menu, keyboard) and system programs |
| `esp32/netsurf` | NetSurf/QuickJS port |
| `esp32/firmware` | ESP-IDF firmware (drivers, Wi-Fi, flash layout, build script) |
| `esp32/game`, `esp32/wuerfelwelt`, `esp32/doom`, `esp32/quake`, `esp32/konsole` | games and the old game console |
| `esp32/gehaeuse` | 3D-printable case (OpenSCAD) |
| `spatz`, `pudding`, `mcu_demo` | the software 3D engine *spatz*, the PC version of Pudding-Panik and a bare-metal Cortex-M7 demo |
| `docs/nachweise` | screenshots and logs taken on the board |

## License

ESP32-OS as a whole – in particular everything under `esp32/` and the
firmware built from it – is licensed under the **GNU General Public License
v2.0** – see [`LICENSE`](LICENSE). It contains code and data from KolibriOS
(GPL-2.0), doomgeneric and quakegeneric (GPL-2.0-or-later) and builds on
NetSurf (GPL-2.0-only), its MIT-licensed libraries, QuickJS-ng (MIT),
zlib and the MP3 decoder nanomp3 (MIT OR Apache-2.0, based on minimp3, CC0).

Exception: the stand-alone crates of the root Cargo workspace, **`spatz`**
(software 3D engine) and **`pudding`** (PC version of the game), are
licensed under **MIT OR Apache-2.0**, as declared in their `Cargo.toml`
(license texts: `spatz/LICENSE-MIT`, `spatz/LICENSE-APACHE`,
`pudding/LICENSE-MIT`, `pudding/LICENSE-APACHE`).
When they are built into the ESP32-OS firmware, the firmware as a whole is
GPL-2.0.

Details: [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md); the license
texts of all third-party components are in [`LICENSES/`](LICENSES/README.md).

Doom and Quake game data are not included.
