"""Empfängt Bildschirmfotos vom Board (Firmware mit `KONSOLE_BILDER=…`
gebaut, siehe src/foto.rs) und speichert sie als PNG (quer, 1280×720).
Alle anderen Logzeilen werden ausgegeben.

    python esp32/firmware/bildschirmfoto.py SEKUNDEN ZIELORDNER [PORT] [--reset] [--name praefix]

Braucht pyserial und Pillow.
"""
import os
import sys
import time

import serial
from PIL import Image

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

args = [a for a in sys.argv[1:] if not a.startswith("--")]
secs = float(args[0])
ziel = args[1]
port = args[2] if len(args) > 2 else "COM3"
name = "bild"
if "--name" in sys.argv:
    name = sys.argv[sys.argv.index("--name") + 1]
    if name in args:
        args.remove(name)
os.makedirs(ziel, exist_ok=True)


def empfangen(s, w, h, baud, gedreht, nr):
    s.baudrate = baud
    n = w * h * 2
    buf = b""
    ende = time.time() + 60
    # Kopf suchen.
    while b"BILDDATA" not in buf and time.time() < ende:
        buf += s.read(4096)
        buf = buf[-(64 + n):]
    if b"BILDDATA" not in buf:
        print(f"BILD {nr}: kein Kopf empfangen")
        s.baudrate = 115200
        return
    buf = buf[buf.index(b"BILDDATA") + 8:]
    while len(buf) < n + 4 and time.time() < ende:
        buf += s.read(65536)
    s.baudrate = 115200
    if len(buf) < n + 4:
        print(f"BILD {nr}: nur {len(buf)} von {n + 4} Bytes")
        return
    px, summe = buf[:n], int.from_bytes(buf[n:n + 4], "little")
    if sum(px) & 0xFFFFFFFF != summe:
        print(f"BILD {nr}: Prüfsumme falsch")
        return
    # RGB565 → RGB888.
    rgb = bytearray(w * h * 3)
    for i in range(w * h):
        v = px[2 * i] | px[2 * i + 1] << 8
        r, g, b = v >> 11, (v >> 5) & 63, v & 31
        rgb[3 * i] = (r << 3) | (r >> 2)
        rgb[3 * i + 1] = (g << 2) | (g >> 4)
        rgb[3 * i + 2] = (b << 3) | (b >> 2)
    img = Image.frombytes("RGB", (w, h), bytes(rgb))
    # Das Panel zeigt das Querbild 90° (gedreht: 270°) gegen den Uhrzeigersinn.
    img = img.transpose(Image.Transpose.ROTATE_90 if gedreht else Image.Transpose.ROTATE_270)
    pfad = os.path.join(ziel, f"{name}_{nr}.png")
    img.save(pfad)
    print(f"BILD {nr} gespeichert: {pfad}")


s = serial.Serial(None, 115200, timeout=0.2)
s.port = port
# Beim USB-Serial/JTAG setzt DTR aus mit RTS an das Board zurück
s.rts = False
s.dtr = True
s.open()
with s:
    if "--reset" in sys.argv:
        s.dtr = False
        s.rts = True
        time.sleep(0.1)
        s.rts = False
    end = time.time() + secs
    buf = b""
    while time.time() < end:
        buf += s.read(4096)
        *lines, buf = buf.split(b"\n")
        for line in lines:
            t = line.decode("utf-8", errors="replace").rstrip("\r")
            print(t)
            if ": BILD " in t and "gesendet" not in t:
                teile = t.split(": BILD ")[1].split()
                nr, w, h, baud, gedreht = int(teile[0]), int(teile[1]), int(teile[2]), int(teile[3]), teile[4] == "1"
                empfangen(s, w, h, baud, gedreht, nr)
                buf = b""
