# Changelog

## 1.0 (2026-11-23, not yet released)

First complete version. Acceptance tests and measurements are documented in `docs/development/PLAN.md` and `docs/nachweise/`.

- Kernel layer with the KolibriOS system functions in Rust, one heap per program, crash isolation, compositor with dirty rectangles, two firmware slots with rollback.
- Original KolibriOS C programs, unchanged: samples, PasswordGen, TinyHashView.
- Touch desktop: icons, taskbar, start menu, gestures, on-screen keyboard (Swiss German, US), English and German interface, first-run wizard.
- Programs: file manager, image viewer, PDF viewer, settings, notes, calculator, clock with alarm and timer, calendar, MP3 player, task manager, screenshot tool.
- Wi-Fi 6 through the ESP32-C6: scan, connect, remembered networks, NTP time.
- Web browser: NetSurf 3.11 with HTTPS, tabs, bookmarks, history, downloads, zoom, forms and JavaScript (QuickJS-ng).
- Games started from the desktop: Pudding-Panik, Würfelwelt, Doom, Quake.
- Firmware update over Wi-Fi from GitHub releases with checksum and rollback.
- Web flasher page and release build script with checks for credentials, development switches and game data.
