//! Die Anwendung: Titelbild, Hauptmenü, Optionen, Rekorde, Spiel, Pause,
//! Ergebnis und Demo – dazu der Musik-Schalter (Springen 5 s halten), die
//! Touch-Steuerung, der Spielstand und die Klangbefehle. Plattformunabhängig:
//! Die Firmware liefert Eingaben und Zeit, holt Bild, Klangbefehle und
//! Spielstand ab.

use crate::audio::{AudioCmd, Sfx, SongId};
use crate::campaign::{self, Difficulty, STORY};
use crate::level::Theme;
use crate::gfx::{Canvas, Overlay, hex, logical_h, logical_w, text_width};
use crate::hud::{self, HudData, clock};
use crate::input::{Input, btn};
use crate::level::{self, LEVELS};
use crate::play::{Phase, PuddingGame};
use crate::player::Controls;
use crate::render::{Renderer, Scratch, Target};
use crate::save::{LEVEL_COUNT, Progress, SAVE_SIZE, SaveData, Settings};
use crate::touchpad::{self, TouchPad};
use alloc::vec::Vec;
use core::fmt::Write as _;
use spatz::math::{Vec3, cos, sin};
use spatz::render::{Camera, DrawList, MeshStore};
use spatz::render::overlay::{TextBuf, fmt};

/// Fester Simulationstakt (wie am PC).
pub const SIM_DT: f32 = 1.0 / 120.0;
/// So lange Springen halten, um die Musik an-/auszuschalten.
pub const MUSIC_HOLD: f32 = 5.0;
/// Demo (der Pudding spielt von selbst) nach [`DEMO_AFTER`] Sekunden ohne
/// Eingabe im Titelbild – abgeschaltet: Das Titelbild bleibt stehen.
const DEMO_ENABLED: bool = false;
/// Nach so vielen Sekunden ohne Eingabe im Titelbild startet die Demo.
const DEMO_AFTER: f32 = 25.0;
/// Drehgeschwindigkeit der Menü-Kamera um den Pudding (rad/s).
const ORBIT_SPEED: f32 = 0.18;
/// Größtes Mesh (gemessen 630 Vertices) mit Reserve – Größe des [`Scratch`].
pub const MAX_MESH_VERTICES: usize = 700;
/// Zeichenaufrufe pro Frame (gemessen höchstens 164) mit Reserve.
const MAX_ITEMS: usize = 400;

/// Dauer des Aufleuchtens eines angetippten Menüpunkts (s).
const FLASH: f32 = 0.18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Title,
    Main,
    Options,
    /// Untermenü der Optionen: Touch-Steuerung einstellen.
    Controls,
    Records,
    ConfirmErase,
    Demo,
    Playing,
    Paused,
    Results,
    /// Schwierigkeit wählen (nach „Neues Spiel“).
    Difficulty,
    /// Story-Karte vor einem Kapitel bzw. Abspann.
    Story,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Controls,
    TouchSize,
    TouchAlpha,
    StickMode,
    Hand,
    Continue,
    NewGame,
    Diff(Difficulty),
    Options,
    Records,
    Music,
    MusicVol,
    SfxVol,
    Fps,
    Graphics,
    Brightness,
    Flip,
    Scaling,
    Erase,
    Back,
    Resume,
    ToCheckpoint,
    Restart,
    SaveQuit,
    NextLevel,
    Again,
    MainMenu,
    Yes,
    No,
    ZurKonsole,
}

impl Item {
    /// Mit Links/Rechts (oder Antippen am Rand) einstellbar.
    fn adjustable(self) -> bool {
        matches!(
            self,
            Item::MusicVol | Item::SfxVol | Item::Brightness | Item::TouchSize | Item::TouchAlpha
        )
    }
}

type Label = TextBuf<40>;

/// Ergebnis einer gewonnenen Partie.
#[derive(Clone, Copy, Debug)]
struct Results {
    time: f32,
    cherries: u32,
    total: u32,
    deaths: u32,
    new_record: bool,
}

pub use crate::hud::Perf;

/// Alles, was die Streifen eines Bildes zum Zeichnen brauchen: aufbereitete
/// Dreiecke, Partikel, Himmel ([`Renderer`]) und das aufgezeichnete HUD bzw.
/// Menü. Unabhängig vom Spielzustand – die Firmware zeichnet ein Bild,
/// während die App schon das nächste simuliert ([`App::swap_frame`]).
pub struct FrameData {
    renderer: Renderer,
    overlay: Overlay,
    /// Bildpunkte je logischer Einheit, mit denen `overlay` aufgezeichnet ist.
    overlay_s: usize,
    /// Vergrößertes Palettenbild zwischen Hintergrund und Overlay (Doom).
    pal_bild: Option<alloc::boxed::Box<PalBild>>,
    /// Fertiges RGB565-Vollbild (ESP32-OS-Desktop): Adresse, Breite, Höhe.
    vollbild: Option<(usize, usize, usize)>,
}

/// Bild mit 8-Bit-Farbindizes (z. B. Doom 320×200, Quake 320×240), in ein
/// Zielrechteck vergrößert. Zwischenspalten/-zeilen, die zwischen zwei
/// Quellpunkte fallen, werden gemittelt – so bleiben die Bildpunkte bei
/// krummen Faktoren (1,5 und 1,8) gleichmäßig.
///
/// Beim Vorbereiten wird nur das 8-Bit-Bild kopiert; umgerechnet wird erst
/// beim Zeichnen der Streifen, direkt in den (internen) Streifenspeicher –
/// so trägt der zweite Kern die Arbeit, während der erste schon das nächste
/// Bild des Spiels rechnet. Für die Breite 1,5-fach gibt es einen schnellen
/// Weg (2 Quellpunkte → 3 Bildpunkte).
struct PalBild {
    w: usize,
    h: usize,
    ziel: (i32, i32, i32, i32),
    /// Nummer des kopierten Bildes (gleich → nichts zu kopieren).
    nummer: u64,
    /// Je Zielspalte bzw. -zeile: Quellindex · 2 + „mit dem nächsten mitteln“.
    spalten: Vec<u32>,
    zeilen: Vec<u32>,
    /// Breite genau 1,5-fach (schneller Weg).
    anderthalb: bool,
    px: Vec<u8>,
    pal: [u16; 256],
    aktiv: bool,
}

#[inline(always)]
fn mitte565(a: u16, b: u16) -> u16 {
    ((a & 0xF7DE) >> 1) + ((b & 0xF7DE) >> 1)
}

impl PalBild {
    /// Abbildung Ziel → Quelle (Mitte auf Mitte), Anteile nahe ½ gemittelt.
    fn abbildung(n_quelle: usize, n_ziel: usize, out: &mut Vec<u32>) {
        out.clear();
        let k = n_quelle as f32 / n_ziel.max(1) as f32;
        let max = n_quelle.saturating_sub(1) as f32;
        for i in 0..n_ziel {
            let f = ((i as f32 + 0.5) * k - 0.5).clamp(0.0, max);
            let i0 = f as u32;
            let frac = f - i0 as f32;
            out.push(if frac < 0.25 {
                i0 * 2
            } else if frac > 0.75 {
                (i0 + 1).min(max as u32) * 2
            } else {
                i0 * 2 + 1
            });
        }
    }

    /// Quellzeile `sy` in Farben und Zielbreite nach `out`.
    #[inline(always)]
    fn zeile(&self, sy: usize, out: &mut [u16]) {
        let z = &self.px[sy * self.w..(sy + 1) * self.w];
        let pal = &self.pal;
        if self.anderthalb {
            for (o, q) in out.chunks_exact_mut(3).zip(z.chunks_exact(2)) {
                let (a, b) = (pal[q[0] as usize], pal[q[1] as usize]);
                o[0] = a;
                o[1] = mitte565(a, b);
                o[2] = b;
            }
            return;
        }
        let w1 = self.w - 1;
        for (o, &s) in out.iter_mut().zip(&self.spalten) {
            let sx = (s >> 1) as usize;
            let c = pal[z[sx] as usize];
            *o = if s & 1 != 0 { mitte565(c, pal[z[(sx + 1).min(w1)] as usize]) } else { c };
        }
    }

    fn zeichne(&self, t: &mut Target) {
        let (zx, zy, _, zh) = self.ziel;
        let (zx, zy) = (zx.max(0) as usize, zy.max(0) as usize);
        let zw = self.spalten.len();
        if zw == 0 || zx >= t.w {
            return;
        }
        let x1 = (zx + zw).min(t.w);
        let n = x1 - zx;
        let mut zweite = [0u16; 1024];
        let (y0, y1) = t.rows();
        for y in y0.max(zy)..y1.min(zy + zh.max(0) as usize) {
            let r = y - y0;
            let m = self.zeilen[y - zy];
            let sy = (m >> 1) as usize;
            let out = &mut t.color[r * t.w + zx..r * t.w + x1];
            self.zeile(sy, out);
            if m & 1 != 0 && n <= zweite.len() {
                let b = &mut zweite[..n];
                self.zeile((sy + 1).min(self.h - 1), b);
                for (o, &v) in out.iter_mut().zip(b.iter()) {
                    *o = mitte565(*o, v);
                }
            }
        }
    }
}

impl FrameData {
    pub fn new() -> Self {
        Self { renderer: Renderer::new(MAX_ITEMS), overlay: Overlay::new(), overlay_s: 1, pal_bild: None, vollbild: None }
    }

    /// Palettenbild `w`×`h` (Farbindizes `px`, Farben `pal`) vergrößert ins
    /// Rechteck `ziel` (x, y, Breite, Höhe in Bildpunkten) – nach
    /// [`FrameData::flach`] aufrufen, gilt bis zum nächsten `flach`.
    /// `nummer` kennzeichnet das Bild: bei gleicher Nummer (und Lage) wird
    /// nicht neu kopiert.
    pub fn palettenbild(&mut self, px: &[u8], w: usize, h: usize, pal: &[u16; 256], ziel: (i32, i32, i32, i32), nummer: u64) {
        assert!(px.len() >= w * h && w > 0 && h > 0, "Palettenbild zu klein");
        let b = self.pal_bild.get_or_insert_with(|| {
            alloc::boxed::Box::new(PalBild {
                w: 0,
                h: 0,
                ziel: (0, 0, 0, 0),
                nummer: u64::MAX,
                spalten: Vec::new(),
                zeilen: Vec::new(),
                anderthalb: false,
                px: Vec::new(),
                pal: [0; 256],
                aktiv: false,
            })
        });
        b.aktiv = true;
        let neu = (b.w, b.h, b.ziel) != (w, h, ziel);
        if neu {
            PalBild::abbildung(w, ziel.2.max(0) as usize, &mut b.spalten);
            PalBild::abbildung(h, ziel.3.max(0) as usize, &mut b.zeilen);
            (b.w, b.h, b.ziel) = (w, h, ziel);
            b.anderthalb = w % 2 == 0 && ziel.2 as usize == w / 2 * 3;
        }
        if neu || b.nummer != nummer {
            b.px.clear();
            b.px.extend_from_slice(&px[..w * h]);
            b.nummer = nummer;
        }
        b.pal = *pal;
    }

    /// Fertiges RGB565-Bild `w`×`h` als Hintergrund (nach [`FrameData::flach`];
    /// das Overlay wird darüber gezeichnet). Ohne Kopie: die Streifen lesen
    /// direkt aus `px`.
    ///
    /// # Safety
    /// `px` zeigt auf `w * h` Bildpunkte, die gültig und unverändert bleiben,
    /// bis dieses `FrameData` neu vorbereitet wird (ESP32-OS: Doppelpuffer).
    pub unsafe fn vollbild565(&mut self, px: *const u16, w: usize, h: usize) {
        self.vollbild = Some((px as usize, w, h));
    }

    /// Zeichnet die Zeilen des Streifens `t`: Himmel, 3D-Szene, HUD bzw.
    /// Menü. Nur lesend – beide Kerne zeichnen gleichzeitig.
    pub fn render_band(&self, s: &mut Scratch, t: &mut Target) {
        self.band(s, t, None);
    }

    /// Flaches Bild (2D-Spiele): Hintergrund `himmel(y)` je Zeile, darüber
    /// alles, was `zeichne` in Bildpunkten (Faktor 1, `w`×`h`) zeichnet –
    /// einmal aufgezeichnet, je Streifen abgespielt.
    pub fn flach(&mut self, w: usize, h: usize, himmel: &dyn Fn(usize) -> u16, zeichne: &mut dyn FnMut(&mut Canvas)) {
        self.renderer.begin_flat(w, h, himmel);
        self.overlay.clear();
        self.overlay_s = 1;
        self.vollbild = None;
        if let Some(b) = &mut self.pal_bild {
            b.aktiv = false;
        }
        {
            let mut c = Canvas::recorder(&mut self.overlay, w, h, 1);
            zeichne(&mut c);
        }
        let tint = self.overlay.hoist_full_tint(w as i32, h as i32);
        self.renderer.set_tint(tint);
    }

    /// Zeichnet zusätzlich über das vorbereitete Bild (gleicher Maßstab wie
    /// das bisherige Overlay) – z. B. FPS-Anzeige und Home-Taste der Konsole.
    pub fn dazu(&mut self, zeichne: &mut dyn FnMut(&mut Canvas)) {
        let (w, h) = self.renderer.size();
        let mut c = Canvas::recorder(&mut self.overlay, w, h, self.overlay_s);
        zeichne(&mut c);
    }

    /// 3D-Bild für Spiele außerhalb dieser Crate, Teil 1: Kamera, Umgebung
    /// (Himmel, Licht, Nebel) und Zeichenliste sortieren. Danach liefert
    /// [`FrameData::projizieren`] Bildpunkte; [`FrameData::szene_abschliessen`]
    /// zeichnet das Overlay auf und bereitet die Dreiecke auf.
    pub fn szene_beginnen(&mut self, cam: &Camera, env: &spatz::render::Environment, w: usize, h: usize, store: &MeshStore, list: &DrawList) {
        self.renderer.begin(cam, env, w, h);
        self.renderer.sort(store, list);
        if let Some(b) = &mut self.pal_bild {
            b.aktiv = false;
        }
    }

    /// Weltpunkt → (x, y, Tiefe) in Bildpunkten (nach `szene_beginnen`).
    pub fn projizieren(&self, p: spatz::math::Vec3) -> Option<(f32, f32, f32)> {
        self.renderer.project(p)
    }

    /// Teil 2: Overlay (in Bildpunkten, Faktor 1) aufzeichnen und alle
    /// Dreiecke aufbereiten – danach ist das Bild unabhängig vom Spiel.
    pub fn szene_abschliessen(&mut self, store: &MeshStore, list: &DrawList, zeichne: &mut dyn FnMut(&mut Canvas)) {
        let (w, h) = self.renderer.size();
        self.overlay.clear();
        self.overlay_s = 1;
        {
            let mut c = Canvas::recorder(&mut self.overlay, w, h, 1);
            zeichne(&mut c);
        }
        let tint = self.overlay.hoist_full_tint(w as i32, h as i32);
        self.renderer.set_tint(tint);
        self.renderer.build(store, list);
    }

    /// Bildpunkte je Einheit, in denen das Overlay aufgezeichnet ist (für
    /// [`FrameData::dazu`]: Pudding-Panik 2, flache Bilder 1).
    pub fn overlay_massstab(&self) -> usize {
        self.overlay_s
    }

    /// Aufbereitete Dreiecke (Messung).
    pub fn prims(&self) -> usize {
        self.renderer.prim_count()
    }

    /// Wie [`FrameData::render_band`]; ohne [`Renderer::build`] wird die
    /// Szene aus `scene` direkt gezeichnet (falls angegeben).
    fn band(&self, s: &mut Scratch, t: &mut Target, scene: Option<(&MeshStore, &DrawList)>) {
        use crate::render::{RENDER_PROFILE as P, profile_clock};
        use core::sync::atomic::Ordering::Relaxed;
        let t0 = profile_clock();
        if let Some((p, w, h)) = self.vollbild {
            let (y0, y1) = t.rows();
            let n = w.min(t.w);
            for y in y0..y1.min(h) {
                // SAFETY: Vertrag von `vollbild565` (Bild bleibt gültig).
                let q = unsafe { core::slice::from_raw_parts((p as *const u16).add(y * w), n) };
                let r = y - y0;
                t.color[r * t.w..r * t.w + n].copy_from_slice(q);
            }
            let scale = self.overlay_s;
            let mut c = Canvas::scaled(t.color, t.w, t.h, scale, t.row0);
            self.overlay.replay(&mut c);
            return;
        }
        self.renderer.clear(t);
        let t1 = profile_clock();
        match scene {
            Some((store, list)) => self.renderer.draw(s, t, store, list),
            None => self.renderer.draw_built(s, t),
        }
        if let Some(b) = &self.pal_bild
            && b.aktiv
        {
            b.zeichne(t);
        }
        let t2 = profile_clock();
        let scale = self.overlay_s;
        let mut c = Canvas::scaled(t.color, t.w, t.h, scale, t.row0);
        self.overlay.replay(&mut c);
        let t3 = profile_clock();
        P[0].fetch_add(t1.wrapping_sub(t0), Relaxed);
        P[1].fetch_add(t2.wrapping_sub(t1), Relaxed);
        P[2].fetch_add(t3.wrapping_sub(t2), Relaxed);
    }
}

pub struct App {
    pub save: SaveData,
    pub screen: Screen,
    game: PuddingGame,
    game_level: usize,
    /// Was die Streifen des nächsten Bildes zum Zeichnen brauchen.
    frame: FrameData,
    /// Arbeitsspeicher für [`App::render_full`] (die Firmware bringt je Kern
    /// einen eigenen mit).
    scratch: Scratch,
    cursor: usize,
    main_cursor: usize,
    /// Schwierigkeit der laufenden Partie.
    pub difficulty: Difficulty,
    /// Angezeigte Story-Karte und wie lange schon.
    story: usize,
    story_t: f32,
    prev_held: u8,
    prev_buttons: u8,
    prev_touch: bool,
    /// Springen-Taste erst nach dem Loslassen wieder als Sprung werten.
    jump_block: bool,
    jump_latch: bool,
    jump_hold: f32,
    music_toggled: bool,
    /// Touch-Steuerung sichtbar (an, sobald im Spiel getippt wird; aus,
    /// sobald ein echter Knopf gedrückt wird – außer bei `touch_always`).
    pub touch_ui: bool,
    /// Touch-Steuerung immer zeigen (Konsole ohne Knöpfe).
    pub touch_always: bool,
    /// Handy/Tablet: keine Board-Optionen (Helligkeit, Bild drehen,
    /// Vergrößern) und kein Hinweis auf den Musik-Schalter per Knopf.
    pub phone: bool,
    /// Handy: Renderfaktor für die volle Display-Auflösung (Höhe ≈ 180 · Faktor);
    /// 0 = Board (Faktor 1 oder 2 je nach Einstellung „Grafik“).
    pub native_scale: usize,
    /// Berührung, die vor dem Spielstart begann, zählt erst nach dem Loslassen.
    touch_block: bool,
    pad: TouchPad,
    /// Angetippter Menüpunkt leuchtet kurz auf: (Bildschirm, Zeile y, Zeit).
    flash: Option<(Screen, i32, f32)>,
    repeat_t: f32,
    acc: f32,
    orbit: f32,
    idle: f32,
    toast: Label,
    toast_t: f32,
    results: Option<Results>,
    audio: Vec<AudioCmd>,
    pending_save: bool,
    play_time: f32,
    pub perf: Perf,
    /// Akkustand in Prozent (falls die Firmware ihn kennt).
    pub battery: Option<u8>,
    /// Läuft unter ESP32-OS: Hauptmenü bekommt „ZUM DESKTOP“ (Rückweg).
    pub konsole: bool,
    /// „ZUM DESKTOP“ wurde gewählt (einmaliges Signal, siehe [`App::zur_konsole`]).
    zur_konsole: bool,
}

fn parse_level(i: usize) -> level::LevelDef {
    level::parse(LEVELS[i].1).expect("eingebautes Level ist gültig")
}

impl App {
    pub fn new(save: Option<SaveData>) -> Self {
        let save = save.unwrap_or_default();
        let mut game = PuddingGame::new(&parse_level(0), false);
        game.title_t = 0.0;
        game.rig.intro = 0.0;
        let mut app = Self {
            save,
            screen: Screen::Title,
            game,
            game_level: 0,
            frame: FrameData::new(),
            scratch: Scratch::with_capacity(MAX_MESH_VERTICES),
            cursor: 0,
            main_cursor: 0,
            difficulty: Difficulty::Easy,
            story: 0,
            story_t: 0.0,
            prev_held: 0,
            prev_buttons: 0,
            prev_touch: false,
            jump_block: false,
            jump_latch: false,
            jump_hold: 0.0,
            music_toggled: false,
            touch_ui: false,
            touch_always: false,
            phone: false,
            native_scale: 0,
            touch_block: false,
            pad: TouchPad::default(),
            flash: None,
            repeat_t: 0.0,
            acc: 0.0,
            orbit: 0.6,
            idle: 0.0,
            toast: Label::new(),
            toast_t: 0.0,
            results: None,
            audio: Vec::with_capacity(32),
            pending_save: false,
            play_time: 0.0,
            perf: Perf::default(),
            battery: None,
            konsole: false,
            zur_konsole: false,
        };
        let s = app.save.settings;
        app.audio.push(AudioCmd::MusicVolume(s.music_vol));
        app.audio.push(AudioCmd::SfxVolume(s.sfx_vol));
        app.audio.push(AudioCmd::MusicEnabled(s.music));
        app.audio.push(AudioCmd::Song(Some(SongId::Menu)));
        app
    }

    // ----------------------------------------------------------------------
    // Schnittstelle zur Firmware
    // ----------------------------------------------------------------------

    /// Klangbefehle abholen (an den Audio-Kern weiterreichen).
    pub fn drain_audio(&mut self, mut f: impl FnMut(AudioCmd)) {
        for c in self.audio.drain(..) {
            f(c);
        }
    }

    /// Spielstand, falls gespeichert werden soll (Laufnummer schon erhöht).
    pub fn take_save(&mut self) -> Option<[u8; SAVE_SIZE]> {
        if !core::mem::take(&mut self.pending_save) {
            return None;
        }
        self.save.seq = self.save.seq.wrapping_add(1);
        self.save.play_time_s = self.save.play_time_s.saturating_add(self.play_time as u32);
        self.play_time -= (self.play_time as u32) as f32;
        Some(self.save.to_bytes())
    }

    /// „ZUM DESKTOP“ gewählt? Liefert das Signal nur einmal.
    pub fn zur_konsole(&mut self) -> bool {
        core::mem::take(&mut self.zur_konsole)
    }

    /// Aktuelle Einstellungen (Auflösung, Drehung, Helligkeit für die Firmware).
    pub fn settings(&self) -> &Settings {
        &self.save.settings
    }

    /// Renderauflösung: 640×360 (scharf) oder 320×180 (schnell).
    pub fn render_size(&self) -> (usize, usize) {
        let hires = self.save.settings.hires;
        let s = if self.native_scale > 0 {
            // Handy: VOLL = Display-Auflösung, SPARSAM = halbe Auflösung.
            if hires { self.native_scale } else { self.native_scale.div_ceil(2) }
        } else if hires {
            2
        } else {
            1
        };
        (logical_w() * s, logical_h() * s)
    }

    /// Autopilot in der laufenden Partie an-/ausschalten (PC-Simulation, Tests).
    pub fn set_autopilot(&mut self, on: bool) {
        self.game.autopilot = if on {
            let mut ap = crate::autopilot::Autopilot::new();
            ap.resync(&self.game.world, self.game.player.body.pos);
            Some(ap)
        } else {
            None
        };
    }

    fn sfx(&mut self, s: Sfx) {
        self.audio.push(AudioCmd::Sfx(s));
    }

    fn show_toast(&mut self, s: &str) {
        self.toast.clear();
        let _ = self.toast.write_str(s);
        self.toast_t = 1.6;
    }

    fn request_save(&mut self) {
        self.pending_save = true;
    }

    // ----------------------------------------------------------------------
    // Aktualisieren
    // ----------------------------------------------------------------------

    /// Ein Frame: Eingabe auswerten, Simulation im festen Takt.
    pub fn update(&mut self, input: Input, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        let touching = input.touching();
        if !touching {
            self.touch_block = false;
        }
        let tap = if self.prev_touch { None } else { input.first_touch() };
        self.prev_touch = touching;

        // Touch-Steuerung im Spiel: wird zu (virtuellen) Knöpfen.
        let mut held = input.held;
        if self.screen == Screen::Playing {
            if input.held & !self.prev_buttons != 0 && !self.touch_always {
                self.touch_ui = false;
            }
            if touching && !self.touch_block {
                self.touch_ui = true;
                let layout = touchpad::Layout::new(&self.save.settings);
                held |= self.pad.update(&layout, &input.touch);
            } else {
                self.pad.release();
            }
        } else {
            self.pad.release();
        }
        self.pad.tick(dt);
        self.prev_buttons = input.held;
        if let Some((_, _, t)) = &mut self.flash {
            *t += dt;
            if *t > FLASH {
                self.flash = None;
            }
        }

        let pressed = held & !self.prev_held;
        self.prev_held = held;
        if held & btn::JUMP == 0 {
            self.jump_block = false;
        }
        self.music_switch(held, dt);
        self.toast_t -= dt;
        if held != 0 || touching {
            self.idle = 0.0;
        } else {
            self.idle += dt;
        }

        match self.screen {
            Screen::Title => {
                if pressed != 0 || tap.is_some() {
                    self.sfx(Sfx::MenuSelect);
                    self.goto(Screen::Main);
                    self.jump_block = true;
                } else if DEMO_ENABLED && self.idle > DEMO_AFTER {
                    self.start_demo();
                }
            }
            Screen::Demo => {
                if pressed != 0 || tap.is_some() {
                    self.leave_demo();
                    self.jump_block = true;
                }
            }
            Screen::Story => {
                self.story_t += dt;
                if pressed & btn::MENU != 0 {
                    self.sfx(Sfx::MenuBack);
                    self.back_to_menu();
                } else if (pressed != 0 || tap.is_some()) && self.story_t > 0.8 && !self.jump_block {
                    self.sfx(Sfx::MenuSelect);
                    self.story_done();
                }
            }
            Screen::Playing => {
                let lr = btn::LEFT | btn::RIGHT;
                let pause_combo = held & lr == lr && pressed & lr != 0;
                let tap_pause = tap.is_some_and(|(x, y)| touchpad::in_pause(x, y));
                if pressed & btn::MENU != 0 || pause_combo || tap_pause {
                    self.sfx(Sfx::MenuBack);
                    self.goto(Screen::Paused);
                } else if pressed & btn::JUMP != 0 && !self.jump_block {
                    self.jump_latch = true;
                }
            }
            _ => self.menu_input(held, pressed, tap, dt),
        }

        // Simulation im festen Takt.
        self.acc += dt;
        let mut steps = 0;
        while self.acc >= SIM_DT && steps < 12 {
            self.acc -= SIM_DT;
            steps += 1;
            self.sim_step(held);
        }
        if steps == 12 {
            self.acc = 0.0;
        }
        self.after_sim();
    }

    fn sim_step(&mut self, held: u8) {
        match self.screen {
            Screen::Paused => {}
            Screen::Playing => {
                let axis = |pos: u8, neg: u8| (held & pos != 0) as i32 as f32 - (held & neg != 0) as i32 as f32;
                let user = Controls {
                    move_x: (axis(btn::RIGHT, btn::LEFT) + self.pad.stick.0).clamp(-1.0, 1.0),
                    move_y: (axis(btn::UP, btn::DOWN) + self.pad.stick.1).clamp(-1.0, 1.0),
                    jump_pressed: core::mem::take(&mut self.jump_latch),
                    jump_held: held & btn::JUMP != 0 && !self.jump_block,
                    cam_turn: 0.0,
                };
                self.play_time += SIM_DT;
                let c = self.game.controls(user, SIM_DT);
                self.game.step(&c, SIM_DT);
            }
            Screen::Demo => {
                let c = self.game.controls(Controls::default(), SIM_DT);
                self.game.step(&c, SIM_DT);
            }
            _ => {
                self.orbit += SIM_DT * ORBIT_SPEED;
                // Als Kulisse schmilzt der Pudding nicht.
                if self.screen != Screen::Results {
                    self.game.player.melt = 0.0;
                }
                self.game.step(&Controls::default(), SIM_DT);
            }
        }
    }

    /// Nach den Simulationsschritten: Klänge, Checkpoints, Ziel.
    fn after_sim(&mut self) {
        let allow_giggle = matches!(self.screen, Screen::Playing | Screen::Title);
        let in_menu = !matches!(self.screen, Screen::Playing | Screen::Demo | Screen::Results);
        for i in 0..self.game.events.len() {
            let e = self.game.events[i];
            if (e == Sfx::Giggle && !allow_giggle) || (in_menu && e != Sfx::Giggle) {
                continue;
            }
            self.audio.push(AudioCmd::Sfx(e));
        }
        self.game.events.clear();
        // Schwer: Leben aufgebraucht → Kapitel von vorn (nach der Platsch-Animation).
        if self.screen == Screen::Playing
            && let Some(lives) = self.difficulty.lives()
            && self.game.player.deaths >= lives
            && self.game.phase == Phase::Playing
        {
            self.start_level(self.game_level);
            self.show_toast("KEINE LEBEN MEHR - KAPITEL VON VORN!");
            return;
        }
        if self.screen == Screen::Playing {
            if core::mem::take(&mut self.game.checkpoint_reached) {
                self.save.progress = Some(self.game.progress(self.game_level as u8));
                self.request_save();
            }
            if let Some(t) = self.game.finished_t()
                && t > 2.6
            {
                self.finish_level();
            }
        } else {
            self.game.checkpoint_reached = false;
        }
        if self.screen == Screen::Demo && self.game.finished_t().is_some_and(|t| t > 5.0) {
            self.leave_demo();
        }
    }

    fn finish_level(&mut self) {
        let g = &self.game;
        let time = g.finish_time.unwrap_or(g.time);
        let r = Results {
            time,
            cherries: g.player.cherries,
            total: g.total_cherries,
            deaths: g.player.deaths,
            new_record: false,
        };
        let new_record =
            self.save.record_win(self.game_level, (time * 1000.0) as u32, r.cherries.min(255) as u8, r.deaths);
        self.results = Some(Results { new_record, ..r });
        if new_record {
            self.sfx(Sfx::Record);
        }
        // Automatisch weiter: „Weiterspielen“ beginnt beim nächsten Kapitel.
        let next = self.game_level + 1;
        self.save.progress = (next < LEVEL_COUNT).then(|| Progress {
            level: next as u8,
            difficulty: self.difficulty.as_u8(),
            ..Progress::default()
        });
        self.request_save();
        self.goto(Screen::Results);
    }

    /// Springen 5 s halten: Musik an/aus (überall im Spiel).
    fn music_switch(&mut self, held: u8, dt: f32) {
        if held & btn::JUMP == 0 {
            self.jump_hold = 0.0;
            self.music_toggled = false;
            return;
        }
        self.jump_hold += dt;
        if self.jump_hold >= MUSIC_HOLD && !self.music_toggled {
            self.music_toggled = true;
            let on = !self.save.settings.music;
            self.save.settings.music = on;
            self.audio.push(AudioCmd::MusicEnabled(on));
            self.sfx(if on { Sfx::MusicOn } else { Sfx::MusicOff });
            self.show_toast(if on { "MUSIK AN" } else { "MUSIK AUS" });
            self.request_save();
        }
    }

    // ----------------------------------------------------------------------
    // Zustände wechseln
    // ----------------------------------------------------------------------

    fn goto(&mut self, s: Screen) {
        let menu_music = matches!(s, Screen::Title | Screen::Main | Screen::Options | Screen::Records);
        let was_menu_music = matches!(
            self.screen,
            Screen::Title | Screen::Main | Screen::Options | Screen::Controls | Screen::Records | Screen::ConfirmErase
        );
        if menu_music && !was_menu_music {
            self.audio.push(AudioCmd::Song(Some(SongId::Menu)));
        }
        if matches!(s, Screen::Playing | Screen::Demo) && !matches!(self.screen, Screen::Paused) {
            self.audio.push(AudioCmd::Song(Some(SongId::Level)));
        }
        if s == Screen::Playing {
            // Der Finger, der gerade „Weiter“ getippt hat, springt nicht gleich.
            self.touch_block = true;
        }
        // Zurück ins Hauptmenü: Auswahl bleibt auf dem Punkt, von dem man kam.
        let back_to_main = s == Screen::Main && matches!(self.screen, Screen::Options | Screen::Records);
        let back_to_options = s == Screen::Options && self.screen == Screen::Controls;
        if self.screen == Screen::Main {
            self.main_cursor = self.cursor;
        }
        self.screen = s;
        self.cursor = if back_to_main { self.main_cursor } else { 0 };
        if back_to_options {
            let (items, n) = self.items();
            self.cursor = items[..n].iter().position(|&i| i == Item::Controls).unwrap_or(0);
        }
        self.repeat_t = 0.0;
    }

    /// Szene für ein Level bereitstellen (baut nur bei Levelwechsel neu).
    fn load_level(&mut self, level: usize) {
        if level != self.game_level {
            // Altes Level zuerst freigeben (ein leeres Level baut fast
            // nichts), dann das neue bauen – so passt beides nie gleichzeitig
            // in den Speicher.
            self.game = PuddingGame::new(&level::LevelDef::default(), false);
            self.game = PuddingGame::new(&parse_level(level), false);
            self.game_level = level;
        }
    }

    fn start_level(&mut self, level: usize) {
        self.load_level(level);
        self.game.set_difficulty(self.difficulty);
        self.game.autopilot = None;
        self.game.restart();
        self.results = None;
        self.save.progress = Some(self.game.progress(level as u8));
        self.request_save();
        self.jump_block = true;
        self.goto(Screen::Playing);
    }

    /// Kapitel `i` direkt starten (Simulation, Tests).
    pub fn start_chapter(&mut self, i: usize, d: Difficulty) {
        self.difficulty = d;
        self.start_level(i.min(LEVEL_COUNT - 1));
    }

    /// Die laufende Partie (nur lesen).
    pub fn game(&self) -> &PuddingGame {
        &self.game
    }

    fn continue_game(&mut self) {
        let Some(p) = self.save.progress else { return };
        let level = (p.level as usize).min(LEVEL_COUNT - 1);
        self.difficulty = Difficulty::from_u8(p.difficulty);
        self.load_level(level);
        self.game.set_difficulty(self.difficulty);
        self.game.autopilot = None;
        self.game.resume(&p);
        self.results = None;
        self.jump_block = true;
        self.goto(Screen::Playing);
    }

    /// Zurück ins Menü: Szene wird wieder zur Kulisse (Pudding kichert vor sich hin).
    fn back_to_menu(&mut self) {
        self.game.autopilot = None;
        self.game.restart();
        self.game.title_t = 0.0;
        self.game.rig.intro = 0.0;
        self.results = None;
        self.goto(Screen::Main);
    }

    /// Story-Karte `i` zeigen (Kapitel `i` als Kulisse; nach dem letzten
    /// Kapitel der Abspann).
    fn show_story(&mut self, i: usize) {
        self.story = i.min(STORY.len() - 1);
        self.story_t = 0.0;
        self.load_level(self.story.min(LEVEL_COUNT - 1));
        self.game.autopilot = None;
        self.game.restart();
        self.game.title_t = 0.0;
        self.game.rig.intro = 0.0;
        self.results = None;
        self.jump_block = true;
        self.goto(Screen::Story);
    }

    /// Story-Karte weggetippt: Kapitel starten bzw. nach dem Abspann zum Titel.
    fn story_done(&mut self) {
        if self.story < LEVEL_COUNT {
            self.start_level(self.story);
        } else {
            self.save.progress = None;
            self.request_save();
            self.back_to_menu();
            self.goto(Screen::Title);
        }
    }

    fn start_demo(&mut self) {
        self.game.set_difficulty(Difficulty::Easy);
        self.game.restart();
        self.game.autopilot = Some(crate::autopilot::Autopilot::new());
        self.goto(Screen::Demo);
    }

    fn leave_demo(&mut self) {
        self.game.autopilot = None;
        self.game.restart();
        self.game.title_t = 0.0;
        self.game.rig.intro = 0.0;
        self.idle = 0.0;
        self.goto(Screen::Title);
    }

    // ----------------------------------------------------------------------
    // Menüs
    // ----------------------------------------------------------------------

    fn items(&self) -> ([Item; 12], usize) {
        let mut v = [Item::Back; 12];
        let mut n = 0;
        let mut add = |it: Item| {
            v[n] = it;
            n += 1;
        };
        match self.screen {
            Screen::Main => {
                if self.save.progress.is_some() {
                    add(Item::Continue);
                }
                add(Item::NewGame);
                add(Item::Options);
                add(Item::Records);
                if self.konsole {
                    add(Item::ZurKonsole);
                }
            }
            Screen::Options => {
                for it in [
                    Item::Music,
                    Item::MusicVol,
                    Item::SfxVol,
                    Item::Graphics,
                    Item::Scaling,
                    Item::Brightness,
                    Item::Flip,
                    Item::Fps,
                    Item::Controls,
                    Item::Erase,
                    Item::Back,
                ] {
                    if self.phone && matches!(it, Item::Scaling | Item::Brightness | Item::Flip) {
                        continue;
                    }
                    if self.konsole && it == Item::Fps {
                        continue;
                    }
                    add(it);
                }
            }
            Screen::Controls => {
                for it in [Item::TouchSize, Item::TouchAlpha, Item::StickMode, Item::Hand, Item::Back] {
                    add(it);
                }
            }
            Screen::Records => add(Item::Back),
            Screen::ConfirmErase => {
                add(Item::No);
                add(Item::Yes);
            }
            Screen::Paused => {
                for it in [Item::Resume, Item::ToCheckpoint, Item::Restart, Item::SaveQuit] {
                    add(it);
                }
            }
            Screen::Results => {
                add(Item::NextLevel);
                add(Item::Again);
                add(Item::MainMenu);
            }
            Screen::Difficulty => {
                for d in Difficulty::ALL {
                    add(Item::Diff(d));
                }
                add(Item::Back);
            }
            _ => {}
        }
        (v, n)
    }

    fn label(&self, it: Item) -> Label {
        let s = &self.save.settings;
        let onoff = |b: bool| if b { "AN" } else { "AUS" };
        let bar = |v: u8| {
            let mut t = TextBuf::<12>::new();
            for i in 0..10 {
                let _ = t.write_char(if i < v { '#' } else { '-' });
            }
            t
        };
        match it {
            Item::Continue => {
                let p = self.save.progress.unwrap_or_default();
                fmt(format_args!("WEITERSPIELEN: KAPITEL {}", (p.level as usize).min(LEVEL_COUNT - 1) + 1))
            }
            Item::NewGame => fmt(format_args!("NEUES SPIEL")),
            Item::Diff(d) => fmt(format_args!("{}", d.name())),
            Item::Options => fmt(format_args!("OPTIONEN")),
            Item::Records => fmt(format_args!("REKORDE")),
            Item::Music => fmt(format_args!("MUSIK: {}", onoff(s.music))),
            Item::MusicVol => fmt(format_args!("MUSIK      {}", bar(s.music_vol).as_str())),
            Item::SfxVol => fmt(format_args!("EFFEKTE    {}", bar(s.sfx_vol).as_str())),
            Item::Fps => fmt(format_args!("FPS-ANZEIGE: {}", onoff(s.show_fps))),
            Item::Graphics if self.native_scale > 0 => {
                let (w, h) = self.render_size();
                fmt(format_args!("GRAFIK: {} {w}X{h}", if s.hires { "VOLL" } else { "SPARSAM" }))
            }
            Item::Graphics => fmt(format_args!("GRAFIK: {}", if s.hires { "640X360" } else { "PERFORMANCE 320X180" })),
            Item::Brightness => fmt(format_args!("HELLIGKEIT {}", bar(s.brightness).as_str())),
            Item::Flip => fmt(format_args!("BILD DREHEN: {}", onoff(s.flip))),
            Item::Scaling => fmt(format_args!("VERGRÖSSERN: {}", if s.smooth { "WEICH (BILINEAR)" } else { "SCHARF" })),
            Item::Controls => fmt(format_args!("TOUCH-STEUERUNG >")),
            Item::TouchSize => fmt(format_args!("GRÖSSE: {}", touchpad::SIZE_NAMES[(s.touch_size as usize).min(2)])),
            Item::TouchAlpha => {
                let mut b = TextBuf::<8>::new();
                for i in 1..=5 {
                    let _ = b.write_char(if i <= s.touch_alpha { '#' } else { '-' });
                }
                fmt(format_args!("DECKKRAFT  {}", b.as_str()))
            }
            Item::StickMode => fmt(format_args!("JOYSTICK: {}", if s.stick_fixed { "FEST" } else { "SCHWEBEND" })),
            Item::Hand => fmt(format_args!("HAND: {}", if s.left_handed { "LINKSHÄNDER" } else { "RECHTSHÄNDER" })),
            Item::Erase => fmt(format_args!("SPIELSTAND LÖSCHEN")),
            Item::Back => fmt(format_args!("ZURÜCK")),
            Item::Resume => fmt(format_args!("WEITER")),
            Item::ToCheckpoint => fmt(format_args!("ZUM CHECKPOINT")),
            Item::Restart => fmt(format_args!("KAPITEL NEU STARTEN")),
            Item::SaveQuit => fmt(format_args!("SPEICHERN + MENÜ")),
            Item::NextLevel if self.game_level + 1 >= LEVEL_COUNT => fmt(format_args!("WEITER")),
            Item::NextLevel => fmt(format_args!("WEITER ZU KAPITEL {}", self.game_level + 2)),
            Item::Again => fmt(format_args!("NOCHMAL")),
            Item::MainMenu => fmt(format_args!("HAUPTMENÜ")),
            Item::Yes => fmt(format_args!("JA, ALLES LÖSCHEN")),
            Item::No => fmt(format_args!("NEIN")),
            Item::ZurKonsole => fmt(format_args!("ZUM DESKTOP")),
        }
    }

    fn menu_top(&self) -> i32 {
        match self.screen {
            Screen::Main => 68,
            Screen::Options => 26,
            Screen::Controls => 34,
            Screen::Records => 148,
            Screen::ConfirmErase => 96,
            Screen::Paused => 70,
            Screen::Results => 128,
            Screen::Difficulty => 62,
            _ => 60,
        }
    }

    const ROW: i32 = 13;

    fn menu_input(&mut self, held: u8, pressed: u8, tap: Option<(u16, u16)>, dt: f32) {
        let (items, n) = self.items();
        if n == 0 {
            return;
        }
        // Auf/Ab mit Wiederholung beim Halten.
        let mut step = 0i32;
        if pressed & btn::UP != 0 {
            step = -1;
            self.repeat_t = 0.0;
        } else if pressed & btn::DOWN != 0 {
            step = 1;
            self.repeat_t = 0.0;
        } else if held & (btn::UP | btn::DOWN) != 0 {
            self.repeat_t += dt;
            if self.repeat_t > 0.42 {
                self.repeat_t -= 0.12;
                step = if held & btn::UP != 0 { -1 } else { 1 };
            }
        }
        if step != 0 {
            self.cursor = (self.cursor as i32 + step).rem_euclid(n as i32) as usize;
            self.sfx(Sfx::MenuMove);
        }
        self.cursor = self.cursor.min(n - 1);
        let mut activate = pressed & btn::JUMP != 0 && !self.jump_block;
        let mut delta = (pressed & btn::RIGHT != 0) as i32 - (pressed & btn::LEFT != 0) as i32;
        if let Some((x, y)) = tap {
            let top = self.menu_top();
            let i = (y as i32 - top + 2).div_euclid(Self::ROW);
            if (0..n as i32).contains(&i) {
                self.cursor = i as usize;
                // Sichtbares Feedback: die Zeile leuchtet kurz auf.
                self.flash = Some((self.screen, top + i * Self::ROW, 0.0));
                // Einstellbare Punkte: linkes/rechtes Drittel = weniger/mehr.
                let third = logical_w() as u16 / 3;
                if items[self.cursor].adjustable() && x < third {
                    delta = -1;
                } else if items[self.cursor].adjustable() && x > 2 * third {
                    delta = 1;
                } else {
                    activate = true;
                }
            }
        }
        let item = items[self.cursor];
        if delta != 0 {
            self.adjust(item, delta);
        }
        if activate {
            self.jump_block = true;
            self.activate(item);
        } else if pressed & btn::MENU != 0 {
            self.back();
        }
    }

    fn back(&mut self) {
        self.sfx(Sfx::MenuBack);
        match self.screen {
            Screen::Options | Screen::Records => {
                self.request_save();
                self.goto(Screen::Main);
            }
            Screen::Difficulty => self.goto(Screen::Main),
            Screen::ConfirmErase | Screen::Controls => self.goto(Screen::Options),
            Screen::Paused => self.goto(Screen::Playing),
            Screen::Main => self.goto(Screen::Title),
            _ => {}
        }
    }

    /// Werte mit Links/Rechts ändern.
    fn adjust(&mut self, item: Item, d: i32) {
        let s = &mut self.save.settings;
        let vol = |v: u8, lo: i32| (v as i32 + d).clamp(lo, 10) as u8;
        match item {
            Item::MusicVol => {
                s.music_vol = vol(s.music_vol, 0);
                let v = s.music_vol;
                self.audio.push(AudioCmd::MusicVolume(v));
            }
            Item::SfxVol => {
                s.sfx_vol = vol(s.sfx_vol, 0);
                let v = s.sfx_vol;
                self.audio.push(AudioCmd::SfxVolume(v));
            }
            Item::Brightness => s.brightness = vol(s.brightness, 1),
            Item::TouchSize => s.touch_size = (s.touch_size as i32 + d).clamp(0, 2) as u8,
            Item::TouchAlpha => s.touch_alpha = (s.touch_alpha as i32 + d).clamp(1, 5) as u8,
            Item::Music | Item::Fps | Item::Graphics | Item::Flip | Item::Scaling | Item::StickMode | Item::Hand => {
                return self.activate(item);
            }
            _ => return,
        }
        self.sfx(Sfx::MenuMove);
    }

    fn activate(&mut self, item: Item) {
        self.sfx(Sfx::MenuSelect);
        let s = &mut self.save.settings;
        match item {
            Item::Continue => self.continue_game(),
            Item::NewGame => self.goto(Screen::Difficulty),
            Item::Diff(d) => {
                self.difficulty = d;
                self.show_story(0);
            }
            Item::Options => self.goto(Screen::Options),
            Item::Records => self.goto(Screen::Records),
            Item::Music => {
                s.music = !s.music;
                let on = s.music;
                self.audio.push(AudioCmd::MusicEnabled(on));
            }
            Item::MusicVol | Item::SfxVol | Item::Brightness => self.adjust(item, 1),
            Item::Fps => s.show_fps = !s.show_fps,
            Item::Graphics => s.hires = !s.hires,
            Item::Flip => s.flip = !s.flip,
            Item::Scaling => s.smooth = !s.smooth,
            Item::Controls => self.goto(Screen::Controls),
            Item::TouchSize => s.touch_size = (s.touch_size + 1) % 3,
            Item::TouchAlpha => s.touch_alpha = s.touch_alpha % 5 + 1,
            Item::StickMode => s.stick_fixed = !s.stick_fixed,
            Item::Hand => s.left_handed = !s.left_handed,
            Item::Erase => {
                self.goto(Screen::ConfirmErase);
            }
            Item::Yes => {
                let settings = self.save.settings;
                let seq = self.save.seq;
                self.save = SaveData { seq, settings, ..SaveData::default() };
                self.request_save();
                self.show_toast("SPIELSTAND GELÖSCHT");
                self.goto(Screen::Options);
            }
            Item::No => self.goto(Screen::Options),
            Item::Back if self.screen == Screen::Controls => self.goto(Screen::Options),
            Item::Back if self.screen == Screen::Difficulty => self.goto(Screen::Main),
            Item::Back => {
                self.request_save();
                self.goto(Screen::Main);
            }
            Item::Resume => self.goto(Screen::Playing),
            Item::ToCheckpoint => {
                let melt = self.game.player.melt;
                self.game.respawn();
                self.game.player.melt = melt;
                self.goto(Screen::Playing);
            }
            Item::Restart => self.start_level(self.game_level),
            Item::SaveQuit => {
                self.save.progress = Some(self.game.progress(self.game_level as u8));
                self.request_save();
                self.show_toast("GESPEICHERT");
                self.back_to_menu();
            }
            Item::NextLevel => self.show_story(self.game_level + 1),
            Item::Again => self.start_level(self.game_level),
            Item::MainMenu => self.back_to_menu(),
            Item::ZurKonsole => {
                self.request_save();
                self.zur_konsole = true;
            }
        }
    }

    // ----------------------------------------------------------------------
    // Zeichnen
    // ----------------------------------------------------------------------

    fn camera(&self, alpha: f32) -> Camera {
        match self.screen {
            Screen::Playing | Screen::Paused | Screen::Results | Screen::Demo => self.game.game_camera(alpha),
            _ => {
                // Kulisse: Kamera kreist langsam um den kichernden Pudding.
                let p = self.game.player.render_pos(alpha) + Vec3::new(0.0, 0.55, 0.0);
                // Zwischen den Schritten interpoliert (wie alle Positionen).
                let a = self.orbit - (1.0 - alpha) * SIM_DT * ORBIT_SPEED;
                let eye = p + Vec3::new(sin(a) * 3.4, 1.1, cos(a) * 3.4);
                let mut cam = Camera { fov_y: 1.0, near: 0.1, far: 80.0, ..Camera::default() };
                cam.look_at(eye, p);
                cam
            }
        }
    }

    /// Bereitet den Frame für ein Bild der Größe `w`×`h` vor (Zeichenliste,
    /// Kamera, Sortierung). Danach [`App::render_band`] für jeden Streifen –
    /// auch auf zwei Kernen gleichzeitig.
    pub fn begin_frame(&mut self, w: usize, h: usize) {
        self.prepare_scene(w, h);
        self.build_triangles();
    }

    /// Erster Teil von [`App::begin_frame`]: Zeichenliste, Kamera, Sortieren.
    pub fn prepare_scene(&mut self, w: usize, h: usize) {
        let alpha = (self.acc / SIM_DT).clamp(0.0, 1.0);
        let cam = self.camera(alpha);
        self.game.prepare_with(alpha, &cam);
        self.frame.renderer.begin(&cam, &self.game.env, w, h);
        self.frame.renderer.sort(&self.game.store, &self.game.list);
        self.record_overlay(w, h);
    }

    /// HUD bzw. Menü und FPS-Anzeige einmal aufzeichnen; jeder Streifen
    /// spielt davon nur ab, was ihn berührt.
    fn record_overlay(&mut self, w: usize, h: usize) {
        let mut ov = core::mem::take(&mut self.frame.overlay);
        ov.clear();
        {
            self.frame.overlay_s = (w / logical_w()).max(1);
            let mut c = Canvas::recorder(&mut ov, w, h, self.frame.overlay_s);
            self.draw_overlay(&mut c);
            // In der Konsole zeigt der Rahmen die FPS (einheitlich in allen Spielen).
            if self.save.settings.show_fps && !self.konsole {
                hud::perf_overlay(&mut c, &self.perf);
            }
        }
        // Abtönung des ganzen Bildes (Menü, Pause, Tod) übernimmt der Renderer.
        let tint = ov.hoist_full_tint(logical_w() as i32, logical_h() as i32);
        self.frame.renderer.set_tint(tint);
        self.frame.overlay = ov;
    }

    /// Zweiter Teil: alle Dreiecke einmal aufbereiten statt pro Streifen.
    pub fn build_triangles(&mut self) {
        self.frame.renderer.build(&self.game.store, &self.game.list);
    }

    /// Zeichnet die Zeilen des Streifens `t`: 3D-Szene, HUD bzw. Menü,
    /// FPS-Anzeige. Liest die App nur – darf parallel laufen.
    pub fn render_band(&self, s: &mut Scratch, t: &mut Target) {
        self.frame.band(s, t, Some((&self.game.store, &self.game.list)));
    }

    /// Tauscht die vorbereiteten Bilddaten gegen `fd` (Firmware: `fd` wird
    /// gezeichnet, während die App schon das nächste Bild vorbereitet).
    pub fn swap_frame(&mut self, fd: &mut FrameData) {
        core::mem::swap(&mut self.frame, fd);
    }

    /// Ganzer Frame in einem Durchgang (PC-Simulation, Tests).
    pub fn render_full(&mut self, t: &mut Target) {
        self.begin_frame(t.w, t.h);
        let mut s = core::mem::take(&mut self.scratch);
        self.render_band(&mut s, t);
        self.scratch = s;
    }

    /// Aufbereitete Dreiecke des aktuellen Frames (Messung).
    pub fn debug_prims(&self) -> usize {
        self.frame.renderer.prim_count()
    }

    /// Gezeichnete Dreiecke im letzten [`App::render_full`].
    pub fn tris_drawn(&self) -> u32 {
        self.scratch.stats.tris_drawn
    }

    /// (größtes Mesh in Vertices, Zeichenaufrufe dieses Frames) – zum
    /// Bemessen des Renderer-Speichers.
    pub fn debug_sizes(&self) -> (usize, usize) {
        (self.game.store.max_vertices(), self.game.list.items.len())
    }

    pub(crate) fn draw_overlay(&self, c: &mut Canvas) {
        let (w, h) = (logical_w() as i32, logical_h() as i32);
        match self.screen {
            Screen::Playing | Screen::Demo => {
                // Steuerelemente zuerst: das HUD liegt immer obenauf.
                if self.screen == Screen::Playing && (self.touch_ui || self.touch_always) {
                    self.pad.draw(c, &touchpad::Layout::new(&self.save.settings), true);
                }
                self.draw_hud(c);
            }
            Screen::Title => {
                c.text_centered(w / 2, 30, "PUDDING-PANIK", hex(0xFFE14D), hex(0x80203A), 3);
                c.text_centered(w / 2, 58, "EIN WACKELPUDDING AUF DER FLUCHT", hex(0xFFFFFF), hex(0x401020), 1);
                if (self.orbit * 2.4) as i32 % 2 == 0 {
                    c.text_centered(w / 2, 140, "ZUM STARTEN TIPPEN", hex(0xFFF2A0), hex(0x401020), 1);
                }
                if !self.phone {
                    c.text_centered(w / 2, 164, "MUSIK AN/AUS: SPRUNG 5 S HALTEN", hex(0xC8C8FF), hex(0x201030), 1);
                }
            }
            Screen::Main
            | Screen::Options
            | Screen::Controls
            | Screen::Records
            | Screen::ConfirmErase
            | Screen::Difficulty => self.draw_menu(c),
            Screen::Story => self.draw_story(c),
            Screen::Paused => {
                self.draw_hud(c);
                c.tint_rect(0, 0, w, h, 0, 16);
                c.text_centered(w / 2, 34, "PAUSE", hex(0xFFFFFF), hex(0x301020), 2);
                self.draw_items(c);
            }
            Screen::Results => self.draw_results(c),
        }
        if self.toast_t > 0.0 {
            let s = self.toast.as_str();
            let tw = text_width(s, 1);
            c.darken_rect(w / 2 - tw / 2 - 4, 118, tw + 8, 12);
            c.text_centered(w / 2, 120, s, hex(0xFFE14D), 0, 1);
        }
        // Fortschritt beim Halten für den Musik-Schalter.
        if self.jump_hold > 1.0 && !self.music_toggled {
            let k = ((self.jump_hold - 1.0) / (MUSIC_HOLD - 1.0)).clamp(0.0, 1.0);
            let (x, y) = (w - 66, 4 + 2 * 13 + 12);
            c.darken_rect(x - 2, y - 2, 66, 11);
            c.tiny(x, y, "MUSIK", hex(0xFFFFFF));
            c.frame(x + 22, y - 1, 38, 7, hex(0xFFFFFF));
            c.fill_rect(x + 23, y, (36.0 * k) as i32, 5, hex(0xFFE14D));
        }
    }

    fn draw_hud(&self, c: &mut Canvas) {
        let g = &self.game;
        let d = HudData {
            cherries: g.player.cherries,
            total_cherries: g.total_cherries,
            time: g.finish_time.unwrap_or(g.time),
            melt: g.player.melt,
            hint: g.hint.map(|i| g.world.hints[i].text.as_str()),
            title_t: g.title_t,
            level_name: LEVELS[self.game_level].0,
            demo: self.screen == Screen::Demo,
            dead_t: if let Phase::Dead { t } = g.phase { Some(t) } else { None },
            frost: g.theme == Theme::Freezer,
            goal: campaign::goal_text(self.game_level),
            lives: match self.screen {
                Screen::Playing | Screen::Paused => {
                    self.difficulty.lives().map(|l| (l.saturating_sub(g.player.deaths), l))
                }
                _ => None,
            },
            battery: self.battery,
        };
        hud::draw(c, &d, &g.messages);
    }

    fn draw_items(&self, c: &mut Canvas) {
        let (items, n) = self.items();
        let top = self.menu_top();
        let w = logical_w() as i32;
        for (i, &it) in items.iter().take(n).enumerate() {
            let y = top + i as i32 * Self::ROW;
            let label = self.label(it);
            let sel = i == self.cursor;
            if sel {
                c.blend_rect(24, y - 3, w - 48, Self::ROW, hex(0xFF5C8A));
            }
            // Angetippt: kurz hell aufleuchten (statt Tastengefühl).
            if let Some((screen, fy, t)) = self.flash
                && screen == self.screen
                && fy == y
            {
                c.tint_rect(24, y - 3, w - 48, Self::ROW, hex(0xFFFFFF), (20.0 * (1.0 - t / FLASH)) as u32);
            }
            let col = if sel {
                hex(0xFFFFFF)
            } else {
                hex(0xFFE8F0)
            };
            c.text_centered(w / 2, y, label.as_str(), col, hex(0x301020), 1);
        }
    }

    fn draw_menu(&self, c: &mut Canvas) {
        let (w, h) = (logical_w() as i32, logical_h() as i32);
        c.tint_rect(0, 0, w, h, hex(0x2A1030), 14);
        match self.screen {
            Screen::Main => {
                c.text_centered(w / 2, 16, "PUDDING-PANIK", hex(0xFFE14D), hex(0x80203A), 2);
                if let Some(b) = self.battery {
                    let t = fmt::<16>(format_args!("AKKU {}%", b));
                    c.tiny(w - 4 - crate::gfx::tiny_width(t.as_str()), 2, t.as_str(), hex(0xC0FFC0));
                }
                if let Some(p) = self.save.progress {
                    let i = (p.level as usize).min(LEVEL_COUNT - 1);
                    let d = Difficulty::from_u8(p.difficulty).name();
                    let t = fmt::<48>(format_args!("KAPITEL {}: {} ({})", i + 1, LEVELS[i].0, d));
                    hud::centered_fit(c, 42, t.as_str(), hex(0xC8FFC8), 1);
                }
            }
            Screen::Options => c.text_centered(w / 2, 6, "OPTIONEN", hex(0xFFE14D), hex(0x80203A), 2),
            Screen::Difficulty => {
                c.text_centered(w / 2, 14, "WIE HART SOLL ES SEIN?", hex(0xFFE14D), hex(0x80203A), 2);
                if let Some(Item::Diff(d)) = self.items().0.get(self.cursor).copied() {
                    let [a, b] = d.info();
                    c.text_centered(w / 2, 36, a, hex(0xFFFFFF), hex(0x301020), 1);
                    c.text_centered(w / 2, 46, b, hex(0xFFFFFF), hex(0x301020), 1);
                }
            }
            Screen::Controls => {
                // Vorschau mit den aktuellen Einstellungen (Knüppel in Ruhe).
                let l = touchpad::Layout::new(&self.save.settings);
                TouchPad::default().draw(c, &l, false);
                c.text_centered(w / 2, 8, "TOUCH-STEUERUNG", hex(0xFFE14D), hex(0x80203A), 2);
            }
            Screen::ConfirmErase => {
                c.text_centered(w / 2, 40, "SPIELSTAND LÖSCHEN?", hex(0xFF8080), hex(0x401010), 1);
                c.text_centered(w / 2, 56, "REKORDE UND FORTSCHRITT", hex(0xFFFFFF), 0, 1);
                c.text_centered(w / 2, 66, "GEHEN VERLOREN.", hex(0xFFFFFF), 0, 1);
            }
            Screen::Records => {
                c.text_centered(w / 2, 12, "REKORDE", hex(0xFFE14D), hex(0x80203A), 2);
                // Eine Zeile pro Kapitel: Name links, Bestzeit und Kirschen rechts.
                let mut y = 32;
                for (i, (name, _)) in LEVELS.iter().enumerate() {
                    let r = &self.save.records[i];
                    let t = fmt::<40>(format_args!("{} {}", i + 1, name));
                    c.text_shadow(20, y, t.as_str(), hex(0xFFD0E0), 0, 1);
                    let line = if r.best_time_ms == 0 {
                        fmt::<40>(format_args!("--:--.-"))
                    } else {
                        let (m, s, ds) = clock(r.best_time_ms as f32 / 1000.0);
                        fmt::<40>(format_args!("{:02}:{:02}.{}  ● {}", m, s, ds, r.best_cherries))
                    };
                    c.text_shadow(w - 20 - text_width(line.as_str(), 1), y, line.as_str(), hex(0xFFFFFF), 0, 1);
                    y += 12;
                }
                let total = self.save.play_time_s + self.play_time as u32;
                let t = fmt::<40>(format_args!("SPIELZEIT GESAMT {}:{:02} H", total / 3600, (total / 60) % 60));
                c.text_centered(w / 2, y + 2, t.as_str(), hex(0xC8C8FF), 0, 1);
            }
            _ => {}
        }
        self.draw_items(c);
        let hint = if matches!(self.screen, Screen::Options | Screen::Controls) {
            "ANTIPPEN ZUM WÄHLEN, AM RAND: WERT ÄNDERN"
        } else {
            "ZUM AUSWÄHLEN ANTIPPEN"
        };
        c.darken_rect(0, h - 11, w, 11);
        c.text_centered(w / 2, h - 9, hint, hex(0xC8C8FF), 0, 1);
    }

    fn draw_story(&self, c: &mut Canvas) {
        let (w, h) = (logical_w() as i32, logical_h() as i32);
        c.tint_rect(0, 0, w, h, hex(0x120818), 22);
        let card = &STORY[self.story];
        hud::centered_fit(c, 18, card.title, hex(0xFFE14D), 2);
        let mut y = 48;
        for (i, line) in card.lines.iter().enumerate() {
            if self.story_t > 0.3 + i as f32 * 0.25 {
                c.text_centered(w / 2, y, line, hex(0xFFFFFF), hex(0x301020), 1);
            }
            y += 12;
        }
        if self.story_t > 0.8 && (self.story_t * 2.0) as i32 % 2 == 0 {
            c.text_centered(w / 2, h - 18, "ZUM WEITERSPIELEN TIPPEN", hex(0xFFF2A0), hex(0x401020), 1);
        }
    }

    fn draw_results(&self, c: &mut Canvas) {
        let w = logical_w() as i32;
        let Some(r) = self.results else { return };
        c.tint_rect(0, 0, w, logical_h() as i32, hex(0x200818), 16);
        c.text_centered(w / 2, 28, "GESCHAFFT!", hex(0x7CFF7C), hex(0x103010), 3);
        let (m, s, ds) = clock(r.time);
        let l1 = fmt::<40>(format_args!("ZEIT {:02}:{:02}.{}", m, s, ds));
        c.text_centered(w / 2, 56, l1.as_str(), hex(0xFFFFFF), 0, 1);
        let l2 = fmt::<40>(format_args!("KIRSCHEN {}/{}   PLATSCHER {}", r.cherries, r.total, r.deaths));
        c.text_centered(w / 2, 67, l2.as_str(), hex(0xFFFFFF), 0, 1);
        let rating = match (r.cherries == r.total, r.deaths) {
            (true, 0) => "PERFEKTER PUDDING!",
            (true, _) => "KIRSCHEN-KÖNIG!",
            (false, 0) => "OHNE PLATSCHER!",
            _ => "LECKER!",
        };
        c.text_centered(w / 2, 80, rating, hex(0xFFE14D), hex(0x402010), 1);
        // Sterne: Ziel, alle Kirschen, ohne Platscher.
        let stars = 1 + (r.cherries == r.total) as i32 + (r.deaths == 0) as i32;
        for i in 0..3 {
            let col = if i < stars { hex(0xFFD23C) } else { hex(0x504050) };
            c.text_centered(w / 2 - 20 + i * 20, 92, "*", col, 0, 2);
        }
        if r.new_record && ((self.orbit * 6.0) as i32) % 2 == 0 {
            c.text_centered(w / 2, 111, "NEUER REKORD!", hex(0xFF5C8A), hex(0x401020), 1);
        }
        self.draw_items(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{H, W};
    use alloc::vec;

    fn frame(app: &mut App, held: u8, frames: usize) {
        for _ in 0..frames {
            app.update(Input::buttons(held), 1.0 / 30.0);
        }
    }

    fn press(app: &mut App, b: u8) {
        frame(app, b, 1);
        frame(app, 0, 1);
    }

    fn touch(app: &mut App, pts: &[(u16, u16)], frames: usize) {
        let mut input = Input::default();
        for (slot, &p) in input.touch.iter_mut().zip(pts) {
            *slot = Some(p);
        }
        for _ in 0..frames {
            app.update(input, 1.0 / 30.0);
        }
    }

    /// Vom Hauptmenü (ohne Spielstand) per Knopf: NEUES SPIEL → Schwierigkeit
    /// `d` → Story-Karte → Kapitel 1.
    fn new_game_buttons(app: &mut App, d: usize) {
        press(app, btn::JUMP);
        assert_eq!(app.screen, Screen::Difficulty);
        for _ in 0..d {
            press(app, btn::DOWN);
        }
        press(app, btn::JUMP);
        assert_eq!(app.screen, Screen::Story);
        frame(app, 0, 30);
        press(app, btn::JUMP);
        assert_eq!(app.screen, Screen::Playing);
    }

    /// Dasselbe per Touch (Hauptmenü ohne Spielstand, „Leicht“).
    fn new_game_touch(app: &mut App) {
        tap(app, 160, 0); // NEUES SPIEL
        assert_eq!(app.screen, Screen::Difficulty);
        tap(app, 160, 0); // LEICHT
        assert_eq!(app.screen, Screen::Story);
        touch(app, &[], 30);
    }

    #[test]
    fn menu_flow_play_pause_save_and_continue() {
        let mut app = App::new(None);
        assert_eq!(app.screen, Screen::Title);
        press(&mut app, btn::JUMP);
        assert_eq!(app.screen, Screen::Main);
        // Ohne Spielstand: NEUES SPIEL steht oben.
        new_game_buttons(&mut app, 0);
        assert!(app.take_save().is_some(), "Neues Spiel legt einen Spielstand an");
        frame(&mut app, btn::UP, 60);
        assert!(app.game.time > 1.5);
        press(&mut app, btn::MENU);
        assert_eq!(app.screen, Screen::Paused);
        let t = app.game.time;
        frame(&mut app, 0, 30);
        assert_eq!(app.game.time, t, "Pause hält die Zeit an");
        // SPEICHERN + MENÜ
        for _ in 0..3 {
            press(&mut app, btn::DOWN);
        }
        press(&mut app, btn::JUMP);
        assert_eq!(app.screen, Screen::Main);
        let bytes = app.take_save().expect("gespeichert");
        let loaded = SaveData::from_bytes(&bytes).unwrap();
        assert!(loaded.progress.is_some());
        // Neu starten mit dem Spielstand → WEITERSPIELEN.
        let mut app2 = App::new(Some(loaded));
        press(&mut app2, btn::JUMP);
        press(&mut app2, btn::JUMP);
        assert_eq!(app2.screen, Screen::Playing);
        assert!(app2.game.time >= t - 0.01, "Zeit wird fortgesetzt");
    }

    /// Nur mit dem Touchscreen: Menü antippen, mit dem Steuerknüppel laufen,
    /// rechts tippen zum Springen, oben links tippen für die Pause.
    #[test]
    fn touch_only_play() {
        let mut app = App::new(None);
        touch(&mut app, &[(160, 90)], 1);
        touch(&mut app, &[], 1);
        assert_eq!(app.screen, Screen::Main);
        // NEUES SPIEL antippen, Schwierigkeit wählen, Story-Karte wegtippen.
        new_game_touch(&mut app);
        touch(&mut app, &[(160, 70)], 1);
        assert_eq!(app.screen, Screen::Playing);
        // Finger bleibt liegen → noch gesperrt, kein Sprung, kein Knüppel.
        touch(&mut app, &[(160, 70)], 5);
        assert!(!app.touch_ui);
        touch(&mut app, &[], 1);
        // Warten, bis der Pudding steht, dann Knüppel nach oben schieben.
        frame(&mut app, 0, 150);
        let start = app.game.player.body.pos;
        touch(&mut app, &[(60, 130)], 1);
        touch(&mut app, &[(60, 100)], 15);
        assert!(app.touch_ui, "Touch-Steuerung wird angezeigt");
        let moved = (app.game.player.body.pos - start).length();
        assert!(moved > 0.8, "Pudding läuft per Steuerknüppel ({moved})");
        // Gleichzeitig rechts tippen: Sprung (der Pudding hebt ab).
        let y0 = app.game.player.body.pos.y;
        touch(&mut app, &[(60, 100), (260, 120)], 8);
        assert!(app.game.player.body.pos.y > y0 + 0.2, "Sprung per Touch");
        touch(&mut app, &[], 40);
        // Oben links: Pause.
        touch(&mut app, &[(10, 30)], 1);
        assert_eq!(app.screen, Screen::Paused);
        touch(&mut app, &[], 1);
        // Echter Knopf blendet die Touch-Steuerung wieder aus.
        touch(&mut app, &[(160, 72)], 1); // WEITER
        assert_eq!(app.screen, Screen::Playing);
        touch(&mut app, &[], 1);
        press(&mut app, btn::UP);
        assert!(!app.touch_ui);
        // Konsole ohne Knöpfe: Steuerung bleibt immer sichtbar.
        app.touch_always = true;
        press(&mut app, btn::UP);
        let (mut c, mut d) = (vec![0u16; W * H], vec![0u16; W * H]);
        let with = {
            app.render_full(&mut Target::new(&mut c, &mut d, W, H));
            c.clone()
        };
        app.touch_always = false;
        app.render_full(&mut Target::new(&mut c, &mut d, W, H));
        assert_ne!(with, c, "Steuerung wird gezeichnet");
    }

    /// Ein angetipptes Menüfeld in einem Bildschirm (Zeile `i`, x-Position).
    fn tap(app: &mut App, x: u16, i: i32) {
        let y = (app.menu_top() + i * App::ROW + 3) as u16;
        touch(app, &[(x, y)], 1);
        touch(app, &[], 1);
    }

    /// In der Spielekonsole: Hauptmenü-Eintrag „ZUM DESKTOP“ gibt einmal das
    /// Signal zur Rückkehr (ohne Konsole gibt es den Eintrag nicht).
    #[test]
    fn hauptmenue_fuehrt_zur_konsole() {
        let mut app = App::new(None);
        touch(&mut app, &[(160, 90)], 1);
        touch(&mut app, &[], 1);
        let (items, n) = app.items();
        assert!(!items[..n].contains(&Item::ZurKonsole), "ohne Konsole kein Eintrag");

        let mut app = App::new(None);
        app.konsole = true;
        touch(&mut app, &[(160, 90)], 1);
        touch(&mut app, &[], 1);
        assert_eq!(app.screen, Screen::Main);
        let (items, n) = app.items();
        let i = items[..n].iter().position(|&it| it == Item::ZurKonsole).expect("Eintrag im Hauptmenü");
        assert_eq!(i, n - 1, "steht unten");
        assert!(!app.zur_konsole());
        tap(&mut app, 160, i as i32);
        assert!(app.zur_konsole(), "Signal nach dem Antippen");
        assert!(!app.zur_konsole(), "Signal nur einmal");
        assert!(app.take_save().is_some(), "Spielzeit wird vorher gespeichert");
    }

    /// Palettenbild (Doom): vergrößert ins Zielrechteck, Zwischenspalten
    /// gemittelt, außerhalb bleibt der Hintergrund; in Streifen gezeichnet.
    #[test]
    fn palettenbild_wird_vergroessert() {
        let (w, h) = (64, 24);
        let mut pal = [0u16; 256];
        pal[1] = 0xF800;
        pal[2] = 0x07E0;
        pal[3] = 0x001F;
        pal[4] = 0xFFFF;
        // 4×2 Quelle, Ziel 6×4 bei (10, 4): 1,5-fach breit, 2-fach hoch.
        let px = [1u8, 2, 3, 4, 4, 3, 2, 1];
        let mut fd = FrameData::new();
        fd.flach(w, h, &|_| 7, &mut |_| {});
        fd.palettenbild(&px, 4, 2, &pal, (10, 4, 6, 4), 1);
        // Gleiches Bild nochmal: nichts neu zu rechnen, gleiches Ergebnis.
        fd.palettenbild(&px, 4, 2, &pal, (10, 4, 6, 4), 1);
        let (mut farbe, mut tiefe) = (vec![0u16; w * h], vec![0u16; w * h]);
        let mut s = Scratch::default();
        let mut rest = Target::new(&mut farbe, &mut tiefe, w, h);
        for y in (8..h).step_by(8).chain([h]) {
            let (mut band, tail) = rest.split(y);
            fd.render_band(&mut s, &mut band);
            rest = tail;
        }
        let at = |x: usize, y: usize| farbe[y * w + x];
        let mitte = |a: u16, b: u16| ((a & 0xF7DE) >> 1) + ((b & 0xF7DE) >> 1);
        assert_eq!(at(9, 4), 7, "links daneben: Hintergrund");
        assert_eq!(at(10, 4), 0xF800);
        assert_eq!(at(11, 4), mitte(0xF800, 0x07E0), "Zwischenspalte gemittelt");
        assert_eq!(at(12, 4), 0x07E0);
        assert_eq!(at(15, 4), 0xFFFF);
        assert_eq!(at(16, 4), 7, "rechts daneben: Hintergrund");
        assert_eq!(at(10, 7), 0xFFFF, "unterste Zeile = zweite Quellzeile");
        assert_eq!(at(10, 8), 7, "darunter: Hintergrund");
        // Ziel über mehrere Streifen bis zur letzten Zeile.
        fd.palettenbild(&px, 4, 2, &pal, (0, 0, w as i32, h as i32), 1);
        let mut rest = Target::new(&mut farbe, &mut tiefe, w, h);
        for y in (8..h).step_by(8).chain([h]) {
            let (mut band, tail) = rest.split(y);
            fd.render_band(&mut s, &mut band);
            rest = tail;
        }
        assert_eq!(farbe[(h - 1) * w], 0xFFFF, "letzte Zeile, links: Quelle (0, 1)");
        assert_eq!(farbe[(h - 1) * w + w - 1], 0xF800, "letzte Zeile, rechts: Quelle (3, 1)");
        // Nächstes Bild ohne Palettenbild: nur Hintergrund.
        fd.flach(w, h, &|_| 7, &mut |_| {});
        let mut t = Target::new(&mut farbe, &mut tiefe, w, h);
        fd.render_band(&mut s, &mut t);
        assert_eq!(farbe[4 * w + 10], 7);
    }

    /// Die Demo ist abgeschaltet: Ohne Eingabe bleibt das Titelbild stehen.
    #[test]
    fn title_stays_without_demo() {
        let mut app = App::new(None);
        frame(&mut app, 0, (2.0 * DEMO_AFTER * 30.0) as usize);
        assert_eq!(app.screen, Screen::Title);
    }

    /// Alles nur per Touch: Optionen, Untermenü Touch-Steuerung, Speichern.
    #[test]
    fn options_and_controls_by_touch_and_settings_persist() {
        let mut app = App::new(None);
        touch(&mut app, &[(160, 90)], 1); // Titel → Hauptmenü
        touch(&mut app, &[], 1);
        tap(&mut app, 160, 1); // OPTIONEN
        assert_eq!(app.screen, Screen::Options);
        let hires = app.settings().hires;
        tap(&mut app, 160, 3); // GRAFIK umschalten
        assert_eq!(app.settings().hires, !hires);
        assert_eq!(app.render_size(), if hires { (W, H) } else { (2 * W, 2 * H) });
        assert!(app.flash.is_some(), "angetippte Zeile leuchtet auf");
        assert!(app.settings().smooth, "Standard: weich (PPA, bilinear)");
        tap(&mut app, 160, 4); // VERGRÖSSERN: scharf
        assert!(!app.settings().smooth);
        let b = app.settings().brightness;
        tap(&mut app, 20, 5); // Helligkeit: linkes Drittel = dunkler
        assert_eq!(app.settings().brightness, b - 1);
        tap(&mut app, 300, 5);
        tap(&mut app, 300, 5);
        assert_eq!(app.settings().brightness, b + 1);
        tap(&mut app, 160, 6); // BILD DREHEN
        assert!(app.settings().flip);

        // Untermenü TOUCH-STEUERUNG.
        tap(&mut app, 160, 8);
        assert_eq!(app.screen, Screen::Controls);
        tap(&mut app, 300, 0); // GRÖSSE: groß
        assert_eq!(app.settings().touch_size, 2);
        tap(&mut app, 20, 1); // DECKKRAFT: weniger
        assert_eq!(app.settings().touch_alpha, 2);
        tap(&mut app, 160, 2); // JOYSTICK: fest
        assert!(app.settings().stick_fixed);
        tap(&mut app, 160, 3); // HAND: links
        assert!(app.settings().left_handed);
        tap(&mut app, 160, 4); // ZURÜCK → Optionen, Auswahl auf TOUCH-STEUERUNG
        assert_eq!(app.screen, Screen::Options);
        assert_eq!(app.cursor, 8);
        tap(&mut app, 160, 10); // ZURÜCK speichert
        assert_eq!(app.screen, Screen::Main);
        let saved = SaveData::from_bytes(&app.take_save().unwrap()).unwrap();
        assert_eq!(saved.settings, *app.settings());
    }

    /// Linkshänder: Knüppel rechts, Springen links – im Spiel.
    #[test]
    fn left_handed_controls_in_game() {
        let mut save = SaveData::default();
        save.settings.left_handed = true;
        let mut app = App::new(Some(save));
        app.touch_always = true;
        touch(&mut app, &[(160, 90)], 1);
        touch(&mut app, &[], 1);
        new_game_touch(&mut app);
        tap(&mut app, 160, 4);
        assert_eq!(app.screen, Screen::Playing);
        frame(&mut app, 0, 150);
        let y0 = app.game.player.body.pos.y;
        touch(&mut app, &[(60, 130)], 8); // links = Springen
        assert!(app.game.player.body.pos.y > y0 + 0.2, "Sprung links");
        touch(&mut app, &[], 40);
        let start = app.game.player.body.pos;
        touch(&mut app, &[(260, 130)], 1);
        touch(&mut app, &[(260, 100)], 15); // rechts = Knüppel
        assert!((app.game.player.body.pos - start).length() > 0.8, "Knüppel rechts");
    }

    /// Bei 55 Hz Anzeige und 120 Hz Physik rückt pro Bild abwechselnd 2 oder
    /// 3 Schritte weit vor. Dank Interpolation bewegt sich trotzdem alles
    /// gleichmäßig: Kamerafahrt im Menü und zeitgesteuerte Animationen
    /// (Wippen, Drehen, Wellen) rücken pro Bild gleich weit vor.
    #[test]
    fn motion_is_even_at_55_hz() {
        let mut app = App::new(None);
        let dt = 1.0 / 55.0;
        let (mut cam_steps, mut time_steps) = (vec![], vec![]);
        let mut last: Option<(Vec3, f32)> = None;
        for _ in 0..165 {
            app.update(Input::default(), dt);
            let alpha = (app.acc / SIM_DT).clamp(0.0, 1.0);
            let cam = app.camera(alpha).position;
            let t = app.game.world.render_time(alpha);
            if let Some((c, t0)) = last {
                cam_steps.push((cam - c).length());
                time_steps.push(t - t0);
            }
            last = Some((cam, t));
        }
        let spread = |v: &[f32]| {
            let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
            (hi - lo) / (0.5 * (hi + lo))
        };
        assert!(spread(&cam_steps[5..]) < 0.02, "Kamera ruckelt: {:?}", &cam_steps[5..12]);
        assert!(spread(&time_steps[5..]) < 0.02, "Animationen ruckeln: {:?}", &time_steps[5..12]);
        assert!((time_steps[50] - dt).abs() < 1e-4);
    }

    #[test]
    fn holding_jump_five_seconds_toggles_music() {
        let mut app = App::new(None);
        let mut cmds = vec![];
        app.drain_audio(|c| cmds.push(c));
        frame(&mut app, btn::JUMP, 30 * 4);
        app.drain_audio(|c| cmds.push(c));
        assert!(!cmds.contains(&AudioCmd::MusicEnabled(false)));
        frame(&mut app, btn::JUMP, 30 * 2);
        app.drain_audio(|c| cmds.push(c));
        assert!(cmds.contains(&AudioCmd::MusicEnabled(false)));
        assert!(!app.save.settings.music);
        // Weiter halten schaltet nicht erneut um.
        cmds.clear();
        frame(&mut app, btn::JUMP, 30 * 6);
        app.drain_audio(|c| cmds.push(c));
        assert!(!cmds.iter().any(|c| matches!(c, AudioCmd::MusicEnabled(_))));
    }

    /// Kampagne: Schwierigkeit wählen, Story-Karte, Kapitel für Kapitel
    /// automatisch weiter, jeder Kapitelstart wird gespeichert, am Ende Abspann.
    #[test]
    fn campaign_runs_through_all_chapters_with_story_and_autosave() {
        let mut app = App::new(None);
        press(&mut app, btn::JUMP); // Titel → Hauptmenü
        new_game_buttons(&mut app, 2); // SCHWER
        assert_eq!(app.difficulty, Difficulty::Hard);
        for chapter in 0..LEVEL_COUNT {
            assert_eq!(app.screen, Screen::Playing);
            assert_eq!(app.game_level, chapter);
            let p = SaveData::from_bytes(&app.take_save().unwrap()).unwrap().progress.unwrap();
            assert_eq!((p.level as usize, p.difficulty), (chapter, 2), "Kapitelstart wird gespeichert");
            // Ziel erreicht.
            app.game.finish_time = Some(30.0);
            app.game.phase = Phase::Finished { t: 2.7 };
            frame(&mut app, 0, 1);
            assert_eq!(app.screen, Screen::Results);
            let saved = SaveData::from_bytes(&app.take_save().unwrap()).unwrap();
            if chapter + 1 < LEVEL_COUNT {
                assert_eq!(saved.progress.map(|p| p.level as usize), Some(chapter + 1), "weiter beim nächsten Kapitel");
            } else {
                assert_eq!(saved.progress, None);
            }
            press(&mut app, btn::JUMP); // WEITER
            assert_eq!(app.screen, Screen::Story);
            assert_eq!(app.story, chapter + 1);
            frame(&mut app, 0, 30);
            press(&mut app, btn::JUMP);
        }
        assert_eq!(app.screen, Screen::Title, "nach dem Abspann zurück zum Titel");
    }

    /// Schwer: 3 Leben pro Kapitel, weniger Checkpoints, schnelleres Schmelzen.
    #[test]
    fn hard_mode_has_lives_and_fewer_checkpoints() {
        let mut app = App::new(None);
        press(&mut app, btn::JUMP);
        new_game_buttons(&mut app, 2);
        assert!(app.game.world.checkpoints.iter().any(|c| !c.enabled));
        assert!(app.game.world.checkpoints.last().is_some_and(|c| c.enabled));
        assert_eq!(app.game.melt_time(), 40.0);
        app.game.player.deaths = 3;
        frame(&mut app, 0, 1);
        assert_eq!(app.screen, Screen::Playing);
        assert_eq!(app.game.player.deaths, 0, "Kapitel beginnt von vorn");

        // Leicht: unbegrenzt viele Versuche, alle Checkpoints.
        let mut easy = App::new(None);
        press(&mut easy, btn::JUMP);
        new_game_buttons(&mut easy, 0);
        assert!(easy.game.world.checkpoints.iter().all(|c| c.enabled));
        easy.game.player.deaths = 9;
        frame(&mut easy, 0, 1);
        assert_eq!(easy.game.player.deaths, 9);
    }

    /// Echte Spielszenen aller Kapitel: die einmal pro Frame aufbereiteten
    /// Dreiecke ergeben bitgenau dasselbe Bild wie der bisherige Weg (jedes
    /// Objekt pro Streifen), in Streifen von 16 Zeilen bei 640×360.
    #[test]
    fn prepared_triangles_match_direct_drawing_in_every_chapter() {
        let (w, h) = (W * 2, H * 2);
        let n = w * h;
        for chapter in 0..LEVEL_COUNT {
            let mut app = App::new(None);
            app.start_chapter(chapter, Difficulty::Easy);
            app.set_autopilot(true);
            for k in 0..6 {
                frame(&mut app, 0, 40 + k * 25);
                app.begin_frame(w, h);
                let (mut c1, mut d1, mut c2, mut d2) = (vec![0u16; n], vec![0u16; n], vec![0u16; n], vec![0u16; n]);
                let draw = |c: &mut [u16], d: &mut [u16], direct: bool, app: &App| {
                    let mut s = Scratch::default();
                    let mut rest = Target::new(c, d, w, h);
                    for y in (16..h).step_by(16).chain([h]) {
                        let (mut band, tail) = rest.split(y);
                        app.frame.renderer.clear(&mut band);
                        if direct {
                            app.frame.renderer.draw_direct(&mut s, &mut band, &app.game.store, &app.game.list);
                        } else {
                            app.frame.renderer.draw(&mut s, &mut band, &app.game.store, &app.game.list);
                        }
                        rest = tail;
                    }
                };
                draw(&mut c1, &mut d1, true, &app);
                draw(&mut c2, &mut d2, false, &app);
                assert!(c1 == c2 && d1 == d2, "Kapitel {}, Bild {k}: Abweichung", chapter + 1);
            }
        }
    }

    /// Das einmal aufgezeichnete HUD/Menü ergibt in 16-Zeilen-Streifen
    /// bitgenau dasselbe Bild wie das direkte Zeichnen in jedem Streifen.
    #[test]
    fn recorded_overlay_matches_direct_drawing() {
        let (w, h) = (W * 2, H * 2);
        let mut app = App::new(None);
        app.start_chapter(0, Difficulty::Hard);
        app.touch_always = true;
        app.save.settings.show_fps = true;
        frame(&mut app, 0, 40);
        let mut hoisted = 0;
        for screen in [
            Screen::Playing,
            Screen::Paused,
            Screen::Demo,
            Screen::Title,
            Screen::Main,
            Screen::Options,
            Screen::Controls,
            Screen::Records,
            Screen::ConfirmErase,
            Screen::Difficulty,
            Screen::Story,
        ] {
            app.screen = screen;
            app.begin_frame(w, h);
            assert!(!app.frame.overlay.is_empty(), "{screen:?}");
            hoisted += app.frame.renderer.tint().is_some() as u32;
            let mut a: Vec<u16> = (0..w * h).map(|i| (i as u32).wrapping_mul(2_654_435_761) as u16).collect();
            // Herausgezogene Abtönung: Der Renderer tönt die Szene, hier den
            // Hintergrund.
            let tint = app.frame.renderer.tint();
            let mut b: Vec<u16> = match tint {
                Some((c, t)) => a.iter().map(|&p| crate::gfx::mix(p, c, t)).collect(),
                None => a.clone(),
            };
            for y0 in (0..h).step_by(16) {
                let y1 = (y0 + 16).min(h);
                let mut c = Canvas::scaled(&mut a[y0 * w..y1 * w], w, h, 2, y0);
                app.draw_overlay(&mut c);
                hud::perf_overlay(&mut c, &app.perf);
                let mut c = Canvas::scaled(&mut b[y0 * w..y1 * w], w, h, 2, y0);
                app.frame.overlay.replay(&mut c);
            }
            if tint.is_none() {
                assert!(a == b, "{screen:?}: Abweichung");
            } else {
                // Gleich bis auf Rundung (je Kanal höchstens 2 Stufen).
                let ch = |p: u16| [(p >> 11) as i32, ((p >> 5) & 63) as i32, (p & 31) as i32];
                for (i, (&p, &q)) in a.iter().zip(&b).enumerate() {
                    let (x, y) = (ch(p), ch(q));
                    assert!(
                        (0..3).all(|k| (x[k] - y[k]).abs() <= 2),
                        "{screen:?}: Pixel {i} {p:04X} vs {q:04X}"
                    );
                }
            }
        }
        assert!(hoisted >= 5, "Menüs und Pause tönen über den Renderer ({hoisted})");
    }

    /// Ein weggetauschtes Bild zeichnet bitgleich, auch wenn das Spiel
    /// danach weiterläuft und schon das nächste Bild vorbereitet.
    #[test]
    fn swapped_frame_renders_identically_while_game_continues() {
        let (w, h) = (W * 2, H * 2);
        let n = w * h;
        let mut app = App::new(None);
        app.start_chapter(2, Difficulty::Easy);
        app.set_autopilot(true);
        app.touch_always = true;
        frame(&mut app, 0, 60);
        let mut fd = FrameData::new();
        for _ in 0..4 {
            app.begin_frame(w, h);
            let (mut c1, mut d1) = (vec![0u16; n], vec![0u16; n]);
            let mut s = Scratch::default();
            app.render_band(&mut s, &mut Target::new(&mut c1, &mut d1, w, h));
            app.swap_frame(&mut fd);
            // Weiterspielen und das nächste Bild vorbereiten.
            frame(&mut app, 0, 3);
            app.begin_frame(w, h);
            let (mut c2, mut d2) = (vec![0u16; n], vec![0u16; n]);
            let mut rest = Target::new(&mut c2, &mut d2, w, h);
            for y in (8..h).step_by(8).chain([h]) {
                let (mut band, tail) = rest.split(y);
                fd.render_band(&mut s, &mut band);
                rest = tail;
            }
            assert!(c1 == c2 && d1 == d2, "Abweichung nach dem Tausch");
            assert!(fd.prims() > 50);
        }
    }

    #[test]
    fn renders_every_screen_in_both_resolutions() {
        let mut app = App::new(None);
        for s in [1, 2] {
            let (pw, ph) = (W * s, H * s);
            let (mut c, mut d) = (vec![0u16; pw * ph], vec![0u16; pw * ph]);
            for screen in [
                Screen::Title,
                Screen::Main,
                Screen::Options,
                Screen::Records,
                Screen::ConfirmErase,
                Screen::Difficulty,
                Screen::Story,
            ] {
                app.screen = screen;
                app.render_full(&mut Target::new(&mut c, &mut d, pw, ph));
            }
            assert!(app.tris_drawn() > 50);
        }
    }
}
