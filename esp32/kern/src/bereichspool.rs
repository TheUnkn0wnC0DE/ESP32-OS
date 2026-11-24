//! M4/M6: Pool für die Speicherbereiche der Programme. Dauertest M4: Die
//! Bereiche kamen einzeln aus dem PSRAM-Heap des Systems und lagen zwischen
//! dessen Belegungen (Fenster, Abrufe, Bilder) – nach einer Stunde war der
//! grösste freie Block 6 MB, obwohl 10 MB frei waren, und der Browser (10 MB)
//! bzw. der PDF-Betrachter (8 MB) starteten nicht mehr. Der Pool ist ein
//! Block, beim ersten Programmstart einmal reserviert; darin liegen nur
//! Programmbereiche. Kleine Bereiche (< `GRENZE_KLEIN`) kommen vom oberen
//! Ende, grosse vom unteren – so zerstückeln die immer laufenden kleinen
//! Dienste (Taskleiste, Tastatur, Menü) den Platz der grossen nicht. Freie
//! Stücke werden beim Zurückgeben mit den Nachbarn verschmolzen.
//!
//! Reine Buchhaltung über Adressen (kein Zugriff auf den Speicher selbst):
//! am PC prüfbar.

/// Bereiche darunter gelten als klein (vom oberen Ende).
pub const GRENZE_KLEIN: usize = 2 * 1024 * 1024;
/// Ausrichtung der Bereiche.
pub const AUSRICHTUNG: usize = 64;

pub struct Pool {
    anfang: usize,
    ende: usize,
    /// freie Stücke (Anfang, Länge), nach Adresse sortiert, nie benachbart
    frei: Vec<(usize, usize)>,
}

fn aufrunden(n: usize) -> usize {
    n.div_ceil(AUSRICHTUNG) * AUSRICHTUNG
}

impl Pool {
    /// Pool über [anfang, anfang + laenge) (anfang ausgerichtet).
    pub fn neu(anfang: usize, laenge: usize) -> Pool {
        let a = aufrunden(anfang);
        let l = (laenge - (a - anfang)) / AUSRICHTUNG * AUSRICHTUNG;
        Pool { anfang: a, ende: a + l, frei: vec![(a, l)] }
    }

    /// Liegt `p` im Pool?
    pub fn enthaelt(&self, p: usize) -> bool {
        (self.anfang..self.ende).contains(&p)
    }

    /// Bereich der Länge `n` holen (Adresse) oder None.
    pub fn holen(&mut self, n: usize) -> Option<usize> {
        let n = aufrunden(n.max(1));
        if n < GRENZE_KLEIN {
            // klein: höchstes passendes Stück, vom oberen Ende
            let i = self.frei.iter().rposition(|&(_, l)| l >= n)?;
            let (a, l) = self.frei[i];
            if l == n {
                self.frei.remove(i);
            } else {
                self.frei[i] = (a, l - n);
            }
            Some(a + l - n)
        } else {
            // gross: niedrigstes passendes Stück, vom unteren Ende
            let i = self.frei.iter().position(|&(_, l)| l >= n)?;
            let (a, l) = self.frei[i];
            if l == n {
                self.frei.remove(i);
            } else {
                self.frei[i] = (a + n, l - n);
            }
            Some(a)
        }
    }

    /// Bereich zurückgeben (mit den Nachbarn verschmelzen).
    pub fn frei(&mut self, p: usize, n: usize) {
        let n = aufrunden(n.max(1));
        let i = self.frei.partition_point(|&(a, _)| a < p);
        self.frei.insert(i, (p, n));
        // mit dem rechten Nachbarn
        if i + 1 < self.frei.len() && self.frei[i].0 + self.frei[i].1 == self.frei[i + 1].0 {
            self.frei[i].1 += self.frei[i + 1].1;
            self.frei.remove(i + 1);
        }
        // mit dem linken Nachbarn
        if i > 0 && self.frei[i - 1].0 + self.frei[i - 1].1 == self.frei[i].0 {
            self.frei[i - 1].1 += self.frei[i].1;
            self.frei.remove(i);
        }
    }

    /// Freie Bytes und grösstes freies Stück.
    pub fn stand(&self) -> (usize, usize) {
        (self.frei.iter().map(|&(_, l)| l).sum(), self.frei.iter().map(|&(_, l)| l).max().unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MB: usize = 1024 * 1024;

    #[test]
    fn klein_oben_gross_unten_und_verschmelzen() {
        let mut p = Pool::neu(0x1000_0000, 16 * MB);
        let taskleiste = p.holen(560 * 1024).unwrap();
        let tastatur = p.holen(560 * 1024).unwrap();
        let icon = p.holen(4 * MB).unwrap();
        // klein oben, gross unten
        assert!(taskleiste > icon && tastatur > icon);
        assert_eq!(icon, 0x1000_0000);
        // Browser (10,4 MB) passt neben den Diensten
        let browser = p.holen(10 * MB + 400 * 1024).unwrap();
        p.frei(browser, 10 * MB + 400 * 1024);
        // Menü auf/zu, Musik auf/zu, viele Runden: der grosse Block bleibt
        for _ in 0..200 {
            let menue = p.holen(1076 * 1024).unwrap();
            let musik = p.holen(2308 * 1024).unwrap();
            p.frei(menue, 1076 * 1024);
            p.frei(musik, 2308 * 1024);
            let pdf = p.holen(8 * MB + 300 * 1024).unwrap();
            p.frei(pdf, 8 * MB + 300 * 1024);
        }
        let (frei, block) = p.stand();
        assert_eq!(frei, 16 * MB - 4 * MB - 2 * aufrunden(560 * 1024));
        assert_eq!(block, frei, "nach allen Runden ein einziges freies Stück");
        // alles zurück: wieder ganz
        p.frei(icon, 4 * MB);
        p.frei(tastatur, 560 * 1024);
        p.frei(taskleiste, 560 * 1024);
        assert_eq!(p.stand(), (16 * MB, 16 * MB));
    }

    #[test]
    fn zu_gross_gibt_none() {
        let mut p = Pool::neu(0x1000_0000, 4 * MB);
        assert!(p.holen(5 * MB).is_none());
        assert!(p.enthaelt(0x1000_0000) && !p.enthaelt(0x1000_0000 + 4 * MB));
    }
}
