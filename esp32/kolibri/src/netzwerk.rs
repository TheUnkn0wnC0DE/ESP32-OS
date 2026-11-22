//! network.obj nativ (W9, KolibriOS `develop/libraries/network`):
//! `getaddrinfo`, `freeaddrinfo`, `inet_addr`, `inet_ntoa` mit derselben
//! Schnittstelle wie `clayer/network.h` (gepackte Strukturen, `__stdcall`).
//! C-Programme erreichen sie über die `kos_`-Namen (siehe
//! `include/kolibri_libc.h`). Namen löst der Kern auf (`kern::netz`, am
//! Board lwIP-DNS); Speicher kommt aus dem Heap des Programms.

use core::ffi::{CStr, c_char, c_int};

/// `struct sockaddr` (KolibriOS, 16 Byte, gepackt).
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct SockAddr {
    pub sin_family: u16,
    /// Netzwerk-Bytefolge.
    pub sin_port: u16,
    /// Netzwerk-Bytefolge im Speicher.
    pub sin_addr: u32,
    pub sin_zero: u64,
}

/// `struct addrinfo` (KolibriOS, gepackt).
#[repr(C, packed)]
pub struct AddrInfo {
    pub ai_flags: c_int,
    pub ai_family: c_int,
    pub ai_socktype: c_int,
    pub ai_protocol: c_int,
    pub ai_addrlen: c_int,
    pub ai_canonname: *mut c_char,
    pub ai_addr: *mut SockAddr,
    pub ai_next: *mut AddrInfo,
}

/// Ein Ergebnis: Kopf und Adresse in einem Block (freigegeben zusammen).
#[repr(C)]
struct Block {
    info: AddrInfo,
    addr: SockAddr,
}

pub const EAI_BADFLAGS: c_int = 3;
pub const EAI_FAMILY: c_int = 5;
pub const EAI_NONAME: c_int = 8;
pub const EAI_SERVICE: c_int = 9;
pub const EAI_SOCKTYPE: c_int = 10;
const AI_PASSIVE: c_int = 1;
/// Unterstützte Flags (wie network.obj `AI_SUPPORTED`).
const AI_SUPPORTED: c_int = 0x40F;

/// Dienst → Port: Zahl oder (wie üblich) „http“/„https“.
fn port(dienst: &str) -> Option<u16> {
    match dienst {
        "" => Some(0),
        "http" => Some(80),
        "https" => Some(443),
        d => d.parse().ok(),
    }
}

/// Namen auflösen. Rückgabe 0 oder `EAI_*`; `*erg` = Liste oder null.
///
/// # Safety
/// Zeiger wie bei `getaddrinfo` (C-Zeichenketten oder null, `erg` gültig).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn kos_getaddrinfo(
    name: *const c_char,
    dienst: *const c_char,
    hinweise: *const AddrInfo,
    erg: *mut *mut AddrInfo,
) -> c_int {
    if erg.is_null() {
        return EAI_BADFLAGS;
    }
    // SAFETY: Vertrag des Aufrufers.
    unsafe { *erg = core::ptr::null_mut() };
    // SAFETY: C-Zeichenketten oder null.
    let text = |p: *const c_char| (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned());
    let (name, dienst) = (text(name), text(dienst));
    let (mut flags, mut familie, mut art, mut protokoll) = (0, 0, 0, 0);
    if !hinweise.is_null() {
        // SAFETY: gültige Struktur (gepackt: Felder einzeln kopieren).
        unsafe {
            flags = (*hinweise).ai_flags;
            familie = (*hinweise).ai_family;
            art = (*hinweise).ai_socktype;
            protokoll = (*hinweise).ai_protocol;
        }
    }
    if flags & !AI_SUPPORTED != 0 {
        return EAI_BADFLAGS;
    }
    if familie != 0 && familie != kern::netz::AF_INET4 as c_int {
        return EAI_FAMILY;
    }
    if !matches!(art, 0..=2) {
        return EAI_SOCKTYPE;
    }
    let Some(p) = port(dienst.as_deref().unwrap_or("")) else { return EAI_SERVICE };
    let adressen = match &name {
        Some(n) => match kern::netz::aufloesen(n, p) {
            Ok(v) => v,
            Err(_) => return EAI_NONAME,
        },
        None if flags & AI_PASSIVE != 0 => vec![std::net::SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, p)],
        None => vec![std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, p)],
    };
    // Liste von hinten aufbauen (ai_next zeigt auf das nächste)
    let mut kopf: *mut AddrInfo = core::ptr::null_mut();
    for a in adressen.iter().rev() {
        let b = Box::into_raw(Box::new(Block {
            info: AddrInfo {
                ai_flags: flags,
                ai_family: kern::netz::AF_INET4 as c_int,
                ai_socktype: art,
                ai_protocol: protokoll,
                ai_addrlen: core::mem::size_of::<SockAddr>() as c_int,
                ai_canonname: core::ptr::null_mut(),
                ai_addr: core::ptr::null_mut(),
                ai_next: kopf,
            },
            addr: SockAddr {
                sin_family: kern::netz::AF_INET4 as u16,
                sin_port: a.port().to_be(),
                sin_addr: u32::from_le_bytes(a.ip().octets()),
                sin_zero: 0,
            },
        }));
        // SAFETY: gerade angelegt; Adresse zeigt in denselben Block.
        unsafe { (*b).info.ai_addr = core::ptr::addr_of_mut!((*b).addr) };
        kopf = b.cast();
    }
    // SAFETY: Vertrag des Aufrufers.
    unsafe { *erg = kopf };
    0
}

/// Liste aus [`kos_getaddrinfo`] freigeben (null erlaubt).
///
/// # Safety
/// `ai` stammt aus `kos_getaddrinfo` (oder ist null).
#[unsafe(no_mangle)]
pub unsafe extern "system" fn kos_freeaddrinfo(mut ai: *mut AddrInfo) {
    while !ai.is_null() {
        // SAFETY: Block aus kos_getaddrinfo (Kopf am Anfang).
        let naechster = unsafe { (*ai).ai_next };
        drop(unsafe { Box::from_raw(ai.cast::<Block>()) });
        ai = naechster;
    }
}

/// „a.b.c.d“ → Adresse (Netzwerk-Bytefolge im Speicher), sonst −1.
///
/// # Safety
/// `name` ist eine C-Zeichenkette oder null.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn kos_inet_addr(name: *const c_char) -> c_int {
    if name.is_null() {
        return -1;
    }
    // SAFETY: C-Zeichenkette.
    let s = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    match s.trim().parse::<std::net::Ipv4Addr>() {
        Ok(ip) => u32::from_le_bytes(ip.octets()) as c_int,
        Err(_) => -1,
    }
}

struct Puffer(core::cell::UnsafeCell<[u8; 16]>);
// SAFETY: wie im Original ein gemeinsamer Puffer (inet_ntoa ist nicht
// fadensicher – Verhalten von network.obj).
unsafe impl Sync for Puffer {}
static NTOA: Puffer = Puffer(core::cell::UnsafeCell::new([0; 16]));

/// Adresse → „a.b.c.d“ (gemeinsamer Puffer wie im Original).
#[unsafe(no_mangle)]
pub extern "system" fn kos_inet_ntoa(ip: c_int) -> *mut c_char {
    let b = (ip as u32).to_le_bytes();
    let s = format!("{}.{}.{}.{}\0", b[0], b[1], b[2], b[3]);
    let p = NTOA.0.get();
    // SAFETY: höchstens 16 Byte („255.255.255.255\0“).
    unsafe {
        let puffer = &mut *p;
        puffer[..s.len()].copy_from_slice(s.as_bytes());
        puffer.as_mut_ptr().cast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adressen_und_aufloesen() {
        let ip = unsafe { kos_inet_addr(c"192.168.1.42".as_ptr()) };
        assert_eq!((ip as u32).to_le_bytes(), [192, 168, 1, 42]);
        assert_eq!(unsafe { kos_inet_addr(c"keine.ip".as_ptr()) }, -1);
        let t = unsafe { CStr::from_ptr(kos_inet_ntoa(ip)) };
        assert_eq!(t.to_str().unwrap(), "192.168.1.42");

        let mut erg: *mut AddrInfo = core::ptr::null_mut();
        assert_eq!(unsafe { kos_getaddrinfo(c"127.0.0.1".as_ptr(), c"80".as_ptr(), core::ptr::null(), &mut erg) }, 0);
        assert!(!erg.is_null());
        let a = unsafe { *(*erg).ai_addr };
        assert_eq!({ a.sin_family }, 2);
        assert_eq!(u16::from_be({ a.sin_port }), 80);
        assert_eq!({ a.sin_addr }.to_le_bytes(), [127, 0, 0, 1]);
        assert_eq!(unsafe { (*erg).ai_addrlen }, 16);
        unsafe { kos_freeaddrinfo(erg) };

        // Fehler: Ergebnis null (freeaddrinfo(null) harmlos)
        let mut erg: *mut AddrInfo = 1 as *mut AddrInfo;
        assert_eq!(unsafe { kos_getaddrinfo(c"127.0.0.1".as_ptr(), c"kein-port".as_ptr(), core::ptr::null(), &mut erg) }, EAI_SERVICE);
        assert!(erg.is_null());
        unsafe { kos_freeaddrinfo(erg) };
        // localhost per Namensauflösung
        let mut erg: *mut AddrInfo = core::ptr::null_mut();
        assert_eq!(unsafe { kos_getaddrinfo(c"localhost".as_ptr(), c"http".as_ptr(), core::ptr::null(), &mut erg) }, 0);
        unsafe { kos_freeaddrinfo(erg) };
    }
}
