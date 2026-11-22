//! KolibriOS-C-Programme als ESP32-OS-Programme (statisch eingelinkt).
//!
//! Quellen in `c/` stammen unverändert aus dem KolibriOS-Repository
//! (GPL-2.0); `include/sys/ksys.h` ist die mit `werkzeuge/ksys_nativ.py`
//! erzeugte native Fassung.

use kern::ProgrammEintrag;

pub mod boxlib;
pub mod dekoder;
pub mod kodierer;
pub mod dialog;
pub mod libimg;
pub mod libini;
pub mod netzwerk;

macro_rules! programme {
    ($($name:ident: $heap:expr),* $(,)?) => {
        $(
            #[cfg(target_pointer_width = "32")]
            #[allow(non_snake_case)]
            pub fn $name() {
                unsafe extern "C" {
                    #[link_name = concat!(stringify!($name), "_main")]
                    fn haupt() -> i32;
                }
                // SAFETY: C-Programm, läuft im eigenen Faden mit eigenem Heap.
                unsafe {
                    haupt();
                }
            }
        )*
        /// Programmtabelle der C-Programme (leer auf 64-Bit-Zielen).
        #[cfg(target_pointer_width = "32")]
        pub static PROGRAMME: &[ProgrammEintrag] = &[
            $(ProgrammEintrag { name: stringify!($name), heap_kb: $heap, stapel_kb: 128, start: $name },)*
        ];
    };
}
programme! {
    basic_gui: 512,
    boxlib_demo: 512,
    libimg_demo: 2048,
    dialog_demo: 512,
    absturz_test: 128,
    http_tcp_demo: 256,
    netz_laden: 256,
}

#[cfg(not(target_pointer_width = "32"))]
pub static PROGRAMME: &[ProgrammEintrag] = &[];
