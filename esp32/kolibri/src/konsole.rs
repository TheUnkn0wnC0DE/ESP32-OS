//! C1: console.obj nativ – dieselbe Schnittstelle wie die KolibriOS-
//! Bibliothek (`programs/develop/libraries/console_coff/console_en.txt`):
//! Textfenster mit 80×25 Zeichen (kleine Systemschrift 6×9, cp866), Farben
//! über die Flags bzw. Esc-Sequenzen, Eingabe mit Zeilenbearbeitung.
//!
//! Die C-Seite (`c/konsole.c`) stellt die Funktionszeiger `con_*` der
//! libc.obj-Header bereit und zeigt auf die Funktionen hier (`__stdcall` wie
//! im Original). Jeder Faden hat seine eigene Konsole (Zeiger im Faden des
//! Kerns, Speicher im Heap des Programms – endet mit ihm). Ausgaben werden
//! sofort gezeichnet, aber nur die geänderten Zellen einer Zeile (ein Bild
//! je Zeilenstück). Die Bildschirmtastatur erscheint, sobald das Programm
//! auf eine Taste wartet; sie hält die Zeile mit der Schreibmarke frei.
//!
//! Abweichungen vom Original (PLAN, „Abweichungen“): kein Zurückblättern
//! (Bildschirmpuffer = sichtbarer Bereich), Schliessen des Fensters beendet
//! das Programm sofort.
use core::ffi::{c_char, c_int};
use kern::sys;
use std::collections::VecDeque;

/// Aufrufkonvention der C-Schnittstelle: `__stdcall` wie console.obj
/// ("system"). Am PC endet ein Programm durch Abwickeln (auch mitten in
/// getch, z. B. Kreuz) – dort "system-unwind". Am Board wird nie
/// abgewickelt (panic = abort, ohne Personality-Routine): "system".
macro_rules! ffi_fn {
    ($(#[$a:meta])* pub unsafe fn $n:ident($($p:tt)*) $(-> $r:ty)? $b:block) => {
        #[cfg(target_os = "espidf")]
        $(#[$a])* pub unsafe extern "system" fn $n($($p)*) $(-> $r)? $b
        #[cfg(not(target_os = "espidf"))]
        $(#[$a])* pub unsafe extern "system-unwind" fn $n($($p)*) $(-> $r)? $b
    };
    ($(#[$a:meta])* pub fn $n:ident($($p:tt)*) $(-> $r:ty)? $b:block) => {
        #[cfg(target_os = "espidf")]
        $(#[$a])* pub extern "system" fn $n($($p)*) $(-> $r)? $b
        #[cfg(not(target_os = "espidf"))]
        $(#[$a])* pub extern "system-unwind" fn $n($($p)*) $(-> $r)? $b
    };
}

const ZB: i32 = 6;
const ZH: i32 = 9;
const STANDARD_W: usize = 80;
const STANDARD_H: usize = 25;
/// Standardfarbe: grau auf schwarz.
const STANDARD_ATTR: u8 = 7;
const IGNORIERE_SONDER: u32 = 0x100;
const FENSTER_ZU: u32 = 0x200;

/// VGA-Farben, Index = Attribut-Bits (Blau 1, Grün 2, Rot 4, Hell 8); BGR.
const FARBEN: [[u8; 3]; 16] = [
    [0, 0, 0],
    [0xAA, 0, 0],
    [0, 0xAA, 0],
    [0xAA, 0xAA, 0],
    [0, 0, 0xAA],
    [0xAA, 0, 0xAA],
    [0, 0x55, 0xAA],
    [0xAA, 0xAA, 0xAA],
    [0x55, 0x55, 0x55],
    [0xFF, 0x55, 0x55],
    [0x55, 0xFF, 0x55],
    [0xFF, 0xFF, 0x55],
    [0x55, 0x55, 0xFF],
    [0xFF, 0x55, 0xFF],
    [0x55, 0xFF, 0xFF],
    [0xFF, 0xFF, 0xFF],
];

/// ANSI-Farbe 0..7 (schwarz, rot, grün, gelb, blau, magenta, cyan, weiss)
/// → Attribut-Bits.
const ANSI: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

#[derive(Clone, Copy, PartialEq)]
struct Zelle {
    z: u8,
    a: u8,
}

const LEER: Zelle = Zelle { z: b' ', a: STANDARD_ATTR };

enum Esc {
    Nein,
    Esc,
    /// `Esc[` mit gesammelten Zeichen
    Csi(Vec<u8>),
    /// `Esc]` (Titel) bis BEL/ST
    Osc(Vec<u8>),
}

pub struct Konsole {
    w: usize,
    h: usize,
    zellen: Vec<Zelle>,
    /// Zweiter Puffer (Esc[?1049h), gemerkter Hauptpuffer und Lage
    gemerkt: Option<(Vec<Zelle>, usize, usize)>,
    x: usize,
    y: usize,
    flags: u32,
    cursor_h: i32,
    cursor_an: bool,
    /// Rollbereich (Esc[r): erste und letzte Zeile
    oben: usize,
    unten: usize,
    esc: Esc,
    /// geänderte Spalten je Zeile (von, bis ausschliesslich)
    schmutz: Vec<Option<(usize, usize)>>,
    /// gezeichnete Schreibmarke (Zelle)
    marke: Option<(usize, usize)>,
    /// Tasten für getch: erweiterte Taste = 0, dann Scancode
    warte_scancode: Option<u8>,
    /// gelesene, noch nicht abgeholte Tasten (Format wie getch2): beim
    /// Schreiben ankommende Tasten gehen nicht verloren (Review 24.11.)
    vorrat: VecDeque<u16>,
    /// Zeilen, um die der Bildschirm insgesamt gerollt ist (Zeileneingabe)
    gerollt: usize,
    /// Bildschirmtastatur angefordert: erst, wenn das Programm auf Eingabe
    /// wartet (Matrix u. a. schreiben nur – sonst wanderte die Tastatur mit
    /// der Schreibmarke und deckte die Taskleiste zu)
    tastatur: bool,
    titel: String,
    fertig: bool,
    /// Fensterlage (Pixel) beim Anlegen
    fx: i32,
    fy: i32,
    /// D1: Ausgabe auffangen statt anzeigen (Umleitung/Pipe der Shell)
    aufnahme: Option<Vec<u8>>,
}

fn faden_konsole() -> Option<&'static mut Konsole> {
    let z = kern::bibliothek();
    // SAFETY: der Zeiger stammt aus `Box::into_raw` dieses Fadens
    // (con_init) und lebt bis con_exit bzw. zum Ende des Programms.
    (z != 0).then(|| unsafe { &mut *(z as *mut Konsole) })
}

impl Konsole {
    fn neu(w: usize, h: usize, titel: String) -> Konsole {
        Konsole {
            w,
            h,
            zellen: vec![LEER; w * h],
            gemerkt: None,
            x: 0,
            y: 0,
            flags: u32::from(STANDARD_ATTR),
            cursor_h: (ZH * 15 / 100).max(1),
            cursor_an: true,
            oben: 0,
            unten: h - 1,
            esc: Esc::Nein,
            schmutz: vec![None; h],
            marke: None,
            warte_scancode: None,
            vorrat: VecDeque::new(),
            gerollt: 0,
            tastatur: false,
            titel,
            fertig: false,
            fx: 0,
            fy: 0,
            aufnahme: None,
        }
    }

    fn attr(&self) -> u8 {
        (self.flags & 0xFF) as u8
    }

    fn markiere(&mut self, y: usize, x0: usize, x1: usize) {
        if y >= self.h || x0 >= x1 {
            return;
        }
        let s = &mut self.schmutz[y];
        *s = Some(match *s {
            Some((a, b)) => (a.min(x0), b.max(x1)),
            None => (x0, x1),
        });
    }

    fn alles_markieren(&mut self) {
        for y in 0..self.h {
            self.markiere(y, 0, self.w);
        }
    }

    fn setze(&mut self, x: usize, y: usize, z: Zelle) {
        if x < self.w && y < self.h && self.zellen[y * self.w + x] != z {
            self.zellen[y * self.w + x] = z;
            self.markiere(y, x, x + 1);
        }
    }

    /// Bereich [oben, unten] um n Zeilen nach oben schieben (unten leer).
    fn rollen_hoch(&mut self, oben: usize, unten: usize, n: usize) {
        let w = self.w;
        for _ in 0..n.min(unten + 1 - oben) {
            self.zellen.copy_within((oben + 1) * w..(unten + 1) * w, oben * w);
            let leer = Zelle { z: b' ', a: self.attr() };
            self.zellen[unten * w..(unten + 1) * w].fill(leer);
        }
        for y in oben..=unten {
            self.markiere(y, 0, w);
        }
    }

    fn rollen_runter(&mut self, oben: usize, unten: usize, n: usize) {
        let w = self.w;
        for _ in 0..n.min(unten + 1 - oben) {
            self.zellen.copy_within(oben * w..unten * w, (oben + 1) * w);
            let leer = Zelle { z: b' ', a: self.attr() };
            self.zellen[oben * w..(oben + 1) * w].fill(leer);
        }
        for y in oben..=unten {
            self.markiere(y, 0, w);
        }
    }

    fn neue_zeile(&mut self) {
        if self.y == self.unten {
            self.rollen_hoch(self.oben, self.unten, 1);
            self.gerollt += 1;
        } else if self.y + 1 < self.h {
            self.y += 1;
        }
    }

    fn loeschen(&mut self, von: usize, bis: usize) {
        let leer = Zelle { z: b' ', a: self.attr() };
        for i in von..bis.min(self.w * self.h) {
            self.setze(i % self.w, i / self.w, leer);
        }
    }

    fn zeichen(&mut self, c: u8) {
        if self.flags & IGNORIERE_SONDER == 0 {
            match std::mem::replace(&mut self.esc, Esc::Nein) {
                Esc::Esc => {
                    match c {
                        b'[' => self.esc = Esc::Csi(Vec::new()),
                        b']' => self.esc = Esc::Osc(Vec::new()),
                        _ => {}
                    }
                    return;
                }
                Esc::Csi(mut v) => {
                    if (0x40..=0x7E).contains(&c) {
                        self.csi(&v, c);
                    } else if v.len() < 32 {
                        v.push(c);
                        self.esc = Esc::Csi(v);
                    }
                    return;
                }
                Esc::Osc(mut v) => {
                    if c == 7 || c == 0x9C {
                        // Esc]0;Titel bzw. Esc]2;Titel
                        if let Some(t) = v.get(1..).filter(|_| matches!(v.first(), Some(b'0' | b'2'))) {
                            let t = t.strip_prefix(b";").unwrap_or(t);
                            self.titel = String::from_utf8_lossy(t).into_owned();
                            sys::titel(&self.titel);
                        }
                    } else if v.len() < 256 {
                        v.push(c);
                        self.esc = Esc::Osc(v);
                    }
                    return;
                }
                Esc::Nein => {}
            }
            match c {
                b'\n' => {
                    self.x = 0;
                    self.neue_zeile();
                    return;
                }
                b'\r' => {
                    self.x = 0;
                    return;
                }
                8 => {
                    self.x = self.x.saturating_sub(1);
                    return;
                }
                b'\t' => {
                    let ziel = ((self.x / 8 + 1) * 8).min(self.w);
                    while self.x < ziel {
                        self.zeichen_setzen(b' ');
                    }
                    return;
                }
                27 => {
                    self.esc = Esc::Esc;
                    return;
                }
                _ => {}
            }
        }
        self.zeichen_setzen(c);
    }

    fn zeichen_setzen(&mut self, c: u8) {
        if self.x >= self.w {
            self.x = 0;
            self.neue_zeile();
        }
        let a = self.attr();
        self.setze(self.x, self.y, Zelle { z: c, a });
        self.x += 1;
    }

    fn csi(&mut self, roh: &[u8], end: u8) {
        let privat = roh.first() == Some(&b'?');
        let text = String::from_utf8_lossy(if privat { &roh[1..] } else { roh }).into_owned();
        let p: Vec<usize> = text.split(';').map(|t| t.parse().unwrap_or(0)).collect();
        let n = |i: usize| p.get(i).copied().filter(|&v| v > 0).unwrap_or(1);
        let (w, h) = (self.w, self.h);
        match end {
            b'm' => {
                let mut a = self.attr();
                for &v in if p.is_empty() { &[0][..] } else { &p[..] } {
                    match v {
                        0 => a = STANDARD_ATTR,
                        1 => a |= 0x08,
                        5 => a |= 0x80,
                        7 => a = (a << 4) | (a >> 4),
                        30..=37 => a = (a & 0xF8) | ANSI[v - 30],
                        40..=47 => a = (a & 0x8F) | (ANSI[v - 40] << 4),
                        _ => {}
                    }
                }
                self.flags = (self.flags & !0xFF) | u32::from(a);
            }
            b'A' => self.y = self.y.saturating_sub(n(0)),
            b'B' => self.y = (self.y + n(0)).min(h - 1),
            b'C' => self.x = (self.x + n(0)).min(w - 1),
            b'D' => self.x = self.x.saturating_sub(n(0)),
            b'H' | b'f' => {
                self.y = (n(0) - 1).min(h - 1);
                self.x = (n(1) - 1).min(w - 1);
            }
            b'd' => self.y = (n(0) - 1).min(h - 1),
            b'J' => {
                let pos = self.y * w + self.x.min(w - 1);
                match p.first().copied().unwrap_or(0) {
                    0 => self.loeschen(pos, w * h),
                    1 => self.loeschen(0, pos + 1),
                    _ => {
                        self.loeschen(0, w * h);
                        self.x = 0;
                        self.y = 0;
                    }
                }
            }
            b'K' => {
                let z = self.y * w;
                match p.first().copied().unwrap_or(0) {
                    0 => self.loeschen(z + self.x, z + w),
                    1 => self.loeschen(z, z + self.x.min(w - 1) + 1),
                    _ => self.loeschen(z, z + w),
                }
            }
            b'L' if (self.oben..=self.unten).contains(&self.y) => self.rollen_runter(self.y, self.unten, n(0)),
            b'M' if (self.oben..=self.unten).contains(&self.y) => self.rollen_hoch(self.y, self.unten, n(0)),
            b'P' => {
                let z = self.y * w;
                let k = n(0).min(w - self.x);
                self.zellen.copy_within(z + self.x + k..z + w, z + self.x);
                let leer = Zelle { z: b' ', a: self.attr() };
                self.zellen[z + w - k..z + w].fill(leer);
                self.markiere(self.y, self.x, w);
            }
            b'X' => {
                let z = self.y * w + self.x;
                self.loeschen(z, z + n(0).min(w - self.x));
            }
            b'r' => {
                let o = n(0) - 1;
                let u = p.get(1).copied().filter(|&v| v > 0).unwrap_or(h) - 1;
                if o < u && u < h {
                    self.oben = o;
                    self.unten = u;
                } else {
                    self.oben = 0;
                    self.unten = h - 1;
                }
                self.x = 0;
                self.y = 0;
            }
            b'h' | b'l' if privat => {
                let an = end == b'h';
                match p.first().copied().unwrap_or(0) {
                    25 => self.cursor_an = an,
                    1049 => {
                        if an && self.gemerkt.is_none() {
                            self.gemerkt = Some((self.zellen.clone(), self.x, self.y));
                            self.zellen.fill(LEER);
                        } else if !an && let Some((z, x, y)) = self.gemerkt.take() {
                            self.zellen = z;
                            self.x = x;
                            self.y = y;
                        }
                        self.alles_markieren();
                    }
                    // ?1 (Cursortasten der Anwendung): Tasten kommen ohnehin
                    // als erweiterte Codes
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn schreiben(&mut self, b: &[u8]) {
        for &c in b {
            self.zeichen(c);
        }
        self.zeichnen();
    }

    /// Fensterrahmen (Funktion 0) – beim Anlegen und bei Ereignis 1.
    fn rahmen(&mut self) {
        let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
        let (cw, ch) = (self.w as i32 * ZB, self.h as i32 * ZH);
        sys::neuzeichnen(1);
        let titel = if self.fertig { format!("{} [Finished]", self.titel) } else { self.titel.clone() };
        sys::fenster((self.fx as u32) << 16 | (cw + 9) as u32, (self.fy as u32) << 16 | (ch + skin + 4) as u32, 0x34 << 24, 0, Some(&titel));
        sys::neuzeichnen(2);
        // Funktion 0 setzt den Titel nur beim ersten Mal („[Finished]“)
        if self.fertig {
            sys::titel(&titel);
        }
        self.alles_markieren();
        self.marke = None;
    }

    /// Geänderte Zellen zeichnen (je Zeile ein Bild), dann die Schreibmarke.
    fn zeichnen(&mut self) {
        // alte Schreibmarke entfernen
        if let Some((mx, my)) = self.marke.take() {
            self.markiere(my, mx, mx + 1);
        }
        let mut puffer = Vec::new();
        for y in 0..self.h {
            let Some((x0, x1)) = self.schmutz[y].take() else { continue };
            let x1 = x1.min(self.w);
            let bw = (x1 - x0) * ZB as usize;
            puffer.clear();
            puffer.resize(bw * ZH as usize * 3, 0);
            for x in x0..x1 {
                let c = self.zellen[y * self.w + x];
                let (vg, hg) = (FARBEN[(c.a & 0x0F) as usize], FARBEN[(c.a >> 4) as usize]);
                let g = kern::schrift::glyphe_klein(c.z);
                for (r, &bits) in g.iter().enumerate() {
                    for s in 0..ZB as usize {
                        let farbe = if bits >> s & 1 != 0 { vg } else { hg };
                        let o = (r * bw + (x - x0) * ZB as usize + s) * 3;
                        puffer[o..o + 3].copy_from_slice(&farbe);
                    }
                }
            }
            sys::bild(&puffer, (bw as u32) << 16 | ZH as u32, ((x0 as i32 * ZB) as u32) << 16 | (y as i32 * ZH) as u32);
        }
        if self.cursor_an && self.cursor_h > 0 && self.x < self.w && self.y < self.h && !self.fertig {
            let c = self.zellen[self.y * self.w + self.x];
            let px = self.x as i32 * ZB;
            let py = self.y as i32 * ZH + ZH - self.cursor_h;
            let f = FARBEN[(c.a & 0x0F) as usize];
            let farbe = u32::from(f[2]) << 16 | u32::from(f[1]) << 8 | u32::from(f[0]);
            sys::rechteck((px as u32) << 16 | ZB as u32, (py as u32) << 16 | self.cursor_h as u32, farbe);
            self.marke = Some((self.x, self.y));
        }
        // Bildschirmtastatur: Zeile der Schreibmarke frei halten
        if self.tastatur {
            sys::textfeld_lage(0, self.y as i32 * ZH, self.w as i32 * ZB, ZH);
        }
    }

    /// Ereignisse abarbeiten; `warten`: bis eine Taste kommt. Liefert die
    /// Taste (Funktion 2: ASCII in Bits 8–15) oder None.
    fn ereignisse(&mut self, warten: bool) -> Option<u32> {
        loop {
            let e = if warten { sys::warte_ereignis() } else { sys::pruefe_ereignis() };
            match e {
                0 => return None,
                1 => {
                    self.rahmen();
                    self.zeichnen();
                }
                2 => {
                    let t = sys::taste();
                    if t & 0xFF == 0 {
                        if warten {
                            return Some(t);
                        }
                        // beim Schreiben: Taste aufheben, nicht verlieren
                        self.vorrat.push_back(getch2_aus(t));
                    }
                }
                3 => {
                    if sys::knopf_id() >> 8 == 1 {
                        // Kreuz: Konsole zu, Programm endet (Touch-Bedienung)
                        self.flags |= FENSTER_ZU;
                        sys::textfeld_fokus(false);
                        sys::ende();
                    }
                }
                6 => {
                    // Antippen ins Fenster: Bildschirmtastatur wieder zeigen
                    // (nur bei Programmen, die Eingaben lesen)
                    if self.tastatur && sys::maus(2) & 1 != 0 {
                        sys::textfeld_fokus(true);
                        self.zeichnen();
                    }
                }
                _ => {}
            }
        }
    }

    /// Taste lesen wie con_getch2: Bits 0–7 ASCII (0 bei erweiterten
    /// Tasten), Bits 8–15 Scancode.
    fn taste2(&mut self) -> u16 {
        if let Some(t) = self.vorrat.pop_front() {
            return t;
        }
        // wartet auf eine Taste: jetzt die Bildschirmtastatur
        if !self.tastatur {
            self.tastatur = true;
            sys::textfeld_fokus(true);
            sys::textfeld_lage(0, self.y as i32 * ZH, self.w as i32 * ZB, ZH);
        }
        getch2_aus(self.ereignisse(true).unwrap_or(0))
    }
}

/// Taste aus Funktion 2 (ASCII in Bits 8–15, Scancode 16–23) im Format
/// von con_getch2.
fn getch2_aus(t: u32) -> u16 {
    let a = (t >> 8 & 0xFF) as u8;
    let scan = (t >> 16 & 0xFF) as u8;
    match erweitert(a) {
        Some(s) => u16::from(s) << 8,
        None => u16::from(scan) << 8 | u16::from(a),
    }
}

/// KolibriOS-ASCII-Codes der Sondertasten → Scancode (wie DOS/BIOS).
fn erweitert(a: u8) -> Option<u8> {
    Some(match a {
        0xB0 => 0x4B, // links
        0xB1 => 0x50, // runter
        0xB2 => 0x48, // hoch
        0xB3 => 0x4D, // rechts
        0xB4 => 0x47, // Pos1
        0xB5 => 0x4F, // Ende
        0xB6 => 0x53, // Entf
        0xB7 => 0x51, // Bild runter
        0xB8 => 0x49, // Bild hoch
        0xB9 => 0x52, // Einfg
        _ => return None,
    })
}

ffi_fn! {
/// Hat der eigene Faden eine Konsole? (stdout/stdin der libc, c/libc_zusatz.c)
#[unsafe(no_mangle)]
pub fn kos_konsole_da() -> c_int {
    c_int::from(faden_konsole().is_some())
}
}

unsafe fn c_text<'a>(s: *const c_char) -> &'a [u8] {
    if s.is_null() {
        return &[];
    }
    // SAFETY: nullterminierte Zeichenkette des Aufrufers.
    unsafe { core::ffi::CStr::from_ptr(s) }.to_bytes()
}

ffi_fn! {
/// con_init(wnd_w, wnd_h, scr_w, scr_h, title); -1 = Standard. 0 = ok.
///
/// # Safety
/// `titel` ist null oder eine nullterminierte Zeichenkette.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_init(ww: u32, wh: u32, sw: u32, sh: u32, titel: *const c_char) -> c_int {
    if faden_konsole().is_some() {
        return 0;
    }
    let wahl = |a: u32, b: u32, std: usize| {
        let v = if a == u32::MAX { b } else { a };
        if v == u32::MAX || v == 0 { std } else { (v as usize).clamp(10, 200) }
    };
    // Bildschirmpuffer = sichtbarer Bereich (kein Zurückblättern)
    let w = wahl(ww, sw, STANDARD_W);
    let h = wahl(wh, sh, STANDARD_H).min(60);
    // SAFETY: Vertrag des Aufrufers.
    let t = String::from_utf8_lossy(unsafe { c_text(titel) }).into_owned();
    let mut k = Box::new(Konsole::neu(w, h, if t.is_empty() { "Console".into() } else { t }));
    // mittig oben (die Bildschirmtastatur kommt unten)
    let bw = (sys::bildschirmgroesse() >> 16) as i32 + 1;
    k.fx = ((bw - (w as i32 * ZB + 10)) / 2).max(0);
    k.fy = 4;
    sys::maske(0x27);
    k.rahmen();
    k.zeichnen();
    if !kern::bibliothek_setzen(Box::into_raw(k) as usize) {
        return -1;
    }
    0
}
}

ffi_fn! {
/// con_exit(bCloseWindow): 0 = Fenster bleibt („[Finished]“) bis zum
/// Schliessen. Abweichung (PLAN): kehrt dann nicht zurück – das Kreuz
/// beendet das Programm; Code nach con_exit(0) läuft nicht mehr.
#[unsafe(no_mangle)]
pub fn kos_con_exit(schliessen: c_int) {
    let Some(k) = faden_konsole() else { return };
    sys::textfeld_fokus(false);
    if schliessen == 0 {
        k.fertig = true;
        k.rahmen();
        k.zeichnen();
        // bis zum Kreuz (beendet dort das Programm)
        loop {
            k.ereignisse(true);
        }
    }
    kern::bibliothek_setzen(0);
    // SAFETY: aus Box::into_raw in kos_con_init, danach nicht mehr benutzt.
    drop(unsafe { Box::from_raw(k as *mut Konsole) });
}
}

ffi_fn! {
/// # Safety
/// nullterminierte Zeichenkette.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_set_title(t: *const c_char) {
    let Some(k) = faden_konsole() else { return };
    // SAFETY: Vertrag des Aufrufers.
    k.titel = String::from_utf8_lossy(unsafe { c_text(t) }).into_owned();
    sys::titel(&k.titel);
}
}

ffi_fn! {
/// # Safety
/// nullterminierte Zeichenkette.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_write_asciiz(s: *const c_char) {
    if let Some(k) = faden_konsole() {
        // SAFETY: Vertrag des Aufrufers.
        let t = unsafe { c_text(s) };
        if let Some(a) = k.aufnahme.as_mut() {
            aufnehmen(a, t);
            return;
        }
        k.schreiben(t);
        k.ereignisse(false);
    }
}
}

ffi_fn! {
/// # Safety
/// `s` zeigt auf `n` lesbare Bytes.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_write_string(s: *const c_char, n: u32) {
    if let Some(k) = faden_konsole()
        && !s.is_null()
    {
        // SAFETY: Vertrag des Aufrufers.
        let t = unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n as usize) };
        if let Some(a) = k.aufnahme.as_mut() {
            aufnehmen(a, t);
            return;
        }
        k.schreiben(t);
        k.ereignisse(false);
    }
}
}

/// D1: höchstens so viel Ausgabe auffangen (Heap der Shell; Rest verworfen,
/// wie `ZS_MAX` in c/shell_zusatz.c).
const AUFNAHME_MAX: usize = 256 * 1024;

fn aufnehmen(a: &mut Vec<u8>, t: &[u8]) {
    let n = t.len().min(AUFNAHME_MAX.saturating_sub(a.len()));
    if n > 0 && a.try_reserve(n).is_ok() {
        a.extend_from_slice(&t[..n]);
    }
}

ffi_fn! {
/// D1: Ausgabe der eigenen Konsole ab jetzt auffangen (Umleitung/Pipe der
/// Shell, c/shell_zusatz.c) statt anzeigen. Nur dieser Faden.
#[unsafe(no_mangle)]
pub fn kos_konsole_aufnahme_start() {
    if let Some(k) = faden_konsole() {
        k.aufnahme = Some(Vec::new());
    }
}
}

ffi_fn! {
/// D1: Länge der bisher aufgefangenen Ausgabe in Bytes.
#[unsafe(no_mangle)]
pub fn kos_konsole_aufnahme_laenge() -> c_int {
    faden_konsole().and_then(|k| k.aufnahme.as_ref()).map_or(0, |a| a.len().min(c_int::MAX as usize) as c_int)
}
}

ffi_fn! {
/// D1: Auffangen beenden; höchstens `max` Bytes nach `ziel` (null ist
/// erlaubt), Rückgabe: kopierte Bytes. Danach wird wieder angezeigt.
///
/// # Safety
/// `ziel` ist null oder zeigt auf `max` beschreibbare Bytes.
#[unsafe(no_mangle)]
pub unsafe fn kos_konsole_aufnahme_ende(ziel: *mut c_char, max: c_int) -> c_int {
    let Some(a) = faden_konsole().and_then(|k| k.aufnahme.take()) else { return 0 };
    if ziel.is_null() {
        return 0;
    }
    let n = a.len().min(max.max(0) as usize);
    if n > 0 {
        // SAFETY: Vertrag des Aufrufers.
        unsafe { core::ptr::copy_nonoverlapping(a.as_ptr(), ziel.cast::<u8>(), n) };
    }
    n as c_int
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_get_flags() -> u32 {
    faden_konsole().map_or(7, |k| k.flags)
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_set_flags(neu: u32) -> u32 {
    let Some(k) = faden_konsole() else { return 7 };
    let alt = k.flags;
    k.flags = (neu & 0x1FF) | (alt & FENSTER_ZU);
    alt
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_get_font_height() -> c_int {
    ZH
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_get_cursor_height() -> c_int {
    faden_konsole().map_or(0, |k| k.cursor_h)
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_set_cursor_height(h: c_int) -> c_int {
    let Some(k) = faden_konsole() else { return 0 };
    let alt = k.cursor_h;
    if (0..ZH).contains(&h) {
        k.cursor_h = h;
        k.zeichnen();
    }
    alt
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_getch() -> c_int {
    let Some(k) = faden_konsole() else { return 0 };
    if let Some(s) = k.warte_scancode.take() {
        return c_int::from(s);
    }
    k.zeichnen();
    let t = k.taste2();
    let a = (t & 0xFF) as u8;
    if a == 0 {
        k.warte_scancode = Some((t >> 8) as u8);
        return 0;
    }
    c_int::from(a)
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_getch2() -> u16 {
    let Some(k) = faden_konsole() else { return 0 };
    k.zeichnen();
    k.taste2()
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_kbhit() -> c_int {
    let Some(k) = faden_konsole() else { return 1 };
    if k.warte_scancode.is_some() || !k.vorrat.is_empty() {
        return 1;
    }
    // Ereignisse ohne Warten: Tasten landen im Vorrat
    k.ereignisse(false);
    c_int::from(!k.vorrat.is_empty())
}
}

/// Rückruf von con_gets2 (`__stdcall`): Taste, Zeiger auf Zeile, Grösse,
/// Lage. 0 = unverändert, 1 = geändert (alte Zeile löschen, neu zeigen),
/// 2 = geändert (zeigen), 3 = sofort zurück.
#[cfg(not(target_os = "espidf"))]
pub type Rueckruf = unsafe extern "system-unwind" fn(c_int, *mut *mut c_char, *mut c_int, *mut c_int) -> c_int;
#[cfg(target_os = "espidf")]
pub type Rueckruf = unsafe extern "system" fn(c_int, *mut *mut c_char, *mut c_int, *mut c_int) -> c_int;

/// Zeile lesen mit Bearbeitung (links/rechts, Pos1/Ende, Entf, Löschen);
/// unbekannte Tasten an `rr`. Ergebnis mit '\n', nullterminiert.
///
/// # Safety
/// `str` zeigt auf `n` beschreibbare Bytes.
unsafe fn zeile_lesen(rr: Option<Rueckruf>, mut str: *mut c_char, mut n: c_int) -> *mut c_char {
    let Some(k) = faden_konsole() else { return core::ptr::null_mut() };
    if str.is_null() || n < 1 {
        return core::ptr::null_mut();
    }
    // SAFETY: Vertrag des Aufrufers.
    unsafe { *str = 0 };
    let (sx, mut sy) = (k.x, k.y);
    let mut pos: c_int = 0;
    let laenge = |p: *mut c_char| -> c_int {
        // SAFETY: nullterminiert (wird hier immer so gehalten).
        unsafe { core::ffi::CStr::from_ptr(p) }.to_bytes().len() as c_int
    };
    let mut alt_laenge = 0;
    loop {
        // Zeile ab der Startlage neu zeigen (alte Länge überschreiben)
        let l = laenge(str);
        // SAFETY: l Bytes gültig.
        let text = unsafe { core::slice::from_raw_parts(str.cast::<u8>(), l as usize) }.to_vec();
        k.x = sx;
        k.y = sy;
        let vorher = k.flags;
        let gerollt = k.gerollt;
        k.flags |= IGNORIERE_SONDER;
        for &c in &text {
            k.zeichen_setzen(c);
        }
        for _ in l..alt_laenge {
            k.zeichen_setzen(b' ');
        }
        k.flags = vorher;
        // Umbruch in der letzten Zeile hat gerollt: Zeilenanfang mitnehmen
        sy = sy.saturating_sub(k.gerollt - gerollt);
        alt_laenge = l;
        // Schreibmarke an pos (Umbruch über die Zeilenbreite beachten)
        let ziel = sx + pos as usize;
        k.x = ziel % k.w;
        k.y = (sy + ziel / k.w).min(k.h - 1);
        k.zeichnen();
        let t = k.taste2();
        if k.flags & FENSTER_ZU != 0 {
            return core::ptr::null_mut();
        }
        let a = (t & 0xFF) as u8;
        let scan = (t >> 8) as u8;
        // SAFETY: str hat n Bytes, l < n.
        let zeile = unsafe { core::slice::from_raw_parts_mut(str.cast::<u8>(), n as usize) };
        match (a, scan) {
            (13 | 10, _) => {
                if l + 1 < n {
                    zeile[l as usize] = b'\n';
                    zeile[l as usize + 1] = 0;
                }
                // Ende der Eingabe: Schreibmarke hinter die Zeile, neue Zeile
                let ende = sx + l as usize;
                k.x = ende % k.w;
                k.y = (sy + ende / k.w).min(k.h - 1);
                k.schreiben(b"\n");
                return str;
            }
            (8, _) if pos > 0 => {
                zeile.copy_within(pos as usize..=l as usize, pos as usize - 1);
                pos -= 1;
            }
            (0, 0x4B) => pos = (pos - 1).max(0),
            (0, 0x4D) => pos = (pos + 1).min(l),
            (0, 0x47) => pos = 0,
            (0, 0x4F) => pos = l,
            (0, 0x53) if pos < l => zeile.copy_within(pos as usize + 1..=l as usize, pos as usize),
            (c, _) if c >= 0x20 && c != 0x7F && l + 1 < n => {
                zeile.copy_within(pos as usize..=l as usize, pos as usize + 1);
                zeile[pos as usize] = c;
                pos += 1;
                if l + 2 >= n {
                    // voll: wie das Original zurück nach n−1 Zeichen
                    let ende = sx + l as usize + 1;
                    k.x = ende % k.w;
                    k.y = (sy + ende / k.w).min(k.h - 1);
                    k.zeichnen();
                    return str;
                }
            }
            _ => {
                if let Some(f) = rr {
                    let code = if a == 0 { c_int::from(scan) << 8 } else { c_int::from(a) };
                    // SAFETY: Rückruf des Programms mit gültigen Zeigern.
                    let r = unsafe { f(code, &mut str, &mut n, &mut pos) };
                    pos = pos.clamp(0, laenge(str));
                    if r == 3 {
                        return str;
                    }
                }
            }
        }
    }
}

ffi_fn! {
/// # Safety
/// `str` zeigt auf `n` beschreibbare Bytes.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_gets(str: *mut c_char, n: c_int) -> *mut c_char {
    // SAFETY: Vertrag des Aufrufers.
    unsafe { zeile_lesen(None, str, n) }
}
}

ffi_fn! {
/// # Safety
/// `str` zeigt auf `n` beschreibbare Bytes, `rr` ist ein gültiger Rückruf.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_gets2(rr: Option<Rueckruf>, str: *mut c_char, n: c_int) -> *mut c_char {
    // SAFETY: Vertrag des Aufrufers.
    unsafe { zeile_lesen(rr, str, n) }
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_cls() {
    let Some(k) = faden_konsole() else { return };
    let leer = Zelle { z: b' ', a: k.attr() };
    k.zellen.fill(leer);
    k.x = 0;
    k.y = 0;
    k.alles_markieren();
    k.zeichnen();
}
}

ffi_fn! {
/// # Safety
/// `px`, `py` sind null oder beschreibbar.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_get_cursor_pos(px: *mut c_int, py: *mut c_int) {
    let (x, y) = faden_konsole().map_or((0, 0), |k| (k.x as c_int, k.y as c_int));
    // SAFETY: Vertrag des Aufrufers.
    unsafe {
        if !px.is_null() {
            *px = x;
        }
        if !py.is_null() {
            *py = y;
        }
    }
}
}

ffi_fn! {
#[unsafe(no_mangle)]
pub fn kos_con_set_cursor_pos(x: c_int, y: c_int) {
    let Some(k) = faden_konsole() else { return };
    if (0..k.w as c_int).contains(&x) {
        k.x = x as usize;
    }
    if (0..k.h as c_int).contains(&y) {
        k.y = y as usize;
    }
    k.zeichnen();
}
}

ffi_fn! {
/// con_get_input: verfügbare Tasten als Zeichen bzw. Esc-Sequenzen
/// (Pfeile `Esc[A` usw.) in `buf`, ohne zu warten. Liefert die Anzahl Bytes.
///
/// # Safety
/// `buf` zeigt auf `n` beschreibbare Bytes.
#[unsafe(no_mangle)]
pub unsafe fn kos_con_get_input(buf: *mut c_char, n: c_int) -> c_int {
    if buf.is_null() || n <= 0 {
        return 0;
    }
    let mut aus = Vec::new();
    while kos_con_kbhit() != 0 {
        let Some(k) = faden_konsole() else { break };
        if k.flags & FENSTER_ZU != 0 {
            break;
        }
        // zweite Hälfte einer erweiterten Taste aus getch zuerst
        let t = match k.warte_scancode.take() {
            Some(sc) => u16::from(sc) << 8,
            None => k.taste2(),
        };
        let a = (t & 0xFF) as u8;
        let folge: &[u8] = match (a, (t >> 8) as u8) {
            (0, 0x48) => b"\x1b[A",
            (0, 0x50) => b"\x1b[B",
            (0, 0x4D) => b"\x1b[C",
            (0, 0x4B) => b"\x1b[D",
            (0, 0x47) => b"\x1b[H",
            (0, 0x4F) => b"\x1b[F",
            (0, 0x53) => b"\x1b[3~",
            (0, 0x49) => b"\x1b[5~",
            (0, 0x51) => b"\x1b[6~",
            (0, 0x52) => b"\x1b[2~",
            (0, _) => b"",
            _ => core::slice::from_ref(&a),
        };
        if aus.len() + folge.len() > n as usize {
            // passt nicht mehr: für den nächsten Aufruf zurücklegen
            k.vorrat.push_front(t);
            break;
        }
        aus.extend_from_slice(folge);
    }
    // SAFETY: aus.len() ≤ n.
    unsafe { core::ptr::copy_nonoverlapping(aus.as_ptr(), buf.cast::<u8>(), aus.len()) };
    aus.len() as c_int
}
}
