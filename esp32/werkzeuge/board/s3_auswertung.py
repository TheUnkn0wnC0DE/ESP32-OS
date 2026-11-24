"""S3: Bildrate je Phase aus dem Board-Log.

Das Log hat alle 2 s eine Zeile „Desktop: X Bilder/s neu, Y übersprungen/s,
Fläche F px/Bild, Compositor C ms/Bild“ (Zeitstempel I (ms) ab Start).
Die Phasen (s3_phasen.txt: Name, Beginn, Ende in ms ab Desktop-Start) werden
auf die Log-Zeit umgerechnet (Desktop-Start = Zeile „ESP32-OS-Desktop
startet“); gemittelt werden die 2-s-Fenster, die ganz in der Phase liegen.

    python s3_auswertung.py board_log.txt s3_phasen.txt [Bezeichnung]
"""
import re
import sys


def main():
    log, phasen_datei = sys.argv[1], sys.argv[2]
    titel = sys.argv[3] if len(sys.argv) > 3 else ''
    zeilen = open(log, encoding='utf-8', errors='replace').read().splitlines()
    start = None
    messung = []
    zeit = re.compile(r'^[IWE] \((\d+)\)')
    desk = re.compile(r'Desktop: ([\d.]+) Bilder/s neu, ([\d.]+) übersprungen/s, Fläche (\d+) px/Bild, Compositor ([\d.]+) ms/Bild')
    for z in zeilen:
        m = zeit.match(z)
        if not m:
            continue
        t = int(m.group(1))
        if 'ESP32-OS-Desktop startet' in z and start is None:
            start = t
        d = desk.search(z)
        if d:
            messung.append((t, float(d.group(1)), int(d.group(3)), float(d.group(4))))
    if start is None:
        print('Desktop-Start nicht im Log')
        return 1
    print(f'S3 Bildrate {titel} (Desktop-Start bei {start} ms)')
    print(f'{"Phase":<14} {"Fenster":>7} {"Bilder/s":>9} {"min":>6} {"Fläche px":>10} {"Compositor ms":>14}')
    for zeile in open(phasen_datei, encoding='utf-8'):
        teile = zeile.split()
        if len(teile) != 3:
            continue
        name, a, e = teile[0], start + int(teile[1]), start + int(teile[2])
        drin = [m for m in messung if m[0] - 2000 >= a and m[0] <= e]
        if not drin:
            print(f'{name:<14} {0:>7} {"–":>9}')
            continue
        fps = [m[1] for m in drin]
        fl = sum(m[2] for m in drin) / len(drin)
        co = sum(m[3] for m in drin) / len(drin)
        print(f'{name:<14} {len(drin):>7} {sum(fps) / len(fps):>9.1f} {min(fps):>6.1f} {fl:>10.0f} {co:>14.2f}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
