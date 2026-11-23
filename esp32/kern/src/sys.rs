//! KolibriOS-Systemfunktionen als typisierte Rust-Schnittstelle.
//!
//! Die Parameter folgen den Registern aus `sysfuncs.txt` (z. B. `ebx` =
//! x·65536 + Breite), damit die C-Schnittstelle (`abi.rs`, ksys.h) und
//! Rust-Programme dieselbe Logik nutzen. Jede Funktion prüft zuerst, ob der
//! Faden beendet werden soll (`pruefe_ende`).

use crate::fenster::{self, Fenster, Knopf, Welt, fenster_zeichnen, knopf_zeichnen};
use crate::grafik::{Clip, Puffer, Zeichner, zu565};
use crate::prozess::{self, Ende, eigene_tid, kern_abschnitt, pruefe_ende};
use crate::schrift::{self, Kodierung};
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

fn hoch(v: u32) -> i32 {
    (v >> 16) as i16 as i32
}
fn tief(v: u32) -> i32 {
    v as u16 as i16 as i32
}

/// Ruft `f` mit dem Fenster des laufenden Fadens und einem Zeichner auf, der
/// die Koordinaten wie KolibriOS auslegt (relativ zum Fenster oder – Flag B
/// – zum Arbeitsbereich).
/// Messung der Zeichenaufrufe (B5): Summe Warten auf die Welt-Sperre,
/// Summe Arbeit darin (µs) und Anzahl Aufrufe – abgeholt mit
/// [`zeichnen_messung_nehmen`].
static ZEICHNEN_WARTEN_US: AtomicU32 = AtomicU32::new(0);
static ZEICHNEN_ARBEIT_US: AtomicU32 = AtomicU32::new(0);
static ZEICHNEN_AUFRUFE: AtomicU32 = AtomicU32::new(0);

/// Misst beim Verlassen der Sperre die Arbeitszeit (auch bei frühem `?`).
struct ArbeitMessen(std::time::Instant);

impl Drop for ArbeitMessen {
    fn drop(&mut self) {
        ZEICHNEN_ARBEIT_US.fetch_add(self.0.elapsed().as_micros() as u32, Relaxed);
    }
}

/// (Warten µs, Arbeit µs, Aufrufe) seit dem letzten Abholen.
pub fn zeichnen_messung_nehmen() -> (u32, u32, u32) {
    (ZEICHNEN_WARTEN_US.swap(0, Relaxed), ZEICHNEN_ARBEIT_US.swap(0, Relaxed), ZEICHNEN_AUFRUFE.swap(0, Relaxed))
}

fn mit_fenster<R>(f: impl FnOnce(&mut Zeichner<'_>) -> R) -> Option<R> {
    pruefe_ende();
    let tid = eigene_tid()?;
    let t0 = std::time::Instant::now();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let t1 = std::time::Instant::now();
            ZEICHNEN_WARTEN_US.fetch_add((t1 - t0).as_micros() as u32, Relaxed);
            ZEICHNEN_AUFRUFE.fetch_add(1, Relaxed);
            let _messen = ArbeitMessen(t1);
            let skin = w.skin.clone();
            let fz = w.faden(tid)?;
            let fe = fz.fenster.as_mut()?;
            let (ax, ay, aw, ah) = fe.arbeitsbereich(&skin);
            let relativ = fe.flags & 2 != 0;
            let (bw, bh) = (fe.w, fe.h);
            let mut z = Zeichner::neu(&mut fe.buf);
            if relativ {
                z.ox = ax;
                z.oy = ay;
                z.clip = Clip { x0: ax, y0: ay, x1: ax + aw, y1: ay + ah };
            } else {
                z.clip = Clip { x0: 0, y0: 0, x1: bw, y1: bh };
            }
            let r = f(&mut z);
            let b = z.beruehrt;
            let (fx, fy) = (fe.x, fe.y);
            let sichtbar = !fe.minimiert;
            if sichtbar {
                w.markiere(b.verschoben(fx, fy));
            }
            Some(r)
        })
    })
}

fn mit_faden<R>(f: impl FnOnce(&mut Welt, usize) -> R) -> Option<R> {
    pruefe_ende();
    let tid = eigene_tid()?;
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let s = w.slot_von(tid)?;
            Some(f(w, s))
        })
    })
}

// ------------------------------------------------------------ Fenster (0)

/// Funktion 0: Fenster definieren und zeichnen.
pub fn fenster(ebx: u32, ecx: u32, edx: u32, esi: u32, titel: Option<&str>) {
    let stil = edx >> 24 & 0xF;
    if (5..=15).contains(&stil) {
        return;
    }
    mit_faden(|w, s| {
        let (sw, sh) = (w.w, w.h);
        let skin = w.skin.clone();
        let titel_farbe = w.farben[4];
        let neu = w.slots[s].as_ref().is_some_and(|f| f.fenster.is_none());
        if neu {
            let (mut x, mut y) = (hoch(ebx), hoch(ecx));
            let (mut fw, mut fh) = (tief(ebx) + 1, tief(ecx) + 1);
            if x < 0 || x + fw > sw {
                x = 0;
                if fw > sw {
                    fw = sw;
                }
            }
            if y < 0 || y + fh > sh {
                y = 0;
                if fh > sh {
                    fh = sh;
                }
            }
            let fw = fw.max(1);
            let fh = fh.max(1);
            let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
            fz.fenster = Some(Fenster {
                x,
                y,
                w: fw,
                h: fh,
                stil,
                flags: edx >> 28,
                arbeit: edx & 0x80FF_FFFF,
                kopf: esi,
                titel: titel.unwrap_or("").into(),
                buf: Puffer::neu(fw as usize, fh as usize, edx & 0xFF_FFFF),
                knoepfe: Vec::new(),
                minimiert: false,
                vor_max: None,
                immer_oben: false,
                unbeweglich: esi >> 24 & 0xF == 1,
                ebene: 0,
                zeiger: 0,
            });
            // aktivieren reiht ein und zeichnet den Kopf des bisher aktiven Fensters inaktiv
            w.aktivieren(s);
        }
        let aktiv = w.aktiv() == Some(s);
        let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
        fz.redraw = false;
        let f = fz.fenster.as_mut().unwrap_or_else(|| unreachable!());
        f.stil = stil;
        f.flags = edx >> 28;
        f.arbeit = if edx & 0x8000_0000 != 0 { edx & 0xFF_FFFF | 0x8000_0000 } else { edx & 0xFF_FFFF };
        f.kopf = esi;
        if f.titel.is_empty()
            && let Some(t) = titel
        {
            f.titel = t.into();
        }
        fenster_zeichnen(f, &skin, aktiv, titel_farbe);
        f.knoepfe.retain(|k| !k.system);
        if matches!(stil, 3 | 4) {
            let fw = f.w;
            for (b, id) in [(skin.knopf_zu, 1), (skin.knopf_min, 0xFFFF)] {
                let x = if b.0 < 0 { fw + b.0 } else { b.0 };
                f.knoepfe.push(Knopf { id, x, y: b.1, w: b.2, h: b.3, kein_rahmen: false, system: true });
            }
        }
        w.markiere_fenster(s);
    });
}

/// Funktion 71.1: Fenstertitel setzen.
pub fn titel(t: &str) {
    mit_faden(|w, s| {
        if let Some(f) = w.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) {
            f.titel = t.into();
        }
        w.kopf_neu(s);
        w.markiere_fenster(s);
    });
}

/// Funktion 67: Fenster verschieben/Grösse ändern (−1 = unverändert).
pub fn fenster_bewegen(x: i32, y: i32, bw: i32, bh: i32) {
    mit_faden(|w, s| {
        let skin = w.skin.clone();
        let titel_farbe = w.farben[4];
        let aktiv = w.aktiv() == Some(s);
        let (sw, sh) = (w.w, w.h);
        let alt = w.fenster_rechteck(s);
        w.markiere(alt);
        let fz = w.slots[s].as_mut()?;
        let f = fz.fenster.as_mut()?;
        let nx = if x == -1 { f.x } else { x };
        let ny = if y == -1 { f.y } else { y };
        let nw = if bw == -1 { f.w } else { (bw + 1).clamp(1, sw) };
        let nh = if bh == -1 { f.h } else { (bh + 1).clamp(1, sh) };
        let groesse = nw != f.w || nh != f.h;
        f.x = nx;
        f.y = ny;
        if groesse {
            f.w = nw;
            f.h = nh;
            f.buf = Puffer::neu(nw as usize, nh as usize, f.arbeit & 0xFF_FFFF);
            fenster_zeichnen(f, &skin, aktiv, titel_farbe);
            fz.redraw = true;
        }
        w.markiere_fenster(s);
        fenster::wecken();
        Some(())
    });
}

/// Funktion 12: 1 = Neuzeichnen beginnt (löscht Knöpfe), 2 = Ende.
pub fn neuzeichnen(sub: u32) {
    if sub == 1 {
        mit_faden(|w, s| {
            if let Some(f) = w.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) {
                f.knoepfe.clear();
            }
        });
    } else {
        pruefe_ende();
    }
}

// --------------------------------------------------------- Zeichnen

/// Funktion 1.
pub fn pixel(x: i32, y: i32, farbe: u32) {
    mit_fenster(|z| z.pixel(x, y, farbe));
}

/// Funktion 13: `ebx` = x·65536 + Breite, `ecx` = y·65536 + Höhe.
pub fn rechteck(ebx: u32, ecx: u32, farbe: u32) {
    mit_fenster(|z| z.rechteck(hoch(ebx), hoch(ecx), tief(ebx), tief(ecx), farbe));
}

/// Funktion 38: `ebx` = x0·65536 + x1, `ecx` = y0·65536 + y1.
pub fn linie(ebx: u32, ecx: u32, farbe: u32) {
    mit_fenster(|z| z.linie(hoch(ebx), hoch(ecx), tief(ebx), tief(ecx), farbe));
}

/// Funktion 7: Bild BGR (3 Byte/Pixel), `ecx` = w·65536 + h, `edx` = x·65536 + y.
pub fn bild(data: &[u8], ecx: u32, edx: u32) {
    mit_fenster(|z| z.bild(data, hoch(ecx), tief(ecx), hoch(edx), tief(edx), 24, &[], 0));
}

/// Funktion 65: Bild mit `bpp` (1,2,4,8,9,15,16,24,32), Palette, Zeilen-Zusatz `ebp`.
pub fn palettenbild(data: &[u8], ecx: u32, edx: u32, bpp: u32, palette: &[u32], ebp: i32) {
    mit_fenster(|z| z.bild(data, hoch(ecx), tief(ecx), hoch(edx), tief(edx), bpp, palette, ebp));
}

/// Funktion 73 (ROP 0): `p` = [zx, zy, zw, zh, qx, qy, qw, qh], Quelle 32 bpp.
pub fn blit(ebx: u32, p: [i32; 8], data: &[u8], zeile: usize) {
    let transparent = ebx & 0x20 != 0;
    let relativ = ebx & 0x2000_0000 != 0;
    if ebx & 0xF != 0 {
        return;
    }
    mit_fenster(|z| {
        // CR: relativ zum Arbeitsbereich, sonst zum Fenster
        let (ox, oy) = (z.ox, z.oy);
        if !relativ {
            z.ox = 0;
            z.oy = 0;
        }
        z.blit32((p[0], p[1], p[2], p[3]), (p[4], p[5], p[6], p[7]), data, zeile, transparent);
        z.ox = ox;
        z.oy = oy;
    });
}

/// Text in Rechteck-Aufrufe umsetzen (Nutzerpuffer, Funktion 4 mit C=1).
fn text_rects(ebx: u32, ecx: u32, text: &[u8], esi: u32, mut rect: impl FnMut(i32, i32, i32, i32)) {
    let k = Kodierung::aus_bits(ecx >> 28);
    let null = ecx & 0x8000_0000 != 0;
    let max = if null { usize::MAX } else { esi as usize };
    let mult = (ecx >> 24 & 7) as i32 + 1;
    let zeichen = schrift::zeichen(text, k, null, max);
    schrift::zeichnen(&zeichen, k, mult, hoch(ebx), tief(ebx), &mut rect, None);
}

/// Funktion 4: Text ins Fenster. `ecx` = Flags+Farbe, `esi` = Länge (A=0),
/// `edi` = Hintergrundfarbe (B=1).
pub fn text(ebx: u32, ecx: u32, t: &[u8], esi: u32, edi: u32) {
    let k = Kodierung::aus_bits(ecx >> 28);
    let null = ecx & 0x8000_0000 != 0;
    let max = if null { usize::MAX } else { esi as usize };
    let mult = (ecx >> 24 & 7) as i32 + 1;
    let zeichen = schrift::zeichen(t, k, null, max);
    let farbe = zu565(ecx);
    let hg = (ecx & 0x4000_0000 != 0).then(|| zu565(edi));
    mit_fenster(|z| {
        if let Some(h) = hg {
            let (zw, zh) = k.zelle();
            z.fuellen565(hoch(ebx), tief(ebx), zw * mult * zeichen.len() as i32, zh * mult, h);
        }
        schrift::zeichnen(&zeichen, k, mult, hoch(ebx), tief(ebx), &mut |x, y, w, h| z.fuellen565(x, y, w, h, farbe), None);
    });
}

/// Funktion 4 mit C=1: Text in Nutzerpuffer `[Xsize: u32, Ysize: u32, 32-bpp-Pixel…]`.
pub fn text_in_puffer(ebx: u32, ecx: u32, t: &[u8], esi: u32, puffer: &mut [u8]) {
    pruefe_ende();
    if puffer.len() < 8 {
        return;
    }
    let bw = u32::from_le_bytes([puffer[0], puffer[1], puffer[2], puffer[3]]) as i32;
    let bh = u32::from_le_bytes([puffer[4], puffer[5], puffer[6], puffer[7]]) as i32;
    let farbe = (ecx & 0xFF_FFFF).to_le_bytes();
    let px = &mut puffer[8..];
    text_rects(ebx, ecx, t, esi, |x, y, w, h| {
        for yy in y.max(0)..(y + h).min(bh) {
            for xx in x.max(0)..(x + w).min(bw) {
                let o = (yy * bw + xx) as usize * 4;
                if let Some(p) = px.get_mut(o..o + 4) {
                    p[..3].copy_from_slice(&farbe[..3]);
                }
            }
        }
    });
}

/// Funktion 47: Zahl zeichnen. `wert` = Zahl (bl=0) bzw. gelesener Wert.
pub fn zahl(ebx: u32, wert: u64, edx: u32, esi: u32, edi: u32) {
    let t = schrift::zahl_text(wert, ebx);
    // Schrift F (Bit 28) → 0 = 6×9, 1 = 8×16 (cp866)
    let ff = (esi >> 28) & 1;
    // Text-Flags: B (Bit 30) und SSS übernehmen, FF aus F, C entfällt
    let ecx = esi & 0x4700_0000 | ff << 28 | esi & 0xFF_FFFF;
    text(edx, ecx, &t, t.len() as u32, edi);
}

/// Funktion 8: Knopf definieren (`edx` Bit 31 = löschen).
pub fn knopf(ebx: u32, ecx: u32, edx: u32, esi: u32) {
    let id = edx & 0xFF_FFFF;
    if edx & 0x8000_0000 != 0 {
        mit_faden(|w, s| {
            if let Some(f) = w.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) {
                f.knoepfe.retain(|k| k.id != id || k.system);
            }
        });
        return;
    }
    let zeichnen = edx & 0x4000_0000 == 0;
    let kein_rahmen = edx & 0x2000_0000 != 0;
    mit_faden(|w, s| {
        let skin = w.skin.clone();
        let Some(f) = w.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) else { return };
        let (ax, ay, _, _) = f.arbeitsbereich(&skin);
        let (ox, oy) = if f.flags & 2 != 0 { (ax, ay) } else { (0, 0) };
        let (x, y, bw, bh) = (hoch(ebx) + ox, hoch(ecx) + oy, tief(ebx) + 1, tief(ecx) + 1);
        if bw <= 0 || bh <= 0 || bw >= 0x8000 || bh >= 0x8000 {
            return;
        }
        // Gleicher Knopf (Kennung und Rechteck) noch einmal: ersetzen statt
        // doppelt anlegen – so zeichnet ein Programm einen Knopf neu, ohne ihn
        // vorher zu löschen (sonst träfe ein Klick dazwischen ins Leere).
        // Gleiche Kennung an anderer Stelle bleibt ein weiterer Knopf (KolibriOS).
        match f.knoepfe.iter_mut().find(|k| !k.system && k.id == id && (k.x, k.y, k.w, k.h) == (x, y, bw, bh)) {
            Some(k) => k.kein_rahmen = kein_rahmen,
            None => f.knoepfe.push(Knopf { id, x, y, w: bw, h: bh, kein_rahmen, system: false }),
        }
        if zeichnen {
            knopf_zeichnen(&mut Zeichner::neu(&mut f.buf), x, y, bw, bh, esi & 0xFF_FFFF);
            // nur der Knopf hat sich geändert (Tastatur: eine Taste neu)
            let (fx, fy, sichtbar) = (f.x, f.y, !f.minimiert);
            if sichtbar {
                w.markiere(Clip { x0: fx + x, y0: fy + y, x1: fx + x + bw, y1: fy + y + bh });
            }
        }
    });
}

// --------------------------------------------------------- Ereignisse

fn ereignis_holen(w: &mut Welt, s: usize) -> u32 {
    let aktiv = w.aktiv() == Some(s);
    let Some(fz) = w.slots[s].as_mut() else { return 0 };
    let m = fz.maske;
    if m & 1 != 0 && fz.redraw {
        return 1;
    }
    if m & 2 != 0 && aktiv && !fz.tasten.is_empty() {
        return 2;
    }
    if m & 4 != 0 && fz.knopf.is_some() {
        return 3;
    }
    if m & 16 != 0 && fz.hg_ev {
        fz.hg_ev = false;
        return 5;
    }
    if m & 32 != 0 && fz.maus_ev {
        fz.maus_ev = false;
        return 6;
    }
    if m & 64 != 0 && fz.ipc_ev {
        fz.ipc_ev = false;
        return 7;
    }
    if m & 128 != 0 && fz.netz_ev {
        fz.netz_ev = false;
        return 8;
    }
    0
}

/// Wartet bis zu `ms` (None = unbegrenzt) auf ein Ereignis.
fn warten_auf(ms: Option<u64>) -> u32 {
    let Some(tid) = eigene_tid() else { return 0 };
    let ende = ms.map(|m| std::time::Instant::now() + std::time::Duration::from_millis(m));
    loop {
        pruefe_ende();
        let r = kern_abschnitt(|| {
            let mut g = fenster::welt();
            let w = g.get_or_insert_with(|| Welt::neu(640, 360));
            let Some(s) = w.slot_von(tid) else { return Some(0) };
            let e = ereignis_holen(w, s);
            if e != 0 {
                return Some(e);
            }
            let rest = match ende {
                Some(t) => {
                    let jetzt = std::time::Instant::now();
                    if jetzt >= t {
                        return Some(0);
                    }
                    (t - jetzt).as_millis() as u64
                }
                None => 50,
            };
            drop(fenster::schlafen(g, rest.clamp(1, 50)));
            None
        });
        if let Some(e) = r {
            return e;
        }
    }
}

/// Funktion 10.
pub fn warte_ereignis() -> u32 {
    warten_auf(None)
}

/// Funktion 11.
pub fn pruefe_ereignis() -> u32 {
    mit_faden(ereignis_holen).unwrap_or(0)
}

/// Funktion 23: Zeit in 1/100 s (0 = sofort 0).
pub fn warte_ereignis_max(hundertstel: u32) -> u32 {
    if hundertstel == 0 {
        pruefe_ende();
        return 0;
    }
    warten_auf(Some(u64::from(hundertstel) * 10))
}

/// Funktion 40: Ereignismaske setzen, liefert die alte.
pub fn maske(ebx: u32) -> u32 {
    mit_faden(|w, s| {
        let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
        std::mem::replace(&mut fz.maske, ebx)
    })
    .unwrap_or(7)
}

/// Ein Textfeld (box_lib edit_box) des laufenden Fadens bekommt bzw.
/// verliert den Eingabefokus – Anlass für die Bildschirmtastatur (D8).
pub fn textfeld_fokus(an: bool) {
    let Some(tid) = eigene_tid() else { return };
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let vorher = w.textfokus;
            if an {
                w.textfokus = Some(tid);
            } else if w.textfokus == Some(tid) {
                w.textfokus = None;
                w.textfeld = None;
            }
            if vorher != w.textfokus {
                w.tastatur_manuell = None;
            }
        })
    });
    fenster::wecken();
}

/// Lage des fokussierten Textfelds (Koordinaten wie die Zeichenfunktionen
/// des Fensters), damit die Bildschirmtastatur es nicht verdeckt.
pub fn textfeld_lage(x: i32, y: i32, w: i32, h: i32) {
    let Some(tid) = eigene_tid() else { return };
    kern_abschnitt(|| {
        fenster::mit_welt(|wl| {
            if wl.textfokus != Some(tid) {
                return;
            }
            let skin = wl.skin.clone();
            let Some(f) = wl.faden(tid).and_then(|z| z.fenster.as_ref()) else { return };
            let (ax, ay) = if f.flags & 2 != 0 {
                let (ax, ay, _, _) = f.arbeitsbereich(&skin);
                (ax, ay)
            } else {
                (0, 0)
            };
            let (sx, sy) = (f.x + ax + x, f.y + ay + y);
            wl.textfeld = Some(Clip { x0: sx, y0: sy, x1: sx + w, y1: sy + h });
        })
    });
}

/// Bildschirmtastatur von Hand ein- bzw. ausblenden (Taskleiste).
pub fn tastatur_umschalten() {
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let jetzt = w.tastatur_manuell.unwrap_or(w.textfokus.is_some());
            w.tastatur_manuell = Some(!jetzt);
        })
    });
    fenster::wecken();
}

/// Soll die Bildschirmtastatur sichtbar sein? Und wo ist das Textfeld?
pub fn tastatur_soll() -> (bool, Option<Clip>) {
    kern_abschnitt(|| fenster::mit_welt(|w| (w.tastatur_manuell.unwrap_or(w.textfokus.is_some()), w.textfeld)))
}

/// Faden mit Textfokus (None = keiner).
pub fn textfeld_mit_fokus() -> Option<u32> {
    kern_abschnitt(|| fenster::mit_welt(|w| w.textfokus))
}

/// Funktion 2: Taste holen (1 = Puffer leer).
pub fn taste() -> u32 {
    mit_faden(|w, s| {
        if w.aktiv() != Some(s) {
            return 1;
        }
        w.slots[s].as_mut().and_then(|f| f.tasten.pop_front()).unwrap_or(1)
    })
    .unwrap_or(1)
}

/// Funktion 17: Knopf holen (1 = Puffer leer).
pub fn knopf_id() -> u32 {
    mit_faden(|w, s| w.slots[s].as_mut().and_then(|f| f.knopf.take()).unwrap_or(1)).unwrap_or(1)
}

/// Zoom-Geste abholen (ESP32-OS-Erweiterung): (Faktor in Tausendstel seit
/// dem letzten Abholen, Mittelpunkt relativ zum Arbeitsbereich).
pub fn zoom_holen() -> (u32, i32, i32) {
    mit_faden(|w, s| {
        let skin = w.skin.clone();
        let (fx, fy) = w.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).map_or((0, 0), |f| {
            let (ax, ay, _, _) = f.arbeitsbereich(&skin);
            (f.x + ax, f.y + ay)
        });
        let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
        let (z, x, y) = std::mem::replace(&mut fz.zoom, (1000, 0, 0));
        (z, x - fx, y - fy)
    })
    .unwrap_or((1000, 0, 0))
}

/// ESP32-OS (B8): Lage des letzten Drückens einer Maustaste seit dem
/// letzten Abholen, relativ zum Arbeitsbereich (wie 37.1) – für Programme,
/// die Lage und Tasten nur abfragen (Browser: Markieren beginnt dort, wo der
/// Finger aufsetzte, nicht wo er beim nächsten Abfragen schon ist).
pub fn druck_lage_holen() -> Option<(i32, i32)> {
    mit_faden(|w, s| {
        let skin = w.skin.clone();
        let (fx, fy) = w.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).map_or((0, 0), |f| {
            let (ax, ay, _, _) = f.arbeitsbereich(&skin);
            (f.x + ax, f.y + ay)
        });
        let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
        fz.druck_lage.take().map(|(x, y)| (x - fx, y - fy))
    })
    .flatten()
}

/// Funktion 37.4: Zeiger laden (Bild schon gelesen); liefert Griff oder 0.
pub fn zeiger_laden(bild: Option<crate::zeiger::Zeigerbild>) -> u32 {
    let Some(b) = bild else { return 0 };
    let Some(tid) = eigene_tid() else { return 0 };
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let b = b.clone();
            match w.zeiger.iter().position(Option::is_none) {
                Some(i) => {
                    w.zeiger[i] = Some((tid, b));
                    i as u32 + 1
                }
                None => {
                    w.zeiger.push(Some((tid, b)));
                    w.zeiger.len() as u32
                }
            }
        })
    })
}

/// Funktion 37.5: Zeiger des eigenen Fensters setzen; liefert den alten.
pub fn zeiger_setzen(griff: u32) -> u32 {
    mit_faden(|w, s| {
        let gueltig = griff.checked_sub(1).and_then(|i| w.zeiger.get(i as usize)).is_some_and(Option::is_some);
        if w.touchpad {
            let r = w.zeiger_rechteck();
            w.markiere(r);
        }
        let f = w.slots[s].as_mut()?.fenster.as_mut()?;
        let alt = std::mem::replace(&mut f.zeiger, if gueltig { griff } else { 0 });
        if w.touchpad {
            let r = w.zeiger_rechteck();
            w.markiere(r);
        }
        Some(alt)
    })
    .flatten()
    .unwrap_or(0)
}

/// Funktion 37.6: eigenen Zeiger löschen.
pub fn zeiger_loeschen(griff: u32) {
    let Some(tid) = eigene_tid() else { return };
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            if let Some(z) = griff.checked_sub(1).and_then(|i| w.zeiger.get_mut(i as usize))
                && z.as_ref().is_some_and(|(t, _)| *t == tid)
            {
                if w.touchpad {
                    let r = w.zeiger_rechteck();
                    w.markiere(r);
                }
                let z = &mut w.zeiger[(griff - 1) as usize];
                *z = None;
                for f in w.slots.iter_mut().flatten().filter_map(|f| f.fenster.as_mut()) {
                    if f.zeiger == griff {
                        f.zeiger = 0;
                    }
                }
            }
        })
    });
}

/// Funktion 66.1/66.2: Tastaturmodus (0 ASCII, 1 Scancodes).
pub fn tastatur_modus(sub: u32, ecx: u32) -> u32 {
    mit_faden(|w, s| {
        let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
        if sub == 1 {
            fz.tasten_modus = ecx;
        }
        fz.tasten_modus
    })
    .unwrap_or(0)
}

// ------------------------------------------------------------- Maus (37)

/// Funktion 37 (Unterfunktionen 0, 1, 2, 3, 7).
pub fn maus(sub: u32) -> u32 {
    mit_faden(|w, s| {
        let m = w.maus;
        match sub {
            0 => (m.x as u32) << 16 | m.y as u32 & 0xFFFF,
            1 => {
                // Wie der KolibriOS-Kern (gui/mouse.inc, .mswin): relativ zur
                // clientbox – bei Fenstern ohne Flag „relativ“ ist das das
                // ganze Fenster (window.inc), sonst der Arbeitsbereich. Z3:
                // PasswordGen (Stil 0x14) traf vorher seine Checkboxen nicht.
                let skin = w.skin.clone();
                let (fx, fy) = w.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).map_or((0, 0), |f| {
                    let (ax, ay, _, _) = f.clientbox(&skin);
                    (f.x + ax, f.y + ay)
                });
                (((m.x - fx) as u32) << 16).wrapping_add((m.y - fy) as u32)
            }
            2 => m.tasten & 0x1F,
            3 => {
                let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
                m.tasten & 0x1F | std::mem::take(&mut fz.maus_ereignisse)
            }
            7 => {
                let fz = w.slots[s].as_mut().unwrap_or_else(|| unreachable!());
                let (dx, dy) = std::mem::take(&mut fz.scroll);
                (dx as u32) << 16 | dy as u32 & 0xFFFF
            }
            _ => 0,
        }
    })
    .unwrap_or(0)
}

// ------------------------------------------------------- Zeit, System

/// Funktion 3.
pub fn uhrzeit() -> u32 {
    pruefe_ende();
    crate::zeit::uhrzeit_bcd()
}

/// Funktion 29.
pub fn datum() -> u32 {
    pruefe_ende();
    crate::zeit::datum_bcd()
}

/// Funktion 26.9.
pub fn zeitzaehler() -> u32 {
    pruefe_ende();
    crate::zeit::hundertstel()
}

/// Funktion 26.10.
pub fn zeit_ns() -> u64 {
    pruefe_ende();
    crate::zeit::nanos()
}

/// Funktion 5: warten (1/100 s); 0 tut nichts.
pub fn warten(hundertstel: u32) {
    pruefe_ende();
    let mut rest = u64::from(hundertstel) * 10;
    while rest > 0 {
        let s = rest.min(50);
        std::thread::sleep(std::time::Duration::from_millis(s));
        rest -= s;
        pruefe_ende();
    }
}

/// Funktion 14: (Breite−1)·65536 + (Höhe−1).
pub fn bildschirmgroesse() -> u32 {
    pruefe_ende();
    kern_abschnitt(|| fenster::mit_welt(|w| ((w.w - 1) as u32) << 16 | (w.h - 1) as u32))
}

/// Funktion 35: Bildpunkt des Bildschirms (Versatz = y·Breite + x).
pub fn bildpunkt(versatz: u32) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let mut p = Puffer::neu(w.w as usize, w.h as usize, 0);
            w.komponieren(&mut p);
            p.px.get(versatz as usize).map_or(0, |&c| crate::grafik::von565(c))
        })
    })
}

/// Funktion 48 (Unterfunktionen 0, 2, 3, 4, 5, 6, 7). `buf` für 2/3 (Farbtabelle).
pub fn stil(sub: u32, ecx: u32, edx: u32, buf: &mut [u8]) -> (u32, u32) {
    pruefe_ende();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| match sub {
            0 => {
                for fz in w.slots.iter_mut().flatten() {
                    fz.redraw = true;
                }
                fenster::wecken();
                (0, 0)
            }
            2 => {
                for (i, c) in buf.chunks_exact(4).take(10).enumerate() {
                    w.farben[i] = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                }
                (0, 0)
            }
            3 => {
                let n = (edx as usize).min(buf.len()).min(40);
                let mut b = [0u8; 40];
                for (i, c) in w.farben.iter().enumerate() {
                    b[i * 4..i * 4 + 4].copy_from_slice(&c.to_le_bytes());
                }
                buf[..n].copy_from_slice(&b[..n]);
                (0, 0)
            }
            4 => (w.skin.hoehe as u32, 0),
            5 => {
                let (l, o, r, u) = w.arbeitsflaeche;
                ((l as u32) << 16 | r as u32, (o as u32) << 16 | u as u32)
            }
            6 => {
                let (l, r, o, u) = (hoch(ecx), tief(ecx), hoch(edx), tief(edx));
                w.arbeitsflaeche = (l.max(0), o.max(0), r.min(w.w - 1), u.min(w.h - 1));
                (0, 0)
            }
            7 => {
                let (l, o, r, u) = w.skin.raender;
                ((l as u32) << 16 | r as u32, (o as u32) << 16 | u as u32)
            }
            _ => (0, 0),
        })
    })
}

/// Funktion 48.8 (aus Speicher): Skin-Datei setzen, Touch-Faktor wie bisher.
/// Liefert 0 = ok, 1 = Fehler.
pub fn skin_setzen(daten: &[u8]) -> u32 {
    pruefe_ende();
    let s = kern_abschnitt(|| fenster::mit_welt(|w| w.skin.skala));
    u32::from(crate::skin::anwenden(daten, s).is_err())
}

/// Funktion 9: Fadeninfo in `buf` (1 KB); `slot` −1 = eigener. Liefert den
/// höchsten Slot.
pub fn faden_info(slot: i32, buf: &mut [u8; 1024]) -> i32 {
    pruefe_ende();
    let tid = eigene_tid();
    let infos = crate::prozess::liste();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let s = if slot == -1 { tid.and_then(|t| w.slot_von(t)).unwrap_or(1) } else { slot.max(0) as usize };
            let hoechster = (w.slots.len() - 1) as i32;
            buf[..76].fill(0);
            let put32 = |b: &mut [u8; 1024], o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
            let put16 = |b: &mut [u8; 1024], o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
            let stapelpos = w.stapel.iter().position(|&x| x == s).map_or(0, |p| p + 2);
            put16(buf, 4, stapelpos as u16);
            let an_pos = slot.max(0) as usize;
            let slot_an = if an_pos >= 2 { w.stapel.get(an_pos - 2).copied().unwrap_or(0) } else { 1 };
            put16(buf, 6, slot_an as u16);
            if s == 1 {
                buf[10..21].copy_from_slice(b"IDLE       ");
                put32(buf, 26, 0x100_0000);
                put32(buf, 30, 1);
                return hoechster;
            }
            let Some(fz) = w.slots.get(s).and_then(Option::as_ref) else {
                put16(buf, 50, 9);
                return hoechster;
            };
            let mut name = [b' '; 11];
            for (d, c) in name.iter_mut().zip(fz.name.bytes()) {
                *d = c;
            }
            buf[10..21].copy_from_slice(&name);
            let mem = infos.iter().find(|p| p.pid == fz.pid).map_or(0, |p| p.heap_groesse);
            put32(buf, 26, mem.saturating_sub(1) as u32);
            put32(buf, 30, fz.tid);
            if let Some(f) = &fz.fenster {
                let (ax, ay, aw, ah) = f.clientbox(&w.skin);
                for (o, v) in [(34, f.x), (38, f.y), (42, f.w - 1), (46, f.h - 1), (54, ax), (58, ay), (62, aw), (66, ah)] {
                    put32(buf, o, v as u32);
                }
                buf[70] = u8::from(f.vor_max.is_some()) | u8::from(f.minimiert) << 1;
            }
            put16(buf, 50, if fz.wartet { 5 } else { 0 });
            buf[71..75].copy_from_slice(&fz.maske.to_le_bytes());
            buf[75] = fz.tasten_modus as u8;
            hoechster
        })
    })
}

/// Funktion 18 (Unterfunktionen 2, 3, 7, 13, 16, 17, 18, 21).
pub fn system(sub: u32, ecx: u32) -> u32 {
    pruefe_ende();
    match sub {
        2 | 18 => {
            let pid = if sub == 2 {
                kern_abschnitt(|| fenster::mit_welt(|w| w.slots.get(ecx as usize).and_then(|f| f.as_ref()).map(|f| f.pid)))
            } else {
                Some(ecx)
            };
            match pid {
                Some(p) if crate::prozess::beende(p) => 0,
                _ => u32::MAX,
            }
        }
        3 => {
            kern_abschnitt(|| {
                fenster::mit_welt(|w| {
                    let s = ecx as usize;
                    if w.slots.get(s).and_then(|f| f.as_ref()).is_some_and(|f| f.fenster.is_some()) {
                        w.aktivieren(s);
                    }
                })
            });
            0
        }
        7 => kern_abschnitt(|| fenster::mit_welt(|w| w.aktiv().unwrap_or(1) as u32)),
        // Kernversion: 0.7.7.0 + Revision; ESP32-OS meldet 1.0.0.0
        13 => 0x0001_0000,
        16 | 17 => {
            let infos = crate::prozess::liste();
            let groesse: usize = infos.iter().map(|p| p.heap_groesse).sum();
            let belegt: usize = infos.iter().map(|p| p.heap_belegt).sum();
            if sub == 16 { ((groesse - belegt) / 1024) as u32 } else { (groesse / 1024) as u32 }
        }
        21 => kern_abschnitt(|| {
            fenster::mit_welt(|w| {
                w.slots.iter().position(|f| f.as_ref().is_some_and(|f| f.tid == ecx)).map_or(0, |s| s as u32)
            })
        }),
        _ => u32::MAX,
    }
}

/// Funktion 18.22: Fenster minimieren (ecx 0 = Slot, 1 = PID) oder
/// wiederherstellen (2 = Slot, 3 = PID). Liefert 0 oder −1.
pub fn minimieren(ecx: u32, edx: u32) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let s = if ecx & 1 == 0 { Some(edx as usize) } else { w.slots.iter().position(|f| f.as_ref().is_some_and(|f| f.pid == edx && f.fenster.is_some())) };
            let Some(s) = s.filter(|&s| w.slots.get(s).and_then(|f| f.as_ref()).is_some_and(|f| f.fenster.is_some())) else { return u32::MAX };
            if ecx < 2 {
                w.markiere_fenster(s);
                if let Some(f) = w.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) {
                    f.minimiert = true;
                }
                if let Some(a) = w.aktiv() {
                    w.kopf_neu(a);
                    w.markiere_fenster(a);
                }
            } else {
                w.aktivieren(s);
            }
            0
        })
    })
}

/// Alle Fenster neu zeichnen lassen (ESP32-OS: Sprache, Hintergrund).
pub fn alle_neu_zeichnen() {
    kern_abschnitt(|| fenster::mit_welt(|w| w.alle_neu_zeichnen()));
}

/// Funktion 18.25: Fensterebene lesen (ecx 1) oder setzen (ecx 2, esi).
/// `edx` = PID oder −1 (eigenes Fenster).
pub fn fensterebene(ecx: u32, edx: u32, esi: u32) -> u32 {
    pruefe_ende();
    let eigen = eigene_tid();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            let s = if edx == u32::MAX {
                eigen.and_then(|t| w.slot_von(t))
            } else {
                w.slots.iter().position(|f| f.as_ref().is_some_and(|f| f.pid == edx && f.fenster.is_some()))
            };
            let Some(s) = s else { return 0 };
            if ecx == 1 {
                w.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).map_or(0, |f| f.ebene as u32)
            } else {
                u32::from(w.ebene_setzen(s, esi as i32))
            }
        })
    })
}

/// Fensterliste (ESP32-OS-Erweiterung für die Taskleiste).
pub fn fenster_liste() -> Vec<fenster::FensterInfo> {
    pruefe_ende();
    kern_abschnitt(|| fenster::mit_welt(|w| w.fenster_liste()))
}

/// Funktion 15: Hintergrund. `daten` für 15.5 (BGR), Rückgabe für 15.6/15.8.
pub fn hintergrund(sub: u32, ecx: u32, edx: u32, daten: &[u8]) -> (u32, u32) {
    pruefe_ende();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| match sub {
            1 => {
                // Grenze wie ein grosses Hintergrundbild (System-Heap schützen)
                let n = (ecx as usize).checked_mul(edx as usize).and_then(|p| p.checked_mul(3));
                match n {
                    Some(n) if ecx <= 4096 && edx <= 4096 && n <= 16 << 20 => {
                        w.hg_groesse = (ecx, edx);
                        w.hg_daten = vec![0; n];
                    }
                    _ => {}
                }
                (0, 0)
            }
            2 => {
                let o = ecx as usize;
                if o + 3 <= w.hg_daten.len() {
                    w.hg_daten[o..o + 3].copy_from_slice(&edx.to_le_bytes()[..3]);
                }
                (0, 0)
            }
            3 => {
                w.hintergrund_rendern();
                (0, 0)
            }
            4 => {
                w.hg_modus = if ecx == 1 { 1 } else { 2 };
                (0, 0)
            }
            5 => {
                let o = edx as usize;
                if o < w.hg_daten.len() {
                    let n = daten.len().min(w.hg_daten.len() - o);
                    w.hg_daten[o..o + n].copy_from_slice(&daten[..n]);
                }
                (0, 0)
            }
            6 => (w.hg_daten.as_mut_ptr() as usize as u32, 0),
            7 => (1, 0),
            8 => {
                let c = w.hg_zuletzt;
                (((c.x0.max(0) as u32) << 16) | (c.x1 - 1).max(0) as u32, ((c.y0.max(0) as u32) << 16) | (c.y1 - 1).max(0) as u32)
            }
            9 => {
                w.hintergrund_rendern();
                w.hg_zuletzt = Clip { x0: hoch(ecx), y0: hoch(edx), x1: tief(ecx) + 1, y1: tief(edx) + 1 };
                (0, 0)
            }
            _ => (u32::MAX, 0),
        })
    })
}

/// Funktion 39: Hintergrund lesen (1 Grösse, 2 Pixel, 4 Modus).
pub fn hintergrund_lesen(sub: u32, ecx: u32) -> u32 {
    pruefe_ende();
    kern_abschnitt(|| {
        fenster::mit_welt(|w| match sub {
            1 => w.hg_groesse.0 << 16 | w.hg_groesse.1,
            2 => {
                let o = ecx as usize;
                w.hg_daten.get(o..o + 3).map_or(2, |b| u32::from(b[2]) << 16 | u32::from(b[1]) << 8 | u32::from(b[0]))
            }
            4 => w.hg_modus,
            _ => u32::MAX,
        })
    })
}

/// Funktion 51.1: Faden starten (Stapel in KB). Liefert TID oder −1.
pub fn faden(stapel_kb: usize, f: Box<dyn FnOnce() + Send>) -> i32 {
    pruefe_ende();
    crate::prozess::faden_starten(stapel_kb, f).map_or(-1, |t| t as i32)
}

/// Reserve im Heap eines Programms, die Funktion 68.12 nicht hergibt:
/// 1/16 des Heaps, höchstens 256 KB. Rust-Code im Faden des Programms
/// (Systemfunktionen, Bilddekoder) belegt ebenfalls dort – fände er den Heap
/// voll, würde das Programm beendet („Speicher voll“). So liefert zuerst
/// malloc des Programms null (B10: der Browser zeigt „Zu wenig Speicher“
/// statt beendet zu werden).
pub fn heap_reserve(groesse_heap: usize) -> usize {
    (groesse_heap / 16).min(256 * 1024)
}

/// Freier Platz im Heap des eigenen Programms (ohne Blockköpfe und
/// Zerstückelung zu rechnen); None ausserhalb eines Programms.
pub fn programm_speicher_frei() -> Option<usize> {
    prozess::eigener_heap().map(|h| h.groesse().saturating_sub(h.belegt()))
}

/// Grösster freie Block im Heap des eigenen Programms (None = kein eigener
/// Heap): ob eine grosse Belegung am Stück gelingt.
pub fn programm_speicher_groesster_block() -> Option<usize> {
    prozess::eigener_heap().map(|h| h.groesster_freier_block())
}

/// Funktion 68.12: Speicherblock im Heap des Programms.
pub fn speicher_holen(groesse: usize) -> *mut u8 {
    pruefe_ende();
    prozess::eigener_heap().map_or(core::ptr::null_mut(), |h| {
        let frei = h.groesse().saturating_sub(h.belegt());
        if frei < groesse.saturating_add(heap_reserve(h.groesse())) {
            return core::ptr::null_mut();
        }
        h.alloc(groesse, 16)
    })
}

/// Funktion 68.13: Speicherblock freigeben (true = ok).
///
/// # Safety
/// `p` stammt aus `speicher_holen` desselben Programms.
pub unsafe fn speicher_frei(p: *mut u8) -> bool {
    pruefe_ende();
    match prozess::eigener_heap() {
        // SAFETY: Vertrag des Aufrufers.
        Some(h) if h.enthaelt(p) => {
            unsafe { h.free(p) };
            true
        }
        _ => false,
    }
}

/// Funktion 68.20: Block vergrössern/verkleinern.
///
/// # Safety
/// `p` stammt aus `speicher_holen` (oder ist null), `alt` ist seine Grösse.
pub unsafe fn speicher_neu(p: *mut u8, alt: usize, neu: usize) -> *mut u8 {
    let q = speicher_holen(neu);
    if !q.is_null() && !p.is_null() {
        // SAFETY: beide Blöcke gültig, Länge = Minimum.
        unsafe {
            core::ptr::copy_nonoverlapping(p, q, alt.min(neu));
            speicher_frei(p);
        }
    }
    q
}

static DEBUG_AUSGABE: std::sync::Mutex<Option<fn(&str)>> = std::sync::Mutex::new(None);

/// Ziel des Debug-Boards festlegen (Board: Logger der Firmware; Standard: stderr).
pub fn debug_ausgabe_setzen(f: fn(&str)) {
    *DEBUG_AUSGABE.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// Funktion 63.1: Zeichen auf das Debug-Board (Systemlog).
pub fn debug_zeichen(b: u8) {
    use std::sync::Mutex;
    static ZEILE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    kern_abschnitt(|| {
        let mut z = ZEILE.lock().unwrap_or_else(|e| e.into_inner());
        if b == b'\n' {
            let zeile = String::from_utf8_lossy(&z).into_owned();
            match *DEBUG_AUSGABE.lock().unwrap_or_else(|e| e.into_inner()) {
                Some(f) => f(&zeile),
                None => eprintln!("[debug] {zeile}"),
            }
            z.clear();
        } else if z.len() < 512 {
            z.push(b);
        }
    });
}

static ABLAGE: std::sync::Mutex<Vec<Vec<u8>>> = std::sync::Mutex::new(Vec::new());
/// Grösster Eintrag der Zwischenablage (System-Heap schützen).
const ABLAGE_MAX: usize = 1 << 20;

/// Funktion 54: Zwischenablage. 0 Anzahl, 1 Slot lesen (Kopie im Heap des
/// Programms, Zeiger), 2 schreiben, 3 letzten löschen, 4 Sperre lösen.
/// `daten` für 54.2. Rückgabe wie eax.
pub fn ablage(sub: u32, ecx: u32, daten: &[u8]) -> u32 {
    pruefe_ende();
    match sub {
        0 => kern_abschnitt(|| ABLAGE.lock().unwrap_or_else(|e| e.into_inner()).len() as u32),
        1 => {
            let Some(blob) = kern_abschnitt(|| ABLAGE.lock().unwrap_or_else(|e| e.into_inner()).get(ecx as usize).cloned()) else { return 1 };
            let p = speicher_holen(blob.len().max(1));
            if p.is_null() {
                return 1;
            }
            // SAFETY: frisch belegter Block der Länge blob.len().
            unsafe { core::ptr::copy_nonoverlapping(blob.as_ptr(), p, blob.len()) };
            p as usize as u32
        }
        2 if daten.len() > ABLAGE_MAX => 1,
        2 => kern_abschnitt(|| {
            let mut a = ABLAGE.lock().unwrap_or_else(|e| e.into_inner());
            if a.len() >= 32 {
                a.remove(0);
            }
            // Wie der KolibriOS-Kern (core/clipboard.inc, „store size of
            // slot“): das erste Doppelwort ist immer die Grösse – Programme
            // wie PasswordGen/TinyHashView lassen es leer und prüfen es beim
            // Einfügen.
            let mut b = daten.to_vec();
            if b.len() >= 4 {
                let n = b.len() as u32;
                b[..4].copy_from_slice(&n.to_le_bytes());
            }
            a.push(b);
            0
        }),
        3 => kern_abschnitt(|| if ABLAGE.lock().unwrap_or_else(|e| e.into_inner()).pop().is_some() { 0 } else { 1 }),
        4 => 0,
        _ => u32::MAX,
    }
}

/// Text in die Zwischenablage (Kopf: Grösse, Typ 0 = Text, Kodierung 0 = UTF-8
/// wie `KSYS_CLIP_UTF8` in ksys.h; 1 = cp866, 2 = cp1251).
pub fn ablage_text_setzen(t: &str) {
    let mut b = Vec::with_capacity(12 + t.len());
    b.extend_from_slice(&((12 + t.len()) as u32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(t.as_bytes());
    ablage(2, 0, &b);
}

/// Letzter Text der Zwischenablage (UTF-8 oder cp866).
pub fn ablage_text() -> Option<String> {
    pruefe_ende();
    let blob = kern_abschnitt(|| ABLAGE.lock().unwrap_or_else(|e| e.into_inner()).last().cloned())?;
    if blob.len() < 12 || u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]) > 1 {
        return None;
    }
    // Grösse im Kopf; manche KolibriOS-Programme lassen sie 0 (z. B.
    // PasswordGen, nur die Länge beim Aufruf 54.2 stimmt) → gespeicherte Länge
    let n = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
    let n = if (12..=blob.len()).contains(&n) { n } else { blob.len() };
    let k = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]);
    let t = &blob[12..n];
    let t = &t[..t.iter().position(|&c| c == 0).unwrap_or(t.len())];
    Some(match k {
        1 => t.iter().map(|&c| char::from_u32(crate::schrift::cp866_zu_uni(c)).unwrap_or('?')).collect(),
        2 => t.iter().map(|&c| char::from_u32(crate::schrift::cp1251_zu_uni(c)).unwrap_or('?')).collect(),
        _ => String::from_utf8_lossy(t).into_owned(),
    })
}

/// Textzeile auf das Debug-Board (wie mehrfach Funktion 63.1).
pub fn debug_text(t: &str) {
    for b in t.bytes().chain([10u8]) {
        debug_zeichen(b);
    }
}

/// Funktion −1: Faden beenden.
pub fn ende() -> ! {
    prozess::beende_selbst(Ende::Normal)
}

// ------------------------------------------------- Plattform-Seite (Eingabe, Bild)

/// Eingaben der Plattform (Touch-Treiber, Autopilot) in den Kern.
pub mod eingabe {
    use super::*;

    /// Zeiger auf (x, y) in logischen Bildschirmkoordinaten, `tasten`-Bits.
    pub fn maus(x: i32, y: i32, tasten: u32) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.maus(x, y, tasten)));
    }

    /// Rollen (Wischen) um dx, dy Schritte.
    pub fn scroll(dx: i32, dy: i32) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.scroll(dx, dy)));
    }

    /// Touchpad-Modus ein/aus (Einstellungen).
    pub fn touchpad_setzen(an: bool) {
        kern_abschnitt(|| {
            fenster::mit_welt(|w| {
                if w.touchpad != an {
                    w.touchpad = an;
                    let r = w.zeiger_rechteck();
                    w.markiere(r);
                }
            })
        });
    }

    /// Finger auf dem Bildschirm (Bit 31) mit Lage x << 16 | y. Die Gesten
    /// melden ein Tippen erst beim Loslassen als Klick; wer schon beim
    /// Aufsetzen reagieren will (Bildschirmtastatur: gedrückte Taste), liest
    /// hier mit. 32 Bit: RISC-V-32 hat keine 64-Bit-Atomics.
    static BERUEHRUNG: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

    /// Lage des Fingers (logische Bildschirmkoordinaten) oder `None`.
    pub fn beruehrung_setzen(p: Option<(i32, i32)>) {
        let v = p.map_or(0, |(x, y)| 1 << 31 | (x.clamp(0, 0x7FFF) as u32) << 16 | y.clamp(0, 0xFFFF) as u32);
        BERUEHRUNG.store(v, core::sync::atomic::Ordering::Relaxed);
    }

    /// Liegt gerade ein Finger auf? Lage in Bildschirmkoordinaten.
    pub fn beruehrung() -> Option<(i32, i32)> {
        let v = BERUEHRUNG.load(core::sync::atomic::Ordering::Relaxed);
        (v & 1 << 31 != 0).then_some(((v >> 16 & 0x7FFF) as i32, (v & 0xFFFF) as i32))
    }

    /// Ist der Touchpad-Modus an?
    pub fn touchpad() -> bool {
        kern_abschnitt(|| fenster::mit_welt(|w| w.touchpad))
    }

    /// Aktuelle Zeigerposition (Bildschirm).
    pub fn zeiger() -> (i32, i32) {
        kern_abschnitt(|| fenster::mit_welt(|w| (w.maus.x, w.maus.y)))
    }

    /// Fenster unter (x, y) am Kopf: Slot (Titelleiste ziehen).
    pub fn kopf_bei(x: i32, y: i32) -> Option<usize> {
        kern_abschnitt(|| fenster::mit_welt(|w| w.kopf_bei(x, y)))
    }

    /// Fenster nach vorne holen und aktivieren (Titelleiste berührt).
    pub fn aktivieren(slot: usize) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.aktivieren(slot)));
    }

    /// Fenster verschieben (Titelleiste ziehen).
    pub fn verschieben(slot: usize, dx: i32, dy: i32) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.verschieben(slot, dx, dy)));
    }

    /// Maximieren/wiederherstellen (Doppeltipp auf die Titelleiste).
    pub fn maximieren(slot: usize) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.maximieren_umschalten(slot)));
    }

    /// Zoom-Geste (Faktor in Tausendstel) an das aktive Fenster.
    pub fn zoom(faktor_tausendstel: u32, mx: i32, my: i32) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.zoom(faktor_tausendstel, mx, my)));
    }

    /// Taste (Rückgabewert-Format von Funktion 2).
    pub fn taste(code: u32) {
        kern_abschnitt(|| fenster::mit_welt(|w| w.taste(code)));
    }
}

/// Bildschirm zusammensetzen; liefert false, wenn sich nichts geändert hat
/// und `immer` nicht gesetzt ist.
pub fn komponieren(ziel: &mut Puffer, immer: bool) -> bool {
    kern_abschnitt(|| {
        fenster::mit_welt(|w| {
            if w.schmutz.is_empty() && !immer {
                return false;
            }
            if ziel.w != w.w as usize || ziel.h != w.h as usize {
                *ziel = Puffer::neu(w.w as usize, w.h as usize, 0);
            }
            w.komponieren(ziel);
            w.schmutz.clear();
            true
        })
    })
}

/// Zusammenzusetzende Rechtecke eines Bildes (aktuelle und die des vorigen
/// Bildes, siehe [`Schirm`]): Rechtecke, die in einem anderen liegen,
/// entfallen (beim Scrollen kommt dasselbe Feld aus beiden Bildern); ist
/// ihre Hülle nicht grösser als ihre Summe (überlappend, aneinander), wird
/// die Hülle zusammengesetzt, sonst jedes einzeln (O1: vorher der ganze Bildschirm, sobald die doppelt
/// gezählten Flächen den halben Bildschirm überstiegen – beim Scrollen im
/// Browser 230 400 statt ~165 000 Bildpunkte je Bild).
pub fn rechtecke_zusammenfassen(rects: Vec<Clip>) -> Vec<Clip> {
    let enthalten = |a: &Clip, b: &Clip| a.x0 >= b.x0 && a.y0 >= b.y0 && a.x1 <= b.x1 && a.y1 <= b.y1;
    let mut aus: Vec<Clip> = Vec::with_capacity(rects.len());
    for (i, r) in rects.iter().enumerate() {
        if r.leer() {
            continue;
        }
        // in einem anderen enthalten (bei gleichen nur das erste behalten)
        let weg = rects.iter().enumerate().any(|(j, o)| j != i && !o.leer() && enthalten(r, o) && (!enthalten(o, r) || j < i));
        if !weg {
            aus.push(*r);
        }
    }
    let Some(huelle) = aus.iter().copied().reduce(|a, b| a.vereinigung(b)) else {
        return aus;
    };
    let summe: i64 = aus.iter().map(|r| r.flaeche()).sum();
    if summe >= huelle.flaeche() { vec![huelle] } else { aus }
}

/// Doppelpuffer für die Anzeige mit Dirty-Rects: setzt nur geänderte
/// Bereiche zusammen. Weil zwei Puffer abwechseln, bekommt jeder Puffer auch
/// die Änderungen des vorigen Bildes nachgetragen.
pub struct Schirm {
    pub puffer: [Puffer; 2],
    vorige: Vec<Clip>,
    naechster: usize,
    /// Zusammengesetzte Fläche im letzten Aufruf (Messung).
    pub letzte_flaeche: i64,
    /// Zeilen (von, bis), die der letzte Aufruf zusammengesetzt hat – das
    /// neue Bild unterscheidet sich vom vorigen nur darin (O1).
    pub letzte_zeilen: Option<(usize, usize)>,
}

impl Default for Schirm {
    fn default() -> Schirm {
        Schirm {
            puffer: [Puffer::neu(1, 1, 0), Puffer::neu(1, 1, 0)],
            vorige: Vec::new(),
            naechster: 0,
            letzte_flaeche: 0,
            letzte_zeilen: None,
        }
    }
}

impl Schirm {
    /// Nächsten Puffer aktualisieren; liefert seinen Index oder None, wenn
    /// sich seit den letzten zwei Bildern nichts geändert hat.
    pub fn aktualisieren(&mut self) -> Option<usize> {
        kern_abschnitt(|| {
            fenster::mit_welt(|w| {
                let i = self.naechster;
                let ganz = Clip { x0: 0, y0: 0, x1: w.w, y1: w.h };
                let mut rects = w.schmutz_holen();
                if self.puffer[i].w != w.w as usize || self.puffer[i].h != w.h as usize {
                    self.puffer[i] = Puffer::neu(w.w as usize, w.h as usize, 0);
                    rects = vec![ganz];
                    self.vorige = vec![ganz];
                }
                if rects.is_empty() && self.vorige.is_empty() {
                    return None;
                }
                let mut alle = rects.clone();
                alle.extend_from_slice(&self.vorige);
                let alle = rechtecke_zusammenfassen(alle);
                self.letzte_flaeche = alle.iter().map(|r| r.flaeche()).sum();
                self.letzte_zeilen = alle
                    .iter()
                    .map(|r| (r.y0.max(0) as usize, r.y1.clamp(0, w.h) as usize))
                    .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
                for r in &alle {
                    w.komponieren_teil(&mut self.puffer[i], *r);
                }
                self.vorige = rects;
                self.naechster = 1 - i;
                Some(i)
            })
        })
    }
}

/// Bildschirmfoto als 0x00RRGGBB-Pixel (Tests, `bildschirmfoto.py`).
pub fn foto() -> (usize, usize, Vec<u32>) {
    let mut p = Puffer::neu(1, 1, 0);
    komponieren(&mut p, true);
    let px = p.px.iter().map(|&c| crate::grafik::von565(c)).collect();
    (p.w, p.h, px)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Z3: Kopf ohne Grösse (0) wie von PasswordGen – vorher Absturz beim
    /// Lesen („slice index starts at 12 but ends at 0“).
    #[test]
    fn ablage_ohne_groesse_im_kopf() {
        let mut b = vec![0u8; 12];
        b[4] = 0; // Text
        b[8] = 1; // cp866
        b.extend_from_slice(b"Ab3!");
        ablage(2, b.len() as u32, &b);
        assert_eq!(ablage_text().as_deref(), Some("Ab3!"));
        // der Kern trägt die Grösse ein (wie KolibriOS)
        let kopf = ABLAGE.lock().unwrap().last().map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        assert_eq!(kopf, Some(16));
        // zu grosse Angabe: gespeicherte Länge
        b[0] = 200;
        ablage(2, b.len() as u32, &b);
        assert_eq!(ablage_text().as_deref(), Some("Ab3!"));
        ablage_text_setzen("Grüezi");
        assert_eq!(ablage_text().as_deref(), Some("Grüezi"));
    }
}
