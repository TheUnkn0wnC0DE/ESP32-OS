//! Bild-Export ohne externe Abhängigkeiten: minimaler PNG-Encoder
//! (unkomprimierte Deflate-Blöcke) für Screenshots und ein animierter
//! GIF-Encoder (Median-Cut-Palette, LZW, Differenzbilder) für Clips.

use alloc::vec;
use alloc::vec::Vec;

const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[n] = c;
        n += 1;
    }
    t
}

static CRC_TABLE: [u32; 256] = crc_table();

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for data in chunks {
        for &b in *data {
            c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
    }
    c ^ 0xFFFF_FFFF
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// Kodiert ein `0x00RRGGBB`-Bild als PNG (RGB, 8 Bit).
pub fn encode_png(width: usize, height: usize, pixels: &[u32]) -> Vec<u8> {
    assert_eq!(pixels.len(), width * height);
    // Rohdaten: pro Zeile Filterbyte 0 + RGB.
    let mut raw = Vec::with_capacity(height * (1 + width * 3));
    for row in pixels.chunks_exact(width.max(1)) {
        raw.push(0);
        for &p in row {
            raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    // zlib-Strom mit „stored“-Blöcken.
    let mut z = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 16);
    z.extend_from_slice(&[0x78, 0x01]);
    let mut blocks = raw.chunks(65535).peekable();
    if blocks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(b) = blocks.next() {
        let last = blocks.peek().is_none();
        z.push(last as u8);
        let len = b.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = Vec::with_capacity(z.len() + 64);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&(width as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(height as u32).to_be_bytes());
    ihdr[8] = 8; // Bit-Tiefe
    ihdr[9] = 2; // RGB
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

// ---------------------------------------------------------------------------
// Animiertes GIF
// ---------------------------------------------------------------------------

/// Anzahl der Farbfächer bei 5 Bit pro Kanal (Histogramm, Paletten-Tabelle).
pub const RGB555_BINS: usize = 1 << 15;

/// `0x00RRGGBB` → 15-Bit-Fach (5 Bit pro Kanal).
#[inline]
pub fn rgb555(p: u32) -> usize {
    (((p >> 9) & 0x7C00) | ((p >> 6) & 0x03E0) | ((p >> 3) & 0x001F)) as usize
}

/// Palettenindex für „durchsichtig“ (Pixel wie im Vorbild).
const GIF_TRANSPARENT: u8 = 255;

/// Höchstens 255 Farben (Index 255 ist für Transparenz reserviert) plus
/// Nachschlagetabelle 15-Bit-Farbe → Palettenindex.
pub struct GifPalette {
    colors: Vec<u32>,
    lut: Vec<u8>,
}

impl GifPalette {
    /// Median-Cut über ein Histogramm mit [`RGB555_BINS`] Fächern (Zähler pro
    /// [`rgb555`]-Farbe): zerlegt den Farbraum so lange in Kästen, bis 255
    /// Farben vergeben sind – große Spannweiten zuerst, damit auch kleine,
    /// auffällige Farben (Augen, Kirschen) eine eigene Farbe bekommen.
    pub fn from_histogram(hist: &[u32]) -> Self {
        assert_eq!(hist.len(), RGB555_BINS);
        let chan = |bin: u16, c: usize| -> u32 { ((bin >> (10 - 5 * c)) & 31) as u32 };
        let mut e: Vec<(u16, u32)> =
            hist.iter().enumerate().filter(|&(_, &n)| n > 0).map(|(i, &n)| (i as u16, n)).collect();
        if e.is_empty() {
            e.push((0, 1));
        }
        let mut boxes = vec![(0usize, e.len())];
        while boxes.len() < GIF_TRANSPARENT as usize {
            let mut best: Option<(usize, usize)> = None;
            let mut best_score = 0.0f32;
            for (bi, &(s, t)) in boxes.iter().enumerate() {
                if t - s < 2 {
                    continue;
                }
                let (mut lo, mut hi) = ([31u32; 3], [0u32; 3]);
                let mut pop = 0u64;
                for &(bin, n) in &e[s..t] {
                    for c in 0..3 {
                        lo[c] = lo[c].min(chan(bin, c));
                        hi[c] = hi[c].max(chan(bin, c));
                    }
                    pop += n as u64;
                }
                let (c, range) = (0..3).map(|c| (c, hi[c] - lo[c])).max_by_key(|&(_, r)| r).unwrap_or((0, 0));
                let score = (range * range) as f32 * crate::math::sqrt(pop as f32);
                if score > best_score {
                    best_score = score;
                    best = Some((bi, c));
                }
            }
            let Some((bi, c)) = best else { break };
            let (s, t) = boxes[bi];
            let part = &mut e[s..t];
            part.sort_unstable_by_key(|&(bin, _)| chan(bin, c));
            let total: u64 = part.iter().map(|&(_, n)| n as u64).sum();
            let mut acc = 0u64;
            let mut cut = 1;
            for (i, &(_, n)) in part.iter().enumerate() {
                acc += n as u64;
                if acc * 2 >= total {
                    cut = i + 1;
                    break;
                }
            }
            let cut = cut.clamp(1, part.len() - 1);
            boxes[bi] = (s, s + cut);
            boxes.push((s + cut, t));
        }
        // Farbe eines Kastens = gewichteter Mittelwert seiner Fächer.
        let expand = |v: u32| (v << 3) | (v >> 2);
        let colors: Vec<u32> = boxes
            .iter()
            .map(|&(s, t)| {
                let mut sum = [0u64; 3];
                let mut pop = 0u64;
                for &(bin, n) in &e[s..t] {
                    for (c, acc) in sum.iter_mut().enumerate() {
                        *acc += expand(chan(bin, c)) as u64 * n as u64;
                    }
                    pop += n as u64;
                }
                let m = |c: usize| ((sum[c] + pop / 2) / pop.max(1)) as u32;
                (m(0) << 16) | (m(1) << 8) | m(2)
            })
            .collect();
        // Jedes 15-Bit-Fach auf die nächstgelegene Palettenfarbe abbilden.
        let lut = (0..RGB555_BINS as u32)
            .map(|bin| {
                let (r, g, b) = (expand((bin >> 10) & 31), expand((bin >> 5) & 31), expand(bin & 31));
                let mut best = (u32::MAX, 0u8);
                for (i, &col) in colors.iter().enumerate() {
                    let dr = r as i32 - (col >> 16) as i32;
                    let dg = g as i32 - ((col >> 8) & 255) as i32;
                    let db = b as i32 - (col & 255) as i32;
                    let d = (3 * dr * dr + 4 * dg * dg + 2 * db * db) as u32;
                    if d < best.0 {
                        best = (d, i as u8);
                    }
                }
                best.1
            })
            .collect();
        Self { colors, lut }
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

/// Schreibt ein animiertes GIF (Endlosschleife) Bild für Bild in den Speicher.
/// Ab dem zweiten Bild wird nur das geänderte Rechteck gespeichert, darin
/// unveränderte Pixel als „durchsichtig“ – das spart viel Platz.
pub struct GifEncoder {
    out: Vec<u8>,
    width: usize,
    height: usize,
    lut: Vec<u8>,
    prev: Vec<u8>,
    cur: Vec<u8>,
    rect: Vec<u8>,
    /// LZW-Wörterbuch: (Präfixcode · 256 + Zeichen) → Code (0 = leer).
    dict: Vec<u16>,
    frames: usize,
}

impl GifEncoder {
    pub fn new(width: usize, height: usize, palette: &GifPalette) -> Self {
        assert!(width > 0 && height > 0 && width <= 0xFFFF && height <= 0xFFFF);
        let mut out = Vec::with_capacity(1 << 20);
        out.extend_from_slice(b"GIF89a");
        out.extend_from_slice(&(width as u16).to_le_bytes());
        out.extend_from_slice(&(height as u16).to_le_bytes());
        // Globale Farbtabelle mit 256 Einträgen, 8 Bit Farbauflösung.
        out.extend_from_slice(&[0xF7, 0, 0]);
        for i in 0..256 {
            let c = palette.colors.get(i).copied().unwrap_or(0);
            out.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        }
        // NETSCAPE2.0: Endlosschleife.
        out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
        out.extend_from_slice(b"NETSCAPE2.0");
        out.extend_from_slice(&[0x03, 0x01, 0x00, 0x00, 0x00]);
        Self {
            out,
            width,
            height,
            lut: palette.lut.clone(),
            prev: Vec::new(),
            cur: vec![0; width * height],
            rect: Vec::with_capacity(width * height),
            dict: vec![0; 4096 * 256],
            frames: 0,
        }
    }

    /// Hängt ein Bild (`0x00RRGGBB`, Breite × Höhe) mit `delay_cs`
    /// Hundertstelsekunden Anzeigedauer an.
    pub fn frame(&mut self, pixels: &[u32], delay_cs: u16) {
        let (w, h) = (self.width, self.height);
        assert_eq!(pixels.len(), w * h);
        for (d, &p) in self.cur.iter_mut().zip(pixels) {
            *d = self.lut[rgb555(p)];
        }
        // Geändertes Rechteck gegenüber dem Vorbild.
        let (mut x0, mut y0, mut x1, mut y1) = (0, 0, w, h);
        let diff = !self.prev.is_empty();
        if diff {
            (x0, y0, x1, y1) = (w, h, 0, 0);
            for y in 0..h {
                let (a, b) = (&self.cur[y * w..(y + 1) * w], &self.prev[y * w..(y + 1) * w]);
                if let Some(first) = a.iter().zip(b).position(|(p, q)| p != q) {
                    let last = w - a.iter().rev().zip(b.iter().rev()).position(|(p, q)| p != q).unwrap_or(0);
                    x0 = x0.min(first);
                    x1 = x1.max(last);
                    y0 = y0.min(y);
                    y1 = y + 1;
                }
            }
            if x0 >= x1 {
                (x0, y0, x1, y1) = (0, 0, 1, 1);
            }
        }
        self.rect.clear();
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * w + x;
                let c = self.cur[i];
                self.rect.push(if diff && c == self.prev[i] { GIF_TRANSPARENT } else { c });
            }
        }
        let o = &mut self.out;
        // Grafiksteuerung: „nicht entsorgen“, Verzögerung, ggf. Transparenz.
        o.extend_from_slice(&[0x21, 0xF9, 0x04, (1 << 2) | diff as u8]);
        o.extend_from_slice(&delay_cs.to_le_bytes());
        o.extend_from_slice(&[GIF_TRANSPARENT, 0x00]);
        o.push(0x2C);
        for v in [x0, y0, x1 - x0, y1 - y0] {
            o.extend_from_slice(&(v as u16).to_le_bytes());
        }
        o.push(0);
        lzw_encode(o, &mut self.dict, &self.rect);
        core::mem::swap(&mut self.prev, &mut self.cur);
        if self.cur.len() != w * h {
            self.cur = vec![0; w * h];
        }
        self.frames += 1;
    }

    pub fn frame_count(&self) -> usize {
        self.frames
    }

    /// Schließt die Datei ab und liefert die Bytes.
    pub fn finish(mut self) -> Vec<u8> {
        self.out.push(0x3B);
        self.out
    }
}

/// Bitstrom (LSB zuerst) in GIF-Unterblöcken zu je höchstens 255 Bytes.
struct BitSink<'a> {
    out: &'a mut Vec<u8>,
    block: [u8; 255],
    len: usize,
    acc: u32,
    bits: u32,
}

impl BitSink<'_> {
    #[inline]
    fn put(&mut self, code: u32, size: u32) {
        self.acc |= code << self.bits;
        self.bits += size;
        while self.bits >= 8 {
            self.byte(self.acc as u8);
            self.acc >>= 8;
            self.bits -= 8;
        }
    }

    #[inline]
    fn byte(&mut self, b: u8) {
        self.block[self.len] = b;
        self.len += 1;
        if self.len == 255 {
            self.flush_block();
        }
    }

    fn flush_block(&mut self) {
        if self.len > 0 {
            self.out.push(self.len as u8);
            self.out.extend_from_slice(&self.block[..self.len]);
            self.len = 0;
        }
    }

    fn finish(mut self) {
        if self.bits > 0 {
            self.byte(self.acc as u8);
        }
        self.flush_block();
        self.out.push(0);
    }
}

/// LZW-Kompression nach GIF-Art (8-Bit-Zeichen, Codes 9…12 Bit, Neustart des
/// Wörterbuchs bei 4096 Einträgen).
fn lzw_encode(out: &mut Vec<u8>, dict: &mut [u16], data: &[u8]) {
    const MIN: u32 = 8;
    const CLEAR: u32 = 1 << MIN;
    const EOI: u32 = CLEAR + 1;
    out.push(MIN as u8);
    dict.fill(0);
    let mut sink = BitSink { out, block: [0; 255], len: 0, acc: 0, bits: 0 };
    let mut size = MIN + 1;
    let mut max_code = EOI;
    sink.put(CLEAR, size);
    let Some((&first, rest)) = data.split_first() else {
        sink.put(EOI, size);
        sink.finish();
        return;
    };
    let mut cur = first as u32;
    for &k in rest {
        let slot = ((cur as usize) << 8) | k as usize;
        let next = dict[slot];
        if next != 0 {
            cur = next as u32;
            continue;
        }
        sink.put(cur, size);
        max_code += 1;
        dict[slot] = max_code as u16;
        if max_code >= 1 << size {
            size += 1;
        }
        if max_code == 4095 {
            sink.put(CLEAR, size);
            dict.fill(0);
            size = MIN + 1;
            max_code = EOI;
        }
        cur = k as u32;
    }
    sink.put(cur, size);
    // Der Decoder legt beim Lesen von `cur` noch einen Eintrag an und
    // verbreitert ggf. seine Codes – das Endezeichen muss dazu passen.
    if max_code > EOI && max_code + 1 >= 1 << size && size < 12 {
        size += 1;
    }
    sink.put(EOI, size);
    sink.finish();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Referenz-Decoder (Verhalten wie giflib) für Rundreise-Tests.
    fn lzw_decode(stream: &[u8]) -> Vec<u8> {
        let min = stream[0] as u32;
        let mut data = Vec::new();
        let mut i = 1;
        while stream[i] != 0 {
            let n = stream[i] as usize;
            data.extend_from_slice(&stream[i + 1..i + 1 + n]);
            i += 1 + n;
        }
        let clear = 1u32 << min;
        let eoi = clear + 1;
        let reset = || -> Vec<Vec<u8>> {
            let mut t: Vec<Vec<u8>> = (0..clear).map(|c| vec![c as u8]).collect();
            t.push(Vec::new());
            t.push(Vec::new());
            t
        };
        let mut table = reset();
        let mut size = min + 1;
        let mut prev: Option<Vec<u8>> = None;
        let mut out = Vec::new();
        let (mut acc, mut bits, mut pos) = (0u32, 0u32, 0usize);
        loop {
            while bits < size {
                acc |= (*data.get(pos).expect("Datenende vor EOI") as u32) << bits;
                pos += 1;
                bits += 8;
            }
            let code = acc & ((1 << size) - 1);
            acc >>= size;
            bits -= size;
            if code == clear {
                table = reset();
                size = min + 1;
                prev = None;
                continue;
            }
            if code == eoi {
                break;
            }
            let entry = if (code as usize) < table.len() {
                table[code as usize].clone()
            } else {
                let p = prev.clone().expect("ungültiger Code");
                assert_eq!(code as usize, table.len());
                let mut e = p.clone();
                e.push(p[0]);
                e
            };
            out.extend_from_slice(&entry);
            if let Some(p) = prev {
                if table.len() < 4096 {
                    let mut e = p;
                    e.push(entry[0]);
                    table.push(e);
                }
            }
            prev = Some(entry);
            if table.len() == 1 << size && size < 12 {
                size += 1;
            }
        }
        out
    }

    #[test]
    fn lzw_roundtrip() {
        let mut dict = vec![0u16; 4096 * 256];
        let mut rng = 12345u32;
        let mut cases: Vec<Vec<u8>> = vec![vec![], vec![7], vec![1, 1, 1, 1, 1, 1, 1, 1, 1], (0..=255).collect()];
        // Viel Text, damit das Wörterbuch mehrfach überläuft (12 Bit, Neustart).
        for len in [3000usize, 70_000, 200_003] {
            cases.push(
                (0..len)
                    .map(|i| {
                        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        if i % 7 < 3 { (rng >> 24) as u8 } else { (i / 13 % 9) as u8 }
                    })
                    .collect(),
            );
        }
        for data in &cases {
            let mut s = Vec::new();
            lzw_encode(&mut s, &mut dict, data);
            assert_eq!(&lzw_decode(&s), data, "Länge {}", data.len());
        }
    }

    #[test]
    fn gif_structure_and_palette() {
        let mut hist = vec![0u32; RGB555_BINS];
        let img: Vec<u32> =
            (0..64 * 32).map(|i| if i % 64 < 32 { 0xFF2040 } else { 0x40C0FF + (i / 64) as u32 }).collect();
        for &p in &img {
            hist[rgb555(p)] += 1;
        }
        let pal = GifPalette::from_histogram(&hist);
        assert!(!pal.is_empty() && pal.len() <= 255);
        let red = pal.colors[pal.lut[rgb555(0xFF2040)] as usize];
        assert!(((red >> 16) as i32 - 0xFF).abs() < 8 && ((red & 255) as i32 - 0x40).abs() < 8);
        let mut enc = GifEncoder::new(64, 32, &pal);
        enc.frame(&img, 4);
        let mut img2 = img.clone();
        img2[5 * 64 + 10] = 0x000000;
        enc.frame(&img2, 4);
        enc.frame(&img2, 4);
        assert_eq!(enc.frame_count(), 3);
        let gif = enc.finish();
        assert_eq!(&gif[..6], b"GIF89a");
        assert_eq!(*gif.last().unwrap(), 0x3B);
    }

    #[test]
    fn png_has_valid_structure() {
        let png = encode_png(2, 2, &[0xFF0000, 0x00FF00, 0x0000FF, 0xFFFFFF]);
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
