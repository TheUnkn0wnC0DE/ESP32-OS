# werkzeuge (tools)

- `alle_tests.sh`: runs every PC test suite (64-bit and 32-bit) and prints one result line per suite. Run `bash esp32/werkzeuge/alle_tests.sh` before each commit.
- `spieldaten_pruefen.py`: checks that firmware images contain no Doom or Quake game data (part of the release build).
- `test_mp3.py`: generates small MP3 test tones without an encoder, used by the music player tests.
