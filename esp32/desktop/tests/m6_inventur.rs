//! M6 (Messung, nur bei Bedarf: `M6_INVENTUR=1`): Was belegt der Desktop im
//! System-Heap nach dem Start? Spur ab `kern::init`, Desktop starten, Menü
//! einmal auf und zu, dann die noch belegten Blöcke ab 4 KB nach Grösse
//! (grösste Summe zuerst). Ohne Aufrufstapel: deren Auflösung im Allokator
//! verklemmt sich beim Start mehrerer Fäden; die grossen Puffer erkennt man
//! an ihrer Grösse (z. B. 640×360×2 = 460 800 B).
use kern::{KernAlloc, sys};
use std::collections::HashMap;
use std::time::Duration;

#[global_allocator]
static A: KernAlloc = KernAlloc;

fn tippen(x: i32, y: i32) {
    sys::eingabe::maus(x, y, 1);
    std::thread::sleep(Duration::from_millis(50));
    sys::eingabe::maus(x, y, 0);
    std::thread::sleep(Duration::from_millis(400));
}

#[test]
fn system_heap_inventur() {
    if std::env::var_os("M6_INVENTUR").is_none() {
        return;
    }
    let wurzel = std::env::temp_dir().join(format!("esp32os_m6_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(&wurzel).unwrap();
    kern::SPUR_AB_GROESSE.store(usize::MAX, std::sync::atomic::Ordering::Relaxed);
    kern::spur_starten();
    kern::init(&kern::PC, desktop::PROGRAMME);
    kern::TEST_OHNE_ABWICKELN.store(true, std::sync::atomic::Ordering::Relaxed);
    kern::datei::einhaengen("/sys", &wurzel);
    kern::datei::einhaengen("/rd/1", &wurzel);
    kern::fenster::bildschirm(640, 360);
    kern::skin::anwenden(kern::skin::STANDARD, 2).unwrap();
    let anwendungen = desktop::anwendungen();
    desktop::shell::anwendungen_setzen(&anwendungen);
    kern::starte_name("@start").unwrap();
    std::thread::sleep(Duration::from_millis(3000));
    tippen(40, 340);
    std::thread::sleep(Duration::from_millis(800));
    tippen(40, 340);
    for _ in 0..4 {
        kern::aufraeumen();
        std::thread::sleep(Duration::from_millis(100));
    }
    let offen = kern::spur_offen();
    let summe: usize = offen.iter().map(|(n, _)| n).sum();
    let mut groessen: HashMap<usize, usize> = HashMap::new();
    for (n, _) in &offen {
        *groessen.entry(if *n >= 4096 { *n } else { 0 }).or_default() += 1;
    }
    let mut g: Vec<_> = groessen.into_iter().collect();
    g.sort_by_key(|(n, k)| std::cmp::Reverse(n * k));
    println!("System-Heap nach dem Start: {} KB in {} Blöcken", summe / 1024, offen.len());
    for (n, k) in g.iter().take(30) {
        if *n == 0 {
            println!("  kleine Blöcke (< 4 KB): {k}");
        } else {
            println!("{:>9} B × {k:>3} = {:>6} KB", n, n * k / 1024);
        }
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}
