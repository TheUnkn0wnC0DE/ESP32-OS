//! D2: Ogg Vorbis im Musikplayer – stb_vorbis (Sean Barrett, MIT oder
//! Public Domain, `c/stb_vorbis.c` unverändert, v1.25) über seine
//! Push-Schnittstelle: die Datei wird stückweise in ein Fenster gelesen
//! (Musikdateien sind mehrere MB, der Player hat 2 MB Heap), der Dekoder
//! arbeitet nur in einem festen Puffer im Heap des Programms
//! (`stb_vorbis_alloc`, kein malloc). Kaputte oder abgeschnittene Dateien
//! enden mit einer Meldung bzw. am Ende des Lesbaren – nie mit einem
//! Absturz: ohne Fortschritt ist Schluss, das Fenster wächst nur bis
//! [`FENSTER_MAX`].

use std::io::Read;

/// Arbeitsspeicher des Dekoders (Codebücher und Puffer der Datei; typische
/// Dateien brauchen 100–200 KB).
const ARBEIT: usize = 384 * 1024;
/// Lesefenster: so viel wird auf einmal nachgeladen.
const FENSTER: usize = 64 * 1024;
/// Grösste Fenstergrösse (eine Vorbis-Seite hat höchstens ~64 KB; ein Paket
/// über mehrere Seiten darf mehr brauchen, aber nicht unbegrenzt).
const FENSTER_MAX: usize = 512 * 1024;

#[repr(C)]
struct StbAlloc {
    alloc_buffer: *mut u8,
    alloc_buffer_length_in_bytes: i32,
}

#[repr(C)]
struct StbInfo {
    sample_rate: u32,
    channels: i32,
    setup_memory_required: u32,
    setup_temp_memory_required: u32,
    temp_memory_required: u32,
    max_frame_size: i32,
}

#[repr(C)]
struct StbVorbis {
    _p: [u8; 0],
}

/// `VORBIS_need_more_data` aus stb_vorbis.c
const MEHR_DATEN: i32 = 1;

unsafe extern "C" {
    fn stb_vorbis_open_pushdata(daten: *const u8, n: i32, verbraucht: *mut i32, fehler: *mut i32, alloc: *const StbAlloc) -> *mut StbVorbis;
    fn stb_vorbis_decode_frame_pushdata(f: *mut StbVorbis, daten: *const u8, n: i32, kanaele: *mut i32, ausgabe: *mut *mut *mut f32, proben: *mut i32) -> i32;
    fn stb_vorbis_get_info(f: *mut StbVorbis) -> StbInfo;
    fn stb_vorbis_close(f: *mut StbVorbis);
}

/// Dekodiert eine Ogg-Vorbis-Datei stückweise zu 16-Bit-PCM (verschränkt,
/// höchstens 2 Kanäle).
pub struct OggLeser<R: Read> {
    quelle: R,
    puf: Vec<u8>,
    an: usize,
    ende: usize,
    datei_zu_ende: bool,
    v: *mut StbVorbis,
    // hält den Arbeitsspeicher, solange `v` lebt (u64: 8-Byte-Ausrichtung
    // für stb_vorbis)
    _arbeit: Vec<u64>,
    pub rate: u32,
    pub kanaele: usize,
    rest: Vec<i16>,
    rest_an: usize,
    fertig: bool,
}

// SAFETY: `v` gehört allein diesem Leser.
unsafe impl<R: Read + Send> Send for OggLeser<R> {}

impl<R: Read> OggLeser<R> {
    /// Öffnet die Datei (liest die Köpfe); Fehler in Worten.
    pub fn neu(quelle: R) -> Result<OggLeser<R>, String> {
        // Speicher ohne Abbruch anfordern: zu wenig Heap ist ein Fehler, kein
        // Absturz des Players
        let voll = || kern::sprache::t("not enough memory for Ogg", "zu wenig Speicher für Ogg").to_string();
        let mut arbeit: Vec<u64> = Vec::new();
        arbeit.try_reserve_exact(ARBEIT / 8).map_err(|_| voll())?;
        arbeit.resize(ARBEIT / 8, 0);
        let mut l = OggLeser {
            quelle,
            puf: Vec::new(),
            an: 0,
            ende: 0,
            datei_zu_ende: false,
            v: core::ptr::null_mut(),
            _arbeit: arbeit,
            rate: 0,
            kanaele: 0,
            rest: Vec::new(),
            rest_an: 0,
            fertig: false,
        };
        let alloc = StbAlloc { alloc_buffer: l._arbeit.as_mut_ptr().cast(), alloc_buffer_length_in_bytes: ARBEIT as i32 };
        loop {
            if !l.nachladen() && l.ende - l.an == 0 {
                return Err(kern::sprache::t("not an Ogg Vorbis file", "keine Ogg-Vorbis-Datei").into());
            }
            let (mut verbraucht, mut fehler) = (0i32, 0i32);
            // SAFETY: Fenster gültig; der Arbeitsspeicher lebt so lange wie `l`.
            let v = unsafe { stb_vorbis_open_pushdata(l.puf[l.an..].as_ptr(), (l.ende - l.an) as i32, &mut verbraucht, &mut fehler, &alloc) };
            if !v.is_null() {
                l.v = v;
                l.an += verbraucht.max(0) as usize;
                break;
            }
            // mehr vom Anfang der Datei nötig – sonst kaputt
            if fehler != MEHR_DATEN || l.datei_zu_ende || l.ende - l.an >= FENSTER_MAX {
                return Err(format!("{} ({fehler})", kern::sprache::t("not an Ogg Vorbis file", "keine Ogg-Vorbis-Datei")));
            }
        }
        // SAFETY: `v` gültig.
        let info = unsafe { stb_vorbis_get_info(l.v) };
        if info.sample_rate == 0 || info.sample_rate > 192_000 || info.channels < 1 {
            return Err(kern::sprache::t("unsupported Ogg Vorbis file", "Ogg-Vorbis-Datei nicht unterstützt").into());
        }
        l.rate = info.sample_rate;
        l.kanaele = (info.channels as usize).min(2);
        Ok(l)
    }

    /// Fenster nach vorn schieben und nachladen; false, wenn nichts dazukam.
    fn nachladen(&mut self) -> bool {
        if self.datei_zu_ende {
            return false;
        }
        // Verbrauchtes vorn weg
        if self.an > 0 {
            self.puf.copy_within(self.an..self.ende, 0);
            self.ende -= self.an;
            self.an = 0;
        }
        if self.ende + FENSTER > FENSTER_MAX {
            return false;
        }
        if self.puf.len() < self.ende + FENSTER {
            // ohne Speicher: wie Dateiende (kein Abbruch)
            if self.puf.try_reserve_exact(self.ende + FENSTER - self.puf.len()).is_err() {
                self.datei_zu_ende = true;
                return false;
            }
            self.puf.resize(self.ende + FENSTER, 0);
        }
        let mut dazu = 0;
        while dazu < FENSTER {
            match self.quelle.read(&mut self.puf[self.ende + dazu..self.ende + FENSTER]) {
                Ok(0) | Err(_) => {
                    self.datei_zu_ende = true;
                    break;
                }
                Ok(n) => dazu += n,
            }
        }
        self.ende += dazu;
        dazu > 0
    }

    /// Nächsten Rahmen dekodieren und in `rest` legen; false am Ende.
    fn rahmen(&mut self) -> bool {
        loop {
            let (mut ch, mut aus, mut n) = (0i32, core::ptr::null_mut::<*mut f32>(), 0i32);
            // SAFETY: `v` gültig, Fenster gültig.
            let benutzt = unsafe { stb_vorbis_decode_frame_pushdata(self.v, self.puf[self.an..].as_ptr(), (self.ende - self.an) as i32, &mut ch, &mut aus, &mut n) };
            let benutzt = benutzt.max(0) as usize;
            if benutzt == 0 && n == 0 {
                // mehr Daten nötig; ohne neue Daten ist Schluss (abgeschnitten)
                if !self.nachladen() {
                    return false;
                }
                continue;
            }
            self.an = (self.an + benutzt).min(self.ende);
            if n <= 0 || aus.is_null() || ch < 1 {
                continue; // Neusynchronisation, erster Rahmen
            }
            let (n, ch) = (n as usize, ch as usize);
            let k = self.kanaele;
            self.rest.clear();
            self.rest.reserve(n * k);
            for i in 0..n {
                for c in 0..k {
                    // Mono-Datei: derselbe Kanal; sonst die ersten beiden
                    let kanal = c.min(ch - 1);
                    // SAFETY: stb_vorbis liefert `ch` Kanäle mit je `n` Proben.
                    let x = unsafe { *(*aus.add(kanal)).add(i) };
                    let x = if x.is_finite() { x } else { 0.0 };
                    self.rest.push((x * 32767.0).clamp(-32768.0, 32767.0) as i16);
                }
            }
            self.rest_an = 0;
            return true;
        }
    }

    /// Verschränkte 16-Bit-Proben in `pcm` (Anzahl; 0 = Ende).
    pub fn lesen(&mut self, pcm: &mut [i16]) -> usize {
        let mut n = 0;
        while n < pcm.len() {
            if self.rest_an >= self.rest.len() {
                if self.fertig || !self.rahmen() {
                    self.fertig = true;
                    break;
                }
            }
            let k = (self.rest.len() - self.rest_an).min(pcm.len() - n);
            pcm[n..n + k].copy_from_slice(&self.rest[self.rest_an..self.rest_an + k]);
            self.rest_an += k;
            n += k;
        }
        n
    }
}

impl<R: Read> Drop for OggLeser<R> {
    fn drop(&mut self) {
        if !self.v.is_null() {
            // SAFETY: `v` aus stb_vorbis_open_pushdata, nur hier geschlossen.
            unsafe { stb_vorbis_close(self.v) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alles(d: &[u8]) -> Result<usize, String> {
        let mut l = OggLeser::neu(std::io::Cursor::new(d.to_vec()))?;
        let mut pcm = vec![0i16; 4096];
        let mut n = 0;
        loop {
            let k = l.lesen(&mut pcm);
            if k == 0 {
                break;
            }
            n += k;
            assert!(n < 48_000 * 2 * 600, "endet nicht");
        }
        Ok(n)
    }

    /// Kein Ogg, leer, nur der Seitenkopf, Zufallsdaten: Fehler, kein Absturz.
    #[test]
    fn keine_ogg_dateien() {
        assert!(alles(b"").is_err());
        assert!(alles(b"RIFF....WAVEfmt ").is_err());
        assert!(alles(b"OggS").is_err());
        let mut z = 0x1234_5678u32;
        for laenge in [27usize, 100, 4096, 70_000] {
            let mut d: Vec<u8> = (0..laenge)
                .map(|_| {
                    z ^= z << 13;
                    z ^= z >> 17;
                    z ^= z << 5;
                    z as u8
                })
                .collect();
            d[..4].copy_from_slice(b"OggS");
            let _ = alles(&d);
        }
    }

    /// Echte Datei: ganz, an vielen Stellen abgeschnitten, mit gekippten
    /// Bytes – immer Fehler oder Ende, nie ein Absturz oder eine
    /// Endlosschleife.
    fn echt_pruefen(d: &[u8]) {
        let ganz = alles(d).expect("gültige Datei");
        assert!(ganz > 1000, "{ganz} Proben");
        for teil in 1..64 {
            let _ = alles(&d[..d.len() * teil / 64]);
        }
        let mut z = 0x9E37_79B9u32;
        for _ in 0..200 {
            let mut k = d.to_vec();
            for _ in 0..8 {
                z ^= z << 13;
                z ^= z >> 17;
                z ^= z << 5;
                let i = z as usize % k.len();
                k[i] ^= (z >> 8) as u8 | 1;
            }
            let _ = alles(&k);
        }
    }

    /// Freie Testdatei im Repo (CC0, Wikimedia Commons, siehe
    /// tests/daten/LIESMICH_OGG.txt): Vorbis, mono, etwa 2 s.
    #[test]
    fn cc0_datei_kaputt_und_abgeschnitten() {
        let d = include_bytes!("../tests/daten/cc0_daring_heart.ogg");
        let l = OggLeser::neu(std::io::Cursor::new(d.to_vec())).expect("öffnen");
        assert!(l.rate > 0 && l.kanaele >= 1, "{} Hz, {} Kanäle", l.rate, l.kanaele);
        let n = alles(d).unwrap() / l.kanaele;
        let s = n as f64 / f64::from(l.rate);
        assert!((1.5..3.0).contains(&s), "{s} s");
        echt_pruefen(d);
    }

    /// Weitere Datei nur lokal (Pfad in `OGG_TESTDATEI`).
    #[test]
    fn lokale_datei_kaputt_und_abgeschnitten() {
        let Some(pfad) = std::env::var_os("OGG_TESTDATEI") else { return };
        echt_pruefen(&std::fs::read(pfad).expect("OGG_TESTDATEI lesbar"));
    }
}
