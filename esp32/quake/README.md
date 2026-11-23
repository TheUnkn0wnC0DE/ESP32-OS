# quake

Quake for the console, based on quakegeneric (WinQuake, GPL-2.0; sources in `quakegeneric/`), with touch controls and sound through the WinQuake mixer. The shareware data is not part of the repository: `python esp32/quake/pak/holen.py` downloads `pak0.pak` (version 1.06, checksums checked, needs 7-Zip). On the board the game reads it from `ESP32-OS/quake/` on the microSD card.

Test (in `esp32/`): `cargo test --release -p quake` (the autopilot test is skipped without the PAK).
