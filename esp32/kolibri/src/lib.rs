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
                    fn haupt(argc: i32, argv: *const *mut core::ffi::c_char) -> i32;
                }
                // argc/argv wie crt0 von libc.obj (Z3: TinyHashView liest
                // argv[1]; vorher bekam main keine Argumente)
                let mut teile: Vec<Vec<u8>> = std::iter::once(concat!("/sys/", stringify!($name)).to_string())
                    .chain(argv_zerlegen(&kern::argumente()))
                    .map(|t| t.into_bytes().into_iter().filter(|&b| b != 0).chain([0]).collect())
                    .collect();
                let mut zeiger: Vec<*mut core::ffi::c_char> = teile.iter_mut().map(|t| t.as_mut_ptr().cast()).collect();
                zeiger.push(core::ptr::null_mut());
                // SAFETY: C-Programm, läuft im eigenen Faden mit eigenem Heap;
                // argv bleibt bis zum Ende gültig (teile/zeiger leben hier).
                unsafe {
                    haupt(teile.len() as i32, zeiger.as_ptr());
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
    // Z3: Passwort-Generator (programs/other/PasswordGen)
    passwordgen: 256,
    // Z3: Prüfsummen MD5/SHA-1/SHA-256 (programs/other/TinyHashView)
    thashview: 512,
}

#[cfg(not(target_pointer_width = "32"))]
pub static PROGRAMME: &[ProgrammEintrag] = &[];

/// Parameter wie `crt0.asm` von libc.obj zerlegen: durch Leerzeichen
/// getrennt, ein mit `"` beginnender Parameter reicht bis zum nächsten `"`
/// (ohne die Anführungszeichen); höchstens 64 Parameter.
pub fn argv_zerlegen(params: &str) -> Vec<String> {
    let mut aus = Vec::new();
    let mut it = params.chars().peekable();
    while aus.len() < 64 {
        while it.peek() == Some(&' ') {
            it.next();
        }
        let Some(&c) = it.peek() else { break };
        let ende = if c == '"' {
            it.next();
            '"'
        } else {
            ' '
        };
        aus.push(it.by_ref().take_while(|&z| z != ende).collect());
    }
    aus
}

#[cfg(test)]
mod tests {
    #[test]
    fn argv_wie_crt0() {
        assert_eq!(super::argv_zerlegen("/rd/1/a.txt"), ["/rd/1/a.txt"]);
        assert_eq!(super::argv_zerlegen("  -x \"/sd0/1/Mein Ordner/b.pdf\" y"), ["-x", "/sd0/1/Mein Ordner/b.pdf", "y"]);
        assert!(super::argv_zerlegen("").is_empty());
        assert_eq!(super::argv_zerlegen("\"offen bis Ende"), ["offen bis Ende"]);
    }
}
