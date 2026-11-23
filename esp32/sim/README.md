# sim (pudding-sim)

Headless PC simulation of the console games. It runs the same game code as the firmware and replaces only display, speaker, input and flash, so screens, GIFs, audio and timings can be produced without hardware.

```powershell
cargo run --profile sim -p pudding-sim -- shots out      # PNG of every screen
cargo run --profile sim -p pudding-sim -- gif out.gif 10 # 10 s of gameplay at 640x360
cargo run --profile sim -p pudding-sim -- wav out        # music and sound effects
```
