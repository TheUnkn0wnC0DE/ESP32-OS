# esp32

The ESP32-OS source. This folder is a Cargo workspace for everything that also runs on the PC; the firmware in `firmware/` is a separate workspace for the RISC-V target.

| Crate | Content |
|---|---|
| [`kern`](kern/) | kernel layer: KolibriOS system functions, processes, heaps, compositor |
| [`kolibri`](kolibri/) | KolibriOS C programs and native system libraries |
| [`desktop`](desktop/) | shell, on-screen keyboard and system programs |
| [`netsurf`](netsurf/) | NetSurf browser with QuickJS-ng |
| [`konsole`](konsole/) | full-screen frame for desktop and games |
| [`game`](game/), [`sim`](sim/) | Pudding-Panik and its PC simulation |
| [`wuerfelwelt`](wuerfelwelt/), [`doom`](doom/), [`quake`](quake/) | further games |
| [`firmware`](firmware/) | ESP-IDF firmware, build and flash scripts |
| [`werkzeuge`](werkzeuge/) | test runner and helper scripts |
| [`gehaeuse`](gehaeuse/) | 3D-printable case |

Run all PC tests with `bash esp32/werkzeuge/alle_tests.sh`. Building the firmware is described in [`firmware/`](firmware/) and in the main [README](../README.md#building).
