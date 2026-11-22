#!/usr/bin/env python3
"""W0-Prüfung: Stehen die WLAN-Zugangsdaten aus wlan.local irgendwo im Repo
oder in der Git-Geschichte? Gibt nur Zählungen aus, nie die Werte selbst.

    python esp32/firmware/werkzeuge/geheimnis_pruefen.py

Durchsucht alle von Git erfassten und alle nicht ignorierten neuen Dateien
sowie `git log -p --all` (alle Commits aller Zweige). Rückgabe 0 = sauber,
1 = Treffer, 2 = wlan.local fehlt oder ist unvollständig.
"""
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


def werte() -> dict[str, bytes]:
    datei = REPO / "wlan.local"
    if not datei.exists():
        print("wlan.local fehlt")
        sys.exit(2)
    w = {}
    for z in datei.read_text(encoding="utf-8").splitlines():
        if "=" in z:
            k, v = z.split("=", 1)
            if k.strip() in ("SSID", "PASSWORT") and v.strip():
                w[k.strip()] = v.strip().encode("utf-8")
    if "PASSWORT" not in w:
        print("wlan.local: PASSWORT fehlt")
        sys.exit(2)
    return w


def git(*args: str) -> bytes:
    return subprocess.run(["git", "-C", str(REPO), *args], capture_output=True, check=True).stdout


def main() -> int:
    w = werte()
    # wlan.local muss ignoriert sein
    ignoriert = subprocess.run(["git", "-C", str(REPO), "check-ignore", "-q", "wlan.local"]).returncode == 0
    print(f"wlan.local von Git ignoriert: {'ja' if ignoriert else 'NEIN'}")
    dateien = [d for d in git("ls-files", "-z", "-co", "--exclude-standard").split(b"\0") if d]
    treffer = {k: 0 for k in w}
    betroffen = 0
    for d in dateien:
        p = REPO / d.decode("utf-8", "replace")
        try:
            inhalt = p.read_bytes()
        except OSError:
            continue
        gefunden = False
        for k, v in w.items():
            n = inhalt.count(v)
            treffer[k] += n
            gefunden |= n > 0
        betroffen += gefunden
    print(f"Dateien durchsucht: {len(dateien)}, mit Treffer: {betroffen}")
    for k, n in treffer.items():
        print(f"  {k}: {n} Treffer in Dateien")
    log = git("log", "-p", "--all", "--no-color")
    commits = git("rev-list", "--all", "--count").decode().strip()
    log_treffer = {k: log.count(v) for k, v in w.items()}
    print(f"git log -p --all ({commits} Commits, {len(log) // 1024} KiB):")
    for k, n in log_treffer.items():
        print(f"  {k}: {n} Treffer in der Geschichte")
    # Das Passwort darf nirgends stehen; die SSID ebenfalls nicht (sie verrät
    # das Heimnetz).
    sauber = ignoriert and not any(treffer.values()) and not any(log_treffer.values())
    print("ERGEBNIS:", "sauber" if sauber else "TREFFER")
    return 0 if sauber else 1


if __name__ == "__main__":
    sys.exit(main())
