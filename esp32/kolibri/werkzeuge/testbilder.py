"""Erzeugt Testbilder für die libimg-Dekoder und Pillows Referenz (RGBA roh).

    python testbilder.py <zielordner>

Je Bild: <name>.<endung> und <name>.rgba (u32 w, u32 h, w*h*4 Bytes RGBA;
bei GIF alle Einzelbilder hintereinander, je mit eigenem Kopf).
"""
import io
import struct
import sys
from pathlib import Path
from PIL import Image, ImageDraw

ziel = Path(sys.argv[1])
ziel.mkdir(parents=True, exist_ok=True)


def grund(w=37, h=23):
    im = Image.new("RGBA", (w, h))
    px = im.load()
    for y in range(h):
        for x in range(w):
            px[x, y] = ((x * 7) % 256, (y * 11) % 256, ((x + y) * 5) % 256, 255 if (x + y) % 5 else 90)
    d = ImageDraw.Draw(im)
    d.ellipse((5, 3, 25, 19), fill=(250, 40, 30, 255))
    return im


def ref(name, frames):
    with open(ziel / f"{name}.rgba", "wb") as f:
        for im in frames:
            im = im.convert("RGBA")
            f.write(struct.pack("<II", *im.size))
            f.write(im.tobytes())


def speichern(name, im, fmt, **kw):
    b = io.BytesIO()
    im.save(b, fmt, **kw)
    endung = {"PNG": "png", "BMP": "bmp", "GIF": "gif", "JPEG": "jpg"}[fmt]
    (ziel / f"{name}.{endung}").write_bytes(b.getvalue())
    neu = Image.open(io.BytesIO(b.getvalue()))
    if fmt == "GIF":
        frames = []
        try:
            while True:
                frames.append(neu.convert("RGBA").copy())
                neu.seek(neu.tell() + 1)
        except EOFError:
            pass
        ref(name, frames)
    else:
        ref(name, [neu])


g = grund()
speichern("png_rgba8", g, "PNG")
speichern("png_rgb8_interlace", g.convert("RGB"), "PNG", interlace=1) if False else None
# Pillow kann nicht interlaced schreiben -> Interlace-Test mit eigener Datei unten
speichern("png_rgb8", g.convert("RGB"), "PNG")
speichern("png_l8", g.convert("L"), "PNG")
speichern("png_1bit", g.convert("1"), "PNG")
# 16-Bit-Grau: Werte v*257; Pillow skaliert beim Umwandeln nicht -> Referenz = 8-Bit-Bild
l8 = g.convert("L")
l16 = Image.frombytes("I;16", l8.size, b"".join(struct.pack("<H", v * 257) for v in l8.tobytes()))
_b = io.BytesIO()
l16.save(_b, "PNG")
(ziel / "png_l16.png").write_bytes(_b.getvalue())
ref("png_l16", [l8])
speichern("png_pal_trns", g.convert("P", palette=Image.ADAPTIVE, colors=16), "PNG", transparency=3)
speichern("bmp_24", g.convert("RGB"), "BMP")
speichern("bmp_8", g.convert("P", palette=Image.ADAPTIVE, colors=200), "BMP")
speichern("bmp_4", g.convert("P", palette=Image.ADAPTIVE, colors=16), "BMP")
speichern("bmp_1", g.convert("1"), "BMP")
speichern("gif_ein", g.convert("P", palette=Image.ADAPTIVE, colors=64), "GIF")
f1 = g.convert("P", palette=Image.ADAPTIVE, colors=32)
f2 = g.rotate(90, expand=False).convert("P", palette=Image.ADAPTIVE, colors=32)
speichern("gif_anim", f1, "GIF", save_all=True, append_images=[f2], duration=[100, 250], loop=0, interlace=True)
speichern("jpg_444", g.convert("RGB"), "JPEG", quality=95, subsampling=0)
speichern("jpg_420", g.convert("RGB"), "JPEG", quality=90, subsampling=2)
speichern("jpg_grau", g.convert("L"), "JPEG", quality=90)
speichern("jpg_restart", g.convert("RGB"), "JPEG", quality=85, subsampling=2, restart_marker_blocks=2) if "restart_marker_blocks" in Image.SAVE.get("JPEG").__code__.co_varnames else None
# Progressive JPEGs (B0b): Spektralauswahl + schrittweise Näherung (libjpeg-Skript)
speichern("jpg_prog_444", g.convert("RGB"), "JPEG", quality=95, subsampling=0, progressive=True)
speichern("jpg_prog_420", g.convert("RGB"), "JPEG", quality=90, subsampling=2, progressive=True)
speichern("jpg_prog_grau", g.convert("L"), "JPEG", quality=90, progressive=True)
gross = grund(203, 151)
gross_d = ImageDraw.Draw(gross)
for i in range(0, 203, 9):
    gross_d.line((i, 0, 202 - i, 150), fill=(i, 255 - i, 90, 255), width=2)
speichern("jpg_prog_gross", gross.convert("RGB"), "JPEG", quality=80, subsampling=2, progressive=True, optimize=True)

# PNG mit Adam7-Interlace und 16-Bit-RGBA von Hand (Pillow schreibt kein Interlace)
import zlib


def png_roh(name, im, interlace, tiefe16):
    im = im.convert("RGBA")
    w, h = im.size
    px = im.load()
    def zeile(xs, y):
        out = bytearray([0])
        for x in xs:
            r, g_, b, a = px[x, y]
            if tiefe16:
                for v in (r, g_, b, a):
                    out += struct.pack(">H", v * 257)
            else:
                out += bytes((r, g_, b, a))
        return bytes(out)
    roh = b""
    if interlace:
        for x0, y0, dx, dy in [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]:
            xs = list(range(x0, w, dx))
            if not xs:
                continue
            for y in range(y0, h, dy):
                roh += zeile(xs, y)
    else:
        for y in range(h):
            roh += zeile(range(w), y)
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", w, h, 16 if tiefe16 else 8, 6, 0, 0, 1 if interlace else 0)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(roh, 9)) + chunk(b"IEND", b"")
    (ziel / f"{name}.png").write_bytes(data)
    ref(name, [Image.open(io.BytesIO(data))])


png_roh("png_adam7", g, True, False)
png_roh("png_rgba16", g, False, True)
print("ok", len(list(ziel.glob("*.rgba"))))
