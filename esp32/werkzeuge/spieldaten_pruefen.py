"""S5 (Auftrag 12.8): Prüft, dass die Firmware-Abbilder keine Spieldaten
enthalten – weder DOOM1.WAD (Kopf "IWAD" + Lumpzahl, Lump-Verzeichnis)
noch pak0.pak (Verzeichnis mit 64-Byte-Einträgen wie "maps/e1m1.bsp").

Aufruf: python -I spieldaten_pruefen.py C:/eo/esp32os.bin C:/eo/doomapp.bin …
Rückgabe 0 = sauber, 1 = Spieldaten gefunden.
"""

import re
import sys

NUL = b'\x00'
# Lump-Name: 1–8 Zeichen, mit Nullen auf 8 Byte aufgefüllt
NAME8 = re.compile(rb'[A-Z0-9_\-\[\]]{1,8}' + NUL + b'*')
PFAD = rb'(maps|progs|sound|gfx)/[a-z0-9_/]+\.(bsp|mdl|wav|lmp)'


def pruefen(pfad):
    d = open(pfad, 'rb').read()
    befunde = []
    # WAD-Kopf: "IWAD"/"PWAD" + Lumpzahl
    for m in re.finditer(rb'[IP]WAD', d):
        lumps = int.from_bytes(d[m.start() + 4:m.start() + 8], 'little')
        if 100 <= lumps <= 20000:
            befunde.append(f'WAD-Kopf bei {m.start():#x} ({lumps} Lumps)')
    # Lump-Verzeichnis: 16-Byte-Einträge (Position, Länge, Name) – nach
    # PLAYPAL/COLORMAP folgt 16 Byte später der nächste Name
    for m in re.finditer(rb'PLAYPAL' + NUL + b'|COLORMAP', d):
        nach = d[m.start() + 16:m.start() + 24]
        if len(nach) == 8 and NAME8.fullmatch(nach) and nach[:1].isalpha():
            befunde.append(f'WAD-Verzeichnis bei {m.start():#x}')
    # PAK-Verzeichnis: Name mit Nullen bis 56 Byte, dann Offset und Länge;
    # der nächste Eintrag folgt genau 64 Byte später
    for m in re.finditer(PFAD + NUL, d):
        a = m.start()
        name = d[a:a + 56]
        laenge = len(m.group()) - 1
        if len(name) == 56 and name[laenge:] == bytes(56 - laenge) and re.match(PFAD, d[a + 64:a + 120]):
            befunde.append(f'PAK-Verzeichnis "{m.group()[:-1].decode()}" bei {a:#x}')
    return len(d), befunde


def main():
    fehler = False
    for p in sys.argv[1:]:
        groesse, befunde = pruefen(p)
        print(f'{p}: {groesse // 1024} KB, {"sauber" if not befunde else "SPIELDATEN: " + "; ".join(befunde[:5])}')
        fehler |= bool(befunde)
    sys.exit(1 if fehler else 0)


if __name__ == '__main__':
    main()
