"""Baut eine KolibriOS-Skin-Datei (.skn) aus den Original-Quellen eines Skins
(default.asm + BMPs + colors.dtp.asm), Byte für Byte nach den Makros in
KolibriOS skins/skin.inc – Ersatz für den FASM-Lauf ohne kpack.

    python skn_bauen.py <skin-ordner> <ziel.skn>

Layout (skin.inc):
  dd 'SKIN', 1, params, buttons, bitmaps
  params:  dd height; dw right,left,bottom,top; dd inner,outer,frame (aktiv);
           dd inner,outer,frame (inaktiv); dd dtp_groesse; dtp (10 dd)
  buttons: { dd typ (1 schliessen, 2 minimieren); dw left,top,width,height } dd 0
  bitmaps: { dw art (1 left, 2 oper, 3 base), aktiv (1/0); dd offset } dd 0
  bitmap:  align 4; dd w,h; Zeilen oben→unten, je Pixel 3 Byte wie in der BMP (B,G,R)
"""
import re
import struct
import sys
from pathlib import Path


def zahl(s):
    s = s.strip()
    return int(s, 16) if s.lower().startswith("0x") else int(s)


def bmp(pfad):
    d = pfad.read_bytes()
    off = struct.unpack_from("<I", d, 10)[0]
    w, h = struct.unpack_from("<ii", d, 18)
    bpp = struct.unpack_from("<H", d, 28)[0]
    if bpp != 24:
        raise SystemExit(f"{pfad}: nur 24 bpp")
    zeile = w * 3 + (w % 4)  # wie skin.inc (nicht die BMP-Norm!)
    out = bytearray(struct.pack("<II", w, h))
    for y in range(h):
        a = off + zeile * (h - 1 - y)
        out += d[a:a + w * 3]
    return w, h, bytes(out)


def main():
    ordner = Path(sys.argv[1])
    asm = (ordner / "default.asm").read_text(encoding="latin-1")
    asm = re.sub(r"\\\s*\n", " ", re.sub(r";[^\n]*", "", asm))
    bitmaps = {}
    for name, datei in re.findall(r"BITMAP\s+(\w+)\s*,\s*'([^']+)'", asm):
        bitmaps[name] = bmp(ordner / datei)
    hoehe_s = re.search(r"height\s*=\s*([\w.]+)", asm).group(1)
    hoehe = bitmaps[hoehe_s.split(".")[0]][1] if ".height" in hoehe_s else zahl(hoehe_s)
    m = [zahl(x) for x in re.search(r"margins\s*=\s*\[([^\]]+)\]", asm).group(1).split(":")]
    left, top, right, bottom = m

    def farben(art):
        t = re.search(rf"colors\s+{art}\s*=\s*\[([^\]]+)\]", asm).group(1)
        f = dict((k.strip(), zahl(v)) for k, v in (x.split("=") for x in t.split(":")))
        return f["binner"], f["bouter"], f["bframe"]

    dtp_asm = (ordner / "colors.dtp.asm").read_text(encoding="latin-1")
    dtp = b"".join(struct.pack("<I", zahl(v)) for v in re.findall(r"\bdd\s+(0x[0-9A-Fa-f]+|\d+)", dtp_asm))
    if len(dtp) != 40:
        raise SystemExit(f"dtp: {len(dtp)} Bytes statt 40")

    params = struct.pack("<I4H", hoehe, right, left, bottom, top)
    params += struct.pack("<3I", *farben("active")) + struct.pack("<3I", *farben("inactive"))
    params += struct.pack("<I", len(dtp)) + dtp

    buttons = b""
    for typ, wert in re.findall(r"\b(close|minimize)\s*=\s*(\[[^\]]+\]\[[^\]]+\])", asm):
        q = [zahl(x) for x in re.findall(r"-?\w+", wert)]
        buttons += struct.pack("<I4h", 1 if typ == "close" else 2, *q)
    buttons += struct.pack("<I", 0)

    eintraege = re.findall(r"\b(left|oper|base)\s+(active|inactive)\s*=\s*(\w+)", asm)
    kopf = 20
    p_off = kopf
    b_off = p_off + len(params)
    bm_off = b_off + len(buttons)
    bm_tab_len = 8 * len(eintraege) + 4
    daten = bytearray()
    lage = {}
    pos = bm_off + bm_tab_len
    for name, (w, h, roh) in bitmaps.items():
        pad = (-pos) % 4
        daten += b"\0" * pad
        pos += pad
        lage[name] = pos
        daten += roh
        pos += len(roh)
    tab = b"".join(
        struct.pack("<HHI", {"left": 1, "oper": 2, "base": 3}[art], 1 if akt == "active" else 0, lage[name])
        for art, akt, name in eintraege
    ) + struct.pack("<I", 0)
    out = b"SKIN" + struct.pack("<4I", 1, p_off, b_off, bm_off) + params + buttons + tab + bytes(daten)
    Path(sys.argv[2]).write_bytes(out)
    print(f"{sys.argv[2]}: {len(out)} Bytes, Höhe {hoehe}, {len(bitmaps)} Bitmaps, Knöpfe {len(buttons) // 12}")


main()
