//! Z3-Nachweis (PC, 32 Bit wie das Board): TinyHashView (KolibriOS
//! `programs/other/TinyHashView`, unverändert) mit libcryptal – MD5, SHA-1
//! und SHA-256 einer Datei per Touch, Kopieren, Einfügen und Vergleichen;
//! Meldungen über `@notify`. Sollwerte aus Python hashlib.
#![cfg(target_pointer_width = "32")]

use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

const MD5: &str = "1025fb3873cfc6c1845c92ed8993d9b5";
const SHA1: &str = "8cc40a0094991c197101264a32e6775b11c6dce5";
const SHA256: &str = "64b7e2e3bef0e587fac4705f5b78ad0aeabea7822852f6f13ef9efac2243aa55";

fn titel_da(anfang: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel.starts_with(anfang))
}

fn bis(mut f: impl FnMut() -> bool, sekunden: u64, was: &str) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(sekunden), "Zeitüberschreitung: {was}; Fenster: {:?}", sys::fenster_liste().iter().map(|f| f.titel.clone()).collect::<Vec<_>>());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn foto(name: &str) {
    std::thread::sleep(Duration::from_millis(250));
    let (w, h, px) = sys::foto();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erwartet");
    desktop::bild_schreiben(&dir.join(format!("{name}.png")), &spatz::image::encode_png(w, h, &px));
}

fn tippen(x: i32, y: i32) {
    sys::eingabe::maus(x, y, 0);
    std::thread::sleep(Duration::from_millis(60));
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(60));
    sys::eingabe::maus(x, y, 0);
    std::thread::sleep(Duration::from_millis(150));
}

/// Meldung mit `text` abwarten und per Antippen schliessen.
fn meldung(text: &str) {
    bis(|| titel_da(&format!("@notify: {text}")), 5, text);
    tippen(640 - 8 - 150, 30);
    bis(|| !titel_da("@notify"), 3, "Meldung zu");
}

#[test]
fn pruefsummen_per_touch() {
    let ordner = std::env::temp_dir().join(format!("esp32os_z3_hash_{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let daten: Vec<u8> = (0..5000u32).map(|i| ((i * 7 + i / 13) % 256) as u8).collect();
    std::fs::write(ordner.join("probe.bin"), &daten).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::datei::einhaengen("/rd/1", &ordner);
    // Datei als Parameter (wie aus dem Dateimanager); Fenster an der
    // Mausposition (0, 0), 665 px breit → auf 640 begrenzt
    assert!(kern::datei::starten("/sys/thashview", "/rd/1/probe.bin") > 0);
    bis(|| titel_da("/rd/1/probe.bin - thashview"), 5, "Fenster");
    // Das Programm warnt selbst, dass 640 px zu schmal sind (-W)
    meldung("Low screen resolution!");
    foto("z3_thashview_start");

    // Knöpfe MD5/SHA1/SHA256 (10,30/60/90 60×20), „Copy“ ab x 610
    for ((y, soll), name) in [(40, MD5), (70, SHA1), (100, SHA256)].into_iter().zip(["MD5", "SHA1", "SHA256"]) {
        tippen(40, y);
        tippen(625, y);
        bis(|| sys::ablage_text().as_deref() == Some(soll), 5, name);
        meldung("Copied to clipboard!");
    }
    foto("z3_thashview_summen");

    // „Paste“ (540,120) holt SHA-256 aus der Zwischenablage, „Compare“
    // (592,120) vergleicht → Meldung „stimmt“
    tippen(560, 130);
    meldung("Pasted from clipboard!");
    tippen(615, 130);
    bis(|| titel_da("@notify: The checksum matches"), 5, "Vergleich stimmt");
    foto("z3_thashview_vergleich");
    std::fs::remove_dir_all(&ordner).ok();
}
