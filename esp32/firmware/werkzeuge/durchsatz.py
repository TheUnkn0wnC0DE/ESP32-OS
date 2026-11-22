#!/usr/bin/env python3
"""W8: WLAN-Durchsatz zum Board messen (Gegenstück zu src/durchsatz.rs,
Firmware mit `ESP32OS_DURCHSATZ=1` gebaut). Der PC baut alle Verbindungen
selbst auf (keine eingehenden Verbindungen, keine Firewall-Regel nötig).

    python esp32/firmware/werkzeuge/durchsatz.py BOARD-IP [SEKUNDEN]

Messungen (je SEKUNDEN, Standard 10):
  TCP PC → Board   ('R': Board empfängt)
  TCP Board → PC   ('S': Board sendet)
  UDP PC → Board   (Datagramme zu 1460 Byte, Board zählt, meldet die Zahl)
  UDP Board → PC   (Board sendet an die Absenderadresse der Anfrage)
"""
import socket
import struct
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

TCP_PORT = 5001
UDP_PORT = 5002
BLOCK = 1460


def mbit(n: int, s: float) -> str:
    return f"{n * 8 / s / 1e6:6.2f} Mbit/s ({n / 1024:.0f} KiB in {s:.1f} s)"


def tcp_hin(ip: str, sek: float) -> None:
    with socket.create_connection((ip, TCP_PORT), timeout=10) as s:
        s.sendall(b"R")
        daten = bytes(64 * 1024)
        n, t0 = 0, time.time()
        while time.time() - t0 < sek:
            s.sendall(daten)
            n += len(daten)
        s.shutdown(socket.SHUT_WR)
        # Board meldet seine Zählung (8 Byte) und Zeit in µs (8 Byte)
        antwort = b""
        while len(antwort) < 16:
            t = s.recv(16 - len(antwort))
            if not t:
                break
            antwort += t
        dauer = time.time() - t0
    if len(antwort) == 16:
        bn, bus = struct.unpack("<QQ", antwort)
        print(f"TCP PC → Board: {mbit(bn, bus / 1e6)}  (PC gesendet {n / 1024:.0f} KiB)")
    else:
        print(f"TCP PC → Board: PC-Sicht {mbit(n, dauer)}, keine Antwort vom Board")


def tcp_her(ip: str, sek: float) -> None:
    with socket.create_connection((ip, TCP_PORT), timeout=10) as s:
        s.sendall(b"S" + struct.pack("<I", int(sek * 1000)))
        n, t0 = 0, None
        while True:
            t = s.recv(65536)
            if not t:
                break
            if t0 is None:
                t0 = time.time()
            n += len(t)
        dauer = time.time() - (t0 or time.time())
    print(f"TCP Board → PC: {mbit(n, max(dauer, 1e-3))}")


def udp_hin(ip: str, sek: float, rate_mbit: float = 20.0) -> None:
    """Mit fester Senderate (ungebremst verwirft schon der Zugangspunkt fast alles)."""
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.settimeout(3)
        block = b"\x00" * BLOCK
        n, gesendet, t0 = 0, 0, time.time()
        s.sendto(b"START", (ip, UDP_PORT))
        abstand = BLOCK * 8 / (rate_mbit * 1e6)
        while time.time() - t0 < sek:
            s.sendto(block, (ip, UDP_PORT))
            gesendet += 1
            n += BLOCK
            # Rate halten (aktiv warten, sleep ist unter Windows zu grob)
            ziel = t0 + gesendet * abstand
            while time.time() < ziel:
                pass
        time.sleep(0.3)
        s.sendto(b"ENDE", (ip, UDP_PORT))
        try:
            antwort, _ = s.recvfrom(64)
            bn, bus = struct.unpack("<QQ", antwort[:16])
            verlust = 100 * (1 - bn / max(n, 1))
            print(f"UDP PC → Board (gesendet mit {rate_mbit:.0f} Mbit/s): {mbit(bn, bus / 1e6)}, Verlust {verlust:.1f} %")
        except (socket.timeout, struct.error):
            print("UDP PC → Board: keine Antwort vom Board")


def udp_her(ip: str, sek: float) -> None:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.settimeout(2)
        s.sendto(b"SENDE" + struct.pack("<I", int(sek * 1000)), (ip, UDP_PORT))
        n, t0, t1 = 0, None, None
        while True:
            try:
                t, _ = s.recvfrom(4096)
            except socket.timeout:
                break
            if t == b"FERTIG":
                break
            t1 = time.time()
            if t0 is None:
                t0 = t1
            n += len(t)
        if t0 is None:
            print("UDP Board → PC: nichts empfangen")
        else:
            print(f"UDP Board → PC: {mbit(n, max(t1 - t0, 1e-3))}")


def main() -> None:
    ip = sys.argv[1]
    sek = float(sys.argv[2]) if len(sys.argv) > 2 else 10.0
    for f in (tcp_hin, tcp_her, udp_hin, udp_her):
        f(ip, sek)
        time.sleep(1)


if __name__ == "__main__":
    main()
