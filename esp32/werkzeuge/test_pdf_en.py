"""Erzeugt ein kleines englisches Test-PDF (eigener Inhalt, frei) für die
Pressefotos des PDF-Betrachters: desktop/tests/pdf/quickstart.pdf.

    python esp32/werkzeuge/test_pdf_en.py
"""
import os

ZIEL = os.path.join(os.path.dirname(__file__), "..", "desktop", "tests", "pdf", "quickstart.pdf")

ZEILEN = [
    (24, "ESP32-OS Quick Start"),
    (12, ""),
    (12, "ESP32-OS is a small desktop operating system for the ESP32-P4."),
    (12, "It runs on a 5-inch touch screen with windows, a taskbar,"),
    (12, "a Start menu and an on-screen keyboard."),
    (12, ""),
    (14, "First steps"),
    (12, "1. Tap the Start button and open Files."),
    (12, "2. Connect to Wi-Fi in Settings."),
    (12, "3. Open the browser, the weather app or the music player."),
    (12, "4. Swipe left or right to turn pages in this viewer."),
]


def text_strom():
    teile = ["BT", "/F1 24 Tf", "72 760 Td"]
    vorher = 24
    for i, (groesse, zeile) in enumerate(ZEILEN):
        if i:
            teile.append(f"0 -{max(vorher, groesse) + 8} Td")
        teile.append(f"/F1 {groesse} Tf")
        zeile = zeile.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
        teile.append(f"({zeile}) Tj")
        vorher = groesse
    teile.append("ET")
    # Farbflächen wie in bericht.pdf (Zeichnen prüfen)
    teile += ["0.23 0.48 0.75 rg", "72 300 200 120 re f", "0 0.5 0 RG 4 w 0.8 1 0.8 rg", "320 300 200 120 re B"]
    return "\n".join(teile).encode("latin-1")


def main():
    strom = text_strom()
    objekte = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length %d >>\nstream\n" % len(strom) + strom + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    aus = bytearray(b"%PDF-1.4\n")
    lage = []
    for i, o in enumerate(objekte, 1):
        lage.append(len(aus))
        aus += b"%d 0 obj\n" % i + o + b"\nendobj\n"
    xref = len(aus)
    aus += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objekte) + 1)
    for p in lage:
        aus += b"%010d 00000 n \n" % p
    aus += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objekte) + 1, xref)
    with open(ZIEL, "wb") as f:
        f.write(aus)
    print(f"{ZIEL}: {len(aus)} Byte")


if __name__ == "__main__":
    main()
