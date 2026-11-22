// Gibt die Link- und Umgebungsdaten von ESP-IDF an Cargo weiter.
fn main() {
    embuild::espidf::sysenv::output();
    // Entwicklungs-Build mit Testbildern (P2-Nachweis)
    println!("cargo:rustc-check-cfg=cfg(esp32os_testbilder)");
    println!("cargo:rerun-if-env-changed=ESP32OS_TESTBILDER");
    if std::env::var_os("ESP32OS_TESTBILDER").is_some() {
        println!("cargo:rustc-cfg=esp32os_testbilder");
    }
}
