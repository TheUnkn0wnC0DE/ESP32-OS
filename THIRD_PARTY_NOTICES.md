# Third-party notices

ESP32-OS as a whole is distributed under the **GNU General Public License,
version 2** (see [`LICENSE`](LICENSE)). Because it contains and links code
that is licensed under GPL-2.0 (KolibriOS parts, NetSurf: "version 2 only"),
the combined work and the firmware images are GPL-2.0.

The stand-alone crates of the root Cargo workspace, `spatz` and `pudding`,
are the project's own code and are licensed under **MIT OR Apache-2.0**
(`license` in the root `Cargo.toml`). Where they are built into the
firmware (the ESP32 games use `spatz`), they are part of the GPL-2.0
combined work.

Note for binary releases: the firmware also links ESP-IDF and esp_hosted
(Apache-2.0). Whether a firmware image combining them with GPL-2.0-only code
may be distributed is still to be clarified before the first binary
release.

This file lists the third-party code and data that ESP32-OS contains or uses.
The original copyright notices in the individual files are kept unchanged.

## Contained in this repository

| Component | Where | Copyright | License |
|---|---|---|---|
| **KolibriOS** – system fonts `char.mt`, `charUni.mt`, mouse cursor `arrow.cur`, skin *Shkvorka* (built into `shkvorka.skn` from `skins/Leency/Shkvorka`) | `esp32/kern/daten/` | KolibriOS team (parts: MenuetOS, Ville Mikael Turjanmaa) | GPL-2.0 |
| **KolibriOS** – icon set `icons32.png` | `esp32/desktop/daten/` | KolibriOS team | GPL-2.0 |
| **KolibriOS** – sample programs (`basic_gui.c`, `boxlib.c`, `dialog.c`, `libimg.c`, `http_tcp_demo.c`, the `test_dialog.c` adaptation) and `logo.png` | `esp32/kolibri/c/`, `esp32/kolibri/daten/` | KolibriOS team; boxlib/libimg samples by maxcodehack and turbocat2001 (Logaev Maxim) | GPL-2.0 |
| **KolibriOS** – C headers of the ktcc/libc "clayer" (`clayer/*.h`, `sys/socket.h`) and `sys/ksys.h` (converted from the original by `esp32/kolibri/werkzeuge/ksys_nativ.py`) | `esp32/kolibri/include/` | KolibriOS team; Logaev Maxim (turbocat2001) | GPL-2.0 |
| **doomgeneric** (based on Chocolate Doom and the id Software Doom source) | `esp32/doom/doomgeneric/` | id Software, Inc.; Simon Howard; Ben Ryves (`mus2mid.c`); ozkl (doomgeneric) | GPL-2.0-or-later (`esp32/doom/doomgeneric/LICENSE`) |
| **quakegeneric** (based on the id Software WinQuake source) | `esp32/quake/quakegeneric/` | id Software, Inc.; the quakegeneric authors – upstream presumably https://github.com/erysdren/quakegeneric (not recorded in the imported files) | GPL-2.0-or-later (`esp32/quake/quakegeneric/LICENSE`) |

KolibriOS: https://kolibrios.org, source https://git.kolibrios.org/KolibriOS/kolibrios.
The ESP32-OS kernel (`esp32/kern`) re-implements the KolibriOS system
function interface natively in Rust, following the KolibriOS documentation
(`kernel/trunk/docs/sysfuncs.txt`).

doomgeneric: https://github.com/ozkl/doomgeneric.

## Downloaded at build time (not contained in this repository)

These sources are fetched by scripts with a fixed version and a SHA-256
check and are compiled into the firmware.

| Component | Fetched by | Copyright | License |
|---|---|---|---|
| **NetSurf 3.11** (browser core and framebuffer front end) | `esp32/netsurf/werkzeuge/quellen_holen.py` | The NetSurf developers | GPL-2.0-only (artwork: MIT) |
| NetSurf libraries **libcss, libdom, libhubbub, libparserutils** | same | J-M Bell and the NetSurf developers | MIT |
| **libwapcaplet** | same | The NetSurf Browser Project | MIT |
| **libnsutils, libnsfb** | same | Vincent Sanders, Michael Drake | MIT |
| **libnsbmp, libnsgif** | same | Richard Wilson, Sean Fox, Michael Drake | MIT |
| NetSurf internal bitmap font (`glyph_data`) | same | Tim Tyler, Michael Drake | MIT |
| **utf8proc** (as shipped with NetSurf) | same | Steven G. Johnson, Jiahao Chen, Tony Kelman, Jonas Fonseca, Public Software Group | MIT (plus Unicode data license) |
| **QuickJS-ng 0.18.0** (amalgamation) | same | Fabrice Bellard, Charlie Gordon, the QuickJS-ng contributors | MIT |
| **zlib 1.4** | same | Jean-loup Gailly, Mark Adler | zlib license |
| **ESP-IDF v5.5.5** | esp-idf-sys (Cargo build) | Espressif Systems | Apache-2.0 (components with their own licenses) |
| **esp_hosted 3.1.0**, **esp_wifi_remote 1.6.5** | `esp32/firmware/werkzeuge/komponenten_holen.py`, ESP component manager | Espressif Systems | Apache-2.0 |
| Rust crates (esp-idf-svc, esp-idf-hal, esp-idf-sys, log, …) | Cargo (`Cargo.lock`) | their authors | mostly MIT OR Apache-2.0 – see each crate |

## Game data (never contained, never distributed)

Doom and Quake need the shareware game data. It is **not** part of this
repository or of any firmware image. Helper scripts download it for local
use only:

- `esp32/doom/wad/holen.py` – `DOOM1.WAD` (Doom shareware 1.9, id Software)
- `esp32/quake/pak/holen.py` – `pak0.pak` (Quake shareware 1.06, id Software)

The shareware data is subject to id Software's shareware license terms.
