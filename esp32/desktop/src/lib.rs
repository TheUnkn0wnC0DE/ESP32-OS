//! ESP32-OS-Desktop als `konsole::Spiel`: Touch → Kern-Eingabe, Kern-
//! Compositor (Dirty-Rects, Doppelpuffer) → Anzeige-Pipeline (Streifen auf
//! zwei Kernen, PPA vergrössert ×2). Ohne Änderung wird kein Bild gezeichnet.

pub mod bildbetrachter;
pub mod notify;
pub mod pdf;
pub mod pdfbetrachter;
pub mod bildschirmfoto;
pub mod dateimanager;
pub mod einstellungen;
pub mod ersteinrichtung;
pub mod gesten;
pub mod kalender;
pub mod konfig;
pub mod midi;
pub mod musik;
pub mod notizen;
pub mod rechner;
pub mod shell;
pub mod taskmanager;
pub mod tastatur;
pub mod uhr;
pub mod wetter;

/// Test- und Erwartungsbild schreiben (Tests): unter Windows hält zuweilen
/// ein anderer Prozess (Dateibeobachter) die eben geschriebene Datei kurz
/// offen – dann bis 2 s erneut versuchen.
#[doc(hidden)]
pub fn bild_schreiben(pfad: &std::path::Path, daten: &[u8]) {
    for i in 0..40 {
        match std::fs::write(pfad, daten) {
            Ok(()) => return,
            Err(e) if i < 39 => {
                if i == 0 {
                    eprintln!("{}: {e} – neuer Versuch", pfad.display());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => panic!("{}: {e}", pfad.display()),
        }
    }
}

/// Zuletzt angezeigter Text eines Programms (Tests, Board-Log) – fester
/// Puffer statt `String`: Ein `static` mit Heap-Daten zeigte nach dem Ende
/// des Programms in dessen freigegebenen Heap (Neustart → Heap kaputt).
/// Gesperrt wird nur im Kern-Abschnitt: am Board legt `std::Mutex` sein
/// pthread-Objekt beim ersten Sperren im Heap des Aufrufers an (A-Review).
pub(crate) struct Textmerker(std::sync::Mutex<([u8; 64], usize)>);

impl Textmerker {
    pub(crate) const fn neu() -> Textmerker {
        Textmerker(std::sync::Mutex::new(([0; 64], 0)))
    }

    pub(crate) fn setzen(&self, s: &str) {
        let mut n = s.len().min(64);
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        let mut b = [0u8; 64];
        b[..n].copy_from_slice(&s.as_bytes()[..n]);
        kern::kern_abschnitt(|| *self.0.lock().unwrap_or_else(|e| e.into_inner()) = (b, n));
    }

    pub(crate) fn holen(&self) -> String {
        let (b, n) = kern::kern_abschnitt(|| *self.0.lock().unwrap_or_else(|e| e.into_inner()));
        String::from_utf8_lossy(&b[..n]).into_owned()
    }
}
pub mod wlan;

use kern::sys::Schirm;
use konsole::{Spiel, Wechsel};
use pudding_core::app::{FrameData, Perf};
use pudding_core::audio::AudioCmd;
use pudding_core::input::Input;

/// Logische Auflösung (quer). Das Panel (1280×720 quer) zeigt sie ×2.
pub const BREITE: usize = 640;
pub const HOEHE: usize = 360;

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

/// Messwerte des Compositors seit dem letzten Abholen (Log der Firmware).
#[derive(Clone, Copy, Debug, Default)]
pub struct Messung {
    /// Neu zusammengesetzte Bilder.
    pub bilder: u32,
    /// Übersprungene Bilder (nichts geändert).
    pub leer: u32,
    /// Neu gesetzte Fläche in Pixeln (Summe).
    pub flaeche: u32,
    /// Zeit im Compositor (µs, Summe).
    pub zeit_us: u32,
}

static M_BILDER: AtomicU32 = AtomicU32::new(0);
static M_LEER: AtomicU32 = AtomicU32::new(0);
static M_FLAECHE: AtomicU32 = AtomicU32::new(0);
static M_ZEIT: AtomicU32 = AtomicU32::new(0);

/// Messwerte abholen und zurücksetzen.
pub fn messung_holen() -> Messung {
    Messung {
        bilder: M_BILDER.swap(0, Relaxed),
        leer: M_LEER.swap(0, Relaxed),
        flaeche: M_FLAECHE.swap(0, Relaxed),
        zeit_us: M_ZEIT.swap(0, Relaxed),
    }
}

pub struct Desktop {
    /// Eingespielte Finger: (ms seit Start, Finger), absteigend sortiert.
    autopilot: Vec<(u64, [Option<(i32, i32)>; 2])>,
    /// Q4 (Dauertest): Zeitpunkte (ms, absteigend), an denen der Autopilot
    /// alle noch offenen Programme beendet (Ereignis `t:0:0:9`, Anfang jeder
    /// Runde) – ein daneben gegangener Tipp hinterlässt sonst offene
    /// Fenster, die sich über die Runden summieren.
    ap_aufraeumen: Vec<u64>,
    ap_finger: [Option<(i32, i32)>; 2],
    /// Die Firmware rückt den Autopiloten selbst vor (`fremde_eingabe`).
    ap_extern: bool,
    /// Berührung dient nur dem Aufwecken (Firmware, Bildschirmschoner).
    gesperrt: bool,
    gesten: gesten::Gesten,
    waechter_ms: u64,
    /// Nächstes Sichern der Uhrzeit (alle 10 min, Rückfall nach dem Ausschalten).
    uhr_ms: u64,
    /// Zuletzt gesehener NTP-Abgleich (danach Uhr sichern).
    ntp_gesehen: Option<i64>,
    /// Zuletzt an den Synth geschickte Lautstärke.
    lautstaerke: Option<u8>,
    start_us: u64,
    schirm: Schirm,
    aktuell: Option<usize>,
    unveraendert: bool,
    uhr: fn() -> u64,
}

impl Desktop {
    /// `uhr`: Mikrosekunden (Board: esp_timer, PC: Instant).
    pub fn neu(uhr: fn() -> u64) -> Desktop {
        Desktop {
            autopilot: Vec::new(),
            ap_aufraeumen: Vec::new(),
            ap_finger: [None, None],
            ap_extern: false,
            gesperrt: false,
            gesten: gesten::Gesten::default(),
            waechter_ms: 0,
            uhr_ms: 600_000,
            ntp_gesehen: None,
            lautstaerke: None,
            start_us: uhr(),
            schirm: Schirm::default(),
            aktuell: None,
            unveraendert: false,
            uhr,
        }
    }

    /// Autopilot um höchstens einen Eintrag vorrücken: stand die Schleife
    /// (Foto, Flash-Schreiben), sieht die Gestenerkennung trotzdem jeden
    /// Zustand – sonst fielen Druck und Loslassen in ein Bild.
    fn ap_vorruecken(&mut self) {
        let ms = ((self.uhr)() - self.start_us) / 1000;
        if self.autopilot.last().is_some_and(|e| e.0 <= ms)
            && let Some((_, f)) = self.autopilot.pop()
        {
            self.ap_finger = f;
        }
    }

    /// Touch-Autopilot (Tests am Board), Zeiten ab Start in ms:
    /// "ms:x:y:1" Finger auf/zu (x, y), "ms:x:y:0" Finger weg,
    /// "ms:x1:y1:x2:y2" zwei Finger. Die Fingerlage gilt bis zum nächsten
    /// Eintrag und geht durch die Gestenerkennung wie echte Berührungen.
    pub fn autopilot(&mut self, skript: &str) {
        konfig::AUTOPILOT_AKTIV.store(true, Relaxed);
        self.ap_aufraeumen = skript
            .split(',')
            .filter_map(|t| {
                let v: Vec<i64> = t.split(':').filter_map(|z| z.trim().parse().ok()).collect();
                (v.len() == 4 && v[3] == 9).then_some(v[0] as u64)
            })
            .collect();
        self.ap_aufraeumen.sort_unstable();
        self.ap_aufraeumen.reverse();
        self.autopilot = skript
            .split(',')
            .filter_map(|t| {
                let v: Vec<i64> = t.split(':').filter_map(|z| z.trim().parse().ok()).collect();
                if v.len() == 4 && v[3] == 9 {
                    return None;
                }
                let f = match v.len() {
                    4 if v[3] != 0 => [Some((v[1] as i32, v[2] as i32)), None],
                    4 => [None, None],
                    5 => [Some((v[1] as i32, v[2] as i32)), Some((v[3] as i32, v[4] as i32))],
                    _ => return None,
                };
                Some((v[0] as u64, f))
            })
            .collect();
        self.autopilot.sort_by_key(|e| e.0);
        self.autopilot.reverse();
    }
}

impl Spiel for Desktop {
    fn update(&mut self, input: &Input, _dt: f32) -> Wechsel {
        let ms = ((self.uhr)() - self.start_us) / 1000;
        if !self.ap_extern {
            self.ap_vorruecken();
        }
        if self.ap_aufraeumen.last().is_some_and(|&t| ms >= t) {
            self.ap_aufraeumen.pop();
            // auch liegengebliebene Meldungen („Programm nicht gestartet“):
            // eine offene Meldung mitten im Bild lenkt sonst alle weiteren
            // Tipps der Runde um (Dauertest 27.11.: ein Startfehler zog so
            // weitere nach sich); im Protokoll stehen sie trotzdem
            let offen: Vec<(u32, &str)> = kern::liste().iter().filter(|p| !p.name.starts_with('@') || p.name == "@meldung").map(|p| (p.pid, p.name)).collect();
            for &(pid, _) in &offen {
                kern::beende(pid);
            }
            kern::sys::debug_text(&format!("Autopilot: Runde beginnt, {} Programme offen geblieben {:?}", offen.len(), offen.iter().map(|o| o.1).collect::<Vec<_>>()));
        }
        let echt = [input.touch[0], input.touch.get(1).copied().flatten()].map(|t| t.map(|(x, y)| (i32::from(x), i32::from(y))));
        let ap = if self.gesperrt { [None, None] } else { self.ap_finger };
        let finger = if echt.iter().any(Option::is_some) { echt } else { ap };
        self.gesten.schritt(ms, finger);
        if ms >= self.waechter_ms {
            self.waechter_ms = ms + 500;
            shell::waechter();
        }
        // NTP hat die Uhr gestellt (W3b): gleich sichern (Rückfall nach dem
        // Ausschalten), sonst alle 10 min
        let ntp = kern::zeit::ntp_gestellt_um();
        if ms >= self.uhr_ms || ntp != self.ntp_gesehen {
            self.uhr_ms = ms + 600_000;
            self.ntp_gesehen = ntp;
            let _ = konfig::uhr_sichern();
        }
        Wechsel::Bleiben
    }

    fn render_size(&self) -> (usize, usize) {
        (BREITE, HOEHE)
    }

    fn touch_raster(&self) -> (u16, u16) {
        (BREITE as u16, HOEHE as u16)
    }

    fn bild(&mut self, fd: &mut FrameData, _perf: &Perf) {
        let t0 = (self.uhr)();
        let neu = self.schirm.aktualisieren();
        M_ZEIT.fetch_add(((self.uhr)() - t0) as u32, Relaxed);
        self.unveraendert = neu.is_none() && self.aktuell.is_some();
        if self.unveraendert {
            M_LEER.fetch_add(1, Relaxed);
            return;
        }
        M_BILDER.fetch_add(1, Relaxed);
        M_FLAECHE.fetch_add(self.schirm.letzte_flaeche as u32, Relaxed);
        let i = neu.or(self.aktuell).unwrap_or(0);
        self.aktuell = Some(i);
        fd.flach(BREITE, HOEHE, &|_| 0, &mut |_| {});
        let p = &self.schirm.puffer[i];
        // SAFETY: Doppelpuffer – Puffer i wird erst übernächstes Bild wieder
        // beschrieben; dann ist dieses Bild längst gezeichnet (join).
        unsafe { fd.vollbild565(p.px.as_ptr(), p.w, p.h) };
    }

    /// Lautstärke (Einstellungen) an den Synth; Probeton nach dem Ändern.
    fn ton(&mut self, f: &mut dyn FnMut(AudioCmd)) {
        let v = konfig::LAUTSTAERKE.load(Relaxed);
        if self.lautstaerke != Some(v) {
            self.lautstaerke = Some(v);
            f(AudioCmd::SfxVolume(v));
            f(AudioCmd::MusicVolume(v));
        }
        if konfig::TON_PROBE.swap(false, Relaxed) {
            f(AudioCmd::Sfx(pudding_core::audio::Sfx::Cherry));
        }
        // Signalton der Uhr (Wecker/Timer, A3)
        let n = konfig::SIGNAL.swap(0, Relaxed);
        if n > 0 {
            f(AudioCmd::Sfx(pudding_core::audio::Sfx::Checkpoint));
            kern::sys::debug_text(&format!("Ton: Signal der Uhr an den Synth (Lautstärke {v})"));
        }
    }

    /// Akkustand von der Firmware (nur mit Spannungsteiler, siehe board.rs).
    fn akku(&mut self, prozent: Option<u8>) {
        konfig::AKKU.store(prozent.map_or(255, |p| p.min(100)), Relaxed);
    }

    fn bildschirm_aus_s(&self) -> Option<u32> {
        Some(konfig::BILDSCHIRM_AUS.load(Relaxed)).filter(|&s| s > 0)
    }

    fn ruhe_nach_s(&self) -> Option<u32> {
        Some(konfig::RUHE_NACH.load(Relaxed)).filter(|&s| s > 0)
    }

    fn ruhe_jetzt(&mut self) -> bool {
        konfig::RUHE_JETZT.swap(false, Relaxed)
    }

    /// Wecker/Timer klingelt (A3): Bildschirm an, auch aus dem Ruhezustand.
    fn wach_halten(&mut self) -> bool {
        uhr::KLINGELN.load(Relaxed) != 0
    }

    fn fremde_eingabe(&mut self) -> bool {
        self.ap_extern = true;
        self.ap_vorruecken();
        self.ap_finger.iter().any(Option::is_some)
    }

    fn eingabe_sperren(&mut self, an: bool) {
        self.gesperrt = an;
    }

    /// Helligkeit aus den Einstellungen (1..=10).
    fn helligkeit(&self) -> u8 {
        konfig::HELLIGKEIT.load(Relaxed)
    }

    fn speichern(&mut self) -> Option<Vec<u8>> {
        None
    }

    fn eigene_home_taste(&self) -> bool {
        true
    }

    fn eigene_fps(&self) -> bool {
        true
    }

    /// Text und Linien scharf: PPA vergrössert ohne Glättung.
    fn glatt(&self) -> bool {
        false
    }

    fn unveraendert(&self) -> bool {
        self.unveraendert
    }

    fn geaenderte_zeilen(&self) -> Option<(usize, usize)> {
        self.schirm.letzte_zeilen
    }
}

/// Messprogramm K5: 10 s ruhig, dann Fenster im Kreis verschieben.
pub mod messung {
    use kern::sys;

    pub fn fenster_kreisen() {
        sys::pruefe_ereignis();
        sys::fenster(200 << 16 | 239, 100 << 16 | 159, 0x33 << 24 | 0xDDD7CF, 0, Some("Messung K5"));
        sys::text(10 << 16 | 10, 0xB0_000000, "Dirty-Rects\0".as_bytes(), 0, 0);
        let t0 = sys::zeitzaehler();
        let mut i = 0u32;
        loop {
            sys::warte_ereignis_max(2);
            let t = sys::zeitzaehler().wrapping_sub(t0);
            if t < 1000 {
                continue;
            }
            i += 1;
            let a = (i % 360) as f32 * core::f32::consts::PI / 180.0;
            let x = 200.0 + 150.0 * a.cos();
            let y = 100.0 + 80.0 * a.sin();
            sys::fenster_bewegen(x as i32, y as i32, -1, -1);
        }
    }
}

macro_rules! eintrag {
    ($name:expr, $f:path, $heap:expr) => {
        kern::ProgrammEintrag { name: $name, heap_kb: $heap, stapel_kb: 48, start: $f }
    };
}

/// Shell-Programme (ohne C-Programme).
const SHELL: [kern::ProgrammEintrag; 23] = [
    eintrag!("@meldung", shell::meldung, 256),
    // Dienste: Spitze am Board (M4, 24.11.) @tastatur 56 KB, @taskbar
    // 54 KB, @menu 56 KB – vorher 512/512/1024 KB; der Browser (10,6 MB am
    // Stück) fand daneben im Programm-Pool keinen Platz mehr
    eintrag!("@tastatur", tastatur::dienst, 256),
    eintrag!("@start", shell::start, 512),
    // 1,75 MB: Spitze mit Hintergrundbild 1350 KB (PC-Test einstellungen,
    // M6 27.11.: Bild direkt in RGB565 statt RGBA – vorher 1800 KB und
    // 2,5 MB Reserve; davor 4 MB, die im Programm-Pool fehlten, M4)
    eintrag!("@icon", shell::icon, 1792),
    eintrag!("@taskbar", shell::taskbar, 256),
    eintrag!("@menu", shell::menu, 256),
    eintrag!("info", shell::info, 256),
    eintrag!("dateimanager", dateimanager::programm, 1024),
    // 8 MB: startet neben Dateimanager und Diensten (O4; mit 12 MB blieb
    // am Board zu wenig PSRAM), grössere Bilder meldet der Bildbetrachter
    eintrag!("bildbetrachter", bildbetrachter::programm, 8192),
    // 6 MB am Stück: Hintergrundbilder lädt bildbetrachter::laden passend
    // zum freien Speicher (grosse Fotos verkleinert). Mit 12 MB fand der
    // Start kurz nach dem Hochfahren keinen so grossen PSRAM-Block (U1).
    eintrag!("einstellungen", einstellungen::programm, 6144),
    eintrag!("ersteinrichtung", ersteinrichtung::programm, 512),
    eintrag!("@screenshot", bildschirmfoto::programm, 4096),
    eintrag!("messung", messung::fenster_kreisen, 128),
    eintrag!("taskmanager", taskmanager::programm, 256),
    eintrag!("notizen", notizen::programm, 1024),
    eintrag!("rechner", rechner::programm, 128),
    eintrag!("uhr", uhr::programm, 128),
    // Q2: Wetter (Open-Meteo); JSON der Vorhersage ~10 KB, Abruf 64 KB
    eintrag!("wetter", wetter::programm, 512),
    eintrag!("kalender", kalender::programm, 128),
    // Musik: Lesepuffer des Decoders (256 KB), Ausgabe, Dateiliste; der
    // Decoder braucht viel Stapel (am PC ~80 KB, am Board mehr; mit 48 KB
    // Stack-Überlauf), Stapel liegt im Heap des Programms (PSRAM)
    kern::ProgrammEintrag { name: "musik", heap_kb: 2048, stapel_kb: 256, start: musik::programm },
    // Testprogramm (A6), nicht im Menü
    eintrag!("endlostest", taskmanager::endlostest, 64),
    // PDF-Betrachter: Datei (bis 4 MB) und gezeichnete Seite (bis 1,2 MP, je nach freiem Speicher)
    eintrag!("pdfbetrachter", pdfbetrachter::programm, 8192),
    // Benachrichtigung wie KolibriOS notify3 (für C-Programme, Z3)
    eintrag!("@notify", notify::programm, 64),
];

/// Programme des Desktops (statisch eingelinkt): Shell + KolibriOS-C-Programme.
#[cfg(target_pointer_width = "32")]
pub static PROGRAMME: &[kern::ProgrammEintrag] = &[
    SHELL[0], SHELL[1], SHELL[2], SHELL[3], SHELL[4], SHELL[5], SHELL[6], SHELL[7], SHELL[8], SHELL[9], SHELL[10], SHELL[11], SHELL[12], SHELL[13], SHELL[14], SHELL[15], SHELL[16], SHELL[17], SHELL[18], SHELL[19], SHELL[20], SHELL[21], SHELL[22],
    eintrag!("absturz_test", kolibri::absturz_test, 128),
    eintrag!("basic_gui", kolibri::basic_gui, 512),
    eintrag!("boxlib_demo", kolibri::boxlib_demo, 512),
    eintrag!("libimg_demo", kolibri::libimg_demo, 2048),
    eintrag!("dialog_demo", kolibri::dialog_demo, 512),
    eintrag!("http_tcp_demo", kolibri::http_tcp_demo, 256),
    eintrag!("netz_laden", kolibri::netz_laden, 256),
    eintrag!("passwordgen", kolibri::passwordgen, 256),
    eintrag!("thashview", kolibri::thashview, 512),
    // C1: KolibriOS-Shell in der Konsole (console.obj nativ) und „Matrix“
    eintrag!("shell", kolibri::shell, 1024),
    eintrag!("matrix", kolibri::matrix, 256),
    // Browser (B1): NetSurf + QuickJS; Heap für Seiten, Bilder und die
    // JS-Laufzeit (bis 6 MB, 3 MB Reserve für die Seite, netsurf/build.rs), Stapel für Layout und
    // JS (Grenze 192 KB). 10,5 MB (M6 27.11.: +512 KB aus der kleineren
    // Reserve von @icon; im Pool bleibt neben den Diensten derselbe Abstand
    // wie vorher mit 10 MB)
    kern::ProgrammEintrag { name: "browser", heap_kb: 10 * 1024 + 512, stapel_kb: 384, start: netsurf::programm },
];

#[cfg(not(target_pointer_width = "32"))]
pub static PROGRAMME: &[kern::ProgrammEintrag] = &SHELL;

/// Anwendungen im Startmenü / auf dem Desktop (Stand D1; Alltagsprogramme
/// folgen mit P/A, Spiele mit S).
pub fn anwendungen() -> Vec<shell::Anwendung> {
    let a = |programm, titel, titel_de, symbol, auf_desktop| shell::Anwendung { programm, titel, titel_de, symbol, auf_desktop };
    let mut v = vec![a("info", "Info", "Info", 113, true), a("dateimanager", "Files", "Dateien", 0, true), a("einstellungen", "Settings", "Einstellungen", 5, true), a("notizen", "Notes", "Notizen", 9, true), a("rechner", "Calculator", "Rechner", 4, true), a("uhr", "Clock", "Uhr", 119, true), a("kalender", "Calendar", "Kalender", 52, true), a("musik", "Music", "Musik", 65, true), a("wetter", "Weather", "Wetter", 128, true)];
    if cfg!(target_pointer_width = "32") {
        v.extend([
            a("browser", "Browser", "Browser", 125, true),
            a("pdfbetrachter", "PDF viewer", "PDF-Betrachter", 73, false),
            a("basic_gui", "Example GUI", "Beispiel GUI", 2, false),
            a("boxlib_demo", "Controls", "Steuerelemente", 6, false),
            a("libimg_demo", "Image demo", "Bild-Demo", 70, false),
            a("http_tcp_demo", "Network test", "Netzwerk-Test", 33, false),
            a("netz_laden", "Download test", "Download-Test", 50, false),
            a("passwordgen", "Password generator", "Passwort-Generator", 69, false),
            a("thashview", "Checksums", "Prüfsummen", 58, false),
            a("shell", "Console", "Konsole", 2, false),
            a("matrix", "Matrix", "Matrix", 70, false),
        ]);
    }
    // Spiele (S0, Auftrag 13.8): je ein Desktop-Symbol, im Startmenü im
    // Ordner „Spiele...“; Start per Neustart ins Spiel (alter Launcher weg)
    v.extend([
        a("@spiel_pudding", "Pudding Panic", "Pudding-Panik", 98, true),
        a("@spiel_wuerfelwelt", "Cube World", "Würfelwelt", 130, true),
        a("@spiel_doom", "Doom", "Doom", 43, true),
        a("@spiel_quake", "Quake", "Quake", 56, true),
    ]);
    v
}
