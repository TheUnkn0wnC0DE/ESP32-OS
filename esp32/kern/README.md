# kern

The kernel layer of ESP32-OS. It implements the KolibriOS system functions natively in Rust (windows, drawing, text, buttons, events, files, threads, memory, clipboard, sockets) and runs each program as a thread with its own heap. It also contains the compositor with dirty rectangles, the skin and font renderer and the file system mounts. The crate is platform-independent: the firmware provides the board platform, the PC tests use `kern::PC`.

Test (in `esp32/`): `cargo test --release -p kern`, and with `--target i686-pc-windows-msvc` for the 32-bit system call path.
