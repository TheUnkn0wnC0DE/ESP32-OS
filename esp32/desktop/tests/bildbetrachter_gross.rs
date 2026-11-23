//! Bildbetrachter mit grossen Fotos (PC): 12-MP-JPEG wie aus einer
//! Handykamera (4032×3024, EXIF „90° drehen“) und ein 7,2-MP-PNG werden
//! nicht abgelehnt, sondern beim Dekodieren verkleinert; der Titel nennt
//! die Masse der Datei.

use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use kolibri::dekoder::Rgba;
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
        assert!(t.elapsed() < Duration::from_secs(30), "Zeitüberschreitung: {was}; Titel: {}", titel());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn foto(name: &str) -> Vec<u32> {
    std::thread::sleep(Duration::from_millis(300));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
    px
}

#[test]
fn grosse_fotos_verkleinert() {
    let ordner = std::env::temp_dir().join(format!("esp32os_bv_gross_{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gross/foto_12mp.jpg"), ordner.join("a_foto_12mp.jpg")).unwrap();
    // 3000×2400 PNG (7,2 MP, 29 MB roh): Verlauf mit hellem Rechteck
    let (w, h) = (3000u32, 2400u32);
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let innen = (1000..2000).contains(&x) && (800..1600).contains(&y);
            px.push(if innen { 0xFFF0_F0F0 } else { 0xFF00_0000 | (x * 255 / w) << 16 | (y * 255 / h) << 8 | 0x40 });
        }
    }
    std::fs::write(ordner.join("b_gross.png"), kolibri::kodierer::png(&Rgba { w, h, px, delay: 0 })).unwrap();

    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sd0/1", &ordner);
    kern::starte_name("@taskbar").unwrap();
    assert!(kern::datei::starten("/sys/bildbetrachter", "/sd0/1/a_foto_12mp.jpg") > 0);
    // hochkant (EXIF 6), Masse der Datei im Titel
    bis(|| titel().contains("a_foto_12mp.jpg (1/2, 3024x4032,"), "12-MP-JPEG");
    let bild = foto("p2_gross_jpeg");
    // Bildfläche zeigt das Foto (rot gefülltes Rechteck oben rechts nach dem
    // Drehen), keine Fehlermeldung
    let rot = bild.iter().filter(|&&c| c >> 16 & 255 > 180 && c >> 8 & 255 < 80 && c & 255 < 80).count();
    assert!(rot > 300, "rote Fläche sichtbar ({rot} Punkte); Titel {}", titel());

    // Wischen → PNG
    let mut g = Gesten::default();
    let mut t = 1000u64;
    for i in 0..=10 {
        g.schritt(t, [Some((400 - i * 24, 180)), None]);
        t += 25;
        std::thread::sleep(Duration::from_millis(8));
    }
    g.schritt(t + 30, [None, None]);
    g.schritt(t + 130, [None, None]);
    bis(|| titel().contains("b_gross.png (2/2, 3000x2400,"), "7,2-MP-PNG");
    let bild = foto("p2_gross_png");
    let hell = bild.iter().filter(|&&c| c >> 16 & 255 > 220 && c >> 8 & 255 > 220 && c & 255 > 220).count();
    assert!(hell > 2000, "helle Fläche sichtbar ({hell} Punkte)");
    std::fs::remove_dir_all(&ordner).ok();
}
