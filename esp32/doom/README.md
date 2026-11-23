# doom

Doom for the console, based on doomgeneric (Chocolate Doom, GPL-2.0; sources in `doomgeneric/`), with touch controls, sound through the console mixer and a 4:3 picture in the middle of the screen. The shareware data is not part of the repository: `python esp32/doom/wad/holen.py` downloads `DOOM1.WAD` (version 1.9, checksum checked). On the board the game reads it from `ESP32-OS/doom/` on the microSD card and copies it once into the internal data partition.

Test (in `esp32/`): `cargo test --release -p doom` (the play test is skipped without the WAD).
