//! Fenstersystem, Ereignisse und Eingabe nach KolibriOS-Vorbild.
//!
//! Jeder Faden (Slot) hat höchstens ein Fenster mit eigenem RGB565-Puffer.
//! Programme zeichnen nur in ihren Puffer; der Compositor setzt Hintergrund
//! und Fenster in Stapelreihenfolge zusammen. Aufdecken braucht deshalb kein
//! Neuzeichnen durch das Programm. Rahmen und Kopf zeichnet der Kern.
//!
//! Alles liegt in einer `Welt` unter einer Sperre; wartende Fäden schlafen an
//! einer gemeinsamen Bedingungsvariable.

use crate::grafik::{Clip, Puffer, Zeichner, zu565};
use crate::schrift::{self, Kodierung};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard};

/// Kopf-Bitmaps eines geladenen Skins ([links, oper, basis]), schon skaliert.
#[derive(Debug)]
pub struct KopfBilder {
    pub aktiv: [crate::skin::Bild; 3],
    pub inaktiv: [crate::skin::Bild; 3],
}

/// Skin-Werte des Fensterrahmens (KolibriOS: Funktion 48.4/48.7,
/// `skincode.inc`). Ohne geladenen Skin: einfache Farbfläche.
#[derive(Clone, Debug)]
pub struct Skin {
    pub hoehe: i32,
    /// (innen, aussen, rahmen) für aktive / inaktive Fenster.
    pub rahmen_aktiv: (u32, u32, u32),
    pub rahmen_inaktiv: (u32, u32, u32),
    /// Kopffarben ohne Bitmaps.
    pub kopf_aktiv: u32,
    pub kopf_inaktiv: u32,
    /// Schliessen-/Minimieren-Knopf: (x, y, w, h), x < 0 = vom rechten Rand.
    pub knopf_zu: (i32, i32, i32, i32),
    pub knopf_min: (i32, i32, i32, i32),
    /// Titelbereich: links, oben, rechts, unten (48.7).
    pub raender: (i32, i32, i32, i32),
    pub bilder: Option<std::sync::Arc<KopfBilder>>,
    /// Touch-Vergrösserung des Skins.
    pub skala: i32,
}

impl Default for Skin {
    fn default() -> Skin {
        Skin {
            hoehe: 22,
            rahmen_aktiv: (0x1A2C44, 0x1A2C44, 0x2E5A8A),
            rahmen_inaktiv: (0x4A5A6E, 0x4A5A6E, 0x6B7F96),
            kopf_aktiv: 0x2E5A8A,
            kopf_inaktiv: 0x6B7F96,
            knopf_zu: (-20, 3, 16, 16),
            knopf_min: (-38, 3, 16, 16),
            raender: (8, 4, 44, 2),
            bilder: None,
            skala: 1,
        }
    }
}

/// Fester Fensterrahmen der Skin-Fenster: 1 + 3 + 1 Pixel (KolibriOS).
pub const RAHMEN: i32 = 5;

/// Standardfarben (Funktion 48.3, 10 Einträge).
pub const STANDARDFARBEN: [u32; 10] = [
    0x2E5A8A, // frame
    0x2E5A8A, // grab
    0x8A8A8A, // work_dark
    0xFFFFFF, // work_light
    0xFFFFFF, // grab_text
    0xE6E6E6, // work
    0x3A7AC0, // work_button
    0xFFFFFF, // work_button_text
    0x000000, // work_text
    0x404040, // graph
];

#[derive(Clone, Debug)]
pub struct Knopf {
    pub id: u32,
    /// In Fensterkoordinaten.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub kein_rahmen: bool,
    pub system: bool,
}

pub struct Fenster {
    pub x: i32,
    pub y: i32,
    /// Echte Grösse (KolibriOS-Grösse + 1).
    pub w: i32,
    pub h: i32,
    /// Y: 0..4 (Funktion 0).
    pub stil: u32,
    /// X-Bits: 1 Titel, 2 relativ zum Arbeitsbereich, 4 nicht füllen, 8 Verlauf.
    pub flags: u32,
    pub arbeit: u32,
    pub kopf: u32,
    pub titel: String,
    pub buf: Puffer,
    pub knoepfe: Vec<Knopf>,
    pub minimiert: bool,
    /// Gespeicherte Lage vor dem Maximieren.
    pub vor_max: Option<(i32, i32, i32, i32)>,
    pub immer_oben: bool,
    pub unbeweglich: bool,
    /// Ebene (Funktion 18.25): -2 Hintergrund, -1 immer hinten, 0 normal,
    /// 1 immer oben. Fenster ≠ 0 werden nie „aktiv“ (kein Tastaturfokus).
    pub ebene: i32,
    /// Zeiger des Fensters (37.5); 0 = Standardpfeil.
    pub zeiger: u32,
    /// S5: Fenster mit Skin ohne Flag „relativ“ (KolibriOS-Programme, die in
    /// Fensterkoordinaten zeichnen und die Höhe der KolibriOS-Titelleiste
    /// annehmen): Inhalt um so viel nach unten, wie die Touch-Titelleiste
    /// höher ist; das Fenster ist um so viel höher. 0 sonst.
    pub versatz: i32,
}

impl Fenster {
    /// Arbeitsbereich (x, y, w, h) relativ zum Fenster.
    pub fn arbeitsbereich(&self, skin: &Skin) -> (i32, i32, i32, i32) {
        match self.stil {
            3 | 4 => (RAHMEN, skin.hoehe, self.w - 2 * RAHMEN, self.h - skin.hoehe - RAHMEN),
            0 | 2 if self.flags & 1 != 0 => (1, 21, self.w - 2, self.h - 22),
            0 | 2 => (1, 1, self.w - 2, self.h - 2),
            _ => (0, 0, self.w, self.h),
        }
    }

    /// Clientbox wie im KolibriOS-Kern (`window.inc`,
    /// `set_window_clientbox`): mit Flag „relativ“ (0x20 im Stil) der
    /// Arbeitsbereich, sonst das ganze Fenster (0, 0, Breite − 1, Höhe − 1
    /// wie `box`). Gilt für Funktion 9 und 37.1.
    pub fn clientbox(&self, skin: &Skin) -> (i32, i32, i32, i32) {
        if self.flags & 2 != 0 { self.arbeitsbereich(skin) } else { (0, self.versatz, self.w - 1, self.h - 1 - self.versatz) }
    }

    /// Ursprung der Zeichen-, Knopf- und Mauskoordinaten des Programms
    /// (relativ zum Fenster): Arbeitsbereich mit Flag „relativ“, sonst die
    /// Fensterecke (S5: um `versatz` tiefer).
    pub fn ursprung(&self, skin: &Skin) -> (i32, i32) {
        if self.flags & 2 != 0 {
            let (ax, ay, _, _) = self.arbeitsbereich(skin);
            (ax, ay)
        } else {
            (0, self.versatz)
        }
    }

    pub fn enthaelt(&self, x: i32, y: i32) -> bool {
        !self.minimiert && x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// Eintrag der Fensterliste (Taskleiste).
#[derive(Clone, Debug)]
pub struct FensterInfo {
    pub slot: usize,
    pub pid: u32,
    pub tid: u32,
    pub titel: String,
    pub aktiv: bool,
    pub minimiert: bool,
    pub ebene: i32,
}

/// Zustand eines Fadens (KolibriOS-Slot).
pub struct FadenZ {
    pub tid: u32,
    pub pid: u32,
    pub name: String,
    pub maske: u32,
    pub redraw: bool,
    pub tasten: VecDeque<u32>,
    pub knopf: Option<u32>,
    pub maus_ev: bool,
    /// Ereignis-Bits wie Funktion 37.3 (8–10 gedrückt, 16–18 losgelassen,
    /// 15/23 Rollen) – je Faden, nur für Mausereignisse, die er auch bekommt:
    /// ein kurzes Tippen geht nicht verloren, wenn das Programm den Zustand
    /// erst nach dem Loslassen liest, und Tippen auf die Bildschirmtastatur
    /// zählt nicht als Klick ins Fenster darunter.
    pub maus_ereignisse: u32,
    /// Bildschirmlage des letzten Drückens (ESP32-OS, B8): wer Zustand und
    /// Lage nur abfragt, sieht beim Ziehen sonst schon die spätere Lage –
    /// Text markieren begänne an der falschen Stelle.
    pub druck_lage: Option<(i32, i32)>,
    pub hg_ev: bool,
    pub ipc_ev: bool,
    pub netz_ev: bool,
    pub tasten_modus: u32,
    pub scroll: (i32, i32),
    /// Zoom-Geste (zwei Finger): Faktor in Tausendstel seit letztem Abholen
    /// und Mittelpunkt (ESP32-OS-Erweiterung).
    pub zoom: (u32, i32, i32),
    pub wartet: bool,
    pub fenster: Option<Fenster>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Maus {
    pub x: i32,
    pub y: i32,
    pub tasten: u32,
}

pub struct Welt {
    pub w: i32,
    pub h: i32,
    /// links, oben, rechts, unten (inklusiv, Funktion 48.5).
    pub arbeitsflaeche: (i32, i32, i32, i32),
    pub skin: Skin,
    pub farben: [u32; 10],
    /// Index = Slot; 0 unbenutzt, 1 = IDLE.
    pub slots: Vec<Option<FadenZ>>,
    /// Slots mit Fenster, unten → oben.
    pub stapel: Vec<usize>,
    pub maus: Maus,
    druck: Option<(usize, u32, u32)>,
    pub hintergrund: u32,
    pub hintergrund_bild: Option<Puffer>,
    /// Hintergrundbild (Funktion 15): Grösse, Daten BGR, Modus 1 kacheln / 2 strecken.
    pub hg_groesse: (u32, u32),
    pub hg_daten: Vec<u8>,
    pub hg_modus: u32,
    /// Zuletzt neu gezeichneter Hintergrundbereich (15.8).
    pub hg_zuletzt: Clip,
    /// Geänderte Bildschirmbereiche seit dem letzten Zusammensetzen.
    pub schmutz: Vec<Clip>,
    /// Faden, dessen Textfeld den Eingabefokus hat (Bildschirmtastatur, D8).
    pub textfokus: Option<u32>,
    /// S1: Faden, der bei jeder Änderung der Fensterliste sofort ein
    /// Neuzeichnen-Ereignis bekommt (Taskleiste; vorher fragte sie alle
    /// 500 ms ab – der Eintrag kam eine halbe Sekunde nach dem Fenster).
    pub listen_beobachter: Option<u32>,
    /// Lage des fokussierten Textfelds (Bildschirm) – Tastatur weicht aus.
    pub textfeld: Option<Clip>,
    /// Bildschirmtastatur von Hand ein/aus (None = automatisch nach Fokus).
    pub tastatur_manuell: Option<bool>,
    /// Touchpad-Modus: Zeiger sichtbar, Finger bewegt ihn relativ.
    pub touchpad: bool,
    /// Geladene Zeiger (37.4): Index + 1 = Griff; (Besitzer-TID, Bild).
    pub zeiger: Vec<Option<(u32, crate::zeiger::Zeigerbild)>>,
    pub pfeil: crate::zeiger::Zeigerbild,
}

static WELT: Mutex<Option<Welt>> = Mutex::new(None);
static WECKER: Condvar = Condvar::new();

pub(crate) fn welt() -> MutexGuard<'static, Option<Welt>> {
    WELT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Führt `f` mit der Welt aus (legt sie bei Bedarf mit 640×360 an).
pub fn mit_welt<R>(f: impl FnOnce(&mut Welt) -> R) -> R {
    let mut g = welt();
    let w = g.get_or_insert_with(|| Welt::neu(640, 360));
    f(w)
}

/// Weckt alle wartenden Fäden (neues Ereignis, Beenden).
pub fn wecken() {
    // Die Bedingungsvariable legt ihren Zustand beim ersten Gebrauch an –
    // immer im System-Heap, nie in dem eines Programms.
    crate::prozess::kern_abschnitt(|| WECKER.notify_all());
}

/// Wartet bis zu `ms` auf eine Änderung (gibt die Sperre frei).
pub(crate) fn schlafen(g: MutexGuard<'static, Option<Welt>>, ms: u64) -> MutexGuard<'static, Option<Welt>> {
    WECKER
        .wait_timeout(g, std::time::Duration::from_millis(ms))
        .unwrap_or_else(|e| e.into_inner())
        .0
}

/// Bildschirmgrösse (logische Auflösung) festlegen; leert die Welt nicht.
pub fn bildschirm(w: i32, h: i32) {
    mit_welt(|x| {
        x.w = w;
        x.h = h;
        x.arbeitsflaeche = (0, 0, w - 1, h - 1);
        x.alles();
    });
}

impl Welt {
    pub fn neu(w: i32, h: i32) -> Welt {
        let mut slots = Vec::new();
        slots.resize_with(2, || None);
        Welt {
            w,
            h,
            arbeitsflaeche: (0, 0, w - 1, h - 1),
            skin: Skin::default(),
            farben: STANDARDFARBEN,
            slots,
            stapel: Vec::new(),
            maus: Maus::default(),
            druck: None,
            hintergrund: 0x3A6EA5,
            hintergrund_bild: None,
            hg_groesse: (0, 0),
            hg_daten: Vec::new(),
            hg_modus: 2,
            hg_zuletzt: crate::grafik::LEER,
            schmutz: vec![Clip { x0: 0, y0: 0, x1: w, y1: h }],
            textfokus: None,
            listen_beobachter: None,
            textfeld: None,
            tastatur_manuell: None,
            touchpad: false,
            zeiger: Vec::new(),
            pfeil: crate::zeiger::pfeil(),
        }
    }

    /// S1: Fensterliste geändert (Fenster neu, zu, aktiv, minimiert, Titel):
    /// den Beobachter (Taskleiste) sofort neu zeichnen lassen.
    pub fn liste_geaendert(&mut self) {
        if let Some(t) = self.listen_beobachter
            && let Some(fz) = self.faden(t)
        {
            fz.redraw = true;
            wecken();
        }
    }

    /// Ganzen Bildschirm neu zusammensetzen.
    pub fn alles(&mut self) {
        self.schmutz.clear();
        self.schmutz.push(Clip { x0: 0, y0: 0, x1: self.w, y1: self.h });
    }

    /// Bereich (Bildschirmkoordinaten) als geändert merken.
    pub fn markiere(&mut self, c: Clip) {
        let c = c.schnitt(Clip { x0: 0, y0: 0, x1: self.w, y1: self.h });
        if c.leer() {
            return;
        }
        if self.schmutz.iter().any(|s| s.schnitt(c) == c) {
            return;
        }
        self.schmutz.push(c);
        // Viele kleine Bereiche zu einem zusammenfassen
        if self.schmutz.len() > 16 {
            let u = self.schmutz.iter().fold(crate::grafik::LEER, |a, &b| a.vereinigung(b));
            self.schmutz.clear();
            self.schmutz.push(u);
        }
    }

    /// Bildschirmrechteck des Fensters in Slot `s` (leer ohne Fenster).
    pub fn fenster_rechteck(&self, s: usize) -> Clip {
        match self.slots.get(s).and_then(|f| f.as_ref()).and_then(|f| f.fenster.as_ref()) {
            Some(f) if !f.minimiert => Clip { x0: f.x, y0: f.y, x1: f.x + f.w, y1: f.y + f.h },
            _ => crate::grafik::LEER,
        }
    }

    /// Fenster in Slot `s` als geändert merken.
    pub fn markiere_fenster(&mut self, s: usize) {
        let r = self.fenster_rechteck(s);
        self.markiere(r);
    }

    /// Geänderte Bereiche abholen (und vergessen).
    pub fn schmutz_holen(&mut self) -> Vec<Clip> {
        std::mem::take(&mut self.schmutz)
    }

    pub fn slot_von(&self, tid: u32) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|f| f.tid == tid))
    }

    pub fn faden(&mut self, tid: u32) -> Option<&mut FadenZ> {
        let s = self.slot_von(tid)?;
        self.slots[s].as_mut()
    }

    /// Slot des aktiven Fensters: oberstes normales (Ebene 0), nicht
    /// minimiertes Fenster; gibt es keines, das oberste überhaupt.
    pub fn aktiv(&self) -> Option<usize> {
        let sichtbar = |s: usize, nur_normal: bool| {
            self.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).is_some_and(|f| !f.minimiert && (!nur_normal || f.ebene == 0))
        };
        self.stapel.iter().rev().copied().find(|&s| sichtbar(s, true)).or_else(|| self.stapel.iter().rev().copied().find(|&s| sichtbar(s, false)))
    }

    fn ebene_von(&self, s: usize) -> i32 {
        self.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).map_or(0, |f| f.ebene)
    }

    /// Fenster in Slot `s` an die richtige Stelle seiner Ebene (oben in ihr).
    fn einreihen(&mut self, s: usize) {
        self.stapel.retain(|&x| x != s);
        let e = self.ebene_von(s);
        let pos = self.stapel.iter().position(|&x| self.ebene_von(x) > e).unwrap_or(self.stapel.len());
        self.stapel.insert(pos, s);
    }

    /// Funktion 18.25.2: Ebene setzen.
    pub fn ebene_setzen(&mut self, s: usize, ebene: i32) -> bool {
        let alt_aktiv = self.aktiv();
        let Some(f) = self.slots.get_mut(s).and_then(|f| f.as_mut()).and_then(|f| f.fenster.as_mut()) else { return false };
        f.ebene = ebene.clamp(-2, 1);
        f.immer_oben = f.ebene == 1;
        self.einreihen(s);
        self.markiere_fenster(s);
        let neu_aktiv = self.aktiv();
        if alt_aktiv != neu_aktiv {
            for a in [alt_aktiv, neu_aktiv].into_iter().flatten() {
                self.kopf_neu(a);
                self.markiere_fenster(a);
            }
        }
        true
    }

    pub fn faden_neu(&mut self, tid: u32, pid: u32, name: &str) {
        let z = FadenZ {
            tid,
            pid,
            name: name.into(),
            maske: 7,
            redraw: true,
            tasten: VecDeque::new(),
            knopf: None,
            maus_ev: false,
            maus_ereignisse: 0,
            druck_lage: None,
            hg_ev: false,
            ipc_ev: false,
            netz_ev: false,
            tasten_modus: 0,
            scroll: (0, 0),
            zoom: (1000, 0, 0),
            wartet: false,
            fenster: None,
        };
        match self.slots.iter().skip(2).position(Option::is_none) {
            Some(i) => self.slots[i + 2] = Some(z),
            None => self.slots.push(Some(z)),
        }
    }

    pub fn faden_weg(&mut self, tid: u32) {
        if self.touchpad && self.zeiger.iter().flatten().any(|(t, _)| *t == tid) {
            let r = self.zeiger_rechteck();
            self.markiere(r);
        }
        for z in self.zeiger.iter_mut() {
            if z.as_ref().is_some_and(|(t, _)| *t == tid) {
                *z = None;
            }
        }
        if self.textfokus == Some(tid) {
            self.textfokus = None;
            self.textfeld = None;
            self.tastatur_manuell = None;
        }
        if let Some(s) = self.slot_von(tid) {
            let alt_aktiv = self.aktiv();
            self.markiere_fenster(s);
            let hatte_fenster = self.slots[s].as_ref().is_some_and(|f| f.fenster.is_some());
            self.slots[s] = None;
            self.stapel.retain(|&x| x != s);
            if self.listen_beobachter == Some(tid) {
                self.listen_beobachter = None;
            } else if hatte_fenster {
                self.liste_geaendert();
            }
            if self.druck.is_some_and(|d| d.0 == s) {
                self.druck = None;
            }
            if alt_aktiv == Some(s)
                && let Some(n) = self.aktiv()
            {
                self.kopf_neu(n);
                self.markiere_fenster(n);
            }
        }
    }

    /// Fenster nach oben holen (innerhalb seiner Ebene) und – bei Ebene 0 –
    /// aktivieren. Minimierte Fenster werden wiederhergestellt.
    pub fn aktivieren(&mut self, s: usize) {
        let alt = self.aktiv();
        let war_oben = self.stapel.last() == Some(&s);
        // Wiederhergestellt (war minimiert): immer neu zusammensetzen – auch
        // wenn es schon oben lag und das aktive Fenster gleich bleibt
        // (Fenster der Ebene ≥ 1 wie die Bildschirmtastatur werden nie aktiv).
        let mut war_minimiert = false;
        if let Some(f) = self.slots[s].as_mut().and_then(|f| f.fenster.as_mut())
            && f.minimiert
        {
            f.minimiert = false;
            war_minimiert = true;
        }
        self.einreihen(s);
        let neu = self.aktiv();
        if alt != neu || war_minimiert || !war_oben {
            self.liste_geaendert();
        }
        if alt != neu {
            if let Some(a) = alt {
                self.kopf_neu(a);
                self.markiere_fenster(a);
            }
        }
        if !war_oben || alt != neu || war_minimiert {
            self.kopf_neu(s);
            self.markiere_fenster(s);
        }
    }

    /// Allen Programmen mit Fenster ein Neuzeichnen-Ereignis schicken (z. B.
    /// nach dem Wechsel der Sprache oder des Hintergrunds).
    pub fn alle_neu_zeichnen(&mut self) {
        for fz in self.slots.iter_mut().flatten() {
            if fz.fenster.is_some() {
                fz.redraw = true;
            }
        }
        wecken();
    }

    /// Kopf (Titelleiste) eines Fensters neu zeichnen (aktiv/inaktiv).
    pub fn kopf_neu(&mut self, s: usize) {
        self.rahmen_neu(s);
    }

    /// Rahmen und Kopf eines Skin-Fensters neu zeichnen (Programminhalt bleibt).
    pub fn rahmen_neu(&mut self, s: usize) {
        let aktiv = self.aktiv() == Some(s);
        let skin = self.skin.clone();
        let titel_farbe = self.farben[4];
        if let Some(f) = self.slots[s].as_mut().and_then(|f| f.fenster.as_mut())
            && matches!(f.stil, 3 | 4)
        {
            rahmen_zeichnen(f, &skin, aktiv);
            kopf_zeichnen(f, &skin, aktiv, titel_farbe);
        }
    }

    /// Oberstes Fenster unter (x, y).
    pub fn fenster_bei(&self, x: i32, y: i32) -> Option<usize> {
        self.stapel.iter().rev().copied().find(|&s| {
            self.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).is_some_and(|f| f.enthaelt(x, y))
        })
    }

    /// Hintergrundbild (Funktion 15) auf Bildschirmgrösse bringen (15.3/15.9).
    pub fn hintergrund_rendern(&mut self) {
        let (bw, bh) = self.hg_groesse;
        if bw == 0 || bh == 0 || self.hg_daten.len() < (bw * bh * 3) as usize {
            self.hintergrund_bild = None;
        } else {
            let (w, h) = (self.w as usize, self.h as usize);
            let mut p = Puffer::neu(w, h, 0);
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = if self.hg_modus == 1 {
                        (x as u32 % bw, y as u32 % bh)
                    } else {
                        ((x as u64 * u64::from(bw) / w as u64) as u32, (y as u64 * u64::from(bh) / h as u64) as u32)
                    };
                    let o = ((sy * bw + sx) * 3) as usize;
                    let c = u32::from(self.hg_daten[o + 2]) << 16 | u32::from(self.hg_daten[o + 1]) << 8 | u32::from(self.hg_daten[o]);
                    p.px[y * w + x] = zu565(c);
                }
            }
            self.hintergrund_bild = Some(p);
        }
        self.hg_zuletzt = Clip { x0: 0, y0: 0, x1: self.w, y1: self.h };
        self.alles();
        for fz in self.slots.iter_mut().flatten() {
            fz.hg_ev = true;
        }
        wecken();
    }

    /// Fensterliste für die Taskleiste (ESP32-OS-Erweiterung).
    pub fn fenster_liste(&self) -> Vec<FensterInfo> {
        let aktiv = self.aktiv();
        self.stapel
            .iter()
            .filter_map(|&s| {
                let fz = self.slots[s].as_ref()?;
                let f = fz.fenster.as_ref()?;
                Some(FensterInfo {
                    slot: s,
                    pid: fz.pid,
                    tid: fz.tid,
                    titel: if f.titel.is_empty() { fz.name.clone() } else { f.titel.clone() },
                    aktiv: aktiv == Some(s),
                    minimiert: f.minimiert,
                    ebene: f.ebene,
                })
            })
            .collect()
    }

    /// Zeigerbild an der aktuellen Mausposition (Fenster darunter bestimmt).
    fn zeigerbild(&self) -> &crate::zeiger::Zeigerbild {
        let h = self
            .fenster_bei(self.maus.x, self.maus.y)
            .and_then(|s| self.slots[s].as_ref()?.fenster.as_ref())
            .map_or(0, |f| f.zeiger);
        match h.checked_sub(1).and_then(|i| self.zeiger.get(i as usize)).and_then(|z| z.as_ref()) {
            Some((_, b)) => b,
            None => &self.pfeil,
        }
    }

    /// Bildschirmrechteck des Zeigers (für Dirty-Rects).
    pub fn zeiger_rechteck(&self) -> Clip {
        let z = self.zeigerbild();
        let (x, y) = (self.maus.x - z.hot.0, self.maus.y - z.hot.1);
        Clip { x0: x, y0: y, x1: x + 32, y1: y + 32 }
    }

    /// Nur das Rechteck `r` zusammensetzen (Dirty-Rect).
    pub fn komponieren_teil(&self, ziel: &mut Puffer, r: Clip) {
        let mut z = Zeichner::neu(ziel);
        z.clip = z.clip.schnitt(r);
        if z.clip.leer() {
            return;
        }
        let (x0, y0) = (z.clip.x0, z.clip.y0);
        let (cw, ch) = (z.clip.x1 - x0, z.clip.y1 - y0);
        // Fenster sind undurchsichtige Rechtecke: ab dem obersten, das das
        // Rechteck ganz bedeckt, ist alles darunter unsichtbar (B5: beim
        // Scrollen im Browser setzt der Compositor sonst Hintergrund, Bild
        // und alle Fenster neu – und hält dabei die Sperre)
        let bedeckt = self.stapel.iter().rposition(|&s| {
            self.slots[s].as_ref().and_then(|f| f.fenster.as_ref()).is_some_and(|f| {
                !f.minimiert
                    && f.x <= x0
                    && f.y <= y0
                    && f.x + f.buf.w as i32 >= x0 + cw
                    && f.y + f.buf.h as i32 >= y0 + ch
            })
        });
        if bedeckt.is_none() {
            z.fuellen565(x0, y0, cw, ch, zu565(self.hintergrund));
            if let Some(b) = &self.hintergrund_bild {
                z.puffer(b, 0, 0);
            }
        }
        for &s in &self.stapel[bedeckt.unwrap_or(0)..] {
            if let Some(f) = self.slots[s].as_ref().and_then(|f| f.fenster.as_ref())
                && !f.minimiert
            {
                z.puffer(&f.buf, f.x, f.y);
            }
        }
        // Zeiger obenauf (nur im Touchpad-Modus sichtbar)
        if self.touchpad {
            let zb = self.zeigerbild();
            let (x0, y0) = (self.maus.x - zb.hot.0, self.maus.y - zb.hot.1);
            for yy in 0..32 {
                for xx in 0..32 {
                    let p = zb.px[(yy * 32 + xx) as usize];
                    if p >> 24 >= 128 {
                        z.pixel(x0 + xx, y0 + yy, p & 0xFF_FFFF);
                    }
                }
            }
        }
    }

    /// Bildschirm zusammensetzen (ein Weg für Vollbild und Dirty-Rects).
    pub fn komponieren(&self, ziel: &mut Puffer) {
        self.komponieren_teil(ziel, Clip { x0: 0, y0: 0, x1: self.w, y1: self.h });
    }

    // ---------------------------------------------------------- Eingabe

    /// Zeiger an (x, y) mit Tastenzustand `tasten` (Bit 0 links, 1 rechts, 2 Mitte).
    pub fn maus(&mut self, x: i32, y: i32, tasten: u32) {
        let x = x.clamp(0, self.w - 1);
        let y = y.clamp(0, self.h - 1);
        let alt = self.maus.tasten;
        let gedrueckt = tasten & !alt;
        let los = alt & !tasten;
        let bewegt = x != self.maus.x || y != self.maus.y;
        if self.touchpad && bewegt {
            let alt = self.zeiger_rechteck();
            self.markiere(alt);
        }
        self.maus.x = x;
        self.maus.y = y;
        if self.touchpad && bewegt {
            let neu = self.zeiger_rechteck();
            self.markiere(neu);
        }
        self.maus.tasten = tasten;
        if gedrueckt & 3 != 0 {
            if let Some(s) = self.fenster_bei(x, y) {
                if self.ebene_von(s) >= -1 {
                    self.aktivieren(s);
                }
                if let Some(f) = self.slots[s].as_ref().and_then(|f| f.fenster.as_ref())
                    && let Some(k) = knopf_bei(f, x - f.x, y - f.y)
                {
                    self.druck = Some((s, k.id, tasten));
                }
            }
        }
        if los != 0
            && let Some((s, id, t)) = self.druck
            && los & t != 0
        {
            self.druck = None;
            let noch_drin = self.slots[s]
                .as_ref()
                .and_then(|f| f.fenster.as_ref())
                .and_then(|f| knopf_bei(f, x - f.x, y - f.y))
                .is_some_and(|k| k.id == id);
            if noch_drin {
                if id == 0xFFFF {
                    self.markiere_fenster(s);
                    if let Some(f) = self.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) {
                        f.minimiert = true;
                    }
                } else if let Some(fz) = self.slots[s].as_mut() {
                    fz.knopf = Some(id << 8 | (t & !1 & 0xFF));
                }
            }
        }
        if bewegt || gedrueckt != 0 || los != 0 {
            let aktiv = self.aktiv();
            // Über einem System-Fenster der obersten Ebene (Tastatur, Taskleiste,
            // Kontextmenü) gehört die Maus nur diesem – wie in KolibriOS, wo das
            // angeklickte Fenster aktiv würde und die anderen nichts merken.
            let system = self.fenster_bei(x, y).filter(|&s| self.ebene_von(s) >= 1);
            for (s, fz) in self.slots.iter_mut().enumerate() {
                let Some(fz) = fz else { continue };
                if system.is_some_and(|sy| sy != s) {
                    continue;
                }
                let inaktiv_sperre = fz.maske & 0x8000_0000 != 0 && aktiv != Some(s);
                let aussen = fz.maske & 0x4000_0000 != 0
                    && !fz.fenster.as_ref().is_some_and(|f| f.enthaelt(x, y));
                if !inaktiv_sperre && !aussen {
                    fz.maus_ev = true;
                    fz.maus_ereignisse |= (gedrueckt & 7) << 8 | (los & 7) << 16;
                    if gedrueckt != 0 {
                        fz.druck_lage = Some((x, y));
                    }
                }
            }
        }
        wecken();
    }

    /// Slot des Fensters, dessen Kopf (Titelleiste ohne Systemknöpfe) bei
    /// (x, y) liegt – nur Skin-Fenster der Ebene 0.
    pub fn kopf_bei(&self, x: i32, y: i32) -> Option<usize> {
        let s = self.fenster_bei(x, y)?;
        let f = self.slots[s].as_ref()?.fenster.as_ref()?;
        let (lx, ly) = (x - f.x, y - f.y);
        let im_kopf = matches!(f.stil, 3 | 4) && ly < self.skin.hoehe && f.ebene == 0;
        let auf_knopf = f.knoepfe.iter().any(|k| lx >= k.x && ly >= k.y && lx < k.x + k.w && ly < k.y + k.h);
        (im_kopf && !auf_knopf).then_some(s)
    }

    /// Fenster um (dx, dy) verschieben (Titelleiste ziehen). Unbewegliche
    /// und maximierte Fenster bleiben; die Titelleiste bleibt erreichbar.
    pub fn verschieben(&mut self, s: usize, dx: i32, dy: i32) {
        let (sw, sh) = (self.w, self.h);
        let hoehe = self.skin.hoehe;
        let alt = self.fenster_rechteck(s);
        let Some(f) = self.slots[s].as_mut().and_then(|f| f.fenster.as_mut()) else { return };
        if f.unbeweglich || f.vor_max.is_some() {
            return;
        }
        f.x = (f.x + dx).clamp(-f.w + 60, sw - 60);
        f.y = (f.y + dy).clamp(0, sh - hoehe);
        self.markiere(alt);
        self.markiere_fenster(s);
    }

    /// Maximieren bzw. wiederherstellen (Doppeltipp auf die Titelleiste).
    /// Neue Grösse → Redraw-Ereignis an das Programm.
    pub fn maximieren_umschalten(&mut self, s: usize) {
        let (l, o, r, u) = self.arbeitsflaeche;
        let skin = self.skin.clone();
        let titel_farbe = self.farben[4];
        let alt = self.fenster_rechteck(s);
        self.aktivieren(s);
        let Some(fz) = self.slots[s].as_mut() else { return };
        let Some(f) = fz.fenster.as_mut() else { return };
        if f.stil == 4 || f.unbeweglich {
            return; // feste Grösse
        }
        let vorher = f.vor_max;
        let (x, y, w, h) = match f.vor_max.take() {
            Some(v) => v,
            None => {
                f.vor_max = Some((f.x, f.y, f.w, f.h));
                (l, o, r - l + 1, u - o + 1)
            }
        };
        // neuer Puffer nur, wenn der System-Heap reicht – sonst bleibt das
        // Fenster, wie es ist (Fehlerbericht 24.11., grafik::Puffer::versuche)
        let Some(buf) = Puffer::versuche(w as usize, h as usize, f.arbeit & 0xFF_FFFF, crate::prozess::system_frei()) else {
            f.vor_max = vorher;
            return;
        };
        f.x = x;
        f.y = y;
        f.w = w;
        f.h = h;
        f.buf = buf;
        fenster_zeichnen(f, &skin, true, titel_farbe);
        fz.redraw = true;
        self.markiere(alt);
        self.markiere_fenster(s);
        wecken();
    }

    /// Zoom-Geste an das aktive Fenster (Faktor in Tausendstel, Mittelpunkt).
    pub fn zoom(&mut self, faktor_tausendstel: u32, mx: i32, my: i32) {
        if let Some(fz) = self.aktiv().and_then(|s| self.slots[s].as_mut()) {
            let z = u64::from(fz.zoom.0) * u64::from(faktor_tausendstel) / 1000;
            fz.zoom = (z.clamp(1, 1_000_000) as u32, mx, my);
            fz.maus_ev = true;
        }
        wecken();
    }

    /// Rollrad/Wischen: an das aktive Fenster (Funktion 37.7).
    pub fn scroll(&mut self, dx: i32, dy: i32) {
        if let Some(fz) = self.aktiv().and_then(|s| self.slots[s].as_mut()) {
            fz.maus_ereignisse |= if dy != 0 { 1 << 15 } else { 0 } | if dx != 0 { 1 << 23 } else { 0 };
            fz.scroll.0 += dx;
            fz.scroll.1 += dy;
            fz.maus_ev = true;
        }
        wecken();
    }

    /// Taste an das aktive Fenster: `code` = Rückgabewert von Funktion 2
    /// (al=0, ah=ASCII, Bits 16–23 Scancode).
    pub fn taste(&mut self, code: u32) {
        if let Some(fz) = self.aktiv().and_then(|s| self.slots[s].as_mut())
            && fz.tasten.len() < 120
        {
            fz.tasten.push_back(code);
        }
        wecken();
    }
}

fn knopf_bei(f: &Fenster, x: i32, y: i32) -> Option<&Knopf> {
    f.knoepfe.iter().rev().find(|k| x >= k.x && y >= k.y && x < k.x + k.w && y < k.y + k.h)
}

fn heller(c: u32, d: i32) -> u32 {
    let k = |v: u32| (v as i32 + d).clamp(0, 255) as u32;
    k(c >> 16 & 0xFF) << 16 | k(c >> 8 & 0xFF) << 8 | k(c & 0xFF)
}

/// Knopf zeichnen (3D-Stil: Verlauf, helle Kante oben/links, dunkle unten/rechts).
pub fn knopf_zeichnen(z: &mut Zeichner<'_>, x: i32, y: i32, w: i32, h: i32, farbe: u32) {
    for k in 0..h {
        let c = heller(farbe, 24 - (k * 40 / h.max(1)));
        z.fuellen565(x, y + k, w, 1, zu565(c));
    }
    let hell = heller(farbe, 60);
    let dunkel = heller(farbe, -70);
    z.fuellen565(x, y, w, 1, zu565(hell));
    z.fuellen565(x, y, 1, h, zu565(hell));
    z.fuellen565(x, y + h - 1, w, 1, zu565(dunkel));
    z.fuellen565(x + w - 1, y, 1, h, zu565(dunkel));
}

/// Bitmap an (x, 0) in den Zeichner kopieren.
fn bitmap(z: &mut Zeichner<'_>, b: &crate::skin::Bild, x: i32) {
    for yy in 0..b.h {
        for xx in 0..b.w {
            z.pixel(x + xx, yy, b.px[(yy * b.w + xx) as usize]);
        }
    }
}

/// Kopf eines Skin-Fensters (Stil 3/4) inkl. Titel – wie
/// `drawwindow_IV_caption` (links-Bitmap, Basis gekachelt, oper rechts) und
/// `window._.draw_window_caption` (Titel 8×16 in Farbe `grab_text`).
pub fn kopf_zeichnen(f: &mut Fenster, skin: &Skin, aktiv: bool, titel_farbe: u32) {
    let (w, hoehe) = (f.w, skin.hoehe);
    let titel = f.titel.clone();
    let mit_titel = f.flags & 1 != 0;
    let mut z = Zeichner::neu(&mut f.buf);
    z.clip = Clip { x0: 0, y0: 0, x1: w, y1: hoehe.min(z.p.h as i32) };
    match &skin.bilder {
        Some(b) => {
            let [links, oper, basis] = if aktiv { &b.aktiv } else { &b.inaktiv };
            bitmap(&mut z, links, 0);
            let ende = w - oper.w;
            let mut x = links.w;
            while basis.w > 0 && x < ende {
                bitmap(&mut z, basis, x);
                x += basis.w;
            }
            bitmap(&mut z, oper, ende);
        }
        None => {
            let c = if aktiv { skin.kopf_aktiv } else { skin.kopf_inaktiv };
            for k in 0..hoehe {
                z.fuellen565(0, k, w, 1, zu565(heller(c, 20 - k * 30 / hoehe.max(1))));
            }
            let aussen = if aktiv { skin.rahmen_aktiv.1 } else { skin.rahmen_inaktiv.1 };
            z.fuellen565(0, 0, w, 1, zu565(aussen));
            z.fuellen565(0, 0, 1, hoehe, zu565(aussen));
            z.fuellen565(w - 1, 0, 1, hoehe, zu565(aussen));
            for (b, zeichen) in [(skin.knopf_zu, b'x'), (skin.knopf_min, b'_')] {
                let bx = if b.0 < 0 { w + b.0 } else { b.0 };
                knopf_zeichnen(&mut z, bx, b.1, b.2, b.3, if zeichen == b'x' { 0xB03030 } else { c });
                let gl = [u32::from(zeichen)];
                let cx = bx + (b.2 - 6) / 2;
                let cy = b.1 + (b.3 - 9) / 2 - if zeichen == b'x' { 1 } else { 2 };
                let farbe = zu565(0xFFFFFF);
                schrift::zeichnen(&gl, Kodierung::Cp866Klein, 1, cx, cy, &mut |x, y, w, h| z.fuellen565(x, y, w, h, farbe), None);
            }
        }
    }
    let (l, o, r, u) = skin.raender;
    if mit_titel && !titel.is_empty() && w - l - r > 0 {
        let zeichen = schrift::zeichen(titel.as_bytes(), Kodierung::Utf8, false, usize::MAX);
        let n = ((w - l - r) / 8) as usize;
        let farbe = zu565(titel_farbe);
        let ty = (hoehe - u - o) / 2 + o - 8;
        schrift::zeichnen(&zeichen[..zeichen.len().min(n)], Kodierung::Utf8, 1, l, ty, &mut |x, y, w, h| {
            z.fuellen565(x, y, w, h, farbe)
        }, None);
    }
}

/// Rahmen eines Skin-Fensters unterhalb des Kopfes: aussen 1, rahmen 3,
/// innen 1 Pixel (`drawwindow_IV`).
pub fn rahmen_zeichnen(f: &mut Fenster, skin: &Skin, aktiv: bool) {
    let (w, h, k) = (f.w, f.h, skin.hoehe);
    let (innen, aussen, rahmen) = if aktiv { skin.rahmen_aktiv } else { skin.rahmen_inaktiv };
    let mut z = Zeichner::neu(&mut f.buf);
    for (i, c) in [(0, aussen), (1, rahmen), (2, rahmen), (3, rahmen), (4, innen)] {
        let c = zu565(c);
        let oben = (k - 1).max(i);
        z.fuellen565(i, h - 1 - i, w - 2 * i, 1, c);
        z.fuellen565(i, oben, 1, h - i - oben, c);
        z.fuellen565(w - 1 - i, oben, 1, h - i - oben, c);
    }
}

/// Ganzes Fenster nach Funktion 0 zeichnen (Rahmen, Kopf, Arbeitsbereich).
pub fn fenster_zeichnen(f: &mut Fenster, skin: &Skin, aktiv: bool, titel_farbe: u32) {
    let (w, h) = (f.w, f.h);
    let (ax, ay, aw, ah) = f.arbeitsbereich(skin);
    let fuellen = f.flags & 4 == 0;
    let arbeit = f.arbeit;
    match f.stil {
        3 | 4 => {
            rahmen_zeichnen(f, skin, aktiv);
            kopf_zeichnen(f, skin, aktiv, titel_farbe);
            if fuellen && h >= 26 {
                Zeichner::neu(&mut f.buf).rechteck(ax, ay, aw, ah, arbeit);
            }
        }
        0 | 2 => {
            let kopf = f.kopf & 0xFF_FFFF;
            let mut z = Zeichner::neu(&mut f.buf);
            if fuellen {
                z.rechteck(0, 0, w, h, arbeit);
            }
            if f.flags & 1 != 0 {
                z.fuellen565(0, 0, w, 21, zu565(kopf));
            }
            let c = zu565(heller(kopf, -60));
            z.fuellen565(0, 0, w, 1, c);
            z.fuellen565(0, h - 1, w, 1, c);
            z.fuellen565(0, 0, 1, h, c);
            z.fuellen565(w - 1, 0, 1, h, c);
            let _ = (ax, ay, aw, ah);
        }
        _ => {}
    }
}
