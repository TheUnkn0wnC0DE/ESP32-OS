// ============================================================================
// Gehäuse „Pudding-Panik“ für das Waveshare ESP32-P4-WIFI6-Touch-LCD-5
// ============================================================================
// Teile (alle ohne Stützen druckbar):
//   SCHALE – Rahmen ums Display und dünne Griffe links/rechts. Die Platine
//            wird an ihren 4 Abstandshülsen festgeschraubt (Langlöcher).
//   BUCKEL – dicker Mittelteil hinten mit Akku und Lautsprecher; 4 Schrauben
//            M2, abnehmbar (Akku tauschen, ohne das Display auszubauen).
//   TASTER – 3 Stößel für RESET, BOOT und POWER.
//
// Koordinaten: Blick auf die RÜCKSEITE wie in der Maßzeichnung von Waveshare
// (docs.waveshare.com/ESP32-P4-WIFI6-Touch-LCD-5): x nach rechts (USB-Buchsen
// links), y nach oben (Tasten und SD-Karte oben), z = 0 auf der
// Glasvorderseite, wächst nach hinten.
//
// Ausgabe einzelner Teile (Druckrichtung schon richtig):
//   openscad -D 'TEIL="schale_druck"' -o schale.stl gehaeuse.scad
//   openscad -D 'TEIL="buckel_druck"' -o buckel.stl gehaeuse.scad
//   openscad -D 'TEIL="taster_druck"' -o taster.stl gehaeuse.scad
//   openscad -D 'TEIL="testrahmen"' -o testrahmen.stl gehaeuse.scad
// Kontrolle: TEIL="pruefung" zeigt alle Überschneidungen mit einer
// Nachbildung von Display, Platine, Akku und Lautsprecher – muss leer sein.
// ============================================================================

TEIL = "alles"; // [alles, schale, buckel, taster, schale_druck, buckel_druck, taster_druck, testrahmen, druckplatte, pruefung, platine]

/* [Display – Maßzeichnung Waveshare, passgenau] */
GLAS_B = 126.90;
GLAS_H = 70.70;
// Luft je Seite um das Glas
GLAS_SPIEL = 0.30;
// Eckenradius der Glastasche (bewusst kleiner als am Glas)
GLAS_ECKE = 3.0;
// so weit steht der Rahmen vor die Glasfläche
RAND_UEBER = 0.5;

/* [Platine – Maßzeichnung Waveshare] */
PCB_B = 118.50;
PCB_H = 64.50;
// Glaskante (USB-Seite) bis Platinenkante; auf der anderen Seite ~6,7 mm
PCB_RAND_LINKS = 1.72;
// Lochmitte der Abstandshülsen ab Platinenrand (Raster 112 × 57 mm)
LOCH_X = 3.25;
LOCH_Y = 3.75;

/* [Tiefe – aus Fotos geschätzt, nachmessen lohnt sich] */
// Glasvorderseite bis Rückseite der Platine
Z_PCB = 7.0;
// Glasvorderseite bis Ende der Abstandshülsen (hier liegt die Schale an)
Z_HUELSE = 11.0;
// höchstes Bauteil auf der Platinenrückseite (ohne 40-Pin-Leiste)
BAUTEIL_MAX = 3.5;
// Spitzen der 40-Pin-Leiste (liegen im Buckel)
Z_STIFTE = 15.5;

/* [Akku und Lautsprecher] */
// Breite, Länge, Dicke (gemessen)
AKKU = [29, 49, 19];
// Luft je Seite und in der Dicke
AKKU_SPIEL = 1.5;
// Mitte des Akkus (liegt längs, Kabel nach unten zum BAT-Stecker)
AKKU_POS = [8.5, 2.0];
// Lautsprecher (Kasten) Breite, Höhe, Dicke
LS = [26, 26, 6];
LS_SPIEL = 1.5;
// Mitte des Lautsprechers – nah am SPK-Stecker (Kabel nur 3–4 cm)
LS_POS = [-26, 18];

/* [Gehäuse] */
WAND = 3.0;
BODEN = 2.0;
// Außenkanten des Buckels in x (deckt die 40-Pin-Leiste und den SPK-Stecker)
BUCKEL_X = [-47.6, 49.6];
// Langlöcher für die Hülsen-Schrauben (M2.5; M3 passt auch)
LANGLOCH_B = 3.3;
LANGLOCH_SPIEL_X = 0.9;
// Tastenstößel: Bohrung, Schaft, Überstand außen
TASTER_LOCH = 4.6;
TASTER_D = 4.0;
TASTER_AUSSEN = 2.5;
SCHRIFT = "PUDDING-PANIK";
// nur für TEIL="pruefung": 0 = alle Paare, 1–5 = einzeln
PRUEF = 0;

$fn = 48;

// ---------------------------------------------------------------------------
// Abgeleitete Maße
// ---------------------------------------------------------------------------
GX = GLAS_B / 2;
GY = GLAS_H / 2;
IX = GX + GLAS_SPIEL;       // Innenmaß Glastasche
IY = GY + GLAS_SPIEL;
OX = IX + WAND;             // Außenmaß
OY = IY + WAND;
PCB_X0 = -GX + PCB_RAND_LINKS;
PCB_X1 = PCB_X0 + PCB_B;
PCB_Y = PCB_H / 2;
// Platinenkoordinaten (x ab USB-Kante, y ab Oberkante) → Gehäuse
function px(x) = PCB_X0 + x;
function py(y) = PCB_Y - y;

LOECHER = [for (x = [px(LOCH_X), PCB_X1 - LOCH_X]) for (s = [1, -1]) [x, s * (PCB_Y - LOCH_Y)]];

Z_RAND = -RAND_UEBER;
Z_BODEN = Z_HUELSE;                      // Innenseite der Griffe
Z_RUECK = Z_HUELSE + BODEN;              // Rückseite der Griffe
Z_AKKU_VORN = Z_PCB + BAUTEIL_MAX + 1.5;
Z_BUCKEL_INNEN = max(Z_AKKU_VORN + AKKU[2] + AKKU_SPIEL, Z_STIFTE + 3);
Z_BUCKEL = Z_BUCKEL_INNEN + BODEN;

BX0 = BUCKEL_X[0];
BX1 = BUCKEL_X[1];
OFF_X0 = BX0 + WAND;                     // Öffnung in der Schale
OFF_X1 = BX1 - WAND;
// Schrauben Buckel ↔ Schale: in der oberen/unteren Wand, neben der Platine
SCHRAUBEN = [for (x = [BX0 + 4.0, BX1 - 4.0]) for (s = [1, -1]) [x, s * (IY + 0.1)]];

// Bauteile am Platinenrand (aus der Maßzeichnung)
USB_Y = [py(23.27), py(23.27 + 17.95)];   // Mitten der zwei USB-C-Buchsen (linke Kante)
TASTEN_X = [px(118.5 - 22.37 - 2 * 11.68), px(118.5 - 22.37 - 11.68), px(118.5 - 22.37)]; // RESET, BOOT, POWER
TASTEN_Z = Z_PCB + 1.75;
SD_X = px(60.0);
LED_POS = [px(4.0), py(49.6)];            // PWR-LED

// ---------------------------------------------------------------------------
// Grundformen
// ---------------------------------------------------------------------------
module rrect(x0, x1, y0, y1, r, z0, z1) {
    translate([0, 0, z0]) linear_extrude(z1 - z0)
        offset(r = r) offset(delta = -r) translate([x0, y0]) square([x1 - x0, y1 - y0]);
}

// Abgerundeter Kasten mit 45°-Fasen unten (f0) und oben (f1).
module kasten(x0, x1, y0, y1, r, z0, z1, f0 = 0, f1 = 0) {
    hull() {
        rrect(x0 + f0, x1 - f0, y0 + f0, y1 - f0, max(r - f0, 0.5), z0, z0 + 0.01);
        rrect(x0, x1, y0, y1, r, z0 + f0, z1 - f1);
        rrect(x0 + f1, x1 - f1, y0 + f1, y1 - f1, max(r - f1, 0.5), z1 - 0.01, z1);
    }
}

// ---------------------------------------------------------------------------
// SCHALE
// ---------------------------------------------------------------------------
module schale() {
    difference() {
        union() {
            difference() {
                kasten(-OX, OX, -OY, OY, 4, Z_RAND, Z_RUECK, 0.8, 1.2);
                // Glastasche und Innenraum bis zu den Hülsen
                rrect(-IX, IX, -IY, IY, GLAS_ECKE, Z_RAND - 1, Z_BODEN);
                // Einführschräge am Rand
                hull() {
                    rrect(-IX - 0.6, IX + 0.6, -IY - 0.6, IY + 0.6, GLAS_ECKE + 0.6, Z_RAND - 0.01, Z_RAND);
                    rrect(-IX, IX, -IY, IY, GLAS_ECKE, Z_RAND + 0.6, Z_RAND + 0.61);
                }
                // Öffnung für den Buckel
                rrect(OFF_X0, OFF_X1, -IY, IY, 1, Z_BODEN - 1, Z_RUECK + 1);
            }
            // Schraubdome für den Buckel (an der Wand, neben der Platine)
            for (p = SCHRAUBEN) translate([p[0], p[1], Z_PCB - 1.0]) cylinder(d = 4.4, h = Z_RUECK - Z_PCB + 1.0);
        }
        // Kernlöcher M2 (selbstschneidend)
        for (p = SCHRAUBEN) translate([p[0], p[1], Z_PCB - 0.5]) cylinder(d = 1.7, h = 20, $fn = 16);
        // Langlöcher für die Abstandshülsen
        for (p = LOECHER) hull() for (dx = [-LANGLOCH_SPIEL_X, LANGLOCH_SPIEL_X])
            translate([p[0] + dx, p[1], Z_BODEN - 1]) cylinder(d = LANGLOCH_B, h = BODEN + 2, $fn = 24);
        // USB-C (linke Wand)
        for (y = USB_Y) translate([-OX - 1, y, Z_PCB + 1.6]) rotate([0, 90, 0])
            linear_extrude(WAND + 2.5) offset(r = 2) offset(delta = -2) square([8, 13], center = true);
        // Tasten RESET, BOOT, POWER (obere Wand)
        for (x = TASTEN_X) translate([x, IY - 1, TASTEN_Z]) rotate([-90, 0, 0])
            cylinder(d = TASTER_LOCH, h = WAND + 2, $fn = 32);
        // SD-Karte (obere Wand)
        translate([SD_X - 8, IY - 1, Z_PCB - 0.5]) cube([16, WAND + 2, 4.5]);
        // Lichtloch für die PWR-LED
        translate([LED_POS[0], LED_POS[1], Z_BODEN - 1]) cylinder(d = 3.5, h = BODEN + 2, $fn = 24);
    }
}

// ---------------------------------------------------------------------------
// BUCKEL
// ---------------------------------------------------------------------------
module rahmen(c, innen, h, wand = 1.6) {
    // Rechteckiger Rahmen um eine Aussparung `innen` (Breite, Höhe) bei `c`,
    // von der Innenseite der Rückwand `h` hoch.
    translate([0, 0, Z_BUCKEL_INNEN - h]) linear_extrude(h + 0.01) difference() {
        translate([c[0], c[1]]) square([innen[0] + 2 * wand, innen[1] + 2 * wand], center = true);
        translate([c[0], c[1]]) square([innen[0], innen[1]], center = true);
    }
}

module buckel() {
    akku_in = [AKKU[0] + 2 * AKKU_SPIEL, AKKU[1] + 2 * AKKU_SPIEL];
    ls_in = [LS[0] + 2 * LS_SPIEL, LS[1] + 2 * LS_SPIEL];
    difference() {
        union() {
            difference() {
                kasten(BX0, BX1, -OY, OY, 4, Z_RUECK, Z_BUCKEL, 0, 3);
                rrect(OFF_X0, OFF_X1, -IY, IY, 2, Z_RUECK - 1, Z_BUCKEL_INNEN);
            }
            // Säulen für die Schrauben
            for (p = SCHRAUBEN) translate([p[0], p[1], Z_RUECK]) cylinder(d = 5.4, h = Z_BUCKEL_INNEN - Z_RUECK + 0.01);
            // Akku-Rahmen (8 mm hoch) und Lautsprecher-Rahmen (5 mm)
            rahmen(AKKU_POS, akku_in, 8);
            rahmen(LS_POS, ls_in, 5);
        }
        // Schrauben: Durchgang M2 und tiefe Senkung für den Kopf von hinten
        for (p = SCHRAUBEN) {
            translate([p[0], p[1], Z_RUECK - 1]) cylinder(d = 2.3, h = 50, $fn = 16);
            translate([p[0], p[1], Z_RUECK + 3]) cylinder(d = 4.3, h = 50, $fn = 24);
        }
        // Kabeldurchlass Akku (unten links, zum BAT-Stecker)
        translate([AKKU_POS[0] - akku_in[0] / 2 - 0.5, AKKU_POS[1] - akku_in[1] / 2 - 2, Z_BUCKEL_INNEN - 9])
            cube([12, 3, 10]);
        // Kabeldurchlass Lautsprecher (oben, zum SPK-Stecker)
        translate([LS_POS[0] - 10, LS_POS[1] + ls_in[1] / 2 - 0.5, Z_BUCKEL_INNEN - 6]) cube([8, 3, 7]);
        // Lautsprechergitter
        for (i = [-5:5], j = [-5:5]) {
            x = i * 3.2 + (j % 2 == 0 ? 0 : 1.6);
            y = j * 2.8;
            if (x * x + y * y <= 100)
                translate([LS_POS[0] + x, LS_POS[1] + y, Z_BUCKEL_INNEN - 1]) cylinder(d = 2, h = BODEN + 2, $fn = 12);
        }
        // Schriftzug hinten (0,6 mm vertieft)
        translate([(BX0 + BX1) / 2, -29, Z_BUCKEL - 0.6]) linear_extrude(1)
            text(SCHRIFT, size = 5.5, font = "Liberation Sans:style=Bold", halign = "center", valign = "center");
    }
}

// ---------------------------------------------------------------------------
// TASTER (gedruckt mit dem Schaft nach unten)
// ---------------------------------------------------------------------------
TASTER_SCHAFT = WAND + TASTER_AUSSEN;
module taster() {
    cylinder(d = TASTER_D, h = TASTER_SCHAFT, $fn = 32);
    // Kragen innen (Unterseite als Kegel → ohne Stützen druckbar)
    translate([0, 0, TASTER_SCHAFT]) cylinder(d1 = TASTER_D, d2 = 6.5, h = 1.25, $fn = 32);
    translate([0, 0, TASTER_SCHAFT + 1.25]) cylinder(d = 6.5, h = 0.6, $fn = 32);
    // Nase zur Taste
    translate([0, 0, TASTER_SCHAFT + 1.85]) cylinder(d = 3.0, h = 0.3, $fn = 24);
}

// Eingebaut: Schaft durch die Wand nach außen, Kragen innen an der Wand.
module taster_eingebaut() {
    stopp = (TASTER_LOCH - TASTER_D) / 2 / ((6.5 - TASTER_D) / 2) * 1.25; // Kegel bis Lochrand
    for (x = TASTEN_X) translate([x, IY + stopp + TASTER_SCHAFT, TASTEN_Z]) rotate([90, 0, 0]) taster();
}

// ---------------------------------------------------------------------------
// Nachbildung von Display, Platine, Akku, Lautsprecher (nur zur Kontrolle)
// ---------------------------------------------------------------------------
module platine() {
    // Glas + Displaymodul
    rrect(-GX, GX, -GY, GY, 3.5, 0, Z_PCB - 1.7);
    // Platine
    translate([PCB_X0, -PCB_Y, Z_PCB - 1.6]) cube([PCB_B, PCB_H, 1.6]);
    // Bauteile allgemein
    translate([PCB_X0 + 1, -PCB_Y + 1, Z_PCB]) cube([PCB_B - 2, PCB_H - 2, BAUTEIL_MAX]);
    // Abstandshülsen
    for (p = LOECHER) translate([p[0], p[1], Z_PCB]) cylinder(d = 5.5, h = Z_HUELSE - Z_PCB - 0.01);
    // 40-Pin-Leiste
    translate([px(56.07) - 1.3, py(61.5) - 1.3, Z_PCB]) cube([19 * 2.54 + 2.6, 2.54 + 2.6, Z_STIFTE - Z_PCB]);
    // USB-C
    for (y = USB_Y) translate([PCB_X0 - 0.5, y - 4.5, Z_PCB]) cube([8, 9, 3.3]);
    // Tasten (seitlich, Stößel bis 0,75 mm über die Platinenkante)
    for (x = TASTEN_X) translate([x - 1.75, PCB_Y - 3.5, Z_PCB]) cube([3.5, 3.5 + 0.75, 3.5]);
    // SD-Halter
    translate([SD_X - 7.5, PCB_Y - 15, Z_PCB]) cube([15, 15, 2.2]);
    // Akku (liegt an der Rückwand des Buckels) und Lautsprecher
    translate([AKKU_POS[0] - AKKU[0] / 2, AKKU_POS[1] - AKKU[1] / 2, Z_BUCKEL_INNEN - AKKU[2] - 0.01]) cube(AKKU);
    translate([LS_POS[0] - LS[0] / 2, LS_POS[1] - LS[1] / 2, Z_BUCKEL_INNEN - LS[2] - 0.01]) cube(LS);
}

// ---------------------------------------------------------------------------
// Auswahl
// ---------------------------------------------------------------------------
module schale_druck() { translate([0, 0, Z_RUECK]) rotate([180, 0, 0]) schale(); }
module buckel_druck() { translate([0, 0, Z_BUCKEL]) rotate([180, 0, 0]) buckel(); }
module taster_druck() { for (i = [0:2]) translate([i * 10, 0, 0]) taster(); }
// Nur der vordere Rand der Schale (4,5 mm): in ~30 min gedruckt – prüft, ob
// das Glas passt, bevor die ganze Schale gedruckt wird.
module testrahmen() {
    translate([0, 0, -Z_RAND]) intersection() {
        schale();
        translate([-100, -100, Z_RAND]) cube([200, 200, 4.5]);
    }
}

if (TEIL == "schale") schale();
else if (TEIL == "buckel") buckel();
else if (TEIL == "taster") taster();
else if (TEIL == "schale_druck") schale_druck();
else if (TEIL == "buckel_druck") buckel_druck();
else if (TEIL == "taster_druck") taster_druck();
else if (TEIL == "testrahmen") testrahmen();
else if (TEIL == "platine") platine();
else if (TEIL == "druckplatte") {
    schale_druck();
    translate([0, -2 * OY - 6, 0]) buckel_druck();
    translate([BX1 + 8, -2 * OY - 6, 0]) taster_druck();
} else if (TEIL == "pruefung") {
    // PRUEF = 0: alles; 1–5: einzelne Paare (zur Fehlersuche)
    if (PRUEF == 0 || PRUEF == 1) intersection() { schale(); platine(); }
    if (PRUEF == 0 || PRUEF == 2) intersection() { buckel(); platine(); }
    if (PRUEF == 0 || PRUEF == 3) intersection() { schale(); buckel(); }
    if (PRUEF == 0 || PRUEF == 4) intersection() { taster_eingebaut(); platine(); }
    if (PRUEF == 0 || PRUEF == 5) intersection() { taster_eingebaut(); buckel(); }
} else {
    color("SteelBlue", 0.9) schale();
    color("Orange", 0.9) buckel();
    color("White") taster_eingebaut();
    color("DimGray", 0.5) platine();
}
