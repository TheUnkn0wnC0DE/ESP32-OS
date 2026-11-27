//! S3 (Entwicklungs-Build `ESP32OS_PPA_MESS`): Wie schnell dreht der PPA
//! einen Block in den Bildspeicher, je nach Blockhöhe? Beim Desktop liegt
//! ein Streifen (8 Quellzeilen → 1280×16 Panelpixel) gedreht als 16 Pixel
//! breites Band im Bildspeicher, je Panelzeile also nur 32 Byte am Stück.
//! Gemessen wird bei laufender Anzeige (sie liest den PSRAM mit 55 Hz mit):
//! Quelle im PSRAM, 1280 × {16, 32, 64, 128} gedreht (90°) in einen
//! Bildspeicher, dazu ungedreht als Vergleich. Ergebnis ins Protokoll.

use crate::board::{LCD_H, LCD_W};
use crate::display::FB_BYTES;
use crate::hw::{alloc_pixels, now_us, zeroed};
use esp_idf_svc::sys;

fn auftrag(ppa: sys::ppa_client_handle_t, src: &[u16], bw: usize, bh: usize, ziel: *mut u16, zw: usize, zh: usize, ox: usize, drehen: bool) -> bool {
    // SAFETY: Quelle und Ziel sind 128-Byte-ausgerichtete Pixelpuffer
    // (alloc_pixels), gross genug; blockierender Auftrag.
    unsafe {
        let mut op: sys::ppa_srm_oper_config_t = zeroed();
        op.in_.buffer = src.as_ptr().cast();
        op.in_.pic_w = bw as u32;
        op.in_.pic_h = bh as u32;
        op.in_.block_w = bw as u32;
        op.in_.block_h = bh as u32;
        op.in_.__bindgen_anon_1.srm_cm = sys::ppa_srm_color_mode_t_PPA_SRM_COLOR_MODE_RGB565;
        op.out.buffer = ziel.cast();
        op.out.buffer_size = FB_BYTES as u32;
        op.out.pic_w = zw as u32;
        op.out.pic_h = zh as u32;
        op.out.block_offset_x = ox as u32;
        op.out.block_offset_y = 0;
        op.out.__bindgen_anon_1.srm_cm = sys::ppa_srm_color_mode_t_PPA_SRM_COLOR_MODE_RGB565;
        op.rotation_angle = if drehen { sys::ppa_srm_rotation_angle_t_PPA_SRM_ROTATION_ANGLE_90 } else { sys::ppa_srm_rotation_angle_t_PPA_SRM_ROTATION_ANGLE_0 };
        op.scale_x = 1.0;
        op.scale_y = 1.0;
        op.mode = sys::ppa_trans_mode_t_PPA_TRANS_MODE_BLOCKING;
        sys::ppa_do_scale_rotate_mirror(ppa, &op) == 0
    }
}

/// CPU statt PPA: `rows` Quellzeilen (640 breit) ab Zeile `y0` verdoppeln
/// und wie der PPA (90°) gedreht in den Bildspeicher schreiben: Quellspalte
/// x → Panelzeilen 1279−2x und 1278−2x, Quellzeile y → Panelspalten 2y, 2y+1.
#[inline(never)]
fn cpu_drehen(src: &[u16], rows: usize, fb: *mut u16, y0: usize) {
    for x in 0..640 {
        let py = 1279 - 2 * x;
        // SAFETY: py, py−1 < 1280 und 2·(y0+rows) ≤ 720; 2·y0 gerade, also
        // 4-Byte-ausgerichtet (fb aus alloc_pixels).
        unsafe {
            let z1 = fb.add(py * LCD_W + 2 * y0).cast::<u32>();
            let z0 = fb.add((py - 1) * LCD_W + 2 * y0).cast::<u32>();
            for r in 0..rows {
                let p = u32::from(src[r * 640 + x]);
                let v = p | p << 16;
                z1.add(r).write(v);
                z0.add(r).write(v);
            }
        }
    }
}

/// Misst und schreibt je Fall: µs je Auftrag, µs je 1280×16-Einheit, MB/s.
pub fn lauf() {
    let mut cfg: sys::ppa_client_config_t = zeroed();
    cfg.oper_type = sys::ppa_operation_t_PPA_OPERATION_SRM;
    cfg.max_pending_trans_num = 1;
    cfg.data_burst_length = sys::ppa_data_burst_length_t_PPA_DATA_BURST_LENGTH_128;
    let mut ppa = core::ptr::null_mut();
    // SAFETY: gültige Konfiguration.
    if unsafe { sys::ppa_register_client(&cfg, &mut ppa) } != 0 {
        log::error!("PPA-MESS: kein PPA-Client");
        return;
    }
    let Some(ziel) = alloc_pixels(LCD_W * LCD_H, false) else {
        log::error!("PPA-MESS: kein Zielpuffer");
        return;
    };
    let Some(quelle) = alloc_pixels(1280 * 128, false) else {
        log::error!("PPA-MESS: kein Quellpuffer");
        return;
    };
    for (i, p) in quelle.iter_mut().enumerate() {
        *p = (i as u16).wrapping_mul(2654);
    }
    let quelle_intern = alloc_pixels(1280 * 16, true);
    let ziel_ptr = ziel.as_mut_ptr();
    for durchgang in 0..2 {
        for &(hoehe, drehen, intern) in &[(16usize, true, true), (16, true, false), (32, true, false), (64, true, false), (128, true, false), (16, false, false), (64, false, false)] {
            let src: &[u16] = if intern {
                match &quelle_intern {
                    Some(q) => q,
                    None => continue,
                }
            } else {
                &quelle[..1280 * hoehe]
            };
            // ganzer Schirm in Bändern dieser Höhe (wie ein Vollbild)
            let baender = 720 / hoehe;
            let t0 = now_us();
            let mut ok = true;
            for b in 0..baender {
                ok &= if drehen {
                    auftrag(ppa, src, 1280, hoehe, ziel_ptr, LCD_W, LCD_H, b * hoehe, true)
                } else {
                    // ungedreht: Ziel als 1280×720 quer betrachtet
                    auftrag(ppa, src, 1280, hoehe, ziel_ptr, 1280, 720, 0, false)
                };
            }
            let us = (now_us() - t0).max(1) as f64;
            let bytes = (baender * 1280 * hoehe * 2) as f64;
            if durchgang == 1 {
                log::warn!(
                    "PPA-MESS: Block 1280x{hoehe} {} Quelle {}: {baender} Aufträge {:.1} ms, je Auftrag {:.0} µs, Vollbild-Anteil je 16 Zeilen {:.0} µs, {:.0} MB/s{}",
                    if drehen { "gedreht" } else { "ungedreht" },
                    if intern { "intern" } else { "PSRAM" },
                    us / 1000.0,
                    us / baender as f64,
                    us / (baender * hoehe / 16) as f64,
                    bytes / us,
                    if ok { "" } else { " (Fehler)" }
                );
            }
        }
    }
    // CPU: ganzer Schirm (360 Quellzeilen) in Streifen dieser Höhe
    for durchgang in 0..2 {
        for &rows in &[8usize, 16, 32] {
            let t0 = now_us();
            for b in 0..360 / rows {
                cpu_drehen(&quelle[..640 * rows], rows, ziel_ptr, b * rows);
            }
            let t1 = now_us();
            // SAFETY: ganzer Zielpuffer, aus dem Cache in den PSRAM.
            unsafe { sys::esp_cache_msync(ziel_ptr.cast(), FB_BYTES, crate::hw::CACHE_MSYNC_DIR_C2M as _) };
            let t2 = now_us();
            if durchgang == 1 {
                log::warn!(
                    "PPA-MESS: CPU verdoppeln+drehen {rows} Zeilen je Streifen: Vollbild {:.1} ms (+ Cache zurückschreiben {:.1} ms), {:.0} MB/s",
                    (t1 - t0) as f64 / 1000.0,
                    (t2 - t1) as f64 / 1000.0,
                    FB_BYTES as f64 / (t2 - t0).max(1) as f64
                );
            }
        }
    }
    // CPU am Cache vorbei (nicht gecachtes Fenster des PSRAM, +0x4000_0000):
    // kein Zurückschreiben, keine gemeinsamen Cache-Zeilen mit dem PPA
    let nc = (ziel_ptr as usize + 0x4000_0000) as *mut u16;
    for durchgang in 0..2 {
        for &rows in &[8usize, 32] {
            let t0 = now_us();
            for b in 0..360 / rows {
                cpu_drehen(&quelle[..640 * rows], rows, nc, b * rows);
            }
            let t1 = now_us();
            if durchgang == 1 {
                log::warn!(
                    "PPA-MESS: CPU ungecacht verdoppeln+drehen {rows} Zeilen je Streifen: Vollbild {:.1} ms, {:.0} MB/s",
                    (t1 - t0) as f64 / 1000.0,
                    FB_BYTES as f64 / (t1 - t0).max(1) as f64
                );
            }
        }
    }
    // gleichzeitig PPA und CPU (ungecacht): je die Hälfte des Schirms
    for durchgang in 0..2 {
        let t0 = now_us();
        let mut ok = true;
        // PPA: obere Hälfte (Quellzeilen 0..180) nicht blockierend wäre
        // genauer; hier im eigenen Faden blockierend, CPU im Messfaden
        let quelle_ptr = quelle.as_ptr() as usize;
        let ziel_us = ziel_ptr as usize;
        let ppa_us = ppa as usize;
        let h = std::thread::spawn(move || {
            let t = now_us();
            // SAFETY: Puffer leben bis zum join; nur lesend bzw. eigene Hälfte.
            let q = unsafe { core::slice::from_raw_parts(quelle_ptr as *const u16, 1280 * 16) };
            for b in 0..22 {
                auftrag(ppa_us as sys::ppa_client_handle_t, q, 1280, 16, ziel_us as *mut u16, LCD_W, LCD_H, b * 16, true);
            }
            now_us() - t
        });
        for b in 0..22 {
            cpu_drehen(&quelle[..640 * 8], 8, nc, 180 + b * 8);
        }
        let t_cpu = now_us() - t0;
        let t_ppa = h.join().unwrap_or(0);
        ok &= t_ppa > 0;
        if durchgang == 1 {
            log::warn!(
                "PPA-MESS: gleichzeitig je halber Schirm: PPA {:.1} ms, CPU ungecacht {:.1} ms, zusammen {:.1} ms{}",
                t_ppa as f64 / 1000.0,
                t_cpu as f64 / 1000.0,
                (now_us() - t0) as f64 / 1000.0,
                if ok { "" } else { " (Fehler)" }
            );
        }
    }
    // SAFETY: Client wird nicht mehr benutzt.
    unsafe { sys::ppa_unregister_client(ppa) };
}
