//! libimg nativ (KolibriOS `develop/libraries/libs-dev/libimg`): Dekodieren
//! (BMP, PNG, GIF inkl. Animation, JPEG Baseline – siehe [`crate::dekoder`]),
//! Anlegen, Umwandeln, Zeichnen, Mischen (Alpha), Skalieren, Spiegeln,
//! Drehen. C-Schnittstelle wie `clayer/libimg.h` (nur 32-Bit-Ziele, weil
//! `Image` 32-Bit-Zeiger hat); Rust-Programme nutzen [`Bild`].

use crate::dekoder::{self, Rgba};

/// Bild für Rust-Programme (0xAARRGGBB).
pub type Bild = Rgba;

/// Datei dekodieren (alle Einzelbilder).
pub fn laden(daten: &[u8]) -> Result<Vec<Bild>, dekoder::Fehler> {
    dekoder::dekodieren(daten).map(|(_, v)| v)
}

/// Bildausschnitt (xoff, yoff, w, h) an (x, y) ins Fenster (Funktion 65, 32 bpp).
pub fn zeichnen(b: &Bild, x: i32, y: i32, w: u32, h: u32, xoff: u32, yoff: u32) {
    let (w, h) = (w.min(b.w.saturating_sub(xoff)), h.min(b.h.saturating_sub(yoff)));
    if w == 0 || h == 0 {
        return;
    }
    let mut bgrx = Vec::with_capacity((w * h * 4) as usize);
    for yy in yoff..yoff + h {
        for &p in &b.px[(yy * b.w + xoff) as usize..(yy * b.w + xoff + w) as usize] {
            bgrx.extend_from_slice(&p.to_le_bytes());
        }
    }
    kern::sys::palettenbild(&bgrx, w << 16 | h, (x as u32) << 16 | y as u32 & 0xFFFF, 32, &[], 0);
}

/// Neu abtasten auf `nw`×`nh` (bilinear oder nächster Nachbar).
pub fn skalieren(b: &Bild, nw: u32, nh: u32, bilinear: bool) -> Bild {
    let (nw, nh) = (nw.max(1), nh.max(1));
    let mut px = Vec::with_capacity((nw * nh) as usize);
    for y in 0..nh {
        for x in 0..nw {
            if !bilinear || b.w < 2 || b.h < 2 {
                px.push(b.px[((y * b.h / nh) * b.w + x * b.w / nw) as usize]);
                continue;
            }
            let fx = ((x as f32 + 0.5) * b.w as f32 / nw as f32 - 0.5).clamp(0.0, (b.w - 1) as f32);
            let fy = ((y as f32 + 0.5) * b.h as f32 / nh as f32 - 0.5).clamp(0.0, (b.h - 1) as f32);
            let (x0, y0) = (fx as u32, fy as u32);
            let (x1, y1) = ((x0 + 1).min(b.w - 1), (y0 + 1).min(b.h - 1));
            let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
            let p = |x, y| b.px[(y * b.w + x) as usize];
            let mut c = 0u32;
            for s in 0..4 {
                let k = |v: u32| (v >> (8 * s) & 255) as f32;
                let o = k(p(x0, y0)) * (1.0 - ax) + k(p(x1, y0)) * ax;
                let u = k(p(x0, y1)) * (1.0 - ax) + k(p(x1, y1)) * ax;
                c |= ((o * (1.0 - ay) + u * ay).round() as u32 & 255) << (8 * s);
            }
            px.push(c);
        }
    }
    Bild { w: nw, h: nh, px, delay: b.delay }
}

/// `src` (mit Alpha) über `dst` legen.
pub fn mischen(dst: &mut Bild, src: &Bild, out_x: u32, out_y: u32, in_x: u32, in_y: u32, w: u32, h: u32) {
    for yy in 0..h {
        for xx in 0..w {
            let (sx, sy, dx, dy) = (in_x + xx, in_y + yy, out_x + xx, out_y + yy);
            if sx >= src.w || sy >= src.h || dx >= dst.w || dy >= dst.h {
                continue;
            }
            let s = src.px[(sy * src.w + sx) as usize];
            let d = &mut dst.px[(dy * dst.w + dx) as usize];
            let a = s >> 24;
            let k = |v: u32, sh: u32| ((v >> sh & 255) * a + (*d >> sh & 255) * (255 - a)) / 255;
            *d = (*d & 0xFF00_0000) | k(s, 16) << 16 | k(s, 8) << 8 | k(s, 0);
        }
    }
}

#[cfg(target_pointer_width = "32")]
pub mod c {
    //! C-Schnittstelle (`clayer/libimg.h`).
    use super::*;

    pub const BPP8I: u32 = 1;
    pub const BPP24: u32 = 2;
    pub const BPP32: u32 = 3;
    pub const BPP8G: u32 = 7;

    #[repr(C, packed)]
    pub struct Image {
        pub checksum: u32,
        pub width: u32,
        pub height: u32,
        pub next: *mut Image,
        pub previous: *mut Image,
        pub typ: u32,
        pub data: *mut u8,
        pub palette: *mut u32,
        pub extended: u32,
        pub flags: u32,
        pub delay: u32,
    }

    fn bpp(t: u32) -> usize {
        match t {
            BPP24 => 3,
            BPP32 => 4,
            _ => 1,
        }
    }

    fn neu(w: u32, h: u32, t: u32) -> *mut Image {
        let n = w as usize * h as usize * bpp(t);
        let data = Box::into_raw(vec![0u8; n].into_boxed_slice()) as *mut u8;
        let palette = if t == BPP8I { Box::into_raw(vec![0u32; 256].into_boxed_slice()) as *mut u32 } else { core::ptr::null_mut() };
        Box::into_raw(Box::new(Image {
            checksum: 0,
            width: w,
            height: h,
            next: core::ptr::null_mut(),
            previous: core::ptr::null_mut(),
            typ: t,
            data,
            palette,
            extended: 0,
            flags: 0,
            delay: 0,
        }))
    }

    unsafe fn daten<'a>(i: &Image) -> &'a mut [u8] {
        // SAFETY: data hat w*h*bpp Bytes (aus `neu`).
        unsafe { core::slice::from_raw_parts_mut(i.data, i.width as usize * i.height as usize * bpp(i.typ)) }
    }

    /// Image → 0xAARRGGBB.
    unsafe fn als_rgba(i: &Image) -> Bild {
        let d = unsafe { daten(i) };
        let pal = |k: u8| if i.palette.is_null() { 0 } else { unsafe { *i.palette.add(usize::from(k)) } };
        let px = match i.typ {
            BPP32 => d.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
            BPP24 => d.chunks_exact(3).map(|c| 0xFF00_0000 | u32::from(c[2]) << 16 | u32::from(c[1]) << 8 | u32::from(c[0])).collect(),
            BPP8I => d.iter().map(|&k| 0xFF00_0000 | pal(k)).collect(),
            _ => d.iter().map(|&g| 0xFF00_0000 | u32::from(g) * 0x01_0101).collect(),
        };
        Bild { w: i.width, h: i.height, px, delay: i.delay }
    }

    /// 0xAARRGGBB → neues Image vom Typ 24 oder 32.
    fn aus_rgba(b: &Bild, t: u32) -> *mut Image {
        let t = if t == BPP24 { BPP24 } else { BPP32 };
        let i = neu(b.w, b.h, t);
        // SAFETY: frisch angelegt.
        let d = unsafe { daten(&*i) };
        for (k, &p) in b.px.iter().enumerate() {
            if t == BPP32 {
                d[4 * k..4 * k + 4].copy_from_slice(&p.to_le_bytes());
            } else {
                d[3 * k..3 * k + 3].copy_from_slice(&p.to_le_bytes()[..3]);
            }
        }
        // SAFETY: frisch angelegt.
        unsafe { (*i).delay = b.delay };
        i
    }

    unsafe fn freigeben_eins(i: *mut Image) {
        if i.is_null() {
            return;
        }
        // SAFETY: aus `neu`.
        let b = unsafe { Box::from_raw(i) };
        let n = b.width as usize * b.height as usize * bpp(b.typ);
        unsafe {
            drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(b.data, n)));
            if !b.palette.is_null() {
                drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(b.palette, 256)));
            }
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_decode(data: *const u8, size: u32, _b_color: u32) -> *mut Image {
        if data.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: `size` Bytes vom Programm.
        let d = unsafe { core::slice::from_raw_parts(data, size as usize) };
        let (f, frames) = match dekoder::dekodieren(d) {
            Ok(x) => x,
            Err(e) => {
                // ins Protokoll: übernommene C-Programme prüfen das Ergebnis
                // oft nicht (libimg_demo)
                let kopf: Vec<u8> = d.iter().take(8).copied().collect();
                kern::sys::debug_text(&format!("libimg: img_decode {size} Byte, Anfang {kopf:02x?}: {e:?}
"));
                return core::ptr::null_mut();
            }
        };
        let deckend = matches!(f, dekoder::Format::Jpeg | dekoder::Format::Bmp) && frames.iter().all(|b| b.px.iter().all(|&p| p >> 24 == 255));
        let mut erstes: *mut Image = core::ptr::null_mut();
        let mut letztes: *mut Image = core::ptr::null_mut();
        for b in &frames {
            let i = aus_rgba(b, if deckend { BPP24 } else { BPP32 });
            if frames.len() > 1 {
                // SAFETY: frisch angelegt.
                unsafe { (*i).flags = 1 }; // Image.IsAnimated
            }
            if letztes.is_null() {
                erstes = i;
            } else {
                // SAFETY: beide gültig.
                unsafe {
                    (*letztes).next = i;
                    (*i).previous = letztes;
                }
            }
            letztes = i;
        }
        erstes
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_create(w: u32, h: u32, t: u32) -> *mut Image {
        if w == 0 || h == 0 || !matches!(t, BPP8I | BPP24 | BPP32 | BPP8G) {
            return core::ptr::null_mut();
        }
        neu(w, h, t)
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_destroy(i: *mut Image) -> bool {
        if i.is_null() {
            return false;
        }
        // an den Anfang der Kette, dann alle
        let mut p = i;
        unsafe {
            while !(*p).previous.is_null() {
                p = (*p).previous;
            }
            while !p.is_null() {
                let n = (*p).next;
                freigeben_eins(p);
                p = n;
            }
        }
        true
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_destroy_layer(i: *mut Image) -> bool {
        if i.is_null() {
            return false;
        }
        unsafe {
            let (v, n) = ((*i).previous, (*i).next);
            if !v.is_null() {
                (*v).next = n;
            }
            if !n.is_null() {
                (*n).previous = v;
            }
            freigeben_eins(i);
        }
        true
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_count(i: *mut Image) -> i32 {
        let mut n = 0;
        let mut p = i;
        while !p.is_null() {
            n += 1;
            p = unsafe { (*p).next };
        }
        n
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_convert(src: *mut Image, dst: *mut Image, t: u32, _f: u32, _p: u32) -> *mut Image {
        let Some(s) = (unsafe { src.as_ref() }) else { return core::ptr::null_mut() };
        if !matches!(t, BPP24 | BPP32) {
            return core::ptr::null_mut();
        }
        let neu_bild = aus_rgba(&unsafe { als_rgba(s) }, t);
        if !dst.is_null() {
            unsafe { freigeben_eins(dst) };
        }
        neu_bild
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_draw(i: *mut Image, x: u32, y: u32, w: u32, h: u32, xoff: u32, yoff: u32) {
        if let Some(i) = unsafe { i.as_ref() } {
            zeichnen(&unsafe { als_rgba(i) }, x as i32, y as i32, w, h, xoff, yoff);
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_blend(dst: *mut Image, src: *mut Image, ox: u32, oy: u32, ix: u32, iy: u32, w: u32, h: u32) -> *mut Image {
        let (Some(d), Some(s)) = (unsafe { dst.as_mut() }, unsafe { src.as_ref() }) else { return core::ptr::null_mut() };
        let mut ziel = unsafe { als_rgba(d) };
        mischen(&mut ziel, &unsafe { als_rgba(s) }, ox, oy, ix, iy, w, h);
        let n = aus_rgba(&ziel, d.typ);
        // Inhalt zurückkopieren (Zeiger `dst` bleibt gültig)
        unsafe {
            daten(d).copy_from_slice(daten(&*n));
            freigeben_eins(n);
        }
        dst
    }

    #[unsafe(no_mangle)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe extern "system" fn img_scale(
        src: *mut Image,
        cx: u32,
        cy: u32,
        cw: u32,
        ch: u32,
        dst: *mut Image,
        scale_type: u32,
        inter: u32,
        nw: u32,
        nh: u32,
    ) -> *mut Image {
        let Some(s) = (unsafe { src.as_ref() }) else { return core::ptr::null_mut() };
        let ganz = unsafe { als_rgba(s) };
        let (cw, ch) = (cw.min(ganz.w.saturating_sub(cx)), ch.min(ganz.h.saturating_sub(cy)));
        if cw == 0 || ch == 0 {
            return core::ptr::null_mut();
        }
        let mut aus = Bild { w: cw, h: ch, px: Vec::with_capacity((cw * ch) as usize), delay: 0 };
        for yy in cy..cy + ch {
            aus.px.extend_from_slice(&ganz.px[(yy * ganz.w + cx) as usize..(yy * ganz.w + cx + cw) as usize]);
        }
        let (fw, fh) = (nw as f32 / cw as f32, nh as f32 / ch as f32);
        let (zw, zh) = match scale_type {
            0 => (cw, ch),
            1 => {
                let k = (nw / cw).min(nh / ch).max(1);
                (cw * k, ch * k)
            }
            2 => (nw, nh),
            3 => (nw, nh),
            4 => {
                let f = fw.min(fh);
                ((cw as f32 * f) as u32, (ch as f32 * f) as u32)
            }
            5 => (nw, (ch as f32 * fw) as u32),
            6 => ((cw as f32 * fh) as u32, nh),
            7 => {
                let f = fw.max(fh);
                ((cw as f32 * f) as u32, (ch as f32 * f) as u32)
            }
            _ => return core::ptr::null_mut(),
        };
        let ergebnis = if scale_type == 2 {
            // Kacheln
            let mut t = Bild { w: zw, h: zh, px: Vec::with_capacity((zw * zh) as usize), delay: 0 };
            for y in 0..zh {
                for x in 0..zw {
                    t.px.push(aus.px[((y % ch) * cw + x % cw) as usize]);
                }
            }
            t
        } else {
            skalieren(&aus, zw, zh, inter != 0)
        };
        if !dst.is_null() {
            unsafe { freigeben_eins(dst) };
        }
        aus_rgba(&ergebnis, if s.typ == BPP24 { BPP24 } else { BPP32 })
    }

    unsafe fn ersetzen(i: &mut Image, b: &Bild) {
        let n = aus_rgba(b, i.typ);
        unsafe {
            let alt_n = i.width as usize * i.height as usize * bpp(i.typ);
            drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(i.data, alt_n)));
            if !i.palette.is_null() && (*n).typ != BPP8I {
                drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(i.palette, 256)));
                i.palette = core::ptr::null_mut();
            }
            i.data = (*n).data;
            i.typ = (*n).typ;
            i.width = b.w;
            i.height = b.h;
            (*n).data = Box::into_raw(Vec::<u8>::new().into_boxed_slice()) as *mut u8;
            (*n).width = 0;
            (*n).height = 0;
            freigeben_eins(n);
        }
    }

    unsafe fn flip(i: &mut Image, f: u32) -> bool {
        if !matches!(i.typ, BPP24 | BPP32) {
            return false;
        }
        let b = unsafe { als_rgba(i) };
        let mut n = b.clone();
        for y in 0..b.h {
            for x in 0..b.w {
                let sx = if f & 2 != 0 { b.w - 1 - x } else { x };
                let sy = if f & 1 != 0 { b.h - 1 - y } else { y };
                n.px[(y * b.w + x) as usize] = b.px[(sy * b.w + sx) as usize];
            }
        }
        unsafe { ersetzen(i, &n) };
        true
    }

    unsafe fn drehen(i: &mut Image, r: u32) -> bool {
        if !matches!(i.typ, BPP24 | BPP32) || !(1..=3).contains(&r) {
            return false;
        }
        let b = unsafe { als_rgba(i) };
        let (nw, nh) = if r == 2 { (b.w, b.h) } else { (b.h, b.w) };
        let mut n = Bild { w: nw, h: nh, px: vec![0; (nw * nh) as usize], delay: b.delay };
        for y in 0..b.h {
            for x in 0..b.w {
                let (zx, zy) = match r {
                    1 => (b.h - 1 - y, x),
                    2 => (b.w - 1 - x, b.h - 1 - y),
                    _ => (y, b.w - 1 - x),
                };
                n.px[(zy * nw + zx) as usize] = b.px[(y * b.w + x) as usize];
            }
        }
        unsafe { ersetzen(i, &n) };
        true
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_flip_layer(i: *mut Image, f: u32) -> bool {
        unsafe { i.as_mut().is_some_and(|i| flip(i, f)) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_flip(i: *mut Image, f: u32) -> bool {
        let mut p = i;
        let mut ok = !p.is_null();
        while let Some(x) = unsafe { p.as_mut() } {
            ok &= unsafe { flip(x, f) };
            p = x.next;
        }
        ok
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_rotate_layer(i: *mut Image, r: u32) -> bool {
        unsafe { i.as_mut().is_some_and(|i| drehen(i, r)) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_rotate(i: *mut Image, r: u32) -> bool {
        let mut p = i;
        let mut ok = !p.is_null();
        while let Some(x) = unsafe { p.as_mut() } {
            ok &= unsafe { drehen(x, r) };
            p = x.next;
        }
        ok
    }

    /// 24-Bit-Daten (B, G, R wie Funktion 7) nach `rgb`.
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_to_rgb2(i: *mut Image, rgb: *mut u8) {
        let Some(i) = (unsafe { i.as_ref() }) else { return };
        let b = unsafe { als_rgba(i) };
        for (k, p) in b.px.iter().enumerate() {
            unsafe { core::ptr::copy_nonoverlapping(p.to_le_bytes().as_ptr(), rgb.add(3 * k), 3) };
        }
    }

    /// Neues Image (Typ 24) mit denselben Pixeln – wie das Original ein
    /// neues Bild, das das Programm mit `img_destroy` freigibt.
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_to_rgb(i: *mut Image) -> *mut Image {
        let Some(i) = (unsafe { i.as_ref() }) else { return core::ptr::null_mut() };
        aus_rgba(&unsafe { als_rgba(i) }, BPP24)
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn img_resize_data(i: *mut Image, w: u32, h: u32) -> *mut Image {
        let Some(r) = (unsafe { i.as_mut() }) else { return core::ptr::null_mut() };
        let b = unsafe { als_rgba(r) };
        let mut n = Bild { w, h, px: vec![0; (w * h) as usize], delay: b.delay };
        for y in 0..h.min(b.h) {
            for x in 0..w.min(b.w) {
                n.px[(y * w + x) as usize] = b.px[(y * b.w + x) as usize];
            }
        }
        unsafe { ersetzen(r, &n) };
        i
    }
}
