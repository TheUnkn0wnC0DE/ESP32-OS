//! Baut den Quake-Code (quakegeneric/) für den PC (Simulation, Tests). Auf dem
//! ESP32 übersetzt ESP-IDF denselben Ordner als Komponente
//! (quakegeneric/CMakeLists.txt).

fn main() {
    println!("cargo:rerun-if-changed=quakegeneric");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        return;
    }
    let mut b = cc::Build::new();
    for e in std::fs::read_dir("quakegeneric").expect("Ordner quakegeneric") {
        let p = e.expect("Eintrag").path();
        if p.extension().is_some_and(|x| x == "c") {
            b.file(p);
        }
    }
    let namen = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("Crate-Ordner")).join("quakegeneric").join("qnamen.h");
    if b.get_compiler().is_like_msvc() {
        b.flag(format!("/FI{}", namen.display()));
    } else {
        b.flag("-include").flag(namen.to_str().expect("Pfad"));
    }
    b.include("quakegeneric")
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .define("_CRT_NONSTDC_NO_DEPRECATE", None)
        .warnings(false)
        .opt_level(2)
        .compile("quakegeneric");
}
