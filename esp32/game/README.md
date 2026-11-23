# game (pudding-core)

Pudding-Panik, a 3D jump'n'run written for this console: game logic, a 3D renderer for RGB565 that splits each frame into strips for both CPU cores, HUD, menus, touch controls, save games and a small synthesizer. The crate is `no_std` with `alloc`; the firmware supplies display, touch, time, audio and flash, and `../sim` runs the same code on the PC.

Test (in `esp32/`): `cargo test --release -p pudding-core`.
