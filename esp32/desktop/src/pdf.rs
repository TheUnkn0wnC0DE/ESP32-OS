//! PDF lesen und Seiten zeichnen („Wenn Zeit bleibt“, PDF-Betrachter).
//!
//! Eigene, kleine Umsetzung (MuPDF, das KolibriOS nutzt, steht unter
//! AGPL-3.0 und passt nicht zur GPL-2.0 von ESP32-OS):
//!
//! - Datei: Querverweis-Tabelle oder -Strom (PDF 1.5) samt `/Prev`-Kette,
//!   Objektströme; ist der Querverweis kaputt, werden die Objekte in der
//!   Datei gesucht. Filter: Flate (mit PNG-Prädiktoren), ASCIIHex, ASCII85,
//!   DCT (JPEG über den Dekoder des Systems).
//! - Seiten: Seitenbaum mit geerbten Ressourcen und Grösse, Inhaltsströme
//!   mit Pfaden (gefüllt/gezogen), Farben (Grau, RGB, CMYK), Text,
//!   Bildern und Formular-XObjects.
//! - Text: Breiten aus der Schrift (`/Widths`, `/W`), Zeichen über
//!   `/ToUnicode`, die Kodierung bzw. `/Differences`; gezeichnet mit der
//!   Systemschrift, auf die Schriftgrösse geglättet skaliert (eingebettete
//!   Schriften werden nicht gerastert – siehe PLAN, Abweichungen).
//!
//! Beschnitt (`W`), Muster, Schattierungen und Transparenzgruppen werden
//! nicht ausgewertet (Inhalt wird ohne sie gezeichnet).

use kern::sprache::t;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

// ------------------------------------------------------------ Objekte

#[derive(Clone, Debug, PartialEq)]
pub enum Obj {
    Null,
    Bool(bool),
    Zahl(f64),
    Text(Vec<u8>),
    Name(Vec<u8>),
    Feld(Vec<Obj>),
    Dict(BTreeMap<Vec<u8>, Obj>),
    Strom(BTreeMap<Vec<u8>, Obj>, Vec<u8>),
    Ref(u32, u16),
}

impl Obj {
    pub fn zahl(&self) -> Option<f64> {
        if let Obj::Zahl(z) = self { Some(*z) } else { None }
    }
    pub fn name(&self) -> Option<&[u8]> {
        if let Obj::Name(n) = self { Some(n) } else { None }
    }
    pub fn feld(&self) -> Option<&[Obj]> {
        if let Obj::Feld(f) = self { Some(f) } else { None }
    }
    pub fn dict(&self) -> Option<&BTreeMap<Vec<u8>, Obj>> {
        match self {
            Obj::Dict(d) | Obj::Strom(d, _) => Some(d),
            _ => None,
        }
    }
    fn get(&self, k: &str) -> Option<&Obj> {
        self.dict().and_then(|d| d.get(k.as_bytes()))
    }
}

/// Fehler beim Lesen (Text für die Anzeige).
pub type Ergebnis<T> = Result<T, String>;

// ------------------------------------------------------------ Lexer

fn ist_leer(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

fn ist_trenner(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

/// Zerleger über einem Byte-Bereich (Datei oder Inhaltsstrom).
pub struct Leser<'a> {
    d: &'a [u8],
    pub pos: usize,
    /// Schachtelung von Feldern/Wörterbüchern (Stapel schützen)
    tiefe: u32,
}

/// Tiefste Schachtelung von [ ] und << >> (danach: kaputt).
const MAX_SCHACHTELUNG: u32 = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum Wort {
    Obj(Obj),
    /// Operator bzw. Schlüsselwort (obj, endobj, stream, R, Tj …)
    Op(Vec<u8>),
    FeldAuf,
    FeldZu,
    DictAuf,
    DictZu,
}

impl<'a> Leser<'a> {
    pub fn neu(d: &'a [u8], pos: usize) -> Leser<'a> {
        Leser { d, pos, tiefe: 0 }
    }

    fn leer_ueberspringen(&mut self) {
        while self.pos < self.d.len() {
            let c = self.d[self.pos];
            if ist_leer(c) {
                self.pos += 1;
            } else if c == b'%' {
                while self.pos < self.d.len() && !matches!(self.d[self.pos], 10 | 13) {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    pub fn wort(&mut self) -> Option<Wort> {
        self.leer_ueberspringen();
        let d = self.d;
        let c = *d.get(self.pos)?;
        match c {
            b'[' => {
                self.pos += 1;
                Some(Wort::FeldAuf)
            }
            b']' => {
                self.pos += 1;
                Some(Wort::FeldZu)
            }
            b'<' if d.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                Some(Wort::DictAuf)
            }
            b'>' if d.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Some(Wort::DictZu)
            }
            b'<' => {
                self.pos += 1;
                let mut h = Vec::new();
                while self.pos < d.len() && d[self.pos] != b'>' {
                    if d[self.pos].is_ascii_hexdigit() {
                        h.push(d[self.pos]);
                    }
                    self.pos += 1;
                }
                self.pos += 1;
                if h.len() % 2 == 1 {
                    h.push(b'0');
                }
                Some(Wort::Obj(Obj::Text(h.chunks(2).map(|p| hex(p[0]) << 4 | hex(p[1])).collect())))
            }
            b'(' => Some(Wort::Obj(Obj::Text(self.klammertext()))),
            b'/' => {
                self.pos += 1;
                let mut n = Vec::new();
                while self.pos < d.len() && !ist_leer(d[self.pos]) && !ist_trenner(d[self.pos]) {
                    if d[self.pos] == b'#' && self.pos + 2 < d.len() && d[self.pos + 1].is_ascii_hexdigit() && d[self.pos + 2].is_ascii_hexdigit() {
                        n.push(hex(d[self.pos + 1]) << 4 | hex(d[self.pos + 2]));
                        self.pos += 3;
                    } else {
                        n.push(d[self.pos]);
                        self.pos += 1;
                    }
                }
                Some(Wort::Obj(Obj::Name(n)))
            }
            b'{' | b'}' | b')' | b'>' => {
                self.pos += 1;
                Some(Wort::Op(vec![c]))
            }
            _ => {
                let a = self.pos;
                while self.pos < d.len() && !ist_leer(d[self.pos]) && !ist_trenner(d[self.pos]) {
                    self.pos += 1;
                }
                let w = &d[a..self.pos];
                if w.is_empty() {
                    self.pos += 1;
                    return Some(Wort::Op(vec![c]));
                }
                if let Some(z) = zahl(w) {
                    return Some(Wort::Obj(Obj::Zahl(z)));
                }
                Some(match w {
                    b"true" => Wort::Obj(Obj::Bool(true)),
                    b"false" => Wort::Obj(Obj::Bool(false)),
                    b"null" => Wort::Obj(Obj::Null),
                    _ => Wort::Op(w.to_vec()),
                })
            }
        }
    }

    fn klammertext(&mut self) -> Vec<u8> {
        let d = self.d;
        self.pos += 1;
        let mut tiefe = 1;
        let mut t = Vec::new();
        while self.pos < d.len() {
            let c = d[self.pos];
            self.pos += 1;
            match c {
                b'(' => {
                    tiefe += 1;
                    t.push(c);
                }
                b')' => {
                    tiefe -= 1;
                    if tiefe == 0 {
                        break;
                    }
                    t.push(c);
                }
                b'\\' => {
                    let Some(&e) = d.get(self.pos) else { break };
                    self.pos += 1;
                    match e {
                        b'n' => t.push(10),
                        b'r' => t.push(13),
                        b't' => t.push(9),
                        b'b' => t.push(8),
                        b'f' => t.push(12),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match d.get(self.pos) {
                                    Some(&o @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(o - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            t.push(v as u8);
                        }
                        13 => {
                            if d.get(self.pos) == Some(&10) {
                                self.pos += 1;
                            }
                        }
                        10 => {}
                        _ => t.push(e),
                    }
                }
                _ => t.push(c),
            }
        }
        t
    }

    /// Ein ganzes Objekt (Felder und Wörterbücher verschachtelt; „n g R“
    /// als Verweis). None am Ende oder bei einem Operator.
    pub fn objekt(&mut self) -> Option<Obj> {
        let w = self.wort()?;
        self.aus_wort(w)
    }

    fn aus_wort(&mut self, w: Wort) -> Option<Obj> {
        if matches!(w, Wort::FeldAuf | Wort::DictAuf) {
            if self.tiefe >= MAX_SCHACHTELUNG {
                return None;
            }
            self.tiefe += 1;
            let o = self.aus_wort_innen(w);
            self.tiefe -= 1;
            return o;
        }
        self.aus_wort_innen(w)
    }

    fn aus_wort_innen(&mut self, w: Wort) -> Option<Obj> {
        match w {
            Wort::Obj(Obj::Zahl(n)) => {
                // „n g R“?
                let merk = self.pos;
                if n >= 0.0 && n.fract() == 0.0 {
                    if let Some(Wort::Obj(Obj::Zahl(g))) = self.wort() {
                        if g.fract() == 0.0 && self.wort() == Some(Wort::Op(b"R".to_vec())) {
                            return Some(Obj::Ref(n as u32, g as u16));
                        }
                    }
                }
                self.pos = merk;
                Some(Obj::Zahl(n))
            }
            Wort::Obj(o) => Some(o),
            Wort::FeldAuf => {
                let mut f = Vec::new();
                loop {
                    match self.wort()? {
                        Wort::FeldZu => break,
                        w => match self.aus_wort(w) {
                            Some(o) => f.push(o),
                            None => continue,
                        },
                    }
                }
                Some(Obj::Feld(f))
            }
            Wort::DictAuf => {
                let mut d = BTreeMap::new();
                loop {
                    match self.wort()? {
                        Wort::DictZu => break,
                        Wort::Obj(Obj::Name(k)) => {
                            let w = self.wort()?;
                            if w == Wort::DictZu {
                                break;
                            }
                            if let Some(v) = self.aus_wort(w) {
                                d.insert(k, v);
                            }
                        }
                        _ => {}
                    }
                }
                Some(Obj::Dict(d))
            }
            _ => None,
        }
    }
}

fn hex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn zahl(w: &[u8]) -> Option<f64> {
    if !w.iter().all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.')) || !w.iter().any(u8::is_ascii_digit) {
        return None;
    }
    let s = std::str::from_utf8(w).ok()?;
    s.parse::<f64>().ok().or_else(|| {
        // „--5“ oder „5.“ u. ä. tolerant lesen
        let t: String = s.chars().skip_while(|c| *c == '-' && s.starts_with("--")).collect();
        t.trim_end_matches('.').parse().ok()
    })
}

// ------------------------------------------------------------ Datei

/// Geöffnete PDF-Datei.
pub struct Pdf {
    d: Vec<u8>,
    /// Objektnummer → Ort: (false, Byte-Versatz) bzw. (true, Objektstrom, Index)
    xref: BTreeMap<u32, Ort>,
    trailer: BTreeMap<Vec<u8>, Obj>,
    /// Seiten (Wörterbücher mit geerbten Werten)
    pub seiten: Vec<Seite>,
    /// Zuletzt entpackte Objektströme (vorn der neueste): jedes Objekt darin
    /// würde sonst den ganzen Strom neu entpacken.
    objektstroeme: RefCell<Vec<(u32, Rc<Objektstrom>)>>,
    /// Laufende Objekt-Auflösungen (Verweise auf sich selbst, z. B. /Length
    /// oder Filter eines Objektstroms aus demselben Strom, enden hier)
    aufloesungen: std::cell::Cell<u32>,
}

/// So viele Objekt-Auflösungen dürfen ineinander laufen.
const MAX_AUFLOESUNG: u32 = 16;

/// Entpackter Objektstrom: Daten, Beginn des ersten Objekts, Versätze.
struct Objektstrom {
    daten: Vec<u8>,
    erste: usize,
    orte: Vec<usize>,
}

/// Höchstens so viele Einträge in Breiten-/Unicode-Tabellen einer Schrift
/// (ein Unicode-Plane und etwas Rand; mehr ist kaputt).
const MAX_ZUORDNUNG: usize = 70_000;

/// So viele entpackte Objektströme bleiben im Speicher.
const OBJEKTSTROEME: usize = 4;

#[derive(Clone, Copy, Debug)]
enum Ort {
    Datei(usize),
    InStrom(u32, u32),
}

/// Eine Seite: Grösse (MediaBox, in Punkt), Drehung, Inhalt, Ressourcen.
#[derive(Clone, Debug)]
pub struct Seite {
    pub kasten: [f64; 4],
    pub drehung: i32,
    inhalt: Vec<Obj>,
    ressourcen: Obj,
}

impl Pdf {
    pub fn oeffnen(d: Vec<u8>) -> Ergebnis<Pdf> {
        if !d.starts_with(b"%PDF") && !d.windows(5).take(1024).any(|w| w == b"%PDF-") {
            return Err(t("not a PDF file", "keine PDF-Datei").into());
        }
        let mut p = Pdf { d, xref: BTreeMap::new(), trailer: BTreeMap::new(), seiten: Vec::new(), objektstroeme: RefCell::new(Vec::new()), aufloesungen: std::cell::Cell::new(0) };
        if p.xref_lesen().is_err() || p.trailer.get(b"Root".as_slice()).is_none() {
            p.xref_suchen();
        }
        if p.trailer.get(b"Encrypt".as_slice()).is_some() {
            return Err(t("encrypted PDF file (not supported)", "verschlüsselte PDF-Datei (nicht unterstützt)").into());
        }
        let wurzel = p.trailer.get(b"Root".as_slice()).cloned().ok_or("ohne Wurzel (/Root)")?;
        let wurzel = p.aufloesen(&wurzel);
        let seiten = wurzel.get("Pages").cloned().ok_or("ohne Seitenbaum")?;
        let mut liste = Vec::new();
        p.seiten_sammeln(&seiten, (None, None), None, 0, &mut liste, 0, &mut std::collections::BTreeSet::new());
        if liste.is_empty() {
            return Err(t("no pages", "keine Seiten").into());
        }
        p.seiten = liste;
        Ok(p)
    }

    fn xref_lesen(&mut self) -> Ergebnis<()> {
        let ende = self.d.len().saturating_sub(1100);
        let s = self.d[ende..].windows(9).rposition(|w| w == b"startxref").ok_or("startxref fehlt")? + ende + 9;
        let mut l = Leser::neu(&self.d, s);
        let mut pos = l.objekt().and_then(|o| o.zahl()).ok_or("startxref")? as usize;
        let mut gesehen = Vec::new();
        while pos > 0 && pos < self.d.len() && !gesehen.contains(&pos) {
            gesehen.push(pos);
            let trailer = if self.d[pos..].starts_with(b"xref") { self.xref_tabelle(pos)? } else { self.xref_strom(pos)? };
            for (k, v) in &trailer {
                self.trailer.entry(k.clone()).or_insert_with(|| v.clone());
            }
            // Hybrid-Dateien: zusätzlicher Querverweis-Strom
            if let Some(x) = trailer.get(b"XRefStm".as_slice()).and_then(Obj::zahl) {
                let _ = self.xref_strom(x as usize);
            }
            pos = trailer.get(b"Prev".as_slice()).and_then(Obj::zahl).map_or(0, |z| z as usize);
        }
        Ok(())
    }

    fn xref_tabelle(&mut self, pos: usize) -> Ergebnis<BTreeMap<Vec<u8>, Obj>> {
        let mut l = Leser::neu(&self.d, pos + 4);
        loop {
            match l.wort() {
                Some(Wort::Obj(Obj::Zahl(erste))) => {
                    let n = l.objekt().and_then(|o| o.zahl()).ok_or("xref")? as u32;
                    for i in 0..n {
                        let off = l.objekt().and_then(|o| o.zahl()).ok_or("xref")?;
                        let _gen = l.objekt();
                        let art = l.wort();
                        let nr = erste as u32 + i;
                        if art == Some(Wort::Op(b"n".to_vec())) && off > 0.0 {
                            self.xref.entry(nr).or_insert(Ort::Datei(off as usize));
                        }
                    }
                }
                Some(Wort::Op(w)) if w == b"trailer" => {
                    return l.objekt().and_then(|o| o.dict().cloned()).ok_or_else(|| "trailer".into());
                }
                _ => return Err("xref-Tabelle".into()),
            }
        }
    }

    fn xref_strom(&mut self, pos: usize) -> Ergebnis<BTreeMap<Vec<u8>, Obj>> {
        let (_, o) = self.objekt_bei(pos).ok_or("xref-Strom")?;
        let Obj::Strom(dict, _) = &o else { return Err("xref-Strom".into()) };
        let daten = self.strom_daten(&o)?;
        let w: Vec<usize> = dict.get(b"W".as_slice()).and_then(Obj::feld).ok_or("/W")?.iter().map(|z| z.zahl().unwrap_or(0.0) as usize).collect();
        // je Feld höchstens 8 Byte, Zeile nicht leer (sonst liefe die
        // Schleife über /Size ohne Daten)
        if w.len() < 3 || w.iter().any(|&b| b > 8) || w[..3].iter().sum::<usize>() == 0 {
            return Err("/W".into());
        }
        let groesse = dict.get(b"Size".as_slice()).and_then(Obj::zahl).unwrap_or(0.0) as u32;
        let index: Vec<u32> = dict
            .get(b"Index".as_slice())
            .and_then(Obj::feld)
            .map(|f| f.iter().map(|z| z.zahl().unwrap_or(0.0) as u32).collect())
            .unwrap_or_else(|| vec![0, groesse]);
        let breite = w[0] + w[1] + w[2];
        let lies = |b: &[u8]| b.iter().fold(0usize, |a, &c| a << 8 | usize::from(c));
        let mut z = 0;
        for paar in index.chunks(2) {
            let (erste, n) = (paar[0], *paar.get(1).unwrap_or(&0));
            for i in 0..n {
                let Some(e) = daten.get(z * breite..(z + 1) * breite) else { break };
                z += 1;
                let art = if w[0] == 0 { 1 } else { lies(&e[..w[0]]) };
                let f2 = lies(&e[w[0]..w[0] + w[1]]);
                let f3 = lies(&e[w[0] + w[1]..]);
                let ort = match art {
                    1 => Ort::Datei(f2),
                    2 => Ort::InStrom(f2 as u32, f3 as u32),
                    _ => continue,
                };
                self.xref.entry(erste + i).or_insert(ort);
            }
        }
        Ok(dict.clone())
    }

    /// Querverweis kaputt: alle „n g obj“ in der Datei sammeln, Trailer aus
    /// dem letzten „trailer“ bzw. dem Katalog.
    fn xref_suchen(&mut self) {
        let d = &self.d;
        let mut i = 0;
        let mut neu = BTreeMap::new();
        let mut katalog = None;
        while let Some(o) = d[i..].windows(4).position(|w| w == b" obj").map(|o| o + i) {
            i = o + 4;
            // davor „nr gen“: Ziffern, Leerzeichen, Ziffern
            let ziffern_zurueck = |mut j: usize| {
                while j > 0 && d[j - 1].is_ascii_digit() {
                    j -= 1;
                }
                j
            };
            let gen_anfang = ziffern_zurueck(o);
            if gen_anfang == o || gen_anfang == 0 || !ist_leer(d[gen_anfang - 1]) {
                continue;
            }
            let start = ziffern_zurueck(gen_anfang - 1);
            if start == gen_anfang - 1 {
                continue;
            }
            let Some(nr) = std::str::from_utf8(&d[start..gen_anfang - 1]).ok().and_then(|s| s.parse::<u32>().ok()) else { continue };
            neu.insert(nr, Ort::Datei(start));
            if katalog.is_none() && d[o..(o + 200).min(d.len())].windows(8).any(|w| w == b"/Catalog") {
                katalog = Some(nr);
            }
        }
        for (k, v) in neu {
            self.xref.insert(k, v);
        }
        if let Some(t) = d.windows(7).rposition(|w| w == b"trailer") {
            let mut l = Leser::neu(d, t + 7);
            if let Some(Obj::Dict(td)) = l.objekt() {
                for (k, v) in td {
                    self.trailer.entry(k).or_insert(v);
                }
            }
        }
        if self.trailer.get(b"Root".as_slice()).is_none()
            && let Some(k) = katalog
        {
            self.trailer.insert(b"Root".to_vec(), Obj::Ref(k, 0));
        }
    }

    /// Objekt „n g obj … endobj“ ab `pos` (mit Strom).
    fn objekt_bei(&self, pos: usize) -> Option<(u32, Obj)> {
        let mut l = Leser::neu(&self.d, pos);
        let nr = l.objekt()?.zahl()? as u32;
        let _gen = l.objekt()?;
        if l.wort()? != Wort::Op(b"obj".to_vec()) {
            return None;
        }
        let o = l.objekt()?;
        if let Obj::Dict(dict) = &o
            && l.wort() == Some(Wort::Op(b"stream".to_vec()))
        {
            let mut a = l.pos;
            if self.d.get(a) == Some(&13) {
                a += 1;
            }
            if self.d.get(a) == Some(&10) {
                a += 1;
            }
            let len = dict.get(b"Length".as_slice()).map(|v| self.aufloesen(v)).and_then(|v| v.zahl()).map(|z| z as usize);
            let ende = match len {
                Some(n) if a.checked_add(n).is_some_and(|e| e <= self.d.len()) && self.d[a + n..(a + n + 20).min(self.d.len())].windows(9).any(|w| w == b"endstream") => a + n,
                _ => self.d[a..].windows(9).position(|w| w == b"endstream").map(|p| {
                    let mut e = a + p;
                    while e > a && matches!(self.d[e - 1], 10 | 13) {
                        e -= 1;
                    }
                    e
                })?,
            };
            return Some((nr, Obj::Strom(dict.clone(), self.d[a..ende].to_vec())));
        }
        Some((nr, o))
    }

    /// Objekt `nr` holen (aus der Datei oder einem Objektstrom).
    pub fn objekt(&self, nr: u32) -> Obj {
        self.objekt_tief(nr, 0)
    }

    fn objekt_tief(&self, nr: u32, tiefe: u32) -> Obj {
        let laufend = self.aufloesungen.get();
        if tiefe > 8 || laufend >= MAX_AUFLOESUNG {
            return Obj::Null;
        }
        self.aufloesungen.set(laufend + 1);
        let o = self.objekt_tief_innen(nr, tiefe);
        self.aufloesungen.set(laufend);
        o
    }

    fn objekt_tief_innen(&self, nr: u32, tiefe: u32) -> Obj {
        match self.xref.get(&nr) {
            Some(Ort::Datei(p)) => self.objekt_bei(*p).map_or(Obj::Null, |(_, o)| o),
            Some(Ort::InStrom(s, idx)) => {
                let Some(os) = self.objektstrom(*s, tiefe) else { return Obj::Null };
                let Some(&off) = os.orte.get(*idx as usize) else { return Obj::Null };
                let mut l = Leser::neu(&os.daten, os.erste + off);
                l.objekt().unwrap_or(Obj::Null)
            }
            None => Obj::Null,
        }
    }

    /// Objektstrom `s` entpackt – aus dem Zwischenspeicher oder neu.
    fn objektstrom(&self, s: u32, tiefe: u32) -> Option<Rc<Objektstrom>> {
        let gefunden = self.objektstroeme.borrow().iter().position(|(n, _)| *n == s);
        if let Some(i) = gefunden {
            let mut c = self.objektstroeme.borrow_mut();
            let e = c.remove(i);
            let os = e.1.clone();
            c.insert(0, e);
            return Some(os);
        }
        let strom = self.objekt_tief(s, tiefe + 1);
        let daten = self.strom_daten(&strom).ok()?;
        let n = strom.get("N").and_then(Obj::zahl).unwrap_or(0.0) as usize;
        let erste = strom.get("First").and_then(Obj::zahl).unwrap_or(0.0) as usize;
        let mut l = Leser::neu(&daten, 0);
        let mut orte = Vec::new();
        for _ in 0..n.min(1_000_000) {
            let (Some(_), Some(b)) = (l.objekt().and_then(|o| o.zahl()), l.objekt().and_then(|o| o.zahl())) else { break };
            orte.push(b as usize);
        }
        let os = Rc::new(Objektstrom { daten, erste, orte });
        let mut c = self.objektstroeme.borrow_mut();
        c.insert(0, (s, os.clone()));
        c.truncate(OBJEKTSTROEME);
        Some(os)
    }

    /// Verweis auflösen (mehrstufig), sonst das Objekt selbst.
    pub fn aufloesen(&self, o: &Obj) -> Obj {
        let mut o = o.clone();
        for _ in 0..8 {
            match o {
                Obj::Ref(n, _) => o = self.objekt(n),
                _ => return o,
            }
        }
        Obj::Null
    }

    fn feld_von(&self, o: Option<&Obj>) -> Vec<Obj> {
        match o.map(|o| self.aufloesen(o)) {
            Some(Obj::Feld(f)) => f,
            Some(Obj::Null) | None => Vec::new(),
            Some(o) => vec![o],
        }
    }

    /// Seitenbaum ablaufen; geerbt werden MediaBox, CropBox, Resources und
    /// Rotate. Jeder Knoten nur einmal (`besucht`) – ein Knoten, der auf sich
    /// selbst zeigt, liefe sonst exponentiell lange.
    #[allow(clippy::too_many_arguments)]
    fn seiten_sammeln(&self, knoten: &Obj, (kasten, zuschnitt): (Option<[f64; 4]>, Option<[f64; 4]>), res: Option<Obj>, drehung: i32, aus: &mut Vec<Seite>, tiefe: u32, besucht: &mut std::collections::BTreeSet<u32>) {
        if tiefe > 32 || aus.len() > 5000 {
            return;
        }
        if let Obj::Ref(nr, _) = knoten
            && !besucht.insert(*nr)
        {
            return;
        }
        let k = self.aufloesen(knoten);
        let kasten = k.get("MediaBox").map(|v| self.aufloesen(v)).and_then(|v| kasten_aus(&v)).or(kasten);
        let zuschnitt = k.get("CropBox").map(|v| self.aufloesen(v)).and_then(|v| kasten_aus(&v)).or(zuschnitt);
        let res = k.get("Resources").map(|v| self.aufloesen(v)).or(res);
        let drehung = k.get("Rotate").and_then(|v| self.aufloesen(v).zahl()).map_or(drehung, |z| z as i32);
        if let Some(kinder) = k.get("Kids") {
            for kind in self.feld_von(Some(kinder)) {
                self.seiten_sammeln(&kind, (kasten, zuschnitt), res.clone(), drehung, aus, tiefe + 1, besucht);
            }
            return;
        }
        if k.dict().is_none() {
            return;
        }
        let kasten = zuschnitt.or(kasten).unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let inhalt = self.feld_von(k.get("Contents"));
        aus.push(Seite { kasten, drehung: drehung.rem_euclid(360), inhalt, ressourcen: res.unwrap_or(Obj::Null) });
    }

    /// Daten eines Stroms nach seinen Filtern.
    pub fn strom_daten(&self, o: &Obj) -> Ergebnis<Vec<u8>> {
        let Obj::Strom(dict, roh) = o else { return Err("kein Strom".into()) };
        let filter = self.feld_von(dict.get(b"Filter".as_slice()));
        let parms = self.feld_von(dict.get(b"DecodeParms".as_slice()).or(dict.get(b"DP".as_slice())));
        let mut d = roh.clone();
        for (i, f) in filter.iter().enumerate() {
            let name = f.name().unwrap_or(b"");
            d = match name {
                b"FlateDecode" | b"Fl" => {
                    let roh = flate(&d, entpack_grenze())?;
                    let p = parms.get(i).map(|p| self.aufloesen(p));
                    praediktor(roh, p.as_ref())
                }
                b"LZWDecode" | b"LZW" => {
                    let p = parms.get(i).map(|p| self.aufloesen(p));
                    let frueh = p.as_ref().and_then(|p| p.get("EarlyChange").and_then(Obj::zahl)).is_none_or(|z| z != 0.0);
                    praediktor(lzw(&d, frueh, entpack_grenze()), p.as_ref())
                }
                b"RunLengthDecode" | b"RL" => lauflaenge(&d, entpack_grenze()),
                b"ASCIIHexDecode" | b"AHx" => ascii_hex(&d),
                b"ASCII85Decode" | b"A85" => ascii85(&d),
                // Bilder: Rohdaten bleiben für den Bild-Dekoder
                b"DCTDecode" | b"DCT" | b"JPXDecode" => return Ok(d),
                n => return Err(format!("Filter {} nicht unterstützt", String::from_utf8_lossy(n))),
            };
        }
        Ok(d)
    }
}

/// LZW (PDF 7.4.4): Codes 9–12 Bit, 256 = Tabelle leeren, 257 = Ende;
/// `frueh` = EarlyChange (Breite wächst einen Code früher, Standard).
fn lzw(d: &[u8], frueh: bool, max: usize) -> Vec<u8> {
    const KEIN: u16 = u16::MAX;
    let mut praefix = vec![KEIN; 4096];
    let mut letztes = vec![0u8; 4096];
    let mut laenge = vec![0u16; 4096];
    for i in 0..256 {
        letztes[i] = i as u8;
        laenge[i] = 1;
    }
    let erstes = |mut c: usize, praefix: &[u16], letztes: &[u8]| {
        while praefix[c] != KEIN {
            c = praefix[c] as usize;
        }
        letztes[c]
    };
    let (mut n, mut breite) = (258usize, 9u32);
    let (mut puffer, mut bits) = (0u32, 0u32);
    let mut vorher: Option<usize> = None;
    let mut aus = Vec::new();
    for &b in d {
        puffer = (puffer << 8 | u32::from(b)) & 0xFF_FFFF;
        bits += 8;
        while bits >= breite {
            bits -= breite;
            let code = (puffer >> bits) as usize & ((1 << breite) - 1);
            if code == 256 {
                n = 258;
                breite = 9;
                vorher = None;
                continue;
            }
            if code == 257 {
                return aus;
            }
            let c = match vorher {
                None if code < 256 => code,
                None => return aus,
                Some(v) => {
                    if code > n || (code == n && n >= 4096) {
                        return aus;
                    }
                    // neuer Eintrag: voriger Code + erstes Zeichen des
                    // aktuellen (bei code == n: des vorigen, „KwKwK“)
                    let erster = erstes(if code == n { v } else { code }, &praefix, &letztes);
                    if n < 4096 {
                        praefix[n] = v as u16;
                        letztes[n] = erster;
                        laenge[n] = laenge[v] + 1;
                        n += 1;
                    }
                    code
                }
            };
            let len = laenge[c] as usize;
            let start = aus.len();
            if start + len > max {
                return aus;
            }
            aus.resize(start + len, 0);
            let mut k = c;
            for i in (0..len).rev() {
                aus[start + i] = letztes[k];
                if praefix[k] != KEIN {
                    k = praefix[k] as usize;
                }
            }
            vorher = Some(c);
            let grenze = n + usize::from(frueh);
            breite = match grenze {
                0..512 => 9,
                512..1024 => 10,
                1024..2048 => 11,
                _ => 12,
            };
        }
    }
    aus
}

/// RunLength (PDF 7.4.5): Länge 0–127 = n+1 Bytes wörtlich, 129–255 =
/// nächstes Byte 257−n mal, 128 = Ende.
fn lauflaenge(d: &[u8], max: usize) -> Vec<u8> {
    let mut aus = Vec::new();
    let mut i = 0;
    while i < d.len() && aus.len() <= max {
        let n = d[i] as usize;
        i += 1;
        match n {
            0..=127 => {
                let e = (i + n + 1).min(d.len());
                aus.extend_from_slice(&d[i..e]);
                i = e;
            }
            128 => break,
            _ => {
                if let Some(&b) = d.get(i) {
                    aus.extend(std::iter::repeat_n(b, 257 - n));
                }
                i += 1;
            }
        }
    }
    aus
}

/// Reicht der Speicher des Programms für `bytes` am Stück (mit Reserve)?
/// Ohne eigenen Heap (Werkzeuge am PC) immer.
pub fn speicher_reicht(bytes: u64) -> bool {
    let frei = kern::sys::programm_speicher_frei().is_none_or(|f| f as u64 >= bytes + 512 * 1024);
    let am_stueck = kern::sys::programm_speicher_groesster_block().is_none_or(|g| g as u64 >= bytes + 64 * 1024);
    frei && am_stueck
}

// ------------------------------------------------------------ Farbräume

/// Farbraum (Inhaltsströme und Bilder).
#[derive(Clone, Debug, PartialEq)]
enum Raum {
    Grau,
    Rgb,
    Cmyk,
    /// CIE L*a*b* (nach sRGB, Weisspunkt D65)
    Lab,
    /// Separation/DeviceN: `n` Farbanteile (0 = Papier). Mit Typ-2-Funktion
    /// (C0, C1, N) in den Ersatzraum, sonst Grau nach der Farbmenge.
    Tinte { n: usize, ersatz: Box<Raum>, funktion: Option<(Vec<f64>, Vec<f64>, f64)> },
    /// Indexed: Grundraum und Farbtabelle
    Index(Box<Raum>, Vec<u8>),
    /// Muster (Pattern) werden nicht gezeichnet: mittleres Grau
    Muster,
}

impl Raum {
    fn komponenten(&self) -> usize {
        match self {
            Raum::Grau | Raum::Index(..) | Raum::Muster => 1,
            Raum::Rgb | Raum::Lab => 3,
            Raum::Cmyk => 4,
            Raum::Tinte { n, .. } => *n,
        }
    }

    /// Farbe nach `cs`/`CS` (PDF 8.6.8): schwarz bzw. volle Farbe.
    fn anfang(&self) -> u32 {
        match self {
            Raum::Cmyk => self.farbe(&[0.0, 0.0, 0.0, 1.0]),
            Raum::Tinte { n, .. } => self.farbe(&vec![1.0; *n]),
            _ => self.farbe(&vec![0.0; self.komponenten()]),
        }
    }

    fn farbe(&self, z: &[f64]) -> u32 {
        let k = |i: usize| z.get(i).copied().unwrap_or(0.0);
        match self {
            Raum::Grau => grau(k(0)),
            Raum::Rgb => rgb(k(0), k(1), k(2)),
            Raum::Cmyk => cmyk(k(0), k(1), k(2), k(3)),
            Raum::Lab => lab(k(0), k(1), k(2)),
            Raum::Tinte { n, ersatz, funktion } => match funktion {
                Some((c0, c1, e)) if *n == 1 => {
                    let t = k(0).clamp(0.0, 1.0).powf(*e);
                    // ungleich lange C0/C1 (unsauber erzeugt): fehlende Werte
                    // wie die Vorgaben C0 = 0, C1 = 1
                    let w: Vec<f64> = (0..c0.len().max(c1.len()))
                        .map(|i| {
                            let (a, b) = (c0.get(i).copied().unwrap_or(0.0), c1.get(i).copied().unwrap_or(1.0));
                            a + t * (b - a)
                        })
                        .collect();
                    ersatz.farbe(&w)
                }
                _ => {
                    let menge = (0..*n).map(k).sum::<f64>() / (*n).max(1) as f64;
                    grau(1.0 - menge.clamp(0.0, 1.0))
                }
            },
            Raum::Index(basis, tab) => {
                let bk = basis.komponenten();
                let i = k(0).round().max(0.0) as usize * bk;
                let w: Vec<f64> = (0..bk).map(|j| tab.get(i + j).map_or(0.0, |&v| f64::from(v) / 255.0)).collect();
                basis.farbe(&w)
            }
            Raum::Muster => 0xFF80_8080,
        }
    }
}

/// CIE L*a*b* → sRGB (Weisspunkt D65).
fn lab(l: f64, a: f64, b: f64) -> u32 {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let g = |t: f64| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9505 * g(fx), g(fy), 1.089 * g(fz));
    let s = |c: f64| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    };
    rgb(
        s(3.2406 * x - 1.5372 * y - 0.4986 * z),
        s(-0.9689 * x + 1.8758 * y + 0.0415 * z),
        s(0.0557 * x - 0.2040 * y + 1.0570 * z),
    )
}

impl Pdf {
    /// Farbraum aus Name bzw. Feld (ohne Ressourcen-Namen, siehe
    /// `Zeichner::raum_name`).
    fn raum(&self, o: &Obj, tiefe: u32) -> Raum {
        let o = self.aufloesen(o);
        match &o {
            Obj::Name(n) => match n.as_slice() {
                b"DeviceRGB" | b"CalRGB" | b"RGB" => Raum::Rgb,
                b"DeviceCMYK" | b"CMYK" => Raum::Cmyk,
                b"Pattern" => Raum::Muster,
                _ => Raum::Grau,
            },
            Obj::Feld(f) if tiefe < 4 => {
                let art = f.first().and_then(Obj::name).unwrap_or(b"");
                match art {
                    b"ICCBased" => {
                        let n = f.get(1).map(|v| self.aufloesen(v)).and_then(|v| v.get("N").and_then(Obj::zahl)).unwrap_or(3.0);
                        match n as u32 {
                            1 => Raum::Grau,
                            4 => Raum::Cmyk,
                            _ => Raum::Rgb,
                        }
                    }
                    b"CalGray" => Raum::Grau,
                    b"CalRGB" => Raum::Rgb,
                    b"Lab" => Raum::Lab,
                    b"Indexed" | b"I" => {
                        let basis = f.get(1).map_or(Raum::Rgb, |b| self.raum(b, tiefe + 1));
                        let tab = match f.get(3).map(|v| self.aufloesen(v)) {
                            Some(Obj::Text(t)) => t,
                            Some(st @ Obj::Strom(..)) => self.strom_daten(&st).unwrap_or_default(),
                            _ => Vec::new(),
                        };
                        Raum::Index(Box::new(basis), tab)
                    }
                    b"Separation" | b"DeviceN" => {
                        let n = if art == b"Separation" {
                            1
                        } else {
                            f.get(1).map(|v| self.aufloesen(v)).and_then(|v| v.feld().map(<[Obj]>::len)).unwrap_or(1)
                        };
                        let ersatz = f.get(2).map_or(Raum::Grau, |b| self.raum(b, tiefe + 1));
                        let fu = f.get(3).map(|v| self.aufloesen(v));
                        let funktion = fu.as_ref().filter(|fu| fu.get("FunctionType").and_then(Obj::zahl) == Some(2.0)).map(|fu| {
                            let feld = |k: &str, std: f64| {
                                fu.get(k).map(|v| self.aufloesen(v)).and_then(|v| v.feld().map(|f| f.iter().filter_map(Obj::zahl).collect())).unwrap_or_else(|| vec![std])
                            };
                            (feld("C0", 0.0), feld("C1", 1.0), fu.get("N").and_then(Obj::zahl).unwrap_or(1.0))
                        });
                        Raum::Tinte { n: n.max(1), ersatz: Box::new(ersatz), funktion }
                    }
                    b"Pattern" => Raum::Muster,
                    _ if f.len() == 1 => self.raum(&f[0], tiefe + 1),
                    _ => Raum::Grau,
                }
            }
            _ => Raum::Grau,
        }
    }
}

fn kasten_aus(o: &Obj) -> Option<[f64; 4]> {
    let f = o.feld()?;
    if f.len() < 4 {
        return None;
    }
    let z: Vec<f64> = f.iter().map(|v| v.zahl().unwrap_or(0.0)).collect();
    Some([z[0].min(z[2]), z[1].min(z[3]), z[0].max(z[2]), z[1].max(z[3])])
}

/// zlib (2 Byte Kopf) oder rohes Deflate.
fn flate(d: &[u8], max: usize) -> Ergebnis<Vec<u8>> {
    let kopf = d.len() > 2 && d[0] & 0x0F == 8 && (u16::from(d[0]) << 8 | u16::from(d[1])) % 31 == 0;
    let roh = if kopf { &d[2..] } else { d };
    kolibri::dekoder::inflate_max(roh, d.len().saturating_mul(4), max).map_err(|_| t("Flate data damaged or too large", "Flate-Daten beschädigt oder zu gross").to_string())
}

/// Grösste Länge entpackter Daten eines Stroms: was gerade in den Speicher
/// des Programms passt (Platz zum Umkopieren beim Wachsen), höchstens
/// 16 MB. Am Board nähme der Kern sonst Speicher aus dem System-Heap, bis
/// die Firmware abbricht.
fn entpack_grenze() -> usize {
    let frei = kern::sys::programm_speicher_frei().map_or(usize::MAX, |f| f.saturating_sub(512 * 1024) / 3);
    frei.min(16 << 20)
}

/// Prädiktoren: PNG (Predictor ≥ 10) und TIFF 2 bei 8 Bit je Komponente
/// (TIFF 2 mit anderer Tiefe: Abweichung Z1, Daten bleiben wie sie sind).
fn praediktor(d: Vec<u8>, p: Option<&Obj>) -> Vec<u8> {
    let Some(p) = p else { return d };
    let pred = p.get("Predictor").and_then(Obj::zahl).unwrap_or(1.0) as u32;
    if pred < 2 || (pred > 2 && pred < 10) {
        return d;
    }
    let farben = p.get("Colors").and_then(Obj::zahl).unwrap_or(1.0) as usize;
    let bits = p.get("BitsPerComponent").and_then(Obj::zahl).unwrap_or(8.0) as usize;
    let spalten = p.get("Columns").and_then(Obj::zahl).unwrap_or(1.0) as usize;
    let bpp = farben.saturating_mul(bits).div_ceil(8).max(1);
    // unsinnige Zeilenlänge (länger als die Daten, Überlauf): Daten bleiben
    let Some(zeile) = spalten.checked_mul(farben).and_then(|z| z.checked_mul(bits)).map(|z| z.div_ceil(8)) else { return d };
    if zeile == 0 || zeile > d.len() {
        return d;
    }
    if pred == 2 {
        let mut d = d;
        if bits == 8 {
            for z in d.chunks_mut(zeile) {
                for i in bpp..z.len() {
                    z[i] = z[i].wrapping_add(z[i - bpp]);
                }
            }
        }
        return d;
    }
    let mut aus = Vec::with_capacity(d.len());
    let mut vorige = vec![0u8; zeile];
    for z in d.chunks(zeile + 1) {
        if z.len() < 2 {
            break;
        }
        let (art, roh) = (z[0], &z[1..]);
        let mut cur = vec![0u8; zeile];
        for i in 0..roh.len().min(zeile) {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = vorige[i];
            let c = if i >= bpp { vorige[i - bpp] } else { 0 };
            cur[i] = roh[i].wrapping_add(match art {
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => {
                    let p = i16::from(a) + i16::from(b) - i16::from(c);
                    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => 0,
            });
        }
        aus.extend_from_slice(&cur);
        vorige = cur;
    }
    aus
}

fn ascii_hex(d: &[u8]) -> Vec<u8> {
    let h: Vec<u8> = d.iter().copied().take_while(|&c| c != b'>').filter(u8::is_ascii_hexdigit).collect();
    h.chunks(2).map(|p| hex(p[0]) << 4 | hex(*p.get(1).unwrap_or(&b'0'))).collect()
}

fn ascii85(d: &[u8]) -> Vec<u8> {
    let mut aus = Vec::new();
    let mut gruppe = [0u32; 5];
    let mut n = 0;
    for &c in d {
        match c {
            b'~' => break,
            b'z' if n == 0 => aus.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                gruppe[n] = u32::from(c - b'!');
                n += 1;
                if n == 5 {
                    let v = gruppe.iter().fold(0u64, |a, &g| a * 85 + u64::from(g)) as u32;
                    aus.extend_from_slice(&v.to_be_bytes());
                    n = 0;
                }
            }
            _ => {}
        }
    }
    if n > 1 {
        for g in gruppe.iter_mut().skip(n) {
            *g = 84;
        }
        let v = gruppe.iter().fold(0u64, |a, &g| a * 85 + u64::from(g)) as u32;
        aus.extend_from_slice(&v.to_be_bytes()[..n - 1]);
    }
    aus
}

// ------------------------------------------------------------ Schriften

/// Schrift einer Seite: Breiten, Zeichenzuordnung, Bytes je Code.
struct Schrift {
    zwei_byte: bool,
    erste: u32,
    breiten: Vec<f64>,
    cid_breiten: BTreeMap<u32, f64>,
    standard_breite: f64,
    unicode: BTreeMap<u32, String>,
    kodierung: [char; 256],
}

impl Schrift {
    fn laden(pdf: &Pdf, o: &Obj) -> Schrift {
        let o = pdf.aufloesen(o);
        let art = o.get("Subtype").and_then(Obj::name).unwrap_or(b"");
        let zwei_byte = art == b"Type0";
        let mut s = Schrift {
            zwei_byte,
            erste: o.get("FirstChar").and_then(|v| pdf.aufloesen(v).zahl()).unwrap_or(0.0) as u32,
            breiten: pdf.feld_von(o.get("Widths")).iter().map(|v| pdf.aufloesen(v).zahl().unwrap_or(0.0)).collect(),
            cid_breiten: BTreeMap::new(),
            standard_breite: if zwei_byte { 1000.0 } else { 0.0 },
            unicode: BTreeMap::new(),
            kodierung: kodierung_basis(b"StandardEncoding"),
        };
        // Type3: Breiten im Glyphenraum, umgerechnet mit /FontMatrix (sonst
        // wie alle Schriften Tausendstel der Schriftgrösse)
        if art == b"Type3" {
            let a = pdf.feld_von(o.get("FontMatrix")).first().and_then(|v| pdf.aufloesen(v).zahl()).unwrap_or(0.001);
            for w in &mut s.breiten {
                *w *= a * 1000.0;
            }
        }
        if zwei_byte && let Some(nach) = pdf.feld_von(o.get("DescendantFonts")).first() {
            let nach = pdf.aufloesen(nach);
            if let Some(dw) = nach.get("DW").and_then(|v| pdf.aufloesen(v).zahl()) {
                s.standard_breite = dw;
            }
            let w = pdf.feld_von(nach.get("W"));
            let mut i = 0;
            while i < w.len() {
                let a = pdf.aufloesen(&w[i]).zahl().unwrap_or(0.0) as u32;
                match w.get(i + 1).map(|v| pdf.aufloesen(v)) {
                    Some(Obj::Feld(f)) => {
                        for (k, v) in f.iter().enumerate().take(MAX_ZUORDNUNG.saturating_sub(s.cid_breiten.len())) {
                            s.cid_breiten.insert(a.saturating_add(k as u32), pdf.aufloesen(v).zahl().unwrap_or(0.0));
                        }
                        i += 2;
                    }
                    Some(Obj::Zahl(b)) => {
                        let breite = w.get(i + 2).and_then(|v| pdf.aufloesen(v).zahl()).unwrap_or(0.0);
                        let platz = MAX_ZUORDNUNG.saturating_sub(s.cid_breiten.len()) as u32;
                        for c in a..=(b as u32).min(a.saturating_add(platz.min(65535))) {
                            s.cid_breiten.insert(c, breite);
                        }
                        i += 3;
                    }
                    _ => break,
                }
            }
        }
        // Kodierung einfacher Schriften
        match o.get("Encoding").map(|v| pdf.aufloesen(v)) {
            Some(Obj::Name(n)) => s.kodierung = kodierung_basis(&n),
            Some(e @ Obj::Dict(_)) => {
                if let Some(b) = e.get("BaseEncoding").and_then(Obj::name) {
                    s.kodierung = kodierung_basis(b);
                }
                let mut code = 0u32;
                for v in pdf.feld_von(e.get("Differences")) {
                    match v {
                        Obj::Zahl(z) => code = z as u32,
                        Obj::Name(n) => {
                            if let Some(c) = glyphname(&n)
                                && (code as usize) < 256
                            {
                                s.kodierung[code as usize] = c;
                            }
                            code += 1;
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                if !zwei_byte {
                    s.kodierung = kodierung_basis(b"WinAnsiEncoding");
                }
            }
        }
        if let Some(tu) = o.get("ToUnicode").map(|v| pdf.aufloesen(v))
            && let Ok(d) = pdf.strom_daten(&tu)
        {
            s.unicode = cmap_lesen(&d);
        }
        s
    }

    /// Codes eines Textes (1 oder 2 Byte).
    fn codes(&self, t: &[u8]) -> Vec<u32> {
        if self.zwei_byte {
            t.chunks(2).map(|p| u32::from(p[0]) << 8 | u32::from(*p.get(1).unwrap_or(&0))).collect()
        } else {
            t.iter().map(|&b| u32::from(b)).collect()
        }
    }

    /// Breite eines Codes in Tausendstel der Schriftgrösse.
    fn breite(&self, c: u32) -> f64 {
        if self.zwei_byte {
            return *self.cid_breiten.get(&c).unwrap_or(&self.standard_breite);
        }
        let w = c.checked_sub(self.erste).and_then(|i| self.breiten.get(i as usize)).copied().unwrap_or(0.0);
        if w > 0.0 { w } else { 500.0 }
    }

    fn text(&self, c: u32) -> String {
        if let Some(u) = self.unicode.get(&c) {
            return u.clone();
        }
        if self.zwei_byte {
            return char::from_u32(c).filter(|ch| !ch.is_control()).map_or(String::new(), String::from);
        }
        let ch = self.kodierung[(c & 0xFF) as usize];
        if ch == '\0' { String::new() } else { ch.to_string() }
    }
}

/// ToUnicode-CMap: bfchar und bfrange.
fn cmap_lesen(d: &[u8]) -> BTreeMap<u32, String> {
    let mut m = BTreeMap::new();
    let mut l = Leser::neu(d, 0);
    let hex_zahl = |t: &[u8]| t.iter().fold(0u32, |a, &b| a << 8 | u32::from(b));
    let utf16 = |t: &[u8]| {
        let u: Vec<u16> = t.chunks(2).map(|p| u16::from(p[0]) << 8 | u16::from(*p.get(1).unwrap_or(&0))).collect();
        String::from_utf16_lossy(&u)
    };
    while let Some(w) = l.wort() {
        match w {
            Wort::Op(o) if o == b"beginbfchar" => loop {
                let merk = l.pos;
                match (l.objekt(), l.objekt()) {
                    (Some(Obj::Text(a)), Some(Obj::Text(b))) => {
                        m.insert(hex_zahl(&a), utf16(&b));
                    }
                    _ => {
                        l.pos = merk;
                        break;
                    }
                }
            },
            Wort::Op(o) if o == b"beginbfrange" => loop {
                let merk = l.pos;
                let (Some(Obj::Text(a)), Some(Obj::Text(b))) = (l.objekt(), l.objekt()) else {
                    l.pos = merk;
                    break;
                };
                let (a, b) = (hex_zahl(&a), hex_zahl(&b));
                match l.objekt() {
                    Some(Obj::Text(ziel)) => {
                        let mut u: Vec<u16> = ziel.chunks(2).map(|p| u16::from(p[0]) << 8 | u16::from(*p.get(1).unwrap_or(&0))).collect();
                        let platz = MAX_ZUORDNUNG.saturating_sub(m.len()) as u32;
                        for c in a..=b.min(a.saturating_add(platz.min(0xFFFF))) {
                            m.insert(c, String::from_utf16_lossy(&u));
                            if let Some(x) = u.last_mut() {
                                *x = x.wrapping_add(1);
                            }
                        }
                    }
                    Some(Obj::Feld(f)) => {
                        for (k, z) in f.iter().enumerate().take(MAX_ZUORDNUNG.saturating_sub(m.len())) {
                            if let Obj::Text(t) = z {
                                m.insert(a.saturating_add(k as u32), utf16(t));
                            }
                        }
                    }
                    _ => break,
                }
            },
            _ => {}
        }
    }
    m
}

/// Grundkodierungen (WinAnsi, MacRoman, Standard): Zeichen je Code.
fn kodierung_basis(name: &[u8]) -> [char; 256] {
    let mut k = ['\0'; 256];
    for (i, c) in k.iter_mut().enumerate().take(127).skip(32) {
        *c = i as u8 as char;
    }
    match name {
        b"WinAnsiEncoding" => {
            const W: [u16; 32] = [
                0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C,
                0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
            ];
            for (i, &u) in W.iter().enumerate() {
                k[128 + i] = char::from_u32(u32::from(u)).unwrap_or('\0');
            }
            for (i, c) in k.iter_mut().enumerate().skip(160) {
                *c = char::from_u32(i as u32).unwrap_or('\0');
            }
        }
        b"MacRomanEncoding" => {
            const M: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»… ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{F8FF}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";
            for (i, c) in M.chars().enumerate().take(128) {
                k[128 + i] = c;
            }
        }
        _ => {
            // Standard: Anführungszeichen wie im Adobe-Satz
            k[0x27] = '\u{2019}';
            k[0x60] = '\u{2018}';
            for (code, c) in [(0xA1, '¡'), (0xA2, '¢'), (0xA3, '£'), (0xA5, '¥'), (0xA7, '§'), (0xAA, '“'), (0xAB, '«'), (0xB1, '–'), (0xB7, '•'), (0xBA, '”'), (0xBB, '»'), (0xD0, '—'), (0xE1, 'Æ'), (0xF1, 'æ'), (0xFB, 'ß')] {
                k[code] = c;
            }
        }
    }
    k
}

/// Häufige Glyphnamen (Differences) → Zeichen; „uniXXXX“ und „gXX“-artige
/// Namen ohne Zuordnung bleiben leer.
fn glyphname(n: &[u8]) -> Option<char> {
    if n.len() == 1 && n[0].is_ascii_alphabetic() {
        return Some(n[0] as char);
    }
    if let Some(h) = n.strip_prefix(b"uni")
        && h.len() == 4
    {
        return u32::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok().and_then(char::from_u32);
    }
    const NAMEN: &[(&str, char)] = &[
        ("space", ' '), ("exclam", '!'), ("quotedbl", '"'), ("numbersign", '#'), ("dollar", '$'), ("percent", '%'), ("ampersand", '&'),
        ("quoteright", '\u{2019}'), ("quotesingle", '\''), ("parenleft", '('), ("parenright", ')'), ("asterisk", '*'), ("plus", '+'), ("comma", ','),
        ("hyphen", '-'), ("period", '.'), ("slash", '/'), ("zero", '0'), ("one", '1'), ("two", '2'), ("three", '3'), ("four", '4'), ("five", '5'),
        ("six", '6'), ("seven", '7'), ("eight", '8'), ("nine", '9'), ("colon", ':'), ("semicolon", ';'), ("less", '<'), ("equal", '='),
        ("greater", '>'), ("question", '?'), ("at", '@'), ("bracketleft", '['), ("backslash", '\\'), ("bracketright", ']'), ("underscore", '_'),
        ("quoteleft", '\u{2018}'), ("braceleft", '{'), ("bar", '|'), ("braceright", '}'), ("asciitilde", '~'), ("adieresis", 'ä'), ("odieresis", 'ö'),
        ("udieresis", 'ü'), ("Adieresis", 'Ä'), ("Odieresis", 'Ö'), ("Udieresis", 'Ü'), ("germandbls", 'ß'), ("eacute", 'é'), ("egrave", 'è'),
        ("agrave", 'à'), ("ccedilla", 'ç'), ("endash", '–'), ("emdash", '—'), ("bullet", '•'), ("quotedblleft", '“'), ("quotedblright", '”'),
        ("quotedblbase", '„'), ("ellipsis", '…'), ("fi", 'ﬁ'), ("fl", 'ﬂ'), ("Euro", '€'), ("degree", '°'), ("section", '§'), ("copyright", '©'),
        ("registered", '®'), ("trademark", '™'), ("minus", '−'), ("multiply", '×'), ("guillemotleft", '«'), ("guillemotright", '»'),
    ];
    let s = std::str::from_utf8(n).ok()?;
    NAMEN.iter().find(|(k, _)| *k == s).map(|(_, c)| *c)
}

// ------------------------------------------------------------ Zeichnen

/// Ziel einer Seite: RGBA-Bildpunkte (0xAARRGGBB wie `kolibri::dekoder`).
pub struct Leinwand {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

/// Abbildung (a b c d e f) wie in PDF: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Clone, Copy, Debug)]
struct M([f64; 6]);

impl M {
    const EINS: M = M([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    fn mal(self, o: M) -> M {
        // self danach o (Zeilenvektor · self · o)
        let [a, b, c, d, e, f] = self.0;
        let [p, q, r, s, t, u] = o.0;
        M([a * p + b * r, a * q + b * s, c * p + d * r, c * q + d * s, e * p + f * r + t, e * q + f * s + u])
    }
    fn punkt(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
    fn skala(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        ((a * d - b * c).abs()).sqrt()
    }
    fn invers(&self) -> Option<M> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return None;
        }
        Some(M([d / det, -b / det, -c / det, a / det, (c * f - d * e) / det, (b * e - a * f) / det]))
    }
}

#[derive(Clone)]
struct Zustand {
    ctm: M,
    fuellfarbe: u32,
    strichfarbe: u32,
    fuell_raum: Raum,
    strich_raum: Raum,
    strichbreite: f64,
    /// Strichmuster (Längen im Benutzerraum, an/aus im Wechsel) und Anfang
    muster: Vec<f64>,
    phase: f64,
    /// Deckkraft beim Füllen (auch Text, Bilder) und Ziehen, 0..1 (ca/CA)
    deck_fuell: f64,
    deck_strich: f64,
    // Text
    schrift: Option<usize>,
    groesse: f64,
    zeichenabstand: f64,
    wortabstand: f64,
    hskala: f64,
    zeilenabstand: f64,
    hebung: f64,
    textmodus: i32,
}

/// Gesammelter Text einer Seite (für Suche/Kopieren und Tests).
#[derive(Default, Debug)]
pub struct TextStueck {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

/// Glyphe (8×16-Bitmap, 16 Zeilen) für ein Zeichen – vom Aufrufer
/// (Systemschrift), damit pdf.rs ohne Kern testbar bleibt.
pub type GlyphenQuelle<'a> = &'a dyn Fn(char) -> [u8; 16];

struct Zeichner<'a> {
    pdf: &'a Pdf,
    l: &'a mut Leinwand,
    schriften: Vec<(Vec<u8>, Schrift)>,
    /// Schriften ab hier gehören zum laufenden Formular (gleiche Namen,
    /// andere Ressourcen); Indizes davor bleiben gültig
    schrift_basis: usize,
    glyphe: GlyphenQuelle<'a>,
    pub texte: Vec<TextStueck>,
    tiefe: u32,
    /// Operatoren der ganzen Seite (auch in Formularen)
    ops: u32,
}

/// Höchstens so viele Operatoren je Seite, so tiefer q-Stapel.
const MAX_OPS: u32 = 2_000_000;
const MAX_STAPEL: usize = 64;

/// Seite `i` mit `skala` Bildpunkten je Punkt zeichnen; liefert den Text.
pub fn seite_zeichnen(pdf: &Pdf, i: usize, skala: f64, glyphe: GlyphenQuelle<'_>) -> Ergebnis<(Leinwand, Vec<TextStueck>)> {
    let s = pdf.seiten.get(i).ok_or("keine solche Seite")?;
    let [x0, y0, x1, y1] = s.kasten;
    let (bw, bh) = ((x1 - x0) * skala, (y1 - y0) * skala);
    let quer = s.drehung == 90 || s.drehung == 270;
    let (w, h) = if quer { (bh, bw) } else { (bw, bh) };
    let (w, h) = (w.round().clamp(1.0, 4000.0) as usize, h.round().clamp(1.0, 4000.0) as usize);
    if !speicher_reicht((w * h * 4) as u64) {
        return Err(format!("{} ({w}×{h})", t("not enough memory for the page", "zu wenig Speicher für die Seite")));
    }
    let mut l = Leinwand { w, h, px: vec![0xFFFF_FFFF; w * h] };
    // Seite → Bildpunkte: verschieben, skalieren, y umdrehen, drehen.
    // /Rotate dreht im Uhrzeigersinn: bei 90 wird die linke Kante oben,
    // (u, v) → (w − v, u); bei 270 (u, v) → (v, h − u).
    let basis = M([skala, 0.0, 0.0, -skala, -x0 * skala, y1 * skala]);
    let dreh = match s.drehung {
        90 => M([0.0, 1.0, -1.0, 0.0, w as f64, 0.0]),
        180 => M([-1.0, 0.0, 0.0, -1.0, w as f64, h as f64]),
        270 => M([0.0, -1.0, 1.0, 0.0, 0.0, h as f64]),
        _ => M::EINS,
    };
    let start = Zustand {
        ctm: basis.mal(dreh),
        fuellfarbe: 0xFF00_0000,
        strichfarbe: 0xFF00_0000,
        fuell_raum: Raum::Grau,
        strich_raum: Raum::Grau,
        strichbreite: 1.0,
        muster: Vec::new(),
        phase: 0.0,
        deck_fuell: 1.0,
        deck_strich: 1.0,
        schrift: None,
        groesse: 12.0,
        zeichenabstand: 0.0,
        wortabstand: 0.0,
        hskala: 1.0,
        zeilenabstand: 0.0,
        hebung: 0.0,
        textmodus: 0,
    };
    let mut z = Zeichner { pdf, l: &mut l, schriften: Vec::new(), schrift_basis: 0, glyphe, texte: Vec::new(), tiefe: 0, ops: 0 };
    let mut inhalt = Vec::new();
    for c in &s.inhalt {
        if let Ok(d) = pdf.strom_daten(&pdf.aufloesen(c)) {
            inhalt.extend_from_slice(&d);
            inhalt.push(b'\n');
        }
    }
    let res = pdf.aufloesen(&s.ressourcen);
    z.inhalt(&inhalt, &res, start);
    let texte = std::mem::take(&mut z.texte);
    Ok((l, texte))
}

impl Zeichner<'_> {
    fn inhalt(&mut self, d: &[u8], res: &Obj, start: Zustand) {
        if self.tiefe > 6 {
            return;
        }
        self.tiefe += 1;
        let mut gs = start;
        let mut stapel: Vec<Zustand> = Vec::new();
        let mut args: Vec<Obj> = Vec::new();
        let mut pfad: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut cur = (0.0, 0.0);
        let (mut tm, mut tlm) = (M::EINS, M::EINS);
        let mut l = Leser::neu(d, 0);
        let mut zu_tief = 0u32;
        while let Some(w) = l.wort() {
            let op = match w {
                Wort::Op(o) => o,
                w => {
                    if let Some(o) = l.aus_wort_pub(w) {
                        args.push(o);
                    }
                    continue;
                }
            };
            self.ops += 1;
            if self.ops > MAX_OPS {
                break;
            }
            let n = |i: usize| args.get(i).and_then(Obj::zahl).unwrap_or(0.0);
            let punkt = |x: f64, y: f64, g: &Zustand| g.ctm.punkt(x, y);
            match op.as_slice() {
                // zu tiefes q: nur zählen (das passende Q stellt nichts her)
                b"q" if stapel.len() >= MAX_STAPEL => zu_tief += 1,
                b"q" => stapel.push(gs.clone()),
                b"Q" if zu_tief > 0 => zu_tief -= 1,
                b"Q" => {
                    if let Some(z) = stapel.pop() {
                        gs = z;
                    }
                }
                b"cm" => gs.ctm = M([n(0), n(1), n(2), n(3), n(4), n(5)]).mal(gs.ctm),
                b"w" => gs.strichbreite = n(0),
                b"d" => {
                    gs.muster = args.first().and_then(Obj::feld).map(|f| f.iter().filter_map(Obj::zahl).collect()).unwrap_or_default();
                    gs.phase = n(1);
                }
                b"gs" => {
                    let e = args.first().and_then(Obj::name).and_then(|name| {
                        res.get("ExtGState").map(|v| self.pdf.aufloesen(v)).and_then(|v| v.dict().and_then(|d| d.get(name).cloned()))
                    });
                    if let Some(e) = e {
                        let e = self.pdf.aufloesen(&e);
                        self.grafikzustand(&e, &mut gs);
                    }
                }
                b"g" => (gs.fuell_raum, gs.fuellfarbe) = (Raum::Grau, grau(n(0))),
                b"G" => (gs.strich_raum, gs.strichfarbe) = (Raum::Grau, grau(n(0))),
                b"rg" => (gs.fuell_raum, gs.fuellfarbe) = (Raum::Rgb, rgb(n(0), n(1), n(2))),
                b"RG" => (gs.strich_raum, gs.strichfarbe) = (Raum::Rgb, rgb(n(0), n(1), n(2))),
                b"k" => (gs.fuell_raum, gs.fuellfarbe) = (Raum::Cmyk, cmyk(n(0), n(1), n(2), n(3))),
                b"K" => (gs.strich_raum, gs.strichfarbe) = (Raum::Cmyk, cmyk(n(0), n(1), n(2), n(3))),
                b"sc" | b"scn" | b"SC" | b"SCN" => {
                    let z: Vec<f64> = args.iter().filter_map(Obj::zahl).collect();
                    let raum = if op[0] == b's' { &gs.fuell_raum } else { &gs.strich_raum };
                    // Muster (/P1 scn) werden nicht gezeichnet: Grau
                    let f = if z.is_empty() { Raum::Muster.farbe(&z) } else { raum.farbe(&z) };
                    if op[0] == b's' {
                        gs.fuellfarbe = f;
                    } else {
                        gs.strichfarbe = f;
                    }
                }
                b"cs" | b"CS" => {
                    let raum = args.first().map_or(Raum::Grau, |name| self.raum_name(res, name));
                    if op == b"cs" {
                        gs.fuellfarbe = raum.anfang();
                        gs.fuell_raum = raum;
                    } else {
                        gs.strichfarbe = raum.anfang();
                        gs.strich_raum = raum;
                    }
                }
                b"m" => {
                    cur = punkt(n(0), n(1), &gs);
                    pfad.push(vec![cur]);
                }
                b"l" => {
                    cur = punkt(n(0), n(1), &gs);
                    if let Some(t) = pfad.last_mut() {
                        t.push(cur);
                    } else {
                        pfad.push(vec![cur]);
                    }
                }
                b"c" | b"v" | b"y" => {
                    let (p1, p2, p3) = match op.as_slice() {
                        b"c" => (punkt(n(0), n(1), &gs), punkt(n(2), n(3), &gs), punkt(n(4), n(5), &gs)),
                        b"v" => (cur, punkt(n(0), n(1), &gs), punkt(n(2), n(3), &gs)),
                        _ => (punkt(n(0), n(1), &gs), punkt(n(2), n(3), &gs), punkt(n(2), n(3), &gs)),
                    };
                    let p0 = cur;
                    let t = pfad.last_mut();
                    if let Some(t) = t {
                        for k in 1..=12 {
                            let s = k as f64 / 12.0;
                            let u = 1.0 - s;
                            t.push((
                                u * u * u * p0.0 + 3.0 * u * u * s * p1.0 + 3.0 * u * s * s * p2.0 + s * s * s * p3.0,
                                u * u * u * p0.1 + 3.0 * u * u * s * p1.1 + 3.0 * u * s * s * p2.1 + s * s * s * p3.1,
                            ));
                        }
                    }
                    cur = p3;
                }
                b"h" => {
                    if let Some(t) = pfad.last_mut()
                        && let Some(&a) = t.first()
                    {
                        t.push(a);
                    }
                }
                b"re" => {
                    let (x, y, w, h) = (n(0), n(1), n(2), n(3));
                    pfad.push(vec![punkt(x, y, &gs), punkt(x + w, y, &gs), punkt(x + w, y + h, &gs), punkt(x, y + h, &gs), punkt(x, y, &gs)]);
                    cur = punkt(x, y, &gs);
                }
                b"f" | b"F" | b"f*" => {
                    fuellen(self.l, &pfad, mit_deckkraft(gs.fuellfarbe, gs.deck_fuell), op == b"f*");
                    pfad.clear();
                }
                b"B" | b"B*" | b"b" | b"b*" => {
                    fuellen(self.l, &pfad, mit_deckkraft(gs.fuellfarbe, gs.deck_fuell), op.ends_with(b"*"));
                    self.strich(&pfad, &gs);
                    pfad.clear();
                }
                b"S" | b"s" => {
                    self.strich(&pfad, &gs);
                    pfad.clear();
                }
                b"n" => pfad.clear(),
                // Text
                b"BT" => {
                    tm = M::EINS;
                    tlm = M::EINS;
                }
                b"Tf" => {
                    let name = args.first().and_then(Obj::name).unwrap_or(b"").to_vec();
                    gs.schrift = Some(self.schrift_holen(res, &name));
                    gs.groesse = n(1);
                }
                b"Tc" => gs.zeichenabstand = n(0),
                b"Tw" => gs.wortabstand = n(0),
                b"Tz" => gs.hskala = n(0) / 100.0,
                b"TL" => gs.zeilenabstand = n(0),
                b"Ts" => gs.hebung = n(0),
                b"Tr" => gs.textmodus = n(0) as i32,
                b"Td" => {
                    tlm = M([1.0, 0.0, 0.0, 1.0, n(0), n(1)]).mal(tlm);
                    tm = tlm;
                }
                b"TD" => {
                    gs.zeilenabstand = -n(1);
                    tlm = M([1.0, 0.0, 0.0, 1.0, n(0), n(1)]).mal(tlm);
                    tm = tlm;
                }
                b"Tm" => {
                    tlm = M([n(0), n(1), n(2), n(3), n(4), n(5)]);
                    tm = tlm;
                }
                b"T*" => {
                    tlm = M([1.0, 0.0, 0.0, 1.0, 0.0, -gs.zeilenabstand]).mal(tlm);
                    tm = tlm;
                }
                b"Tj" | b"'" | b"\"" => {
                    if op == b"'" || op == b"\"" {
                        if op == b"\"" {
                            gs.wortabstand = n(0);
                            gs.zeichenabstand = n(1);
                        }
                        tlm = M([1.0, 0.0, 0.0, 1.0, 0.0, -gs.zeilenabstand]).mal(tlm);
                        tm = tlm;
                    }
                    if let Some(Obj::Text(t)) = args.last() {
                        let t = t.clone();
                        self.text(&t, &gs, &mut tm);
                    }
                }
                b"TJ" => {
                    if let Some(Obj::Feld(f)) = args.first() {
                        let f = f.clone();
                        for e in f {
                            match e {
                                Obj::Text(t) => self.text(&t, &gs, &mut tm),
                                Obj::Zahl(z) => {
                                    let dx = -z / 1000.0 * gs.groesse * gs.hskala;
                                    tm = M([1.0, 0.0, 0.0, 1.0, dx, 0.0]).mal(tm);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                b"Do" => {
                    if let Some(name) = args.first().and_then(Obj::name) {
                        let x = res.get("XObject").map(|v| self.pdf.aufloesen(v)).and_then(|v| v.dict().and_then(|d| d.get(name).cloned()));
                        if let Some(x) = x {
                            self.xobjekt(&self.pdf.aufloesen(&x), res, &gs);
                        }
                    }
                }
                b"BI" => self.inline_bild(d, &mut l, res, &gs),
                _ => {}
            }
            args.clear();
        }
        self.tiefe -= 1;
    }

    fn schrift_holen(&mut self, res: &Obj, name: &[u8]) -> usize {
        if let Some(i) = self.schriften[self.schrift_basis..].iter().position(|(n, _)| n == name) {
            return self.schrift_basis + i;
        }
        let o = res.get("Font").map(|v| self.pdf.aufloesen(v)).and_then(|v| v.dict().and_then(|d| d.get(name).cloned())).unwrap_or(Obj::Null);
        self.schriften.push((name.to_vec(), Schrift::laden(self.pdf, &o)));
        self.schriften.len() - 1
    }

    fn text(&mut self, t: &[u8], gs: &Zustand, tm: &mut M) {
        let Some(si) = gs.schrift else { return };
        let codes = self.schriften[si].1.codes(t);
        let mut stueck = TextStueck::default();
        for c in codes {
            let (zeichen, breite) = {
                let s = &self.schriften[si].1;
                (s.text(c), s.breite(c))
            };
            // Textraum → Seite: Tm · CTM, Grösse und Hebung
            let trm = M([gs.groesse * gs.hskala, 0.0, 0.0, gs.groesse, 0.0, gs.hebung]).mal(*tm).mal(gs.ctm);
            let (gx, gy) = trm.punkt(0.0, 0.0);
            if stueck.text.is_empty() {
                stueck.x = gx;
                stueck.y = gy;
            }
            stueck.text.push_str(&zeichen);
            // Tr 3/7 unsichtbar (z. B. OCR-Ebene), 1/5 nur gezogen (Strichfarbe),
            // sonst gefüllt
            let farbe = match gs.textmodus {
                3 | 7 => None,
                1 | 5 => Some(mit_deckkraft(gs.strichfarbe, gs.deck_strich)),
                _ => Some(mit_deckkraft(gs.fuellfarbe, gs.deck_fuell)),
            };
            if let Some(farbe) = farbe {
                for ch in zeichen.chars() {
                    glyphe_zeichnen(self.l, (self.glyphe)(ch), &trm, breite / 1000.0, farbe);
                }
            }
            let wort = if !self.schriften[si].1.zwei_byte && c == 32 { gs.wortabstand } else { 0.0 };
            let dx = (breite / 1000.0 * gs.groesse + gs.zeichenabstand + wort) * gs.hskala;
            *tm = M([1.0, 0.0, 0.0, 1.0, dx, 0.0]).mal(*tm);
        }
        if !stueck.text.is_empty() {
            self.texte.push(stueck);
        }
    }

    fn xobjekt(&mut self, x: &Obj, res: &Obj, gs: &Zustand) {
        let art = x.get("Subtype").and_then(Obj::name).unwrap_or(b"");
        if art == b"Form" {
            let Ok(d) = self.pdf.strom_daten(x) else { return };
            let mut g = gs.clone();
            if let Some(m) = x.get("Matrix").and_then(Obj::feld)
                && m.len() == 6
            {
                let z: Vec<f64> = m.iter().map(|v| v.zahl().unwrap_or(0.0)).collect();
                g.ctm = M([z[0], z[1], z[2], z[3], z[4], z[5]]).mal(g.ctm);
            }
            let eigene = x.get("Resources").map(|v| self.pdf.aufloesen(v));
            let r = eigene.unwrap_or_else(|| res.clone());
            // Schriften des Formulars eigens (gleiche Namen, andere Schrift);
            // die geerbte Schrift (gs.schrift) zeigt weiter auf ihren Eintrag
            let basis = std::mem::replace(&mut self.schrift_basis, self.schriften.len());
            self.inhalt(&d, &r, g);
            self.schriften.truncate(self.schrift_basis);
            self.schrift_basis = basis;
            return;
        }
        if art != b"Image" {
            return;
        }
        match bild_dekodieren(self.pdf, x, gs.fuellfarbe) {
            Some(b) => bild_zeichnen(self.l, &b, &gs.ctm, gs.deck_fuell),
            None => bild_ersatz(self.l, &gs.ctm),
        }
    }

    /// Pfad ziehen mit Breite, Strichmuster und Deckkraft aus `gs`.
    fn strich(&mut self, pfad: &[Vec<(f64, f64)>], gs: &Zustand) {
        let k = gs.ctm.skala();
        let breite = (gs.strichbreite * k).max(1.0);
        let farbe = mit_deckkraft(gs.strichfarbe, gs.deck_strich);
        if gs.muster.is_empty() {
            ziehen(self.l, pfad, farbe, breite);
        } else {
            let muster: Vec<f64> = gs.muster.iter().map(|m| m * k).collect();
            ziehen(self.l, &stricheln(pfad, &muster, gs.phase * k), farbe, breite);
        }
    }

    /// Farbraum für `cs`/`CS`: Gerätename oder Name in /ColorSpace der
    /// Ressourcen.
    fn raum_name(&self, res: &Obj, name: &Obj) -> Raum {
        match name.name() {
            Some(b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Pattern" | b"CalRGB" | b"CalGray") => self.pdf.raum(name, 0),
            Some(n) => res
                .get("ColorSpace")
                .map(|v| self.pdf.aufloesen(v))
                .and_then(|v| v.dict().and_then(|d| d.get(n).cloned()))
                .map_or(Raum::Grau, |cs| self.pdf.raum(&cs, 0)),
            None => self.pdf.raum(name, 0),
        }
    }

    /// Grafikzustand aus /ExtGState: Linienbreite, Strichmuster, Schrift,
    /// Deckkraft.
    fn grafikzustand(&mut self, e: &Obj, gs: &mut Zustand) {
        let pdf = self.pdf;
        let wert = |k: &str| e.get(k).map(|v| pdf.aufloesen(v));
        if let Some(w) = wert("LW").and_then(|v| v.zahl()) {
            gs.strichbreite = w;
        }
        if let Some(a) = wert("ca").and_then(|v| v.zahl()) {
            gs.deck_fuell = a.clamp(0.0, 1.0);
        }
        if let Some(a) = wert("CA").and_then(|v| v.zahl()) {
            gs.deck_strich = a.clamp(0.0, 1.0);
        }
        if let Some(Obj::Feld(d)) = wert("D") {
            gs.muster = d.first().map(|v| pdf.aufloesen(v)).and_then(|v| v.feld().map(|m| m.iter().filter_map(Obj::zahl).collect())).unwrap_or_default();
            gs.phase = d.get(1).and_then(Obj::zahl).unwrap_or(0.0);
        }
        if let Some(Obj::Feld(f)) = wert("Font")
            && let (Some(Obj::Ref(nr, generation)), Some(groesse)) = (f.first(), f.get(1).and_then(Obj::zahl))
        {
            let schluessel = format!("\0gs {nr} {generation}").into_bytes();
            let i = match self.schriften[self.schrift_basis..].iter().position(|(n, _)| *n == schluessel) {
                Some(i) => self.schrift_basis + i,
                None => {
                    self.schriften.push((schluessel, Schrift::laden(pdf, &pdf.aufloesen(&Obj::Ref(*nr, *generation)))));
                    self.schriften.len() - 1
                }
            };
            gs.schrift = Some(i);
            gs.groesse = groesse;
        }
    }

    /// Eingebettetes Bild `BI <Paare> ID <Daten> EI` (PDF 8.9.7, mit den
    /// Abkürzungen der Schlüssel und Namen).
    fn inline_bild(&mut self, d: &[u8], l: &mut Leser, res: &Obj, gs: &Zustand) {
        let mut dict = BTreeMap::new();
        loop {
            match l.wort() {
                Some(Wort::Op(o)) if o == b"ID" => break,
                Some(w) => {
                    let Some(Obj::Name(k)) = l.aus_wort_pub(w) else { return };
                    let Some(v) = l.objekt() else { return };
                    dict.insert(abk_schluessel(&k).to_vec(), abk_wert(&v));
                }
                None => return,
            }
        }
        // Farbraum als Name aus den Ressourcen
        if let Some(Obj::Name(n)) = dict.get(b"ColorSpace".as_slice())
            && !matches!(n.as_slice(), b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Indexed")
            && let Some(cs) = res.get("ColorSpace").map(|v| self.pdf.aufloesen(v)).and_then(|v| v.dict().and_then(|c| c.get(n.as_slice()).cloned()))
        {
            dict.insert(b"ColorSpace".to_vec(), cs);
        }
        // Daten ab dem einen Leerzeichen nach „ID“; ohne Filter feste Länge,
        // sonst bis „EI“ zwischen Leerraum
        let start = (l.pos + 1).min(d.len());
        let o = Obj::Dict(dict);
        let zahl = |k: &str| o.get(k).and_then(Obj::zahl).unwrap_or(0.0).max(0.0);
        let ende = if o.get("Filter").is_none() {
            let maske = o.get("ImageMask") == Some(&Obj::Bool(true));
            let bpc = if maske { 1.0 } else { zahl("BitsPerComponent").max(1.0) };
            let n = if maske { 1 } else { self.pdf.raum(o.get("ColorSpace").unwrap_or(&Obj::Name(b"DeviceGray".to_vec())), 0).komponenten() };
            let zeile = (zahl("Width") * n as f64 * bpc / 8.0).ceil() as usize;
            zeile.checked_mul(zahl("Height") as usize).and_then(|n| start.checked_add(n)).map_or(d.len(), |e| e.min(d.len()))
        } else {
            (start..d.len().saturating_sub(1))
                .find(|&i| ist_leer(d[i]) && d[i + 1..].starts_with(b"EI") && d.get(i + 3).is_none_or(|&c| ist_leer(c)))
                .unwrap_or(d.len())
        };
        // hinter „EI“ weiterlesen
        l.pos = d[ende..].windows(2).position(|w| w == b"EI").map_or(d.len(), |p| ende + p + 2);
        let Obj::Dict(dict) = o else { return };
        let x = Obj::Strom(dict, d[start..ende].to_vec());
        match bild_dekodieren(self.pdf, &x, gs.fuellfarbe) {
            Some(b) => bild_zeichnen(self.l, &b, &gs.ctm, gs.deck_fuell),
            None => bild_ersatz(self.l, &gs.ctm),
        }
    }
}

/// Abgekürzte Schlüssel eingebetteter Bilder (PDF Tabelle 91).
fn abk_schluessel(k: &[u8]) -> &[u8] {
    match k {
        b"BPC" => b"BitsPerComponent",
        b"CS" => b"ColorSpace",
        b"D" => b"Decode",
        b"DP" => b"DecodeParms",
        b"F" => b"Filter",
        b"H" => b"Height",
        b"IM" => b"ImageMask",
        b"I" => b"Interpolate",
        b"W" => b"Width",
        b"L" => b"Length",
        _ => k,
    }
}

/// Abgekürzte Namen eingebetteter Bilder (Farbräume, Filter; Tabelle 92).
fn abk_wert(v: &Obj) -> Obj {
    match v {
        Obj::Name(n) => Obj::Name(
            match n.as_slice() {
                b"G" => b"DeviceGray".as_slice(),
                b"RGB" => b"DeviceRGB",
                b"CMYK" => b"DeviceCMYK",
                b"I" => b"Indexed",
                b"AHx" => b"ASCIIHexDecode",
                b"A85" => b"ASCII85Decode",
                b"LZW" => b"LZWDecode",
                b"Fl" => b"FlateDecode",
                b"RL" => b"RunLengthDecode",
                b"CCF" => b"CCITTFaxDecode",
                b"DCT" => b"DCTDecode",
                n => n,
            }
            .to_vec(),
        ),
        Obj::Feld(f) => Obj::Feld(f.iter().map(abk_wert).collect()),
        o => o.clone(),
    }
}

/// Farbe mit Deckkraft `a` (0..1) im obersten Byte.
fn mit_deckkraft(farbe: u32, a: f64) -> u32 {
    (farbe & 0xFF_FFFF) | ((a.clamp(0.0, 1.0) * 255.0).round() as u32) << 24
}

/// Bildpunkt mit `farbe` übermalen (oberstes Byte = Deckkraft).
fn malen(d: &mut u32, farbe: u32) {
    let a = farbe >> 24;
    if a >= 255 {
        *d = farbe;
    } else if a > 0 {
        let m = |s: u32| ((farbe >> s & 255) * a + (*d >> s & 255) * (255 - a)) / 255;
        *d = 0xFF00_0000 | m(16) << 16 | m(8) << 8 | m(0);
    }
}

/// Pfad nach Strichmuster (Längen in Bildpunkten, an/aus im Wechsel) in
/// sichtbare Stücke teilen; jeder Teilpfad beginnt das Muster neu bei
/// `phase` (PDF 8.4.3.6). Ungerade Länge: Muster doppelt.
fn stricheln(pfad: &[Vec<(f64, f64)>], muster: &[f64], phase: f64) -> Vec<Vec<(f64, f64)>> {
    let m: Vec<f64> = if muster.len() % 2 == 1 { muster.iter().chain(muster).copied().collect() } else { muster.to_vec() };
    let summe: f64 = m.iter().sum();
    // negatives oder winziges Muster (unter einem Bildpunkt): durchgezogen
    if m.iter().any(|v| *v < 0.0) || summe < 1.0 {
        return pfad.to_vec();
    }
    let mut aus = Vec::new();
    for t in pfad {
        let mut p = phase.rem_euclid(summe);
        let mut i = 0;
        while p >= m[i] {
            p -= m[i];
            i = (i + 1) % m.len();
        }
        let mut rest = m[i] - p;
        let mut an = i % 2 == 0;
        let mut stueck: Vec<(f64, f64)> = Vec::new();
        if an && let Some(&a) = t.first() {
            stueck.push(a);
        }
        for s in t.windows(2) {
            let (a, b) = (s[0], s[1]);
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let mut pos = 0.0;
            while len - pos > 1e-9 {
                let schritt = rest.min(len - pos);
                pos += schritt;
                rest -= schritt;
                let q = (a.0 + (b.0 - a.0) * pos / len, a.1 + (b.1 - a.1) * pos / len);
                if an {
                    stueck.push(q);
                }
                if rest <= 1e-9 {
                    i = (i + 1) % m.len();
                    rest = m[i];
                    an = !an;
                    if an {
                        stueck = vec![q];
                    } else {
                        aus.push(std::mem::take(&mut stueck));
                        if aus.len() > 200_000 {
                            return aus;
                        }
                    }
                }
            }
        }
        if an && stueck.len() > 1 {
            aus.push(stueck);
        }
    }
    aus
}

fn grau(g: f64) -> u32 {
    let v = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
    0xFF00_0000 | v << 16 | v << 8 | v
}

fn rgb(r: f64, g: f64, b: f64) -> u32 {
    let k = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    0xFF00_0000 | k(r) << 16 | k(g) << 8 | k(b)
}

fn cmyk(c: f64, m: f64, y: f64, k: f64) -> u32 {
    rgb((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))
}

/// Pfad füllen (Mittelpunkt je Bildpunkt, Nonzero bzw. Even-Odd).
fn fuellen(l: &mut Leinwand, pfad: &[Vec<(f64, f64)>], farbe: u32, gerade_ungerade: bool) {
    // Kanten in f32 (der P4 hat nur dafür eine FPU): (unten, oben, x bei
    // unten, Steigung dx/dy, Richtung)
    let kanten: Vec<(f32, f32, f32, f32, i32)> = pfad
        .iter()
        .flat_map(|t| {
            let mut k: Vec<_> = t.windows(2).map(|p| (p[0], p[1])).collect();
            if let (Some(&a), Some(&b)) = (t.first(), t.last())
                && a != b
            {
                k.push((b, a));
            }
            k
        })
        .filter(|(a, b)| a.1 != b.1)
        .map(|(a, b)| {
            let (lo, hi, r) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
            (lo.1 as f32, hi.1 as f32, lo.0 as f32, ((hi.0 - lo.0) / (hi.1 - lo.1)) as f32, r)
        })
        .collect();
    if kanten.is_empty() {
        return;
    }
    let y0 = kanten.iter().map(|k| k.0).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let y1 = (kanten.iter().map(|k| k.1).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(l.h);
    let mut schnitte: Vec<(f32, i32)> = Vec::new();
    for y in y0..y1 {
        let yc = y as f32 + 0.5;
        schnitte.clear();
        for &(lo, hi, x0, steigung, r) in &kanten {
            if yc >= lo && yc < hi {
                schnitte.push((x0 + (yc - lo) * steigung, r));
            }
        }
        schnitte.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut wind = 0;
        for i in 0..schnitte.len() {
            wind = if gerade_ungerade { wind ^ 1 } else { wind + schnitte[i].1 };
            if wind != 0
                && let Some(n) = schnitte.get(i + 1)
            {
                let xa = (schnitte[i].0 + 0.5).floor().max(0.0) as usize;
                let xb = ((n.0 + 0.5).floor().max(0.0) as usize).min(l.w);
                if xa < xb {
                    let zeile = &mut l.px[y * l.w + xa..y * l.w + xb];
                    if farbe >> 24 >= 255 {
                        zeile.fill(farbe);
                    } else {
                        zeile.iter_mut().for_each(|p| malen(p, farbe));
                    }
                }
            }
        }
    }
}

/// Pfad ziehen: jede Strecke als Viereck der Breite `breite`.
fn ziehen(l: &mut Leinwand, pfad: &[Vec<(f64, f64)>], farbe: u32, breite: f64) {
    let h = breite / 2.0;
    for t in pfad {
        for p in t.windows(2) {
            let (a, b) = (p[0], p[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                continue;
            }
            let (nx, ny) = (-dy / len * h, dx / len * h);
            // Enden etwas verlängert, damit Ecken geschlossen wirken
            let (ex, ey) = (dx / len * h * 0.5, dy / len * h * 0.5);
            let q = vec![
                (a.0 + nx - ex, a.1 + ny - ey),
                (b.0 + nx + ex, b.1 + ny + ey),
                (b.0 - nx + ex, b.1 - ny + ey),
                (a.0 - nx - ex, a.1 - ny - ey),
                (a.0 + nx - ex, a.1 + ny - ey),
            ];
            // dünne Linien: wenigstens ein Bildpunkt breit
            if breite <= 1.5 {
                linie(l, a, b, farbe);
            } else {
                fuellen(l, &[q], farbe, false);
            }
        }
    }
}

fn linie(l: &mut Leinwand, a: (f64, f64), b: (f64, f64), farbe: u32) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).clamp(1, 20000);
    let (dx, dy) = (((b.0 - a.0) / n as f64) as f32, ((b.1 - a.1) / n as f64) as f32);
    let (mut x, mut y) = (a.0 as f32, a.1 as f32);
    for _ in 0..=n {
        if x >= 0.0 && y >= 0.0 && (x as usize) < l.w && (y as usize) < l.h {
            malen(&mut l.px[y as usize * l.w + x as usize], farbe);
        }
        x += dx;
        y += dy;
    }
}

/// Glyphe in die Zelle (0,0)…(Breite,1) des Textraums `trm` zeichnen:
/// geglättet (Abdeckung je Bildpunkt aus 4×4 Proben der 8×16-Bitmap).
fn glyphe_zeichnen(l: &mut Leinwand, g: [u8; 16], trm: &M, breite: f64, farbe: u32) {
    // Zelle: Grundlinie y=0, Oberkante ~0.8 (Versalhöhe), Unterlänge ~-0.2
    let zw = breite.clamp(0.25, 1.2);
    let ecken = [trm.punkt(0.0, -0.22), trm.punkt(zw, -0.22), trm.punkt(zw, 0.88), trm.punkt(0.0, 0.88)];
    let (x0, x1) = (ecken.iter().map(|p| p.0).fold(f64::MAX, f64::min), ecken.iter().map(|p| p.0).fold(f64::MIN, f64::max));
    let (y0, y1) = (ecken.iter().map(|p| p.1).fold(f64::MAX, f64::min), ecken.iter().map(|p| p.1).fold(f64::MIN, f64::max));
    if x1 < 0.0 || y1 < 0.0 || x0 >= l.w as f64 || y0 >= l.h as f64 || x1 - x0 > 2000.0 {
        return;
    }
    let Some(inv) = trm.invers() else { return };
    // Bitmap: 8 breit, 16 hoch; Zeile 0 oben. Bildpunkt (x, y) → Textraum
    // (u, v) = inv → Spalte u·8/zw, Zeile (0,88 − v)·16/1,1. Das ist affin:
    // in f32 mit festen Schritten je Probe (der P4 rechnet f64 in Software).
    let [a, b, c, d, e, f] = inv.0;
    let (ku, kv) = (8.0 / zw, 16.0 / 1.10);
    let (sa, sc, se) = ((a * ku) as f32, (c * ku) as f32, (e * ku) as f32);
    let (ta, tc, tf) = ((-b * kv) as f32, (-d * kv) as f32, ((0.88 - f) * kv) as f32);
    let mut proben = [(0f32, 0f32); 16];
    for (k, p) in proben.iter_mut().enumerate() {
        let (ox, oy) = (((k % 4) as f32 + 0.5) / 4.0, ((k / 4) as f32 + 0.5) / 4.0);
        *p = (sa * ox + sc * oy, ta * ox + tc * oy);
    }
    let (r, gr, b) = (farbe >> 16 & 255, farbe >> 8 & 255, farbe & 255);
    for py in y0.floor().max(0.0) as usize..(y1.ceil() as usize).min(l.h) {
        for px in x0.floor().max(0.0) as usize..(x1.ceil() as usize).min(l.w) {
            let (s0, t0) = (sa * px as f32 + sc * py as f32 + se, ta * px as f32 + tc * py as f32 + tf);
            let mut treffer = 0;
            for &(ds, dt) in &proben {
                let (s, t) = (s0 + ds, t0 + dt);
                // Systemschrift: Bit 0 ist der linke Bildpunkt
                if (0.0..8.0).contains(&s) && (0.0..16.0).contains(&t) && g[t as usize] >> (s as u32) & 1 == 1 {
                    treffer += 1;
                }
            }
            let a = treffer * 255 / 16 * (farbe >> 24) / 255;
            if a > 0 {
                let d = &mut l.px[py * l.w + px];
                let m = |q: u32, z: u32| (q * a + z * (255 - a)) / 255;
                *d = 0xFF00_0000 | m(r, *d >> 16 & 255) << 16 | m(gr, *d >> 8 & 255) << 8 | m(b, *d & 255);
            }
        }
    }
}

/// Bild-XObject oder eingebettetes Bild als RGBA: DCT über den Dekoder,
/// sonst Rohdaten im Farbraum (mit /Decode); Masken: ImageMask in der
/// Füllfarbe `fuell`, /Mask als Farbschlüssel oder Maskenbild, /SMask als
/// Deckkraft (mit /Matte). None: Filter fehlt (CCITT, JBIG2, JPEG 2000),
/// Daten kaputt oder zu wenig Speicher – dann zeichnet der Aufrufer
/// `bild_ersatz`.
fn bild_dekodieren(pdf: &Pdf, x: &Obj, fuell: u32) -> Option<kolibri::dekoder::Rgba> {
    let mut b = bild_roh(pdf, x, fuell, false)?;
    match (x.get("SMask").map(|v| pdf.aufloesen(v)), x.get("Mask").map(|v| pdf.aufloesen(v))) {
        (Some(sm @ Obj::Strom(..)), _) => {
            if let Some(m) = bild_roh(pdf, &sm, 0xFF00_0000, false) {
                let matte: Option<Vec<f64>> = sm.get("Matte").and_then(Obj::feld).map(|f| f.iter().filter_map(Obj::zahl).collect());
                let matte = matte.map(|m| match m.len() {
                    1 => grau(m[0]),
                    4 => cmyk(m[0], m[1], m[2], m[3]),
                    _ => rgb(m.first().copied().unwrap_or(0.0), m.get(1).copied().unwrap_or(0.0), m.get(2).copied().unwrap_or(0.0)),
                });
                maske_anwenden(&mut b, &m, |p| p & 255, matte);
            }
        }
        (_, Some(mk @ Obj::Strom(..))) => {
            // Maskenbild wie eine Schablone: gemalt = deckend
            if let Some(m) = bild_roh(pdf, &mk, 0xFF00_0000, true) {
                maske_anwenden(&mut b, &m, |p| p >> 24, None);
            }
        }
        _ => {}
    }
    Some(b)
}

/// Deckkraft aus dem Maskenbild `m` (nächster Nachbar bei anderer Grösse)
/// in `b` übernehmen; `matte`: Farbe, mit der die Bildfarben vorab
/// gemischt sind (PDF 11.6.5.3) – wird herausgerechnet.
fn maske_anwenden(b: &mut kolibri::dekoder::Rgba, m: &kolibri::dekoder::Rgba, deckkraft: impl Fn(u32) -> u32, matte: Option<u32>) {
    for y in 0..b.h {
        let my = (u64::from(y) * u64::from(m.h) / u64::from(b.h)) as u32;
        for x in 0..b.w {
            let mx = (u64::from(x) * u64::from(m.w) / u64::from(b.w)) as u32;
            let a = deckkraft(m.px[(my * m.w + mx) as usize]).min(255);
            let p = &mut b.px[(y * b.w + x) as usize];
            let mut c = *p;
            if let Some(mt) = matte
                && a > 0
            {
                let k = |s: u32| {
                    let (f, h) = ((c >> s & 255) as i32, (mt >> s & 255) as i32);
                    ((h + (f - h) * 255 / a as i32).clamp(0, 255) as u32) << s
                };
                c = (c & 0xFF00_0000) | k(16) | k(8) | k(0);
            }
            *p = (c & 0xFF_FFFF) | ((c >> 24) * a / 255) << 24;
        }
    }
}

/// Bilddaten ohne /SMask und /Mask-Bild (die braucht `bild_dekodieren`).
/// `als_maske`: wie ImageMask lesen (Maskenbild von /Mask).
fn bild_roh(pdf: &Pdf, x: &Obj, fuell: u32, als_maske: bool) -> Option<kolibri::dekoder::Rgba> {
    let zahl = |k: &str| x.get(k).map(|v| pdf.aufloesen(v)).and_then(|v| v.zahl());
    let (w, h) = (zahl("Width")?.max(0.0) as u32, zahl("Height")?.max(0.0) as u32);
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 16_000_000 {
        return None;
    }
    let filter = pdf.feld_von(x.get("Filter"));
    let hat = |namen: &[&[u8]]| filter.iter().any(|f| f.name().is_some_and(|n| namen.contains(&n)));
    if hat(&[b"JPXDecode", b"JBIG2Decode", b"CCITTFaxDecode", b"CCF"]) {
        return None;
    }
    let ist_dct = hat(&[b"DCTDecode", b"DCT"]);
    // Ergebnis 4 Byte je Bildpunkt; der JPEG-Dekoder braucht etwa ebenso
    // viel zum Arbeiten
    let bytes = u64::from(w) * u64::from(h) * 4;
    if !speicher_reicht(if ist_dct { bytes * 2 } else { bytes }) {
        return None;
    }
    let d = pdf.strom_daten(x).ok()?;
    let decode: Vec<f64> = x.get("Decode").map(|v| pdf.aufloesen(v)).and_then(|v| v.feld().map(|f| f.iter().filter_map(Obj::zahl).collect())).unwrap_or_default();
    // /Mask als Feld: Farbschlüssel (Bereiche der Rohwerte je Komponente)
    let schluessel: Vec<u32> = match x.get("Mask").map(|v| pdf.aufloesen(v)) {
        Some(Obj::Feld(f)) => f.iter().filter_map(|v| pdf.aufloesen(v).zahl()).map(|z| z.max(0.0) as u32).collect(),
        _ => Vec::new(),
    };
    if ist_dct {
        let mut b = kolibri::dekoder::dekodieren(&d).ok().and_then(|(_, mut v)| v.pop())?;
        // /Decode (z. B. [1 0] = negatives Graubild) auf die dekodierten
        // Werte; Grau: ein Paar für alle drei Kanäle. Bei CMYK-JPEGs liefert
        // der Dekoder schon RGB – dort nicht (Abweichung Z1).
        let komp = pdf.raum(x.get("ColorSpace").unwrap_or(&Obj::Name(b"DeviceRGB".to_vec())), 0).komponenten();
        if komp <= 3 && decode.len() >= 2 * komp && decode.chunks(2).take(komp).any(|p| p != [0.0, 1.0]) {
            let tab: Vec<[u8; 256]> = (0..3)
                .map(|k| {
                    let (a, z) = (decode[2 * k.min(komp - 1)], decode[2 * k.min(komp - 1) + 1]);
                    let mut t = [0u8; 256];
                    for (v, e) in t.iter_mut().enumerate() {
                        *e = ((a + v as f64 / 255.0 * (z - a)).clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                    t
                })
                .collect();
            for p in &mut b.px {
                let k = |s: u32, i: usize| u32::from(tab[i][(*p >> s & 255) as usize]) << s;
                *p = (*p & 0xFF00_0000) | k(16, 0) | k(8, 1) | k(0, 2);
            }
        }
        let n = schluessel.len() / 2;
        if n > 0 {
            for p in &mut b.px {
                let k = [*p >> 16 & 255, *p >> 8 & 255, *p & 255];
                let komp = |i: usize| if n == 1 { *p & 255 } else { k[i.min(2)] };
                if (0..n.min(3)).all(|i| schluessel[2 * i] <= komp(i) && komp(i) <= schluessel[2 * i + 1]) {
                    *p &= 0xFF_FFFF;
                }
            }
        }
        return Some(b);
    }
    let maske = als_maske || x.get("ImageMask") == Some(&Obj::Bool(true));
    let bpc = if maske { 1 } else { zahl("BitsPerComponent").unwrap_or(8.0) as usize };
    if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let raum = if maske { Raum::Grau } else { pdf.raum(x.get("ColorSpace").unwrap_or(&Obj::Name(b"DeviceGray".to_vec())), 0) };
    let n = raum.komponenten();
    let max = ((1u64 << bpc) - 1) as f64;
    // /Decode je Komponente; Standard [0 1], Lab [0 100 −100 100 −100 100],
    // Indexed [0 2^bpc−1]
    let bereich = |k: usize| -> (f64, f64) {
        match decode.get(2 * k..2 * k + 2) {
            Some(&[a, b]) => (a, b),
            _ => match (&raum, k) {
                (Raum::Index(..), _) => (0.0, max),
                (Raum::Lab, 0) => (0.0, 100.0),
                (Raum::Lab, _) => (-100.0, 100.0),
                _ => (0.0, 1.0),
            },
        }
    };
    let invers = maske && bereich(0).0 >= 0.5;
    let zeile = (w as usize * n * bpc).div_ceil(8);
    let mut px = Vec::with_capacity((w * h) as usize);
    let mut werte = vec![0.0f64; n];
    // Häufige Fälle ohne Fliesskomma je Bildpunkt: eine Komponente bis 8 Bit
    // (Grau, Indexed, Sonderfarbe) über eine Tabelle aller Werte; 8 Bit RGB
    // und CMYK ohne /Decode direkt
    let tabelle: Option<Vec<u32>> = (!maske && n == 1 && bpc <= 8 && schluessel.is_empty()).then(|| {
        let (a, b) = bereich(0);
        (0..1u32 << bpc).map(|i| raum.farbe(&[a + f64::from(i) * (b - a) / max])).collect()
    });
    let direkt = !maske && bpc == 8 && decode.is_empty() && schluessel.is_empty();
    let rgb8 = direkt && raum == Raum::Rgb;
    let cmyk8 = direkt && raum == Raum::Cmyk;
    for y in 0..h as usize {
        // fehlende Zeilen (abgeschnittene Daten) bleiben durchsichtig
        let Some(z) = d.get(y * zeile..(y + 1) * zeile) else {
            px.resize((w * h) as usize, 0);
            break;
        };
        for xi in 0..w as usize {
            let roh = |k: usize| -> u32 {
                let bit = (xi * n + k) * bpc;
                match bpc {
                    8 => u32::from(z[bit / 8]),
                    16 => u32::from(z[bit / 8]) << 8 | u32::from(z[bit / 8 + 1]),
                    _ => u32::from(z[bit / 8]) >> (8 - bpc - bit % 8) & ((1 << bpc) - 1),
                }
            };
            let c = if let Some(tab) = &tabelle {
                tab[roh(0) as usize]
            } else if rgb8 {
                let o = xi * 3;
                0xFF00_0000 | u32::from(z[o]) << 16 | u32::from(z[o + 1]) << 8 | u32::from(z[o + 2])
            } else if cmyk8 {
                let o = xi * 4;
                let k = 255 - u32::from(z[o + 3]);
                let f = |i: usize| (255 - u32::from(z[o + i])) * k / 255;
                0xFF00_0000 | f(0) << 16 | f(1) << 8 | f(2)
            } else if maske {
                // Schablone: 0 = in der Füllfarbe malen, 1 = durchsichtig
                // (/Decode [1 0] kehrt um)
                if (roh(0) == 0) != invers { fuell | 0xFF00_0000 } else { 0 }
            } else if schluessel.len() >= 2 * n && (0..n).all(|k| schluessel[2 * k] <= roh(k) && roh(k) <= schluessel[2 * k + 1]) {
                0
            } else {
                for (k, wert) in werte.iter_mut().enumerate() {
                    let (a, b) = bereich(k);
                    *wert = a + f64::from(roh(k)) * (b - a) / max;
                }
                raum.farbe(&werte)
            };
            px.push(c);
        }
    }
    Some(kolibri::dekoder::Rgba { w, h, px, delay: 0 })
}

/// Nicht darstellbares Bild (Filter fehlt, zu wenig Speicher): hellgraue
/// Fläche mit Rand und Kreuz, damit die Stelle als Bild erkennbar bleibt.
fn bild_ersatz(l: &mut Leinwand, ctm: &M) {
    let e = [ctm.punkt(0.0, 0.0), ctm.punkt(1.0, 0.0), ctm.punkt(1.0, 1.0), ctm.punkt(0.0, 1.0)];
    fuellen(l, &[vec![e[0], e[1], e[2], e[3], e[0]]], 0xFFE4_E4E4, false);
    for (a, b) in [(e[0], e[1]), (e[1], e[2]), (e[2], e[3]), (e[3], e[0]), (e[0], e[2]), (e[1], e[3])] {
        linie(l, a, b, 0xFFA0_A0A0);
    }
}

/// Bild in das Einheitsquadrat der Abbildung `ctm` zeichnen (nächster
/// Nachbar, auch gedreht).
fn bild_zeichnen(l: &mut Leinwand, b: &kolibri::dekoder::Rgba, ctm: &M, deckkraft: f64) {
    let deck = (deckkraft.clamp(0.0, 1.0) * 255.0).round() as u32;
    let ecken = [ctm.punkt(0.0, 0.0), ctm.punkt(1.0, 0.0), ctm.punkt(1.0, 1.0), ctm.punkt(0.0, 1.0)];
    let x0 = ecken.iter().map(|p| p.0).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let x1 = (ecken.iter().map(|p| p.0).fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(l.w);
    let y0 = ecken.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let y1 = (ecken.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(l.h);
    let Some(inv) = ctm.invers() else { return };
    // Bildpunkt-Mitte → Einheitsquadrat (u, v) → Spalte u·w, Zeile (1 − v)·h
    // (Bildzeile 0 oben); affin, also in f32 mit festen Schritten je Punkt
    let [a, bb, c, d, e, f] = inv.0;
    let (bw, bh) = (f64::from(b.w), f64::from(b.h));
    let (sa, sc, se) = ((a * bw) as f32, (c * bw) as f32, ((0.5 * a + 0.5 * c + e) * bw) as f32);
    let (ta, tc, tf) = ((-bb * bh) as f32, (-d * bh) as f32, ((1.0 - (0.5 * bb + 0.5 * d + f)) * bh) as f32);
    let (bwf, bhf) = (b.w as f32, b.h as f32);
    for py in y0..y1 {
        let (mut s, mut t) = (sa * x0 as f32 + sc * py as f32 + se, ta * x0 as f32 + tc * py as f32 + tf);
        for px in x0..x1 {
            let innen = s >= 0.0 && s < bwf && t > 0.0 && t <= bhf;
            let (bx, by) = (s as u32, (t as u32).min(b.h - 1));
            s += sa;
            t += ta;
            if !innen {
                continue;
            }
            let c = b.px[(by * b.w + bx.min(b.w - 1)) as usize];
            malen(&mut l.px[py * l.w + px], (c & 0xFF_FFFF) | ((c >> 24) * deck / 255) << 24);
        }
    }
}

impl Leser<'_> {
    /// Für den Inhaltsstrom: Wort zu Objekt (Felder, Wörterbücher).
    fn aus_wort_pub(&mut self, w: Wort) -> Option<Obj> {
        self.aus_wort(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PDF aus Objekten zusammensetzen (klassische Querverweis-Tabelle).
    fn pdf_bauen(objekte: &[Vec<u8>], xref_falsch: bool) -> Vec<u8> {
        let mut d = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut orte = Vec::new();
        for (i, o) in objekte.iter().enumerate() {
            orte.push(d.len());
            d.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            d.extend_from_slice(o);
            d.extend_from_slice(b"\nendobj\n");
        }
        let xref = d.len();
        d.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objekte.len() + 1).as_bytes());
        for o in &orte {
            d.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        d.extend_from_slice(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", objekte.len() + 1, if xref_falsch { 7 } else { xref }).as_bytes(),
        );
        d
    }

    fn strom(dict: &str, daten: &[u8]) -> Vec<u8> {
        let mut v = format!("<< {dict} /Length {} >>\nstream\n", daten.len()).into_bytes();
        v.extend_from_slice(daten);
        v.extend_from_slice(b"\nendstream");
        v
    }

    fn voll(_: char) -> [u8; 16] {
        [0xFF; 16]
    }

    fn einfach(xref_falsch: bool) -> Vec<u8> {
        let inhalt = b"1 0 0 rg 100 100 200 150 re f\nBT /F1 24 Tf 72 700 Td (Hallo Welt) Tj ET";
        pdf_bauen(
            &[
                b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
                b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".to_vec(),
                b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
                strom("", inhalt),
                b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
            ],
            xref_falsch,
        )
    }

    #[test]
    fn einfache_seite_text_und_rechteck() {
        let p = Pdf::oeffnen(einfach(false)).expect("PDF");
        assert_eq!(p.seiten.len(), 1);
        assert_eq!(p.seiten[0].kasten, [0.0, 0.0, 612.0, 792.0]);
        let (l, texte) = seite_zeichnen(&p, 0, 1.0, &voll).expect("Seite");
        assert_eq!((l.w, l.h), (612, 792));
        let text: String = texte.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(text, "Hallo Welt");
        // Rechteck 100..300 × 100..250 (PDF) → Bildpunkte y 542..692
        assert_eq!(l.px[600 * l.w + 200] & 0xFF_FFFF, 0xFF_0000, "rot gefüllt");
        assert_eq!(l.px[400 * l.w + 200] & 0xFF_FFFF, 0xFF_FFFF, "ausserhalb weiss");
        // Text: Grundlinie y = 792 - 700 = 92, Zeichen bis ~21 px darüber
        let dunkel = (75..92).flat_map(|y| (72..200).map(move |x| (x, y))).filter(|&(x, y)| l.px[y * l.w + x] & 0xFF < 0x80).count();
        assert!(dunkel > 500, "Text gezeichnet ({dunkel} dunkle Bildpunkte)");
    }

    /// /Rotate im Uhrzeigersinn: roter Streifen an der linken Seitenkante
    /// liegt bei 90° oben, bei 180° rechts, bei 270° unten.
    #[test]
    fn drehung_im_uhrzeigersinn() {
        for (grad, (w, h), rot, weiss) in [
            (0, (200, 100), (25, 50), (175, 50)),
            (90, (100, 200), (50, 25), (50, 175)),
            (180, (200, 100), (175, 50), (25, 50)),
            (270, (100, 200), (50, 175), (50, 25)),
        ] {
            let seite = format!("<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Rotate {grad} >>");
            let d = pdf_bauen(
                &[
                    b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
                    b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] >>".to_vec(),
                    seite.into_bytes(),
                    strom("", b"1 0 0 rg 0 0 50 100 re f"),
                ],
                false,
            );
            let p = Pdf::oeffnen(d).expect("PDF");
            let (l, _) = seite_zeichnen(&p, 0, 1.0, &voll).expect("Seite");
            assert_eq!((l.w, l.h), (w, h), "{grad}°");
            assert_eq!(l.px[rot.1 * l.w + rot.0] & 0xFF_FFFF, 0xFF_0000, "{grad}°: Streifen bei {rot:?}");
            assert_eq!(l.px[weiss.1 * l.w + weiss.0] & 0xFF_FFFF, 0xFF_FFFF, "{grad}°: weiss bei {weiss:?}");
        }
    }

    /// Seite 20×10 pt mit Inhalt, Ressourcen und weiteren Objekten ab Nr. 5.
    fn seite_mit(inhalt: &[u8], res: &str, weitere: &[Vec<u8>]) -> Pdf {
        let mut o = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 20 10] >>".to_vec(),
            format!("<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources {res} >>").into_bytes(),
            strom("", inhalt),
        ];
        o.extend_from_slice(weitere);
        Pdf::oeffnen(pdf_bauen(&o, false)).expect("PDF")
    }

    /// Seite 1:1 zeichnen; Farbe (ohne Deckkraft) an (x, y).
    fn farbe_bei(p: &Pdf, punkte: &[(usize, usize)]) -> Vec<u32> {
        let (l, _) = seite_zeichnen(p, 0, 1.0, &voll).expect("Seite");
        punkte.iter().map(|&(x, y)| l.px[y * l.w + x] & 0xFF_FFFF).collect()
    }

    const LINKS_RECHTS: [(usize, usize); 2] = [(5, 5), (15, 5)];

    #[test]
    fn lzw_und_lauflaenge() {
        // Beispiel aus PDF 7.4.4.2: „-----A---B“
        assert_eq!(lzw(&[0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01], true, usize::MAX), b"-----A---B");
        // TIFF-LZW aus Pillow (gleiche Breitenwechsel wie EarlyChange 1),
        // 30000 Bytes: alle Breiten 9–12 und Tabellen-Neustart
        let erwartet: Vec<u8> = (0..30000usize).map(|i| if (i / 300) % 3 != 0 { ((i * 7) ^ (i >> 5)) as u8 } else { (i / 600) as u8 }).collect();
        assert_eq!(lzw(include_bytes!("../tests/pdf/lzw_probe.bin"), true, usize::MAX), erwartet);
        assert!(lzw(include_bytes!("../tests/pdf/lzw_probe.bin"), true, 1000).len() <= 1000, "Grenze");
        assert_eq!(lauflaenge(&[2, b'a', b'b', b'c', 254, b'x', 128, b'z'], usize::MAX), b"abcxxx");
    }

    #[test]
    fn strichmuster_und_deckkraft() {
        // [2 2] 0 d: an 0–2, aus 2–4, an 4–6 …
        let p = seite_mit(b"0 G [2 2] 0 d 0 5 m 20 5 l S", "<< >>", &[]);
        assert_eq!(farbe_bei(&p, &[(1, 5), (3, 5), (5, 5), (7, 5)]), [0, 0xFF_FFFF, 0, 0xFF_FFFF]);
        // ExtGState: Deckkraft 0,5 beim Füllen, Linienbreite aus /LW
        let p = seite_mit(b"/GS1 gs 1 0 0 rg 0 0 20 10 re f", "<< /ExtGState << /GS1 << /ca 0.5 /LW 3 >> >> >>", &[]);
        let f = farbe_bei(&p, &[(5, 5)])[0];
        assert!(f >> 16 == 255 && (126..=129).contains(&(f >> 8 & 255)), "halb rot: {f:06x}");
        // Strichmuster aus ExtGState /D
        let p = seite_mit(b"/GS1 gs 0 G 0 5 m 20 5 l S", "<< /ExtGState << /GS1 << /D [[2 2] 0] >> >> >>", &[]);
        assert_eq!(farbe_bei(&p, &[(1, 5), (3, 5)]), [0, 0xFF_FFFF]);
    }

    #[test]
    fn bildmasken() {
        let bild = |zusatz: &str, daten: &[u8]| strom(&format!("/Type /XObject /Subtype /Image /Width 2 /Height 1 {zusatz}"), daten);
        let res = "<< /XObject << /Im 5 0 R >> >>";
        // /SMask: links deckend, rechts durchsichtig
        let p = seite_mit(b"q 20 0 0 10 0 0 cm /Im Do Q", res, &[
            bild("/ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask 6 0 R", &[255, 0, 0, 255, 0, 0]),
            bild("/ColorSpace /DeviceGray /BitsPerComponent 8", &[255, 0]),
        ]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0xFF_0000, 0xFF_FFFF]);
        // Schablone in der Füllfarbe: 0 = malen
        let p = seite_mit(b"0 1 0 rg q 20 0 0 10 0 0 cm /Im Do Q", res, &[bild("/ImageMask true", &[0x40])]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0x00_FF00, 0xFF_FFFF]);
        // /Decode [1 0] kehrt die Schablone um
        let p = seite_mit(b"0 1 0 rg q 20 0 0 10 0 0 cm /Im Do Q", res, &[bild("/ImageMask true /Decode [1 0]", &[0x40])]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0xFF_FFFF, 0x00_FF00]);
        // /Mask als Bild (wie MuPDF: Abtastwert 1 = verdeckt)
        let p = seite_mit(b"0 0 1 rg 0 0 20 10 re f q 20 0 0 10 0 0 cm /Im Do Q", res, &[
            bild("/ColorSpace /DeviceRGB /BitsPerComponent 8 /Mask 6 0 R", &[255, 0, 0, 255, 0, 0]),
            bild("/ImageMask true", &[0x40]),
        ]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0xFF_0000, 0x00_00FF]);
        // /Mask als Farbschlüssel: Grauwert 10 liegt in [0 50] → durchsichtig
        let p = seite_mit(b"0 0 1 rg 0 0 20 10 re f q 20 0 0 10 0 0 cm /Im Do Q", res, &[bild("/ColorSpace /DeviceGray /BitsPerComponent 8 /Mask [0 50]", &[10, 200])]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0x00_00FF, 0xC8_C8C8]);
        // nicht unterstützter Filter: graue Ersatzfläche statt nichts
        let p = seite_mit(b"q 20 0 0 10 0 0 cm /Im Do Q", res, &[bild("/ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode", &[0])]);
        assert_eq!(farbe_bei(&p, &[(10, 2)]), [0xE4_E4E4]);
    }

    #[test]
    fn eingebettete_bilder() {
        let p = seite_mit(b"q 20 0 0 10 0 0 cm BI /W 2 /H 1 /CS /RGB /BPC 8 ID \xFF\x00\x00\x00\x00\xFF EI Q 0 g 0 0 1 1 re f", "<< >>", &[]);
        assert_eq!(farbe_bei(&p, &[(5, 5), (15, 5), (0, 9)]), [0xFF_0000, 0x00_00FF, 0]);
        // mit Filter (ASCIIHex) und Indexed-Farbraum in Kurzform
        let p = seite_mit(b"q 20 0 0 10 0 0 cm BI /W 2 /H 1 /CS [/I /RGB 1 <00FF00FFFF00>] /BPC 8 /F /AHx ID 0001> EI Q", "<< >>", &[]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0x00_FF00, 0xFF_FF00]);
    }

    #[test]
    fn farbraeume_im_inhalt() {
        // Sonderfarbe mit Typ-2-Funktion nach CMYK: Tinte 1 = Rot
        let res = "<< /ColorSpace << /CS0 [/Separation /Spot /DeviceCMYK << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 1 1 0] /N 1 >>] /I0 [/Indexed /DeviceRGB 1 <00FF000000FF>] >> >>";
        let p = seite_mit(b"/CS0 cs 1 sc 0 0 10 10 re f /I0 cs 1 sc 10 0 10 10 re f", res, &[]);
        assert_eq!(farbe_bei(&p, &LINKS_RECHTS), [0xFF_0000, 0x00_00FF]);
        // cs setzt die Anfangsfarbe: Sonderfarbe voll, Lab schwarz
        let res = "<< /ColorSpace << /CS0 [/Separation /Spot /DeviceCMYK << /FunctionType 2 /C1 [0 0 1 0] /N 1 >>] /L [/Lab << /WhitePoint [0.9505 1 1.089] >>] >> >>";
        let p = seite_mit(b"/CS0 cs 0 0 10 10 re f /L cs 100 0 0 scn 10 0 10 10 re f", res, &[]);
        let f = farbe_bei(&p, &LINKS_RECHTS);
        assert_eq!(f[0], 0xFF_FF00, "volle Sonderfarbe gelb");
        assert!(f[1] & 0xFF >= 0xFD && f[1] >> 16 >= 0xFD, "Lab L=100 weiss: {:06x}", f[1]);
    }

    #[test]
    fn textarten_und_schrift_aus_grafikzustand() {
        let schrift = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec();
        let dunkel = |p: &Pdf| {
            let (l, _) = seite_zeichnen(p, 0, 1.0, &voll).expect("Seite");
            l.px.iter().filter(|&&c| c & 0xFF_FFFF != 0xFF_FFFF).map(|&c| c & 0xFF_FFFF).collect::<Vec<_>>()
        };
        // Tr 3: unsichtbar
        let p = seite_mit(b"BT /F1 8 Tf 3 Tr 2 2 Td (AB) Tj ET", "<< /Font << /F1 5 0 R >> >>", &[schrift.clone()]);
        assert!(dunkel(&p).is_empty(), "Tr 3 zeichnet nichts");
        // Tr 1: in der Strichfarbe
        let p = seite_mit(b"1 0 0 RG 0 0 1 rg BT /F1 8 Tf 1 Tr 2 2 Td (AB) Tj ET", "<< /Font << /F1 5 0 R >> >>", &[schrift.clone()]);
        let d = dunkel(&p);
        assert!(!d.is_empty() && d.iter().all(|c| c & 0xFF == c >> 8 & 0xFF && c >> 16 == 0xFF), "rot gezogen");
        // Schrift über /ExtGState /Font
        let p = seite_mit(b"BT /GS1 gs 2 2 Td (AB) Tj ET", "<< /ExtGState << /GS1 << /Font [5 0 R 8] >> >> >>", &[schrift]);
        assert!(!dunkel(&p).is_empty(), "Schrift aus dem Grafikzustand");
    }

    /// Review Z1: kaputte bzw. bösartige Dateien enden mit Fehler oder
    /// leerer Stelle, nie mit Absturz, Stapelüberlauf oder endloser Arbeit.
    #[test]
    fn boesartige_dateien() {
        // 100 000 verschachtelte Felder (Stapel)
        let mut tief = b"0 0 1 rg 0 0 20 10 re f ".to_vec();
        tief.extend(std::iter::repeat_n(b'[', 100_000));
        let p = seite_mit(&tief, "<< >>", &[]);
        assert_eq!(farbe_bei(&p, &[(5, 5)]), [0x00_00FF], "Seite bis dahin gezeichnet");
        let mut l = Leser::neu(&tief[24..], 0);
        assert_eq!(l.objekt(), None, "zu tief verschachtelt");
        // /Length zeigt auf das eigene Objekt; zwei Längen zeigen aufeinander
        let p = seite_mit(b"", "<< /XObject << /A 5 0 R /B 6 0 R >> >>", &[
            b"<< /Length 5 0 R >>\nstream\n0 0 1 rg 0 0 20 10 re f\nendstream".to_vec(),
            b"<< /Length 7 0 R >>\nstream\nx\nendstream".to_vec(),
            b"<< /Length 6 0 R >>\nstream\nx\nendstream".to_vec(),
        ]);
        assert!(matches!(p.objekt(5), Obj::Strom(..)));
        assert!(matches!(p.objekt(6), Obj::Strom(..)));
        // Seitenbaum zeigt auf sich selbst (zweimal) – sonst 2^32 Aufrufe
        let d = pdf_bauen(
            &[
                b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
                b"<< /Type /Pages /Kids [2 0 R 2 0 R 3 0 R] /Count 1 /MediaBox [0 0 20 10] >>".to_vec(),
                b"<< /Type /Page /Parent 2 0 R >>".to_vec(),
            ],
            false,
        );
        let t = std::time::Instant::now();
        assert_eq!(Pdf::oeffnen(d).expect("PDF").seiten.len(), 1);
        assert!(t.elapsed().as_secs() < 2, "Seitenbaum ohne Wiederholung");
        // eingebettetes Bild mit riesigen Massen ohne Filter
        let p = seite_mit(b"BI /W 4294967295 /H 4294967295 /BPC 8 /CS /G ID xyz EI 0 0 1 rg 0 0 20 10 re f", "<< >>", &[]);
        let _ = seite_zeichnen(&p, 0, 1.0, &voll).expect("Seite trotz Bild");
        // Prädiktor mit 200 Mio. Spalten: Daten bleiben, kein Speicher
        let pp = Obj::Dict(BTreeMap::from([(b"Predictor".to_vec(), Obj::Zahl(12.0)), (b"Columns".to_vec(), Obj::Zahl(2e8))]));
        assert_eq!(praediktor(vec![1, 2, 3], Some(&pp)), vec![1, 2, 3]);
        // Deflate-Bombe: 1 MB Nullen, Grenze 64 KB
        let bombe = kolibri::kodierer::zlib(&vec![0u8; 1 << 20]);
        assert!(flate(&bombe, 64 << 10).is_err());
        assert_eq!(flate(&bombe, 2 << 20).map(|v| v.len()), Ok(1 << 20));
        // LZW und RunLength halten ihre Grenze
        assert!(lauflaenge(&[129, 7].repeat(10_000), 1000).len() <= 1000 + 128);
        // q ohne Ende
        let mut viele_q = b"q ".repeat(100_000);
        viele_q.extend_from_slice(b"0 0 1 rg 0 0 20 10 re f");
        assert_eq!(farbe_bei(&seite_mit(&viele_q, "<< >>", &[]), &[(5, 5)]), [0x00_00FF]);
    }

    #[test]
    fn querverweis_strom_ohne_breite_wird_abgelehnt() {
        let mut d = b"%PDF-1.5\n".to_vec();
        let x = d.len();
        d.extend_from_slice(b"1 0 obj\n<< /Type /XRef /Size 2000000000 /W [0 0 0] /Length 0 >>\nstream\n\nendstream\nendobj\n");
        d.extend_from_slice(format!("startxref\n{x}\n%%EOF\n").as_bytes());
        let t = std::time::Instant::now();
        assert!(Pdf::oeffnen(d).is_err());
        assert!(t.elapsed().as_secs() < 2);
    }

    /// Formular erbt die Schrift der Seite (gültiges PDF, vorher Absturz).
    #[test]
    fn formular_erbt_schrift() {
        let schrift = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec();
        let form = strom("/Type /XObject /Subtype /Form /BBox [0 0 20 10]", b"BT 2 2 Td (x) Tj ET");
        let p = seite_mit(b"BT /F1 8 Tf ET /Fm Do", "<< /Font << /F1 5 0 R >> /XObject << /Fm 6 0 R >> >>", &[schrift, form]);
        let (_, texte) = seite_zeichnen(&p, 0, 1.0, &voll).expect("Seite");
        assert_eq!(texte.iter().map(|t| t.text.as_str()).collect::<String>(), "x");
    }

    /// Type3: Breiten im Glyphenraum über /FontMatrix.
    #[test]
    fn type3_breiten_mit_fontmatrix() {
        let p = seite_mit(b"", "<< >>", &[b"<< /Type /Font /Subtype /Type3 /FontMatrix [0.01 0 0 0.01 0 0] /FirstChar 65 /LastChar 65 /Widths [50] >>".to_vec()]);
        assert_eq!(Schrift::laden(&p, &p.objekt(5)).breite(65), 500.0);
    }

    /// TIFF-Prädiktor 2 (8 Bit) und /Decode [1 0] bei JPEG.
    #[test]
    fn tiff_praediktor_und_jpeg_decode() {
        let pp = Obj::Dict(BTreeMap::from([(b"Predictor".to_vec(), Obj::Zahl(2.0)), (b"Columns".to_vec(), Obj::Zahl(3.0))]));
        assert_eq!(praediktor(vec![10, 1, 1, 5, 5, 5], Some(&pp)), vec![10, 11, 12, 5, 10, 15]);
        let jpeg = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bilder/foto.jpg")).unwrap();
        let bild = |zusatz: &str| strom(&format!("/Type /XObject /Subtype /Image /Width 640 /Height 480 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode {zusatz}"), &jpeg);
        let res = "<< /XObject << /Im 5 0 R >> >>";
        let normal = farbe_bei(&seite_mit(b"q 20 0 0 10 0 0 cm /Im Do Q", res, &[bild("")]), &[(10, 5)])[0];
        let negativ = farbe_bei(&seite_mit(b"q 20 0 0 10 0 0 cm /Im Do Q", res, &[bild("/Decode [1 0 1 0 1 0]")]), &[(10, 5)])[0];
        assert_eq!(negativ, !normal & 0xFF_FFFF, "Decode [1 0] kehrt um");
    }

    #[test]
    fn kaputter_querverweis_wird_gesucht() {
        let p = Pdf::oeffnen(einfach(true)).expect("PDF trotz falschem startxref");
        assert_eq!(p.seiten.len(), 1);
    }

    /// PDF 1.5: Querverweis-Strom (mit PNG-Prädiktor) und Objektstrom,
    /// Inhalt Flate-gepackt; Type0-Schrift mit ToUnicode und /W-Breiten;
    /// Seite um 90° gedreht.
    #[test]
    fn querverweis_strom_objektstrom_und_cid_schrift() {
        let z = kolibri::kodierer::zlib;
        let cmap: &[u8] = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n2 beginbfchar <0003> <0048> <0004> <00FC> endbfchar\n1 beginbfrange <0010> <0012> <0061> endbfrange\nendcmap end end";
        let inhalt = z(b"BT /F1 10 Tf 50 50 Td <000300040010001100120003> Tj ET");
        let kat: &[u8] = b"<< /Type /Catalog /Pages 2 0 R >>";
        let seiten: &[u8] = b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] >>";
        let seite: &[u8] = b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 4 0 R >> >> /Rotate 90 >>";
        let schrift: &[u8] = b"<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 7 0 R >>";
        // Objekte 1..4 liegen im Objektstrom (Nr. 8)
        let mut koerper = Vec::new();
        let mut kopf = String::new();
        for (nr, o) in [(1, kat), (2, seiten), (3, seite), (4, schrift)] {
            kopf.push_str(&format!("{nr} {} ", koerper.len()));
            koerper.extend_from_slice(o);
            koerper.push(b' ');
        }
        let mut objstm = kopf.into_bytes();
        let erste = objstm.len();
        objstm.extend_from_slice(&koerper);
        let mut d = b"%PDF-1.5\n".to_vec();
        // je Objekt: (Art, Feld 2, Feld 3) wie im Querverweis-Strom
        let mut orte = [(0u8, 0usize, 0usize); 10];
        let neu = |d: &mut Vec<u8>, orte: &mut [(u8, usize, usize); 10], nr: usize, o: Vec<u8>| {
            orte[nr] = (1, d.len(), 0);
            d.extend_from_slice(format!("{nr} 0 obj\n").as_bytes());
            d.extend_from_slice(&o);
            d.extend_from_slice(b"\nendobj\n");
        };
        neu(&mut d, &mut orte, 5, strom("/Filter /FlateDecode", &inhalt));
        neu(&mut d, &mut orte, 6, b"<< /Type /Font /Subtype /CIDFontType2 /DW 500 /W [3 [600 700] 16 18 400] >>".to_vec());
        neu(&mut d, &mut orte, 7, strom("", cmap));
        neu(&mut d, &mut orte, 8, strom(&format!("/Type /ObjStm /N 4 /First {erste} /Filter /FlateDecode"), &z(&objstm)));
        for (i, nr) in (1..=4).enumerate() {
            orte[nr] = (2, 8, i);
        }
        // Querverweis-Strom (Nr. 9): W [1 4 1], Zeilen mit PNG-Filter „Up“
        let xpos = d.len();
        orte[9] = (1, xpos, 0);
        let mut roh = Vec::new();
        let mut vorige = [0u8; 6];
        for &(a, b, c) in &orte {
            let b = b as u32;
            let zl = [a, (b >> 24) as u8, (b >> 16) as u8, (b >> 8) as u8, b as u8, c as u8];
            roh.push(2);
            for k in 0..6 {
                roh.push(zl[k].wrapping_sub(vorige[k]));
            }
            vorige = zl;
        }
        let xs = strom("/Type /XRef /Size 10 /W [1 4 1] /Root 1 0 R /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 6 >>", &z(&roh));
        d.extend_from_slice(b"9 0 obj\n");
        d.extend_from_slice(&xs);
        d.extend_from_slice(format!("\nendobj\nstartxref\n{xpos}\n%%EOF\n").as_bytes());

        let p = Pdf::oeffnen(d).expect("PDF 1.5");
        assert_eq!(p.seiten.len(), 1);
        assert_eq!(p.seiten[0].drehung, 90);
        let (l, texte) = seite_zeichnen(&p, 0, 2.0, &voll).expect("Seite");
        // gedreht: quer → hochkant
        assert_eq!((l.w, l.h), (200, 400));
        let text: String = texte.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(text, "HüabcH");
        let s = Schrift::laden(&p, &p.objekt(4));
        assert_eq!((s.breite(3), s.breite(4), s.breite(17), s.breite(99)), (600.0, 700.0, 400.0, 500.0));
    }

    #[test]
    fn filter_und_praediktor() {
        assert_eq!(ascii_hex(b"48 61 6C6C6F>"), b"Hallo");
        assert_eq!(ascii85(b"87cURD]i,\"Ebo80~>"), b"Hello World!");
        // PNG „Sub“ über zwei Spalten
        let p = Obj::Dict(BTreeMap::from([(b"Predictor".to_vec(), Obj::Zahl(12.0)), (b"Columns".to_vec(), Obj::Zahl(2.0))]));
        assert_eq!(praediktor(vec![1, 5, 3], Some(&p)), vec![5, 8]);
    }

    #[test]
    fn leser_texte_und_verweise() {
        let mut l = Leser::neu(b"(a\\(b\\)c\\101) /Na#20me 3 0 R [1 2.5 -3] << /K (v) >>", 0);
        assert_eq!(l.objekt(), Some(Obj::Text(b"a(b)cA".to_vec())));
        assert_eq!(l.objekt(), Some(Obj::Name(b"Na me".to_vec())));
        assert_eq!(l.objekt(), Some(Obj::Ref(3, 0)));
        assert_eq!(l.objekt(), Some(Obj::Feld(vec![Obj::Zahl(1.0), Obj::Zahl(2.5), Obj::Zahl(-3.0)])));
        assert!(matches!(l.objekt(), Some(Obj::Dict(_))));
    }
}
