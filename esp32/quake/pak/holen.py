"""Holt die Shareware-Daten von Quake (id1/pak0.pak, Version 1.06) nach esp32/quake/pak/.

    python esp32/quake/pak/holen.py

Quelle: quake106.zip vom Spiegel des id-Software-Servers (Shareware, frei
weitergebbar). Die PAK steckt im LHA-Archiv „resource.1“; zum Auspacken wird
7-Zip gebraucht. Prüfsummen von Archiv und PAK werden kontrolliert; die PAK
wird nicht eingecheckt (.gitignore).
"""
import hashlib
import pathlib
import shutil
import subprocess
import tempfile
import urllib.request

URL = "https://ftp.gwdg.de/pub/misc/ftp.idsoftware.com/idstuff/quake/quake106.zip"
MD5_ZIP = "8cee4d03ee092909fdb6a4f84f0c1357"
MD5_PAK = "5906e5998fc3d896ddaf5e6a62e03abb"
ZIEL = pathlib.Path(__file__).with_name("pak0.pak")
SIEBEN_ZIP = shutil.which("7z") or r"C:\Program Files\7-Zip\7z.exe"


def md5(p: pathlib.Path) -> str:
    return hashlib.md5(p.read_bytes()).hexdigest()


def main():
    if ZIEL.exists() and md5(ZIEL) == MD5_PAK:
        print(f"{ZIEL} ist schon da.")
        return
    with tempfile.TemporaryDirectory() as t:
        t = pathlib.Path(t)
        zip_ = t / "quake106.zip"
        print(f"Lade {URL} …")
        with urllib.request.urlopen(URL) as r:
            zip_.write_bytes(r.read())
        if md5(zip_) != MD5_ZIP:
            raise SystemExit("Prüfsumme des Archivs falsch")
        subprocess.run([SIEBEN_ZIP, "x", "-y", f"-o{t / 'z'}", str(zip_), "resource.1"], check=True, capture_output=True)
        subprocess.run([SIEBEN_ZIP, "x", "-y", f"-o{t / 'r'}", str(t / "z" / "resource.1")], check=True, capture_output=True)
        pak = next((p for p in (t / "r").rglob("*") if p.name.lower() == "pak0.pak"), None)
        if pak is None or md5(pak) != MD5_PAK:
            raise SystemExit("pak0.pak fehlt oder Prüfsumme falsch")
        shutil.copyfile(pak, ZIEL)
    print(f"{ZIEL} gespeichert (MD5 stimmt).")


if __name__ == "__main__":
    main()
