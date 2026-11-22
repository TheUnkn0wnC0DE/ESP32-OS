//! ESP32-OS-Shell: Startbild (`@start`), Desktop-Symbole (`@icon`),
//! Taskleiste (`@taskbar`), Startmenü (`@menu`) und Info-Dialog (`info`) –
//! eigene Programme wie in KolibriOS, über die Systemfunktionen des Kerns.
//! Original-Symbole aus KolibriOS (`daten/icons32.png`), für Touch
//! vergrössert (Symbole 48 px, Zeilen ≥ 40 px bei logisch 640×360 ≙ ×2).

use kern::sys;
use kolibri::dekoder::Rgba;
use std::sync::{Mutex, OnceLock};

/// NVS-Schlüssel des einmaligen Merkers „Spiele“ (Neustart in den Launcher).
pub const MODUS_SCHLUESSEL: &str = "esp32os_modus";

/// Höhe der Taskleiste (logische Pixel; Knöpfe 41 px ≈ 7,1 mm).
pub const LEISTE_H: i32 = 48;
/// Name des Systems (Startbild, Info, Taskleiste).
pub const NAME: &str = "ESP32-OS";
pub const VERSION: &str = "1.0-dev (2026-11-21)";

/// Eintrag im Startmenü und auf dem Desktop.
#[derive(Clone, Copy, Debug)]
pub struct Anwendung {
    /// Programmname (kern::ProgrammEintrag).
    pub programm: &'static str,
    /// Titel englisch / deutsch (Sprache: [`Anwendung::name`]).
    pub titel: &'static str,
    pub titel_de: &'static str,
    /// Index in icons32.png.
    pub symbol: usize,
    pub auf_desktop: bool,
}

impl Anwendung {
    /// Titel in der aktuellen Sprache.
    pub fn name(&self) -> &'static str {
        kern::sprache::t(self.titel, self.titel_de)
    }
}

/// Haken der Plattform (Board: Neustart, Spielekonsole). Am PC leer.
#[derive(Clone, Copy)]
pub struct Haken {
    pub neustart: fn(),
    pub konsole: fn(),
}

static HAKEN: Mutex<Option<Haken>> = Mutex::new(None);
static ANWENDUNGEN: Mutex<Vec<Anwendung>> = Mutex::new(Vec::new());

pub fn haken_setzen(h: Haken) {
    *HAKEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
}

/// Anwendungen für Menü und Desktop festlegen (Systemstart).
pub fn anwendungen_setzen(v: &[Anwendung]) {
    kern::kern_abschnitt(|| *ANWENDUNGEN.lock().unwrap_or_else(|e| e.into_inner()) = v.to_vec());
}

fn anwendungen() -> Vec<Anwendung> {
    // Sperre nur im Kern-Abschnitt (siehe SYMBOL_CACHE; Zwangsbeenden)
    kern::kern_abschnitt(|| ANWENDUNGEN.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

fn haken() -> Option<Haken> {
    kern::kern_abschnitt(|| *HAKEN.lock().unwrap_or_else(|e| e.into_inner()))
}

// ------------------------------------------------------------ Symbole

static SYMBOLE: OnceLock<Rgba> = OnceLock::new();

fn streifen() -> &'static Rgba {
    SYMBOLE.get_or_init(|| {
        kern::kern_abschnitt(|| {
            kolibri::dekoder::dekodieren(include_bytes!("../daten/icons32.png"))
                .ok()
                .and_then(|(_, mut v)| v.pop())
                .unwrap_or(Rgba { w: 32, h: 32, px: vec![0; 1024], delay: 0 })
        })
    })
}

/// Bereits skalierte Symbole (System-Heap; Skalieren kostet am Board viel
/// Zeit und verzögert das Neuzeichnen – währenddessen gehen Tipps verloren).
/// Nur im Kern-Abschnitt sperren: am Board legt `std::Mutex` sein
/// pthread-Objekt beim ersten Sperren im Heap des Aufrufers an – im
/// Programm-Heap wäre es nach dem Programmende weg (Absturz 22.11.).
static SYMBOL_CACHE: Mutex<Vec<(usize, u32, Rgba)>> = Mutex::new(Vec::new());

/// Symbol `i` in Grösse `g`.
pub fn symbol(i: usize, g: u32) -> Rgba {
    let s = streifen();
    let n = (s.h / 32) as usize;
    let i = i.min(n - 1);
    let gespeichert = kern::kern_abschnitt(|| SYMBOL_CACHE.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, gr, _)| *k == i && *gr == g).map(|(_, _, b)| b.clone()));
    if let Some(b) = gespeichert {
        return b;
    }
    kern::kern_abschnitt(|| {
        let b = Rgba { w: 32, h: 32, px: s.px[i * 1024..(i + 1) * 1024].to_vec(), delay: 0 };
        let b = if g == 32 { b } else { kolibri::libimg::skalieren(&b, g, g, true) };
        SYMBOL_CACHE.lock().unwrap_or_else(|e| e.into_inner()).push((i, g, b.clone()));
        b
    })
}

/// Bild mit Alpha über einen bekannten Untergrund `hg(x, y)` mischen und
/// zeichnen (Funktion 65 kennt kein Alpha).
pub fn bild_auf(b: &Rgba, x: i32, y: i32, hg: impl Fn(i32, i32) -> u32) {
    let mut m = b.clone();
    for yy in 0..b.h as i32 {
        for xx in 0..b.w as i32 {
            let p = b.px[(yy as u32 * b.w + xx as u32) as usize];
            let a = p >> 24;
            let u = hg(x + xx, y + yy);
            let k = |s: u32| ((p >> s & 255) * a + (u >> s & 255) * (255 - a)) / 255;
            m.px[(yy as u32 * b.w + xx as u32) as usize] = 0xFF00_0000 | k(16) << 16 | k(8) << 8 | k(0);
        }
    }
    kolibri::libimg::zeichnen(&m, x, y, m.w, m.h, 0, 0);
}

fn text(x: i32, y: i32, t: &str, farbe: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

fn text_gross(x: i32, y: i32, t: &str, farbe: u32, faktor: u32) {
    sys::text((x as u32) << 16 | y as u32 & 0xFFFF, 0x3000_0000 | (faktor - 1) << 24 | farbe & 0xFF_FFFF, t.as_bytes(), t.chars().count() as u32, 0);
}

/// Text mit Schatten (auf Bildern lesbar).
fn text_schatten(x: i32, y: i32, t: &str) {
    text(x + 1, y + 1, t, 0x101010);
    text(x, y, t, 0xFFFFFF);
}

pub fn bildschirm() -> (i32, i32) {
    let g = sys::bildschirmgroesse();
    ((g >> 16) as i32 + 1, (g & 0xFFFF) as i32 + 1)
}

fn farbe(i: usize) -> u32 {
    let mut f = [0u8; 40];
    sys::stil(3, 0, 40, &mut f);
    u32::from_le_bytes([f[4 * i], f[4 * i + 1], f[4 * i + 2], f[4 * i + 3]])
}

/// Programm starten (wie Funktion 70.7 mit /sys/<name>).
pub fn starten(programm: &str) {
    match programm {
        "@konsole" => {
            if let Some(h) = haken() {
                (h.konsole)();
            }
        }
        "@touchpad" => {
            let _ = crate::konfig::touchpad_setzen(!sys::eingabe::touchpad());
        }
        "@ruhe" => crate::konfig::RUHE_JETZT.store(true, core::sync::atomic::Ordering::Relaxed),
        "@mehr" => {
            let _ = kern::datei::starten("/sys/@menu", "mehr");
        }
        "@neustart" => {
            let _ = crate::konfig::uhr_sichern();
            if let Some(h) = haken() {
                (h.neustart)();
            }
        }
        p => {
            let r = kern::datei::starten(&format!("/sys/{p}"), "");
            if r < 0 {
                sys::debug_text(&format!("Start {p}: Fehler {}", -r));
            }
        }
    }
}

// ------------------------------------------------------------ Kontextmenü

/// Kontextmenü (liegt in `kolibri::dialog`, damit auch box_lib es nutzt).
pub use kolibri::dialog::kontextmenue;

// ------------------------------------------------------------ @start

/// Startbild „ESP32-OS“, danach Desktop und Taskleiste.
pub fn start() {
    crate::konfig::laden();
    let (w, h) = bildschirm();
    sys::pruefe_ereignis();
    sys::fenster((w - 1) as u32, (h - 1) as u32, 0x01_000000, 0, None);
    sys::fensterebene(2, u32::MAX, 1);
    let hg = 0x0E1A2B;
    sys::rechteck((w as u32) << 0, h as u32, hg);
    let logo = symbol(49, 128);
    bild_auf(&logo, (w - 128) / 2, h / 2 - 110, |_, _| hg);
    text_gross((w - 8 * 3 * NAME.len() as i32) / 2, h / 2 + 30, NAME, 0xFFFFFF, 3);
    let unter = kern::sprache::t("KolibriOS native on ESP32-P4", "KolibriOS nativ auf ESP32-P4");
    text((w - 8 * unter.chars().count() as i32) / 2, h / 2 + 90, unter, 0x9FB4D0);
    sys::warten(120);
    starten("@icon");
    starten("@taskbar");
    starten("@tastatur");
    sys::warten(30);
    // Erster Start: Ersteinrichtung (P4)
    if crate::konfig::einrichtung_noetig() {
        sys::debug_text("Ersteinrichtung: startet\n");
        starten("ersteinrichtung");
    } else {
        sys::debug_text("Ersteinrichtung: schon erledigt\n");
    }
}

// ------------------------------------------------------------ @icon

/// Zelle 108 px breit: Beschriftung bis 13 Zeichen („Einstellungen“).
const ZELLE_W: i32 = 108;
const ZELLE_H: i32 = 84;
const SYM: u32 = 48;

/// Senkrechter Verlauf `oben` → `unten` an Zeile `y` von `h`.
fn verlauf(oben: u32, unten: u32, y: i32, h: i32) -> u32 {
    let t = y.clamp(0, h) as u32 * 255 / h.max(1) as u32;
    let mix = |a: u32, b: u32, s: u32| ((a >> s & 255) * (255 - t) + (b >> s & 255) * t) / 255;
    mix(oben, unten, 16) << 16 | mix(oben, unten, 8) << 8 | mix(oben, unten, 0)
}

/// Bild füllend auf w×h skalieren und mittig beschneiden.
pub fn fuellend(b: &Rgba, w: u32, h: u32) -> Rgba {
    let s = (w as f32 / b.w as f32).max(h as f32 / b.h as f32);
    let (sw, sh) = (((b.w as f32 * s).ceil() as u32).max(w), ((b.h as f32 * s).ceil() as u32).max(h));
    let g = kolibri::libimg::skalieren(b, sw, sh, true);
    let (ox, oy) = ((sw - w) / 2, (sh - h) / 2);
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let z = ((y + oy) * g.w + ox) as usize;
        px.extend(g.px[z..z + w as usize].iter().map(|p| p | 0xFF00_0000));
    }
    Rgba { w, h, px, delay: 0 }
}

/// Hintergrund des Desktops: Verlauf (zeilenweise gezeichnet, ohne Puffer)
/// oder das beim Auswählen vorbereitete Bild (`konfig::HG_DATEI`,
/// Bildschirmgrösse). Fehlt das Bild → blauer Verlauf.
pub enum Grund {
    Verlauf(u32, u32),
    Bild(Rgba),
}

pub fn hintergrund_grund(hg: &crate::konfig::Hintergrund, w: i32, h: i32) -> Grund {
    use crate::konfig::{Hintergrund, VERLAEUFE};
    let i = match hg {
        Hintergrund::Bild(p) => match crate::konfig::hintergrund_lesen(w as u32, h as u32) {
            Some(b) => return Grund::Bild(b),
            None => {
                sys::debug_text(&format!("Hintergrund {p}: vorbereitetes Bild fehlt\n"));
                0
            }
        },
        Hintergrund::Verlauf(i) => (*i).min(VERLAEUFE.len() - 1),
    };
    Grund::Verlauf(VERLAEUFE[i].2, VERLAEUFE[i].3)
}

fn icon_zeichnen(w: i32, h: i32, grund: &Grund) {
    let t0 = sys::zeit_ns();
    sys::neuzeichnen(1);
    sys::fenster((w - 1) as u32, (h - 1) as u32, 0x41_000000, 0, None);
    match grund {
        Grund::Verlauf(oben, unten) => {
            // Bänder gleicher Panelfarbe (RGB565) in einem Aufruf: wenige
            // Dutzend statt 360 Aufrufe (Neuzeichnen am Board viel schneller)
            let mut y0 = 0;
            while y0 < h {
                let c = kern::grafik::zu565(verlauf(*oben, *unten, y0, h));
                let mut y1 = y0 + 1;
                while y1 < h && kern::grafik::zu565(verlauf(*oben, *unten, y1, h)) == c {
                    y1 += 1;
                }
                sys::rechteck(w as u32, (y0 as u32) << 16 | (y1 - y0) as u32, kern::grafik::von565(c));
                y0 = y1;
            }
        }
        Grund::Bild(b) => kolibri::libimg::zeichnen(b, 0, 0, b.w, b.h, 0, 0),
    }
    let hg_farbe = |x: i32, y: i32, _h: i32| match grund {
        Grund::Verlauf(oben, unten) => verlauf(*oben, *unten, y, h),
        Grund::Bild(b) => b.px[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize] & 0xFF_FFFF,
    };
    let zeilen = ((h - LEISTE_H - 8) / ZELLE_H).max(1);
    for (i, a) in anwendungen().iter().filter(|a| a.auf_desktop).enumerate() {
        let (sp, ze) = (i as i32 / zeilen, i as i32 % zeilen);
        let (x, y) = (8 + sp * ZELLE_W, 8 + ze * ZELLE_H);
        sys::knopf((x as u32) << 16 | (ZELLE_W - 1) as u32, (y as u32) << 16 | (ZELLE_H - 1) as u32, 0x4000_0000 | (10 + i as u32), 0);
        bild_auf(&symbol(a.symbol, SYM), x + (ZELLE_W - SYM as i32) / 2, y + 4, |xx, yy| hg_farbe(xx, yy, h));
        let t: String = a.name().chars().take(13).collect();
        let tw = 8 * t.chars().count() as i32;
        text_schatten(x + (ZELLE_W - tw) / 2, y + 8 + SYM as i32, &t);
    }
    sys::neuzeichnen(2);
    sys::debug_text(&format!("@icon: gezeichnet in {} ms\n", (sys::zeit_ns() - t0) / 1_000_000));
}

/// Desktop: Hintergrund und Symbole (unterste Ebene).
pub fn icon() {
    let (w, h) = bildschirm();
    sys::maske(0x05);
    let mut erst = true;
    // Hintergrund nur neu berechnen, wenn er sich geändert hat
    let mut bild: Option<(crate::konfig::Hintergrund, Grund)> = None;
    loop {
        match sys::warte_ereignis() {
            1 => {
                let hg = crate::konfig::hintergrund();
                if bild.as_ref().is_none_or(|(k, _)| *k != hg) {
                    let b = hintergrund_grund(&hg, w, h);
                    bild = Some((hg, b));
                }
                if let Some((_, b)) = &bild {
                    icon_zeichnen(w, h, b);
                }
                if erst {
                    sys::fensterebene(2, u32::MAX, (-2i32) as u32);
                    erst = false;
                }
            }
            3 => {
                let k = sys::knopf_id();
                let (id, rechts) = (k >> 8, k & 2 != 0);
                let liste: Vec<Anwendung> = anwendungen().into_iter().filter(|a| a.auf_desktop).collect();
                if let Some(a) = id.checked_sub(10).and_then(|i| liste.get(i as usize)) {
                    if rechts {
                        // Langer Druck (Rechtsklick): Kontextmenü
                        let m = sys::maus(0);
                        let wahl = kontextmenue((m >> 16) as i32, (m & 0xFFFF) as i32, &[kern::sprache::t("Open", "Öffnen"), kern::sprache::t("Cancel", "Abbrechen")]);
                        if wahl == Some(0) {
                            starten(a.programm);
                        }
                    } else {
                        starten(a.programm);
                    }
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------ @taskbar

const START_W: i32 = 112;
const FENSTER_W: i32 = 132;
const KNOPF_START: u32 = 2;
const KNOPF_TASTATUR: u32 = 3;
/// Uhr: langer Druck = Bildschirmfoto (P6).
const KNOPF_UHR: u32 = 4;
/// WLAN-Anzeige (W6): öffnet Einstellungen → WLAN.
const KNOPF_WLAN: u32 = 5;
/// Breite der WLAN-Anzeige.
const WLAN_W: i32 = 36;

/// WLAN-Zustand für die Taskleiste: 0 = kein WLAN, sonst Bit 0 = vorhanden,
/// 1 = verbunden, 2 = verbindet, 3..5 = Balken.
pub fn wlan_stand() -> u32 {
    if !kern::wlan::vorhanden() {
        return 0;
    }
    let z = kern::wlan::zustand();
    1 | u32::from(z.verbunden) << 1 | u32::from(z.verbindet) << 2 | u32::from(kern::wlan::balken(z.rssi)) << 3
}
const KNOPF_FENSTER: u32 = 100;

fn leiste_zeichnen(w: i32, y0: i32, eigen: u32) -> Vec<kern::fenster::FensterInfo> {
    let bg = farbe(0); // taskbar (dtp)
    let tf = farbe(1); // taskbar_text
    sys::neuzeichnen(1);
    sys::fenster((w - 1) as u32, (y0 as u32) << 16 | (LEISTE_H - 1) as u32, 0x01_000000, 0, None);
    sys::rechteck(w as u32, LEISTE_H as u32, bg);
    sys::rechteck(w as u32, 1, 0xFFFFFF & (bg | 0x303030));
    // Start
    sys::knopf(2 << 16 | (START_W - 4) as u32, 3 << 16 | (LEISTE_H - 7) as u32, KNOPF_START, 0x3A7AC0);
    bild_auf(&symbol(49, 28), 8, 6, |_, _| 0x3A7AC0);
    text(40, (LEISTE_H - 16) / 2, NAME, 0xFFFFFF);
    // Fenster der Programme
    let liste: Vec<_> = sys::fenster_liste().into_iter().filter(|f| f.ebene == 0 && f.pid != eigen).collect();
    // Akkustand (nur wenn die Firmware misst, siehe board.rs) links neben
    // dem Tastaturknopf
    let akku = crate::konfig::AKKU.load(core::sync::atomic::Ordering::Relaxed);
    // WLAN-Anzeige (W6) links davon, wenn es WLAN gibt
    let wlan = wlan_stand();
    let rechts = 64 + 48 + if akku <= 100 { 56 } else { 0 } + if wlan != 0 { WLAN_W } else { 0 };
    let platz = ((w - START_W - rechts) / FENSTER_W).max(0) as usize;
    for (i, f) in liste.iter().take(platz).enumerate() {
        let x = START_W + 4 + i as i32 * FENSTER_W;
        let c = if f.aktiv { 0x7887A6 } else if f.minimiert { 0x3E4A63 } else { 0x4E5A75 };
        sys::knopf((x as u32) << 16 | (FENSTER_W - 6) as u32, 3 << 16 | (LEISTE_H - 7) as u32, KNOPF_FENSTER + i as u32, c);
        let t: String = f.titel.chars().take(((FENSTER_W - 14) / 8) as usize).collect();
        text(x + 6, (LEISTE_H - 16) / 2, &t, tf);
    }
    if wlan != 0 {
        let x = w - rechts + 2;
        sys::knopf((x as u32) << 16 | (WLAN_W - 4) as u32, 3 << 16 | (LEISTE_H - 7) as u32, KNOPF_WLAN | 0x4000_0000, 0);
        let (verbunden, verbindet, balken) = (wlan & 2 != 0, wlan & 4 != 0, (wlan >> 3 & 7) as u8);
        for i in 0..4u8 {
            let bh = 6 + i32::from(i) * 4;
            let c = if verbunden && i < balken {
                0xFFFFFF
            } else if verbindet {
                0xC8B060
            } else {
                0x6A7488
            };
            sys::rechteck(((x + 6 + i32::from(i) * 6) as u32) << 16 | 4, ((32 - bh) as u32) << 16 | bh as u32, c);
        }
        if !verbunden && !verbindet {
            // getrennt: kleines rotes Kreuz
            for d in 0..7 {
                sys::rechteck(((x + 22 + d) as u32) << 16 | 1, ((10 + d) as u32) << 16 | 1, 0xE04040);
                sys::rechteck(((x + 28 - d) as u32) << 16 | 1, ((10 + d) as u32) << 16 | 1, 0xE04040);
            }
        }
    }
    if akku <= 100 {
        let ax = w - rechts + 4 + if wlan != 0 { WLAN_W } else { 0 };
        let c = if akku < 20 { 0xE04040 } else { 0x60D070 };
        sys::rechteck((ax as u32) << 16 | 22, 16 << 16 | 14, 0xD0D0D0);
        sys::rechteck(((ax + 22) as u32) << 16 | 2, 20 << 16 | 6, 0xD0D0D0);
        sys::rechteck(((ax + 1) as u32) << 16 | 20, 17 << 16 | 12, bg);
        sys::rechteck(((ax + 2) as u32) << 16 | (u32::from(akku) * 18 / 100).max(1), 18 << 16 | 10, c);
        let p = format!("{akku}%");
        text(ax + 28, (LEISTE_H - 16) / 2, &p, tf);
    }
    // Bildschirmtastatur ein/aus
    let kx = w - rechts + 2 + if akku <= 100 { 56 } else { 0 } + if wlan != 0 { WLAN_W } else { 0 };
    sys::knopf((kx as u32) << 16 | 42, 3 << 16 | (LEISTE_H - 7) as u32, KNOPF_TASTATUR, 0x4E5A75);
    bild_auf(&symbol(118, 28), kx + 8, 6, |_, _| 0x4E5A75);
    // Uhr (unsichtbarer Knopf: langer Druck = Bildschirmfoto)
    sys::knopf(((w - 8 * 5 - 16) as u32) << 16 | (8 * 5 + 14) as u32, 3 << 16 | (LEISTE_H - 7) as u32, KNOPF_UHR | 0x4000_0000, 0);
    let z = sys::uhrzeit();
    let bcd = |v: u32| (v >> 4) * 10 + (v & 15);
    let uhr = format!("{:02}:{:02}", bcd(z & 0xFF), bcd(z >> 8 & 0xFF));
    text(w - 8 * 5 - 12, (LEISTE_H - 16) / 2, &uhr, tf);
    sys::neuzeichnen(2);
    liste
}

/// Taskleiste: immer oben, Arbeitsfläche ohne Leiste.
pub fn taskbar() {
    let (w, h) = bildschirm();
    let y0 = h - LEISTE_H;
    let eigen = kern::eigene_pid().unwrap_or(0);
    sys::maske(0x07);
    // Arbeitsfläche (48.6): maximierte Fenster lassen die Leiste frei
    sys::stil(6, (w - 1) as u32, (y0 - 1) as u32, &mut []);
    let mut liste = Vec::new();
    let mut erst = true;
    let mut letzte = (u32::MAX, 0usize);
    loop {
        let e = sys::warte_ereignis_max(50);
        // Uhrzeit 0x00SSMMHH: nur Stunden und Minuten zählen
        let stand = (
            sys::uhrzeit() & 0xFFFF | u32::from(crate::konfig::AKKU.load(core::sync::atomic::Ordering::Relaxed)) << 16 | wlan_stand() << 24,
            sys::fenster_liste().iter().map(|f| f.slot * 7 + f.aktiv as usize * 3 + f.minimiert as usize).sum(),
        );
        match e {
            1 => {
                liste = leiste_zeichnen(w, y0, eigen);
                if erst {
                    sys::fensterebene(2, u32::MAX, 1);
                    erst = false;
                }
            }
            3 => {
                let k = sys::knopf_id();
                let (id, rechts) = (k >> 8, k & 2 != 0);
                match id {
                    KNOPF_START => starten("@menu"),
                    KNOPF_TASTATUR => sys::tastatur_umschalten(),
                    // langer Druck auf die Uhr = Bildschirmfoto
                    KNOPF_UHR if rechts => starten("@screenshot"),
                    KNOPF_UHR => {}
                    KNOPF_WLAN => {
                        let _ = kern::datei::starten("/sys/einstellungen", "wifi");
                    }
                    id if id >= KNOPF_FENSTER => {
                        if let Some(f) = liste.get((id - KNOPF_FENSTER) as usize) {
                            // aktiv → minimieren, sonst nach vorne
                            sys::minimieren(if f.aktiv { 0 } else { 2 }, f.slot as u32);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if stand != letzte {
            letzte = stand;
            liste = leiste_zeichnen(w, y0, eigen);
        }
    }
}

// ------------------------------------------------------------ @menu

const ZEILE_M: i32 = 44;

/// Startmenü: Programme mit Symbol, „Über ESP32-OS“, Spielekonsole, Neustart.
pub fn menu() {
    let (_, h) = bildschirm();
    // Parameter „mehr“: nur die Programme ohne Desktop-Symbol (Beispiele)
    let mehr = kern::argumente() == "mehr";
    let alle = anwendungen();
    let mut eintraege: Vec<(String, &'static str, usize)> = alle.iter().filter(|a| a.auf_desktop != mehr).map(|a| (a.name().to_string(), a.programm, a.symbol)).collect();
    if !mehr {
        if !eintraege.iter().any(|e| e.1 == "info") {
            eintraege.push((format!("{} {NAME}", kern::sprache::t("About", "Über")), "info", 113));
        }
        if alle.iter().any(|a| !a.auf_desktop) {
            eintraege.push((kern::sprache::t("More programs...", "Mehr Programme...").into(), "@mehr", 2));
        }
        eintraege.push((kern::sprache::t("Task manager", "Task-Manager").into(), "taskmanager", 57));
        eintraege.push((kern::sprache::t("Screenshot", "Bildschirmfoto").into(), "@screenshot", 70));
        eintraege.push((if sys::eingabe::touchpad() { kern::sprache::t("Touchpad mode off", "Touchpad-Modus aus") } else { kern::sprache::t("Touchpad mode on", "Touchpad-Modus ein") }.into(), "@touchpad", 54));
        eintraege.push((kern::sprache::t("Sleep", "Ruhezustand").into(), "@ruhe", 119));
        eintraege.push((kern::sprache::t("Restart", "Neustart").into(), "@neustart", 12));
    }
    let spalten = if eintraege.len() > 6 { 2 } else { 1 };
    let zeilen = eintraege.len().div_ceil(spalten) as i32;
    let skin = sys::stil(4, 0, 0, &mut []).0 as i32;
    let mw = 230 * spalten as i32 + 10;
    let mh = (skin + 5 + zeilen * ZEILE_M + 10).min(h - LEISTE_H);
    let my = h - LEISTE_H - mh;
    sys::maske(0x07);
    let zeichnen = || {
        sys::neuzeichnen(1);
        sys::fenster((mw - 1) as u32, (my as u32) << 16 | (mh - 1) as u32, 0x33 << 24 | 0xF4F4F4, 0, Some(NAME));
        for (i, (t, _, s)) in eintraege.iter().enumerate() {
            let (sp, ze) = (i as i32 / zeilen, i as i32 % zeilen);
            let (x, y) = (2 + sp * 230, 2 + ze * ZEILE_M);
            sys::knopf((x as u32) << 16 | 225, (y as u32) << 16 | (ZEILE_M - 3) as u32, 10 + i as u32, 0xF4F4F4);
            bild_auf(&symbol(*s, 32), x + 6, y + (ZEILE_M - 32) / 2 - 1, |_, _| 0xF4F4F4);
            text(x + 46, y + (ZEILE_M - 16) / 2 - 1, t, 0x101010);
        }
        sys::neuzeichnen(2);
    };
    let eigen = kern::eigene_tid();
    let mut gezeigt = false;
    loop {
        match sys::warte_ereignis_max(10) {
            1 => {
                zeichnen();
                gezeigt = true;
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                if id == 1 {
                    return;
                }
                if let Some((_, p, _)) = id.checked_sub(10).and_then(|i| eintraege.get(i as usize)) {
                    starten(p);
                    return;
                }
            }
            _ => {}
        }
        // Menü schliesst, sobald etwas anderes angetippt wird
        let aktiv = sys::system(7, 0);
        if gezeigt && eigen.is_some_and(|t| sys::system(21, t) != aktiv) && sys::fenster_liste().iter().any(|f| f.aktiv) {
            return;
        }
    }
}

// ------------------------------------------------------------ @meldung

/// Meldungsdialog starten (Titel, Text); eigenes Programm, damit er auch
/// erscheint, wenn der Absender nicht mehr läuft.
pub fn melden(titel: &str, text: &str) {
    let a: String = format!("{titel}\n{text}").chars().take(250).collect();
    let r = kern::datei::starten("/sys/@meldung", &a);
    if r < 0 {
        sys::debug_text(&format!("Meldung: Fehler {}", -r));
    }
}

/// Meldungsdialog (Parameter: „Titel\nText“), OK per Touch.
pub fn meldung() {
    let a = kern::argumente();
    let (titel, inhalt) = a.split_once('\n').unwrap_or((kern::sprache::t("Message", "Meldung"), a.as_str()));
    let (titel, inhalt) = (titel.to_string(), inhalt.to_string());
    // Text auf Zeilen zu höchstens 38 Zeichen umbrechen (320 px bei 8×16)
    let mut zeilen: Vec<String> = Vec::new();
    for absatz in inhalt.split('\n') {
        let mut z = String::new();
        for wort in absatz.split(' ') {
            if z.chars().count() + wort.chars().count() + 1 > 38 && !z.is_empty() {
                zeilen.push(std::mem::take(&mut z));
            }
            if !z.is_empty() {
                z.push(' ');
            }
            z.push_str(wort);
        }
        zeilen.push(z);
    }
    let (w, h) = (400, 140 + zeilen.len() as i32 * 20);
    let (sw, sh) = bildschirm();
    let (x, y) = ((sw - w) / 2, ((sh - LEISTE_H - h) / 2).max(0));
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster((x as u32) << 16 | (w - 1) as u32, (y as u32) << 16 | (h - 1) as u32, 0x33 << 24 | 0xF4F4F4, 0, Some(&titel));
                bild_auf(&symbol(14, 48), 10, 10, |_, _| 0xF4F4F4);
                for (i, z) in zeilen.iter().enumerate() {
                    text(70, 14 + i as i32 * 20, z, 0x202020);
                }
                let by = 14 + zeilen.len() as i32 * 20 + 14;
                sys::knopf(((w - 10 - 110 - 10) as u32) << 16 | 109, (by as u32) << 16 | 39, 2, 0x3A7AC0);
                text(w - 10 - 110 - 10 + 47, by + 12, "OK", 0xFFFFFF);
                sys::neuzeichnen(2);
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                if id == 1 || id == 2 {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Systemdienste, die nach einem Absturz neu gestartet werden.
pub const DIENSTE: [&str; 3] = ["@icon", "@taskbar", "@tastatur"];

/// Wächter (läuft in der Anzeigeschleife, nicht als Programm – so überlebt
/// er jeden Programmabsturz): meldet Abstürze aller Programme und startet
/// abgestürzte Systemdienste neu.
pub fn waechter() {
    for (name, ende) in kern::neue_abstuerze() {
        let grund = match ende {
            kern::Ende::Absturz(m) => m.lines().last().unwrap_or("Panik").to_string(),
            kern::Ende::SpeicherVoll => kern::sprache::t("program memory full", "Speicher des Programms voll").to_string(),
            _ => continue,
        };
        if name != "@meldung" {
            absturz_melden(name, &grund);
        }
        if DIENSTE.contains(&name) {
            let _ = kern::starte_name(name);
        }
    }
}

/// Absturz melden (Rust-Panik/Speicher voll ohne Neustart, oder nach dem
/// Neustart nach einer CPU-Ausnahme).
pub fn absturz_melden(programm: &str, grund: &str) {
    let text = match kern::sprache::aktuell() {
        kern::sprache::Sprache::Englisch => format!("The program \"{programm}\" crashed and was closed.\nReason: {grund}"),
        kern::sprache::Sprache::Deutsch => format!("Das Programm \"{programm}\" ist abgestürzt und wurde beendet.\nGrund: {grund}"),
    };
    melden(kern::sprache::t("Program crashed", "Programm abgestürzt"), &text);
}

// ------------------------------------------------------------ info

/// Info-Dialog „Über ESP32-OS“.
pub fn info() {
    let programme = kern::liste().len();
    let zeilen = [
        format!("Version {VERSION}"),
        kern::sprache::t("KolibriOS system functions native,", "KolibriOS-Systemfunktionen nativ,").to_string(),
        kern::sprache::t("without emulator", "ohne Emulator").to_string(),
        "Waveshare ESP32-P4-WIFI6-Touch-LCD-5".to_string(),
        format!("{} {programme}", kern::sprache::t("Running programs:", "Laufende Programme:")),
        kern::sprache::t("KolibriOS parts: GPL-2.0 (LICENSE)", "KolibriOS-Teile: GPL-2.0 (LICENSE)").to_string(),
    ];
    let (w, h) = (440, 280);
    loop {
        match sys::warte_ereignis() {
            1 => {
                sys::neuzeichnen(1);
                sys::fenster(100 << 16 | (w - 1), 20 << 16 | (h - 1), 0x33 << 24 | 0xF4F4F4, 0, Some(&format!("{} {NAME}", kern::sprache::t("About", "Über"))));
                bild_auf(&symbol(49, 64), 10, 10, |_, _| 0xF4F4F4);
                text_gross(90, 18, NAME, 0x1A3A6A, 2);
                for (i, z) in zeilen.iter().enumerate() {
                    text(90, 58 + i as i32 * 20, z, 0x202020);
                }
                sys::knopf(320 << 16 | 99, 182 << 16 | 39, 2, 0x3A7AC0);
                text(320 + 42, 182 + 12, "OK", 0xFFFFFF);
                sys::neuzeichnen(2);
            }
            3 => {
                let id = sys::knopf_id() >> 8;
                if id == 1 || id == 2 {
                    return;
                }
            }
            _ => {}
        }
    }
}
