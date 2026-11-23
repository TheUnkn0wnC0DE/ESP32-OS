# desktop

The shell and the system programs of ESP32-OS: desktop icons, taskbar, start menu, on-screen keyboard, touch gestures, and the programs File manager, Image viewer, PDF viewer, Settings, Notes, Calculator, Clock, Calendar, Music, Task manager, Screenshot and the `@notify` notification service. The PDF reader is in `src/pdf.rs`.

Test (in `esp32/`): `cargo test --release -p desktop`. The tests drive the programs with touch input and write screenshots to `tests/erwartet/`; image checksums (`.fnv`) are renewed with `ERWARTET_NEU=1`. Tests that need the KolibriOS C programs run with `--target i686-pc-windows-msvc --test c_programme --test thashview --test menue_start`.
