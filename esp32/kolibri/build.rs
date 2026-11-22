//! Kompiliert die KolibriOS-C-Programme (unverändert aus dem Original-Repo,
//! eingebunden über die Hüllen in `c/gen/`, siehe `werkzeuge/praefix.py`)
//! gegen die native <sys/ksys.h> und den libc-Aufsatz. Nur für 32-Bit-Ziele
//! (Zeiger in Registern); am Board baut ESP-IDF die Komponente `kolibri_c`.

/// Programme: Name = Präfix in `c/gen/<name>.c`.
pub const PROGRAMME: &[&str] = &["basic_gui", "boxlib_demo", "libimg_demo", "dialog_demo", "absturz_test", "http_tcp_demo", "netz_laden"];

fn main() {
    println!("cargo:rerun-if-changed=c");
    println!("cargo:rerun-if-changed=include");
    if std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() != Ok("32") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        return;
    }
    let neu = || {
        let mut b = cc::Build::new();
        b.compiler("C:/Program Files/LLVM/bin/clang-cl.exe");
        b.flag("-Wno-everything").include("include");
        b
    };
    let mut z = neu();
    z.define("KOLIBRI_LIBC_INTERN", None).file("c/libc_zusatz.c").compile("kolibri_libc");
    let mut p = neu();
    p.flag("/FIkolibri_libc.h");
    for n in PROGRAMME {
        p.file(format!("c/gen/{n}.c"));
    }
    p.compile("kolibri_programme");
}
