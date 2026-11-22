"""Liest die serielle Ausgabe des Boards für eine feste Zeit mit.

    python esp32/firmware/seriell_log.py [SEKUNDEN] [PORT] [--reset]

Anders als ein abgebrochenes `espflash monitor` geht dabei keine gepufferte
Ausgabe verloren. Mit --reset wird das Board vorher neu gestartet (RTS).
Braucht pyserial (`pip install pyserial`).
"""
import sys
import time

import serial

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

secs = float(sys.argv[1]) if len(sys.argv) > 1 and not sys.argv[1].startswith("-") else 60.0
port = next((a for a in sys.argv[2:] if not a.startswith("-")), "COM3")
with serial.Serial(port, 115200, timeout=0.2) as s:
    if "--reset" in sys.argv:
        # Wie esptool: EN über RTS kurz auf Low, IO0 (DTR) bleibt High.
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
            print(line.decode("utf-8", "replace").rstrip(), flush=True)
    if buf:
        print(buf.decode("utf-8", "replace").rstrip(), flush=True)
