//! Z1-Nachweis (PC): PDF-Betrachter per Touch – Testdateien aus MuPDF
//! (Querverweis-Strom, Objektströme, Flate, JPEG, PNG mit Maske, gedrehte
//! Seite) und matplotlib (eingebettete TrueType-Schrift). Wischen hoch rollt,
//! seitwärts blättert, Knöpfe „<“ „>“ „+“ „−“, Zwei-Finger-Zoom; kaputte Datei
//! zeigt eine Meldung statt abzustürzen.

use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn titel(name: &str) -> String {
    sys::fenster_liste()
        .into_iter()
        .find(|f| f.titel.starts_with("PDF Viewer") && f.titel.contains(name))
        .map(|f| f.titel)
        .unwrap_or_default()
}

fn bis(mut f: impl FnMut() -> bool, was: &str, name: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "Zeitüberschreitung: {was}; Titel: {}", titel(name));
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn foto(name: &str) -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
    px
}

struct Finger {
    g: Gesten,
    t: u64,
}

impl Finger {
    fn schritt(&mut self, dt: u64, f: [Option<(i32, i32)>; 2]) {
        self.t += dt;
        self.g.schritt(self.t, f);
        std::thread::sleep(Duration::from_millis(8));
    }
    fn los(&mut self) {
        self.schritt(30, [None, None]);
        std::thread::sleep(Duration::from_millis(60));
        self.schritt(100, [None, None]);
        self.t += 600;
        std::thread::sleep(Duration::from_millis(400));
    }
    fn wischen(&mut self, x: i32, y: i32, dx: i32, dy: i32) {
        self.schritt(0, [Some((x, y)), None]);
        for i in 1..=10 {
            self.schritt(25, [Some((x + dx * i / 10, y + dy * i / 10)), None]);
        }
        self.los();
    }
    fn tippen(&mut self, x: i32, y: i32) {
        self.schritt(0, [Some((x, y)), None]);
        self.schritt(80, [Some((x, y)), None]);
        self.los();
    }
    fn zoom(&mut self, x: i32, y: i32, a: i32, b: i32) {
        for i in 0..=10 {
            let d = (a + (b - a) * i / 10) / 2;
            self.schritt(30, [Some((x - d, y)), Some((x + d, y))]);
        }
        self.los();
    }
}

/// Anteil der Bildpunkte im Rechteck, die `passt` erfüllen.
fn anteil(px: &[u32], (x0, y0, x1, y1): (usize, usize, usize, usize), passt: impl Fn(u32, u32, u32) -> bool) -> f32 {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = px[y * 640 + x];
            if passt(p >> 16 & 255, p >> 8 & 255, p & 255) {
                n += 1;
            }
        }
    }
    n as f32 / ((x1 - x0) * (y1 - y0)) as f32
}

#[test]
fn pdf_per_touch() {
    let pdf = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pdf");
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sd0/1", &pdf);
    kern::starte_name("@taskbar").unwrap();
    assert_eq!(desktop::dateimanager::programm_fuer("Bericht.PDF"), Some("pdfbetrachter"));

    // Seite 1 in Seitenbreite (566 px für 595 pt): Überschrift, rote Fläche
    assert!(kern::datei::starten("/sys/pdfbetrachter", "/sd0/1/bericht.pdf") > 0);
    bis(|| titel("bericht").contains("bericht.pdf (1/3, 100%)"), "Seite 1", "bericht");
    let px = foto("z1_pdf_seite1");
    let s = 566.0 / 595.0;
    let bx = |x: f32| (5.0 + x * s) as usize;
    let by = |y: f32| (48.0 + y * s) as usize;
    assert!(anteil(&px, (bx(70.0), by(230.0), bx(250.0), by(270.0)), |r, g, b| r > 200 && g < 60 && b < 60) > 0.95, "rote Fläche");
    // Überschrift (dunkelblau, 28 pt) und Absatz (schwarz) sind gezeichnet
    assert!(anteil(&px, (bx(60.0), by(66.0), bx(330.0), by(92.0)), |r, g, b| b > r + 40 && r < 120 && g < 140) > 0.08, "Überschrift");
    assert!(anteil(&px, (bx(60.0), by(112.0), bx(530.0), by(160.0)), |r, g, b| r < 90 && g < 90 && b < 90) > 0.015, "Absatz");

    // Wischen nach oben rollt die Seite (Kreis, Linie, Bilder kommen)
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    let (mx, my) = (280, 200);
    f.wischen(mx, my + 80, 0, -160);
    std::thread::sleep(Duration::from_millis(300));
    let gerollt = foto("z1_pdf_gerollt");
    assert!(titel("bericht").contains("(1/3, 100%)"), "Rollen bleibt auf Seite 1: {}", titel("bericht"));
    assert_ne!(gerollt, px, "Ausschnitt hat sich bewegt");
    f.wischen(mx, my + 80, 0, -160);
    f.wischen(mx, my + 80, 0, -160);
    std::thread::sleep(Duration::from_millis(300));
    let unten = foto("z1_pdf_bilder");
    // JPEG-Farbfelder (rot, grün, blau oben links im Foto) irgendwo sichtbar
    assert!(anteil(&unten, (5, 48, 300, 330), |r, g, b| r > 180 && g < 80 && b < 80) > 0.002, "JPEG rot");
    assert!(anteil(&unten, (5, 48, 300, 330), |r, g, b| b > 180 && r < 80 && g < 100) > 0.002, "JPEG blau");

    // Seitwärts wischen = nächste Seite (Tabelle)
    f.wischen(mx + 120, my, -240, 0);
    bis(|| titel("bericht").contains("(2/3, 100%)"), "Seite 2 per Wischen", "bericht");
    foto("z1_pdf_seite2");

    // Knopf „>“ = Seite 3 (quer: gedreht, Seitenbreite = 842 pt)
    let kx = 5 + 566 + 4 + 28;
    f.tippen(kx, 48 + 58 + 24);
    bis(|| titel("bericht").contains("(3/3, 100%)"), "Seite 3 per Knopf", "bericht");
    let quer = foto("z1_pdf_quer");
    // gelbe Fläche der quer gedrehten Seite
    assert!(anteil(&quer, (5, 48, 571, 324), |r, g, b| r > 230 && g > 190 && b < 90) > 0.10, "gelbe Fläche quer");

    // Knöpfe „+“ / „−“ ändern den Massstab, Zwei-Finger-Zoom auch
    f.tippen(kx, 48 + 112 + 24);
    bis(|| titel("bericht").contains("(3/3, 125%)"), "Zoom +", "bericht");
    f.tippen(kx, 48 + 166 + 24);
    bis(|| titel("bericht").contains("(3/3, 100%)"), "Zoom −", "bericht");
    f.zoom(mx, my, 80, 240);
    bis(|| !titel("bericht").contains("100%)"), "Zwei-Finger-Zoom", "bericht");
    foto("z1_pdf_gezoomt");

    // Knopf „<“ zurück auf Seite 2
    f.tippen(kx, 48 + 4 + 24);
    bis(|| titel("bericht").contains("(2/3,"), "Knopf zurück", "bericht");

    // matplotlib-Diagramm: Balken in den Farben aus der Datei
    assert!(kern::datei::starten("/sys/pdfbetrachter", "/sd0/1/diagramm.pdf") > 0);
    bis(|| titel("diagramm").contains("diagramm.pdf (1/1, 100%)"), "Diagramm", "diagramm");
    let d = foto("z1_pdf_diagramm");
    let ganz = (5, 48, 571, 324);
    assert!(anteil(&d, ganz, |r, g, b| r > 190 && g < 80 && b < 80) > 0.003, "roter Balken");
    assert!(anteil(&d, ganz, |r, g, b| r < 90 && g > 140 && b < 90) > 0.003, "grüner Balken");

    // Kaputte Datei: Meldung im Fenster, kein Absturz
    assert!(kern::datei::starten("/sys/pdfbetrachter", "/sd0/1/kaputt.pdf") > 0);
    bis(|| titel("kaputt").ends_with("kaputt.pdf"), "kaputte Datei", "kaputt");
    std::thread::sleep(Duration::from_millis(300));
    assert!(sys::fenster_liste().iter().any(|f| f.titel.contains("kaputt.pdf")), "Fenster bleibt offen");
    foto("z1_pdf_kaputt");

    // Datei über 4 MB: Meldung statt Speicher voll
    let tmp = std::env::temp_dir().join("esp32os_z1_test");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut gross = std::fs::read(pdf.join("bericht.pdf")).unwrap();
    gross.extend(std::iter::repeat_n(b'%', 4 << 20));
    std::fs::write(tmp.join("gross.pdf"), &gross).unwrap();
    kern::datei::einhaengen("/rd/1", &tmp);
    assert!(kern::datei::starten("/sys/pdfbetrachter", "/rd/1/gross.pdf") > 0);
    bis(|| titel("gross").ends_with("gross.pdf"), "zu grosse Datei", "gross");
    std::thread::sleep(Duration::from_millis(300));
    let g = foto("z1_pdf_zu_gross");
    // Meldungstext (hell) mitten im grauen Bereich
    assert!(anteil(&g, (100, 170, 470, 200), |r, g, b| r > 200 && g > 200 && b > 200) > 0.02, "Meldung sichtbar");
    std::fs::remove_dir_all(&tmp).ok();
}
