"""Erzeugt die native ESP32-OS-Fassung von KolibriOS' <sys/ksys.h>.

Jede Anweisung  asm_inline("int $0x40" : AUSGABEN : EINGABEN [: CLOBBER]);
wird ersetzt durch einen Aufruf von  kos_syscall(&regs)  (esp32/kern/src/abi.rs):
Eingaben -> Register-Struktur, Ausgaben <- Register. Der Rest des Headers
(Typen, Inline-Funktionen, Konstanten) bleibt Wort für Wort erhalten.

    python ksys_nativ.py <original ksys.h> <ziel ksys.h>

Quelle: KolibriOS, programs/develop/ktcc/libc.obj/include/sys/ksys.h (GPL-2.0).
"""
import re
import sys

REG = {"a": "eax", "b": "ebx", "c": "ecx", "d": "edx", "S": "esi", "D": "edi"}

KOPF = """/* ESP32-OS: native Fassung von <sys/ksys.h> - erzeugt von
 * esp32/kolibri/werkzeuge/ksys_nativ.py aus dem KolibriOS-Original (GPL-2.0).
 * Systemaufrufe gehen statt über "int $0x40" an kos_syscall() im Kern. */
#ifndef KSYS_NATIV_REGS
#define KSYS_NATIV_REGS
#include <stdint.h>
typedef struct {
    uint32_t eax, ebx, ecx, edx, esi, edi, ebp;
} ksys_regs_t;
#ifdef __cplusplus
extern "C"
#endif
void kos_syscall(ksys_regs_t* r);
/* Wert <-> Register bytegenau (auch Unions wie ksys_time_t), Little Endian. */
#define KSYS_REIN(reg, e) do { __typeof__(e) __v = (e);     __builtin_memcpy(&(reg), &__v, sizeof __v < 4 ? sizeof __v : 4); } while (0)
#define KSYS_RAUS(v, reg) do { (v) = (__typeof__(v)){0};     __builtin_memcpy(&(v), &(reg), sizeof(v) < 4 ? sizeof(v) : 4); } while (0)
#endif
"""


def operanden(text):
    """'"=a"(val), "b"(x+1)' -> [('=a', 'val'), ('b', 'x+1')] (Klammern zählen)."""
    erg, i = [], 0
    while True:
        m = re.compile(r'\s*,?\s*"([^"]+)"\s*\(').match(text, i)
        if not m:
            break
        tiefe, j = 1, m.end()
        while tiefe:
            tiefe += {"(": 1, ")": -1}.get(text[j], 0)
            j += 1
        erg.append((m.group(1), text[m.end():j - 1].strip()))
        i = j
    return erg


def ersetzen(body):
    teile = []  # Abschnitte nach ':' ausserhalb von Klammern/Strings
    tiefe, akt, in_str = 0, "", False
    for ch in body:
        if ch == '"' and not akt.endswith("\\"):
            in_str = not in_str
        if not in_str:
            tiefe += {"(": 1, ")": -1}.get(ch, 0)
            if ch == ":" and tiefe == 0:
                teile.append(akt)
                akt = ""
                continue
        akt += ch
    teile.append(akt)
    code = teile[0]
    aus = operanden(teile[1]) if len(teile) > 1 else []
    ein = operanden(teile[2]) if len(teile) > 2 else []
    z = ["do { ksys_regs_t __r = {0};"]
    for k, e in ein:
        z.append(f"KSYS_REIN(__r.{REG[k]}, {e});")
    if "0x24(%%ebp)" in code:  # Funktion 65: Zeilenabstand aus Parameter "offset"
        z.append("__r.ebp = (uint32_t)(offset);")
    elif "%%ebp" in code or "%ebp" in code:
        raise SystemExit("unbekannte ebp-Verwendung: " + code)
    z.append("kos_syscall(&__r);")
    for k, v in aus:
        k = k.lstrip("=+&")
        if k == "A":
            z.append(f"{{ uint64_t __q = ((uint64_t)__r.edx << 32) | __r.eax; __builtin_memcpy(&({v}), &__q, sizeof({v})); }}")
        else:
            z.append(f"KSYS_RAUS({v}, __r.{REG[k]});")
    z.append("} while (0);")
    return " ".join(z)


def main():
    src = open(sys.argv[1], encoding="latin-1").read()
    n = 0

    def ers(m):
        nonlocal n
        n += 1
        return ersetzen(m.group(1))

    out = re.sub(r"asm_inline\s*\((.*?)\);", ers, src, flags=re.S)
    if "int $0x40" in out or "__asm__" in out.replace("#define asm_inline __asm__ __volatile__", ""):
        raise SystemExit("nicht alle Systemaufrufe ersetzt")
    out = out.replace("#define asm_inline __asm__ __volatile__", "/* asm_inline: ersetzt durch kos_syscall */", 1)
    # Header-Schutz des Originals beibehalten, eigenen Kopf davor
    open(sys.argv[2], "w", encoding="latin-1", newline="\n").write(KOPF + out)
    print(f"{n} Systemaufrufe ersetzt")


main()
