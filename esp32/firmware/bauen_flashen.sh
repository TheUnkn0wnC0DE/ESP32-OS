#!/usr/bin/env bash
# Build and flash ESP32-OS for the Guition JC4880P443C_I_W on macOS/Linux.
# (Windows: bauen_flashen.ps1.)
#
#   ./bauen_flashen.sh                          # build all three programs
#   ./bauen_flashen.sh --port /dev/cu.usbmodem101   # build, then flash
#
# Needs: rustup, `cargo install ldproxy espflash --locked`, LLVM (libclang),
# Python 3 with Pillow, perl.
set -euo pipefail

port=""
while [ $# -gt 0 ]; do
    case "$1" in
        --port) port="${2:?--port needs a device}"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

cd "$(dirname "$0")"
ns_root="$HOME/.cache/esp32os/ns"
export ESP32OS_EC="${ESP32OS_EC:-$HOME/.cache/esp32os/ec}"
export NETSURF_QUELLEN="$ns_root/3.11"
if [ -z "${LIBCLANG_PATH:-}" ] && command -v brew >/dev/null; then
    export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
fi
# esp-idf-sys brings its own ESP-IDF; an activated one would override it.
unset IDF_PATH

python3 werkzeuge/komponenten_holen.py
python3 ../netsurf/werkzeuge/quellen_holen.py
# ESP-IDF compiles NetSurf from a file list that the netsurf crate writes.
(cd .. && NETSURF_NUR_LISTE=1 cargo build -p netsurf)

mkdir -p target/abbilder
for program in esp32os:"" doomapp:doom quakeapp:quake; do
    name="${program%%:*}" feature="${program#*:}"
    cargo build --release --bin pudding-p4 ${feature:+--features "$feature"}
    cp target/riscv32imafc-esp-espidf/release/pudding-p4 "target/abbilder/$name.elf"
    espflash save-image --chip esp32p4 --flash-size 16mb "target/abbilder/$name.elf" "target/abbilder/$name.bin"
done

[ -n "$port" ] || exit 0
# Bootloader built by ESP-IDF (with rollback support)
boot="$(ls -t target/riscv32imafc-esp-espidf/release/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin | head -1)"
# Erasing otadata makes the board start in esp32os_a.
espflash flash --port "$port" --baud 921600 --flash-size 16mb --bootloader "$boot" \
    --partition-table partitions.csv --target-app-partition esp32os_a --erase-parts otadata \
    --non-interactive target/abbilder/esp32os.elf
espflash write-bin --port "$port" --baud 921600 --non-interactive 0x610000 target/abbilder/doomapp.bin
espflash write-bin --port "$port" --baud 921600 --non-interactive 0x810000 target/abbilder/quakeapp.bin
