#!/usr/bin/env python3
"""W0: Schwärzt die WLAN-Zugangsdaten aus wlan.local in einer Log-Datei
(an Ort und Stelle), bevor jemand sie liest oder als Nachweis ablegt.
Gibt nur die Zahl der Ersetzungen aus, nie die Werte.

    python esp32/firmware/werkzeuge/log_schwaerzen.py LOGDATEI [...]

Rückgabe 0 = nichts gefunden, 1 = geschwärzt (Hinweis: eine Komponente
protokolliert Zugangsdaten – Log-Stufe prüfen), 2 = wlan.local fehlt.
"""
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


def main() -> int:
    datei = REPO / "wlan.local"
    if not datei.exists():
        print("wlan.local fehlt")
        return 2
    werte = {}
    for z in datei.read_text(encoding="utf-8").splitlines():
        if "=" in z:
            k, v = z.split("=", 1)
            if k.strip() in ("SSID", "PASSWORT") and v.strip():
                werte[k.strip()] = v.strip().encode("utf-8")
    gesamt = 0
    for name in sys.argv[1:]:
        p = Path(name)
        b = p.read_bytes()
        # Passwort zuerst (falls die SSID Teil des Passworts ist)
        for k in ("PASSWORT", "SSID"):
            v = werte.get(k)
            if not v:
                continue
            n = b.count(v)
            if n:
                b = b.replace(v, f"[{k}]".encode())
                gesamt += n
                print(f"{p.name}: {k} {n}× geschwärzt")
        p.write_bytes(b)
    if not gesamt:
        print("keine Zugangsdaten im Log")
    return 1 if gesamt else 0


if __name__ == "__main__":
    sys.exit(main())
