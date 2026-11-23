# Licenses

ESP32-OS as a whole (and every firmware image built from it) is licensed
under the **GNU General Public License, version 2** – see [`../LICENSE`](../LICENSE)
(also as [`GPL-2.0.txt`](GPL-2.0.txt)). This folder holds the license texts of
the third-party code it contains or links, copied unchanged from the
upstream sources. Which component uses which license, and who holds the
copyright, is listed in [`../THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).

| File | Applies to |
|---|---|
| [`GPL-2.0.txt`](GPL-2.0.txt) | ESP32-OS as a whole; KolibriOS parts (fonts, cursor, skin, icons, sample programs, headers); doomgeneric and quakegeneric (GPL-2.0-or-later, see their own `LICENSE` in `esp32/doom/doomgeneric/` and `esp32/quake/quakegeneric/`) |
| [`GPL-2.0-netsurf-COPYING.txt`](GPL-2.0-netsurf-COPYING.txt) | NetSurf 3.11 browser core and framebuffer front end (GPL-2.0-only, with NetSurf's notes on its MIT-licensed parts) |
| `MIT-netsurf-*.txt` | NetSurf libraries libcss, libdom, libhubbub, libparserutils, libwapcaplet, libnsutils, libnsfb, libnsbmp, libnsgif (MIT, one file per library with its copyright line) |
| [`MIT-utf8proc.md`](MIT-utf8proc.md) | utf8proc as shipped with NetSurf (MIT, plus the Unicode data license inside) |
| [`MIT-quickjs-ng.txt`](MIT-quickjs-ng.txt) | QuickJS-ng 0.18.0 JavaScript engine (MIT, from the header of `quickjs.h`) |
| [`Zlib.txt`](Zlib.txt) | zlib 1.4 (zlib license) |
| [`MIT-nanomp3.txt`](MIT-nanomp3.txt), [`Apache-2.0.txt`](Apache-2.0.txt) | nanomp3 / nanomp3-core 0.2.0, the MP3 decoder of the music player (MIT OR Apache-2.0) |
| [`CC0-1.0.txt`](CC0-1.0.txt) | minimp3, on which nanomp3 is based (CC0-1.0) |
| [`Apache-2.0.txt`](Apache-2.0.txt) | ESP-IDF, esp_hosted, esp_wifi_remote (Apache-2.0; linked into the firmware, not contained in this repository) |

Game data (`DOOM1.WAD`, `pak0.pak`) is never contained or distributed; the
shareware files stay under id Software's shareware license and are loaded
from the user's SD card.
