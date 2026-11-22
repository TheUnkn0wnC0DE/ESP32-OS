//! ESP32-OS: Absturzmerker nach dem Neustart lesen (D12, siehe
//! `komponenten/esp32os/absturz.c`).

use core::ffi::c_char;

unsafe extern "C" {
    fn esp32os_absturz_holen(name: *mut c_char, nl: i32, grund: *mut c_char, gl: i32) -> i32;
}

/// (Programm/Task, Grund) des Absturzes vor dem letzten Neustart; der Merker
/// wird dabei gelöscht.
pub fn holen() -> Option<(String, String)> {
    let mut n = [0u8; 24];
    let mut g = [0u8; 64];
    // SAFETY: Puffer mit angegebener Länge.
    let da = unsafe { esp32os_absturz_holen(n.as_mut_ptr().cast(), 24, g.as_mut_ptr().cast(), 64) } == 1;
    let s = |b: &[u8]| String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned();
    da.then(|| (s(&n), s(&g)))
}
