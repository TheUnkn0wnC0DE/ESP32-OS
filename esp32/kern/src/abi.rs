//! C-Schnittstelle der Systemfunktionen: `kos_syscall(&mut Regs)` ersetzt
//! `int 0x40` (Register wie in `sysfuncs.txt`). Die native `<sys/ksys.h>`
//! (esp32/kolibri/include, erzeugt aus dem KolibriOS-Original) ruft nur diese
//! Funktion auf. Zeiger stecken in 32-Bit-Registern – deshalb nur auf
//! 32-Bit-Zielen (Board: RISC-V 32; PC-Tests: i686).

use crate::sys;
use core::slice;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Regs {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebp: u32,
}

/// Nullterminierte Bytes ab `p` (bei `breit` zwei Byte je Zeichen).
unsafe fn cstr<'a>(p: u32, breit: bool) -> &'a [u8] {
    if p == 0 {
        return &[];
    }
    let q = p as usize as *const u8;
    let mut n = 0usize;
    // SAFETY: Aufrufer liefert einen nullterminierten String.
    unsafe {
        if breit {
            while *q.add(n) != 0 || *q.add(n + 1) != 0 {
                n += 2;
            }
            slice::from_raw_parts(q, n + 2)
        } else {
            while *q.add(n) != 0 {
                n += 1;
            }
            slice::from_raw_parts(q, n + 1)
        }
    }
}

unsafe fn bytes<'a>(p: u32, n: usize) -> &'a [u8] {
    if p == 0 || n == 0 {
        return &[];
    }
    // SAFETY: Aufrufer garantiert n gültige Bytes.
    unsafe { slice::from_raw_parts(p as usize as *const u8, n) }
}

unsafe fn bytes_mut<'a>(p: u32, n: usize) -> &'a mut [u8] {
    if p == 0 || n == 0 {
        return &mut [];
    }
    // SAFETY: Aufrufer garantiert n gültige, beschreibbare Bytes.
    unsafe { slice::from_raw_parts_mut(p as usize as *mut u8, n) }
}

/// Fenstertitel: optionales Kodierungs-Präfix 1/2/3 (cp866/UTF-16/UTF-8),
/// sonst UTF-8, falls gültig, sonst cp866.
fn titel(roh: &[u8]) -> String {
    let roh = roh.strip_suffix(&[0]).unwrap_or(roh);
    let cp866 = |b: &[u8]| b.iter().map(|&c| char::from_u32(crate::schrift::cp866_zu_uni(c)).unwrap_or('?')).collect();
    match roh.first() {
        Some(1) => cp866(&roh[1..]),
        Some(2) => String::from_utf16_lossy(&roh[1..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()),
        Some(3) => String::from_utf8_lossy(&roh[1..]).into_owned(),
        _ => match core::str::from_utf8(roh) {
            Ok(s) => s.into(),
            Err(_) => cp866(roh),
        },
    }
}

/// Der Systemaufruf. Unbekannte Funktionen liefern eax = −1.
///
/// # Safety
/// `r` zeigt auf gültige Register; Zeiger darin folgen den Verträgen der
/// jeweiligen KolibriOS-Funktion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kos_syscall(r: *mut Regs) {
    // SAFETY: Vertrag des Aufrufers.
    let r = unsafe { &mut *r };
    // SAFETY: alle Zeigerzugriffe folgen sysfuncs.txt.
    unsafe { dispatch(r) }
}

unsafe fn dispatch(r: &mut Regs) {
    let (ebx, ecx, edx, esi, edi) = (r.ebx, r.ecx, r.edx, r.esi, r.edi);
    match r.eax as i32 {
        -1 => sys::ende(),
        0 => {
            let t = (edx >> 28 & 1 != 0 && edi != 0).then(|| titel(unsafe { cstr(edi, false) }));
            sys::fenster(ebx, ecx, edx, esi, t.as_deref());
        }
        1 => sys::pixel(ebx as i32, ecx as i32, edx),
        2 => r.eax = sys::taste(),
        3 => r.eax = sys::uhrzeit(),
        4 => {
            let breit = (ecx >> 28 & 3) == 2;
            let null = ecx & 0x8000_0000 != 0;
            let t = if null {
                unsafe { cstr(edx, breit) }
            } else {
                let n = esi as usize * if breit { 2 } else { 1 };
                // UTF-8: Länge in Zeichen, Bytes bis zu 4 je Zeichen
                let n = if (ecx >> 28 & 3) == 3 { utf8_laenge(edx, esi as usize) } else { n };
                unsafe { bytes(edx, n) }
            };
            if ecx & 0x0800_0000 != 0 {
                let kopf = unsafe { bytes(edi, 8) };
                if kopf.len() == 8 {
                    let w = u32::from_le_bytes([kopf[0], kopf[1], kopf[2], kopf[3]]) as usize;
                    let h = u32::from_le_bytes([kopf[4], kopf[5], kopf[6], kopf[7]]) as usize;
                    sys::text_in_puffer(ebx, ecx, t, esi, unsafe { bytes_mut(edi, 8 + w * h * 4) });
                }
            } else {
                sys::text(ebx, ecx, t, esi, edi);
            }
        }
        5 => sys::warten(ebx),
        7 => {
            let n = 3 * (ecx >> 16) as usize * (ecx & 0xFFFF) as usize;
            sys::bild(unsafe { bytes(ebx, n) }, ecx, edx);
        }
        8 => sys::knopf(ebx, ecx, edx, esi),
        9 => {
            let mut b = [0u8; 1024];
            let n = sys::faden_info(ecx as i32, &mut b);
            unsafe { bytes_mut(ebx, 76) }.copy_from_slice(&b[..76]);
            r.eax = n as u32;
        }
        10 => r.eax = sys::warte_ereignis(),
        11 => r.eax = sys::pruefe_ereignis(),
        12 => sys::neuzeichnen(ebx),
        13 => sys::rechteck(ebx, ecx, edx),
        14 => r.eax = sys::bildschirmgroesse(),
        17 => r.eax = sys::knopf_id(),
        18 if ebx == 22 => r.eax = sys::minimieren(ecx, edx),
        18 if ebx == 25 => r.eax = sys::fensterebene(ecx, edx, esi),
        18 => r.eax = sys::system(ebx, ecx),
        15 => {
            let d = if ebx == 5 { unsafe { bytes(ecx, esi as usize) } } else { &[] };
            let (a, b) = sys::hintergrund(ebx, ecx, edx, d);
            if matches!(ebx, 6 | 7 | 8) {
                r.eax = a;
            }
            if ebx == 8 {
                r.ebx = b;
            }
        }
        39 => r.eax = sys::hintergrund_lesen(ebx, ecx),
        54 => {
            let d = if ebx == 2 { unsafe { bytes(edx, ecx as usize) } } else { &[] };
            r.eax = sys::ablage(ebx, ecx, d);
        }
        23 => r.eax = sys::warte_ereignis_max(ebx),
        26 => match ebx {
            9 => r.eax = sys::zeitzaehler(),
            10 => {
                let ns = sys::zeit_ns();
                r.eax = ns as u32;
                r.edx = (ns >> 32) as u32;
            }
            _ => r.eax = u32::MAX,
        },
        29 => r.eax = sys::datum(),
        35 => r.eax = sys::bildpunkt(ebx),
        37 => match ebx {
            4 | 8 => {
                let bild = match (ebx, edx & 0xFFFF) {
                    (8, _) | (4, 0) => {
                        let pfad = crate::datei::pfad_text(unsafe { cstr(ecx, edx == 2) }, if ebx == 8 { edx } else { 0 });
                        let mut info = [0u8; 40];
                        if crate::datei::info(&pfad, &mut info) == 0 {
                            let n = u64::from_le_bytes([info[32], info[33], info[34], info[35], info[36], info[37], info[38], info[39]]) as usize;
                            let mut b = vec![0u8; n];
                            crate::datei::lesen(&pfad, 0, &mut b);
                            crate::zeiger::cur_lesen(&b)
                        } else {
                            None
                        }
                    }
                    (_, 1) => {
                        let kopf = unsafe { bytes(ecx, 22) };
                        crate::zeiger::cur_laenge(kopf).and_then(|n| crate::zeiger::cur_lesen(unsafe { bytes(ecx, n) }))
                    }
                    _ => crate::zeiger::argb(unsafe { bytes(ecx, 4096) }, ((edx >> 24) as i32, (edx >> 16 & 0xFF) as i32)),
                };
                r.eax = sys::zeiger_laden(bild);
            }
            5 => r.eax = sys::zeiger_setzen(ecx),
            6 => sys::zeiger_loeschen(ecx),
            _ => r.eax = sys::maus(ebx),
        },
        38 => sys::linie(ebx, ecx, edx),
        40 => r.eax = sys::maske(ebx),
        47 => {
            let wert = if ebx & 0xFF == 1 {
                let b = unsafe { bytes(ecx, if ebx & 0x4000_0000 != 0 { 8 } else { 4 }) };
                let mut v = [0u8; 8];
                v[..b.len()].copy_from_slice(b);
                u64::from_le_bytes(v)
            } else {
                u64::from(ecx)
            };
            sys::zahl(ebx, wert, edx, esi, edi);
        }
        48 => {
            let buf = match ebx {
                2 => unsafe { bytes_mut(ecx, edx as usize) },
                3 => unsafe { bytes_mut(ecx, edx as usize) },
                _ => &mut [],
            };
            let (a, b) = sys::stil(ebx, ecx, edx, buf);
            r.eax = a;
            if matches!(ebx, 5 | 7) {
                r.ebx = b;
            }
        }
        51 if ebx == 1 => {
            let start = ecx as usize;
            r.eax = sys::faden(
                64,
                Box::new(move || {
                    // SAFETY: Einsprungadresse vom Programm (KolibriOS: Fadenfunktion ohne Argumente).
                    let f: extern "C" fn() = unsafe { core::mem::transmute(start) };
                    f();
                    sys::ende();
                }),
            ) as u32;
        }
        63 if ebx == 1 => sys::debug_zeichen(ecx as u8),
        65 => {
            let (w, h) = ((ecx >> 16) as usize, (ecx & 0xFFFF) as usize);
            let zeile = match esi {
                9 => w,
                15 | 16 => 2 * w,
                24 => 3 * w,
                32 => 4 * w,
                b => (w * b as usize).div_ceil(8),
            };
            let n = h * (zeile as isize + r.ebp as i32 as isize).max(zeile as isize) as usize;
            let pal: Vec<u32> = if esi <= 8 && edi != 0 {
                unsafe { bytes(edi, 4 << esi) }.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
            } else {
                Vec::new()
            };
            sys::palettenbild(unsafe { bytes(ebx, n) }, ecx, edx, esi, &pal, r.ebp as i32);
        }
        66 => r.eax = sys::tastatur_modus(ebx, ecx),
        67 => sys::fenster_bewegen(ebx as i32, ecx as i32, edx as i32, esi as i32),
        68 => match ebx {
            11 => r.eax = crate::prozess::eigener_heap().map_or(0, |h| h.groesse() as u32),
            12 => r.eax = sys::speicher_holen(ecx as usize) as usize as u32,
            13 => r.eax = u32::from(unsafe { sys::speicher_frei(ecx as usize as *mut u8) }),
            20 => {
                let p = edx as usize as *mut u8;
                let heap = crate::prozess::eigener_heap();
                if !p.is_null() && !heap.is_some_and(|h| h.enthaelt(p)) {
                    // Nicht aus dem Programm-Heap (ESP32-OS: ebx = 1 meldet das,
                    // der libc-Aufsatz nimmt dann realloc der Host-libc).
                    r.eax = 0;
                    r.ebx = 1;
                } else {
                    let alt = if p.is_null() { 0 } else { heap.map_or(0, |h| unsafe { h.nutzgroesse(p) }) };
                    r.eax = unsafe { sys::speicher_neu(p, alt, ecx as usize) } as usize as u32;
                    r.ebx = 0;
                }
            }
            _ => r.eax = 0,
        },
        71 if ebx == 1 || ebx == 2 => {
            let roh = unsafe { cstr(ecx, false) };
            let t = if ebx == 2 {
                let mut v = vec![edx as u8];
                v.extend_from_slice(roh);
                titel(&v)
            } else {
                titel(roh)
            };
            sys::titel(&t);
        }
        73 => {
            let p = unsafe { bytes(ecx, 40) };
            if p.len() == 40 {
                let d = |i: usize| i32::from_le_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]]);
                let werte = [d(0), d(1), d(2), d(3), d(4), d(5), d(6), d(7)];
                let zeile = d(9) as usize;
                let n = (werte[5].max(0) + werte[7]).max(0) as usize * zeile;
                sys::blit(ebx, werte, unsafe { bytes(d(8) as u32, n) }, zeile);
            }
        }
        30 => match ebx {
            1 | 4 => crate::datei::ordner_setzen(&crate::datei::pfad_text(unsafe { cstr(ecx, edx == 2) }, if ebx == 4 { edx } else { 0 })),
            2 | 5 => {
                let o = crate::datei::aktueller_ordner();
                let kod = if ebx == 5 { edx_kodierung(r.esi) } else { 1 };
                let roh = pfad_kodieren(&o, kod);
                let ziel = unsafe { bytes_mut(ecx, edx as usize) };
                let n = roh.len().min(ziel.len());
                ziel[..n].copy_from_slice(&roh[..n]);
                r.eax = roh.len() as u32;
            }
            _ => r.eax = u32::MAX,
        },
        70 | 80 => {
            let blk = unsafe { bytes(ebx, 28) };
            let d = |i: usize| u32::from_le_bytes([blk[i], blk[i + 1], blk[i + 2], blk[i + 3]]);
            let (sub, off, hi, n, ptr) = (d(0), d(4), d(8), d(12) as usize, d(16));
            let pfad = if r.eax == 80 {
                let kod = d(20);
                crate::datei::pfad_text(unsafe { cstr(d(24), kod == 2) }, kod)
            } else if blk[20] == 0 {
                let p = u32::from_le_bytes([blk[21], blk[22], blk[23], blk[24]]);
                crate::datei::pfad_text(unsafe { cstr(p, false) }, 0)
            } else {
                crate::datei::pfad_text(unsafe { cstr(ebx + 20, false) }, 0)
            };
            let offset = u64::from(off) | u64::from(hi) << 32;
            let (a, b) = match sub {
                0 => crate::datei::lesen(&pfad, offset, unsafe { bytes_mut(ptr, n) }),
                1 => {
                    let lang = if matches!(hi, 2 | 3) { 560 } else { 304 };
                    crate::datei::ordner_lesen(&pfad, off, n as u32, hi, unsafe { bytes_mut(ptr, 32 + n * lang) })
                }
                2 => crate::datei::erstellen(&pfad, unsafe { bytes(ptr, n) }),
                3 => crate::datei::schreiben(&pfad, offset, unsafe { bytes(ptr, n) }),
                4 => (crate::datei::groesse_setzen(&pfad, offset), r.ebx),
                5 => (crate::datei::info(&pfad, unsafe { bytes_mut(ptr, 40) }), r.ebx),
                6 => (crate::datei::attribute_setzen(&pfad, unsafe { bytes(ptr, 32) }), r.ebx),
                7 => {
                    let par = if hi == 0 { String::new() } else { String::from_utf8_lossy(unsafe { cstr(hi, false) }).trim_end_matches('\0').to_string() };
                    (crate::datei::starten(&pfad, &par) as u32, r.ebx)
                }
                8 => (crate::datei::loeschen(&pfad), r.ebx),
                9 => (crate::datei::ordner_erstellen(&pfad), r.ebx),
                10 => {
                    let neu = crate::datei::pfad_text(unsafe { cstr(ptr, false) }, 0);
                    (crate::datei::umbenennen(&pfad, &neu), r.ebx)
                }
                11 | 12 => (crate::datei::verknuepfung(), r.ebx),
                _ => (crate::datei::NICHT_UNTERSTUETZT, r.ebx),
            };
            r.eax = a;
            r.ebx = b;
        }
        74 => {
            r.eax = netz_geraet(ebx as u8 as i8, (ebx >> 8) as u8, ecx);
            // 74.8/74.9: 64-Bit-Bytezähler, oberes Dword in ebx
            if matches!(ebx as u8, 8 | 9) {
                r.ebx = 0;
            }
        }
        75 => {
            let (a, b) = unsafe { netz_socket(ebx as u8, ecx, edx, esi, edi) };
            r.eax = a;
            r.ebx = b;
        }
        76 => {
            let (a, b) = netz_optionen(ebx >> 16, (ebx >> 8) as u8, ebx as u8);
            r.eax = a;
            if let Some(b) = b {
                r.ebx = b;
            }
        }
        _ => r.eax = u32::MAX,
    }
}

/// Netzgeräte (Funktion 74): 0 = Loopback, 1 = WLAN (ESP32-C6), wenn
/// vorhanden. Typ 1 = Ethernet-artig (KolibriOS NET_TYPE_ETH).
fn netz_geraet(sub: i8, geraet: u8, ecx: u32) -> u32 {
    let wlan = crate::wlan::vorhanden();
    let anzahl = 1 + u32::from(wlan);
    if sub == -1 {
        return anzahl;
    }
    if u32::from(geraet) >= anzahl {
        return u32::MAX;
    }
    match sub {
        0 => u32::from(geraet == 1),
        1 => {
            let name: &[u8] = if geraet == 0 { b"loopback\0" } else { b"wlan0 (ESP32-C6)\0" };
            // SAFETY: Puffer mit 64 Byte (Vertrag 74.1).
            let ziel = unsafe { bytes_mut(ecx, 64) };
            let n = name.len().min(ziel.len());
            ziel[..n].copy_from_slice(&name[..n]);
            0
        }
        // Link-Status: Loopback immer, WLAN wenn verbunden (1 = aktiv möglich)
        10 => u32::from(geraet == 0 || crate::wlan::zustand().verbunden),
        // Zähler (6–9, 11–16) führt lwIP nicht je Gerät: 0
        2..=4 => u32::MAX,
        6..=9 | 11..=16 => 0,
        _ => u32::MAX,
    }
}

/// Sockets (Funktion 75): (eax, ebx) – eax = Ergebnis oder −1, ebx = Fehlercode.
///
/// # Safety
/// Zeiger in edx/esi folgen sysfuncs.txt (sockaddr, Puffer, optstruct).
unsafe fn netz_socket(sub: u8, ecx: u32, edx: u32, esi: u32, edi: u32) -> (u32, u32) {
    use crate::netz;
    let ok = |a: u32| (a, 0);
    let fehler = |e: u32| (u32::MAX, e);
    // sockaddr: mindestens 8 Byte (Familie, Port, IPv4), sonst EINVAL
    let adresse = || {
        if esi < 8 {
            return Err(netz::fehler::EINVAL);
        }
        netz::adresse_lesen(unsafe { bytes(edx, (esi as usize).min(16)) })
    };
    match sub {
        0 => netz::oeffnen(ecx, edx, esi).map_or_else(fehler, ok),
        1 => netz::schliessen(ecx).map_or_else(fehler, |()| ok(0)),
        2 => adresse().and_then(|a| netz::binden(ecx, a)).map_or_else(fehler, |()| ok(0)),
        3 => netz::lauschen(ecx, edx).map_or_else(fehler, |()| ok(0)),
        4 => adresse().and_then(|a| netz::verbinden(ecx, a)).map_or_else(fehler, |()| ok(0)),
        5 => match netz::annehmen(ecx) {
            Ok((neu, von)) => {
                if edx != 0 && esi >= 8 {
                    netz::adresse_schreiben(von, unsafe { bytes_mut(edx, (esi as usize).min(16)) });
                }
                ok(neu)
            }
            Err(e) => fehler(e),
        },
        6 => netz::senden(ecx, unsafe { bytes(edx, esi as usize) }, edi).map_or_else(fehler, |n| ok(n as u32)),
        7 => netz::empfangen(ecx, unsafe { bytes_mut(edx, esi as usize) }, edi).map_or_else(fehler, |n| ok(n as u32)),
        8 | 9 => {
            // optstruct: dd Ebene, dd Name, dd Länge, db Werte…
            let kopf = unsafe { bytes(edx, 12) };
            if kopf.len() < 12 {
                return fehler(netz::fehler::EINVAL);
            }
            let d = |i: usize| u32::from_le_bytes([kopf[4 * i], kopf[4 * i + 1], kopf[4 * i + 2], kopf[4 * i + 3]]);
            let (ebene, name, laenge) = (d(0), d(1), d(2));
            if sub == 8 {
                let wert = unsafe { bytes(edx + 12, (laenge as usize).min(16)) };
                netz::option_setzen(ecx, ebene, name, wert).map_or_else(fehler, |()| ok(0))
            } else {
                match netz::option_holen(ecx, ebene, name) {
                    Ok(v) => {
                        let ziel = unsafe { bytes_mut(edx + 8, 5) };
                        if ziel.len() == 5 {
                            ziel[..4].copy_from_slice(&1u32.to_le_bytes());
                            ziel[4] = v;
                        }
                        ok(0)
                    }
                    Err(e) => fehler(e),
                }
            }
        }
        // Socketpaare (lokale Röhre) gibt es mit lwIP nicht
        10 => fehler(netz::fehler::EOPNOTSUPP),
        _ => fehler(netz::fehler::EINVAL),
    }
}

/// Netzwerk-Optionen (Funktion 76): IPv4-Daten des WLAN-Geräts (Gerät 1)
/// und der Loopback (Gerät 0). Setzen ist nicht vorgesehen (DHCP).
/// Rückgabe: (eax, neues ebx).
fn netz_optionen(protokoll: u32, geraet: u8, sub: u8) -> (u32, Option<u32>) {
    let z = crate::wlan::zustand();
    let ip = |a: [u8; 4]| u32::from_le_bytes(a);
    let geraet_ok = geraet == 0 || (geraet == 1 && crate::wlan::vorhanden());
    if !geraet_ok {
        return (u32::MAX, None);
    }
    match (protokoll, sub) {
        // Ethernet: MAC (Loopback 0; WLAN: MAC des C6), eax = untere 4 Byte,
        // bx = obere 2 Byte
        (0, 0) if geraet == 0 => (0, Some(0)),
        (0, 0) => {
            let m = z.mac;
            (u32::from_le_bytes([m[0], m[1], m[2], m[3]]), Some(u32::from(u16::from_le_bytes([m[4], m[5]]))))
        }
        // IPv4: Zähler, Adressen (Netzwerk-Bytefolge im Speicher → LE-Wort)
        (1, 0 | 1) => (0, None),
        (1, 2) if geraet == 0 => (ip([127, 0, 0, 1]), None),
        (1, 2) => (if z.verbunden { ip(z.ip) } else { 0 }, None),
        (1, 4) => (if geraet == 1 && z.verbunden { ip(z.dns) } else { 0 }, None),
        (1, 6) if geraet == 0 => (ip([255, 0, 0, 0]), None),
        (1, 6) => (if z.verbunden { ip(z.maske) } else { 0 }, None),
        (1, 8) => (if geraet == 1 && z.verbunden { ip(z.gateway) } else { 0 }, None),
        // Setzen (3, 5, 7, 9): Adressen kommen per DHCP
        (1, 3 | 5 | 7 | 9) => (u32::MAX, None),
        // ICMP/UDP/TCP/ARP-Zähler: lwIP zählt nicht je Gerät
        (2..=5, 0 | 1) => (0, None),
        (5, 2) => (0, None),
        _ => (u32::MAX, None),
    }
}

/// Kodierung für 30.5 (esi = Kodierung; 0 → cp866).
fn edx_kodierung(k: u32) -> u32 {
    if matches!(k, 1..=3) { k } else { 1 }
}

/// Text in Kodierung 1/2/3 mit Nullende.
fn pfad_kodieren(s: &str, kod: u32) -> Vec<u8> {
    match kod {
        2 => s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect(),
        3 => s.bytes().chain([0]).collect(),
        _ => s.chars().map(crate::schrift::uni_zu_cp866).chain([0]).collect(),
    }
}

/// Bytes für `zeichen` UTF-8-Zeichen ab `p`.
fn utf8_laenge(p: u32, zeichen: usize) -> usize {
    let mut n = 0usize;
    for _ in 0..zeichen {
        // SAFETY: Programm liefert so viele gültige Zeichen.
        let b = unsafe { *((p as usize + n) as *const u8) };
        n += match b {
            0xF0.. => 4,
            0xE0.. => 3,
            0xC0.. => 2,
            _ => 1,
        };
    }
    n
}
