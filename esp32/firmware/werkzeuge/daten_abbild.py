"""B0: FAT-Abbild der Partition „daten“ (16 MB, mit Verschleißausgleich) für
die neue Aufteilung bauen – mit allen Dateien der alten Partition aus einer
Flash-Sicherung und DOOM1.WAD unter doom/. Prüft das Ergebnis, indem es das
neue Abbild wieder auspackt und jede Datei per SHA-256 vergleicht.

    <IDF-Python> daten_abbild.py SICHERUNG.bin ALT_OFFSET ALT_GROESSE DOOM1.WAD AUSGABE.bin ARBEITSORDNER

Braucht die Python-Umgebung von ESP-IDF (Modul `construct`) und
IDF_PATH bzw. den festen Pfad unten.
"""
import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

IDF = Path(os.environ.get('IDF_PATH', Path.home() / '.espressif' / 'esp-idf' / 'v5.5.5'))
FATFS = IDF / 'components' / 'fatfs'
NEU_GROESSE = 0x1000000

sicherung, alt_ofs, alt_gr, wad, ausgabe, arbeit = sys.argv[1:7]
ausgabe = str(Path(ausgabe).resolve())
alt_ofs, alt_gr = int(alt_ofs, 0), int(alt_gr, 0)
arbeit = Path(arbeit).resolve()
shutil.rmtree(arbeit, ignore_errors=True)
(arbeit / 'alt').mkdir(parents=True)

# 1) alte Partition aus der Sicherung schneiden und auspacken
with open(sicherung, 'rb') as f:
    f.seek(alt_ofs)
    alt = f.read(alt_gr)
(arbeit / 'alt.bin').write_bytes(alt)
subprocess.run([sys.executable, str(FATFS / 'fatfsparse.py'), '--wl-layer', 'enabled', str(arbeit / 'alt.bin')],
               cwd=arbeit / 'alt', check=True)


def baum(wurzel: Path) -> dict:
    """Relativer Pfad (klein) → (Größe, SHA-256) aller Dateien."""
    erg = {}
    for p in sorted(wurzel.rglob('*')):
        if p.is_file():
            d = p.read_bytes()
            erg[str(p.relative_to(wurzel)).replace('\\', '/').lower()] = (len(d), hashlib.sha256(d).hexdigest())
    return erg


# fatfsparse legt die Dateien unter einem Unterordner (Volume-Name) oder direkt ab
kinder = [p for p in (arbeit / 'alt').iterdir()]
quelle = kinder[0] if len(kinder) == 1 and kinder[0].is_dir() and not (arbeit / 'alt' / 'doom').exists() and kinder[0].name.lower() not in ('doom', 'quake', 'settings', 'bilder') else arbeit / 'alt'
alte = baum(quelle)
print(f'Alte Partition: {len(alte)} Dateien')
for k, (n, h) in alte.items():
    print(f'  {k}  {n} B  {h[:16]}')

# 2) neuen Baum zusammenstellen: alte Dateien + WAD
neu_baum = arbeit / 'neu'
shutil.copytree(quelle, neu_baum)
(neu_baum / 'doom').mkdir(exist_ok=True)
shutil.copyfile(wad, neu_baum / 'doom' / 'DOOM1.WAD')
erwartet = baum(neu_baum)

# 3) 16-MB-Abbild mit Verschleißausgleich (4096-Byte-Sektoren, lange Namen)
subprocess.run([sys.executable, str(FATFS / 'wl_fatfsgen.py'), '--output_file', ausgabe,
                '--partition_size', str(NEU_GROESSE), '--sector_size', '4096', '--long_name_support',
                str(neu_baum)], check=True)
assert Path(ausgabe).stat().st_size == NEU_GROESSE, Path(ausgabe).stat().st_size

# 4) Gegenprobe: neues Abbild auspacken und vergleichen
(arbeit / 'probe').mkdir()
subprocess.run([sys.executable, str(FATFS / 'fatfsparse.py'), '--wl-layer', 'enabled', str(Path(ausgabe).resolve())],
               cwd=arbeit / 'probe', check=True)
kinder = [p for p in (arbeit / 'probe').iterdir()]
probe_wurzel = kinder[0] if len(kinder) == 1 and kinder[0].is_dir() and kinder[0].name.lower() not in ('doom', 'quake', 'settings', 'bilder') else arbeit / 'probe'
probe = baum(probe_wurzel)
fehlt = set(erwartet) - set(probe)
falsch = [k for k in erwartet if k in probe and probe[k] != erwartet[k]]
print(f'Neues Abbild: {len(probe)} Dateien, fehlend {sorted(fehlt)}, abweichend {falsch}')
if fehlt or falsch:
    sys.exit('FEHLER: Gegenprobe stimmt nicht')
print('OK: alle Dateien gleich (SHA-256)')
