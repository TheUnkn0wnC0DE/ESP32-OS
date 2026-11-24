//! W9 (PC): Sockets der Funktion 75 gegen lokale Gegenstellen – TCP
//! verbinden/senden/empfangen, nicht blockierend, Lauschen/Annehmen, UDP,
//! Option SO_NONBLOCK, abgelehnte Verbindung, Aufräumen beim Programmende.

use kern::netz::{self, fehler};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};

fn lokal(port: u16) -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)
}

#[test]
fn sockets() {
    // sockaddr hin und zurück (Familie 2, Port und Adresse in Netzwerk-Bytefolge)
    let mut roh = [0u8; 16];
    netz::adresse_schreiben(lokal(8080), &mut roh);
    assert_eq!(&roh[..8], &[2, 0, 0x1F, 0x90, 127, 0, 0, 1]);
    assert_eq!(netz::adresse_lesen(&roh), Ok(lokal(8080)));
    assert_eq!(netz::adresse_lesen(&[10, 0, 0, 80, 1, 2, 3, 4]), Err(fehler::EINVAL));

    // Nur IPv4 mit TCP/UDP
    assert_eq!(netz::oeffnen(10, netz::SOCK_STREAM, 0), Err(fehler::EOPNOTSUPP));
    assert_eq!(netz::oeffnen(netz::AF_INET4, 3, 0), Err(fehler::EOPNOTSUPP));

    // TCP-Gegenstelle
    let server = TcpListener::bind(lokal(0)).unwrap();
    let port = server.local_addr().unwrap().port();
    let s = netz::oeffnen(netz::AF_INET4, netz::SOCK_STREAM, netz::IPPROTO_TCP).unwrap();
    assert!(s >= 1);
    netz::verbinden(s, lokal(port)).unwrap();
    assert_eq!(netz::verbinden(s, lokal(port)), Err(fehler::EISCONN));

    // M3: zu wenig interner RAM → keine neue TCP-Verbindung (ENOMEM)
    use std::sync::atomic::Ordering::Relaxed;
    netz::TEST_INTERN_FREI.store(netz::NETZ_RESERVE_INTERN - 1, Relaxed);
    let knapp = netz::oeffnen(netz::AF_INET4, netz::SOCK_STREAM, netz::IPPROTO_TCP).unwrap();
    assert_eq!(netz::verbinden(knapp, lokal(port)), Err(fehler::ENOMEM));
    netz::TEST_INTERN_FREI.store(usize::MAX, Relaxed);
    netz::verbinden(knapp, lokal(port)).unwrap();
    let (mut gegen, _) = server.accept().unwrap();
    assert_eq!(netz::senden(s, b"GET / HTTP/1.0\r\n\r\n", 0), Ok(18));
    let mut b = [0u8; 18];
    gegen.read_exact(&mut b).unwrap();
    assert_eq!(&b, b"GET / HTTP/1.0\r\n\r\n");
    // Noch nichts da: MSG_DONTWAIT kehrt sofort zurück
    let mut p = [0u8; 64];
    assert_eq!(netz::empfangen(s, &mut p, netz::MSG_DONTWAIT), Err(fehler::EWOULDBLOCK));
    gegen.write_all(b"HTTP/1.0 200 OK\r\n").unwrap();
    // MSG_PEEK lässt die Daten liegen
    let n = netz::empfangen(s, &mut p, netz::MSG_PEEK).unwrap();
    assert!(n > 0);
    let n = netz::empfangen(s, &mut p, 0).unwrap();
    assert_eq!(&p[..n], b"HTTP/1.0 200 OK\r\n");
    drop(gegen);
    assert_eq!(netz::empfangen(s, &mut p, 0), Ok(0), "Gegenseite zu");
    netz::schliessen(s).unwrap();
    assert_eq!(netz::schliessen(s), Err(fehler::EINVAL), "schon zu");

    // Lauschen und Annehmen
    let l = netz::oeffnen(netz::AF_INET4, netz::SOCK_STREAM, 0).unwrap();
    let frei = TcpListener::bind(lokal(0)).unwrap().local_addr().unwrap().port();
    netz::binden(l, lokal(frei)).unwrap();
    netz::lauschen(l, 4).unwrap();
    // nicht blockierend: noch keine Verbindung
    netz::option_setzen(l, netz::SOL_SOCKET, netz::SO_NONBLOCK, &[1]).unwrap();
    assert_eq!(netz::option_holen(l, netz::SOL_SOCKET, netz::SO_NONBLOCK), Ok(1));
    assert_eq!(netz::annehmen(l).map(|x| x.0), Err(fehler::EWOULDBLOCK));
    netz::option_setzen(l, netz::SOL_SOCKET, netz::SO_NONBLOCK, &[0]).unwrap();
    let mut client = TcpStream::connect(lokal(frei)).unwrap();
    let (neu, von) = netz::annehmen(l).unwrap();
    assert_eq!(*von.ip(), Ipv4Addr::LOCALHOST);
    client.write_all(b"ping").unwrap();
    let mut q = [0u8; 4];
    let n = netz::empfangen(neu, &mut q, 0).unwrap();
    assert_eq!(&q[..n], &b"ping"[..n]);
    assert_eq!(netz::senden(neu, b"pong", 0), Ok(4));
    let mut r = [0u8; 4];
    client.read_exact(&mut r).unwrap();
    assert_eq!(&r, b"pong");

    // UDP: binden, empfangen, verbinden + senden
    let u = netz::oeffnen(netz::AF_INET4, netz::SOCK_DGRAM, netz::IPPROTO_UDP).unwrap();
    let up = UdpSocket::bind(lokal(0)).unwrap().local_addr().unwrap().port();
    netz::binden(u, lokal(up)).unwrap();
    let gegen_u = UdpSocket::bind(lokal(0)).unwrap();
    gegen_u.send_to(b"datagramm", lokal(up)).unwrap();
    let mut d = [0u8; 32];
    assert_eq!(netz::empfangen(u, &mut d, 0), Ok(9));
    assert_eq!(&d[..9], b"datagramm");
    netz::verbinden(u, match gegen_u.local_addr().unwrap() {
        std::net::SocketAddr::V4(v) => v,
        _ => unreachable!(),
    })
    .unwrap();
    assert_eq!(netz::senden(u, b"antwort", 0), Ok(7));
    let (n, _) = gegen_u.recv_from(&mut d).unwrap();
    assert_eq!(&d[..n], b"antwort");

    // SO_NONBLOCK als Bit im Typ: von Anfang an nicht blockierend
    let nb = netz::oeffnen(netz::AF_INET4, netz::SOCK_STREAM | netz::SO_NONBLOCK, 0).unwrap();
    assert_eq!(netz::option_holen(nb, netz::SOL_SOCKET, netz::SO_NONBLOCK), Ok(1));

    // Abgelehnt: nichts lauscht
    let zu = TcpListener::bind(lokal(0)).unwrap().local_addr().unwrap().port();
    let a = netz::oeffnen(netz::AF_INET4, netz::SOCK_STREAM, 0).unwrap();
    assert_eq!(netz::verbinden(a, lokal(zu)), Err(fehler::ECONNREFUSED));

    // Namen auflösen (Zahl direkt, localhost per Resolver)
    assert_eq!(netz::aufloesen("10.0.0.7", 80), Ok(vec![SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 7), 80)]));
    assert!(netz::aufloesen("localhost", 80).unwrap().iter().any(|x| *x.ip() == Ipv4Addr::LOCALHOST));

    // Programmende: alle Sockets dieses Prozesses (hier pid 0) zu
    assert!(netz::anzahl() >= 4);
    netz::prozess_ende(0);
    assert_eq!(netz::anzahl(), 0);
    assert_eq!(netz::senden(neu, b"x", 0), Err(fehler::EINVAL));
}
