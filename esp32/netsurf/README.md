# netsurf

The web browser: upstream NetSurf 3.11 with its framebuffer frontend, running as the ESP32-OS program `browser`, with QuickJS-ng as JavaScript engine (`c/js_quickjs.c`, `js/vorspann.js`). The upstream sources are not in this repository. `werkzeuge/quellen_holen.py` downloads fixed archives (NetSurf, QuickJS-ng, zlib), checks their SHA-256 and unpacks them to `C:\ns`; `build.rs` compiles them together with the patches in `c/`.

Test: `python esp32\netsurf\werkzeuge\quellen_holen.py` once, then `cargo test --release` in `esp32/netsurf` (add `--target i686-pc-windows-msvc` for the 32-bit program path).
