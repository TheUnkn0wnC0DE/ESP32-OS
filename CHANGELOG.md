# Changelog

## 1.0.5 (2026-11-28)

- Music player: also plays Ogg Vorbis files (.ogg, .oga) with stb_vorbis 1.25 (MIT / public domain). Broken or cut-off files end with a message or at the last readable part, never with a crash.
- New App store: Doom and Quake can be removed and installed again over Wi-Fi from the latest GitHub release (HTTPS, SHA-256 from SHA256SUMS). Pudding Panic, Cube World, PasswordGen, TinyHashView and the example programs can be removed from and put back into the Start menu and desktop; they stay part of the firmware. The core system cannot be removed. Removing Doom or Quake asks for a second tap.
- App store: a download that fails or does not match its checksum leaves the game marked as not installed (never a half-written image). A Wi-Fi update and the App store never write to flash at the same time.
- Browser: the JavaScript button is translated ("No JS" / "With JS" in English).
- Start menu: the App store is under *More programs...*.
- Browser: no more half-minute freezes at the end of very large pages. The browser's memory manager now finds free blocks in a tree sorted by address (same placement as before, so no extra waste) instead of walking a long list. Measured on 30 test pages: no page ends early, no freeze, no restart.
- Stability: with system memory almost full, the system no longer stops; programs and windows are refused instead (40-minute test with only 600 KB left: 0 crashes).

## 1.0.4 (2026-11-27)

- Weather: new app with current weather and a 5-day forecast from Open-Meteo (no API key). Choose the place by touch in the app or in *Settings → Weather place*; without Wi-Fi it says so. Icon on the desktop and in the Start menu.
- On-screen keyboard: typing in Notes redraws only the text area, about 31 to 33 frames per second instead of 24 to 30.
- A program that cannot start (for example not enough memory) now shows a message with the reason instead of doing nothing.
- Music player: also plays WAV files (8 and 16 bit PCM, mono and stereo) and MIDI files (.mid, format 0 and 1, with a small built-in synthesizer); an empty folder says "No music files (MP3, WAV, MIDI)" and names the folder on a second line.
- Console: no more "File '/sys/settings/.shell' not found" at every start.
- Taskbar: long titles end with "..". File manager: in the recycle bin only "Recycle bin" is highlighted.
- Texts no longer contain characters the system font cannot show (boxes instead of "..." and "-").
- Stability: a program that is force-closed while it holds its own memory lock is now really ended; before, its memory stayed reserved and large programs (browser) could no longer start.
- Memory: opening the Start menu no longer leaves about 108 KB of PSRAM behind each time (about 1 MB per hour of heavy use, also in 1.0.3), and starting a program no longer leaves a few bytes of internal RAM behind (about 7–9 KB per 75 minutes of heavy use, also in 1.0.3). Programs no longer start when system memory is almost full, and an almost full system memory no longer restarts the board. Measured: 75-minute endurance run with all programs, the Start menu, the browser and sleep mode: 0 crashes, internal RAM and PSRAM stable.
- Smoother scrolling: on the desktop the processor enlarges and rotates the picture for the display itself instead of the 2D unit (about 40 MB/s). Swiping in the file manager 25 → 47 frames per second, in the browser 25 → 31.
- Memory: the desktop background picture needs 450 KB less; the browser gets 10.5 instead of 10 MB.
- Windows of original KolibriOS programs no longer open partly behind the taskbar (PasswordGen: the "Generate!" and "Copy" buttons were hidden).
- Image demo: no longer crashes when its picture in internal flash is damaged; the file is checked at start-up and written again.
- Browser: CSS variables (`var()`), `calc()` with one unit, the space syntax of `rgb()`/`hsl()` and logical properties such as `margin-inline-start` now work; `<script type="module">` and import maps are skipped like in a browser without modules; new `document.currentScript`, `ResizeObserver`, `IntersectionObserver`, `new Image()`, `performance.timing`. JavaScript may use up to 6 MB as long as 3 MB stay free for the page. Measured on 30 well-known pages: JavaScript errors 61 → 17, CSS declarations dropped because of `var()` 8677 → about 700; 22 pages load, 8 are cut off at 512 KB of HTML, none ends early.
## 1.0.3 (2026-11-25)

- Browser: tapping the Start button while the browser is open no longer crashes the system. A page that cannot load shows a readable reason (name not found, no connection, timeout) instead of an error code; DNS falls back to 1.1.1.1 when the router's server does not answer.
- Firmware update: connection errors are shown as readable text instead of codes like -28674.
- Stability: a program that uses up its own memory ends with "memory full" instead of taking the system down; windows and buffers are only created when the system keeps a reserve; new network connections need 48 KB of internal RAM.
- Internal RAM under load: about 206 KB free instead of 86 KB (display strips of the desktop and variables of the C programs moved to PSRAM).
- Large programs (browser, PDF viewer, image viewer) start reliably after long use: their memory comes from a pool reserved at start-up, so PSRAM no longer breaks into small pieces.
- A hardware fault in a program (for example a load or store access fault) ends only that program. On the board this protection never triggered before; it now uses the FreeRTOS critical-section counter instead of the interrupt threshold.
- Browser: starting the browser a second time no longer crashes the board (a static lock lived in the heap of the first browser).
- Windows of original KolibriOS programs (PasswordGen, TinyHashView, example GUI) no longer draw into the taller touch title bar; title bars and borders of all windows are complete.
- After a crash and restart, the message names the reason and the memory state at the time.
- File manager: recycle bin with restore; scrolling redraws only the list.
- Taskbar: a window's entry appears together with the window.
- Start button: tapping it again closes the open menu.
- First-run wizard: closing it with the X counts as done; it no longer appears at every start.
- Console: the KolibriOS shell runs in a native console window, and a new Matrix program shows falling characters.
- Shell: output redirection (>, >>), input from a file (<) and pipes (|) for built-in commands; new commands cat, grep, wc, head, tail and sort. Fixed two bugs of the original shell (ls -1, script detection).
- Sleep mode from the Start menu: the screen turns off, a tap wakes the board.

## 1.0.2 (2026-11-24)

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
