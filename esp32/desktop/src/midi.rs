//! D2: MIDI-Dateien (Standard MIDI File, Format 0 und 1) im Musikplayer.
//!
//! Ein eigener kleiner Synthesizer statt Klangbank (keine General-MIDI-
//! Samples im Speicher): je Note eine Stimme aus Dreieck mit etwas
//! Rechteck und einer Hüllkurve (Anschlag, Abklingen, Halten, Loslassen),
//! Kanal 10 (Schlagzeug) als kurzes Rauschen. Tempowechsel (Meta 0x51)
//! werden beachtet, Programmwechsel, Regler und Tonhöhenrad nicht. Ausgabe
//! mono mit [`RATE`]; der Player rechnet auf die Ausgaberate um.

/// Abtastrate der Ausgabe.
pub const RATE: u32 = 22_050;
/// Höchstzahl gleichzeitiger Stimmen (älteste wird ersetzt).
const STIMMEN: usize = 24;

/// Ein Ereignis auf der Zeitachse (in Abtastwerten).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ereignis {
    An { kanal: u8, note: u8, staerke: u8 },
    Aus { kanal: u8, note: u8 },
}

#[derive(Clone, Copy)]
struct Stimme {
    kanal: u8,
    note: u8,
    /// Phase 0..1 (Ton) bzw. Zufallszustand (Schlagzeug)
    phase: f32,
    schritt: f32,
    staerke: f32,
    /// Abtastwerte seit Anschlag / seit Loslassen
    alter: u32,
    los: Option<u32>,
    zufall: u32,
}

/// Dekodierte MIDI-Datei: Ereignisse mit Zeitpunkt, laufender Synthesizer.
pub struct MidiLeser {
    ereignisse: Vec<(u64, Ereignis)>,
    naechstes: usize,
    zeit: u64,
    ende: u64,
    stimmen: Vec<Stimme>,
}

fn varlen(d: &[u8], i: &mut usize) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let b = *d.get(*i)?;
        *i += 1;
        v = v << 7 | u32::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn be32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

impl MidiLeser {
    /// Ganze Datei einlesen; Fehler in Worten.
    pub fn neu(d: &[u8]) -> Result<MidiLeser, String> {
        if d.get(0..4) != Some(b"MThd") || be32(d, 4) != Some(6) {
            return Err(kern::sprache::t("not a MIDI file", "keine MIDI-Datei").into());
        }
        let format = u16::from_be_bytes([d[8], d[9]]);
        let spuren = u16::from_be_bytes([d[10], d[11]]);
        let teilung = u16::from_be_bytes([d[12], d[13]]);
        if format > 1 || teilung & 0x8000 != 0 || teilung == 0 {
            return Err(format!("{} (Format {format})", kern::sprache::t("not supported", "nicht unterstützt")));
        }
        // alle Spuren: (Tick, Reihenfolge, Ereignis oder Tempo)
        enum Roh {
            E(Ereignis),
            Tempo(u32),
        }
        let mut roh: Vec<(u64, usize, Roh)> = Vec::new();
        let mut i = 14usize;
        for _ in 0..spuren {
            if d.get(i..i + 4) != Some(b"MTrk") {
                break;
            }
            let laenge = be32(d, i + 4).ok_or("MTrk")? as usize;
            let (start, ende) = (i + 8, (i + 8 + laenge).min(d.len()));
            let spur = &d[start..ende];
            let (mut j, mut tick, mut status) = (0usize, 0u64, 0u8);
            while j < spur.len() {
                let Some(dt) = varlen(spur, &mut j) else { break };
                tick += u64::from(dt);
                let Some(&b) = spur.get(j) else { break };
                if b & 0x80 != 0 {
                    j += 1;
                    if b < 0xF0 {
                        status = b;
                    } else if b == 0xFF {
                        let Some(&typ) = spur.get(j) else { break };
                        j += 1;
                        let Some(n) = varlen(spur, &mut j) else { break };
                        let n = n as usize;
                        if typ == 0x51 && n == 3 && j + 3 <= spur.len() {
                            let us = u32::from(spur[j]) << 16 | u32::from(spur[j + 1]) << 8 | u32::from(spur[j + 2]);
                            roh.push((tick, roh.len(), Roh::Tempo(us)));
                        }
                        j += n;
                        if typ == 0x2F {
                            break;
                        }
                        continue;
                    } else if b == 0xF0 || b == 0xF7 {
                        let Some(n) = varlen(spur, &mut j) else { break };
                        j += n as usize;
                        continue;
                    } else {
                        continue;
                    }
                }
                // Daten mit (laufendem) Status
                let kanal = status & 0x0F;
                let a = *spur.get(j).unwrap_or(&0);
                match status & 0xF0 {
                    0x80 => {
                        roh.push((tick, roh.len(), Roh::E(Ereignis::Aus { kanal, note: a & 0x7F })));
                        j += 2;
                    }
                    0x90 => {
                        let v = *spur.get(j + 1).unwrap_or(&0);
                        let e = if v == 0 { Ereignis::Aus { kanal, note: a & 0x7F } } else { Ereignis::An { kanal, note: a & 0x7F, staerke: v & 0x7F } };
                        roh.push((tick, roh.len(), Roh::E(e)));
                        j += 2;
                    }
                    0xA0 | 0xB0 | 0xE0 => j += 2,
                    0xC0 | 0xD0 => j += 1,
                    _ => j += 1,
                }
            }
            i = start + laenge;
        }
        roh.sort_by_key(|(t, n, _)| (*t, *n));
        // Ticks → Abtastwerte mit Tempowechseln (Standard 500000 µs/Viertel)
        let mut us_je_viertel = 500_000f64;
        let (mut letzter_tick, mut zeit_s) = (0u64, 0f64);
        let mut ereignisse = Vec::with_capacity(roh.len());
        for (tick, _, r) in roh {
            zeit_s += (tick - letzter_tick) as f64 * us_je_viertel / 1e6 / f64::from(teilung);
            letzter_tick = tick;
            match r {
                Roh::Tempo(us) if us > 0 => us_je_viertel = f64::from(us),
                Roh::Tempo(_) => {}
                Roh::E(e) => ereignisse.push(((zeit_s * f64::from(RATE)) as u64, e)),
            }
        }
        if ereignisse.is_empty() {
            return Err(kern::sprache::t("no notes", "keine Noten").into());
        }
        // Ende: letztes Ereignis plus Ausklingen
        let ende = ereignisse.last().map_or(0, |e| e.0) + u64::from(RATE) / 2;
        Ok(MidiLeser { ereignisse, naechstes: 0, zeit: 0, ende, stimmen: Vec::with_capacity(STIMMEN) })
    }

    /// Länge in Sekunden.
    pub fn sekunden(&self) -> u32 {
        (self.ende / u64::from(RATE)) as u32
    }

    fn anschlagen(&mut self, kanal: u8, note: u8, staerke: u8) {
        let frequenz = 440.0 * 2f32.powf((f32::from(note) - 69.0) / 12.0);
        let s = Stimme {
            kanal,
            note,
            phase: 0.0,
            schritt: frequenz / RATE as f32,
            staerke: f32::from(staerke) / 127.0,
            alter: 0,
            los: None,
            zufall: 0x1234_5678 ^ u32::from(note) << 8,
        };
        if self.stimmen.len() >= STIMMEN {
            // älteste ersetzen
            if let Some((k, _)) = self.stimmen.iter().enumerate().max_by_key(|(_, v)| v.alter) {
                self.stimmen[k] = s;
            }
        } else {
            self.stimmen.push(s);
        }
    }

    /// Hüllkurve 0..1 (Schlagzeug: kurz und ohne Halten).
    fn huelle(s: &Stimme) -> f32 {
        let ms = s.alter as f32 * 1000.0 / RATE as f32;
        if s.kanal == 9 {
            return (1.0 - ms / 120.0).max(0.0);
        }
        let an = if ms < 5.0 { ms / 5.0 } else { 0.6 + 0.4 * (-(ms - 5.0) / 250.0).exp() };
        match s.los {
            None => an,
            Some(l) => {
                let seit = (s.alter - l) as f32 * 1000.0 / RATE as f32;
                an * (1.0 - seit / 120.0).max(0.0)
            }
        }
    }

    /// Bis zu `pcm.len()` Abtastwerte (mono) erzeugen; 0 = Ende.
    pub fn lesen(&mut self, pcm: &mut [i16]) -> usize {
        let mut n = 0;
        while n < pcm.len() && self.zeit < self.ende {
            while self.naechstes < self.ereignisse.len() && self.ereignisse[self.naechstes].0 <= self.zeit {
                match self.ereignisse[self.naechstes].1 {
                    Ereignis::An { kanal, note, staerke } => self.anschlagen(kanal, note, staerke),
                    Ereignis::Aus { kanal, note } => {
                        for s in self.stimmen.iter_mut().filter(|s| s.kanal == kanal && s.note == note && s.los.is_none()) {
                            s.los = Some(s.alter);
                        }
                    }
                }
                self.naechstes += 1;
            }
            let mut summe = 0f32;
            for s in &mut self.stimmen {
                let h = Self::huelle(s);
                let w = if s.kanal == 9 {
                    s.zufall ^= s.zufall << 13;
                    s.zufall ^= s.zufall >> 17;
                    s.zufall ^= s.zufall << 5;
                    (s.zufall as f32 / u32::MAX as f32) * 2.0 - 1.0
                } else {
                    let dreieck = 4.0 * (s.phase - 0.5).abs() - 1.0;
                    let rechteck = if s.phase < 0.5 { 1.0 } else { -1.0 };
                    0.8 * dreieck + 0.2 * rechteck
                };
                summe += w * h * s.staerke;
                s.phase = (s.phase + s.schritt).fract();
                s.alter += 1;
            }
            // fertige Stimmen entfernen
            self.stimmen.retain(|s| Self::huelle(s) > 0.0 || (s.alter < 4 && s.los.is_none()));
            pcm[n] = (summe * 0.25 * 32767.0).clamp(-32768.0, 32767.0) as i16;
            n += 1;
            self.zeit += 1;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kleine MIDI-Datei: Tempo 120, a' (440 Hz) eine halbe Sekunde, dann
    /// eine Trommel (Kanal 10).
    fn datei() -> Vec<u8> {
        let mut spur = vec![
            0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // Tempo 500000 µs
            0x00, 0x90, 69, 100, // a' an
        ];
        spur.extend([0x83, 0x60, 0x80, 69, 0]); // nach 480 Ticks (= 0,5 s) aus
        spur.extend([0x00, 0x99, 36, 120, 0x60, 0x89, 36, 0]); // Trommel
        spur.extend([0x00, 0xFF, 0x2F, 0x00]);
        let mut d = b"MThd\0\0\0\x06\0\0\0\x01\x01\xE0".to_vec(); // Format 0, 480 Ticks/Viertel
        d.extend(b"MTrk");
        d.extend((spur.len() as u32).to_be_bytes());
        d.extend(spur);
        d
    }

    #[test]
    fn midi_spielt_note_und_trommel() {
        let mut m = MidiLeser::neu(&datei()).unwrap();
        let mut pcm = vec![0i16; RATE as usize * 2];
        let n = m.lesen(&mut pcm);
        // 0,5 s Note + Trommel + 0,5 s Ausklingen
        assert!((RATE as usize..=RATE as usize * 2).contains(&n), "{n} Abtastwerte");
        // erste 0,4 s: Grundton 440 Hz (Nulldurchgänge nach oben)
        let s = &pcm[RATE as usize / 20..RATE as usize * 9 / 20];
        let hoch = s.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count() as f32;
        let hz = hoch / (s.len() as f32 / RATE as f32);
        assert!((420.0..460.0).contains(&hz), "Grundton {hz} Hz");
        assert!(s.iter().any(|&v| v.abs() > 2000), "hörbar laut");
        assert_eq!(m.lesen(&mut pcm), 0, "Ende");
    }

    #[test]
    fn keine_midi_datei() {
        assert!(MidiLeser::neu(b"RIFF....").is_err());
        let mut d = datei();
        d[13] = 0; // Teilung 0
        d[12] = 0;
        assert!(MidiLeser::neu(&d).is_err());
    }
}
