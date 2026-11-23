# Changelog

## 1.0.2 (not yet released)

- Quake: "New Game" no longer aborts with "progs.dat has wrong version number". The firmware treated the end of the PAK directory as the end of the file; in the shareware `pak0.pak` the game code lies behind the directory and was read as zeros.
- On-screen keyboard: a key is drawn smaller and darker while a finger rests on it, for every key including Shift, Backspace, Space and Enter. Only that key is redrawn.
- Defining a button (function 8) marks only the button as changed instead of the whole window.
- Browser: a page no longer takes the whole system down when internal RAM runs out. A new connection opens only with 32 KB of internal RAM left, otherwise the resource fails with "not enough memory". Closed TCP connections stay 10 s instead of 120 s in TIME_WAIT.

## 1.0.1 (2026-11-23)

- Settings opens again after power-on: it reserves 6 MB instead of 12 MB in one block (in 1.0 the largest free block after power-on was exactly 12 MB, too small for the program). Wallpapers from large photos are scaled down while loading.
- The serial log shows the largest free PSRAM block.

## 1.0 (2026-11-23)

First complete version. Acceptance tests and measurements are documented in `docs/development/PLAN.md` and `docs/nachweise/`.

- Kernel layer with the KolibriOS system functions in Rust, one heap per program, crash isolation, compositor with dirty rectangles, two firmware slots with rollback.
- Original KolibriOS C programs, unchanged: samples, PasswordGen, TinyHashView.
- Touch desktop: icons, taskbar, start menu, gestures, on-screen keyboard (Swiss German, US), English and German interface, first-run wizard.
- Programs: file manager, image viewer (large photos scaled down while decoding), PDF viewer, settings, notes, calculator, clock with alarm and timer, calendar, MP3 player, task manager, screenshot tool.
- Wi-Fi 6 through the ESP32-C6: scan, connect, remembered networks, NTP time.
- Web browser: NetSurf 3.11 with HTTPS, tabs, bookmarks, history, downloads, zoom, forms and JavaScript (QuickJS-ng).
- Games started from the desktop: Pudding-Panik, Würfelwelt, Doom, Quake.
- Firmware update over Wi-Fi from GitHub releases with checksum and rollback.
- Web flasher page and release build script with checks for credentials, development switches and game data.
