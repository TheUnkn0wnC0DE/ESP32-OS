//! Hintergrundbild aus einem grossen Handyfoto im Heap der Einstellungen
//! (PC): derselbe Weg wie die Einstellungen (`bildbetrachter::laden`, dann
//! `konfig::hintergrund_vorbereiten`) in einem Programm mit genau dem Heap
//! aus `desktop::PROGRAMME` – passt ein 12-MP-JPEG hinein?

use kern::{KernAlloc, ProgrammEintrag};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[global_allocator]
static A: KernAlloc = KernAlloc;

static ERGEBNIS: OnceLock<Result<(u32, u32, bool), String>> = OnceLock::new();

fn programm() {
    let r = desktop::bildbetrachter::laden("/sd0/1/foto_12mp.jpg").map(|v| (v[0].w, v[0].h, desktop::konfig::hintergrund_vorbereiten(&v[0], 640, 360)));
    let _ = ERGEBNIS.set(r);
}

#[test]
fn hintergrund_aus_12mp_foto_im_heap_der_einstellungen() {
    let wurzel = std::env::temp_dir().join(format!("esp32os_hg_gross_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("settings")).unwrap();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::fenster::bildschirm(640, 360);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/sd0/1", Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gross"));
    let heap_kb = desktop::PROGRAMME.iter().find(|e| e.name == "einstellungen").expect("einstellungen").heap_kb;
    let e: &'static ProgrammEintrag = Box::leak(Box::new(ProgrammEintrag { name: "hg_test", heap_kb, stapel_kb: 48, start: programm }));
    kern::starte(e).expect("Start mit dem Heap der Einstellungen");
    let t = Instant::now();
    while ERGEBNIS.get().is_none() {
        assert!(t.elapsed() < Duration::from_secs(30), "Zeitüberschreitung");
        std::thread::sleep(Duration::from_millis(20));
    }
    let (w, h, gespeichert) = ERGEBNIS.get().unwrap().clone().expect("Foto geladen");
    eprintln!("Heap {heap_kb} KB: 12-MP-Foto als {w}x{h} geladen, Hintergrund gespeichert: {gespeichert}");
    // hochkant (EXIF 6), verkleinert, Hintergrund vorbereitet
    assert!(h > w && w >= 378, "{w}x{h}");
    assert!(gespeichert);
    assert!(desktop::konfig::hintergrund_lesen(640, 360).is_some());
    std::fs::remove_dir_all(&wurzel).ok();
}
