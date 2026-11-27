# Erzeugt die eigene Test-Melodie für den Musikplayer (D2, Board-Nachweis
# MIDI): desktop/tests/daten/Testmelodie.mid – Standard-MIDI-Datei Format 1
# mit drei Spuren (Tempo, Tonleiter C-Dur auf und ab mit Schlussakkord,
# Trommel auf Kanal 10), etwa 9 Sekunden. Keine fremden Inhalte.
#
#   python esp32/werkzeuge/test_midi.py
import pathlib
import struct

TEILUNG = 480  # Ticks je Viertel


def vlq(n):
    b = [n & 0x7F]
    n >>= 7
    while n:
        b.insert(0, 0x80 | (n & 0x7F))
        n >>= 7
    return bytes(b)


def spur(ereignisse):
    """ereignisse: Liste (abstand_ticks, bytes)"""
    d = b''.join(vlq(t) + e for t, e in ereignisse) + vlq(0) + b'\xFF\x2F\x00'
    return b'MTrk' + struct.pack('>I', len(d)) + d


# Spur 0: Tempo 120 (500 000 µs je Viertel), nach 8 Vierteln 150
tempo = spur([
    (0, b'\xFF\x51\x03' + (500000).to_bytes(3, 'big')),
    (8 * TEILUNG, b'\xFF\x51\x03' + (400000).to_bytes(3, 'big')),
])

# Spur 1: Tonleiter C-Dur auf und ab (Achtel), dann C-Dur-Akkord (Halbe)
leiter = [60, 62, 64, 65, 67, 69, 71, 72, 71, 69, 67, 65, 64, 62, 60]
e = [(0, b'\xC0\x00')]
for n in leiter:
    e.append((0, bytes([0x90, n, 96])))
    e.append((TEILUNG // 2, bytes([0x80, n, 0])))
for n in (60, 64, 67):
    e.append((0, bytes([0x90, n, 90])))
e.append((2 * TEILUNG, bytes([0x80, 60, 0])))
e.append((0, bytes([0x80, 64, 0])))
e.append((0, bytes([0x80, 67, 0])))
melodie = spur(e)

# Spur 2: Trommel (Kanal 10) auf jedem Viertel
t = []
for k in range(len(leiter) // 2 + 3):
    t.append((0 if k == 0 else TEILUNG - 60, bytes([0x99, 36 if k % 2 == 0 else 38, 110])))
    t.append((60, bytes([0x89, 36 if k % 2 == 0 else 38, 0])))
trommel = spur(t)

kopf = b'MThd' + struct.pack('>IHHH', 6, 1, 3, TEILUNG)
ziel = pathlib.Path(__file__).resolve().parents[1] / 'desktop' / 'tests' / 'daten' / 'Testmelodie.mid'
ziel.write_bytes(kopf + tempo + melodie + trommel)
print(ziel.name, ziel.stat().st_size, 'Byte')
