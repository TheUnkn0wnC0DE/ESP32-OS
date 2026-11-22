"""Testbilder für den Bildbetrachter (P2): je Format ein Bild, dazu dasselbe
Foto als progressives JPEG (Prüfung Hardware-JPEG, Auftrag 12.4).

    python betrachter_bilder.py <zielordner>

Klein gehalten: Sie liegen im Repo (PC-Tests) und kommen in den
Entwicklungs-Build der Firmware (ESP32OS_TESTBILDER=1).
"""
import math
import sys
from pathlib import Path
from PIL import Image, ImageDraw

ziel = Path(sys.argv[1])
ziel.mkdir(parents=True, exist_ok=True)


def foto(w, h):
    """Weicher Himmel, Sonne, Hügel – komprimiert gut, zeigt Farbfehler."""
    im = Image.new("RGB", (w, h))
    px = im.load()
    for y in range(h):
        for x in range(w):
            t = y / h
            px[x, y] = (int(40 + 120 * t), int(110 + 90 * t), int(230 - 60 * t))
    d = ImageDraw.Draw(im)
    d.ellipse((w * 0.68, h * 0.12, w * 0.86, h * 0.36), fill=(255, 215, 60))
    for i in range(3):
        y0 = h * (0.62 + 0.08 * i)
        pts = [(x, y0 - 30 * math.sin(x / w * math.pi * (2 + i)) - 10 * i) for x in range(0, w + 8, 8)]
        d.polygon(pts + [(w, h), (0, h)], fill=(40 + 30 * i, 140 - 20 * i, 50 + 10 * i))
    # Farbfelder oben links (Prüfung der Farbreihenfolge): rot, grün, blau
    for i, c in enumerate([(230, 20, 20), (20, 200, 40), (30, 50, 230)]):
        d.rectangle((16 + i * 56, 16, 16 + i * 56 + 47, 63), fill=c)
    return im


f = foto(640, 480)
f.save(ziel / "foto.jpg", quality=85, subsampling=2)
f.save(ziel / "progressiv.jpg", quality=85, subsampling=2, progressive=True)

png = Image.new("RGBA", (240, 180), (0, 0, 0, 0))
d = ImageDraw.Draw(png)
for r in range(80, 0, -4):
    a = int(255 * (1 - r / 90))
    d.ellipse((120 - r, 90 - r, 120 + r, 90 + r), fill=(200, 60, 160, a))
d.rectangle((20, 20, 80, 60), fill=(30, 160, 220, 255))
png.save(ziel / "transparent.png")

frames = []
for i, c in enumerate([(220, 40, 40), (40, 180, 60), (40, 80, 220)]):
    im = Image.new("RGB", (120, 90), (240, 240, 240))
    ImageDraw.Draw(im).rectangle((10 + i * 30, 20, 40 + i * 30, 70), fill=c)
    frames.append(im.convert("P", palette=Image.ADAPTIVE, colors=16))
frames[0].save(ziel / "animation.gif", save_all=True, append_images=frames[1:], duration=400, loop=0)

bmp = Image.new("RGB", (128, 96))
d = ImageDraw.Draw(bmp)
for i, c in enumerate([(255, 255, 255), (255, 255, 0), (0, 255, 255), (0, 255, 0), (255, 0, 255), (255, 0, 0), (0, 0, 255), (0, 0, 0)]):
    d.rectangle((i * 16, 0, i * 16 + 15, 95), fill=c)
bmp.save(ziel / "farben.bmp")

for p in sorted(ziel.iterdir()):
    print(f"{p.name}: {p.stat().st_size} Bytes")
