//! Symbole einzeln vorbereiten (O1): Der Streifen `daten/icons32.png`
//! (131 Symbole 32×32) wird beim Bauen zerlegt – je Symbol die Bildpunkte
//! (ARGB, little-endian) als rohes Deflate (miniz_oxide, beste Stufe). Am
//! Board entpackt der Desktop so nur die Symbole, die er zeigt (vorher: den
//! ganzen Streifen dekodieren, mit Skalieren 455 ms beim Start).
//! Vorskalierte 48×48-Fassungen bräuchten ~280 KB Flash (Verläufe packen
//! schlecht) – skaliert wird darum weiter zur Laufzeit (gemerkt).

use std::io::Write;

fn main() {
    let quelle = std::path::Path::new("daten/icons32.png");
    println!("cargo:rerun-if-changed={}", quelle.display());
    let png = std::fs::read(quelle).expect("daten/icons32.png");
    let streifen = kolibri::dekoder::dekodieren(&png).expect("icons32.png dekodieren").1.pop().expect("Bild");
    assert_eq!(streifen.w, 32, "Symbolstreifen 32 Bildpunkte breit");
    let n = (streifen.h / 32) as usize;
    let aus = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let mut blob = Vec::new();
    let mut tabelle = format!(
        "/// Erzeugt von desktop/build.rs: {n} Symbole.\npub const SYMBOLE_ANZAHL: usize = {n};\n\
         /// (Anfang, Länge) im Blob je Symbol 32×32.\npub static SYMBOLE_32: [(usize, usize); SYMBOLE_ANZAHL] = ["
    );
    for i in 0..n {
        let roh: Vec<u8> = streifen.px[i * 1024..(i + 1) * 1024].iter().flat_map(|p| p.to_le_bytes()).collect();
        let d = miniz_oxide::deflate::compress_to_vec(&roh, 10);
        tabelle.push_str(&format!("({}, {}), ", blob.len(), d.len()));
        blob.extend_from_slice(&d);
    }
    tabelle.push_str("];\n/// Alle Symbole (rohes Deflate).\npub static SYMBOLE_BLOB: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/symbole.bin\"));\n");
    std::fs::write(aus.join("symbole.bin"), &blob).expect("symbole.bin");
    std::fs::File::create(aus.join("symbole.rs")).and_then(|mut f| f.write_all(tabelle.as_bytes())).expect("symbole.rs");
}
