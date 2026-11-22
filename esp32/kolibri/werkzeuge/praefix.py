"""Erzeugt für jedes KolibriOS-C-Programm eine Hülle `c/gen/<name>.c`, die
alle globalen Symbole des Programms per #define mit `<name>_` versieht und
dann die unveränderte Originalquelle einbindet. So passen mehrere Programme
(je mit eigenem main, draw_window, …) in ein Binary.

    python praefix.py <name>=<quelle.c> …

Die Symbole liefert riscv32-esp-elf-gcc/-nm (gleiche Quelle, gleiche Namen
auf allen Zielen). `main` wird zu `<name>_main`.
"""
import glob
import os
import subprocess
import sys
import tempfile
from pathlib import Path

hier = Path(__file__).resolve().parent.parent
_bin = Path(glob.glob(os.path.expanduser("~/.espressif/tools/riscv32-esp-elf/*/riscv32-esp-elf/bin"))[0])
_exe = ".exe" if os.name == "nt" else ""
gcc = str(_bin / f"riscv32-esp-elf-gcc{_exe}")
nm = str(_bin / f"riscv32-esp-elf-nm{_exe}")
# Symbole der Laufzeit, die nie umbenannt werden dürfen
FREMD = {"kos_syscall", "debug_printf"}

for arg in sys.argv[1:]:
    name, quelle = arg.split("=", 1)
    q = (hier / quelle).resolve()
    with tempfile.TemporaryDirectory() as t:
        o = Path(t) / "x.o"
        subprocess.run([gcc, "-march=rv32imafc_zicsr_zifencei", "-mabi=ilp32f", "-c", "-w", "-Wno-int-conversion",
                        "-Wno-incompatible-pointer-types", "-I", str(hier / "include"), "-include", "kolibri_libc.h",
                        str(q), "-o", str(o)], check=True)
        aus = subprocess.run([nm, "--defined-only", "-g", str(o)], check=True, capture_output=True, text=True).stdout
    syms = sorted({z.split()[-1] for z in aus.splitlines() if z.split() and z.split()[-2] in "TDBCRVW"} - FREMD)
    zeilen = [
        f"/* Erzeugt von werkzeuge/praefix.py – nicht von Hand ändern.",
        f" * Programm „{name}“: {quelle} (KolibriOS, GPL-2.0, unverändert). */",
    ]
    for s in syms:
        zeilen.append(f"#define {s} {name}_{s}")
    rel = os.path.relpath(q, hier / "c" / "gen").replace("\\", "/")
    zeilen.append(f'#include "{rel}"')
    ziel = hier / "c" / "gen" / f"{name}.c"
    ziel.parent.mkdir(exist_ok=True)
    ziel.write_text("\n".join(zeilen) + "\n", encoding="utf-8")
    print(f"{ziel.name}: {len(syms)} Symbole ({', '.join(syms)})")
