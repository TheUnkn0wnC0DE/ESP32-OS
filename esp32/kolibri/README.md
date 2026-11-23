# kolibri

KolibriOS C programs and system libraries for ESP32-OS. The C sources in `c/` are copied unchanged from the KolibriOS repository and linked statically; `werkzeuge/praefix.py` generates the wrappers in `c/gen/` that give each program its own symbol prefix. `include/sys/ksys.h` is the native version of the KolibriOS system call header. The libraries box_lib, libimg, proc_lib (dialogs), libini and network are native Rust, as are the image decoders (BMP, GIF, PNG, JPEG) and the PNG/zlib encoder.

Test (in `esp32/`): `cargo test --release -p kolibri --target i686-pc-windows-msvc`. The C programs are only built for 32-bit targets, as on the board.
