"""Holt die Shareware-WAD von Doom (DOOM1.WAD, Version 1.9) nach esp32/doom/wad/.

    python esp32/doom/wad/holen.py

Quelle: das Debian-Paket „doom-wad-shareware“ (die Shareware-Version darf
unverändert weitergegeben werden). Prüft die bekannte Prüfsumme der
Original-Datei; die WAD wird nicht eingecheckt (.gitignore).
"""
import hashlib
import io
import lzma
import pathlib
import tarfile
import urllib.request

URL = "https://deb.debian.org/debian/pool/non-free/d/doom-wad-shareware/doom-wad-shareware_1.9.fixed-5_all.deb"
MD5 = "f0cefca49926d00903cf57551d901abe"  # DOOM1.WAD 1.9 Shareware (doomwiki.org)
ZIEL = pathlib.Path(__file__).with_name("DOOM1.WAD")


def wad_aus_deb(deb: bytes) -> bytes:
    assert deb[:8] == b"!<arch>\n", "kein .deb"
    p = 8
    while p < len(deb):
        name = deb[p:p + 16].decode().strip()
        size = int(deb[p + 48:p + 58].decode().strip())
        data = deb[p + 60:p + 60 + size]
        if name.startswith("data.tar"):
            with tarfile.open(fileobj=io.BytesIO(lzma.decompress(data))) as t:
                for m in t.getmembers():
                    if m.name.lower().endswith("doom1.wad"):
                        return t.extractfile(m).read()
        p += 60 + size + (size & 1)
    raise SystemExit("DOOM1.WAD nicht im Paket gefunden")


def main():
    if ZIEL.exists() and hashlib.md5(ZIEL.read_bytes()).hexdigest() == MD5:
        print(f"{ZIEL} ist schon da.")
        return
    print(f"Lade {URL} …")
    with urllib.request.urlopen(URL) as r:
        deb = r.read()
    wad = wad_aus_deb(deb)
    md5 = hashlib.md5(wad).hexdigest()
    if md5 != MD5:
        raise SystemExit(f"Prüfsumme falsch ({md5}), Datei nicht gespeichert")
    ZIEL.write_bytes(wad)
    print(f"{ZIEL} gespeichert ({len(wad)} Bytes, MD5 stimmt).")


if __name__ == "__main__":
    main()
