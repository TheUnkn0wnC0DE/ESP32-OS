//! Gezeichnete Titelbilder der Spiele für die Karten im Menü – alles aus
//! Vielecken, Kreisen und Verläufen, leicht animiert (`t` in Sekunden).

use crate::SpielId;
use pudding_core::gfx::{Canvas, hex, mix};
use spatz::math::{cos, floor, sin};

/// Senkrechter Verlauf von `oben` nach `unten` (eine Zeile je Befehl).
pub fn verlauf(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, oben: u16, unten: u16) {
    for r in 0..h {
        c.fill_rect(x, y + r, w, 1, mix(oben, unten, (r * 32 / h.max(1)) as u32));
    }
}

/// Rechteck mit abgerundeten Ecken als Vieleck (16 Ecken).
pub fn abgerundet(x: f32, y: f32, w: f32, h: f32, r: f32) -> [(f32, f32); 16] {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut p = [(0.0, 0.0); 16];
    let ecken = [(x + w - r, y + r, -0.5f32), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 0.5), (x + r, y + r, 1.0)];
    for (k, &(cx, cy, start)) in ecken.iter().enumerate() {
        for j in 0..4 {
            let a = (start + j as f32 / 3.0 * 0.5) * core::f32::consts::PI;
            p[k * 4 + j] = (cx + r * cos(a), cy + r * sin(a));
        }
    }
    p
}

/// Ellipse als Vieleck (16 Ecken).
pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> [(f32, f32); 16] {
    let mut p = [(0.0, 0.0); 16];
    for (i, q) in p.iter_mut().enumerate() {
        let a = i as f32 / 16.0 * core::f32::consts::TAU;
        *q = (cx + rx * cos(a), cy + ry * sin(a));
    }
    p
}

/// Alle Ecken waagerecht in `[x0, x1]` halten (Bildrand der Karte).
fn klemm(p: &mut [(f32, f32)], x0: f32, x1: f32) {
    for q in p {
        q.0 = q.0.clamp(x0, x1);
    }
}

/// Isometrischer Würfel: Oberseite mittig bei (`x`, `y`), halbe Breite `a`.
fn wuerfel(c: &mut Canvas, x: f32, y: f32, a: f32, farben: [u16; 3]) {
    let h = a / 2.0;
    c.fill_poly(&[(x - a, y), (x, y + h), (x, y + h + a), (x - a, y + a)], farben[1]);
    c.fill_poly(&[(x, y + h), (x + a, y), (x + a, y + a), (x, y + h + a)], farben[2]);
    c.fill_poly(&[(x, y - h), (x + a, y), (x, y + h), (x - a, y)], farben[0]);
}

/// Titelbild eines Spiels in das Rechteck `x, y, w, h`.
pub fn titelbild(c: &mut Canvas, id: SpielId, x: i32, y: i32, w: i32, h: i32, t: f32) {
    match id {
        SpielId::Pudding => pudding(c, x, y, w, h, t),
        SpielId::Doom => doom(c, x, y, w, h, t),
        SpielId::Wuerfelwelt => wuerfelwelt(c, x, y, w, h, t),
        SpielId::Quake => quake(c, x, y, w, h, t),
        SpielId::Konsole => verlauf(c, x, y, w, h, hex(0x303050), hex(0x101020)),
    }
}

fn pudding(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, t: f32) {
    verlauf(c, x, y, w, h, hex(0xFF9EC4), hex(0xFFD49A));
    let (fx, fy, fw, fh) = (x as f32, y as f32, w as f32, h as f32);
    c.mix_disc(fx + fw * 0.78, fy + fh * 0.24, fh * 0.18, hex(0xFFFFFF), 10);
    let cx = fx + fw / 2.0;
    let boden = fy + fh * 0.8;
    // Teller mit Schatten
    c.mix_poly(&ellipse(cx, boden + fh * 0.03, fw * 0.36, fh * 0.07), hex(0x80304A), 10);
    c.fill_poly(&ellipse(cx, boden, fw * 0.34, fh * 0.06), hex(0xF6F2FA));
    c.fill_poly(&ellipse(cx, boden - fh * 0.008, fw * 0.25, fh * 0.035), hex(0xE4DCEC));
    // Pudding: wackelt (breiter/flacher und zurück)
    let w8 = sin(t * 6.0) * 0.05;
    let (bu, bo) = (fw * 0.24 * (1.0 + w8), fw * 0.17 * (1.0 + w8));
    let hoch = fh * 0.42 * (1.0 - w8);
    let oben = boden - hoch;
    let mut koerper = [(0.0, 0.0); 12];
    koerper[0] = (cx - bu, boden);
    koerper[1] = (cx + bu, boden);
    for i in 0..10 {
        let a = i as f32 / 9.0 * core::f32::consts::PI;
        koerper[2 + i] = (cx + bo * cos(a), oben + fh * 0.07 - fh * 0.07 * sin(a));
    }
    c.fill_poly(&koerper, hex(0xFFD23C));
    // Schatten rechts, Glanz links
    c.mix_poly(&[(cx + bo * 0.35, oben + fh * 0.03), (cx + bo, oben + fh * 0.07), (cx + bu, boden), (cx + bu * 0.45, boden)], hex(0xD08A10), 12);
    c.mix_disc(cx - bo * 0.55, oben + hoch * 0.45, fw * 0.025, hex(0xFFFFFF), 22);
    // Karamell oben mit Tropfen
    let mut kappe = [(0.0, 0.0); 12];
    for i in 0..10 {
        let a = i as f32 / 9.0 * core::f32::consts::PI;
        kappe[i] = (cx + bo * 1.02 * cos(a), oben + fh * 0.07 - fh * 0.075 * sin(a));
    }
    kappe[10] = (cx - bo * 1.02, oben + fh * 0.11);
    kappe[11] = (cx + bo * 1.02, oben + fh * 0.11);
    c.fill_poly(&kappe, hex(0x8A4B1C));
    for (dx, l) in [(-0.6f32, 0.06f32), (-0.1, 0.1), (0.45, 0.05)] {
        c.blend_disc(cx + bo * dx, oben + fh * (0.11 + l), fw * 0.018, hex(0x8A4B1C));
        c.fill_rect((cx + bo * dx - fw * 0.017) as i32, (oben + fh * 0.1) as i32, (fw * 0.034) as i32, (fh * l) as i32, hex(0x8A4B1C));
    }
    // Gesicht
    let ay = oben + hoch * 0.55;
    for s in [-1.0f32, 1.0] {
        c.mix_disc(cx + s * bo * 0.38, ay, fw * 0.03, hex(0x2A1030), 30);
        c.ring(cx + s * bo * 0.38 + fw * 0.008, ay - fw * 0.01, fw * 0.009, fw * 0.009, hex(0xFFFFFF));
    }
    c.mix_disc(cx, ay + hoch * 0.16, fw * 0.022, hex(0xE0506A), 26);
    // Kirsche
    let kx = cx + bo * 0.15;
    let ky = oben - fh * 0.02 - sin(t * 6.0) * 2.0;
    c.fill_rect(kx as i32, (ky - fh * 0.09) as i32, 2, (fh * 0.08) as i32, hex(0x3A7A2A));
    c.ring(kx, ky, fw * 0.045, fw * 0.045, hex(0xD8102A));
    c.mix_disc(kx - fw * 0.015, ky - fw * 0.015, fw * 0.012, hex(0xFFFFFF), 24);
}

/// Quake: dunkles Gemäuer, ein Tor mit wirbelndem Teleporter-Licht,
/// Blitze, Schriftzug in Rost.
fn quake(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, t: f32) {
    verlauf(c, x, y, w, h, hex(0x1A1410), hex(0x3A2A1E));
    let (fx, fy, fw, fh) = (x as f32, y as f32, w as f32, h as f32);
    // Mauersteine
    let reihe = fh / 9.0;
    for r in 0..9 {
        let ry = fy + r as f32 * reihe;
        let versatz = if r % 2 == 0 { 0.0 } else { fw / 10.0 };
        c.fill_rect(x, ry as i32, w, 1, hex(0x120D0A));
        let mut sx = fx + versatz;
        while sx < fx + fw {
            c.fill_rect(sx as i32, ry as i32, 1, reihe as i32, hex(0x120D0A));
            sx += fw / 5.0;
        }
    }
    // Torbogen
    let (cx, unten) = (fx + fw / 2.0, fy + fh * 0.96);
    let (bw, bh) = (fw * 0.44, fh * 0.62);
    let mut bogen = [(0.0f32, 0.0f32); 16];
    for (i, p) in bogen.iter_mut().enumerate() {
        let a = core::f32::consts::PI * i as f32 / 15.0;
        *p = (cx - cos(a) * bw * 0.5, unten - bh + bh * 0.42 - sin(a) * bh * 0.42);
    }
    let mut rahmen = [(0.0f32, 0.0f32); 16];
    for (i, p) in rahmen.iter_mut().enumerate() {
        let a = core::f32::consts::PI * i as f32 / 15.0;
        *p = (cx - cos(a) * bw * 0.62, unten - bh + bh * 0.42 - sin(a) * bh * 0.5);
    }
    c.fill_poly(&rahmen, hex(0x4A3826));
    c.fill_rect((cx - bw * 0.62) as i32, (unten - bh * 0.58) as i32, (bw * 1.24) as i32, (bh * 0.58) as i32, hex(0x4A3826));
    // Teleporter-Licht im Tor
    c.fill_poly(&bogen, hex(0x10283A));
    c.fill_rect((cx - bw * 0.5) as i32, (unten - bh * 0.58) as i32, bw as i32, (bh * 0.58) as i32, hex(0x10283A));
    for k in 0..9 {
        let a = t * 1.7 + k as f32 * 0.7;
        let r = bw * (0.06 + 0.04 * k as f32);
        let px = cx + cos(a) * r * 0.8;
        let py = unten - bh * 0.4 + sin(a * 1.3) * r * 0.9;
        c.mix_disc(px, py, bw * 0.16, hex(0x3AB4E8), 7);
    }
    c.mix_disc(cx, unten - bh * 0.42, bw * 0.22, hex(0x8EE6FF), 9);
    // Blitz (flackert)
    let bruch = |v: f32| v - floor(v);
    if bruch(t * 1.3) < 0.12 {
        let bx = fx + fw * (0.18 + 0.64 * bruch(t * 0.37));
        let mut p = bx;
        let mut py = fy;
        for _ in 0..5 {
            let ny = py + fh * 0.07;
            let nx = p + sin(py * 3.1) * fw * 0.04;
            c.fill_poly(&[(p, py), (p + 2.0, py), (nx + 2.0, ny), (nx, ny)], hex(0xE8F4FF));
            p = nx;
            py = ny;
        }
        c.mix_poly(&[(fx, fy), (fx + fw, fy), (fx + fw, fy + fh), (fx, fy + fh)], hex(0xFFFFFF), 4);
    }
    // Schriftzug
    let sc = ((fw / 36.0) as i32).max(2);
    let tw = pudding_core::gfx::text_width("QUAKE", sc);
    let tx = x + (w - tw) / 2;
    let ty = y + (fh * 0.08) as i32;
    c.text(tx + sc, ty + sc, "QUAKE", hex(0x100800), sc);
    c.text(tx, ty, "QUAKE", hex(0xC8803A), sc);
}

fn doom(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, t: f32) {
    verlauf(c, x, y, w, h, hex(0x120000), hex(0x5C0C00));
    let (fx, fy, fw, fh) = (x as f32, y as f32, w as f32, h as f32);
    let boden = fy + fh;
    // Flammen in drei Schichten
    for (schicht, farbe, hoehe) in [(0, hex(0xC8300A), 0.62f32), (1, hex(0xFF7A1A), 0.45), (2, hex(0xFFD860), 0.26)] {
        let n = 7;
        for i in 0..n {
            let bw = fw / (n as f32 - 1.0);
            let x0 = fx + (i as f32 - 0.5) * bw + schicht as f32 * bw * 0.15;
            let flacker = 0.75 + 0.25 * sin(t * (7.0 + i as f32) + i as f32 * 1.7 + schicht as f32);
            let hh = fh * hoehe * flacker;
            let sway = sin(t * 3.0 + i as f32) * bw * 0.25;
            let mut p = [
                (x0, boden),
                (x0 + bw * 0.22, boden - hh * 0.45),
                (x0 + bw * 0.5 + sway, boden - hh),
                (x0 + bw * 0.78, boden - hh * 0.45),
                (x0 + bw, boden),
            ];
            klemm(&mut p, fx, fx + fw);
            c.fill_poly(&p, farbe);
        }
    }
    // Glut steigt auf
    for i in 0..14 {
        let s = i as f32 * 37.3;
        let lauf = (t * (0.25 + (i % 4) as f32 * 0.08) + s * 0.013) % 1.0;
        let ex = fx + ((s * 7.1) % fw);
        let ey = boden - lauf * fh;
        let ex = (ex + sin(t * 2.0 + s) * 4.0).clamp(fx + 2.0, fx + fw - 2.0);
        c.mix_disc(ex, ey.max(fy + 2.0), 1.6, hex(0xFFB040), (24.0 * (1.0 - lauf)) as u32);
    }
    // Schriftzug
    let sc = ((fw / 36.0) as i32).max(2);
    let tw = pudding_core::gfx::text_width("DOOM", sc);
    let tx = x + (w - tw) / 2;
    let ty = y + (fh * 0.16) as i32;
    c.text(tx + sc, ty + sc, "DOOM", hex(0x200000), sc);
    c.text(tx, ty, "DOOM", hex(0xD8201A), sc);
}

fn wuerfelwelt(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, t: f32) {
    verlauf(c, x, y, w, h, hex(0x58B4F0), hex(0xCDEEFF));
    let (fx, fy, fw, fh) = (x as f32, y as f32, w as f32, h as f32);
    // Sonne und Wolken
    c.mix_disc(fx + fw * 0.82, fy + fh * 0.18, fh * 0.11, hex(0xFFF2A0), 26);
    c.mix_disc(fx + fw * 0.82, fy + fh * 0.18, fh * 0.17, hex(0xFFF8D0), 8);
    for (i, &(wx, wy)) in [(0.18f32, 0.2f32), (0.55, 0.12)].iter().enumerate() {
        let dx = ((t * 6.0 + i as f32 * 40.0) % (fw * 1.3)) - fw * 0.15;
        let cx = fx + (fw * wx + dx) % fw;
        let cy = fy + fh * wy;
        for (ox, oy, r) in [(0.0f32, 0.0f32, 0.06f32), (0.06, -0.02, 0.07), (0.12, 0.0, 0.055)] {
            let px = (cx + ox * fw).clamp(fx + r * fw, fx + fw - r * fw);
            c.mix_disc(px, cy + oy * fh, r * fw, hex(0xFFFFFF), 26);
        }
    }
    // Schwebende Insel aus Blöcken
    let a = fw * 0.075;
    let schweb = sin(t * 1.4) * 2.0;
    let mx = fx + fw * 0.5;
    let my = fy + fh * 0.52 + schweb;
    let gras = [hex(0x6ACB46), hex(0x8A5A32), hex(0x6E4626)];
    let stein = [hex(0xB0B0B8), hex(0x8E8E98), hex(0x74747E)];
    let wasser = [hex(0x55B0FF), hex(0x3C88D8), hex(0x2E6EB8)];
    let stamm = [hex(0xA87444), hex(0x8A5A2E), hex(0x6A4220)];
    let laub = [hex(0x4CB050), hex(0x3A9440), hex(0x2C7834)];
    // Unterseite der Insel (Stein, nach unten spitz)
    for (gx, gy, gz) in [(0, 0, -1), (1, 0, -1), (0, 1, -1), (1, 1, -2)] {
        let (px, py) = (mx + (gx - gy) as f32 * a, my + (gx + gy) as f32 * a / 2.0 - gz as f32 * a);
        wuerfel(c, px, py, a, stein);
    }
    // Oberfläche 3×3, ein Wasserblock
    for s in 0..5 {
        for gx in 0..3i32 {
            let gy = s - gx;
            if !(0..3).contains(&gy) {
                continue;
            }
            let farben = if (gx, gy) == (2, 1) { wasser } else { gras };
            let (px, py) = (mx + (gx - gy) as f32 * a, my + (gx + gy) as f32 * a / 2.0 - a);
            wuerfel(c, px, py, a, farben);
        }
    }
    // Baum auf (0, 0)
    for z in 2..4 {
        wuerfel(c, mx, my - z as f32 * a, a, stamm);
    }
    for (lx, ly, lz) in [(0, -1, 4), (-1, 0, 4), (0, 0, 4), (1, 0, 4), (0, 1, 4), (0, 0, 5)] {
        let (px, py) = (mx + (lx - ly) as f32 * a, my + (lx + ly) as f32 * a / 2.0 - lz as f32 * a);
        wuerfel(c, px, py, a, laub);
    }
}
