//! Bilddekoder für libimg (ohne externe Abhängigkeiten): BMP, PNG (inkl.
//! Interlace, alle Farbtypen/Tiefen, tRNS), GIF (inkl. Animation,
//! Transparenz, Interlace), JPEG Baseline (Huffman, 1/3 Komponenten,
//! Unterabtastung, Restart-Marker). Ausgabe: Bilder in 0xAARRGGBB.

/// Ein dekodiertes Einzelbild.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    /// 0xAARRGGBB, zeilenweise von oben.
    pub px: Vec<u32>,
    /// Anzeigedauer in 1/100 s (GIF-Animation).
    pub delay: u32,
}

/// Format eines Bildes (libimg-Kennungen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bmp = 1,
    Gif = 4,
    Png = 5,
    Jpeg = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fehler {
    /// Unbekanntes oder kaputtes Format.
    Format,
    /// Bekanntes Format, aber nicht unterstützte Variante (z. B. progressives JPEG).
    NichtUnterstuetzt,
}

pub fn format(d: &[u8]) -> Option<Format> {
    if d.starts_with(b"BM") {
        Some(Format::Bmp)
    } else if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if d.starts_with(&[0xFF, 0xD8]) {
        Some(Format::Jpeg)
    } else {
        None
    }
}

/// Hardware-JPEG der Plattform (Board: JPEG-Einheit des P4). `None` =
/// nicht dekodierbar (z. B. progressiv) → Software-Dekoder.
pub type JpegHw = fn(&[u8]) -> Option<Rgba>;

static JPEG_HW: std::sync::OnceLock<JpegHw> = std::sync::OnceLock::new();

/// Hardware-JPEG festlegen (einmal beim Systemstart).
pub fn jpeg_hw_setzen(f: JpegHw) {
    let _ = JPEG_HW.set(f);
}

/// Alle Einzelbilder (mehrere nur bei animiertem GIF).
pub fn dekodieren(d: &[u8]) -> Result<(Format, Vec<Rgba>), Fehler> {
    let f = format(d).ok_or(Fehler::Format)?;
    let v = match f {
        Format::Bmp => vec![bmp(d)?],
        Format::Png => vec![png(d)?],
        Format::Gif => gif(d)?,
        // JPEG-Einheit: Treiber mit eigenen Sperren → geschützt (A6)
        Format::Jpeg => match JPEG_HW.get().and_then(|hw| kern::geschuetzt(|| hw(d))) {
            Some(b) => vec![b],
            None => vec![jpeg(d)?],
        },
    };
    Ok((f, v))
}

/// Nur der Software-Dekoder (Vergleich mit der Hardware am Board).
pub fn jpeg_software(d: &[u8]) -> Result<Rgba, Fehler> {
    jpeg(d)
}

/// Breite und Höhe aus dem Dateikopf (ohne zu dekodieren) – um zu grosse
/// Bilder abzulehnen, bevor Speicher belegt wird.
pub fn masse(d: &[u8]) -> Option<(u32, u32)> {
    let be16 = |o: usize| d.get(o..o + 2).map(|b| u32::from(u16::from_be_bytes([b[0], b[1]])));
    let be32 = |o: usize| d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    match format(d)? {
        Format::Png => Some((be32(16)?, be32(20)?)),
        Format::Gif => Some((u16le(d, 6).ok()?, u16le(d, 8).ok()?)),
        Format::Bmp => {
            let w = u32le(d, 18).ok()? as i32;
            let h = u32le(d, 22).ok()? as i32;
            Some((w.unsigned_abs(), h.unsigned_abs()))
        }
        Format::Jpeg => {
            let mut o = 2;
            while o + 4 <= d.len() {
                if d[o] != 0xFF {
                    o += 1;
                    continue;
                }
                let m = d[o + 1];
                if m == 0xFF || m == 0x01 || (0xD0..=0xD8).contains(&m) {
                    o += if m == 0xFF { 1 } else { 2 };
                    continue;
                }
                if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
                    return Some((be16(o + 7)?, be16(o + 5)?));
                }
                o += 2 + be16(o + 2)? as usize;
            }
            None
        }
    }
}

fn u16le(d: &[u8], o: usize) -> Result<u32, Fehler> {
    d.get(o..o + 2).map(|b| u32::from(u16::from_le_bytes([b[0], b[1]]))).ok_or(Fehler::Format)
}
fn u32le(d: &[u8], o: usize) -> Result<u32, Fehler> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(Fehler::Format)
}
fn u32be(d: &[u8], o: usize) -> Result<u32, Fehler> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or(Fehler::Format)
}

fn argb(a: u32, r: u32, g: u32, b: u32) -> u32 {
    a << 24 | r << 16 | g << 8 | b
}

// =================================================================== BMP

fn bmp(d: &[u8]) -> Result<Rgba, Fehler> {
    let off = u32le(d, 10)? as usize;
    let kopf = u32le(d, 14)?;
    let w = u32le(d, 18)? as i32;
    let h_roh = u32le(d, 22)? as i32;
    let bpp = u16le(d, 28)?;
    let komp = if kopf >= 40 { u32le(d, 30)? } else { 0 };
    let (w, oben_unten, h) = (w, h_roh < 0, h_roh.unsigned_abs() as i32);
    if w <= 0 || h <= 0 || w > 16384 || h > 16384 {
        return Err(Fehler::Format);
    }
    let pal_off = 14 + kopf as usize;
    let pal_n = match u32le(d, 46).unwrap_or(0) {
        0 if bpp <= 8 => 1 << bpp,
        n => n,
    } as usize;
    let pal: Vec<u32> = (0..pal_n.min(256))
        .map(|i| {
            let o = pal_off + 4 * i;
            d.get(o..o + 3).map_or(0xFF00_0000, |c| argb(255, u32::from(c[2]), u32::from(c[1]), u32::from(c[0])))
        })
        .collect();
    // Bitfelder (BI_BITFIELDS = 3) für 16/32 bpp
    let (rm, gm, bm, am) = if komp == 3 {
        let o = if kopf >= 52 { 54 } else { pal_off };
        (u32le(d, o)?, u32le(d, o + 4)?, u32le(d, o + 8)?, if kopf >= 56 { u32le(d, 66).unwrap_or(0) } else { 0 })
    } else if bpp == 16 {
        (0x7C00, 0x03E0, 0x001F, 0)
    } else {
        (0xFF_0000, 0xFF00, 0xFF, 0)
    };
    let feld = |v: u32, m: u32| -> u32 {
        if m == 0 {
            return 255;
        }
        let s = m.trailing_zeros();
        let bits = (m >> s).count_ones();
        let x = (v & m) >> s;
        if bits >= 8 { x >> (bits - 8) } else { x * 255 / ((1 << bits) - 1) }
    };
    let mut px = vec![0xFF00_0000u32; (w * h) as usize];
    let setze = |px: &mut Vec<u32>, x: i32, y: i32, c: u32| {
        let zy = if oben_unten { y } else { h - 1 - y };
        if x < w && zy >= 0 && zy < h {
            px[(zy * w + x) as usize] = c;
        }
    };
    match (bpp, komp) {
        (8, 1) | (4, 2) => {
            // RLE8 / RLE4
            let (mut x, mut y, mut i) = (0i32, 0i32, off);
            while i + 1 < d.len() {
                let (n, v) = (d[i] as i32, d[i + 1]);
                i += 2;
                if n > 0 {
                    for k in 0..n {
                        let idx = if bpp == 8 { v } else if k % 2 == 0 { v >> 4 } else { v & 15 };
                        setze(&mut px, x, y, pal.get(usize::from(idx)).copied().unwrap_or(0));
                        x += 1;
                    }
                } else {
                    match v {
                        0 => {
                            x = 0;
                            y += 1;
                        }
                        1 => break,
                        2 => {
                            x += i32::from(*d.get(i).ok_or(Fehler::Format)?);
                            y += i32::from(*d.get(i + 1).ok_or(Fehler::Format)?);
                            i += 2;
                        }
                        k => {
                            let k = i32::from(k);
                            for j in 0..k {
                                let b = *d.get(i + if bpp == 8 { j } else { j / 2 } as usize).ok_or(Fehler::Format)?;
                                let idx = if bpp == 8 { b } else if j % 2 == 0 { b >> 4 } else { b & 15 };
                                setze(&mut px, x, y, pal.get(usize::from(idx)).copied().unwrap_or(0));
                                x += 1;
                            }
                            let bytes = if bpp == 8 { k } else { (k + 1) / 2 };
                            i += (bytes + (bytes & 1)) as usize;
                        }
                    }
                }
            }
        }
        (1 | 4 | 8 | 16 | 24 | 32, 0 | 3) => {
            let zeile = ((w as usize * bpp as usize).div_ceil(32)) * 4;
            for y in 0..h {
                let z = d.get(off + y as usize * zeile..off + (y as usize + 1) * zeile).ok_or(Fehler::Format)?;
                for x in 0..w {
                    let xu = x as usize;
                    let c = match bpp {
                        1 => pal.get(usize::from(z[xu / 8] >> (7 - xu % 8) & 1)).copied().unwrap_or(0),
                        4 => pal.get(usize::from(z[xu / 2] >> (4 - 4 * (xu % 2)) & 15)).copied().unwrap_or(0),
                        8 => pal.get(usize::from(z[xu])).copied().unwrap_or(0),
                        16 => {
                            let v = u32::from(u16::from_le_bytes([z[2 * xu], z[2 * xu + 1]]));
                            argb(255, feld(v, rm), feld(v, gm), feld(v, bm))
                        }
                        24 => argb(255, u32::from(z[3 * xu + 2]), u32::from(z[3 * xu + 1]), u32::from(z[3 * xu])),
                        _ => {
                            let v = u32::from_le_bytes([z[4 * xu], z[4 * xu + 1], z[4 * xu + 2], z[4 * xu + 3]]);
                            let a = if am != 0 { feld(v, am) } else { 255 };
                            argb(a, feld(v, rm), feld(v, gm), feld(v, bm))
                        }
                    };
                    setze(&mut px, x, y, c);
                }
            }
        }
        _ => return Err(Fehler::NichtUnterstuetzt),
    }
    Ok(Rgba { w: w as u32, h: h as u32, px, delay: 0 })
}

// =================================================================== Inflate

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    puffer: u32,
    anzahl: u32,
}

impl Bits<'_> {
    fn bits(&mut self, n: u32) -> Result<u32, Fehler> {
        while self.anzahl < n {
            let b = *self.d.get(self.pos).ok_or(Fehler::Format)?;
            self.pos += 1;
            self.puffer |= u32::from(b) << self.anzahl;
            self.anzahl += 8;
        }
        let v = self.puffer & ((1u64 << n) - 1) as u32;
        self.puffer >>= n;
        self.anzahl -= n;
        Ok(v)
    }
}

struct Huff {
    count: [u16; 16],
    sym: Vec<u16>,
}

impl Huff {
    fn neu(laengen: &[u8]) -> Huff {
        let mut count = [0u16; 16];
        for &l in laengen {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + count[i - 1];
        }
        let mut sym = vec![0u16; laengen.len()];
        for (s, &l) in laengen.iter().enumerate() {
            if l != 0 {
                sym[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Huff { count, sym }
    }
    fn lesen(&self, b: &mut Bits) -> Result<u16, Fehler> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.bits(1)? as i32;
            let c = i32::from(self.count[len]);
            if code - c < first {
                return self.sym.get((index + code - first) as usize).copied().ok_or(Fehler::Format);
            }
            index += c;
            first += c;
            first <<= 1;
            code <<= 1;
        }
        Err(Fehler::Format)
    }
}

const LEN_BASIS: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASIS: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Rohes Deflate (RFC 1951).
pub fn inflate(d: &[u8], groesse_hinweis: usize) -> Result<Vec<u8>, Fehler> {
    let mut b = Bits { d, pos: 0, puffer: 0, anzahl: 0 };
    let mut out = Vec::with_capacity(groesse_hinweis);
    loop {
        let letzter = b.bits(1)?;
        match b.bits(2)? {
            0 => {
                b.puffer = 0;
                b.anzahl = 0;
                let len = u16le(d, b.pos)? as usize;
                b.pos += 4;
                out.extend_from_slice(d.get(b.pos..b.pos + len).ok_or(Fehler::Format)?);
                b.pos += len;
            }
            t @ (1 | 2) => {
                let (lit, dist) = if t == 1 {
                    let mut l = [8u8; 288];
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    (Huff::neu(&l), Huff::neu(&[5u8; 30]))
                } else {
                    let hlit = b.bits(5)? as usize + 257;
                    let hdist = b.bits(5)? as usize + 1;
                    let hclen = b.bits(4)? as usize + 4;
                    const ORDNUNG: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORDNUNG.iter().take(hclen) {
                        cl[o] = b.bits(3)? as u8;
                    }
                    let ch = Huff::neu(&cl);
                    let mut l = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < l.len() {
                        let s = ch.lesen(&mut b)?;
                        let (wert, n) = match s {
                            0..=15 => (s as u8, 1),
                            16 => (*l.get(i.wrapping_sub(1)).ok_or(Fehler::Format)?, 3 + b.bits(2)? as usize),
                            17 => (0, 3 + b.bits(3)? as usize),
                            _ => (0, 11 + b.bits(7)? as usize),
                        };
                        for _ in 0..n {
                            *l.get_mut(i).ok_or(Fehler::Format)? = wert;
                            i += 1;
                        }
                    }
                    (Huff::neu(&l[..hlit]), Huff::neu(&l[hlit..]))
                };
                loop {
                    let s = lit.lesen(&mut b)?;
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let k = (s - 257) as usize;
                        let len = *LEN_BASIS.get(k).ok_or(Fehler::Format)? as usize + b.bits(u32::from(LEN_EXTRA[k]))? as usize;
                        let ds = dist.lesen(&mut b)? as usize;
                        let dd = *DIST_BASIS.get(ds).ok_or(Fehler::Format)? as usize + b.bits(u32::from(DIST_EXTRA[ds]))? as usize;
                        if dd > out.len() {
                            return Err(Fehler::Format);
                        }
                        let start = out.len() - dd;
                        for j in 0..len {
                            out.push(out[start + j]);
                        }
                    }
                }
            }
            _ => return Err(Fehler::Format),
        }
        if letzter == 1 {
            return Ok(out);
        }
    }
}

// =================================================================== PNG

fn png(d: &[u8]) -> Result<Rgba, Fehler> {
    let mut o = 8;
    let (mut w, mut h, mut tiefe, mut typ, mut interlace) = (0u32, 0u32, 0u8, 0u8, 0u8);
    let mut pal: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat = Vec::new();
    while o + 8 <= d.len() {
        let len = u32be(d, o)? as usize;
        let art = &d[o + 4..o + 8];
        let inhalt = d.get(o + 8..o + 8 + len).ok_or(Fehler::Format)?;
        match art {
            b"IHDR" => {
                w = u32be(inhalt, 0)?;
                h = u32be(inhalt, 4)?;
                tiefe = inhalt[8];
                typ = inhalt[9];
                interlace = inhalt[12];
            }
            b"PLTE" => pal = inhalt.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            b"tRNS" => trns = inhalt.to_vec(),
            b"IDAT" => idat.extend_from_slice(inhalt),
            b"IEND" => break,
            _ => {}
        }
        o += 12 + len;
    }
    if w == 0 || h == 0 || w > 16384 || h > 16384 || idat.len() < 2 {
        return Err(Fehler::Format);
    }
    let kanaele = match typ {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(Fehler::Format),
    };
    let bpp_bits = kanaele * u32::from(tiefe);
    let bpp = (bpp_bits as usize).div_ceil(8).max(1);
    let roh = inflate(&idat[2..], (h as usize) * (1 + (w as usize * bpp_bits as usize).div_ceil(8)))?;
    let mut px = vec![0u32; (w * h) as usize];
    let durchgaenge: &[(u32, u32, u32, u32)] = if interlace == 1 {
        &[(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]
    } else {
        &[(0, 0, 1, 1)]
    };
    let mut pos = 0usize;
    let max_wert = (1u32 << tiefe) - 1;
    let grau = |v: u32| if tiefe == 16 { v >> 8 } else { v * 255 / max_wert };
    for &(x0, y0, dx, dy) in durchgaenge {
        if x0 >= w || y0 >= h {
            continue;
        }
        let pw = (w - x0).div_ceil(dx);
        let ph = (h - y0).div_ceil(dy);
        let zeile = (pw as usize * bpp_bits as usize).div_ceil(8);
        let mut vorher = vec![0u8; zeile];
        for py in 0..ph {
            let filter = *roh.get(pos).ok_or(Fehler::Format)?;
            let mut z = roh.get(pos + 1..pos + 1 + zeile).ok_or(Fehler::Format)?.to_vec();
            pos += 1 + zeile;
            for i in 0..zeile {
                let a = if i >= bpp { i32::from(z[i - bpp]) } else { 0 };
                let b = i32::from(vorher[i]);
                let c = if i >= bpp { i32::from(vorher[i - bpp]) } else { 0 };
                let add = match filter {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => (a + b) / 2,
                    4 => {
                        let p = a + b - c;
                        let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                        if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
                    }
                    _ => return Err(Fehler::Format),
                };
                z[i] = (i32::from(z[i]) + add) as u8;
            }
            let probe = |k: usize| -> u32 {
                // k-ter Wert der Zeile (tiefe Bit)
                match tiefe {
                    16 => u32::from(u16::from_be_bytes([z[2 * k], z[2 * k + 1]])),
                    8 => u32::from(z[k]),
                    t => {
                        let bit = k * t as usize;
                        u32::from(z[bit / 8] >> (8 - t as usize - bit % 8)) & max_wert
                    }
                }
            };
            for px_i in 0..pw as usize {
                let k = px_i * kanaele as usize;
                let c = match typ {
                    0 => {
                        let v = probe(k);
                        let tr = trns.len() >= 2 && v == u32::from(u16::from_be_bytes([trns[0], trns[1]]));
                        let g = grau(v);
                        argb(if tr { 0 } else { 255 }, g, g, g)
                    }
                    2 => {
                        let (r, g, b) = (probe(k), probe(k + 1), probe(k + 2));
                        let tr = trns.len() >= 6
                            && r == u32::from(u16::from_be_bytes([trns[0], trns[1]]))
                            && g == u32::from(u16::from_be_bytes([trns[2], trns[3]]))
                            && b == u32::from(u16::from_be_bytes([trns[4], trns[5]]));
                        argb(if tr { 0 } else { 255 }, grau(r), grau(g), grau(b))
                    }
                    3 => {
                        let i = probe(k) as usize;
                        let p = pal.get(i).copied().unwrap_or([0, 0, 0]);
                        let a = trns.get(i).copied().unwrap_or(255);
                        argb(u32::from(a), u32::from(p[0]), u32::from(p[1]), u32::from(p[2]))
                    }
                    4 => {
                        let g = grau(probe(k));
                        argb(grau(probe(k + 1)), g, g, g)
                    }
                    _ => argb(grau(probe(k + 3)), grau(probe(k)), grau(probe(k + 1)), grau(probe(k + 2))),
                };
                let (x, y) = (x0 + px_i as u32 * dx, y0 + py * dy);
                px[(y * w + x) as usize] = c;
            }
            vorher = z;
        }
    }
    Ok(Rgba { w, h, px, delay: 0 })
}

// =================================================================== GIF

fn gif(d: &[u8]) -> Result<Vec<Rgba>, Fehler> {
    let w = u16le(d, 6)?;
    let h = u16le(d, 8)?;
    let fl = *d.get(10).ok_or(Fehler::Format)?;
    if w == 0 || h == 0 {
        return Err(Fehler::Format);
    }
    let mut o = 13;
    let tabelle = |o: &mut usize, flags: u8| -> Result<Vec<u32>, Fehler> {
        let n = 2usize << (flags & 7);
        let t = d.get(*o..*o + 3 * n).ok_or(Fehler::Format)?;
        *o += 3 * n;
        Ok(t.chunks_exact(3).map(|c| argb(255, u32::from(c[0]), u32::from(c[1]), u32::from(c[2]))).collect())
    };
    let global = if fl & 0x80 != 0 { tabelle(&mut o, fl)? } else { Vec::new() };
    let mut bild = vec![0u32; (w * h) as usize];
    let mut frames = Vec::new();
    let (mut transparent, mut delay, mut entsorgung) = (None::<u8>, 0u32, 0u8);
    while o < d.len() {
        match d[o] {
            0x21 => {
                let art = *d.get(o + 1).ok_or(Fehler::Format)?;
                o += 2;
                if art == 0xF9 && d.get(o) == Some(&4) {
                    let f = d[o + 1];
                    delay = u16le(d, o + 2)?;
                    transparent = (f & 1 != 0).then(|| d[o + 4]);
                    entsorgung = (f >> 2) & 7;
                }
                while let Some(&n) = d.get(o) {
                    o += 1 + n as usize;
                    if n == 0 {
                        break;
                    }
                }
            }
            0x2C => {
                let (fx, fy, fw, fh) = (u16le(d, o + 1)?, u16le(d, o + 3)?, u16le(d, o + 5)?, u16le(d, o + 7)?);
                let lf = *d.get(o + 9).ok_or(Fehler::Format)?;
                o += 10;
                let lokal = if lf & 0x80 != 0 { tabelle(&mut o, lf)? } else { Vec::new() };
                let tab = if lokal.is_empty() { &global } else { &lokal };
                let min_code = u32::from(*d.get(o).ok_or(Fehler::Format)?);
                o += 1;
                let mut daten = Vec::new();
                while let Some(&n) = d.get(o) {
                    o += 1;
                    if n == 0 {
                        break;
                    }
                    daten.extend_from_slice(d.get(o..o + n as usize).ok_or(Fehler::Format)?);
                    o += n as usize;
                }
                let idx = lzw(&daten, min_code, (fw * fh) as usize)?;
                let vorher = bild.clone();
                let zeilen: Vec<u32> = if lf & 0x40 != 0 {
                    (0..fh).step_by(8).chain((4..fh).step_by(8)).chain((2..fh).step_by(4)).chain((1..fh).step_by(2)).collect()
                } else {
                    (0..fh).collect()
                };
                for (zi, &yy) in zeilen.iter().enumerate() {
                    for xx in 0..fw {
                        let Some(&i) = idx.get(zi * fw as usize + xx as usize) else { continue };
                        if Some(i) == transparent {
                            continue;
                        }
                        let (x, y) = (fx + xx, fy + yy);
                        if x < w && y < h {
                            bild[(y * w + x) as usize] = tab.get(usize::from(i)).copied().unwrap_or(0xFF00_0000);
                        }
                    }
                }
                frames.push(Rgba { w, h, px: bild.clone(), delay });
                match entsorgung {
                    2 => {
                        for yy in fy..(fy + fh).min(h) {
                            for xx in fx..(fx + fw).min(w) {
                                bild[(yy * w + xx) as usize] = 0;
                            }
                        }
                    }
                    3 => bild = vorher,
                    _ => {}
                }
                transparent = None;
                delay = 0;
                entsorgung = 0;
            }
            0x3B => break,
            _ => return Err(Fehler::Format),
        }
    }
    if frames.is_empty() {
        return Err(Fehler::Format);
    }
    Ok(frames)
}

fn lzw(d: &[u8], min: u32, max: usize) -> Result<Vec<u8>, Fehler> {
    if !(1..=11).contains(&min) {
        return Err(Fehler::Format);
    }
    let clear = 1u32 << min;
    let ende = clear + 1;
    let mut praefix = vec![0u16; 4096];
    let mut suffix = vec![0u8; 4096];
    let mut laenge = vec![0u16; 4096];
    for i in 0..clear {
        suffix[i as usize] = i as u8;
        laenge[i as usize] = 1;
    }
    let (mut groesse, mut naechster, mut alt) = (min + 1, clear + 2, None::<u32>);
    let mut out = Vec::with_capacity(max);
    let (mut puffer, mut bits, mut pos) = (0u32, 0u32, 0usize);
    let mut tmp = Vec::new();
    loop {
        while bits < groesse {
            let Some(&b) = d.get(pos) else { return Ok(out) };
            puffer |= u32::from(b) << bits;
            bits += 8;
            pos += 1;
        }
        let code = puffer & ((1 << groesse) - 1);
        puffer >>= groesse;
        bits -= groesse;
        if code == clear {
            groesse = min + 1;
            naechster = clear + 2;
            alt = None;
            continue;
        }
        if code == ende {
            return Ok(out);
        }
        let ausgeben = |c: u32, tmp: &mut Vec<u8>| {
            tmp.clear();
            let mut c = c as usize;
            loop {
                tmp.push(suffix[c]);
                if laenge[c] <= 1 {
                    break;
                }
                c = praefix[c] as usize;
            }
            tmp.reverse();
        };
        match alt {
            None => {
                if code >= clear {
                    return Err(Fehler::Format);
                }
                ausgeben(code, &mut tmp);
            }
            Some(a) => {
                let erstes;
                if code < naechster {
                    ausgeben(code, &mut tmp);
                    erstes = tmp[0];
                } else if code == naechster {
                    ausgeben(a, &mut tmp);
                    erstes = tmp[0];
                    tmp.push(erstes);
                } else {
                    return Err(Fehler::Format);
                }
                if naechster < 4096 {
                    praefix[naechster as usize] = a as u16;
                    suffix[naechster as usize] = erstes;
                    laenge[naechster as usize] = laenge[a as usize] + 1;
                    naechster += 1;
                    if naechster == 1 << groesse && groesse < 12 {
                        groesse += 1;
                    }
                }
            }
        }
        out.extend_from_slice(&tmp);
        if out.len() >= max {
            out.truncate(max);
            return Ok(out);
        }
        alt = Some(code);
    }
}

// =================================================================== JPEG

#[derive(Default, Clone)]
struct JHuff {
    /// (Länge, Code) → Wert: je Länge erster Code und Index
    min: [i32; 17],
    max: [i32; 18],
    val_ptr: [i32; 17],
    werte: Vec<u8>,
}

impl JHuff {
    fn neu(anz: &[u8; 16], werte: Vec<u8>) -> JHuff {
        let mut h = JHuff { werte, ..Default::default() };
        let (mut code, mut k) = (0i32, 0i32);
        for l in 1..=16 {
            let n = i32::from(anz[l - 1]);
            h.val_ptr[l] = k;
            h.min[l] = code;
            code += n;
            k += n;
            h.max[l] = if n > 0 { code - 1 } else { -1 };
            code <<= 1;
        }
        h.max[17] = i32::MAX;
        h
    }
}

struct JBits<'a> {
    d: &'a [u8],
    pos: usize,
    puffer: u32,
    anzahl: u32,
}

impl JBits<'_> {
    fn bit(&mut self) -> u32 {
        if self.anzahl == 0 {
            let mut b = *self.d.get(self.pos).unwrap_or(&0);
            if b == 0xFF {
                let n = *self.d.get(self.pos + 1).unwrap_or(&0);
                if n == 0 {
                    self.pos += 2;
                } else {
                    // Marker: mit Nullen auffüllen, nicht weiterlesen
                    b = 0;
                }
            } else {
                self.pos += 1;
            }
            self.puffer = u32::from(b);
            self.anzahl = 8;
        }
        self.anzahl -= 1;
        (self.puffer >> self.anzahl) & 1
    }
    fn bits(&mut self, n: u32) -> i32 {
        let mut v = 0;
        for _ in 0..n {
            v = v << 1 | self.bit() as i32;
        }
        v
    }
    fn huff(&mut self, h: &JHuff) -> Result<u8, Fehler> {
        let mut code = 0i32;
        for l in 1..=16 {
            code = code << 1 | self.bit() as i32;
            if code <= h.max[l] {
                return h.werte.get((h.val_ptr[l] + code - h.min[l]) as usize).copied().ok_or(Fehler::Format);
            }
        }
        Err(Fehler::Format)
    }
    fn erweitern(v: i32, n: u32) -> i32 {
        if n == 0 { 0 } else if v < 1 << (n - 1) { v - (1 << n) + 1 } else { v }
    }
    fn neu_ausrichten(&mut self) {
        self.anzahl = 0;
        // Restart-Marker überspringen
        while self.pos + 1 < self.d.len() && !(self.d[self.pos] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.pos + 1])) {
            self.pos += 1;
        }
        self.pos += 2;
    }
}

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57,
    50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

fn idct(k: &[i32; 64], out: &mut [u8; 64]) {
    // getrennte 1D-IDCT (Gleitkomma), genau genug für Anzeige
    let mut t = [0f32; 64];
    let c = |u: usize| if u == 0 { core::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
    static mut COS: [[f32; 8]; 8] = [[0.0; 8]; 8];
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        for x in 0..8 {
            for u in 0..8 {
                // SAFETY: einmalige Initialisierung unter `Once`.
                unsafe { COS[x][u] = ((2 * x + 1) as f32 * u as f32 * core::f32::consts::PI / 16.0).cos() };
            }
        }
    });
    // SAFETY: nach Initialisierung nur gelesen.
    let cos = unsafe { &*core::ptr::addr_of!(COS) };
    for y in 0..8 {
        for x in 0..8 {
            let mut s = 0.0;
            for u in 0..8 {
                s += c(u) * k[y * 8 + u] as f32 * cos[x][u];
            }
            t[y * 8 + x] = s / 2.0;
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                s += c(v) * t[v * 8 + x] * cos[y][v];
            }
            out[y * 8 + x] = (s / 2.0 + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

struct Komp {
    id: u8,
    h: u32,
    v: u32,
    q: usize,
    dc: usize,
    ac: usize,
    pred: i32,
    ebene: Vec<u8>,
    breite: usize,
}

fn jpeg(d: &[u8]) -> Result<Rgba, Fehler> {
    let mut q = [[0i32; 64]; 4];
    let mut dc: [JHuff; 4] = Default::default();
    let mut ac: [JHuff; 4] = Default::default();
    let mut komp: Vec<Komp> = Vec::new();
    let (mut w, mut h, mut restart) = (0u32, 0u32, 0u32);
    let mut adobe_rgb = false;
    // Progressiv (SOF2, B0b): Koeffizienten je Komponente über alle Scans
    // sammeln (natürliche Reihenfolge, noch nicht dequantisiert), am Ende
    // umrechnen.
    let mut progressiv = false;
    let mut koeff: Vec<Vec<[i32; 64]>> = Vec::new();
    let mut o = 2;
    loop {
        while d.get(o) == Some(&0xFF) && d.get(o + 1) == Some(&0xFF) {
            o += 1;
        }
        if d.get(o) != Some(&0xFF) {
            return Err(Fehler::Format);
        }
        let m = *d.get(o + 1).ok_or(Fehler::Format)?;
        o += 2;
        if m == 0xD9 {
            break;
        }
        let len = d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize).ok_or(Fehler::Format)?;
        let seg = d.get(o + 2..o + len).ok_or(Fehler::Format)?;
        match m {
            0xDB => {
                let mut i = 0;
                while i < seg.len() {
                    let (p, t) = (seg[i] >> 4, usize::from(seg[i] & 3));
                    i += 1;
                    for z in ZIGZAG {
                        q[t][z] = if p == 0 {
                            i32::from(seg[i])
                        } else {
                            i32::from(u16::from_be_bytes([seg[i], seg[i + 1]]))
                        };
                        i += if p == 0 { 1 } else { 2 };
                    }
                }
            }
            0xC0..=0xC2 => {
                if seg[0] != 8 {
                    return Err(Fehler::NichtUnterstuetzt);
                }
                progressiv = m == 0xC2;
                h = u32::from(u16::from_be_bytes([seg[1], seg[2]]));
                w = u32::from(u16::from_be_bytes([seg[3], seg[4]]));
                for i in 0..usize::from(seg[5]) {
                    let s = &seg[6 + 3 * i..9 + 3 * i];
                    komp.push(Komp {
                        id: s[0],
                        h: u32::from(s[1] >> 4).max(1),
                        v: u32::from(s[1] & 15).max(1),
                        q: usize::from(s[2] & 3),
                        dc: 0,
                        ac: 0,
                        pred: 0,
                        ebene: Vec::new(),
                        breite: 0,
                    });
                }
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err(Fehler::NichtUnterstuetzt),
            0xC4 => {
                let mut i = 0;
                while i < seg.len() {
                    let (klasse, id) = (seg[i] >> 4, usize::from(seg[i] & 3));
                    let mut anz = [0u8; 16];
                    anz.copy_from_slice(&seg[i + 1..i + 17]);
                    let n: usize = anz.iter().map(|&c| usize::from(c)).sum();
                    let werte = seg[i + 17..i + 17 + n].to_vec();
                    let t = JHuff::neu(&anz, werte);
                    if klasse == 0 {
                        dc[id] = t;
                    } else {
                        ac[id] = t;
                    }
                    i += 17 + n;
                }
            }
            0xDD => restart = u32::from(u16::from_be_bytes([seg[0], seg[1]])),
            0xEE => adobe_rgb = seg.len() >= 12 && seg.starts_with(b"Adobe") && seg[11] == 0,
            0xDA => {
                if w == 0 || h == 0 || komp.is_empty() || w > 16384 || h > 16384 {
                    return Err(Fehler::Format);
                }
                let n = usize::from(seg[0]);
                let mut im_scan = Vec::new();
                for i in 0..n {
                    let (id, t) = (seg[1 + 2 * i], seg[2 + 2 * i]);
                    if let Some(ki) = komp.iter().position(|k| k.id == id) {
                        komp[ki].dc = usize::from(t >> 4 & 3);
                        komp[ki].ac = usize::from(t & 3);
                        im_scan.push(ki);
                    }
                }
                let hmax = komp.iter().map(|k| k.h).max().unwrap_or(1);
                let vmax = komp.iter().map(|k| k.v).max().unwrap_or(1);
                let (mcu_w, mcu_h) = (8 * hmax, 8 * vmax);
                let (mx, my) = (w.div_ceil(mcu_w), h.div_ceil(mcu_h));
                if progressiv {
                    let p = 1 + 2 * n;
                    let (ss, se) = (usize::from(*seg.get(p).ok_or(Fehler::Format)?), usize::from(*seg.get(p + 1).ok_or(Fehler::Format)?));
                    let a = *seg.get(p + 2).ok_or(Fehler::Format)?;
                    let (ah, al) = (u32::from(a >> 4), u32::from(a & 15));
                    if ss > se || se > 63 || (ss == 0 && se != 0) || (ss > 0 && im_scan.len() != 1) || al > 13 {
                        return Err(Fehler::Format);
                    }
                    if koeff.is_empty() {
                        for k in komp.iter_mut() {
                            k.breite = (mx * k.h * 8) as usize;
                        }
                        koeff = komp.iter().map(|k| vec![[0i32; 64]; ((mx * k.h) * (my * k.v)) as usize]).collect();
                    }
                    let mut b = JBits { d, pos: o + len, puffer: 0, anzahl: 0 };
                    for k in komp.iter_mut() {
                        k.pred = 0;
                    }
                    let mut eobrun = 0u32;
                    let scan = ScanProg { ss, se, ah, al };
                    if im_scan.len() == 1 {
                        // Nicht verschachtelt: Blöcke der Komponente zeilenweise
                        // (nur die Fläche der Komponente, nicht die MCU-Auffüllung)
                        let ki = im_scan[0];
                        let k = &komp[ki];
                        let bw_voll = (mx * k.h) as usize;
                        let bw = ((w * k.h).div_ceil(hmax)).div_ceil(8) as usize;
                        let bh = ((h * k.v).div_ceil(vmax)).div_ceil(8) as usize;
                        let mut zaehler = 0u32;
                        for by in 0..bh {
                            for bx in 0..bw {
                                if restart > 0 && zaehler > 0 && zaehler % restart == 0 {
                                    b.neu_ausrichten();
                                    komp[ki].pred = 0;
                                    eobrun = 0;
                                }
                                zaehler += 1;
                                let (dci, aci) = (komp[ki].dc, komp[ki].ac);
                                let blk = &mut koeff[ki][by * bw_voll + bx];
                                prog_block(&mut b, blk, &scan, &dc[dci], &ac[aci], &mut komp[ki].pred, &mut eobrun)?;
                            }
                        }
                    } else {
                        // Verschachtelt (nur DC-Scans): MCU-Reihenfolge
                        let mut zaehler = 0u32;
                        for my_i in 0..my {
                            for mx_i in 0..mx {
                                if restart > 0 && zaehler > 0 && zaehler % restart == 0 {
                                    b.neu_ausrichten();
                                    for k in komp.iter_mut() {
                                        k.pred = 0;
                                    }
                                }
                                zaehler += 1;
                                for &ki in &im_scan {
                                    let (kh, kv, dci) = (komp[ki].h, komp[ki].v, komp[ki].dc);
                                    let bw_voll = (mx * kh) as usize;
                                    for by in 0..kv {
                                        for bx in 0..kh {
                                            let idx = ((my_i * kv + by) as usize) * bw_voll + (mx_i * kh + bx) as usize;
                                            let blk = &mut koeff[ki][idx];
                                            prog_block(&mut b, blk, &scan, &dc[dci], &ac[0], &mut komp[ki].pred, &mut eobrun)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Weiter beim nächsten Marker nach den Scan-Daten
                    o = naechster_marker(d, b.pos.saturating_sub(2).max(o + len)).ok_or(Fehler::Format)?;
                    continue;
                }
                for k in komp.iter_mut() {
                    k.breite = (mx * k.h * 8) as usize;
                    k.ebene = vec![0; k.breite * (my * k.v * 8) as usize];
                }
                let mut b = JBits { d, pos: o + len, puffer: 0, anzahl: 0 };
                let mut zaehler = 0u32;
                for my_i in 0..my {
                    for mx_i in 0..mx {
                        if restart > 0 && zaehler > 0 && zaehler % restart == 0 {
                            b.neu_ausrichten();
                            for k in komp.iter_mut() {
                                k.pred = 0;
                            }
                        }
                        zaehler += 1;
                        for k in komp.iter_mut() {
                            for by in 0..k.v {
                                for bx in 0..k.h {
                                    let mut koeff = [0i32; 64];
                                    let s = u32::from(b.huff(&dc[k.dc])?);
                                    let diff = JBits::erweitern(b.bits(s), s);
                                    k.pred += diff;
                                    koeff[0] = k.pred * q[k.q][0];
                                    let mut i = 1;
                                    while i < 64 {
                                        let rs = b.huff(&ac[k.ac])?;
                                        let (r, s) = (usize::from(rs >> 4), u32::from(rs & 15));
                                        if s == 0 {
                                            if r == 15 {
                                                i += 16;
                                                continue;
                                            }
                                            break;
                                        }
                                        i += r;
                                        if i > 63 {
                                            break;
                                        }
                                        koeff[ZIGZAG[i]] = JBits::erweitern(b.bits(s), s) * q[k.q][ZIGZAG[i]];
                                        i += 1;
                                    }
                                    let mut blk = [0u8; 64];
                                    idct(&koeff, &mut blk);
                                    let x0 = ((mx_i * k.h + bx) * 8) as usize;
                                    let y0 = ((my_i * k.v + by) * 8) as usize;
                                    for yy in 0..8 {
                                        let a = (y0 + yy) * k.breite + x0;
                                        k.ebene[a..a + 8].copy_from_slice(&blk[yy * 8..yy * 8 + 8]);
                                    }
                                }
                            }
                        }
                    }
                }
                return Ok(farbe(&komp, w, h, adobe_rgb));
            }
            _ => {}
        }
        o += len;
    }
    if !progressiv || koeff.is_empty() {
        return Err(Fehler::Format);
    }
    // Progressiv: dequantisieren, IDCT, in die Ebenen schreiben
    for (ki, k) in komp.iter_mut().enumerate() {
        let bw_voll = k.breite / 8;
        let zeilen = koeff[ki].len() / bw_voll.max(1);
        k.ebene = vec![0; k.breite * zeilen * 8];
        for (i, c) in koeff[ki].iter().enumerate() {
            let mut deq = [0i32; 64];
            for z in 0..64 {
                deq[z] = c[z] * q[k.q][z];
            }
            let mut blk = [0u8; 64];
            idct(&deq, &mut blk);
            let (x0, y0) = ((i % bw_voll) * 8, (i / bw_voll) * 8);
            for yy in 0..8 {
                let a = (y0 + yy) * k.breite + x0;
                k.ebene[a..a + 8].copy_from_slice(&blk[yy * 8..yy * 8 + 8]);
            }
        }
    }
    Ok(farbe(&komp, w, h, adobe_rgb))
}

/// Parameter eines progressiven Scans (Spektralauswahl Ss..Se,
/// schrittweise Näherung Ah/Al).
struct ScanProg {
    ss: usize,
    se: usize,
    ah: u32,
    al: u32,
}

/// Ein Block eines progressiven Scans (ITU T.81, G.1.2): DC erster Scan /
/// Verfeinerung, AC erster Scan (mit EOB-Läufen) / Verfeinerung.
/// Koeffizienten in natürlicher Reihenfolge, nicht dequantisiert.
fn prog_block(b: &mut JBits, blk: &mut [i32; 64], s: &ScanProg, dc: &JHuff, ac: &JHuff, pred: &mut i32, eobrun: &mut u32) -> Result<(), Fehler> {
    if s.ss == 0 {
        if s.ah == 0 {
            let t = u32::from(b.huff(dc)?);
            let diff = JBits::erweitern(b.bits(t), t);
            *pred += diff;
            blk[0] = *pred * (1 << s.al);
        } else if b.bit() != 0 {
            blk[0] |= 1 << s.al;
        }
        return Ok(());
    }
    if s.ah == 0 {
        // AC, erster Scan
        if *eobrun > 0 {
            *eobrun -= 1;
            return Ok(());
        }
        let mut k = s.ss;
        while k <= s.se {
            let rs = b.huff(ac)?;
            let (r, t) = (u32::from(rs >> 4), u32::from(rs & 15));
            if t == 0 {
                if r < 15 {
                    *eobrun = (1 << r) - 1;
                    if r > 0 {
                        *eobrun += b.bits(r) as u32;
                    }
                    break;
                }
                k += 16;
            } else {
                k += r as usize;
                if k > 63 {
                    return Err(Fehler::Format);
                }
                blk[ZIGZAG[k]] = JBits::erweitern(b.bits(t), t) * (1 << s.al);
                k += 1;
            }
        }
        return Ok(());
    }
    // AC, Verfeinerung
    let p1 = 1i32 << s.al;
    let verfeinern = |b: &mut JBits, c: &mut i32| {
        if b.bit() != 0 && *c & p1 == 0 {
            if *c >= 0 { *c += p1 } else { *c -= p1 }
        }
    };
    let mut k = s.ss;
    if *eobrun == 0 {
        while k <= s.se {
            let rs = b.huff(ac)?;
            let (mut r, t) = (u32::from(rs >> 4), u32::from(rs & 15));
            let mut wert = 0i32;
            if t == 0 {
                if r < 15 {
                    *eobrun = 1 << r;
                    if r > 0 {
                        *eobrun += b.bits(r) as u32;
                    }
                    break;
                }
                // r = 15: 16 Nullen (übersprungen, Ungleiche verfeinert)
            } else {
                if t != 1 {
                    return Err(Fehler::Format);
                }
                wert = if b.bit() != 0 { p1 } else { -p1 };
            }
            while k <= s.se {
                let z = ZIGZAG[k];
                k += 1;
                if blk[z] != 0 {
                    verfeinern(b, &mut blk[z]);
                } else {
                    if r == 0 {
                        if wert != 0 {
                            blk[z] = wert;
                        }
                        break;
                    }
                    r -= 1;
                }
            }
        }
    }
    if *eobrun > 0 {
        // Rest des Blocks: nur vorhandene Koeffizienten verfeinern
        while k <= s.se {
            let z = ZIGZAG[k];
            if blk[z] != 0 {
                verfeinern(b, &mut blk[z]);
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

/// Erster Marker (0xFF + nicht 0, nicht RSTn, nicht Füllbyte) ab `ab`.
fn naechster_marker(d: &[u8], ab: usize) -> Option<usize> {
    let mut i = ab;
    while i + 1 < d.len() {
        if d[i] == 0xFF {
            let m = d[i + 1];
            if m != 0 && m != 0xFF && !(0xD0..=0xD7).contains(&m) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Ebenen hochrechnen (unterabgetastete Komponenten zentriert-bilinear wie
/// libjpegs „fancy upsampling“) und in RGB umwandeln.
fn farbe(komp: &[Komp], w: u32, h: u32, adobe_rgb: bool) -> Rgba {
                let hmax = komp.iter().map(|k| k.h).max().unwrap_or(1);
                let vmax = komp.iter().map(|k| k.v).max().unwrap_or(1);
                let mut px = vec![0u32; (w * h) as usize];
                let kw: Vec<f32> = komp.iter().map(|k| (w * k.h).div_ceil(hmax) as f32).collect();
                let kh: Vec<f32> = komp.iter().map(|k| (h * k.v).div_ceil(vmax) as f32).collect();
                let probe = |ki: usize, x: u32, y: u32| -> i32 {
                    let k = &komp[ki];
                    let p = |sx: usize, sy: usize| f32::from(k.ebene[sy * k.breite + sx]);
                    if k.h == hmax && k.v == vmax {
                        return p(x as usize, y as usize) as i32;
                    }
                    let fx = ((x as f32 + 0.5) * k.h as f32 / hmax as f32 - 0.5).clamp(0.0, kw[ki] - 1.0);
                    let fy = ((y as f32 + 0.5) * k.v as f32 / vmax as f32 - 0.5).clamp(0.0, kh[ki] - 1.0);
                    let (x0, y0) = (fx as usize, fy as usize);
                    let (x1, y1) = ((x0 + 1).min(kw[ki] as usize - 1), (y0 + 1).min(kh[ki] as usize - 1));
                    let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
                    let o = p(x0, y0) * (1.0 - ax) + p(x1, y0) * ax;
                    let u = p(x0, y1) * (1.0 - ax) + p(x1, y1) * ax;
                    (o * (1.0 - ay) + u * ay).round() as i32
                };
                for y in 0..h {
                    for x in 0..w {
                        let c = if komp.len() >= 3 {
                            let (yy, cb, cr) = (probe(0, x, y), probe(1, x, y) - 128, probe(2, x, y) - 128);
                            if adobe_rgb {
                                argb(255, yy as u32, (cb + 128) as u32, (cr + 128) as u32)
                            } else {
                                let r = (yy as f32 + 1.402 * cr as f32).round().clamp(0.0, 255.0) as u32;
                                let g = (yy as f32 - 0.344_136 * cb as f32 - 0.714_136 * cr as f32).round().clamp(0.0, 255.0) as u32;
                                let bl = (yy as f32 + 1.772 * cb as f32).round().clamp(0.0, 255.0) as u32;
                                argb(255, r, g, bl)
                            }
                        } else {
                            let g = probe(0, x, y) as u32;
                            argb(255, g, g, g)
                        };
                        px[(y * w + x) as usize] = c;
                    }
                }
                Rgba { w, h, px, delay: 0 }
}
