//! `Sys_Error` (sys_null.c) landet beim Haken aus `quake::beim_fehler` und
//! kehrt nicht zurück; ohne Haken bricht das Programm mit der Meldung auf
//! stderr ab. Jeder Fall läuft in einem eigenen Prozess (dieses
//! Testprogramm noch einmal, gesteuert über [`FALL`]).

use core::ffi::{c_char, c_int};
use std::process::{Command, Output};

unsafe extern "C" {
    fn Sys_Error(error: *const c_char, ...);
}

const FALL: &str = "QUAKE_FEHLER_FALL";
const NAME: &str = "sys_error_meldet_und_kehrt_nicht_zurueck";

fn haken(meldung: &str) {
    println!("Haken: {meldung} / gemerkt: {:?}", quake::fehlermeldung());
    std::process::exit(42);
}

fn lauf(fall: &str) -> Output {
    Command::new(std::env::current_exe().expect("Testprogramm"))
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(FALL, fall)
        .output()
        .expect("Testprogramm starten")
}

#[test]
fn sys_error_meldet_und_kehrt_nicht_zurueck() {
    if let Ok(fall) = std::env::var(FALL) {
        if fall == "haken" {
            quake::beim_fehler(haken);
        }
        let nummer: c_int = 7;
        // SAFETY: Formatangaben passen zu den Argumenten.
        unsafe { Sys_Error(c"Testfehler %d in %s".as_ptr(), nummer, c"e1m1".as_ptr()) };
        println!("Sys_Error ist zurückgekehrt");
        return;
    }

    let mit = lauf("haken");
    let aus = String::from_utf8_lossy(&mit.stdout);
    assert_eq!(mit.status.code(), Some(42), "der Haken beendet den Prozess:\n{aus}");
    assert!(aus.contains("Haken: Testfehler 7 in e1m1 / gemerkt: Some(\"Testfehler 7 in e1m1\")"), "{aus}");

    let ohne = lauf("ohne");
    let fehler = String::from_utf8_lossy(&ohne.stderr);
    assert!(!ohne.status.success(), "ohne Haken bricht das Programm ab");
    assert!(fehler.contains("Sys_Error: Testfehler 7 in e1m1"), "{fehler}");
    assert!(!String::from_utf8_lossy(&ohne.stdout).contains("zurückgekehrt"));
}
