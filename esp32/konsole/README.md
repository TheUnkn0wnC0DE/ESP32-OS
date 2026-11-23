# konsole

The common frame for all full-screen programs on the board: the `Spiel` trait (update, draw, touch), the home button, the FPS overlay and the switch between games. The ESP32-OS desktop runs inside this frame as well; Pudding-Panik, Würfelwelt, Doom and Quake are started from it.

Test (in `esp32/`): `cargo test --release -p konsole`.
