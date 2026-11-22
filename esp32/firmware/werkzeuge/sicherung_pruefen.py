"""B0: Flash-Sicherung prüfen, bevor neu aufgeteilt wird.

    <IDF-Python> sicherung_pruefen.py FLASH_1.bin FLASH_2.bin DOOM1.WAD pak0.pak

* beide Abzüge gleich (SHA-256),
* Partitionstabelle bei 0x8000 lesbar (Ausgabe als Liste),
* WAD und PAK im Abzug byte-gleich mit den Dateien auf dem PC,
* NVS: Seitenköpfe gültig, Schlüssel aufgelistet (nvs_tool von ESP-IDF).
Gibt am Ende OK oder FEHLER aus.
"""
import hashlib
import os
import struct
import subprocess
import sys
from pathlib import Path

IDF = Path(os.environ.get('IDF_PATH', Path.home() / '.espressif' / 'esp-idf' / 'v5.5.5'))
a, b, wad, pak = sys.argv[1:5]
fehler = []
d1, d2 = Path(a).read_bytes(), Path(b).read_bytes()
h1, h2 = hashlib.sha256(d1).hexdigest(), hashlib.sha256(d2).hexdigest()
print(f'Abzug 1: {len(d1)} Bytes, SHA-256 {h1}')
print(f'Abzug 2: {len(d2)} Bytes, SHA-256 {h2}')
if d1 != d2 or len(d1) != 32 * 1024 * 1024:
    fehler.append('Abzüge verschieden oder nicht 32 MB')

# Partitionstabelle (Einträge à 32 Bytes, Magie 0xAA50)
print('Partitionstabelle (0x8000):')
teile = {}
for i in range(0, 0xC00, 32):
    e = d1[0x8000 + i:0x8000 + i + 32]
    if e[:2] != b'\xaa\x50':
        break
    typ, sub, ofs, gr = e[2], e[3], *struct.unpack('<II', e[4:12])
    name = e[12:28].split(b'\0')[0].decode()
    teile[name] = (ofs, gr)
    print(f'  {name:10} Typ {typ} Sub {sub:#04x} @ {ofs:#09x} Größe {gr:#09x}')
for n in ('nvs', 'otadata', 'factory', 'doomapp', 'quakeapp', 'doomwad', 'daten', 'quakepak'):
    if n not in teile:
        fehler.append(f'Partition {n} fehlt in der Tabelle')


def vergleich(name, datei):
    ofs, gr = teile[name]
    inhalt = Path(datei).read_bytes()
    gleich = d1[ofs:ofs + len(inhalt)] == inhalt and len(inhalt) <= gr
    print(f'{name}: {Path(datei).name} ({len(inhalt)} Bytes) im Abzug {"GLEICH" if gleich else "VERSCHIEDEN"}, '
          f'SHA-256 {hashlib.sha256(inhalt).hexdigest()}')
    if not gleich:
        fehler.append(f'{name} weicht von {datei} ab')


if 'doomwad' in teile:
    vergleich('doomwad', wad)
if 'quakepak' in teile:
    vergleich('quakepak', pak)

# NVS mit dem Werkzeug von ESP-IDF auflisten
if 'nvs' in teile:
    ofs, gr = teile['nvs']
    nvs = Path(a).with_name('nvs.bin')
    nvs.write_bytes(d1[ofs:ofs + gr])
    werkzeug = IDF / 'components' / 'nvs_flash' / 'nvs_partition_tool' / 'nvs_tool.py'
    r = subprocess.run([sys.executable, str(werkzeug), '--dump', 'minimal', '--color', 'never', str(nvs)],
                       capture_output=True, text=True)
    print('NVS (nvs_tool --dump minimal):')
    print(r.stdout[-3000:] if r.stdout else r.stderr[-2000:])
    if r.returncode != 0:
        fehler.append('nvs_tool meldet Fehler')

# Sicherung der einzelnen Partitionen als Dateien
for n, (ofs, gr) in teile.items():
    Path(a).with_name(f'teil_{n}.bin').write_bytes(d1[ofs:ofs + gr])

print('FEHLER: ' + '; '.join(fehler) if fehler else 'OK: Sicherung vollständig und geprüft')
sys.exit(1 if fehler else 0)
