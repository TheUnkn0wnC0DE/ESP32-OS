//! A2: Rechner (Auftrag Abschnitt 5) – Grundrechenarten wie ein
//! Taschenrechner: Eingabe von links nach rechts („12 × 3,5 =“ → 42),
//! Komma als Dezimalzeichen, C löscht alles, ← die letzte Ziffer. Grosse
//! Tasten für den Finger; Tasten der Tastatur (Ziffern, + - * /, Komma oder
//! Punkt, Eingabe, Rücktaste) gehen auch.

use kern::sys;

// Fenster (Arbeitsbereich-Koordinaten)
pub const FX: i32 = 170;
pub const FY: i32 = 4;
pub const FW: i32 = 300;
pub const FH: i32 = 306;
/// Anzeige
pub const ANZEIGE: (i32, i32, i32, i32) = (8, 3, 280, 34);
/// Tastenfeld: erste Taste, Tastengrösse, Abstand – Tasten 67×40 px
/// (11,6 × 6,9 mm, 1 px = 0,173 mm), fünf Reihen passen über die Taskleiste
pub const TASTEN_X: i32 = 8;
pub const TASTEN_Y: i32 = 40;
pub const TASTE_W: i32 = 67;
pub const TASTE_H: i32 = 40;
pub const ABSTAND: i32 = 3;

/// Tasten in Reihen (Beschriftung, Zeichen)
pub const TASTEN: [[(&str, char); 4]; 5] = [
    [("C", 'C'), ("<-", '<'), ("%", '%'), ("÷", '/')],
    [("7", '7'), ("8", '8'), ("9", '9'), ("×", '*')],
    [("4", '4'), ("5", '5'), ("6", '6'), ("-", '-')],
    [("1", '1'), ("2", '2'), ("3", '3'), ("+", '+')],
    [("0", '0'), (",", ','), ("±", 'n'), ("=", '=')],
];

const K_TASTE: u32 = 10;

/// Zuletzt angezeigter Text (für Tests und das Board-Log).
static ZULETZT: crate::Textmerker = crate::Textmerker::neu();

/// Text, den der Rechner zuletzt angezeigt hat.
pub fn anzeige_zuletzt() -> String {
    ZULETZT.holen()
}

/// Rechenwerk (ohne Oberfläche – auch für Tests).
#[derive(Default)]
pub struct Rechner {
    /// gespeicherter Wert und ausstehende Rechenart
    wert: f64,
    op: Option<char>,
    /// Eingabe als Text (Ziffern, Komma, Minus) – leer = Ergebnis zeigen
    eingabe: String,
    /// angezeigter Wert, wenn keine Eingabe läuft
    anzeige: f64,
    fehler: bool,
}

fn als_zahl(s: &str) -> f64 {
    s.replace(',', ".").parse().unwrap_or(0.0)
}

/// Zahl für die Anzeige: höchstens 12 Stellen, Komma, ohne Nullen am Ende.
pub fn formatieren(x: f64) -> String {
    if !x.is_finite() {
        return "Fehler".into();
    }
    if x != 0.0 && (x.abs() >= 1e12 || x.abs() < 1e-9) {
        return format!("{x:.6e}").replace('.', ",");
    }
    let mut s = format!("{x:.10}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".into();
    }
    // höchstens 12 Ziffern
    let ziffern = s.chars().filter(|c| c.is_ascii_digit()).count();
    if ziffern > 12 {
        let ganz = s.split('.').next().unwrap_or("").trim_start_matches('-').len();
        let nach = 12usize.saturating_sub(ganz);
        s = format!("{x:.nach$}");
        if s.contains('.') {
            s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        }
    }
    s.replace('.', ",")
}

impl Rechner {
    /// Angezeigter Text.
    pub fn anzeige(&self) -> String {
        if self.fehler {
            return "Fehler".into();
        }
        if self.eingabe.is_empty() { formatieren(self.anzeige) } else { self.eingabe.clone() }
    }

    fn rechnen(&mut self) {
        let b = if self.eingabe.is_empty() { self.anzeige } else { als_zahl(&self.eingabe) };
        let r = match self.op {
            Some('+') => self.wert + b,
            Some('-') => self.wert - b,
            Some('*') => self.wert * b,
            Some('/') => {
                if b == 0.0 {
                    self.fehler = true;
                    0.0
                } else {
                    self.wert / b
                }
            }
            _ => b,
        };
        self.wert = r;
        self.anzeige = r;
        self.eingabe.clear();
        if !r.is_finite() {
            self.fehler = true;
        }
    }

    /// Eine Taste (Zeichen aus `TASTEN` oder der Tastatur).
    pub fn taste(&mut self, c: char) {
        if self.fehler && c != 'C' {
            return;
        }
        match c {
            '0'..='9' => {
                if self.eingabe.chars().filter(|c| c.is_ascii_digit()).count() < 12 {
                    if self.eingabe == "0" {
                        self.eingabe.clear();
                    }
                    self.eingabe.push(c);
                }
            }
            ',' | '.' => {
                if self.eingabe.is_empty() {
                    self.eingabe.push('0');
                }
                if !self.eingabe.contains(',') {
                    self.eingabe.push(',');
                }
            }
            '+' | '-' | '*' | '/' => {
                // zweimal Rechenart hintereinander: nur die Art wechseln
                if self.eingabe.is_empty() && self.op.is_some() {
                    self.op = Some(c);
                    return;
                }
                self.rechnen();
                self.op = Some(c);
            }
            '=' => {
                self.rechnen();
                self.op = None;
            }
            '%' => {
                // Prozent vom gespeicherten Wert (200 + 10 % = 220)
                let b = if self.eingabe.is_empty() { self.anzeige } else { als_zahl(&self.eingabe) };
                let p = if matches!(self.op, Some('+') | Some('-')) { self.wert * b / 100.0 } else { b / 100.0 };
                self.eingabe = formatieren(p);
            }
            'n' => {
                if self.eingabe.is_empty() {
                    self.anzeige = -self.anzeige;
                    self.wert = self.anzeige;
                } else if let Some(rest) = self.eingabe.strip_prefix('-') {
                    self.eingabe = rest.to_string();
                } else {
                    self.eingabe.insert(0, '-');
                }
            }
            '<' => {
                self.eingabe.pop();
                if self.eingabe == "-" {
                    self.eingabe.clear();
                }
            }
            'C' => *self = Rechner::default(),
            _ => {}
        }
    }
}

fn text(x: i32, y: i32, s: &str, farbe: u32, gross: bool) {
    let f = if gross { 0x3100_0000 } else { 0x3000_0000 };
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, f | farbe & 0xFF_FFFF, s.as_bytes(), s.chars().count() as u32, 0);
}

/// Mitte der Taste mit Zeichen `c` (Arbeitsbereich) – für Tests/Autopilot.
pub fn tastenmitte(c: char) -> Option<(i32, i32)> {
    for (zi, reihe) in TASTEN.iter().enumerate() {
        for (si, t) in reihe.iter().enumerate() {
            if t.1 == c {
                let x = TASTEN_X + si as i32 * (TASTE_W + ABSTAND) + TASTE_W / 2;
                let y = TASTEN_Y + zi as i32 * (TASTE_H + ABSTAND) + TASTE_H / 2;
                return Some((x, y));
            }
        }
    }
    None
}

fn zeichnen(r: &Rechner) {
    sys::neuzeichnen(1);
    sys::fenster((FX as u32) << 16 | (FW - 1) as u32, (FY as u32) << 16 | (FH - 1) as u32, 0x33 << 24 | 0xE8EAEE, 0, Some(kern::sprache::t("Calculator", "Rechner")));
    anzeige_zeichnen(r);
    for (zi, reihe) in TASTEN.iter().enumerate() {
        for (si, (beschriftung, c)) in reihe.iter().enumerate() {
            let x = TASTEN_X + si as i32 * (TASTE_W + ABSTAND);
            let y = TASTEN_Y + zi as i32 * (TASTE_H + ABSTAND);
            let farbe = match c {
                '=' => 0x3A7BD5,
                '+' | '-' | '*' | '/' => 0xC9D6EA,
                'C' | '<' | '%' | 'n' => 0xD8D8D8,
                _ => 0xFFFFFF,
            };
            sys::knopf((x as u32) << 16 | (TASTE_W - 1) as u32, (y as u32) << 16 | (TASTE_H - 1) as u32, K_TASTE + (zi * 4 + si) as u32, farbe);
            let breite = beschriftung.chars().count() as i32 * 16;
            text(x + (TASTE_W - breite) / 2, y + (TASTE_H - 32) / 2, beschriftung, if *c == '=' { 0xFFFFFF } else { 0x202020 }, true);
        }
    }
    sys::neuzeichnen(2);
}

fn anzeige_zeichnen(r: &Rechner) {
    let (x, y, w, h) = ANZEIGE;
    sys::rechteck((x as u32) << 16 | w as u32, (y as u32) << 16 | h as u32, 0xFFFFFF);
    let s = r.anzeige();
    ZULETZT.setzen(&s);
    // rechtsbündig, grosse Schrift (16 px je Zeichen)
    let breite = s.chars().count() as i32 * 16;
    text(x + w - 8 - breite, y + (h - 32) / 2, &s, if r.fehler { 0xB03020 } else { 0x101010 }, true);
}

/// Programm „rechner“.
pub fn programm() {
    let mut r = Rechner::default();
    loop {
        match sys::warte_ereignis() {
            1 => zeichnen(&r),
            2 => {
                loop {
                    let c = sys::taste();
                    if c & 0xFF == 1 {
                        break;
                    }
                    let z = match (c >> 8) & 0xFF {
                        13 => '=',
                        8 => '<',
                        27 => 'C',
                        b => b as u8 as char,
                    };
                    r.taste(z);
                }
                anzeige_zeichnen(&r);
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                if id == 1 {
                    return;
                }
                if let Some(i) = id.checked_sub(K_TASTE).filter(|&i| i < 20) {
                    let c = TASTEN[i as usize / 4][i as usize % 4].1;
                    r.taste(c);
                    sys::debug_text(&format!("Rechner: {} → {}", TASTEN[i as usize / 4][i as usize % 4].0, r.anzeige()));
                    anzeige_zeichnen(&r);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rechne(tasten: &str) -> String {
        let mut r = Rechner::default();
        for c in tasten.chars() {
            r.taste(c);
        }
        r.anzeige()
    }

    #[test]
    fn grundrechenarten() {
        assert_eq!(rechne("12*3,5="), "42");
        assert_eq!(rechne("1+2*3="), "9", "von links nach rechts wie ein Taschenrechner");
        assert_eq!(rechne("10/4="), "2,5");
        assert_eq!(rechne("0,1+0,2="), "0,3");
        assert_eq!(rechne("5-8="), "-3");
        assert_eq!(rechne("7/0="), "Fehler");
        assert_eq!(rechne("7/0=C"), "0");
        assert_eq!(rechne("200+10%="), "220");
        assert_eq!(rechne("123<"), "12");
        assert_eq!(rechne("5n*2="), "-10");
        assert_eq!(rechne("2+*3="), "6", "zweite Rechenart ersetzt die erste");
        assert_eq!(rechne("1/3="), "0,3333333333");
    }
}
