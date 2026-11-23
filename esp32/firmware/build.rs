// Gibt die Link- und Umgebungsdaten von ESP-IDF an Cargo weiter.
fn main() {
    embuild::espidf::sysenv::output();
    // Entwicklungs-Build mit Testbildern (P2-Nachweis)
    println!("cargo:rustc-check-cfg=cfg(esp32os_testbilder)");
    println!("cargo:rerun-if-env-changed=ESP32OS_TESTBILDER");
    if std::env::var_os("ESP32OS_TESTBILDER").is_some() {
        println!("cargo:rustc-cfg=esp32os_testbilder");
    }
    // Entwicklungs-Build mit Testtönen für den Musikplayer (A5-Nachweis)
    println!("cargo:rustc-check-cfg=cfg(esp32os_musik_test)");
    println!("cargo:rerun-if-env-changed=ESP32OS_MUSIK_TEST");
    if std::env::var_os("ESP32OS_MUSIK_TEST").is_some() {
        println!("cargo:rustc-cfg=esp32os_musik_test");
    }
    // Entwicklungs-Build mit Heap-Tracing des Systemheaps (BR1: Leck im
    // internen RAM); braucht CONFIG_HEAP_TRACING_STANDALONE im sdkconfig
    println!("cargo:rustc-check-cfg=cfg(esp32os_heap_trace)");
    println!("cargo:rerun-if-env-changed=ESP32OS_HEAP_TRACE");
    if std::env::var_os("ESP32OS_HEAP_TRACE").is_some() {
        println!("cargo:rustc-cfg=esp32os_heap_trace");
    }
    // Entwicklungs-Build mit Test-PDFs für den PDF-Betrachter (Z1-Nachweis)
    println!("cargo:rustc-check-cfg=cfg(esp32os_pdf_test)");
    println!("cargo:rerun-if-env-changed=ESP32OS_PDF_TEST");
    if std::env::var_os("ESP32OS_PDF_TEST").is_some() {
        println!("cargo:rustc-cfg=esp32os_pdf_test");
    }
}
