//! Spielstand: Einstellungen, angefangene Partie, Rekorde und freigespielte
//! Level – in fester 128-Byte-Form mit Prüfsumme (CRC-32).
//!
//! Die Firmware legt ihn im NVS-Bereich des Flash ab (ESP-IDF schreibt dort
//! stromausfallsicher). Die Laufnummer und [`SaveData::newest`] erlauben
//! zusätzlich zwei Speicherplätze im Wechsel.

use crate::level::LEVELS;

pub const SAVE_SIZE: usize = 128;
pub const LEVEL_COUNT: usize = LEVELS.len();
const MAGIC: [u8; 4] = *b"PDNG";
const VERSION: u8 = 4;
/// Vorige Version (gleiches Format): wird gelesen, nur die Bildeinstellungen
/// (Auflösung, Vergrößerung) gehen einmalig auf die neuen Standardwerte.
const VERSION_V3: u8 = 3;
/// Byte mit Größe (Bit 0–1) und Deckkraft (Bit 2–4) der Touch-Steuerung.
const TOUCH_AT: usize = 35;
const RECORDS_AT: usize = 36;
const RECORD_SIZE: usize = 8;
/// Schwierigkeit der angefangenen Partie (0 leicht, 1 normal, 2 schwer).
const DIFF_AT: usize = SAVE_SIZE - 8;
const CRC_AT: usize = SAVE_SIZE - 4;

/// Einstellungen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub music: bool,
    /// 0..=10
    pub music_vol: u8,
    /// 0..=10
    pub sfx_vol: u8,
    pub show_fps: bool,
    /// 3D-Bild in 640×360 statt 320×180 („Performance“).
    pub hires: bool,
    /// Bilineare statt Nearest-Neighbor-Vergrößerung (weicher statt scharf).
    pub smooth: bool,
    /// Bild um 180° drehen (je nachdem, wie man die Platine hält).
    pub flip: bool,
    /// Display-Helligkeit 1..=10.
    pub brightness: u8,
    /// Größe der Touch-Steuerung: 0 klein, 1 mittel, 2 groß.
    pub touch_size: u8,
    /// Deckkraft der Touch-Steuerung 1..=5.
    pub touch_alpha: u8,
    /// Fester statt schwebender Steuerknüppel.
    pub stick_fixed: bool,
    /// Linkshänder: Knüppel rechts, Springen links.
    pub left_handed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            music: true,
            music_vol: 7,
            sfx_vol: 8,
            show_fps: true,
            // 640×360, vom PPA weich vergrößert: auf dem ESP32-P4 (v1.3, 360 MHz)
            // im Mittel 54,8 FPS in allen Kapiteln (scharf: 53,5, in den
            // schwersten Szenen bis 47). Android setzt beim ersten Start selbst
            // die volle Auflösung und nutzt `smooth` nicht.
            hires: true,
            smooth: true,
            flip: false,
            brightness: 8,
            touch_size: 1,
            touch_alpha: 3,
            stick_fixed: false,
            left_handed: false,
        }
    }
}

/// Eine angefangene Partie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub level: u8,
    /// 0 = Start, sonst Checkpoint-Index + 1.
    pub checkpoint: u8,
    /// Bitmaske der eingesammelten Kirschen (in Level-Reihenfolge).
    pub cherries_taken: u64,
    pub time_ms: u32,
    pub deaths: u16,
    /// Schwierigkeit (siehe `campaign::Difficulty`).
    pub difficulty: u8,
}

/// Rekord eines Levels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// Beste Zeit in ms (0 = noch nie geschafft).
    pub best_time_ms: u32,
    pub best_cherries: u8,
    pub wins: u16,
    /// Wenigste Platscher bei einem Sieg (255 = noch keiner).
    pub best_deaths: u8,
}

/// Der komplette Spielstand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveData {
    /// Laufnummer (höher = neuer).
    pub seq: u32,
    pub settings: Settings,
    pub progress: Option<Progress>,
    pub records: [Record; LEVEL_COUNT],
    /// Anzahl freigespielter Level (mindestens 1).
    pub unlocked: u8,
    /// Gesamte Spielzeit in Sekunden.
    pub play_time_s: u32,
}

impl Default for SaveData {
    fn default() -> Self {
        Self {
            seq: 0,
            settings: Settings::default(),
            progress: None,
            records: [Record { best_deaths: 255, ..Record::default() }; LEVEL_COUNT],
            unlocked: 1,
            play_time_s: 0,
        }
    }
}

/// CRC-32 (IEEE, bitweise – 128 Byte sind schnell genug).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

impl SaveData {
    /// Eine Partie gewonnen: Rekorde aktualisieren, nächstes Level freischalten.
    /// Gibt zurück, ob die Zeit ein neuer Rekord ist.
    pub fn record_win(&mut self, level: usize, time_ms: u32, cherries: u8, deaths: u32) -> bool {
        let r = &mut self.records[level];
        let new_best = r.best_time_ms == 0 || time_ms < r.best_time_ms;
        if new_best {
            r.best_time_ms = time_ms;
        }
        r.best_cherries = r.best_cherries.max(cherries);
        r.best_deaths = r.best_deaths.min(deaths.min(254) as u8);
        r.wins = r.wins.saturating_add(1);
        self.unlocked = self.unlocked.max((level as u8 + 2).min(LEVEL_COUNT as u8));
        self.progress = None;
        new_best
    }

    pub fn to_bytes(&self) -> [u8; SAVE_SIZE] {
        let mut b = [0u8; SAVE_SIZE];
        b[0..4].copy_from_slice(&MAGIC);
        b[4] = VERSION;
        b[5..9].copy_from_slice(&self.seq.to_le_bytes());
        let s = &self.settings;
        b[9] = s.music as u8
            | (s.show_fps as u8) << 1
            | (s.hires as u8) << 2
            | (s.flip as u8) << 3
            | (s.smooth as u8) << 4
            | (s.stick_fixed as u8) << 5
            | (s.left_handed as u8) << 6;
        b[TOUCH_AT] = s.touch_size.min(2) | s.touch_alpha.clamp(1, 5) << 2;
        b[10] = s.music_vol.min(10);
        b[11] = s.sfx_vol.min(10);
        b[12] = s.brightness.clamp(1, 10);
        if let Some(p) = &self.progress {
            b[13] = 1;
            b[14] = p.level;
            b[15] = p.checkpoint;
            b[16..24].copy_from_slice(&p.cherries_taken.to_le_bytes());
            b[24..28].copy_from_slice(&p.time_ms.to_le_bytes());
            b[28..30].copy_from_slice(&p.deaths.to_le_bytes());
            b[DIFF_AT] = p.difficulty.min(2);
        }
        b[30] = self.unlocked;
        b[31..35].copy_from_slice(&self.play_time_s.to_le_bytes());
        for (i, r) in self.records.iter().enumerate() {
            let o = RECORDS_AT + i * RECORD_SIZE;
            b[o..o + 4].copy_from_slice(&r.best_time_ms.to_le_bytes());
            b[o + 4] = r.best_cherries;
            b[o + 5..o + 7].copy_from_slice(&r.wins.to_le_bytes());
            b[o + 7] = r.best_deaths;
        }
        let crc = crc32(&b[..CRC_AT]);
        b[CRC_AT..].copy_from_slice(&crc.to_le_bytes());
        b
    }

    /// Liest einen Spielstand; `None` bei leerem/beschädigtem Speicher.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < SAVE_SIZE || b[0..4] != MAGIC || (b[4] != VERSION && b[4] != VERSION_V3) {
            return None;
        }
        let crc = u32::from_le_bytes([b[CRC_AT], b[CRC_AT + 1], b[CRC_AT + 2], b[CRC_AT + 3]]);
        if crc != crc32(&b[..CRC_AT]) {
            return None;
        }
        let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let mut settings = Settings {
            music: b[9] & 1 != 0,
            show_fps: b[9] & 2 != 0,
            hires: b[9] & 4 != 0,
            flip: b[9] & 8 != 0,
            smooth: b[9] & 16 != 0,
            stick_fixed: b[9] & 32 != 0,
            left_handed: b[9] & 64 != 0,
            touch_size: (b[TOUCH_AT] & 3).min(2),
            touch_alpha: ((b[TOUCH_AT] >> 2) & 7).clamp(1, 5),
            music_vol: b[10].min(10),
            sfx_vol: b[11].min(10),
            brightness: b[12].clamp(1, 10),
        };
        if b[4] == VERSION_V3 {
            let d = Settings::default();
            settings.hires = d.hires;
            settings.smooth = d.smooth;
        }
        let progress = (b[13] == 1 && (b[14] as usize) < LEVEL_COUNT).then(|| Progress {
            level: b[14],
            checkpoint: b[15],
            cherries_taken: u64::from_le_bytes([b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23]]),
            time_ms: u32_at(24),
            deaths: u16_at(28),
            difficulty: b[DIFF_AT].min(2),
        });
        let mut records = [Record::default(); LEVEL_COUNT];
        for (i, r) in records.iter_mut().enumerate() {
            let o = RECORDS_AT + i * RECORD_SIZE;
            *r =
                Record { best_time_ms: u32_at(o), best_cherries: b[o + 4], wins: u16_at(o + 5), best_deaths: b[o + 7] };
        }
        Some(Self {
            seq: u32_at(5),
            settings,
            progress,
            records,
            unlocked: b[30].clamp(1, LEVEL_COUNT as u8),
            play_time_s: u32_at(31),
        })
    }

    /// Wählt aus zwei Speicherplätzen den gültigen, neueren Stand.
    pub fn newest(a: &[u8], b: &[u8]) -> Option<Self> {
        match (Self::from_bytes(a), Self::from_bytes(b)) {
            (Some(x), Some(y)) => Some(if y.seq > x.seq { y } else { x }),
            (x, y) => x.or(y),
        }
    }
}

const _: () = assert!(RECORDS_AT + LEVEL_COUNT * RECORD_SIZE <= DIFF_AT, "Spielstand zu groß");

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein Spielstand der Version 3 wird übernommen (Fortschritt, Lautstärke
    /// …); nur Auflösung und Vergrößerung springen auf die neuen Standardwerte.
    #[test]
    fn version_3_keeps_progress_and_resets_display_mode() {
        let mut s = SaveData::default();
        s.settings.hires = false;
        s.settings.smooth = false;
        s.settings.music_vol = 2;
        s.progress = Some(Progress { level: 3, checkpoint: 2, ..Progress::default() });
        let mut b = s.to_bytes();
        b[4] = VERSION_V3;
        let crc = crc32(&b[..CRC_AT]);
        b[CRC_AT..].copy_from_slice(&crc.to_le_bytes());
        let r = SaveData::from_bytes(&b).expect("Version 3 lesbar");
        assert_eq!(r.progress.map(|p| (p.level, p.checkpoint)), Some((3, 2)));
        assert_eq!(r.settings.music_vol, 2);
        let d = Settings::default();
        assert_eq!((r.settings.hires, r.settings.smooth), (d.hires, d.smooth));
        // Gespeichert wird wieder als aktuelle Version – die Wahl bleibt dann.
        let mut again = r;
        again.settings.smooth = false;
        let r2 = SaveData::from_bytes(&again.to_bytes()).unwrap();
        assert!(!r2.settings.smooth);
    }

    #[test]
    fn roundtrip_and_corruption() {
        let mut s = SaveData { seq: 7, ..SaveData::default() };
        s.settings.music_vol = 3;
        s.settings.hires = false;
        s.settings.flip = true;
        s.settings.smooth = true;
        s.settings.touch_size = 2;
        s.settings.touch_alpha = 5;
        s.settings.stick_fixed = true;
        s.settings.left_handed = true;
        s.settings.brightness = 4;
        s.progress = Some(Progress { level: 1, checkpoint: 2, cherries_taken: 0b1011, time_ms: 61_500, deaths: 3, difficulty: 2 });
        assert!(s.record_win(0, 40_000, 20, 1));
        s.progress = Some(Progress { level: 1, checkpoint: 2, cherries_taken: 0b1011, time_ms: 61_500, deaths: 3, difficulty: 2 });
        let b = s.to_bytes();
        assert_eq!(SaveData::from_bytes(&b), Some(s));
        assert_eq!(s.unlocked, 2);
        let mut bad = b;
        bad[40] ^= 1;
        assert_eq!(SaveData::from_bytes(&bad), None);
        assert_eq!(SaveData::from_bytes(&[0xFF; SAVE_SIZE]), None);
        let older = SaveData { seq: 3, ..s }.to_bytes();
        assert_eq!(SaveData::newest(&older, &b).unwrap().seq, 7);
        assert_eq!(SaveData::newest(&bad, &older).unwrap().seq, 3);
        assert!(!s.record_win(0, 50_000, 25, 0), "langsamer ist kein Rekord");
        assert_eq!(s.records[0].best_cherries, 25);
        assert_eq!(s.records[0].best_deaths, 0);
    }
}
