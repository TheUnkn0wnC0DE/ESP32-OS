//! Z3-Nachweis (PC, 32 Bit wie das Board): weitere KolibriOS-C-Programme
//! (unverändert aus dem KolibriOS-Baum) per Touch, mit dem
//! Benachrichtigungsdienst `@notify` (wie notify3), den sie aufrufen.
#![cfg(target_pointer_width = "32")]

use kern::{KernAlloc, sys};
use std::path::Path;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn fenster_da(titel: &str) -> bool {
    sys::fenster_liste().iter().any(|f| f.titel.starts_with(titel))
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
    std::thread::sleep(Duration::from_millis(120));
}

#[test]
fn passwordgen_und_notify_per_touch() {
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    // Fenster öffnet an der Mausposition (0, 0); Stil 0x14: Koordinaten
    // relativ zum Fenster, wie in passwordgen.c
    kern::starte_name("passwordgen").unwrap();
    bis(|| fenster_da("PasswordGen"), 5, "Fenster");
    foto("z3_passwordgen_start");

    // „Generate!“ (170,136 100×30), dann „Copy“ (310,136 60×30)
    tippen(220, 151);
    tippen(340, 151);
    bis(|| sys::ablage_text().is_some_and(|t| t.chars().count() == 10), 5, "Passwort (Länge 10) in der Zwischenablage");
    let pw = sys::ablage_text().unwrap_or_default();
    assert!(pw.chars().all(|c| c.is_ascii_alphanumeric()), "nur a-z, A-Z, 0-9: {pw}");
    // Meldung „Copied to clipboard!“ (-I) oben rechts, schliesst sich selbst
    bis(|| fenster_da("@notify"), 5, "Meldung nach Kopieren");
    foto("z3_passwordgen_kopiert");
    bis(|| !fenster_da("@notify"), 8, "Meldung schliesst sich nach 4 s");

    // Zeichensätze a-z, A-Z, 0-9 abwählen → Fehlermeldung (-E)
    for x in [117, 177, 237] {
        tippen(x, 75);
    }
    foto("z3_passwordgen_abgewaehlt");
    tippen(220, 151);
    bis(|| fenster_da("@notify"), 5, "Fehlermeldung");
    foto("z3_passwordgen_fehler");
    // Antippen schliesst die Meldung
    tippen(640 - 8 - 150, 30);
    bis(|| !fenster_da("@notify"), 3, "Meldung per Antippen zu");

    // nur Sonderzeichen → Passwort aus !@#$%^&*_
    tippen(297, 75);
    tippen(220, 151);
    tippen(340, 151);
    bis(|| sys::ablage_text().is_some_and(|t| t.chars().count() == 10 && t.chars().all(|c| "!@#$%^&*_".contains(c))), 5, "Sonderzeichen-Passwort");
    foto("z3_passwordgen_sonderzeichen");
}
