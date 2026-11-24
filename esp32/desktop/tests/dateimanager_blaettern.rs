//! S2: Blättern im Dateimanager zeichnet nur die Liste neu (Symbole aus dem
//! Zwischenspeicher, keine Fensterrahmen/Geräte/Leiste). Geprüft wird, dass
//! das Bild nach jedem Blätterschritt genau dem vollen Neuzeichnen an
//! derselben Stelle entspricht – auch das Teilbild aus den Dirty-Rects
//! (wie am Board) – und dass die Knöpfe der neuen Zeilen stimmen.

use desktop::dateimanager::{LEISTE, SEITE_W, ZEILE};
use desktop::gesten::Gesten;
use kern::{KernAlloc, sys};
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

const LOS: [Option<(i32, i32)>; 2] = [None, None];
#[allow(non_snake_case)]
fn AUF(x: i32, y: i32) -> [Option<(i32, i32)>; 2] {
    [Some((x, y)), None]
}

fn bis(mut f: impl FnMut() -> bool, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "Zeitüberschreitung: {was}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn folge(g: &mut Gesten, t0: &mut u64, schritte: &[(u64, [Option<(i32, i32)>; 2])]) {
    for &(dt, f) in schritte {
        *t0 += dt;
        g.schritt(*t0, f);
        std::thread::sleep(Duration::from_millis(5));
    }
    *t0 += 100;
    g.schritt(*t0, LOS);
    std::thread::sleep(Duration::from_millis(80));
}

fn titel() -> String {
    sys::fenster_liste().into_iter().find(|f| f.titel.starts_with("File Manager")).map(|f| f.titel).unwrap_or_default()
}

/// Bild, sobald es sich 150 ms nicht mehr ändert; das Teilbild aus den
/// Dirty-Rects muss dem Vollbild gleichen.
fn ruhiges_bild(s: &mut kern::sys::Schirm) -> Vec<u32> {
    let mut alt = Vec::new();
    let mut seit = Instant::now();
    let t = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(15));
        let i = s.aktualisieren();
        let (_, _, px) = sys::foto();
        if let Some(i) = i {
            let teil: Vec<u32> = s.puffer[i].px.iter().map(|&c| kern::grafik::von565(c)).collect();
            assert!(teil == px, "Teilbild (Dirty-Rects) weicht vom Vollbild ab");
        }
        if px != alt {
            alt = px;
            seit = Instant::now();
        } else if seit.elapsed() > Duration::from_millis(150) {
            return alt;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "Bild kommt nicht zur Ruhe");
    }
}

// Fenster (0, 0), Arbeitsbereich ab (5, 48)
const LX: i32 = 5 + SEITE_W + 4 + 100;
fn zeile(i: i32) -> i32 {
    48 + LEISTE + i * ZEILE + ZEILE / 2
}

#[test]
fn blaettern_zeichnet_nur_die_liste_und_gleicht_dem_vollbild() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_s2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    // gemischte Symbole: Ordner, Text, Bild, Musik, PDF
    for i in 0..6 {
        std::fs::create_dir_all(wurzel.join(format!("ordner_{i}"))).unwrap();
    }
    for i in 0..40 {
        let endung = ["txt", "png", "mp3", "pdf", "bmp"][i % 5];
        std::fs::write(wurzel.join(format!("datei_{i:02}.{endung}")), vec![b'x'; i * 100]).unwrap();
    }
    // Zielordner tief in der Liste (nach dem Blättern antippen)
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    kern::datei::einhaengen("/sd0/1", &wurzel);
    for p in ["@taskbar", "dateimanager"] {
        kern::starte_name(p).unwrap();
    }
    bis(|| titel() == "File Manager - /sd0/1", "Start auf der SD-Karte");
    let mut schirm = kern::sys::Schirm::default();
    let start = ruhiges_bild(&mut schirm);

    let mut g = Gesten::default();
    let mut t = 10_000u64;
    let mut bilder = vec![start];
    for runde in 0..4 {
        // Finger nach oben: Liste blättert nach unten
        let mut s = vec![(0, AUF(LX, 300))];
        s.extend((1..=6).map(|i| (20, AUF(LX, 300 - 25 * i))));
        folge(&mut g, &mut t, &s);
        let nach = ruhiges_bild(&mut schirm);
        assert!(bilder.iter().all(|b| *b != nach), "Runde {runde}: Blättern ändert das Bild nicht");
        // volles Neuzeichnen an derselben Stelle muss das gleiche Bild geben
        sys::alle_neu_zeichnen();
        let voll = ruhiges_bild(&mut schirm);
        if voll != nach {
            let (w, h, _) = sys::foto();
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
            desktop::bild_schreiben(&dir.join("s2_blaettern.ist.png"), &spatz::image::encode_png(w, h, &nach));
            desktop::bild_schreiben(&dir.join("s2_voll.ist.png"), &spatz::image::encode_png(w, h, &voll));
            panic!("Runde {runde}: Bild nach dem Blättern weicht vom vollen Neuzeichnen ab");
        }
        bilder.push(voll);
    }
    // Knöpfe der neu gezeichneten Zeilen: die oberste sichtbare Zeile ist
    // jetzt eine Datei – Antippen startet ein Programm dafür und wechselt
    // nicht in einen Ordner (die stehen am Listenanfang; ein alter Knopf
    // von dort würde den Ordner öffnen).
    let vorher: Vec<u32> = kern::liste().iter().map(|p| p.pid).collect();
    let mut f = Gesten::default();
    let mut tt = 900_000u64;
    folge(&mut f, &mut tt, &[(0, AUF(LX, zeile(0))), (60, LOS)]);
    bis(|| kern::liste().iter().any(|p| !vorher.contains(&p.pid)), "Programm für die Datei der obersten Zeile");
    assert_eq!(titel(), "File Manager - /sd0/1", "alter Knopf (Ordner vom Listenanfang) noch aktiv");
    for p in kern::liste().into_iter().filter(|p| !vorher.contains(&p.pid)) {
        kern::zwangsbeenden(p.pid, 2000);
    }
    bis(|| kern::liste().iter().all(|p| vorher.contains(&p.pid)), "Dateiprogramm beendet");
    // Dateimanager wieder nach vorn (oberstes Fenster bekommt die Gesten)
    sys::alle_neu_zeichnen();
    // zurück nach oben blättern und den ersten Ordner antippen
    for _ in 0..6 {
        let mut s = vec![(0, AUF(LX, 100))];
        s.extend((1..=6).map(|i| (20, AUF(LX, 100 + 25 * i))));
        folge(&mut f, &mut tt, &s);
    }
    let _ = ruhiges_bild(&mut schirm);
    folge(&mut f, &mut tt, &[(0, AUF(LX, zeile(0))), (60, LOS)]);
    bis(|| titel() == "File Manager - /sd0/1/ordner_0", "erster Ordner nach dem Zurückblättern");
    let _ = std::fs::remove_dir_all(&wurzel);
}
