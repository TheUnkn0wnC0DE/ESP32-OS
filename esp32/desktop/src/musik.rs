//! A5: Musikplayer (Auftrag Abschnitt 5) – MP3 von der SD-Karte über den
//! Lautsprecher (Ringpuffer [`kern::ton`] → Audio-Faden der Firmware →
//! I²S → ES8311).
//!
//! Links die MP3-Dateien des Ordners, rechts Titel, Zeit, Fortschritt und
//! grosse Knöpfe: zurück, Pause/Weiter, nächster. Nach dem Ende eines Titels
//! kommt der nächste. Dekodiert wird mit `nanomp3` (Port von minimp3) aus
//! der Datei in Stücken (nie ganz im Speicher), umgerechnet auf die
//! Abtastrate des Tons ([`kern::ton::RATE`]), Lautstärke aus den
//! Einstellungen.
//!
//! Ordner: Argument (Ordner oder MP3-Datei – der Dateimanager öffnet .mp3
//! damit), sonst `/sd0/1/ESP32-OS/Musik`, ohne SD-Karte `/rd/1/Musik`
//! (interne Partition).

use crate::dateimanager;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use kern::{datei, sys};
use std::io::{Read, Seek, SeekFrom};

// Fenster (Arbeitsbereich-Koordinaten)
pub const FX: i32 = 60;
pub const FY: i32 = 4;
pub const FW: i32 = 520;
pub const FH: i32 = 306;
/// Liste links: x, Breite, erste Zeile, Zeilenhöhe, Zeilen.
pub const LISTE_X: i32 = 6;
pub const LISTE_W: i32 = 200;
pub const LISTE_Y: i32 = 6;
/// Zeilen ≥ 7 mm (1 px = 0,173 mm)
pub const ZEILE_H: i32 = 42;
const ZEILEN: usize = 5;
/// Rechte Seite
const RX: i32 = 216;
const RW: i32 = 288;

pub const K_ZURUECK: u32 = 2;
pub const K_SPIELEN: u32 = 3;
pub const K_NAECHSTER: u32 = 4;
pub const K_TITEL: u32 = 100;

/// Zustand für Tests und das Board-Log: Titel, spielt, Sekunde.
static TITEL: crate::Textmerker = crate::Textmerker::neu();
static SPIELT: AtomicBool = AtomicBool::new(false);
static SEKUNDE: AtomicU32 = AtomicU32::new(0);

/// (Titel, spielt, Position in Sekunden) zuletzt angezeigt.
pub fn zustand() -> (String, bool, u32) {
    (TITEL.holen(), SPIELT.load(Relaxed), SEKUNDE.load(Relaxed))
}

/// Lage eines Knopfs (x, y, Breite, Höhe; Arbeitsbereich).
pub fn knopf_lage(k: u32) -> (i32, i32, i32, i32) {
    match k {
        K_ZURUECK => (RX, 120, 88, 64),
        K_SPIELEN => (RX + 96, 120, 96, 64),
        K_NAECHSTER => (RX + 200, 120, 88, 64),
        _ => (0, 0, 0, 0),
    }
}

/// Mitte eines Knopfs (Arbeitsbereich) – für Tests und den Autopiloten.
pub fn knopf_mitte(k: u32) -> (i32, i32) {
    let (x, y, w, h) = knopf_lage(k);
    (x + w / 2, y + h / 2)
}

/// Mitte des Titels `i` in der Liste (Arbeitsbereich).
pub fn titel_mitte(i: usize) -> (i32, i32) {
    (LISTE_X + LISTE_W / 2, LISTE_Y + i as i32 * ZEILE_H + ZEILE_H / 2)
}

// ------------------------------------------------------------- Datei-Leser

/// Datei des Kerns als `Read + Seek` (liest in Stücken über Funktion 70).
struct KDatei {
    pfad: String,
    pos: u64,
    groesse: u64,
}

impl KDatei {
    fn oeffnen(pfad: &str) -> Option<KDatei> {
        let mut b = [0u8; 40];
        if datei::info(pfad, &mut b) != datei::OK || b[0] & 0x10 != 0 {
            return None;
        }
        let mut g = [0u8; 8];
        g.copy_from_slice(&b[32..40]);
        Some(KDatei { pfad: pfad.to_string(), pos: 0, groesse: u64::from_le_bytes(g) })
    }
}

impl Read for KDatei {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.groesse || buf.is_empty() {
            return Ok(0);
        }
        let (c, n) = datei::lesen(&self.pfad, self.pos, buf);
        if c != datei::OK && c != datei::EOF {
            return Err(std::io::Error::other(format!("Lesefehler {c}")));
        }
        self.pos += u64::from(n);
        Ok(n as usize)
    }
}

impl Seek for KDatei {
    fn seek(&mut self, wohin: SeekFrom) -> std::io::Result<u64> {
        let neu = match wohin {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::End(d) => self.groesse as i64 + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if neu < 0 {
            return Err(std::io::Error::other("vor dem Anfang"));
        }
        self.pos = neu as u64;
        Ok(self.pos)
    }
}

/// Länge in Sekunden aus Dateigrösse und Bitrate des ersten Frames
/// (konstante Bitrate; Dateien mit VBR-Kopf liefern die Länge selbst).
pub fn laenge_geschaetzt(kopf: &[u8], groesse: u64) -> Option<u32> {
    let id3 = nanomp3::id3v2_len(kopf);
    let b = kopf.get(id3..)?;
    let i = b.windows(4).position(|h| h[0] == 0xFF && h[1] & 0xE0 == 0xE0 && h[1] & 0x06 == 0x02 && h[2] >> 4 != 0 && h[2] >> 4 != 15 && (h[2] >> 2) & 3 != 3)?;
    let h = &b[i..i + 4];
    let mpeg1 = h[1] & 0x18 == 0x18;
    const MPEG1: [u32; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
    const MPEG2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let kbit = if mpeg1 { MPEG1 } else { MPEG2 }[usize::from(h[2] >> 4)];
    let daten = groesse.checked_sub((id3 + i) as u64)?;
    Some((daten * 8 / (u64::from(kbit) * 1000)) as u32)
}

// -------------------------------------------------------------- Wiedergabe

type Leser = nanomp3::Reader<KDatei, i16>;

/// D2: WAV (RIFF, PCM 8 oder 16 Bit, mono/stereo, beliebige Rate).
pub struct WavLeser<R: Read> {
    d: R,
    pub rate: u32,
    pub kanaele: usize,
    bits: u16,
    /// noch zu lesende Bytes des data-Blocks
    rest: u64,
    roh: Vec<u8>,
}

impl<R: Read> WavLeser<R> {
    /// Kopf lesen bis zum Anfang der Daten (Blöcke „fmt “ und „data“).
    pub fn neu(mut d: R) -> Result<WavLeser<R>, String> {
        let mut k = [0u8; 12];
        d.read_exact(&mut k).map_err(|_| "zu kurz".to_string())?;
        if &k[0..4] != b"RIFF" || &k[8..12] != b"WAVE" {
            return Err("keine WAV-Datei".into());
        }
        let (mut rate, mut kanaele, mut bits, mut format) = (0u32, 0usize, 0u16, 0u16);
        loop {
            let mut b = [0u8; 8];
            d.read_exact(&mut b).map_err(|_| "kein data-Block".to_string())?;
            let laenge = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as u64;
            match &b[0..4] {
                b"fmt " => {
                    let mut f = vec![0u8; laenge as usize];
                    d.read_exact(&mut f).map_err(|_| "fmt zu kurz".to_string())?;
                    if f.len() < 16 {
                        return Err("fmt zu kurz".into());
                    }
                    format = u16::from_le_bytes([f[0], f[1]]);
                    kanaele = usize::from(u16::from_le_bytes([f[2], f[3]]));
                    rate = u32::from_le_bytes([f[4], f[5], f[6], f[7]]);
                    bits = u16::from_le_bytes([f[14], f[15]]);
                    if laenge % 2 == 1 {
                        let _ = d.read_exact(&mut [0u8; 1]);
                    }
                }
                b"data" => {
                    // 1 = PCM, 0xFFFE = WAVE_FORMAT_EXTENSIBLE (PCM-Untertyp)
                    if !matches!(format, 1 | 0xFFFE) || !matches!(bits, 8 | 16) || !(1..=2).contains(&kanaele) || rate == 0 {
                        return Err(format!("nicht unterstützt (Format {format}, {bits} Bit, {kanaele} Kanäle)"));
                    }
                    return Ok(WavLeser { d, rate, kanaele, bits, rest: laenge, roh: Vec::new() });
                }
                _ => {
                    // anderer Block (LIST, fact …) überspringen
                    let n = laenge + laenge % 2;
                    std::io::copy(&mut (&mut d).take(n), &mut std::io::sink()).map_err(|_| "Block zu kurz".to_string())?;
                }
            }
        }
    }

    /// Abtastwerte (verschachtelt, i16) nach `pcm`; 0 = Ende.
    pub fn lesen(&mut self, pcm: &mut [i16]) -> usize {
        let breite = usize::from(self.bits / 8);
        let n = (pcm.len() / self.kanaele * self.kanaele * breite).min(self.rest as usize);
        let n = n - n % (breite * self.kanaele);
        if n == 0 {
            return 0;
        }
        self.roh.resize(n, 0);
        let mut gelesen = 0;
        while gelesen < n {
            match self.d.read(&mut self.roh[gelesen..n]) {
                Ok(0) | Err(_) => break,
                Ok(k) => gelesen += k,
            }
        }
        let gelesen = gelesen - gelesen % (breite * self.kanaele);
        self.rest -= gelesen as u64;
        if gelesen == 0 {
            self.rest = 0;
            return 0;
        }
        if breite == 2 {
            for (z, b) in pcm.iter_mut().zip(self.roh[..gelesen].chunks_exact(2)) {
                *z = i16::from_le_bytes([b[0], b[1]]);
            }
        } else {
            for (z, &b) in pcm.iter_mut().zip(&self.roh[..gelesen]) {
                *z = (i16::from(b) - 128) << 8;
            }
        }
        gelesen / breite
    }

    /// Länge in Sekunden (aus der Grösse des data-Blocks beim Öffnen).
    fn sekunden(&self) -> u32 {
        (self.rest / (u64::from(self.bits / 8) * self.kanaele as u64) / u64::from(self.rate)) as u32
    }
}

/// Quelle einer Wiedergabe: MP3 oder WAV.
enum Quelle {
    Mp3(Leser),
    Wav(WavLeser<KDatei>),
}

impl Quelle {
    fn lesen(&mut self, pcm: &mut [i16]) -> usize {
        match self {
            Quelle::Mp3(l) => l.read(pcm).unwrap_or(0),
            Quelle::Wav(w) => w.lesen(pcm),
        }
    }
}

/// Spielbare Endungen (D2).
pub fn spielbar(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".mp3") || n.ends_with(".wav")
}

/// Titel ohne Endung.
fn ohne_endung(name: &str) -> &str {
    if spielbar(name) { &name[..name.len() - 4] } else { name }
}

/// Eine laufende Datei: Decoder und Umrechnung auf [`kern::ton::RATE`].
struct Wiedergabe {
    quelle: Quelle,
    rate: u32,
    kanaele: usize,
    /// Quell-Frames je Ausgabe-Frame
    schritt: f64,
    /// dekodierte, noch nicht verbrauchte Quell-Frames (L, R)
    vorrat: Vec<[i32; 2]>,
    /// Lage des nächsten Ausgabe-Frames in `vorrat` (Bruchteil)
    t: f64,
    /// in den Ring geschriebene Ausgabe-Frames seit Anfang (Position)
    geschrieben: u64,
    /// fertige Ausgabe-Frames, die zuerst in den Ring kommen (nach Pause)
    rest: Vec<u32>,
    /// Ende der Datei erreicht
    ende: bool,
    /// Länge in Sekunden (VBR-Kopf oder geschätzt), falls bekannt
    gesamt_s: Option<u32>,
    pcm: Vec<i16>,
}

impl Wiedergabe {
    /// Öffnen, gleich im Heap: der Leser ist ~23 KB gross; mehrfach auf dem
    /// Stapel verschoben sprengte das den Stapel am Board.
    #[inline(never)]
    fn oeffnen(pfad: &str) -> Result<Box<Wiedergabe>, String> {
        let mut d = KDatei::oeffnen(pfad).ok_or_else(|| "nicht gefunden".to_string())?;
        if pfad.to_lowercase().ends_with(".wav") {
            let w = WavLeser::neu(d)?;
            let (rate, kanaele, gesamt_s) = (w.rate, w.kanaele, Some(w.sekunden()));
            return Ok(Box::new(Wiedergabe {
                quelle: Quelle::Wav(w),
                rate,
                kanaele,
                schritt: f64::from(rate) / f64::from(kern::ton::RATE),
                vorrat: Vec::new(),
                t: 0.0,
                geschrieben: 0,
                rest: Vec::new(),
                ende: false,
                gesamt_s,
                pcm: vec![0; nanomp3::MAX_SAMPLES_PER_FRAME],
            }));
        }
        // Kopf für die Längenschätzung (ID3-Kopf bis 64 KB überspringen)
        let mut kopf = vec![0u8; 64 * 1024 + 16];
        let n = d.read(&mut kopf).unwrap_or(0);
        kopf.truncate(n);
        let groesse = d.groesse;
        d.pos = 0;
        // ohne Vorab-Durchlauf der ganzen Datei (SD-Karte: dauert sonst);
        // der Leser puffert selbst (256 KB)
        let leser = nanomp3::Reader::with_options(d, nanomp3::Options::default().skip_scan(true)).map_err(|e| format!("{e:?}"))?;
        let rate = leser.sample_rate();
        let kanaele = leser.channels().map_or(2, |c| c.num() as usize);
        if rate == 0 {
            return Err("keine MP3-Daten".into());
        }
        let gesamt_s = leser.total_samples().map(|n| (n / u64::from(rate)) as u32).or_else(|| laenge_geschaetzt(&kopf, groesse));
        Ok(Box::new(Wiedergabe {
            quelle: Quelle::Mp3(leser),
            rate,
            kanaele,
            schritt: f64::from(rate) / f64::from(kern::ton::RATE),
            vorrat: Vec::new(),
            t: 0.0,
            geschrieben: 0,
            rest: Vec::new(),
            ende: false,
            gesamt_s,
            pcm: vec![0; nanomp3::MAX_SAMPLES_PER_FRAME],
        }))
    }

    /// Ausgabe-Frames erzeugen, bis `max` erreicht oder die Datei zu Ende
    /// ist; `laut` 0..=1.
    fn erzeugen(&mut self, max: usize, laut: f32, aus: &mut Vec<u32>) {
        while aus.len() < max {
            // genug Vorrat für einen Ausgabe-Frame (Mittelung über 2 Frames)?
            if self.t as usize + 2 >= self.vorrat.len() {
                if self.ende {
                    break;
                }
                match self.quelle.lesen(&mut self.pcm) {
                    0 => self.ende = true,
                    n => {
                        let k = self.kanaele.max(1);
                        for f in self.pcm[..n].chunks_exact(k) {
                            let l = i32::from(f[0]);
                            let r = if k > 1 { i32::from(f[1]) } else { l };
                            self.vorrat.push([l, r]);
                        }
                    }
                }
                continue;
            }
            // lineare Interpolation; beim Verkleinern vorher über zwei
            // Quell-Frames mitteln (einfacher Tiefpass gegen Aliasing)
            let i = self.t as usize;
            let a = self.t - i as f64;
            let probe = |j: usize, c: usize| {
                let v = &self.vorrat;
                if self.schritt >= 1.5 { (v[j][c] + v[(j + 1).min(v.len() - 1)][c]) as f64 / 2.0 } else { v[j][c] as f64 }
            };
            let mut s = [0i16; 2];
            for (c, s) in s.iter_mut().enumerate() {
                let x = probe(i, c) * (1.0 - a) + probe(i + 1, c) * a;
                *s = (x * f64::from(laut)).clamp(-32768.0, 32767.0) as i16;
            }
            aus.push(kern::ton::frame(s[0], s[1]));
            self.t += self.schritt;
            // verbrauchte Quell-Frames weg
            let weg = self.t as usize;
            if weg > 1024 {
                self.vorrat.drain(..weg);
                self.t -= weg as f64;
            }
        }
    }

    /// Ring nachfüllen: erst der Rest (nach einer Pause), dann neu Dekodiertes.
    fn fuellen(&mut self, laut: f32, frames: &mut Vec<u32>) {
        let frei = kern::ton::frei();
        if frei < 1024 {
            return;
        }
        frames.clear();
        let n = self.rest.len().min(frei);
        frames.extend(self.rest.drain(..n));
        self.erzeugen(frei, laut, frames);
        let k = kern::ton::schreiben(frames);
        self.geschrieben += k as u64;
        if k < frames.len() {
            self.rest.splice(0..0, frames[k..].iter().copied());
        }
    }

    /// Wiedergabe-Position (Sekunden): geschrieben minus dem, was noch im
    /// Ring auf die Ausgabe wartet.
    fn sekunde(&self) -> u32 {
        (self.geschrieben.saturating_sub(kern::ton::belegt() as u64) / u64::from(kern::ton::RATE)) as u32
    }

    /// Fertig: Datei zu Ende und alles ausgegeben.
    fn fertig(&self) -> bool {
        self.ende && self.t as usize + 2 >= self.vorrat.len() && self.rest.is_empty() && kern::ton::belegt() == 0
    }

    /// Pause: was im Ring noch nicht gespielt ist, zurücknehmen und beim
    /// Weiterspielen zuerst ausgeben (genau an der Stelle, ohne Sprung in
    /// der Datei).
    fn anhalten(&mut self) {
        let mut v = Vec::new();
        kern::ton::zuruecknehmen(&mut v);
        self.geschrieben = self.geschrieben.saturating_sub(v.len() as u64);
        v.append(&mut self.rest);
        self.rest = v;
    }
}

// --------------------------------------------------------------- Programm

struct Zustand {
    ordner: String,
    titel: Vec<String>,
    aktuell: Option<usize>,
    wiedergabe: Option<Box<Wiedergabe>>,
    spielt: bool,
    meldung: Option<String>,
    /// Ton-Strom gehört uns (sonst spielt ein anderes Programm)
    ton_ok: bool,
    /// zuletzt gezeigte Sekunde (nur bei Änderung neu zeichnen)
    gezeigt: Option<u32>,
    oben: usize,
}

fn text(x: i32, y: i32, s: &str, farbe: u32, mal: u32) {
    let f = 0x3000_0000 | (mal.clamp(1, 8) - 1) << 24;
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, f | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

/// Text auf `max` Zeichen kürzen („…“).
fn kurz(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max - 1).collect::<String>()) }
}

fn zeit(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

fn standard_ordner() -> String {
    if dateimanager::art("/sd0/1/ESP32-OS") == Some(true) { "/sd0/1/ESP32-OS/Musik".into() } else { "/rd/1/Musik".into() }
}

fn pfad(ordner: &str, name: &str) -> String {
    if ordner.ends_with('/') { format!("{ordner}{name}") } else { format!("{ordner}/{name}") }
}

/// MP3-Dateien des Ordners (nach Namen).
fn titel_lesen(ordner: &str) -> Vec<String> {
    dateimanager::lesen(ordner)
        .map(|v| v.into_iter().filter(|e| !e.ordner && spielbar(&e.name)).map(|e| e.name).collect())
        .unwrap_or_default()
}

fn lautstaerke() -> f32 {
    let v = f32::from(crate::konfig::LAUTSTAERKE.load(Relaxed).min(10)) / 10.0;
    v * v
}

fn zeichnen(z: &mut Zustand) {
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xF4F4F4, 0, Some(kern::sprache::t("Music", "Musik")));
    for (zeile, i) in (z.oben..z.titel.len()).take(ZEILEN).enumerate() {
        let y = LISTE_Y + zeile as i32 * ZEILE_H;
        let gewaehlt = z.aktuell == Some(i);
        sys::knopf((LISTE_X as u32) << 16 | (LISTE_W - 1) as u32, (y as u32) << 16 | (ZEILE_H - 3) as u32, K_TITEL + i as u32, if gewaehlt { 0xCFE0F5 } else { 0xFFFFFF });
        let name = ohne_endung(&z.titel[i]);
        text(LISTE_X + 6, y + (ZEILE_H - 3 - 16) / 2, &kurz(name, 23), 0x101010, 1);
    }
    let knopf = |k: u32, s: &str, farbe: u32, schrift: u32| {
        let (x, y, w, h) = knopf_lage(k);
        sys::knopf((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, k, farbe);
        let b = s.chars().count() as i32 * 16;
        text(x + (w - b) / 2, y + (h - 32) / 2, s, schrift, 2);
    };
    knopf(K_ZURUECK, "|<", 0xC9D6EA, 0x202020);
    knopf(K_SPIELEN, if z.spielt { "||" } else { ">" }, 0x3A7BD5, 0xFFFFFF);
    knopf(K_NAECHSTER, ">|", 0xC9D6EA, 0x202020);
    z.gezeigt = None;
    anzeige_zeichnen(z);
    sys::neuzeichnen(2);
}

/// Titel, Zeit, Fortschritt und Meldung (rechts oben bzw. unten).
fn anzeige_zeichnen(z: &mut Zustand) {
    let sek = z.wiedergabe.as_ref().map_or(0, |w| w.sekunde());
    if z.gezeigt == Some(sek) {
        return;
    }
    z.gezeigt = Some(sek);
    SEKUNDE.store(sek, Relaxed);
    sys::rechteck((RX as u32) << 16 | RW as u32, 6 << 16 | 108, 0xF4F4F4);
    let titel = z.aktuell.map(|i| z.titel[i].clone()).unwrap_or_default();
    TITEL.setzen(&titel);
    SPIELT.store(z.spielt, Relaxed);
    let name = ohne_endung(&titel);
    if name.is_empty() {
        text(RX, 14, &kurz(kern::sprache::t("Choose a title", "Titel wählen"), 18), 0x606060, 2);
    } else {
        text(RX, 14, &kurz(name, 18), 0x101010, 2);
    }
    text(RX, 56, &zeit(sek), 0x303030, 2);
    // Fortschritt: ohne bekannte Länge ein laufender Balken über 60 s
    let gesamt = z.wiedergabe.as_ref().and_then(|w| w.gesamt_s);
    if let Some(g) = gesamt {
        text(RX + 96, 56, &format!("/ {}", zeit(g)), 0x808080, 2);
    }
    let anteil = match gesamt {
        Some(g) if g > 0 => (sek.min(g) * 1000 / g) as i32,
        _ => ((sek % 60) * 1000 / 60) as i32,
    };
    sys::rechteck((RX as u32) << 16 | RW as u32, 96 << 16 | 10, 0xD8D8D8);
    sys::rechteck((RX as u32) << 16 | (RW * anteil / 1000) as u32, 96 << 16 | 10, 0x3A7BD5);
    sys::rechteck((RX as u32) << 16 | RW as u32, 196 << 16 | 40, 0xF4F4F4);
    if let Some(m) = &z.meldung {
        text(RX, 200, &kurz(m, 36), 0xB03020, 1);
    } else if z.titel.is_empty() {
        text(RX, 200, &kurz(&format!("{} {}", kern::sprache::t("No music files (MP3, WAV) in", "Keine Musikdateien (MP3, WAV) in"), z.ordner), 36), 0x606060, 1);
    }
}

/// Titel `i` von vorn spielen.
#[inline(never)]
fn starten(z: &mut Zustand, i: usize) {
    kern::ton::leeren();
    // alten Leser zuerst freigeben (Puffer 256 KB)
    z.wiedergabe = None;
    z.aktuell = Some(i);
    z.meldung = None;
    let p = pfad(&z.ordner, &z.titel[i]);
    match Wiedergabe::oeffnen(&p) {
        Ok(w) => {
            sys::debug_text(&format!("Musik: spielt {} ({} Hz, {} Kanal/Kanäle)", z.titel[i], w.rate, w.kanaele));
            z.wiedergabe = Some(w);
            z.spielt = true;
        }
        Err(e) => {
            sys::debug_text(&format!("Musik: {} nicht spielbar: {e}", z.titel[i]));
            z.meldung = Some(format!("{}: {e}", kern::sprache::t("Cannot play", "Nicht spielbar")));
            z.wiedergabe = None;
            z.spielt = false;
        }
    }
    // gewählten Titel sichtbar halten
    if i < z.oben || i >= z.oben + ZEILEN {
        z.oben = i.saturating_sub(ZEILEN - 1);
    }
}

/// Programm „musik“.
pub fn programm() {
    let arg = kern::argumente();
    let (ordner, datei_name) = if arg.is_empty() {
        (standard_ordner(), None)
    } else if dateimanager::art(&arg) == Some(false) {
        match arg.rsplit_once('/') {
            Some((o, n)) => (o.to_string(), Some(n.to_string())),
            None => (standard_ordner(), None),
        }
    } else {
        (arg.clone(), None)
    };
    let titel = titel_lesen(&ordner);
    let mut z = Zustand { ordner, titel, aktuell: None, wiedergabe: None, spielt: false, meldung: None, ton_ok: kern::ton::belegen(), gezeigt: None, oben: 0 };
    if !z.ton_ok {
        z.meldung = Some(kern::sprache::t("Sound is in use by another program", "Ton wird von einem anderen Programm benutzt").into());
    }
    sys::debug_text(&format!("Musik: {} Titel in {}", z.titel.len(), z.ordner));
    if let Some(i) = datei_name.and_then(|n| z.titel.iter().position(|t| *t == n))
        && z.ton_ok
    {
        starten(&mut z, i);
    }
    let mut frames = Vec::with_capacity(4096);
    let mut log_s = 0u32;
    loop {
        let e = if z.spielt { sys::warte_ereignis_max(2) } else { sys::warte_ereignis() };
        match e {
            1 => zeichnen(&mut z),
            3 => {
                let id = sys::knopf_id() >> 8;
                let n = z.titel.len();
                match id {
                    1 => {
                        kern::ton::freigeben();
                        return;
                    }
                    K_SPIELEN if n > 0 && z.ton_ok => {
                        if z.wiedergabe.is_none() {
                            let i = z.aktuell.unwrap_or(0);
                            starten(&mut z, i);
                        } else if z.spielt {
                            if let Some(w) = z.wiedergabe.as_mut() {
                                w.anhalten();
                            }
                            z.spielt = false;
                            sys::debug_text(&format!("Musik: Pause bei {}", zeit(z.wiedergabe.as_ref().map_or(0, |w| w.sekunde()))));
                        } else {
                            z.spielt = true;
                            sys::debug_text(&format!("Musik: weiter bei {}", zeit(z.wiedergabe.as_ref().map_or(0, |w| w.sekunde()))));
                        }
                        zeichnen(&mut z);
                    }
                    K_NAECHSTER | K_ZURUECK if n > 0 && z.ton_ok => {
                        let i = z.aktuell.unwrap_or(0);
                        // zurück: nach 3 s an den Anfang des Titels, sonst voriger
                        let neu = if id == K_NAECHSTER {
                            (i + 1) % n
                        } else if z.wiedergabe.as_ref().is_some_and(|w| w.sekunde() >= 3) {
                            i
                        } else {
                            (i + n - 1) % n
                        };
                        sys::debug_text(&format!("Musik: {} → {}", if id == K_NAECHSTER { "nächster" } else { "zurück" }, z.titel[neu]));
                        starten(&mut z, neu);
                        zeichnen(&mut z);
                    }
                    k if k >= K_TITEL && ((k - K_TITEL) as usize) < n && z.ton_ok => {
                        starten(&mut z, (k - K_TITEL) as usize);
                        zeichnen(&mut z);
                    }
                    _ => {}
                }
            }
            6 => {
                // Wischen über der Liste blättert
                let s = sys::maus(7) as i16 as i32;
                if s != 0 {
                    let max = z.titel.len().saturating_sub(ZEILEN);
                    z.oben = (z.oben as i32 + s.signum()).clamp(0, max as i32) as usize;
                    zeichnen(&mut z);
                }
            }
            _ => {}
        }
        // Ton nachfüllen
        if z.spielt
            && let Some(w) = z.wiedergabe.as_mut()
        {
            w.fuellen(lautstaerke(), &mut frames);
            if w.fertig() {
                let n = z.titel.len();
                let i = z.aktuell.unwrap_or(0);
                sys::debug_text(&format!("Musik: Ende von {}", z.titel[i]));
                if i + 1 < n {
                    starten(&mut z, i + 1);
                } else {
                    z.spielt = false;
                    z.wiedergabe = None;
                }
                zeichnen(&mut z);
            }
        }
        if z.spielt {
            anzeige_zeichnen(&mut z);
            // alle 2 s: Position und tatsächlich ausgegebene Frames (Nachweis)
            let s = SEKUNDE.load(Relaxed);
            if s >= log_s + 2 || s < log_s {
                log_s = s;
                sys::debug_text(&format!("Musik: {} bei {}, ausgegeben {} Frames", TITEL.holen(), zeit(s), kern::ton::gespielt()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Länge aus Bitrate und Grösse: 32 kbit/s, 31 824 Byte → 7 s (7,96);
    /// 48 kbit/s, 36 000 Byte → 6 s.
    #[test]
    fn laenge_schaetzen() {
        for (n, s) in [("Testton 1.mp3", 7), ("Testton 2.mp3", 6)] {
            let d = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/daten").join(n)).unwrap();
            assert_eq!(super::laenge_geschaetzt(&d, d.len() as u64), Some(s), "{n}");
        }
        // mit ID3v2-Kopf (100 Byte Nutzdaten) davor; darin ein falscher
        // Frame-Kopf (128 kbit/s), der übersprungen werden muss
        let mut d = b"ID3\x03\x00\x00\x00\x00\x00\x64".to_vec();
        d.extend([0xFF, 0xFB, 0x90, 0x00].iter().cycle().take(100));
        d.extend(std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/daten/Testton 2.mp3")).unwrap());
        assert_eq!(super::laenge_geschaetzt(&d, d.len() as u64), Some(6));
        assert_eq!(super::laenge_geschaetzt(b"keine mp3", 9), None);
    }

    /// Die eigenen Testtöne (`werkzeuge/test_mp3.py`) lassen sich ganz
    /// dekodieren: Abtastrate, mono, Länge (306 bzw. 250 Frames × 1152).
    #[test]
    fn testtoene_dekodieren() {
        for (n, rate, werte) in [("Testton 1.mp3", 44_100, 306 * 1152), ("Testton 2.mp3", 48_000, 250 * 1152)] {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/daten").join(n);
            let f = std::io::BufReader::new(std::fs::File::open(&p).unwrap());
            let mut r = nanomp3::Reader::<_, i16>::with_options(f, nanomp3::Options::default().skip_scan(true)).unwrap();
            let mut pcm = vec![0i16; nanomp3::MAX_SAMPLES_PER_FRAME];
            let mut n_ges = 0;
            let t = std::time::Instant::now();
            loop {
                let k = r.read(&mut pcm).unwrap();
                if k == 0 {
                    break;
                }
                n_ges += k;
                assert!(t.elapsed().as_secs() < 5, "{n}: hängt bei {n_ges}");
            }
            assert_eq!(r.sample_rate(), rate, "{n}");
            assert_eq!(r.channels(), Some(nanomp3::Channels::Mono), "{n}");
            assert_eq!(n_ges, werte, "{n}");
        }
    }

    /// D2: WAV – erzeugte Dateien (16 Bit stereo 22 050 Hz, 8 Bit mono
    /// 8 000 Hz, mit LIST-Block vor „data“) vollständig und wertgleich lesen;
    /// keine WAV bzw. 24 Bit wird abgelehnt.
    #[test]
    fn wav_lesen() {
        fn wav(rate: u32, kanaele: u16, bits: u16, daten: &[u8], mit_list: bool) -> Vec<u8> {
            let mut fmt = Vec::new();
            fmt.extend(1u16.to_le_bytes());
            fmt.extend(kanaele.to_le_bytes());
            fmt.extend(rate.to_le_bytes());
            fmt.extend((rate * u32::from(kanaele) * u32::from(bits / 8)).to_le_bytes());
            fmt.extend((kanaele * bits / 8).to_le_bytes());
            fmt.extend(bits.to_le_bytes());
            let mut d = b"WAVE".to_vec();
            d.extend(b"fmt ");
            d.extend(16u32.to_le_bytes());
            d.extend(fmt);
            if mit_list {
                d.extend(b"LIST");
                d.extend(5u32.to_le_bytes());
                d.extend(b"INFO\0\0"); // 5 Byte + Füllbyte
            }
            d.extend(b"data");
            d.extend((daten.len() as u32).to_le_bytes());
            d.extend(daten);
            let mut v = b"RIFF".to_vec();
            v.extend((d.len() as u32).to_le_bytes());
            v.extend(d);
            v
        }
        // 16 Bit stereo: 2205 Frames (0,1 s) Sägezahn
        let werte: Vec<i16> = (0..2205 * 2).map(|i| (i as i16).wrapping_mul(7)).collect();
        let roh: Vec<u8> = werte.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut w = super::WavLeser::neu(std::io::Cursor::new(wav(22_050, 2, 16, &roh, true))).unwrap();
        assert_eq!((w.rate, w.kanaele), (22_050, 2));
        let mut pcm = vec![0i16; 1152];
        let mut alle = Vec::new();
        loop {
            let n = w.lesen(&mut pcm);
            if n == 0 {
                break;
            }
            alle.extend_from_slice(&pcm[..n]);
        }
        assert_eq!(alle, werte);
        // 8 Bit mono
        let roh8: Vec<u8> = (0..800).map(|i| (i % 256) as u8).collect();
        let mut w = super::WavLeser::neu(std::io::Cursor::new(wav(8_000, 1, 8, &roh8, false))).unwrap();
        let n = w.lesen(&mut pcm);
        assert_eq!(n, 800);
        assert_eq!(pcm[0], -128 << 8);
        assert_eq!(pcm[128], 0);
        assert_eq!(w.lesen(&mut pcm), 0);
        // abgelehnt
        assert!(super::WavLeser::neu(std::io::Cursor::new(b"RIFF\0\0\0\0WAVX".to_vec())).is_err());
        assert!(super::WavLeser::neu(std::io::Cursor::new(wav(44_100, 2, 24, &[0; 12], false))).is_err());
        assert!(super::spielbar("Lied.WAV") && super::spielbar("a.mp3") && !super::spielbar("a.ogg"));
    }
}
