//! Akkuanzeige: Ladestand aus der Spannung einer 1-Zellen-LiPo-Batterie.
//!
//! Die Firmware misst die Spannung über einen Spannungsteiler an einem
//! ADC-Pin (optional, siehe README). Die Umrechnung in Prozent ist eine
//! übliche Näherung der Entladekurve (Ruhespannung); unter Last (Display,
//! Lautsprecher) liegt die Spannung etwas tiefer, daher wird geglättet.

/// Stützstellen (mV, %) der Entladekurve einer LiPo-Zelle (Näherung).
const CURVE: [(u32, u8); 12] = [
    (3300, 0),
    (3500, 5),
    (3600, 10),
    (3700, 20),
    (3750, 30),
    (3800, 40),
    (3850, 50),
    (3900, 60),
    (3950, 70),
    (4000, 80),
    (4100, 90),
    (4200, 100),
];

/// Ladestand in Prozent; `None` außerhalb von 2,8–4,5 V (kein Akku
/// angeschlossen oder Messung falsch).
pub fn percent(mv: u32) -> Option<u8> {
    if !(2800..=4500).contains(&mv) {
        return None;
    }
    let (first, last) = (CURVE[0], CURVE[CURVE.len() - 1]);
    if mv <= first.0 {
        return Some(0);
    }
    if mv >= last.0 {
        return Some(100);
    }
    let i = CURVE.iter().position(|&(v, _)| v >= mv).unwrap_or(CURVE.len() - 1);
    let ((v0, p0), (v1, p1)) = (CURVE[i - 1], CURVE[i]);
    Some((p0 as u32 + (mv - v0) * (p1 - p0) as u32 / (v1 - v0)) as u8)
}

/// Glättet Messwerte (Spannung springt unter Last).
#[derive(Clone, Copy, Debug, Default)]
pub struct Smoother {
    mv: Option<f32>,
}

impl Smoother {
    /// Neuer Messwert in mV → geglätteter Wert in mV.
    pub fn push(&mut self, mv: u32) -> u32 {
        let v = match self.mv {
            None => mv as f32,
            Some(old) => old + (mv as f32 - old) * 0.1,
        };
        self.mv = Some(v);
        v as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_and_limits() {
        assert_eq!(percent(4200), Some(100));
        assert_eq!(percent(4300), Some(100));
        assert_eq!(percent(3850), Some(50));
        assert_eq!(percent(3825), Some(45));
        assert_eq!(percent(3300), Some(0));
        assert_eq!(percent(3000), Some(0));
        assert_eq!(percent(500), None, "kein Akku");
        assert_eq!(percent(5000), None);
        let mut s = Smoother::default();
        assert_eq!(s.push(4000), 4000);
        assert!(s.push(3000) > 3800, "einzelner Ausreißer wird gedämpft");
    }
}
