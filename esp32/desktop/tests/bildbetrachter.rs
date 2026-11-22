//! P2-Nachweis (PC): Bildbetrachter per Touch – je Format ein Foto (BMP,
//! JPEG, PNG mit Transparenz, animiertes GIF), Wischen = nächstes Bild,
//! Zwei-Finger-Zoom, vergrössert verschiebt Wischen den Ausschnitt, Knöpfe.
//! Progressives JPEG (B0b): Software-Dekoder zeigt es wie das Baseline-Bild.

use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn titel() -> String {
    sys::fenster_liste().into_iter().find(|f| f.titel.starts_with("Image Viewer")).map(|f| f.titel).unwrap_or_default()
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(8), "Zeitüberschreitung: {was}; Titel: {}", titel());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn foto(name: &str) -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(250));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    std::fs::write(dir.join(format!("{name}.png")), spatz::image::encode_png(w, h, &px)).unwrap();
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
        std::thread::sleep(Duration::from_millis(250));
    }
    /// Wischen waagrecht um `dx` (links negativ).
    fn wischen(&mut self, x: i32, y: i32, dx: i32) {
        self.schritt(0, [Some((x, y)), None]);
        for i in 1..=10 {
            self.schritt(25, [Some((x + dx * i / 10, y)), None]);
        }
        self.los();
    }
    fn tippen(&mut self, x: i32, y: i32) {
        self.schritt(0, [Some((x, y)), None]);
        self.schritt(80, [Some((x, y)), None]);
        self.los();
    }
    /// Zwei Finger um (x, y) von Abstand `a` auf `b`.
    fn zoom(&mut self, x: i32, y: i32, a: i32, b: i32) {
        for i in 0..=10 {
            let d = (a + (b - a) * i / 10) / 2;
            self.schritt(30, [Some((x - d, y)), Some((x + d, y))]);
        }
        self.los();
    }
}

#[test]
fn bildbetrachter_per_touch() {
    let bilder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bilder");
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sd0/1", &bilder);
    kern::starte_name("@taskbar").unwrap();
    // Ordner: animation.gif, farben.bmp, foto.jpg, progressiv.jpg, transparent.png
    assert!(kern::datei::starten("/sys/bildbetrachter", "/sd0/1/farben.bmp") > 0);
    bis(|| titel().contains("farben.bmp (2/5, 128x96"), "BMP");
    foto("p2_bmp");
    let mut f = Finger { g: Gesten::default(), t: 1000 };
    let (mx, my) = (280, 180);

    // Wischen nach links = nächstes Bild
    f.wischen(mx + 120, my, -240);
    bis(|| titel().contains("foto.jpg (3/5, 640x480"), "JPEG nach Wischen");
    let px = foto("p2_jpeg");
    // Farbfelder oben links (rot, grün, blau) richtig herum
    let (aw, ah) = (640 - 10 - desktop::bildbetrachter::SPALTE, 360 - desktop::shell::LEISTE_H - 48 - 5);
    let s = (aw as f32 / 640.0).min(ah as f32 / 480.0);
    let ox = 5.0 + (aw as f32 - 640.0 * s) / 2.0;
    let feld = |i: usize| {
        let (x, y) = ((ox + (16.0 + i as f32 * 56.0 + 24.0) * s) as usize, (48.0 + 40.0 * s) as usize);
        px[y * 640 + x]
    };
    let (r, g, b) = (feld(0), feld(1), feld(2));
    assert!(r >> 16 & 255 > 180 && r & 255 < 80, "rot {r:06x}");
    assert!(g >> 8 & 255 > 150 && g >> 16 & 255 < 80, "grün {g:06x}");
    assert!(b & 255 > 180 && b >> 16 & 255 < 80, "blau {b:06x}");

    // Zoomen (zwei Finger auseinander), dann verschiebt Wischen nur den Ausschnitt
    f.zoom(mx, my, 80, 260);
    bis(|| !titel().contains(&format!("{}%)", (s * 100.0).round())), "gezoomt");
    let gezoomt = titel();
    foto("p2_zoom");
    f.wischen(mx + 100, my, -200);
    std::thread::sleep(Duration::from_millis(200));
    assert!(titel().contains("foto.jpg"), "beim Zoomen kein Bildwechsel: {}", titel());
    assert_eq!(titel(), gezoomt, "Massstab bleibt beim Verschieben");
    foto("p2_zoom_verschoben");
    // Knopf „Fit“ → wieder ganzes Bild
    f.tippen(5 + aw + 4 + 28, 48 + 112 + 24);
    bis(|| titel().contains(&format!("{}%)", (s * 100.0).round())), "Fit");

    // Progressives JPEG (B0b): Software-Dekoder zeigt es (gleiches Motiv
    // wie foto.jpg); Farbfeld rot oben links wie beim Baseline-Bild
    f.wischen(mx + 120, my, -240);
    bis(|| titel().contains("progressiv.jpg (4/5, 640x480"), "progressiv");
    foto("p2_progressiv_pc");

    // PNG mit Transparenz (Schachbrett darunter)
    f.wischen(mx + 120, my, -240);
    bis(|| titel().contains("transparent.png (5/5, 240x180"), "PNG");
    foto("p2_png");

    // weiter → von vorn: animiertes GIF, Einzelbilder wechseln
    f.wischen(mx + 120, my, -240);
    bis(|| titel().contains("animation.gif (1/5, 120x90"), "GIF");
    let a = foto("p2_gif");
    std::thread::sleep(Duration::from_millis(500));
    let b = foto("p2_gif_2");
    assert_ne!(a, b, "GIF animiert");

    // Wischen nach rechts = voriges Bild (zurück zum PNG), Knopf „>“ = weiter
    f.wischen(mx - 120, my, 240);
    bis(|| titel().contains("transparent.png"), "zurück");
    f.tippen(5 + aw + 4 + 28, 48 + 58 + 24);
    bis(|| titel().contains("animation.gif"), "Knopf weiter");
}
