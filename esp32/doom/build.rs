//! Baut den Doom-Code (doomgeneric/) für den PC (Simulation, Tests). Auf dem ESP32
//! übersetzt ESP-IDF denselben Ordner als Komponente (doomgeneric/CMakeLists.txt).

fn main() {
    println!("cargo:rerun-if-changed=doomgeneric");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        return;
    }
    let mut b = cc::Build::new();
    for e in std::fs::read_dir("doomgeneric").expect("Ordner doomgeneric") {
        let p = e.expect("Eintrag").path();
        if p.extension().is_some_and(|x| x == "c") {
            b.file(p);
        }
    }
    b.include("doomgeneric")
        .define("CMAP256", None)
        .define("DOOMGENERIC_RESX", "320")
        .define("DOOMGENERIC_RESY", "200")
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .define("_CRT_NONSTDC_NO_DEPRECATE", None)
        .warnings(false)
        .opt_level(2)
        .compile("doomgeneric");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // I_Error zeigt unter Windows ein Meldungsfenster.
        println!("cargo:rustc-link-lib=user32");
    }
}
