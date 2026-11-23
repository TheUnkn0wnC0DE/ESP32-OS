#!/usr/bin/env python3
"""Testdatei per WLAN auf die SD-Karte des Boards legen (Gegenstück zu
src/durchsatz.rs, Firmware mit `ESP32OS_DATEI_EMPFANG=1` gebaut). Das Board
speichert nur in seinem Testordner `ESP32-OS/Bild_Test`; der PC baut die
Verbindung auf (keine eingehenden Verbindungen, keine Firewall-Regel).

    python esp32/firmware/werkzeuge/datei_senden.py BOARD-IP DATEI [NAME]

NAME (Standard: Dateiname) darf nur Buchstaben, Ziffern, `.`, `-` und `_`
enthalten.
"""
import os
import re
import socket
import struct
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

TCP_PORT = 5001


def main() -> int:
    if len(sys.argv) not in (3, 4):
        print(__doc__)
        return 2
    ip, pfad = sys.argv[1], sys.argv[2]
    name = sys.argv[3] if len(sys.argv) == 4 else os.path.basename(pfad)
    if not re.fullmatch(r"[A-Za-z0-9_\-][A-Za-z0-9._\-]{0,63}", name):
        print(f"Name {name!r} nicht erlaubt (Buchstaben, Ziffern, . - _; höchstens 64)")
        return 2
    daten = open(pfad, "rb").read()
    t0 = time.time()
    with socket.create_connection((ip, TCP_PORT), timeout=120) as s:
        n = name.encode()
        s.sendall(b"D" + bytes([len(n)]) + n + struct.pack("<Q", len(daten)))
        s.sendall(daten)
        antwort = b""
        while len(antwort) < 8:
            t = s.recv(8 - len(antwort))
            if not t:
                break
            antwort += t
    dauer = time.time() - t0
    if len(antwort) != 8:
        print("keine Antwort vom Board")
        return 1
    (geschrieben,) = struct.unpack("<Q", antwort)
    ok = geschrieben == len(daten)
    print(f"{name}: {geschrieben} von {len(daten)} Byte gespeichert in {dauer:.1f} s – {'OK' if ok else 'FEHLER (siehe Board-Log)'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
