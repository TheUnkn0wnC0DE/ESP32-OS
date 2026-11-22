//! W8: WLAN-Durchsatz messen (Entwicklungs-Build `ESP32OS_DURCHSATZ=1`),
//! Gegenstück zu `werkzeuge/durchsatz.py` auf dem PC. Der PC baut alle
//! Verbindungen auf; das Board antwortet (TCP-Port 5001, UDP-Port 5002).
//! Ein einziger Faden (interner RAM für Stapel ist knapp), Puffer im PSRAM.
//!
//! TCP: erstes Byte `R` = Board empfängt bis zum Ende und meldet
//! (Bytes, µs) als 2 × u64; `S` + Dauer in ms (u32) = Board sendet so lange.
//! UDP: `START` setzt die Zählung zurück, Daten werden gezählt, `ENDE`
//! beantwortet mit (Bytes, µs); `SENDE` + Dauer (u32 ms) = Board sendet
//! Datagramme an den Absender, danach `FERTIG`.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

const BLOCK: usize = 1460;
/// Über 16 KB: landet im PSRAM (`CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL`).
const PUFFER: usize = 32 * 1024;

fn jetzt() -> i64 {
    crate::hw::now_us()
}

fn mbit(n: u64, us: i64) -> f64 {
    n as f64 * 8.0 / us.max(1) as f64
}

pub fn starten() {
    let _ = std::thread::Builder::new().name("durchsatz".into()).stack_size(6144).spawn(|| {
        let (Ok(tcp), Ok(udp)) = (TcpListener::bind("0.0.0.0:5001"), UdpSocket::bind("0.0.0.0:5002")) else {
            log::error!("DURCHSATZ: Ports 5001/5002 nicht frei");
            return;
        };
        let _ = tcp.set_nonblocking(true);
        let _ = udp.set_read_timeout(Some(Duration::from_millis(20)));
        let mut buf = vec![0u8; PUFFER];
        log::info!("DURCHSATZ: bereit (TCP 5001, UDP 5002)");
        let (mut udp_n, mut udp_erst, mut udp_letzt) = (0u64, 0i64, 0i64);
        loop {
            match tcp.accept() {
                Ok((s, _)) => tcp_bedienen(s, &mut buf),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => log::warn!("DURCHSATZ: accept: {e}"),
            }
            let Ok((k, von)) = udp.recv_from(&mut buf) else { continue };
            let d = &buf[..k];
            if d == b"START" {
                (udp_n, udp_erst, udp_letzt) = (0, 0, 0);
            } else if d == b"ENDE" {
                let us = udp_letzt - udp_erst;
                let mut a = [0u8; 16];
                a[..8].copy_from_slice(&udp_n.to_le_bytes());
                a[8..].copy_from_slice(&(us.max(1) as u64).to_le_bytes());
                let _ = udp.send_to(&a, von);
                log::info!("DURCHSATZ: UDP empfangen {:.2} Mbit/s ({} KiB)", mbit(udp_n, us), udp_n / 1024);
            } else if k == 9 && d.starts_with(b"SENDE") {
                let ms = u32::from_le_bytes([d[5], d[6], d[7], d[8]]).min(60_000);
                let block = vec![0x55u8; BLOCK];
                let (t0, mut n) = (jetzt(), 0u64);
                while jetzt() - t0 < i64::from(ms) * 1000 {
                    match udp.send_to(&block, von) {
                        Ok(k) => n += k as u64,
                        // Sendepuffer voll (lwIP): kurz warten
                        Err(_) => std::thread::sleep(Duration::from_millis(1)),
                    }
                }
                for _ in 0..3 {
                    let _ = udp.send_to(b"FERTIG", von);
                }
                log::info!("DURCHSATZ: UDP gesendet {:.2} Mbit/s ({} KiB)", mbit(n, jetzt() - t0), n / 1024);
            } else {
                let t = jetzt();
                if udp_erst == 0 {
                    udp_erst = t;
                }
                udp_letzt = t;
                udp_n += k as u64;
            }
        }
    });
}

fn tcp_bedienen(mut s: TcpStream, buf: &mut [u8]) {
    let _ = s.set_nonblocking(false);
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let mut cmd = [0u8; 1];
    if s.read_exact(&mut cmd).is_err() {
        return;
    }
    match cmd[0] {
        b'R' => {
            let (t0, mut n) = (jetzt(), 0u64);
            loop {
                match s.read(buf) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => n += k as u64,
                }
            }
            let us = jetzt() - t0;
            let mut a = [0u8; 16];
            a[..8].copy_from_slice(&n.to_le_bytes());
            a[8..].copy_from_slice(&(us.max(1) as u64).to_le_bytes());
            let _ = s.write_all(&a);
            log::info!("DURCHSATZ: TCP empfangen {:.2} Mbit/s ({} KiB)", mbit(n, us), n / 1024);
        }
        b'S' => {
            let mut d = [0u8; 4];
            if s.read_exact(&mut d).is_err() {
                return;
            }
            let ms = u32::from_le_bytes(d).min(60_000);
            buf.fill(0x55);
            let (t0, mut n) = (jetzt(), 0u64);
            while jetzt() - t0 < i64::from(ms) * 1000 {
                if s.write_all(buf).is_err() {
                    break;
                }
                n += buf.len() as u64;
            }
            log::info!("DURCHSATZ: TCP gesendet {:.2} Mbit/s ({} KiB)", mbit(n, jetzt() - t0), n / 1024);
        }
        _ => {}
    }
}
