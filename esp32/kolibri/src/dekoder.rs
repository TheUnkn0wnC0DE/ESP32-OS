//! Bilddekoder für libimg (ohne externe Abhängigkeiten): BMP, PNG (inkl.
//! Interlace, alle Farbtypen/Tiefen, tRNS), GIF (inkl. Animation,
//! Transparenz, Interlace), JPEG Baseline und progressiv (Huffman, 1/3
//! Komponenten, Unterabtastung, Restart-Marker). Ausgabe: Bilder in
//! 0xAARRGGBB. Grosse JPEG/PNG liest [`verkleinert`] stückweise und
//! verkleinert schon beim Dekodieren.

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
    /// Bekanntes Format, aber nicht unterstützte Variante (z. B. 12-Bit-JPEG).
    NichtUnterstuetzt,
    /// Auch verkleinert zu gross für Bildpunkt- bzw. Speichergrenze.
    ZuGross,
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

// =================================================================== Stückweise Eingabe

/// Bytequelle der Dekoder: ein Speicherbereich oder eine Datei, die
/// stückweise gelesen wird (`lesen(ab, puffer) -> gelesen`, 0 = Ende) –
/// so liegt ein grosses Foto nie ganz im Speicher.
pub struct Strom<'a> {
    quelle: Option<Box<dyn FnMut(u64, &mut [u8]) -> usize + 'a>>,
    puffer: Vec<u8>,
    speicher: &'a [u8],
    pos: usize,
    len: usize,
    /// Dateiposition hinter `puffer[..len]`
    datei: u64,
}

/// Lesepuffer bei Dateien.
/// 256 KB: jedes Lesen öffnet die Datei neu und sucht die Stelle (Funktion
/// 70), am Board je Aufruf 9–18 ms.
const STROM_PUFFER: usize = 256 * 1024;

impl<'a> Strom<'a> {
    pub fn speicher(d: &'a [u8]) -> Strom<'a> {
        Strom { quelle: None, puffer: Vec::new(), speicher: d, pos: 0, len: d.len(), datei: 0 }
    }

    pub fn datei(lesen: impl FnMut(u64, &mut [u8]) -> usize + 'a) -> Strom<'a> {
        Strom { quelle: Some(Box::new(lesen)), puffer: vec![0; STROM_PUFFER], speicher: &[], pos: 0, len: 0, datei: 0 }
    }

    /// Mindestens `n` (≤ Puffergrösse) Bytes ab `pos` bereitstellen.
    fn bereit(&mut self, n: usize) -> bool {
        if self.len - self.pos >= n {
            return true;
        }
        let Strom { quelle, puffer, pos, len, datei, .. } = self;
        let Some(q) = quelle.as_mut() else { return false };
        puffer.copy_within(*pos..*len, 0);
        *len -= *pos;
        *pos = 0;
        while *len < n {
            let m = q(*datei, &mut puffer[*len..]);
            if m == 0 {
                return false;
            }
            *len += m;
            *datei += m as u64;
        }
        true
    }

    /// Die gerade bereitstehenden Bytes (`[pos..len]` gültig).
    #[inline]
    fn daten(&self) -> &[u8] {
        if self.quelle.is_some() { &self.puffer } else { self.speicher }
    }

    /// Byte `i` ab der Leseposition, ohne weiterzugehen.
    #[inline]
    fn schauen(&mut self, i: usize) -> Option<u8> {
        if !self.bereit(i + 1) {
            return None;
        }
        Some(if self.quelle.is_some() { self.puffer[self.pos + i] } else { self.speicher[self.pos + i] })
    }

    /// Weitergehen (nur über schon angesehene Bytes).
    #[inline]
    fn vor(&mut self, n: usize) {
        self.pos += n;
    }

    fn byte(&mut self) -> Option<u8> {
        let b = self.schauen(0)?;
        self.pos += 1;
        Some(b)
    }

    fn lesen_in(&mut self, ziel: &mut [u8]) -> Result<(), Fehler> {
        for z in ziel {
            *z = self.byte().ok_or(Fehler::Format)?;
        }
        Ok(())
    }

    fn be32(&mut self) -> Result<u32, Fehler> {
        let mut b = [0u8; 4];
        self.lesen_in(&mut b)?;
        Ok(u32::from_be_bytes(b))
    }

    /// `n` Bytes (Segmente, Kopfdaten – höchstens 16 MB).
    fn nehmen(&mut self, n: usize) -> Result<Vec<u8>, Fehler> {
        if n > 1 << 24 {
            return Err(Fehler::Format);
        }
        let mut v = vec![0u8; n];
        self.lesen_in(&mut v)?;
        Ok(v)
    }

    fn ueberspringen(&mut self, n: u64) -> Result<(), Fehler> {
        let da = (self.len - self.pos) as u64;
        if da >= n {
            self.pos += n as usize;
            return Ok(());
        }
        if self.quelle.is_none() {
            return Err(Fehler::Format);
        }
        // Datei: ohne zu lesen weiterspringen
        self.datei += n - da;
        self.pos = 0;
        self.len = 0;
        Ok(())
    }
}

/// Ein verkleinert dekodiertes Bild.
#[derive(Debug)]
pub struct Verkleinert {
    pub bild: Rgba,
    pub format: Format,
    /// Masse der Datei (vor Verkleinern und Drehen)
    pub original: (u32, u32),
    /// Verkleinerungsfaktor (1 = volle Grösse)
    pub teiler: u32,
}

/// Grosses JPEG oder PNG stückweise lesen und beim Dekodieren verkleinern
/// (Bildbetrachter, Handyfotos mit 12–48 MP): JPEG um 1/2, 1/4 oder 1/8
/// gleich in der IDCT, PNG Zeile für Zeile um einen ganzzahligen Faktor.
/// Gewählt wird der kleinste Faktor, bei dem das Ergebnis höchstens
/// `max_px` Bildpunkte hat und das Dekodieren höchstens `max_bytes`
/// Speicher braucht. JPEG wird nach EXIF gedreht. Die Datei
/// (`lesen(ab, puffer) -> gelesen`) liegt nie ganz im Speicher.
pub fn verkleinert(lesen: impl FnMut(u64, &mut [u8]) -> usize, max_px: u64, max_bytes: u64) -> Result<Verkleinert, Fehler> {
    let mut s = Strom::datei(lesen);
    match (s.schauen(0), s.schauen(1)) {
        (Some(0xFF), Some(0xD8)) => {
            let wahl = |w: u32, h: u32, progressiv: bool, bloecke: u64| -> Option<u32> {
                [1u32, 2, 4, 8].into_iter().find(|&f| {
                    let n = u64::from(8 / f);
                    let px = u64::from(w.div_ceil(f)) * u64::from(h.div_ceil(f));
                    // Ebenen (n×n je Block), Ergebnis, progressiv Masken + Werte
                    let bedarf = bloecke * n * n + px * 4 + if progressiv { bloecke * (8 + 2 * n * n) } else { 0 } + 2 * STROM_PUFFER as u64;
                    px <= max_px && bedarf <= max_bytes
                })
            };
            let (bild, original, teiler) = jpeg_strom(&mut s, &wahl, true)?;
            Ok(Verkleinert { bild, format: Format::Jpeg, original, teiler })
        }
        (Some(0x89), Some(b'P')) => {
            let wahl = |w: u32, h: u32| -> Option<u32> {
                (1u32..=64).find(|&f| {
                    let (ow, oh) = (u64::from(w.div_ceil(f)), u64::from(h.div_ceil(f)));
                    // Ergebnis, Zeilen (bis 8 Byte je Punkt), Summen, Fenster
                    let bedarf = ow * oh * 4 + u64::from(w) * 16 + ow * 20 + 4 * FENSTER as u64 + STROM_PUFFER as u64;
                    ow * oh <= max_px && bedarf <= max_bytes
                })
            };
            let (bild, original) = png_strom(&mut s, &wahl)?;
            let teiler = original.0.div_ceil(bild.w.max(1));
            Ok(Verkleinert { bild, format: Format::Png, original, teiler })
        }
        _ => Err(Fehler::NichtUnterstuetzt),
    }
}

/// EXIF-Ausrichtung eines JPEG im Speicher (1 = normal; siehe
/// [`verkleinert`]): Bildbetrachter wählt danach den Weg.
pub fn jpeg_ausrichtung(d: &[u8]) -> u16 {
    let mut o = 2;
    while o + 4 <= d.len() && d[o] == 0xFF {
        let m = d[o + 1];
        let len = usize::from(u16::from_be_bytes([d[o + 2], d[o + 3]]));
        if m == 0xDA || len < 2 {
            break;
        }
        if m == 0xE1
            && let Some(v) = d.get(o + 4..o + 2 + len).and_then(exif_ausrichtung)
        {
            return v;
        }
        o += 2 + len;
    }
    1
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

/// Eingabe des Inflaters: Bytes nacheinander (Speicher oder Datei).
trait Eingabe {
    fn byte(&mut self) -> Option<u8>;
}

/// Bytes eines Speicherbereichs.
struct SpeicherEingabe<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Eingabe for SpeicherEingabe<'_> {
    #[inline]
    fn byte(&mut self) -> Option<u8> {
        let b = self.d.get(self.pos).copied();
        self.pos += 1;
        b
    }
}

struct Bits<E: Eingabe> {
    e: E,
    puffer: u32,
    anzahl: u32,
}

impl<E: Eingabe> Bits<E> {
    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32, Fehler> {
        while self.anzahl < n {
            let b = self.e.byte().ok_or(Fehler::Format)?;
            self.puffer |= u32::from(b) << self.anzahl;
            self.anzahl += 8;
        }
        let v = self.puffer & ((1u64 << n) - 1) as u32;
        self.puffer >>= n;
        self.anzahl -= n;
        Ok(v)
    }
}

/// Ausgabe des Inflaters: ganzer Speicher ([`VecAusgabe`]) oder stückweise
/// mit 32-KB-Fenster ([`StromAusgabe`]).
trait Ausgabe {
    fn byte(&mut self, b: u8) -> Result<(), Fehler>;
    /// `len` Bytes ab `dist` zurück wiederholen (LZ77).
    fn kopieren(&mut self, dist: usize, len: usize) -> Result<(), Fehler>;
}

struct VecAusgabe {
    out: Vec<u8>,
    max: usize,
}

impl Ausgabe for VecAusgabe {
    #[inline]
    fn byte(&mut self, b: u8) -> Result<(), Fehler> {
        if self.out.len() >= self.max {
            return Err(Fehler::Format);
        }
        self.out.push(b);
        Ok(())
    }
    fn kopieren(&mut self, dist: usize, len: usize) -> Result<(), Fehler> {
        if dist > self.out.len() || self.out.len() + len > self.max {
            return Err(Fehler::Format);
        }
        let start = self.out.len() - dist;
        for j in 0..len {
            let b = self.out[start + j];
            self.out.push(b);
        }
        Ok(())
    }
}

/// Fenster des Deflate-Formats.
const FENSTER: usize = 32 * 1024;

/// Ausgabe stückweise an `senke`; behält die letzten 32 KB für Rückverweise.
struct StromAusgabe<'s> {
    puf: Vec<u8>,
    gesamt: usize,
    senke: &'s mut dyn FnMut(&[u8]) -> Result<(), Fehler>,
}

impl StromAusgabe<'_> {
    fn abgeben(&mut self, alles: bool) -> Result<(), Fehler> {
        let behalten = if alles { 0 } else { FENSTER };
        if self.puf.len() > behalten {
            let n = self.puf.len() - behalten;
            (self.senke)(&self.puf[..n])?;
            self.puf.drain(..n);
        }
        Ok(())
    }
}

impl Ausgabe for StromAusgabe<'_> {
    #[inline]
    fn byte(&mut self, b: u8) -> Result<(), Fehler> {
        self.puf.push(b);
        self.gesamt += 1;
        if self.puf.len() >= 3 * FENSTER {
            self.abgeben(false)?;
        }
        Ok(())
    }
    fn kopieren(&mut self, dist: usize, len: usize) -> Result<(), Fehler> {
        if dist > self.puf.len() {
            return Err(Fehler::Format);
        }
        for _ in 0..len {
            let b = self.puf[self.puf.len() - dist];
            self.byte(b)?;
        }
        Ok(())
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
    fn lesen<E: Eingabe>(&self, b: &mut Bits<E>) -> Result<u16, Fehler> {
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
    inflate_max(d, groesse_hinweis, usize::MAX)
}

/// Wie [`inflate`], aber höchstens `max` Bytes Ergebnis (sonst Fehler) –
/// gegen „Deflate-Bomben“ in fremden Dateien (PDF-Betrachter).
pub fn inflate_max(d: &[u8], groesse_hinweis: usize, max: usize) -> Result<Vec<u8>, Fehler> {
    let mut b = Bits { e: SpeicherEingabe { d, pos: 0 }, puffer: 0, anzahl: 0 };
    let mut a = VecAusgabe { out: Vec::with_capacity(groesse_hinweis.min(max)), max };
    inflate_kern(&mut b, &mut a)?;
    Ok(a.out)
}

/// Deflate-Blöcke bis zum letzten lesen.
fn inflate_kern<E: Eingabe, A: Ausgabe>(b: &mut Bits<E>, out: &mut A) -> Result<(), Fehler> {
    loop {
        let letzter = b.bits(1)?;
        match b.bits(2)? {
            0 => {
                // unkomprimiert: auf Byte ausrichten, LEN/NLEN, Daten
                b.puffer = 0;
                b.anzahl = 0;
                let mut kopf = [0u8; 4];
                for k in &mut kopf {
                    *k = b.e.byte().ok_or(Fehler::Format)?;
                }
                let len = usize::from(u16::from_le_bytes([kopf[0], kopf[1]]));
                for _ in 0..len {
                    out.byte(b.e.byte().ok_or(Fehler::Format)?)?;
                }
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
                        let s = ch.lesen(b)?;
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
                    let s = lit.lesen(b)?;
                    if s < 256 {
                        out.byte(s as u8)?;
                    } else if s == 256 {
                        break;
                    } else {
                        let k = (s - 257) as usize;
                        let len = *LEN_BASIS.get(k).ok_or(Fehler::Format)? as usize + b.bits(u32::from(LEN_EXTRA[k]))? as usize;
                        let ds = dist.lesen(b)? as usize;
                        let dd = *DIST_BASIS.get(ds).ok_or(Fehler::Format)? as usize + b.bits(u32::from(DIST_EXTRA[ds]))? as usize;
                        out.kopieren(dd, len)?;
                    }
                }
            }
            _ => return Err(Fehler::Format),
        }
        if letzter == 1 {
            return Ok(());
        }
    }
}

// =================================================================== PNG

/// Kopfdaten eines PNG.
struct PngKopf {
    w: u32,
    h: u32,
    tiefe: u8,
    typ: u8,
    interlace: u8,
    pal: Vec<[u8; 3]>,
    trns: Vec<u8>,
}

impl PngKopf {
    fn kanaele(&self) -> Result<u32, Fehler> {
        match self.typ {
            0 | 3 => Ok(1),
            2 => Ok(3),
            4 => Ok(2),
            6 => Ok(4),
            _ => Err(Fehler::Format),
        }
    }

    /// Bildpunkt `px_i` einer entfilterten Zeile als 0xAARRGGBB.
    fn pixel(&self, z: &[u8], px_i: usize, kanaele: usize) -> u32 {
        let tiefe = u32::from(self.tiefe);
        let max_wert = (1u32 << tiefe) - 1;
        let grau = |v: u32| if tiefe == 16 { v >> 8 } else { v * 255 / max_wert };
        let probe = |k: usize| -> u32 {
            match tiefe {
                16 => u32::from(u16::from_be_bytes([z[2 * k], z[2 * k + 1]])),
                8 => u32::from(z[k]),
                t => {
                    let bit = k * t as usize;
                    u32::from(z[bit / 8] >> (8 - t as usize - bit % 8)) & max_wert
                }
            }
        };
        let trns = &self.trns;
        let k = px_i * kanaele;
        match self.typ {
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
                let p = self.pal.get(i).copied().unwrap_or([0, 0, 0]);
                let a = trns.get(i).copied().unwrap_or(255);
                argb(u32::from(a), u32::from(p[0]), u32::from(p[1]), u32::from(p[2]))
            }
            4 => {
                let g = grau(probe(k));
                argb(grau(probe(k + 1)), g, g, g)
            }
            _ => argb(grau(probe(k + 3)), grau(probe(k)), grau(probe(k + 1)), grau(probe(k + 2))),
        }
    }
}

/// PNG-Filter einer Zeile rückgängig machen.
fn png_entfiltern(filter: u8, z: &mut [u8], vorher: &[u8], bpp: usize) -> Result<(), Fehler> {
    for i in 0..z.len() {
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
    Ok(())
}

fn png(d: &[u8]) -> Result<Rgba, Fehler> {
    let mut s = Strom::speicher(d);
    png_strom(&mut s, &|w, h| (w <= 16384 && h <= 16384).then_some(1)).map(|(b, _)| b)
}

/// PNG zeilenweise aus `s`: Daten werden beim Entpacken Zeile für Zeile
/// entfiltert und um den ganzzahligen Faktor `teiler(w, h)` verkleinert
/// (Mittelwert der Bildpunkte; bei Interlace der nächste Bildpunkt). So
/// passt auch ein Bild, dessen Rohdaten den Speicher weit übersteigen.
/// Liefert das Bild und die Originalmasse.
fn png_strom(s: &mut Strom, teiler: &dyn Fn(u32, u32) -> Option<u32>) -> Result<(Rgba, (u32, u32)), Fehler> {
    let mut sig = [0u8; 8];
    s.lesen_in(&mut sig)?;
    if &sig != b"\x89PNG\r\n\x1a\n" {
        return Err(Fehler::Format);
    }
    let mut k = PngKopf { w: 0, h: 0, tiefe: 0, typ: 0, interlace: 0, pal: Vec::new(), trns: Vec::new() };
    // Stücke bis zum ersten IDAT
    let rest_idat;
    loop {
        let len = s.be32()? as usize;
        let mut art = [0u8; 4];
        s.lesen_in(&mut art)?;
        match &art {
            b"IHDR" => {
                // immer 13 Byte; die Länge nicht ungeprüft anlegen
                if len != 13 {
                    return Err(Fehler::Format);
                }
                let inhalt = s.nehmen(len)?;
                k.w = u32::from_be_bytes([inhalt[0], inhalt[1], inhalt[2], inhalt[3]]);
                k.h = u32::from_be_bytes([inhalt[4], inhalt[5], inhalt[6], inhalt[7]]);
                k.tiefe = inhalt[8];
                k.typ = inhalt[9];
                k.interlace = inhalt[12];
            }
            b"PLTE" if len <= 768 => k.pal = s.nehmen(len)?.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
            b"tRNS" if len <= 256 => k.trns = s.nehmen(len)?,
            b"IDAT" => {
                rest_idat = len;
                break;
            }
            b"IEND" => return Err(Fehler::Format),
            _ => s.ueberspringen(len as u64)?,
        }
        s.ueberspringen(4)?; // CRC
    }
    let (w, h) = (k.w, k.h);
    if w == 0 || h == 0 || w > 65535 || h > 65535 || !matches!(k.tiefe, 1 | 2 | 4 | 8 | 16) {
        return Err(Fehler::Format);
    }
    let kanaele = k.kanaele()? as usize;
    let f = teiler(w, h).ok_or(Fehler::ZuGross)?.max(1);
    let (ow, oh) = (w.div_ceil(f), h.div_ceil(f));
    let bpp_bits = kanaele * usize::from(k.tiefe);
    let bpp = bpp_bits.div_ceil(8).max(1);
    let mut px = vec![0u32; ow as usize * oh as usize];
    let durchgaenge: &[(u32, u32, u32, u32)] = if k.interlace == 1 {
        &[(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]
    } else {
        &[(0, 0, 1, 1)]
    };
    // Zustand der Zeilenverarbeitung
    let mut gang = 0usize;
    let mut py = 0u32;
    let mut zeile: Vec<u8> = Vec::new();
    let mut vorher: Vec<u8> = Vec::new();
    let mut voll = 0usize;
    // Summen einer Ausgabezeile (a, r, g, b, Anzahl) beim Mittelwert
    let mut summe = vec![[0u32; 5]; if k.interlace == 1 { 0 } else { ow as usize }];
    let mut fertig = false;
    // Masse des aktuellen Durchgangs (leere Durchgänge übersprungen)
    let gang_masse = |g: usize| -> Option<(u32, usize)> {
        let (x0, y0, dx, dy) = durchgaenge[g];
        (x0 < w && y0 < h).then(|| {
            let pw = (w - x0).div_ceil(dx);
            ((h - y0).div_ceil(dy), (pw as usize * bpp_bits).div_ceil(8))
        })
    };
    let naechster_gang = |g: &mut usize, zeile: &mut Vec<u8>, vorher: &mut Vec<u8>| -> bool {
        while *g < durchgaenge.len() {
            if let Some((_, zb)) = gang_masse(*g) {
                *zeile = vec![0; 1 + zb];
                *vorher = vec![0; zb];
                return true;
            }
            *g += 1;
        }
        false
    };
    if !naechster_gang(&mut gang, &mut zeile, &mut vorher) {
        return Err(Fehler::Format);
    }
    {
        let mut senke = |daten: &[u8]| -> Result<(), Fehler> {
            let mut i = 0;
            while i < daten.len() {
                if fertig {
                    return Ok(());
                }
                let n = (zeile.len() - voll).min(daten.len() - i);
                zeile[voll..voll + n].copy_from_slice(&daten[i..i + n]);
                voll += n;
                i += n;
                if voll < zeile.len() {
                    break;
                }
                // ganze Zeile: entfiltern, Bildpunkte ausgeben
                voll = 0;
                let (filter, z) = zeile.split_first_mut().ok_or(Fehler::Format)?;
                png_entfiltern(*filter, z, &vorher, bpp)?;
                let (x0, y0, dx, dy) = durchgaenge[gang];
                let (ph, _) = gang_masse(gang).ok_or(Fehler::Format)?;
                let pw = (w - x0).div_ceil(dx) as usize;
                let y = y0 + py * dy;
                if k.interlace == 1 {
                    // nächster Bildpunkt: nur Punkte auf dem Raster des Faktors
                    if y % f == 0 {
                        for px_i in 0..pw {
                            let x = x0 + px_i as u32 * dx;
                            if x % f == 0 {
                                px[((y / f) * ow + x / f) as usize] = k.pixel(z, px_i, kanaele);
                            }
                        }
                    }
                } else {
                    for px_i in 0..pw {
                        let c = k.pixel(z, px_i, kanaele);
                        let s = &mut summe[px_i / f as usize];
                        s[0] += c >> 24;
                        s[1] += c >> 16 & 255;
                        s[2] += c >> 8 & 255;
                        s[3] += c & 255;
                        s[4] += 1;
                    }
                    if (y + 1) % f == 0 || y + 1 == h {
                        let oy = (y / f) as usize;
                        for (ox, s) in summe.iter_mut().enumerate() {
                            let n = s[4].max(1);
                            px[oy * ow as usize + ox] = argb(s[0] / n, s[1] / n, s[2] / n, s[3] / n);
                            *s = [0; 5];
                        }
                    }
                }
                vorher.copy_from_slice(z);
                py += 1;
                if py == ph {
                    py = 0;
                    gang += 1;
                    if !naechster_gang(&mut gang, &mut zeile, &mut vorher) {
                        fertig = true;
                    }
                }
            }
            Ok(())
        };
        // IDAT-Stücke hintereinander als eine zlib-Folge
        let mut idat = IdatEingabe { s, rest: rest_idat, ende: false };
        let mut kopf = [0u8; 2];
        for b in &mut kopf {
            *b = idat.byte().ok_or(Fehler::Format)?;
        }
        let mut bits = Bits { e: idat, puffer: 0, anzahl: 0 };
        let mut aus = StromAusgabe { puf: Vec::with_capacity(4 * FENSTER), gesamt: 0, senke: &mut senke };
        inflate_kern(&mut bits, &mut aus)?;
        aus.abgeben(true)?;
    }
    if !fertig {
        return Err(Fehler::Format);
    }
    Ok((Rgba { w: ow, h: oh, px, delay: 0 }, (w, h)))
}

/// Bytes aller aufeinanderfolgenden IDAT-Stücke.
struct IdatEingabe<'s, 'a> {
    s: &'s mut Strom<'a>,
    rest: usize,
    ende: bool,
}

impl Eingabe for IdatEingabe<'_, '_> {
    fn byte(&mut self) -> Option<u8> {
        while self.rest == 0 {
            if self.ende {
                return None;
            }
            // CRC, dann nächstes Stück
            self.s.ueberspringen(4).ok()?;
            let len = self.s.be32().ok()? as usize;
            let mut art = [0u8; 4];
            self.s.lesen_in(&mut art).ok()?;
            if &art != b"IDAT" {
                self.ende = true;
                return None;
            }
            self.rest = len;
        }
        self.rest -= 1;
        self.s.byte()
    }
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

#[derive(Clone)]
struct JHuff {
    /// (Länge, Code) → Wert: je Länge erster Code und Index
    min: [i32; 17],
    max: [i32; 18],
    val_ptr: [i32; 17],
    werte: Vec<u8>,
    /// Schnelltabelle für Codes bis 9 Bit: Länge << 8 | Wert (0 = länger)
    lut: Vec<u16>,
}

impl Default for JHuff {
    fn default() -> JHuff {
        JHuff { min: [0; 17], max: [-1; 18], val_ptr: [0; 17], werte: Vec::new(), lut: vec![0; 1 << LUT_BITS] }
    }
}

const LUT_BITS: u32 = 9;

impl JHuff {
    fn neu(anz: &[u8; 16], werte: Vec<u8>) -> JHuff {
        let mut h = JHuff { werte, ..Default::default() };
        let (mut code, mut k) = (0i32, 0i32);
        for l in 1..=16 {
            let n = i32::from(anz[l - 1]);
            h.val_ptr[l] = k;
            h.min[l] = code;
            if l as u32 <= LUT_BITS {
                for c in code..code + n {
                    let Some(&wert) = h.werte.get((k + c - code) as usize) else { break };
                    let schieben = LUT_BITS - l as u32;
                    let anfang = (c as usize) << schieben;
                    for e in anfang..(anfang + (1 << schieben)).min(1 << LUT_BITS) {
                        h.lut[e] = (l as u16) << 8 | u16::from(wert);
                    }
                }
            }
            code += n;
            k += n;
            h.max[l] = if n > 0 { code - 1 } else { -1 };
            code <<= 1;
        }
        h.max[17] = i32::MAX;
        h
    }
}

/// Bitleser für die Entropiedaten eines Scans (Füllbytes FF 00, Halt an
/// Markern: danach Nullen). 32-Bit-Speicher, linksbündig (RISC-V-32 hat
/// keine 64-Bit-Register): nach `fuellen` mindestens 25 gültige Bits.
struct JBits<'s, 'a> {
    s: &'s mut Strom<'a>,
    acc: u32,
    n: u32,
    marker: bool,
}

impl<'s, 'a> JBits<'s, 'a> {
    fn neu(s: &'s mut Strom<'a>) -> JBits<'s, 'a> {
        JBits { s, acc: 0, n: 0, marker: false }
    }

    fn fuellen(&mut self) {
        while self.n <= 24 {
            let mut b = 0u8;
            if !self.marker {
                // schnell: zwei Bytes liegen im Puffer
                let (pos, len) = (self.s.pos, self.s.len);
                let (x, y) = if pos + 1 < len {
                    let d = self.s.daten();
                    (Some(d[pos]), Some(d[pos + 1]))
                } else {
                    (self.s.schauen(0), self.s.schauen(1))
                };
                match x {
                    Some(0xFF) => {
                        if y == Some(0) {
                            self.s.vor(2);
                            b = 0xFF;
                        } else {
                            // Marker (oder Dateiende): nicht weiterlesen
                            self.marker = true;
                        }
                    }
                    Some(x) => {
                        self.s.vor(1);
                        b = x;
                    }
                    None => self.marker = true,
                }
            }
            self.acc |= u32::from(b) << (24 - self.n);
            self.n += 8;
        }
    }

    /// `k` Bits (0–16) lesen.
    #[inline]
    fn bits(&mut self, k: u32) -> i32 {
        if k == 0 {
            return 0;
        }
        if self.n < k {
            self.fuellen();
        }
        let v = self.acc >> (32 - k);
        self.acc <<= k;
        self.n -= k;
        v as i32
    }

    #[inline]
    fn bit(&mut self) -> u32 {
        self.bits(1) as u32
    }

    fn huff(&mut self, h: &JHuff) -> Result<u8, Fehler> {
        if self.n < 16 {
            self.fuellen();
        }
        let e = h.lut[(self.acc >> (32 - LUT_BITS)) as usize];
        if e != 0 {
            let l = u32::from(e >> 8);
            self.acc <<= l;
            self.n -= l;
            return Ok(e as u8);
        }
        for l in 1..=16u32 {
            let code = (self.acc >> (32 - l)) as i32;
            if code <= h.max[l as usize] {
                self.acc <<= l;
                self.n -= l;
                return h.werte.get((h.val_ptr[l as usize] + code - h.min[l as usize]) as usize).copied().ok_or(Fehler::Format);
            }
        }
        Err(Fehler::Format)
    }

    fn erweitern(v: i32, n: u32) -> i32 {
        if n == 0 { 0 } else if v < 1 << (n - 1) { v - (1 << n) + 1 } else { v }
    }

    /// Restart: Bits verwerfen, hinter den RSTn-Marker springen.
    fn neu_ausrichten(&mut self) {
        self.acc = 0;
        self.n = 0;
        self.marker = false;
        while let Some(b) = self.s.schauen(0) {
            if b == 0xFF && self.s.schauen(1).is_some_and(|m| (0xD0..=0xD7).contains(&m)) {
                self.s.vor(2);
                return;
            }
            self.s.vor(1);
        }
    }

    /// Nach einem Scan: bis zum nächsten Marker (nicht RSTn, nicht Füllbyte).
    fn zum_marker(self) {
        while let Some(b) = self.s.schauen(0) {
            if b == 0xFF {
                match self.s.schauen(1) {
                    Some(m) if m != 0 && m != 0xFF && !(0xD0..=0xD7).contains(&m) => return,
                    _ => {}
                }
            }
            self.s.vor(1);
        }
    }
}

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57,
    50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Kosinus-Tabelle der IDCT mit `n` Punkten: cos((2x+1)uπ/(2n)).
fn cos_tabelle(n: usize) -> &'static [[f32; 8]; 8] {
    static TAB: [std::sync::OnceLock<[[f32; 8]; 8]>; 4] =
        [std::sync::OnceLock::new(), std::sync::OnceLock::new(), std::sync::OnceLock::new(), std::sync::OnceLock::new()];
    let i = n.trailing_zeros() as usize;
    TAB[i].get_or_init(|| {
        let mut t = [[0f32; 8]; 8];
        for (x, z) in t.iter_mut().enumerate().take(n) {
            for (u, c) in z.iter_mut().enumerate().take(n) {
                *c = ((2 * x + 1) as f32 * u as f32 * core::f32::consts::PI / (2 * n) as f32).cos();
            }
        }
        t
    })
}

/// Auf 0–255 begrenzen und runden. Ohne `f32::round`: das ist auf
/// RISC-V ein Bibliotheksaufruf (je Bildpunkt mehrmals).
#[inline]
fn runden_u8(v: f32) -> u8 {
    (v.clamp(0.0, 255.0) + 0.5) as u8
}

/// IDCT eines Blocks auf `n`×`n` Bildpunkte (n = 8, 4, 2, 1; verkleinert
/// wie libjpeg: nur die n×n tiefsten Frequenzen, gleicher Faktor ½ je
/// Richtung, sodass der Mittelwert erhalten bleibt). `k` in natürlicher
/// Reihenfolge, dequantisiert; Ergebnis zeilenweise in `out[..n*n]`.
fn idct_n(k: &[i32; 64], n: usize, out: &mut [u8; 64]) {
    if n == 1 {
        out[0] = runden_u8(k[0] as f32 / 8.0 + 128.0);
        return;
    }
    let c = |u: usize| if u == 0 { core::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
    let cos = cos_tabelle(n);
    let mut t = [0f32; 64];
    for v in 0..n {
        for x in 0..n {
            let mut s = 0.0;
            for u in 0..n {
                s += c(u) * k[v * 8 + u] as f32 * cos[x][u];
            }
            t[v * 8 + x] = s / 2.0;
        }
    }
    for x in 0..n {
        for y in 0..n {
            let mut s = 0.0;
            for v in 0..n {
                s += c(v) * t[v * 8 + x] * cos[y][v];
            }
            out[y * n + x] = runden_u8(s / 2.0 + 128.0);
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
    /// Progressiv: je Block die Koeffizienten ≠ 0 (Bit = natürlicher Index)
    /// und die behaltenen n×n Werte (nicht dequantisiert)
    masken: Vec<u64>,
    werte: Vec<i16>,
    /// Blöcke je Zeile (mit MCU-Auffüllung)
    bw_voll: usize,
}

/// Ein Block eines progressiven Scans: vollständig wird nur festgehalten,
/// ob ein Koeffizient ≠ 0 ist (nötig, um den Bitstrom zu lesen); Werte nur
/// für die n×n tiefsten Frequenzen, die das verkleinerte Bild braucht.
struct Block<'b> {
    maske: &'b mut u64,
    werte: &'b mut [i16],
    n: usize,
}

impl Block<'_> {
    #[inline]
    fn platz(&self, z: usize) -> Option<usize> {
        let (r, c) = (z / 8, z % 8);
        (r < self.n && c < self.n).then_some(r * self.n + c)
    }
    #[inline]
    fn nicht_null(&self, z: usize) -> bool {
        *self.maske >> z & 1 != 0
    }
    fn setzen(&mut self, z: usize, v: i32) {
        if v != 0 {
            *self.maske |= 1 << z;
        } else {
            *self.maske &= !(1 << z);
        }
        if let Some(i) = self.platz(z) {
            self.werte[i] = v as i16;
        }
    }
    fn wert(&self, z: usize) -> i32 {
        self.platz(z).map_or(0, |i| i32::from(self.werte[i]))
    }
}

/// Ergebnis: Bild, Originalmasse, Faktor (1, 2, 4, 8).
type JpegErgebnis = (Rgba, (u32, u32), u32);

fn jpeg(d: &[u8]) -> Result<Rgba, Fehler> {
    let mut s = Strom::speicher(d);
    // libimg: Rohbild ohne EXIF-Drehung, volle Grösse
    jpeg_strom(&mut s, &|w, h, _, _| (w <= 16384 && h <= 16384).then_some(1), false).map(|(b, _, _)| b)
}

/// Wählt den Verkleinerungsfaktor: (Breite, Höhe, progressiv, Blöcke aller
/// Komponenten) → 1, 2, 4, 8 oder None (zu gross).
type TeilerWahl<'t> = &'t dyn Fn(u32, u32, bool, u64) -> Option<u32>;

/// JPEG (Baseline und progressiv) aus `s`. Mit dem Faktor von `teiler`
/// rechnet die IDCT gleich auf 8/Faktor Bildpunkte je Block; der Speicher
/// wächst nur mit dem verkleinerten Bild. `drehen`: EXIF-Ausrichtung
/// anwenden (beim Umrechnen in RGB, ohne zweiten Puffer).
fn jpeg_strom(s: &mut Strom, teiler: TeilerWahl, drehen: bool) -> Result<JpegErgebnis, Fehler> {
    if s.byte() != Some(0xFF) || s.byte() != Some(0xD8) {
        return Err(Fehler::Format);
    }
    let mut q = [[0i32; 64]; 4];
    let mut dc: [JHuff; 4] = Default::default();
    let mut ac: [JHuff; 4] = Default::default();
    let mut komp: Vec<Komp> = Vec::new();
    let (mut w, mut h, mut restart) = (0u32, 0u32, 0u32);
    let mut adobe_rgb = false;
    let mut ausrichtung = 1u16;
    let mut progressiv = false;
    let mut f = 1u32;
    let mut n = 8usize;
    let mut koeff_bereit = false;
    loop {
        // Marker: FF (Füllbytes FF) m
        if s.byte().ok_or(Fehler::Format)? != 0xFF {
            return Err(Fehler::Format);
        }
        let mut m = s.byte().ok_or(Fehler::Format)?;
        while m == 0xFF {
            m = s.byte().ok_or(Fehler::Format)?;
        }
        if m == 0xD9 {
            break;
        }
        if m == 0x01 || (0xD0..=0xD7).contains(&m) {
            continue;
        }
        let len = usize::from(u16::from_be_bytes([s.byte().ok_or(Fehler::Format)?, s.byte().ok_or(Fehler::Format)?]));
        if len < 2 {
            return Err(Fehler::Format);
        }
        let len = len - 2;
        // unwichtige Segmente (APPn ausser EXIF/Adobe, Kommentare) überspringen
        if matches!(m, 0xE0 | 0xE2..=0xED | 0xEF | 0xFE) {
            s.ueberspringen(len as u64)?;
            continue;
        }
        let seg = s.nehmen(len)?;
        let byte = |i: usize| seg.get(i).copied().ok_or(Fehler::Format);
        match m {
            0xDB => {
                let mut i = 0;
                while i < seg.len() {
                    let (p, t) = (seg[i] >> 4, usize::from(seg[i] & 3));
                    i += 1;
                    for z in ZIGZAG {
                        q[t][z] = if p == 0 {
                            i32::from(byte(i)?)
                        } else {
                            i32::from(u16::from_be_bytes([byte(i)?, byte(i + 1)?]))
                        };
                        i += if p == 0 { 1 } else { 2 };
                    }
                }
            }
            0xC0..=0xC2 => {
                if byte(0)? != 8 {
                    return Err(Fehler::NichtUnterstuetzt);
                }
                progressiv = m == 0xC2;
                h = u32::from(u16::from_be_bytes([byte(1)?, byte(2)?]));
                w = u32::from(u16::from_be_bytes([byte(3)?, byte(4)?]));
                komp.clear();
                for i in 0..usize::from(byte(5)?) {
                    let (id, hv, qt) = (byte(6 + 3 * i)?, byte(7 + 3 * i)?, byte(8 + 3 * i)?);
                    komp.push(Komp {
                        id,
                        h: u32::from(hv >> 4).clamp(1, 4),
                        v: u32::from(hv & 15).clamp(1, 4),
                        q: usize::from(qt & 3),
                        dc: 0,
                        ac: 0,
                        pred: 0,
                        ebene: Vec::new(),
                        breite: 0,
                        masken: Vec::new(),
                        werte: Vec::new(),
                        bw_voll: 0,
                    });
                }
                if w == 0 || h == 0 || komp.is_empty() || komp.len() > 4 {
                    return Err(Fehler::Format);
                }
                let hmax = komp.iter().map(|k| k.h).max().unwrap_or(1);
                let vmax = komp.iter().map(|k| k.v).max().unwrap_or(1);
                let (mx, my) = (u64::from(w.div_ceil(8 * hmax)), u64::from(h.div_ceil(8 * vmax)));
                let bloecke: u64 = komp.iter().map(|k| mx * u64::from(k.h) * my * u64::from(k.v)).sum();
                f = teiler(w, h, progressiv, bloecke).ok_or(Fehler::ZuGross)?;
                if !matches!(f, 1 | 2 | 4 | 8) {
                    return Err(Fehler::NichtUnterstuetzt);
                }
                n = 8 / f as usize;
            }
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err(Fehler::NichtUnterstuetzt),
            0xC4 => {
                let mut i = 0;
                while i < seg.len() {
                    let (klasse, id) = (seg[i] >> 4, usize::from(seg[i] & 3));
                    let mut anz = [0u8; 16];
                    anz.copy_from_slice(seg.get(i + 1..i + 17).ok_or(Fehler::Format)?);
                    let zahl: usize = anz.iter().map(|&c| usize::from(c)).sum();
                    let werte = seg.get(i + 17..i + 17 + zahl).ok_or(Fehler::Format)?.to_vec();
                    let t = JHuff::neu(&anz, werte);
                    if klasse == 0 {
                        dc[id] = t;
                    } else {
                        ac[id] = t;
                    }
                    i += 17 + zahl;
                }
            }
            0xDD => restart = u32::from(u16::from_be_bytes([byte(0)?, byte(1)?])),
            0xEE => adobe_rgb = seg.len() >= 12 && seg.starts_with(b"Adobe") && seg[11] == 0,
            0xE1 => ausrichtung = exif_ausrichtung(&seg).unwrap_or(ausrichtung),
            0xDA => {
                if w == 0 || h == 0 || komp.is_empty() || w > 65535 || h > 65535 {
                    return Err(Fehler::Format);
                }
                let zahl = usize::from(byte(0)?);
                let mut im_scan = Vec::new();
                for i in 0..zahl {
                    let (id, t) = (byte(1 + 2 * i)?, byte(2 + 2 * i)?);
                    if let Some(ki) = komp.iter().position(|k| k.id == id) {
                        komp[ki].dc = usize::from(t >> 4 & 3);
                        komp[ki].ac = usize::from(t & 3);
                        im_scan.push(ki);
                    }
                }
                let hmax = komp.iter().map(|k| k.h).max().unwrap_or(1);
                let vmax = komp.iter().map(|k| k.v).max().unwrap_or(1);
                let (mx, my) = (w.div_ceil(8 * hmax), h.div_ceil(8 * vmax));
                if progressiv {
                    let p = 1 + 2 * zahl;
                    let (ss, se) = (usize::from(byte(p)?), usize::from(byte(p + 1)?));
                    let a = byte(p + 2)?;
                    let (ah, al) = (u32::from(a >> 4), u32::from(a & 15));
                    if ss > se || se > 63 || (ss == 0 && se != 0) || (ss > 0 && im_scan.len() != 1) || al > 13 {
                        return Err(Fehler::Format);
                    }
                    if !koeff_bereit {
                        for k in komp.iter_mut() {
                            k.bw_voll = (mx * k.h) as usize;
                            let bloecke = k.bw_voll * (my * k.v) as usize;
                            k.masken = vec![0u64; bloecke];
                            k.werte = vec![0i16; bloecke * n * n];
                        }
                        koeff_bereit = true;
                    }
                    for k in komp.iter_mut() {
                        k.pred = 0;
                    }
                    let mut b = JBits::neu(s);
                    let mut eobrun = 0u32;
                    let scan = ScanProg { ss, se, ah, al };
                    if im_scan.len() == 1 {
                        // nicht verschachtelt: Blöcke der Komponente zeilenweise
                        let ki = im_scan[0];
                        let k = &komp[ki];
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
                                let k = &mut komp[ki];
                                let i = by * k.bw_voll + bx;
                                let mut blk = Block { maske: &mut k.masken[i], werte: &mut k.werte[i * n * n..(i + 1) * n * n], n };
                                prog_block(&mut b, &mut blk, &scan, &dc[k.dc], &ac[k.ac], &mut k.pred, &mut eobrun)?;
                            }
                        }
                    } else {
                        // verschachtelt (nur DC-Scans): MCU-Reihenfolge
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
                                    let k = &mut komp[ki];
                                    for by in 0..k.v {
                                        for bx in 0..k.h {
                                            let i = ((my_i * k.v + by) as usize) * k.bw_voll + (mx_i * k.h + bx) as usize;
                                            let mut blk = Block { maske: &mut k.masken[i], werte: &mut k.werte[i * n * n..(i + 1) * n * n], n };
                                            prog_block(&mut b, &mut blk, &scan, &dc[k.dc], &ac[0], &mut k.pred, &mut eobrun)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    b.zum_marker();
                    continue;
                }
                // Baseline: Block für Block dekodieren und gleich verkleinert
                // in die Ebenen schreiben
                for k in komp.iter_mut() {
                    k.breite = (mx * k.h) as usize * n;
                    k.ebene = vec![0; k.breite * (my * k.v) as usize * n];
                }
                let mut b = JBits::neu(s);
                let mut zaehler = 0u32;
                let mut blk = [0u8; 64];
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
                                    let t = u32::from(b.huff(&dc[k.dc])?);
                                    // DC-Klasse höchstens 16 (kaputte Tabelle)
                                    if t > 16 {
                                        return Err(Fehler::Format);
                                    }
                                    let diff = JBits::erweitern(b.bits(t), t);
                                    k.pred = k.pred.wrapping_add(diff);
                                    koeff[0] = k.pred.wrapping_mul(q[k.q][0]);
                                    let mut i = 1;
                                    while i < 64 {
                                        let rs = b.huff(&ac[k.ac])?;
                                        let (r, t) = (usize::from(rs >> 4), u32::from(rs & 15));
                                        if t == 0 {
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
                                        let z = ZIGZAG[i];
                                        let v = JBits::erweitern(b.bits(t), t);
                                        // nur die gebrauchten Frequenzen dequantisieren
                                        if z / 8 < n && z % 8 < n {
                                            koeff[z] = v.wrapping_mul(q[k.q][z]);
                                        }
                                        i += 1;
                                    }
                                    idct_n(&koeff, n, &mut blk);
                                    let x0 = (mx_i * k.h + bx) as usize * n;
                                    let y0 = (my_i * k.v + by) as usize * n;
                                    for yy in 0..n {
                                        let a = (y0 + yy) * k.breite + x0;
                                        k.ebene[a..a + n].copy_from_slice(&blk[yy * n..yy * n + n]);
                                    }
                                }
                            }
                        }
                    }
                }
                let (ws, hs) = (w.div_ceil(f), h.div_ceil(f));
                let bild = farbe(&komp, ws, hs, adobe_rgb, if drehen { ausrichtung } else { 1 });
                return Ok((bild, (w, h), f));
            }
            _ => {}
        }
    }
    if !progressiv || !koeff_bereit {
        return Err(Fehler::Format);
    }
    // Progressiv: dequantisieren, IDCT, in die Ebenen schreiben
    let vmax = komp.iter().map(|k| k.v).max().unwrap_or(1);
    let my = h.div_ceil(8 * vmax);
    for k in komp.iter_mut() {
        k.breite = k.bw_voll * n;
        let zeilen = (my * k.v) as usize;
        k.ebene = vec![0; k.breite * zeilen * n];
        let mut deq = [0i32; 64];
        let mut blk = [0u8; 64];
        for i in 0..k.masken.len() {
            for r in 0..n {
                for c in 0..n {
                    let z = r * 8 + c;
                    deq[z] = i32::from(k.werte[i * n * n + r * n + c]).wrapping_mul(q[k.q][z]);
                }
            }
            idct_n(&deq, n, &mut blk);
            let (x0, y0) = ((i % k.bw_voll) * n, (i / k.bw_voll) * n);
            for yy in 0..n {
                let a = (y0 + yy) * k.breite + x0;
                k.ebene[a..a + n].copy_from_slice(&blk[yy * n..yy * n + n]);
            }
        }
        k.masken = Vec::new();
        k.werte = Vec::new();
    }
    let (ws, hs) = (w.div_ceil(f), h.div_ceil(f));
    Ok((farbe(&komp, ws, hs, adobe_rgb, if drehen { ausrichtung } else { 1 }), (w, h), f))
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
fn prog_block(b: &mut JBits, blk: &mut Block, s: &ScanProg, dc: &JHuff, ac: &JHuff, pred: &mut i32, eobrun: &mut u32) -> Result<(), Fehler> {
    if s.ss == 0 {
        if s.ah == 0 {
            let t = u32::from(b.huff(dc)?);
            if t > 16 {
                return Err(Fehler::Format);
            }
            let diff = JBits::erweitern(b.bits(t), t);
            *pred = pred.wrapping_add(diff);
            blk.setzen(0, pred.wrapping_mul(1 << s.al));
        } else if b.bit() != 0 {
            blk.setzen(0, blk.wert(0) | 1 << s.al);
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
                blk.setzen(ZIGZAG[k], JBits::erweitern(b.bits(t), t).wrapping_mul(1 << s.al));
                k += 1;
            }
        }
        return Ok(());
    }
    // AC, Verfeinerung
    let p1 = 1i32 << s.al;
    let verfeinern = |b: &mut JBits, blk: &mut Block, z: usize| {
        if b.bit() != 0 {
            let c = blk.wert(z);
            if blk.platz(z).is_some() && c & p1 == 0 {
                blk.setzen(z, if c >= 0 { c + p1 } else { c - p1 });
            }
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
                if blk.nicht_null(z) {
                    verfeinern(b, blk, z);
                } else {
                    if r == 0 {
                        if wert != 0 {
                            blk.setzen(z, wert);
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
            if blk.nicht_null(z) {
                verfeinern(b, blk, z);
            }
            k += 1;
        }
        *eobrun -= 1;
    }
    Ok(())
}

/// EXIF-Ausrichtung (Tag 0x0112) aus einem APP1-Segment: 1 = normal,
/// 3 = 180°, 6 = 90° im Uhrzeigersinn, 8 = 90° dagegen, 2/4/5/7 gespiegelt.
fn exif_ausrichtung(seg: &[u8]) -> Option<u16> {
    let t = seg.strip_prefix(b"Exif\0\0")?;
    let le = match t.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_ = |o: usize| t.get(o..o + 2).map(|b| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
    let u32_ = |o: usize| t.get(o..o + 4).map(|b| if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) });
    let ifd = u32_(4)? as usize;
    let anzahl = usize::from(u16_(ifd)?);
    for i in 0..anzahl.min(200) {
        let e = ifd + 2 + 12 * i;
        if u16_(e)? == 0x0112 {
            let v = u16_(e + 8)?;
            return (1..=8).contains(&v).then_some(v);
        }
    }
    None
}

/// Zielpunkt eines Bildpunkts (x, y) eines w×h-Bildes nach der
/// EXIF-Ausrichtung `o` (2/4 spiegeln, 3 = 180°, 5/7 quer gespiegelt,
/// 6 = 90° im Uhrzeigersinn, 8 = 90° dagegen).
fn gedreht(x: usize, y: usize, w: usize, h: usize, o: u16) -> (usize, usize) {
    match o {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (h - 1 - y, x),
        7 => (h - 1 - y, w - 1 - x),
        8 => (y, w - 1 - x),
        _ => (x, y),
    }
}

/// Ebenen hochrechnen (unterabgetastete Komponenten zentriert-bilinear wie
/// libjpegs „fancy upsampling“), in RGB umwandeln und gleich nach der
/// EXIF-Ausrichtung `o` ablegen (kein zweiter Bildpuffer).
fn farbe(komp: &[Komp], w: u32, h: u32, adobe_rgb: bool, o: u16) -> Rgba {
    let hmax = komp.iter().map(|k| k.h).max().unwrap_or(1);
    let vmax = komp.iter().map(|k| k.v).max().unwrap_or(1);
    let mut px = vec![0u32; (w * h) as usize];
    // Unterabgetastete Komponenten: bilinear. Stützstellen und Gewichte je
    // Spalte und Zeile einmal vorab (nicht je Bildpunkt teilen).
    let achse = |aus: u32, f: u32, fmax: u32| -> Vec<(usize, usize, f32)> {
        let kn = (aus * f).div_ceil(fmax);
        (0..aus)
            .map(|i| {
                let v = ((i as f32 + 0.5) * f as f32 / fmax as f32 - 0.5).clamp(0.0, kn as f32 - 1.0);
                let i0 = v as usize;
                (i0, (i0 + 1).min(kn as usize - 1), v - i0 as f32)
            })
            .collect()
    };
    let spalten: Vec<Vec<(usize, usize, f32)>> = komp.iter().map(|k| if k.h == hmax { Vec::new() } else { achse(w, k.h, hmax) }).collect();
    let zeilen: Vec<Vec<(usize, usize, f32)>> = komp.iter().map(|k| if k.v == vmax { Vec::new() } else { achse(h, k.v, vmax) }).collect();
    let probe = |ki: usize, x: u32, y: u32| -> i32 {
        let k = &komp[ki];
        let p = |sx: usize, sy: usize| f32::from(k.ebene[sy * k.breite + sx]);
        if k.h == hmax && k.v == vmax {
            return i32::from(k.ebene[y as usize * k.breite + x as usize]);
        }
        let (x0, x1, ax) = if k.h == hmax { (x as usize, x as usize, 0.0) } else { spalten[ki][x as usize] };
        let (y0, y1, ay) = if k.v == vmax { (y as usize, y as usize, 0.0) } else { zeilen[ki][y as usize] };
        let o = p(x0, y0) * (1.0 - ax) + p(x1, y0) * ax;
        let u = p(x0, y1) * (1.0 - ax) + p(x1, y1) * ax;
        (o * (1.0 - ay) + u * ay + 0.5) as i32
    };
    let (wu, hu) = (w as usize, h as usize);
    let quer = (5..=8).contains(&o);
    let zw = if quer { hu } else { wu };
    for y in 0..h {
        for x in 0..w {
            let c = if komp.len() >= 3 {
                let (yy, cb, cr) = (probe(0, x, y), probe(1, x, y) - 128, probe(2, x, y) - 128);
                if adobe_rgb {
                    argb(255, yy as u32, (cb + 128) as u32, (cr + 128) as u32)
                } else {
                    let r = u32::from(runden_u8(yy as f32 + 1.402 * cr as f32));
                    let g = u32::from(runden_u8(yy as f32 - 0.344_136 * cb as f32 - 0.714_136 * cr as f32));
                    let bl = u32::from(runden_u8(yy as f32 + 1.772 * cb as f32));
                    argb(255, r, g, bl)
                }
            } else {
                let g = probe(0, x, y) as u32;
                argb(255, g, g, g)
            };
            let (zx, zy) = gedreht(x as usize, y as usize, wu, hu, o);
            px[zy * zw + zx] = c;
        }
    }
    let (rw, rh) = if quer { (h, w) } else { (w, h) };
    Rgba { w: rw, h: rh, px, delay: 0 }
}
