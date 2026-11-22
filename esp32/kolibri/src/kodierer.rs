//! Bildkodierer (libimg `img_encode`, nativ): PNG (RGB, 8 Bit) mit zlib/
//! Deflate – LZ77 über eine Hash-Kette und feste Huffman-Codes (RFC 1951,
//! 3.2.6). Für Bildschirmfotos (P6): Bedienoberflächen komprimieren mit
//! Zeilenfilter „Sub“ und LZ77 sehr gut.

use crate::dekoder::Rgba;

/// Bits LSB-zuerst sammeln (Deflate-Bitreihenfolge).
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn neu(kap: usize) -> Bits {
        Bits { out: Vec::with_capacity(kap), acc: 0, n: 0 }
    }
    fn schreiben(&mut self, wert: u32, bits: u32) {
        self.acc |= u64::from(wert) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman-Code (MSB zuerst definiert) umgedreht schreiben.
    fn code(&mut self, code: u32, laenge: u32) {
        let mut r = 0;
        for i in 0..laenge {
            r |= (code >> i & 1) << (laenge - 1 - i);
        }
        self.schreiben(r, laenge);
    }
    fn ende(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

/// Fester Literal/Längen-Code (RFC 1951, 3.2.6).
fn literal(b: &mut Bits, sym: u32) {
    match sym {
        0..=143 => b.code(0x30 + sym, 8),
        144..=255 => b.code(0x190 + sym - 144, 9),
        256..=279 => b.code(sym - 256, 7),
        _ => b.code(0xC0 + sym - 280, 8),
    }
}

const LAENGE_BASIS: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LAENGE_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const ABSTAND_BASIS: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const ABSTAND_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn treffer(b: &mut Bits, laenge: usize, abstand: usize) {
    let li = LAENGE_BASIS.iter().rposition(|&x| usize::from(x) <= laenge).unwrap_or(0);
    literal(b, 257 + li as u32);
    if LAENGE_EXTRA[li] > 0 {
        b.schreiben((laenge - usize::from(LAENGE_BASIS[li])) as u32, u32::from(LAENGE_EXTRA[li]));
    }
    let di = ABSTAND_BASIS.iter().rposition(|&x| usize::from(x) <= abstand).unwrap_or(0);
    b.code(di as u32, 5);
    if ABSTAND_EXTRA[di] > 0 {
        b.schreiben((abstand - usize::from(ABSTAND_BASIS[di])) as u32, u32::from(ABSTAND_EXTRA[di]));
    }
}

/// Rohes Deflate (ein Block, feste Codes, LZ77 mit Hash-Kette).
pub fn deflate(d: &[u8]) -> Vec<u8> {
    const FENSTER: usize = 32 * 1024;
    const HASH: usize = 1 << 15;
    const KETTE: usize = 48;
    let mut b = Bits::neu(d.len() / 4 + 64);
    b.schreiben(1, 1); // letzter Block
    b.schreiben(1, 2); // feste Huffman-Codes
    let mut kopf = vec![u32::MAX; HASH];
    let mut vorher = vec![u32::MAX; FENSTER];
    let hash = |i: usize| ((u32::from(d[i]) << 10 ^ u32::from(d[i + 1]) << 5 ^ u32::from(d[i + 2])).wrapping_mul(2_654_435_761) >> 17) as usize & (HASH - 1);
    let einfuegen = |i: usize, kopf: &mut Vec<u32>, vorher: &mut Vec<u32>| {
        if i + 2 < d.len() {
            let h = hash(i);
            vorher[i % FENSTER] = kopf[h];
            kopf[h] = i as u32;
        }
    };
    let mut i = 0;
    while i < d.len() {
        let (mut beste_l, mut beste_a) = (0usize, 0usize);
        if i + 2 < d.len() {
            let mut k = kopf[hash(i)];
            let mut n = 0;
            while k != u32::MAX && n < KETTE {
                let j = k as usize;
                if i - j > FENSTER - 1 {
                    break;
                }
                let max = (d.len() - i).min(258);
                let mut l = 0;
                while l < max && d[j + l] == d[i + l] {
                    l += 1;
                }
                if l > beste_l {
                    beste_l = l;
                    beste_a = i - j;
                    if l == max {
                        break;
                    }
                }
                let v = vorher[j % FENSTER];
                if v == u32::MAX || v as usize >= j {
                    break;
                }
                k = v;
                n += 1;
            }
        }
        if beste_l >= 3 {
            treffer(&mut b, beste_l, beste_a);
            for k in i..i + beste_l {
                einfuegen(k, &mut kopf, &mut vorher);
            }
            i += beste_l;
        } else {
            literal(&mut b, u32::from(d[i]));
            einfuegen(i, &mut kopf, &mut vorher);
            i += 1;
        }
    }
    literal(&mut b, 256);
    b.ende()
}

fn adler32(d: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for c in d.chunks(5552) {
        for &x in c {
            a += u32::from(x);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    b << 16 | a
}

/// zlib-Strom (RFC 1950) um Deflate.
pub fn zlib(d: &[u8]) -> Vec<u8> {
    let mut v = vec![0x78, 0x01];
    v.extend(deflate(d));
    v.extend_from_slice(&adler32(d).to_be_bytes());
    v
}

fn crc32(d: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &x in d {
        c ^= u32::from(x);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn block(png: &mut Vec<u8>, art: &[u8; 4], daten: &[u8]) {
    png.extend_from_slice(&(daten.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(art);
    png.extend_from_slice(daten);
    let c = crc32(&png[start..]);
    png.extend_from_slice(&c.to_be_bytes());
}

/// Bild als PNG (RGB, 8 Bit je Kanal, Zeilenfilter „Sub“).
pub fn png(b: &Rgba) -> Vec<u8> {
    let (w, h) = (b.w as usize, b.h as usize);
    let mut roh = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..h {
        roh.push(1); // Filter Sub
        let zeile = &b.px[y * w..(y + 1) * w];
        let mut links = [0u8; 3];
        for &p in zeile {
            let rgb = [(p >> 16) as u8, (p >> 8) as u8, p as u8];
            for k in 0..3 {
                roh.push(rgb[k].wrapping_sub(links[k]));
            }
            links = rgb;
        }
    }
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    block(&mut png, b"IHDR", &ihdr);
    block(&mut png, b"IDAT", &zlib(&roh));
    block(&mut png, b"IEND", &[]);
    png
}
