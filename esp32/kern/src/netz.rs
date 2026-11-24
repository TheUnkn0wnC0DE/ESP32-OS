//! Netzwerk der KolibriOS-Systemfunktionen (W9): Funktion 75 (Sockets),
//! 74 (Netzgeräte) und 76 (IPv4-Daten) auf `std::net` – am Board ist das
//! lwIP (über den ESP32-C6), am PC der Rechner.
//!
//! * Socket-Nummern gelten systemweit, gehören aber einem Prozess; beim
//!   Programmende schließt [`prozess_ende`] sie (aus `prozess::aufraeumen`).
//! * Blockierende Aufrufe (Empfangen, Annehmen) warten in Häppchen von
//!   100 ms und prüfen dazwischen, ob das Programm enden soll (dann Fehler
//!   `ECONNABORTED`, das Programm endet beim nächsten Systemaufruf): Ein
//!   wartendes Programm lässt sich so trotzdem beenden (Taskmanager).
//! * Alle öffentlichen Einstiege laufen [`geschuetzt`](crate::geschuetzt):
//!   Socket-Sperren und lwIP-Aufrufe liegen ausserhalb des Kern-Abschnitts;
//!   dort darf das Zwangsbeenden nicht anhalten.
//! * Fehlercodes wie der KolibriOS-Kern (`ebx`), siehe [`fehler`].
//! * Alles, was in der Tabelle bleibt, entsteht im Kern-Abschnitt (der Heap
//!   des Programms verschwindet beim Ende).

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Fehlercodes des KolibriOS-Kerns (siehe libc.obj `socket.c`).
pub mod fehler {
    pub const ENOBUFS: u32 = 1;
    pub const EINPROGRESS: u32 = 2;
    pub const EOPNOTSUPP: u32 = 4;
    pub const EWOULDBLOCK: u32 = 6;
    pub const ENOTCONN: u32 = 9;
    pub const EALREADY: u32 = 10;
    pub const EINVAL: u32 = 11;
    pub const EMSGSIZE: u32 = 12;
    pub const ENOMEM: u32 = 18;
    pub const EADDRINUSE: u32 = 20;
    pub const ECONNRESET: u32 = 52;
    pub const ECONNABORTED: u32 = 54;
    pub const EISCONN: u32 = 56;
    pub const ETIMEDOUT: u32 = 60;
    pub const ECONNREFUSED: u32 = 61;
}
use fehler::*;

/// Adressfamilie, Typen, Protokolle, Optionen (KolibriOS `sys/socket.h`).
pub const AF_INET4: u32 = 2;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;
pub const MSG_PEEK: u32 = 0x02;
pub const MSG_DONTWAIT: u32 = 0x40;
pub const SOL_SOCKET: u32 = 0xffff;
pub const SO_NONBLOCK: u32 = 1 << 31;

/// Zeitlimit für Verbindungsaufbau.
const VERBINDEN_MAX: Duration = Duration::from_secs(10);
/// Häppchen beim Warten (Programmende prüfen).
const HAEPPCHEN: Duration = Duration::from_millis(100);

#[derive(Default)]
struct Inhalt {
    gebunden: Option<SocketAddrV4>,
    tcp: Option<Arc<TcpStream>>,
    lauscher: Option<Arc<TcpListener>>,
    udp: Option<Arc<UdpSocket>>,
    nichtblockierend: bool,
}

struct Sock {
    pid: u32,
    typ: u32,
    inhalt: Mutex<Inhalt>,
}

/// Tabelle: Index + 1 = Socket-Nummer.
static TABELLE: Mutex<Vec<Option<Arc<Sock>>>> = Mutex::new(Vec::new());

fn tabelle<R>(f: impl FnOnce(&mut Vec<Option<Arc<Sock>>>) -> R) -> R {
    crate::kern_abschnitt(|| f(&mut TABELLE.lock().unwrap_or_else(|e| e.into_inner())))
}

fn eigene_pid() -> u32 {
    crate::eigene_pid().unwrap_or(0)
}

/// Socket des eigenen Prozesses.
fn holen(nr: u32) -> Result<Arc<Sock>, u32> {
    let pid = eigene_pid();
    tabelle(|t| match t.get(nr.wrapping_sub(1) as usize) {
        Some(Some(s)) if s.pid == pid => Ok(s.clone()),
        _ => Err(EINVAL),
    })
}

fn inhalt(s: &Sock) -> std::sync::MutexGuard<'_, Inhalt> {
    crate::kern_abschnitt(|| s.inhalt.lock().unwrap_or_else(|e| e.into_inner()))
}

/// io-Fehler → KolibriOS-Fehlercode.
fn code(e: &std::io::Error) -> u32 {
    match e.kind() {
        ErrorKind::ConnectionRefused => ECONNREFUSED,
        ErrorKind::ConnectionReset => ECONNRESET,
        ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe => ECONNABORTED,
        ErrorKind::NotConnected => ENOTCONN,
        ErrorKind::AddrInUse | ErrorKind::AddrNotAvailable => EADDRINUSE,
        ErrorKind::WouldBlock => EWOULDBLOCK,
        ErrorKind::TimedOut => ETIMEDOUT,
        ErrorKind::OutOfMemory => ENOMEM,
        ErrorKind::Unsupported => EOPNOTSUPP,
        _ => EINVAL,
    }
}

/// sockaddr (KolibriOS): +0 Familie (u16), +2 Port (Netzwerk-Bytefolge),
/// +4 IPv4-Adresse (Netzwerk-Bytefolge).
pub fn adresse_lesen(roh: &[u8]) -> Result<SocketAddrV4, u32> {
    if roh.len() < 8 || u16::from_le_bytes([roh[0], roh[1]]) as u32 != AF_INET4 {
        return Err(EINVAL);
    }
    let port = u16::from_be_bytes([roh[2], roh[3]]);
    Ok(SocketAddrV4::new(Ipv4Addr::new(roh[4], roh[5], roh[6], roh[7]), port))
}

/// sockaddr schreiben (16 Byte).
pub fn adresse_schreiben(a: SocketAddrV4, ziel: &mut [u8]) {
    if ziel.len() < 8 {
        return;
    }
    ziel[..2].copy_from_slice(&(AF_INET4 as u16).to_le_bytes());
    ziel[2..4].copy_from_slice(&a.port().to_be_bytes());
    ziel[4..8].copy_from_slice(&a.ip().octets());
    for b in ziel.iter_mut().skip(8).take(8) {
        *b = 0;
    }
}

fn eintragen(s: Sock) -> u32 {
    tabelle(move |t| {
        let s = Some(Arc::new(s));
        match t.iter().position(Option::is_none) {
            Some(i) => {
                t[i] = s;
                i as u32 + 1
            }
            None => {
                t.push(s);
                t.len() as u32
            }
        }
    })
}

/// 75.0 Socket öffnen. Nur IPv4, TCP oder UDP.
fn oeffnen_innen(domain: u32, typ: u32, protokoll: u32) -> Result<u32, u32> {
    if domain != AF_INET4 {
        return Err(EOPNOTSUPP);
    }
    // SO_NONBLOCK darf als Bit im Typ stehen (nicht blockierend von Anfang an)
    let nb = typ & SO_NONBLOCK != 0;
    let typ = typ & !SO_NONBLOCK;
    let ok = match typ {
        SOCK_STREAM => protokoll == 0 || protokoll == IPPROTO_TCP,
        SOCK_DGRAM => protokoll == 0 || protokoll == IPPROTO_UDP,
        _ => false,
    };
    if !ok {
        return Err(EOPNOTSUPP);
    }
    let pid = eigene_pid();
    Ok(crate::kern_abschnitt(|| {
        let i = Inhalt { nichtblockierend: nb, ..Inhalt::default() };
        eintragen(Sock { pid, typ, inhalt: Mutex::new(i) })
    }))
}

/// 75.1 Schließen.
fn schliessen_innen(nr: u32) -> Result<(), u32> {
    let s = holen(nr)?;
    if let Some(t) = &inhalt(&s).tcp {
        let _ = t.shutdown(Shutdown::Both);
    }
    tabelle(|t| {
        if let Some(e) = t.get_mut(nr as usize - 1) {
            *e = None;
        }
    });
    Ok(())
}

/// 75.2 Binden.
fn binden_innen(nr: u32, a: SocketAddrV4) -> Result<(), u32> {
    let s = holen(nr)?;
    let mut i = inhalt(&s);
    if i.gebunden.is_some() {
        return Err(EINVAL);
    }
    if s.typ == SOCK_DGRAM {
        let u = crate::kern_abschnitt(|| UdpSocket::bind(a).map(Arc::new)).map_err(|e| code(&e))?;
        i.gebunden = u.local_addr().ok().and_then(|x| match x {
            std::net::SocketAddr::V4(v) => Some(v),
            _ => None,
        });
        i.udp = Some(u);
    } else {
        i.gebunden = Some(a);
    }
    Ok(())
}

/// 75.3 Lauschen (TCP).
fn lauschen_innen(nr: u32, _rueckstau: u32) -> Result<(), u32> {
    let s = holen(nr)?;
    if s.typ != SOCK_STREAM {
        return Err(EOPNOTSUPP);
    }
    let mut i = inhalt(&s);
    let a = i.gebunden.unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
    let l = crate::kern_abschnitt(|| TcpListener::bind(a).map(Arc::new)).map_err(|e| code(&e))?;
    l.set_nonblocking(true).map_err(|e| code(&e))?;
    i.lauscher = Some(l);
    Ok(())
}

/// 75.4 Verbinden.
/// M3: so viel interner RAM muss frei sein, damit ein Programm eine neue
/// TCP-Verbindung aufbauen darf (lwIP- und WLAN-Puffer kommen von dort).
pub const NETZ_RESERVE_INTERN: usize = 48 * 1024;

/// PC-Tests: freien internen RAM vortäuschen (Plattform `PC`,
/// `intern_frei`); Standard unbegrenzt.
pub static TEST_INTERN_FREI: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);

fn intern_reicht() -> bool {
    // Testwert gilt auch ohne kern::init (Netz-Tests)
    let frei = crate::prozess::intern_frei().min(TEST_INTERN_FREI.load(std::sync::atomic::Ordering::Relaxed));
    frei >= NETZ_RESERVE_INTERN
}

fn verbinden_innen(nr: u32, a: SocketAddrV4) -> Result<(), u32> {
    let s = holen(nr)?;
    if s.typ == SOCK_DGRAM {
        let mut i = inhalt(&s);
        if i.udp.is_none() {
            let u = crate::kern_abschnitt(|| UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)).map(Arc::new)).map_err(|e| code(&e))?;
            i.udp = Some(u);
        }
        return i.udp.as_ref().map_or(Err(EINVAL), |u| u.connect(a).map_err(|e| code(&e)));
    }
    if inhalt(&s).tcp.is_some() {
        return Err(EISCONN);
    }
    if !intern_reicht() {
        crate::sys::debug_text("Netz: zu wenig interner RAM für eine neue Verbindung");
        return Err(ENOMEM);
    }
    let t = crate::kern_abschnitt(|| TcpStream::connect_timeout(&a.into(), VERBINDEN_MAX).map(Arc::new)).map_err(|e| code(&e))?;
    let _ = t.set_nodelay(true);
    let mut i = inhalt(&s);
    if i.nichtblockierend {
        let _ = t.set_nonblocking(true);
    }
    i.tcp = Some(t);
    Ok(())
}

/// 75.5 Annehmen: neue Socket-Nummer und Gegenstelle.
fn annehmen_innen(nr: u32) -> Result<(u32, SocketAddrV4), u32> {
    let s = holen(nr)?;
    let (l, nb) = {
        let i = inhalt(&s);
        (i.lauscher.clone().ok_or(EINVAL)?, i.nichtblockierend)
    };
    loop {
        match l.accept() {
            Ok((t, von)) => {
                let _ = t.set_nonblocking(false);
                let von = match von {
                    std::net::SocketAddr::V4(v) => v,
                    _ => SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0),
                };
                let pid = eigene_pid();
                let neu = crate::kern_abschnitt(|| {
                    let mut i = Inhalt::default();
                    i.tcp = Some(Arc::new(t));
                    eintragen(Sock { pid, typ: SOCK_STREAM, inhalt: Mutex::new(i) })
                });
                return Ok((neu, von));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if nb {
                    return Err(EWOULDBLOCK);
                }
                if crate::prozess::ende_angefordert() {
                    return Err(ECONNABORTED);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(code(&e)),
        }
    }
}

/// 75.6 Senden (blockierend: alles; nicht blockierend: so viel wie geht).
fn senden_innen(nr: u32, daten: &[u8], flags: u32) -> Result<usize, u32> {
    let s = holen(nr)?;
    let (tcp, udp, nb) = {
        let i = inhalt(&s);
        (i.tcp.clone(), i.udp.clone(), i.nichtblockierend || flags & MSG_DONTWAIT != 0)
    };
    if let Some(u) = udp {
        return u.send(daten).map_err(|e| code(&e));
    }
    let t = tcp.ok_or(ENOTCONN)?;
    if nb {
        let _ = t.set_nonblocking(true);
        let r = (&*t).write(daten).map_err(|e| code(&e));
        let _ = t.set_nonblocking(inhalt(&s).nichtblockierend);
        return r;
    }
    (&*t).write_all(daten).map_err(|e| code(&e))?;
    Ok(daten.len())
}

/// 75.7 Empfangen: Anzahl Bytes, 0 = Gegenseite hat geschlossen.
fn empfangen_innen(nr: u32, puffer: &mut [u8], flags: u32) -> Result<usize, u32> {
    let s = holen(nr)?;
    let (tcp, udp, nb) = {
        let i = inhalt(&s);
        (i.tcp.clone(), i.udp.clone(), i.nichtblockierend || flags & MSG_DONTWAIT != 0)
    };
    let peek = flags & MSG_PEEK != 0;
    let einmal = |t: Option<&TcpStream>, u: Option<&UdpSocket>, buf: &mut [u8]| -> std::io::Result<usize> {
        match (t, u) {
            (Some(t), _) if peek => t.peek(buf),
            (Some(mut t), _) => t.read(buf),
            (_, Some(u)) if peek => u.peek(buf),
            (_, Some(u)) => u.recv(buf),
            _ => Err(ErrorKind::NotConnected.into()),
        }
    };
    let (t, u) = (tcp.as_deref(), udp.as_deref());
    if t.is_none() && u.is_none() {
        return Err(ENOTCONN);
    }
    if nb {
        if let Some(t) = t {
            let _ = t.set_nonblocking(true);
        }
        if let Some(u) = u {
            let _ = u.set_nonblocking(true);
        }
        let r = einmal(t, u, puffer).map_err(|e| code(&e));
        let dauerhaft = inhalt(&s).nichtblockierend;
        if let Some(t) = t {
            let _ = t.set_nonblocking(dauerhaft);
        }
        if let Some(u) = u {
            let _ = u.set_nonblocking(dauerhaft);
        }
        return r;
    }
    // Blockierend in Häppchen (Programmende prüfen)
    if let Some(t) = t {
        let _ = t.set_read_timeout(Some(HAEPPCHEN));
    }
    if let Some(u) = u {
        let _ = u.set_read_timeout(Some(HAEPPCHEN));
    }
    loop {
        match einmal(t, u, puffer) {
            Ok(n) => return Ok(n),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if crate::prozess::ende_angefordert() {
                    return Err(ECONNABORTED);
                }
            }
            Err(e) => return Err(code(&e)),
        }
    }
}

/// 75.8 Option setzen: (Ebene, Name, Wert-Bytes). Unterstützt
/// `SOL_SOCKET`/`SO_NONBLOCK`.
fn option_setzen_innen(nr: u32, ebene: u32, name: u32, wert: &[u8]) -> Result<(), u32> {
    let s = holen(nr)?;
    if ebene != SOL_SOCKET || name != SO_NONBLOCK {
        return Err(EOPNOTSUPP);
    }
    let an = wert.first().is_some_and(|&b| b != 0);
    let mut i = inhalt(&s);
    i.nichtblockierend = an;
    if let Some(t) = &i.tcp {
        let _ = t.set_nonblocking(an);
    }
    if let Some(u) = &i.udp {
        let _ = u.set_nonblocking(an);
    }
    Ok(())
}

/// 75.9 Option lesen (`SO_NONBLOCK`): 1 Byte.
fn option_holen_innen(nr: u32, ebene: u32, name: u32) -> Result<u8, u32> {
    let s = holen(nr)?;
    if ebene != SOL_SOCKET || name != SO_NONBLOCK {
        return Err(EOPNOTSUPP);
    }
    Ok(u8::from(inhalt(&s).nichtblockierend))
}

/// Socket wirklich schließen, auch wenn noch Kopien leben: Am Board wickelt
/// das Zwangsbeenden den Stapel eines Programms nicht ab (Task wird
/// angehalten) – Kopien aus einem laufenden `empfangen`/`annehmen` werden
/// nie freigegeben. Gibt es mehr als diese Referenz, wird die Verbindung am
/// lwIP-Griff geschlossen und die Referenz vergessen (kein doppeltes
/// Schließen). Am PC wickeln beendete Fäden ab; dort genügt das Freigeben.
#[cfg(target_os = "espidf")]
fn hart_schliessen<T: std::os::fd::AsRawFd>(a: Arc<T>) {
    if Arc::strong_count(&a) > 1 {
        unsafe extern "C" {
            fn close(fd: i32) -> i32;
        }
        // SAFETY: gültiger Griff; die übrigen Kopien gehören einem
        // angehaltenen Faden und werden nie mehr benutzt oder freigegeben.
        unsafe { close(a.as_raw_fd()) };
        core::mem::forget(a);
    }
}

#[cfg(not(target_os = "espidf"))]
fn hart_schliessen<T>(a: Arc<T>) {
    drop(a);
}

/// Sockets eines beendeten Prozesses schließen (TCP, UDP, Lauscher).
pub fn prozess_ende(pid: u32) {
    let weg: Vec<Arc<Sock>> = tabelle(|t| {
        let mut weg = Vec::new();
        for e in t.iter_mut() {
            if e.as_ref().is_some_and(|s| s.pid == pid) {
                weg.extend(e.take());
            }
        }
        weg
    });
    crate::kern_abschnitt(|| {
        for s in weg {
            let (tcp, udp, lauscher) = {
                let mut i = inhalt(&s);
                (i.tcp.take(), i.udp.take(), i.lauscher.take())
            };
            if let Some(t) = tcp {
                let _ = t.shutdown(Shutdown::Both);
                hart_schliessen(t);
            }
            if let Some(u) = udp {
                hart_schliessen(u);
            }
            if let Some(l) = lauscher {
                hart_schliessen(l);
            }
        }
    });
}

/// Anzahl offener Sockets (Tests, Taskmanager).
pub fn anzahl() -> usize {
    tabelle(|t| t.iter().flatten().count())
}

/// Namen auflösen (network.obj `getaddrinfo`): IPv4-Adressen.
fn aufloesen_innen(name: &str, port: u16) -> Result<Vec<SocketAddrV4>, u32> {
    use std::net::ToSocketAddrs;
    if let Ok(ip) = name.parse::<Ipv4Addr>() {
        return Ok(vec![SocketAddrV4::new(ip, port)]);
    }
    let r = (name, port).to_socket_addrs().map_err(|_| EINVAL)?;
    let v: Vec<SocketAddrV4> = r
        .filter_map(|a| match a {
            std::net::SocketAddr::V4(v) => Some(v),
            _ => None,
        })
        .collect();
    if v.is_empty() { Err(EINVAL) } else { Ok(v) }
}

// ---- Öffentliche Einstiege: geschützt (Zwangsbeenden, A6)

/// [`oeffnen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn oeffnen(domain: u32, typ: u32, protokoll: u32) -> Result<u32, u32> {
    crate::geschuetzt(|| oeffnen_innen(domain, typ, protokoll))
}

/// [`schliessen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn schliessen(nr: u32) -> Result<(), u32> {
    crate::geschuetzt(|| schliessen_innen(nr))
}

/// [`binden_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn binden(nr: u32, a: SocketAddrV4) -> Result<(), u32> {
    crate::geschuetzt(|| binden_innen(nr, a))
}

/// [`lauschen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn lauschen(nr: u32, rueckstau: u32) -> Result<(), u32> {
    crate::geschuetzt(|| lauschen_innen(nr, rueckstau))
}

/// [`verbinden_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn verbinden(nr: u32, a: SocketAddrV4) -> Result<(), u32> {
    crate::geschuetzt(|| verbinden_innen(nr, a))
}

/// [`annehmen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn annehmen(nr: u32) -> Result<(u32, SocketAddrV4), u32> {
    crate::geschuetzt(|| annehmen_innen(nr))
}

/// [`senden_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn senden(nr: u32, daten: &[u8], flags: u32) -> Result<usize, u32> {
    crate::geschuetzt(|| senden_innen(nr, daten, flags))
}

/// [`empfangen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn empfangen(nr: u32, puffer: &mut [u8], flags: u32) -> Result<usize, u32> {
    crate::geschuetzt(|| empfangen_innen(nr, puffer, flags))
}

/// [`option_setzen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn option_setzen(nr: u32, ebene: u32, name: u32, wert: &[u8]) -> Result<(), u32> {
    crate::geschuetzt(|| option_setzen_innen(nr, ebene, name, wert))
}

/// [`option_holen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn option_holen(nr: u32, ebene: u32, name: u32) -> Result<u8, u32> {
    crate::geschuetzt(|| option_holen_innen(nr, ebene, name))
}

/// [`aufloesen_innen`] im geschützten Abschnitt (Socket-Sperren und lwIP).
pub fn aufloesen(name: &str, port: u16) -> Result<Vec<SocketAddrV4>, u32> {
    crate::geschuetzt(|| aufloesen_innen(name, port))
}
