"""Erzeugt kleine Test-MP3s (MPEG-1 Layer III, mono) ohne Encoder.

Jedes Granulat (576 Abtastwerte) enthält genau eine Spektrallinie mit dem
Betrag 1 (Huffman-Tabelle 1, keine Skalenfaktoren); die Lautstärke kommt aus
global_gain. Die Linie liegt in der Mitte eines Teilbands (Linie 9 von 18,
ausserhalb der Alias-Reduktion) – das ergibt einen Ton nahe
(Teilband + 0,5) · fs / 64. Wechselt das Teilband, entsteht eine kleine
Melodie. Eigene Testdaten für den Musikplayer (A5), keine fremde Musik.

Aufruf: python -I test_mp3.py <zielordner>
"""

import sys
from pathlib import Path

RATEN = {44_100: 0b00, 48_000: 0b01, 32_000: 0b10}
BITRATEN = {32: 0b0001, 40: 0b0010, 48: 0b0011, 56: 0b0100, 64: 0b0101}


class Bits:
    def __init__(self):
        self.bits = []

    def put(self, wert, n):
        for i in range(n - 1, -1, -1):
            self.bits.append((wert >> i) & 1)

    def code(self, s):
        self.bits.extend(int(c) for c in s)

    def bytes(self, laenge):
        b = self.bits + [0] * (laenge * 8 - len(self.bits))
        assert len(b) == laenge * 8, (len(self.bits), laenge)
        return bytes(int("".join(map(str, b[i:i + 8])), 2) for i in range(0, len(b), 8))


def granulat_daten(linie):
    """Huffman-Daten (Tabelle 1) für eine Linie mit Betrag 1, positiv."""
    b = Bits()
    paare = linie // 2
    for _ in range(paare):
        b.code("1")  # (0, 0)
    if linie % 2 == 0:
        b.code("01")  # (1, 0)
    else:
        b.code("001")  # (0, 1)
    b.put(0, 1)  # Vorzeichen +
    return b.bits, paare + 1


def frame(rate, kbit, linien, gain):
    groesse = 144 * kbit * 1000 // rate
    b = Bits()
    # Kopf: Sync, MPEG-1, Layer III, ohne CRC
    b.put(0xFFFB, 16)
    b.put(BITRATEN[kbit], 4)
    b.put(RATEN[rate], 2)
    b.put(0, 1)  # padding
    b.put(0, 1)  # private
    b.put(0b11, 2)  # mono
    b.put(0, 2)  # mode extension
    b.put(0, 1)  # copyright
    b.put(1, 1)  # original
    b.put(0, 2)  # emphasis
    # Seiteninformation (mono, 17 Byte)
    b.put(0, 9)  # main_data_begin
    b.put(0, 5)  # private_bits
    b.put(0, 4)  # scfsi
    daten = []
    for linie in linien:
        bits, big_values = granulat_daten(linie)
        daten.extend(bits)
        b.put(len(bits), 12)  # part2_3_length (keine Skalenfaktoren)
        b.put(big_values, 9)
        b.put(gain, 8)  # global_gain
        b.put(0, 4)  # scalefac_compress → slen 0/0
        b.put(0, 1)  # window_switching_flag
        for _ in range(3):
            b.put(1, 5)  # table_select: Tabelle 1
        b.put(7, 4)  # region0_count
        b.put(7, 3)  # region1_count
        b.put(0, 1)  # preflag
        b.put(0, 1)  # scalefac_scale
        b.put(0, 1)  # count1table_select
    assert len(b.bits) == 32 + 136
    b.bits.extend(daten)
    return b.bytes(groesse)


def datei(pfad, rate, kbit, sekunden, teilbaender, gain=204):
    """Melodie: jedes Teilband eine halbe Sekunde, im Kreis."""
    frames = int(sekunden * rate / 1152)
    halbe = rate / 2
    aus = bytearray()
    for f in range(frames):
        linien = []
        for g in range(2):
            t = (f * 2 + g) * 576
            sb = teilbaender[int(t // halbe) % len(teilbaender)]
            linien.append(sb * 18 + 9)
        aus += frame(rate, kbit, linien, gain)
    Path(pfad).write_bytes(aus)
    print(f"{pfad}: {len(aus)} Byte, {frames} Frames, {frames * 1152 / rate:.2f} s")


def main():
    ziel = Path(sys.argv[1])
    ziel.mkdir(parents=True, exist_ok=True)
    datei(ziel / "Testton 1.mp3", 44_100, 32, 8.0, [0, 1, 2, 1])
    datei(ziel / "Testton 2.mp3", 48_000, 48, 6.0, [2, 3])


if __name__ == "__main__":
    main()
