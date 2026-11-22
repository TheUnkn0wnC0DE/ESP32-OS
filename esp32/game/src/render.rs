//! 3D-Renderer für den Mikrocontroller: RGB565-Farbe, 16-Bit-Tiefe,
//! Zeilenstreifen.
//!
//! Gleicher Aufbau wie der PC-Renderer der Engine (Top-Left-Füllregel,
//! 16.16-Fixpunkt-Kanten, Tiefe = near/w, Near- und Guard-Band-Clipping),
//! aber auf den Mikrocontroller zugeschnitten:
//!
//! * 2 Byte Farbe + 2 Byte Tiefe pro Pixel.
//! * **Zwei Kerne:** Das Bild wird in Zeilenstreifen geteilt ([`Target::split`]).
//!   Der [`Renderer`] hält nur, was für den ganzen Frame gilt (Kamera,
//!   sortierte Zeichenliste) und wird beim Zeichnen nur gelesen; jeder Kern
//!   bringt seinen eigenen Arbeitsspeicher ([`Scratch`]) mit. Pro Objekt wird
//!   beim Sortieren die Bildschirmhöhe abgeschätzt, damit jeder Streifen nur
//!   die Objekte zeichnet, die ihn berühren.
//! * **Nebel pro Dreieck** statt pro Pixel (kein eigener Nachbearbeitungs-
//!   durchlauf), der Himmel wird beim Löschen gleich mit gezeichnet.

use crate::gfx;
use alloc::vec::Vec;
use spatz::color::Color;
use spatz::math::{Frustum, Mat4, Vec3, Vec4, tan};
use spatz::render::clip::{ClipPlane, ClipVertex, MAX_CLIP_VERTS, clip_polygon};
use spatz::render::{Camera, DrawList, DrawMode, DrawParams, Environment, Mesh, MeshStore, Sprite};

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering::Relaxed};

/// Größte Zahl aufbereiteter Dreiecke, für die vorab Platz geschaffen wird
/// (eine große Zuteilung – landet auf dem ESP32 im PSRAM statt den knappen
/// internen RAM schrittweise zu füllen).
pub const MAX_PRIMS: usize = 4096;

/// Optionale Taktuhr für Messungen auf dem Gerät ([`set_profile_clock`]).
static PROFILE_CLOCK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
/// Takte in [`Renderer::build`]: Vertices transformieren, Dreiecke
/// (Beleuchten, Clippen, Einrichten); dazu bearbeitete Dreiecke.
/// [3] Takte im Beleuchten, [4] im Einrichten + Ablegen, [5] ausgegebene
/// Dreiecke; [6] Takte für geclippte Dreiecke, [7] deren Anzahl, [8] Objekte
/// (Messung).
pub static BUILD_PROFILE: [AtomicU32; 9] = [const { AtomicU32::new(0) }; 9];
/// Takte beim Zeichnen der Streifen (beide Kerne zusammen): Löschen,
/// Dreiecke, HUD/Menü; dazu gefüllte Zeilenstücke und Pixel.
pub static RENDER_PROFILE: [AtomicU32; 5] = [const { AtomicU32::new(0) }; 5];

/// Zeitstempel der Profil-Uhr (0 ohne Uhr).
#[inline(always)]
pub fn profile_clock() -> u32 {
    clock()
}

/// Setzt die Taktuhr für [`BUILD_PROFILE`] (z. B. den Zyklenzähler der CPU).
pub fn set_profile_clock(f: fn() -> u32) {
    PROFILE_CLOCK.store(f as *mut (), Relaxed);
}

#[inline(always)]
fn clock() -> u32 {
    let p = PROFILE_CLOCK.load(Relaxed);
    if p.is_null() {
        return 0;
    }
    // SAFETY: nur über `set_profile_clock` mit einem `fn() -> u32` gesetzt.
    let f: fn() -> u32 = unsafe { core::mem::transmute(p) };
    f()
}

/// Tiefe `near/w ∈ (0, 1]` als 8.8-Fixpunkt eines `u16` (intern i32).
const DEPTH_ONE: f32 = 65535.0 * 256.0;
/// Guard-Band: Faktor auf die halbe Bildgröße, ab dem geclippt wird.
const GUARD: f32 = 3.0;

const F_NEAR: u8 = 1;
const F_LEFT: u8 = 2;
const F_RIGHT: u8 = 4;
const F_TOP: u8 = 8;
const F_BOTTOM: u8 = 16;
const F_GUARD: u8 = 32;
const REJECT: u8 = F_NEAR | F_LEFT | F_RIGHT | F_TOP | F_BOTTOM;
const CLIP: u8 = F_NEAR | F_GUARD;

/// Ziel eines Frames (oder ein Zeilenstreifen davon): Farbe und Tiefe.
///
/// `color`/`depth` enthalten die Zeilen ab `row0` eines Bildes der Größe
/// `w`×`h`.
pub struct Target<'a> {
    pub color: &'a mut [u16],
    pub depth: &'a mut [u16],
    pub w: usize,
    pub h: usize,
    pub row0: usize,
}

impl<'a> Target<'a> {
    /// Ganzes Bild.
    pub fn new(color: &'a mut [u16], depth: &'a mut [u16], w: usize, h: usize) -> Self {
        assert!(color.len() >= w * h && depth.len() >= w * h, "Puffer zu klein");
        Self { color: &mut color[..w * h], depth: &mut depth[..w * h], w, h, row0: 0 }
    }

    /// Enthaltene Zeilen `[y0, y1)`.
    pub fn rows(&self) -> (usize, usize) {
        (self.row0, (self.row0 + self.color.len() / self.w.max(1)).min(self.h))
    }

    /// Teilt bei Zeile `y` in zwei Streifen (z. B. einen je Kern).
    pub fn split(self, y: usize) -> (Target<'a>, Target<'a>) {
        let (y0, y1) = self.rows();
        let y = y.clamp(y0, y1);
        let k = (y - y0) * self.w;
        let (c0, c1) = self.color.split_at_mut(k);
        let (d0, d1) = self.depth.split_at_mut(k);
        let (w, h) = (self.w, self.h);
        (Target { color: c0, depth: d0, w, h, row0: y0 }, Target { color: c1, depth: d1, w, h, row0: y })
    }
}

/// Arbeitsspeicher eines zeichnenden Kerns (projizierte Vertices, Zähler).
#[derive(Default)]
pub struct Scratch {
    verts: Vec<Vert>,
    band: (i32, i32),
    frame: u32,
    pub stats: Stats,
}

impl Scratch {
    /// Mit Platz für das größte Mesh.
    pub fn with_capacity(max_vertices: usize) -> Self {
        Self { verts: Vec::with_capacity(max_vertices), ..Self::default() }
    }
}

/// Zähler pro Frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub objects: u32,
    pub objects_drawn: u32,
    pub tris_drawn: u32,
    /// Gefüllte Zeilenstücke und Pixel (inkl. verdeckter).
    pub rows: u32,
    pub px: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Vert {
    x: f32,
    y: f32,
    w: f32,
    sx: f32,
    sy: f32,
    z: f32,
    f: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct Sv {
    x: f32,
    y: f32,
    z: f32,
}

/// Vorsortierter Eintrag der Zeichenliste.
#[derive(Clone, Copy, Debug)]
struct Entry {
    depth: f32,
    item: u32,
    y0: i16,
    y1: i16,
}

pub struct Renderer {
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    focal: f32,
    near: f32,
    far: f32,
    rx: Vec4,
    ry: Vec4,
    rw: Vec4,
    cam_pos: Vec3,
    cam_fwd: Vec3,
    frustum: Frustum,
    env: Environment,
    fog_on: bool,
    fog_color: Color,
    /// 1 / (Nebelende − Nebelanfang).
    fog_inv: f32,
    sky: Vec<u16>,
    order: Vec<Entry>,
    frame: u32,
    /// Objekte in der Zeichenliste (nach dem Sortieren).
    pub objects: u32,
    /// Vorbereitete Dreiecke des Frames ([`Renderer::build`]) in
    /// Zeichenreihenfolge; `pass_start` = Beginn von Opak, Schatten,
    /// Durchscheinend und Ende.
    prims: Vec<Prim>,
    /// Zeilen `[y0, y1)` je Dreieck (kompakt, für den Streifentest).
    prim_rows: Vec<(i16, i16)>,
    /// Je Band aus [`BIN_ROWS`] Zeilen die Dreiecke, die es berühren (in
    /// Zeichenreihenfolge): `bin_idx[bin_start[b]..bin_start[b + 1]]`. Ein
    /// Streifen innerhalb eines Bandes prüft nur diese statt aller.
    bin_start: Vec<u32>,
    bin_fill: Vec<u32>,
    bin_idx: Vec<u16>,
    pass_start: [usize; 4],
    build_verts: Vec<Vert>,
    /// Abtönung des ganzen Bildes (Farbe, Anteil 0..=32), die sonst als
    /// Rechteck über das fertige Bild gelegt würde (Menüs, Pause, Tod):
    /// Dreiecke, Himmel und Partikel bekommen sie gleich beim Aufbereiten
    /// (siehe [`Renderer::set_tint`]).
    tint: Option<(u16, u32)>,
    /// Partikel des Frames (Kopie aus der Zeichenliste beim Aufbereiten –
    /// gezeichnet wird, während das Spiel schon weiterläuft).
    sprites: Vec<Sprite>,
    /// Dieselben Partikel einmal je Bild projiziert (Bildschirmrechteck,
    /// Tiefe, Farbe) – die Streifen prüfen nur noch die Zeilen.
    sprite_prims: Vec<SpritePrim>,
    /// Frame, für den `prims` gilt.
    built: u32,
}

/// Partikel im Bild: Rechteck [x0, x1) × [y0, y1), Tiefe, fertige Farbe.
#[derive(Clone, Copy, Debug)]
struct SpritePrim {
    x0: i16,
    x1: i16,
    y0: i16,
    y1: i16,
    z: u16,
    c: u16,
}

/// Fortlaufende Frame-Nummer über alle Renderer (die Firmware wechselt
/// zwischen zweien ab; `Scratch` erkennt so jeden neuen Frame).
static FRAME_SEQ: AtomicU32 = AtomicU32::new(1);

impl Renderer {
    /// Platz für `max_items` Zeichenaufrufe vorab.
    pub fn new(max_items: usize) -> Self {
        Self {
            w: gfx::W,
            h: gfx::H,
            cx: gfx::W as f32 * 0.5,
            cy: gfx::H as f32 * 0.5,
            focal: 1.0,
            near: 0.1,
            far: 100.0,
            rx: Vec4::default(),
            ry: Vec4::default(),
            rw: Vec4::default(),
            cam_pos: Vec3::ZERO,
            cam_fwd: Vec3::new(0.0, 0.0, -1.0),
            frustum: Frustum::default(),
            env: Environment::default(),
            fog_on: false,
            fog_color: Color::hex(0xFFD8EC),
            fog_inv: 0.0,
            sky: Vec::with_capacity(gfx::MAX_H),
            order: Vec::with_capacity(max_items),
            frame: 0,
            objects: 0,
            prims: Vec::with_capacity(MAX_PRIMS),
            prim_rows: Vec::with_capacity(MAX_PRIMS),
            bin_start: Vec::with_capacity(64),
            bin_fill: Vec::with_capacity(64),
            bin_idx: Vec::with_capacity(4 * MAX_PRIMS),
            pass_start: [0; 4],
            build_verts: Vec::with_capacity(crate::app::MAX_MESH_VERTICES),
            sprites: Vec::with_capacity(1024),
            sprite_prims: Vec::with_capacity(1024),
            tint: None,
            built: u32::MAX,
        }
    }

    /// Kamera und Umgebung für den Frame setzen.
    pub fn begin(&mut self, cam: &Camera, env: &Environment, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.cx = w as f32 * 0.5;
        self.cy = h as f32 * 0.5;
        let tan_y = tan(cam.fov_y * 0.5);
        let tan_x = tan_y * w as f32 / h as f32;
        self.focal = self.cy / tan_y;
        self.near = cam.near;
        self.fog_on = env.fog && env.fog_end > env.fog_start;
        self.far = if self.fog_on { cam.far.min(env.fog_end) } else { cam.far };
        let (r, u, f) = cam.basis();
        let p = cam.position;
        self.rx = Vec4::new(r.x, r.y, r.z, -r.dot(p)) * self.focal;
        self.ry = Vec4::new(u.x, u.y, u.z, -u.dot(p)) * -self.focal;
        self.rw = Vec4::new(f.x, f.y, f.z, -f.dot(p));
        self.cam_pos = p;
        self.cam_fwd = f;
        self.frustum = Frustum::new(p, r, u, f, tan_x, tan_y, cam.near, self.far);
        self.env = *env;
        self.fog_color = env.sky_color(0.02);
        self.fog_inv = if self.fog_on { 1.0 / (env.fog_end - env.fog_start) } else { 0.0 };
        self.sky.clear();
        for y in 0..h {
            let d = f * self.focal + u * (self.cy - (y as f32 + 0.5));
            self.sky.push(gfx::from_color(env.sky_color(d.y / d.length())));
        }
        self.frame = FRAME_SEQ.fetch_add(1, Relaxed);
        self.objects = 0;
        self.tint = None;
    }

    /// Bildgröße des aktuellen Frames.
    pub fn size(&self) -> (usize, usize) {
        (self.w, self.h)
    }

    /// Flaches Bild ohne 3D-Szene (2D-Spiele): nur der Hintergrund
    /// `himmel(y)` je Zeile; Dreiecke und Partikel leer, gilt als aufbereitet.
    pub fn begin_flat(&mut self, w: usize, h: usize, himmel: &dyn Fn(usize) -> u16) {
        self.w = w;
        self.h = h;
        self.sky.clear();
        self.sky.extend((0..h).map(himmel));
        self.frame = FRAME_SEQ.fetch_add(1, Relaxed);
        self.objects = 0;
        self.tint = None;
        self.order.clear();
        self.prims.clear();
        self.prim_rows.clear();
        self.sprites.clear();
        self.pass_start = [0; 4];
        self.fill_bins();
        self.built = self.frame;
    }

    /// Tönt das ganze Bild dieses Frames: `mix(·, c, t)` auf jede Farbe von
    /// Dreiecken, Himmel und Partikeln statt auf jedes Pixel; Schatten mischen
    /// dann mit dem getönten Schwarz. Nach [`Renderer::begin`], vor
    /// [`Renderer::build`].
    pub fn set_tint(&mut self, tint: Option<(u16, u32)>) {
        self.tint = tint;
        if let Some((c, t)) = tint {
            for s in &mut self.sky {
                *s = gfx::mix(*s, c, t);
            }
        }
    }

    /// Abtönung dieses Frames (Tests).
    pub fn tint(&self) -> Option<(u16, u32)> {
        self.tint
    }

    #[inline]
    fn tinted(&self, c: u16) -> u16 {
        match self.tint {
            Some((tc, t)) => gfx::mix(c, tc, t),
            None => c,
        }
    }

    /// Zahl der mit [`Renderer::build`] aufbereiteten Dreiecke.
    pub fn prim_count(&self) -> usize {
        self.prims.len()
    }

    /// Projiziert einen Weltpunkt in Pixel (`None` hinter der Kamera).
    pub fn project(&self, p: Vec3) -> Option<(f32, f32, f32)> {
        let v = Vec4::from_vec3(p, 1.0);
        let w = self.rw.dot(v);
        if w < self.near {
            return None;
        }
        Some((self.cx + self.rx.dot(v) / w, self.cy + self.ry.dot(v) / w, w))
    }

    /// Himmel in die Zeilen des Streifens und Tiefe dort löschen.
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    #[inline(never)]
    pub fn clear(&self, t: &mut Target) {
        let (y0, y1) = t.rows();
        let n = (y1 - y0) * t.w;
        // Tiefe: alles auf einmal, 32 Bit je Schreibzugriff.
        fill_u16(&mut t.depth[..n], 0);
        for y in y0..y1 {
            let o = (y - y0) * t.w;
            fill_u16(&mut t.color[o..o + t.w], self.sky[y]);
        }
    }

    /// Sortiert die Zeichenliste einmal pro Frame (opak vorne→hinten,
    /// Durchscheinendes hinten→vorne), verwirft Unsichtbares und schätzt die
    /// Bildzeilen jedes Objekts ab.
    pub fn sort(&mut self, store: &MeshStore, list: &DrawList) {
        self.order.clear();
        for (i, it) in list.items.iter().enumerate() {
            self.objects += 1;
            let mesh = store.get(it.mesh);
            let bs = mesh.bounds.transformed(&it.model);
            let depth = (bs.center - self.cam_pos).dot(self.cam_fwd);
            if depth > it.params.cull_distance || depth < it.params.cull_near {
                continue;
            }
            if !self.frustum.sphere_visible(&bs) {
                continue;
            }
            // Bildzeilen grob (konservativ) aus der Hüllkugel.
            let v = Vec4::from_vec3(bs.center, 1.0);
            let (w, y) = (self.rw.dot(v), self.ry.dot(v));
            let (y0, y1) = if w - bs.radius <= self.near {
                (0, self.h as i16)
            } else {
                let yc = self.cy + y / w;
                let e = bs.radius * self.focal / (w - bs.radius) + 2.0;
                (((yc - e) as i32).clamp(0, self.h as i32) as i16, ((yc + e) as i32 + 1).clamp(0, self.h as i32) as i16)
            };
            self.order.push(Entry { depth: depth + it.params.sort_bias, item: i as u32, y0, y1 });
        }
        self.order.sort_unstable_by(|a, b| a.depth.total_cmp(&b.depth));
    }

    /// Bereitet alle Dreiecke des Frames einmal auf (transformieren, clippen,
    /// beleuchten, Kanten einrichten) – nach [`Renderer::sort`], vor dem
    /// Zeichnen der Streifen. Danach zeichnet [`Renderer::draw`] je Streifen
    /// nur noch die Pixelzeilen der Dreiecke, die ihn berühren (statt jedes
    /// Objekt pro Streifen neu durchzurechnen). Ergebnis bitgleich.
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    #[inline(never)]
    pub fn build(&mut self, store: &MeshStore, list: &DrawList) {
        self.sprites.clear();
        self.sprites.extend_from_slice(&list.sprites);
        self.sprite_prims.clear();
        for i in 0..self.sprites.len() {
            let sp = self.sprites[i];
            if let Some(p) = self.sprite_prim(sp.pos, sp.size, self.tinted(gfx::from_color(sp.color))) {
                self.sprite_prims.push(p);
            }
        }
        let mut prims = core::mem::take(&mut self.prims);
        let mut verts = core::mem::take(&mut self.build_verts);
        prims.clear();
        let (w, h) = (self.w as i32, self.h as i32);
        let n = self.order.len();
        for (pass, mode) in [DrawMode::Opaque, DrawMode::Shadow, DrawMode::Blend50].into_iter().enumerate() {
            self.pass_start[pass] = prims.len();
            for j in 0..n {
                let e = self.order[if pass == 2 { n - 1 - j } else { j }];
                let it = &list.items[e.item as usize];
                if it.params.mode == mode {
                    self.emit_mesh(&mut verts, store.get(it.mesh), &it.model, &it.params, &mut |a, b, c, m, col| {
                        if let Some(p) = Prim::setup(w, h, a, b, c, m, col) {
                            prims.push(p);
                        }
                    });
                }
            }
        }
        self.pass_start[3] = prims.len();
        self.prim_rows.clear();
        self.prim_rows.extend(prims.iter().map(|p| (p.y0, p.y1)));
        self.fill_bins();
        self.prims = prims;
        self.build_verts = verts;
        self.built = self.frame;
    }

    /// Zeichnet das sortierte Bild in die Zeilen des Streifens `t`. Darf von
    /// mehreren Kernen gleichzeitig aufgerufen werden (je eigener Streifen und
    /// eigenes `s`). Nutzt die mit [`Renderer::build`] vorbereiteten Dreiecke;
    /// ohne sie wird jedes Objekt direkt in den Streifen gezeichnet.
    pub fn draw(&self, s: &mut Scratch, t: &mut Target, store: &MeshStore, list: &DrawList) {
        if s.frame != self.frame {
            s.frame = self.frame;
            s.stats = Stats::default();
        }
        if self.built != self.frame {
            return self.draw_direct(s, t, store, list);
        }
        self.draw_built(s, t);
    }

    /// Ist der aktuelle Frame mit [`Renderer::build`] aufbereitet?
    pub fn is_built(&self) -> bool {
        self.built == self.frame
    }

    /// Zeichnet die aufbereiteten Dreiecke und Partikel in den Streifen `t`
    /// (braucht weder Meshes noch Zeichenliste; nichts, wenn nicht
    /// aufbereitet).
    /// Ordnet die Dreiecke den Bändern zu (Zählen, Aufsummieren, Einsortieren
    /// – die Reihenfolge je Band bleibt die Zeichenreihenfolge).
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    #[inline(never)]
    fn fill_bins(&mut self) {
        let bins = self.h.div_ceil(BIN_ROWS).max(1);
        let span = |&(y0, y1): &(i16, i16)| (y0 as usize / BIN_ROWS, (y1 as usize).saturating_sub(1) / BIN_ROWS);
        self.bin_start.clear();
        self.bin_start.resize(bins + 1, 0);
        for r in &self.prim_rows {
            let (b0, b1) = span(r);
            for b in b0..=b1.min(bins - 1) {
                self.bin_start[b + 1] += 1;
            }
        }
        for b in 0..bins {
            self.bin_start[b + 1] += self.bin_start[b];
        }
        self.bin_fill.clear();
        self.bin_fill.extend_from_slice(&self.bin_start[..bins]);
        self.bin_idx.clear();
        self.bin_idx.resize(self.bin_start[bins] as usize, 0);
        for (i, r) in self.prim_rows.iter().enumerate() {
            let (b0, b1) = span(r);
            for b in b0..=b1.min(bins - 1) {
                self.bin_idx[self.bin_fill[b] as usize] = i as u16;
                self.bin_fill[b] += 1;
            }
        }
    }

    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    #[inline(never)]
    pub fn draw_built(&self, s: &mut Scratch, t: &mut Target) {
        if s.frame != self.frame {
            s.frame = self.frame;
            s.stats = Stats::default();
        }
        if !self.is_built() {
            return;
        }
        let (y0, y1) = t.rows();
        s.band = (y0 as i32, y1 as i32);
        let band = s.band;
        let (rows0, px0) = (s.stats.rows, s.stats.px);
        let _count = Counted(&s.stats.rows as *const u32, rows0, &s.stats.px as *const u32, px0);
        // Liegt der Streifen in einem Band: nur dessen Dreiecke (gleiche
        // Reihenfolge, Partikel vor dem ersten Schatten-Dreieck wie unten).
        let (b0, b1) = (y0 / BIN_ROWS, y1.saturating_sub(1) / BIN_ROWS);
        if b0 == b1 && b0 + 1 < self.bin_start.len() {
            let list = &self.bin_idx[self.bin_start[b0] as usize..self.bin_start[b0 + 1] as usize];
            let mut sprites = false;
            for &i in list {
                let i = i as usize;
                if !sprites && i >= self.pass_start[1] {
                    sprites = true;
                    self.draw_sprite_prims(t, band);
                }
                let (py0, py1) = self.prim_rows[i];
                if (py1 as i32) <= band.0 || (py0 as i32) >= band.1 {
                    continue;
                }
                if self.prims[i].raster(t, band, &mut s.stats) {
                    s.stats.tris_drawn += 1;
                }
            }
            if !sprites {
                self.draw_sprite_prims(t, band);
            }
            return;
        }
        for pass in 0..3 {
            if pass == 1 {
                self.draw_sprite_prims(t, band);
            }
            let range = self.pass_start[pass]..self.pass_start[pass + 1];
            for (i, &(py0, py1)) in self.prim_rows[range.clone()].iter().enumerate() {
                if (py1 as i32) <= band.0 || (py0 as i32) >= band.1 {
                    continue;
                }
                if self.prims[range.start + i].raster(t, band, &mut s.stats) {
                    s.stats.tris_drawn += 1;
                }
            }
        }
    }

    /// Bisheriger Weg: jedes Objekt pro Streifen transformieren und zeichnen
    /// (Referenz für Tests, Rückfall ohne [`Renderer::build`]).
    pub fn draw_direct(&self, s: &mut Scratch, t: &mut Target, store: &MeshStore, list: &DrawList) {
        if s.frame != self.frame {
            s.frame = self.frame;
            s.stats = Stats::default();
        }
        let (y0, y1) = t.rows();
        s.band = (y0 as i32, y1 as i32);
        let n = self.order.len();
        for (pass, mode) in [DrawMode::Opaque, DrawMode::Shadow, DrawMode::Blend50].into_iter().enumerate() {
            if pass == 1 {
                for sp in &list.sprites {
                    self.draw_sprite(s, t, sp.pos, sp.size, self.tinted(gfx::from_color(sp.color)));
                }
            }
            for j in 0..n {
                let e = self.order[if pass == 2 { n - 1 - j } else { j }];
                if (e.y1 as i32) <= s.band.0 || (e.y0 as i32) >= s.band.1 {
                    continue;
                }
                let it = &list.items[e.item as usize];
                if it.params.mode == mode {
                    self.draw_mesh(s, t, store.get(it.mesh), &it.model, &it.params);
                }
            }
        }
    }

    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    /// Partikel einmal projizieren (gleiche Rundung wie [`Renderer::draw_sprite`]).
    fn sprite_prim(&self, pos: Vec3, size: f32, c: u16) -> Option<SpritePrim> {
        let v = Vec4::from_vec3(pos, 1.0);
        let w = self.rw.dot(v);
        if w < self.near || w > self.far {
            return None;
        }
        let iw = 1.0 / w;
        let sx = self.cx + self.rx.dot(v) * iw;
        let sy = self.cy + self.ry.dot(v) * iw;
        let r = (size * 0.5 * self.focal * iw).max(0.5);
        let z = ((self.near * iw * DEPTH_ONE) as i32 >> 8) as u16;
        let x0 = ceil_i32(sx - r - 0.5).max(0);
        let x1 = ceil_i32(sx + r - 0.5).min(self.w as i32);
        let y0 = ceil_i32(sy - r - 0.5).max(0);
        let y1 = ceil_i32(sy + r - 0.5).min(self.h as i32);
        (x0 < x1 && y0 < y1).then_some(SpritePrim { x0: x0 as i16, x1: x1 as i16, y0: y0 as i16, y1: y1 as i16, z, c })
    }

    /// Vorab projizierte Partikel in den Streifen zeichnen (nur die, die ihn berühren).
    fn draw_sprite_prims(&self, t: &mut Target, band: (i32, i32)) {
        for p in &self.sprite_prims {
            let (y0, y1) = ((p.y0 as i32).max(band.0), (p.y1 as i32).min(band.1));
            if y0 >= y1 {
                continue;
            }
            for y in y0..y1 {
                let o = (y as usize - t.row0) * t.w;
                let (a, b) = (o + p.x0 as usize, o + p.x1 as usize);
                for (d, c) in t.depth[a..b].iter_mut().zip(&mut t.color[a..b]) {
                    if p.z > *d {
                        *d = p.z;
                        *c = p.c;
                    }
                }
            }
        }
    }

    fn draw_sprite(&self, s: &Scratch, t: &mut Target, pos: Vec3, size: f32, c: u16) {
        let v = Vec4::from_vec3(pos, 1.0);
        let w = self.rw.dot(v);
        if w < self.near || w > self.far {
            return;
        }
        let iw = 1.0 / w;
        let sx = self.cx + self.rx.dot(v) * iw;
        let sy = self.cy + self.ry.dot(v) * iw;
        let r = (size * 0.5 * self.focal * iw).max(0.5);
        let z = ((self.near * iw * DEPTH_ONE) as i32 >> 8) as u16;
        let x0 = ceil_i32(sx - r - 0.5).max(0);
        let x1 = ceil_i32(sx + r - 0.5).min(t.w as i32);
        let y0 = ceil_i32(sy - r - 0.5).max(s.band.0);
        let y1 = ceil_i32(sy + r - 0.5).min(s.band.1);
        for y in y0..y1 {
            let o = (y as usize - t.row0) * t.w;
            for x in x0..x1 {
                let i = o + x as usize;
                if z > t.depth[i] {
                    t.depth[i] = z;
                    t.color[i] = c;
                }
            }
        }
    }

    /// Zeichnet ein Mesh (falls sichtbar) direkt in den Streifen.
    pub fn draw_mesh(&self, s: &mut Scratch, t: &mut Target, mesh: &Mesh, model: &Mat4, params: &DrawParams) {
        if mesh.positions.is_empty() {
            return;
        }
        s.stats.objects_drawn += 1;
        let mut verts = core::mem::take(&mut s.verts);
        let band = s.band;
        let mut drawn = 0;
        self.emit_mesh(&mut verts, mesh, model, params, &mut |a, b, c, m, col| {
            if fill_triangle(t, band, a, b, c, m, col) {
                drawn += 1;
            }
        });
        s.stats.tris_drawn += drawn;
        s.verts = verts;
    }

    /// Transformiert, clippt und beleuchtet ein Mesh; gibt jedes sichtbare
    /// Dreieck (Bildschirmkoordinaten, Modus, Farbe) an `out`.
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    fn emit_mesh(
        &self,
        verts: &mut Vec<Vert>,
        mesh: &Mesh,
        model: &Mat4,
        params: &DrawParams,
        out: &mut impl FnMut(Sv, Sv, Sv, DrawMode, u16),
    ) {
        if mesh.positions.is_empty() {
            return;
        }
        let comb =
            |r: Vec4| Vec4::new(r.dot(model.cols[0]), r.dot(model.cols[1]), r.dot(model.cols[2]), r.dot(model.cols[3]));
        let (mx, my, mw) = (comb(self.rx), comb(self.ry), comb(self.rw));
        let (cx, cy, near) = (self.cx, self.cy, self.near);
        let (gx, gy) = (cx * GUARD, cy * GUARD);
        let t0 = clock();
        verts.clear();
        for p in &mesh.positions {
            let v = Vec4::from_vec3(*p, 1.0);
            let (x, y, w) = (mx.dot(v), my.dot(v), mw.dot(v));
            let mut f = 0u8;
            if w < near {
                f |= F_NEAR;
            }
            if x < -cx * w {
                f |= F_LEFT;
            }
            if x > cx * w {
                f |= F_RIGHT;
            }
            if y < -cy * w {
                f |= F_TOP;
            }
            if y > cy * w {
                f |= F_BOTTOM;
            }
            if x < -gx * w || x > gx * w || y < -gy * w || y > gy * w {
                f |= F_GUARD;
            }
            let (sx, sy, z) = if w >= near {
                let iw = 1.0 / w;
                (cx + x * iw, cy + y * iw, near * iw)
            } else {
                (0.0, 0.0, 0.0)
            };
            verts.push(Vert { x, y, w, sx, sy, z, f });
        }

        // Licht in den Modellraum holen (wie im PC-Renderer).
        let (c0, c1, c2) = (model.cols[0].xyz(), model.cols[1].xyz(), model.cols[2].xyz());
        let (l0, l1, l2) = (c0.length().max(1e-12), c1.length().max(1e-12), c2.length().max(1e-12));
        let to_sun = -self.env.sun_dir;
        let lm = Vec3::new(c0.dot(to_sun) / l0, c1.dot(to_sun) / l1, c2.dot(to_sun) / l2);
        let um = Vec3::new(c0.y / l0, c1.y / l1, c2.y / l2);
        let front_sign = if c0.cross(c1).dot(c2) < 0.0 { -1.0 } else { 1.0 };
        let planes: [ClipPlane; 5] = [
            [0.0, 0.0, 1.0, -near],
            [1.0, 0.0, gx, 0.0],
            [-1.0, 0.0, gx, 0.0],
            [0.0, 1.0, gy, 0.0],
            [0.0, -1.0, gy, 0.0],
        ];
        let t1 = clock();
        BUILD_PROFILE[0].fetch_add(t1.wrapping_sub(t0), Relaxed);
        BUILD_PROFILE[2].fetch_add(mesh.tris.len() as u32, Relaxed);
        BUILD_PROFILE[8].fetch_add(1, Relaxed);
        let _end = EndClock(t1);
        for (ti, tri) in mesh.tris.iter().enumerate() {
            let (a, b, c) = (verts[tri[0] as usize], verts[tri[1] as usize], verts[tri[2] as usize]);
            if a.f & b.f & c.f & REJECT != 0 {
                continue;
            }
            if (a.f | b.f | c.f) & CLIP == 0 {
                let area = (b.sx - a.sx) * (c.sy - a.sy) - (b.sy - a.sy) * (c.sx - a.sx);
                let front = area * front_sign < 0.0;
                if !front && !params.double_sided {
                    continue;
                }
                let col = self.shade(mesh, ti, lm, um, params, front, (a.w + b.w + c.w) * (1.0 / 3.0));
                let sv = |v: Vert| Sv { x: v.sx, y: v.sy, z: v.z };
                out(sv(a), sv(b), sv(c), params.mode, col);
            } else {
                let k0 = clock();
                let _clip = EndClip(k0);
                let mut poly = [ClipVertex::default(); MAX_CLIP_VERTS];
                let cv = |v: Vert| ClipVertex { x: v.x, y: v.y, w: v.w };
                poly[0] = cv(a);
                poly[1] = cv(b);
                poly[2] = cv(c);
                let n = clip_polygon(&mut poly, 3, &planes);
                if n < 3 {
                    continue;
                }
                let mut sv = [Sv::default(); MAX_CLIP_VERTS];
                let mut area = 0.0;
                for k in 0..n {
                    let iw = 1.0 / poly[k].w;
                    sv[k] = Sv { x: cx + poly[k].x * iw, y: cy + poly[k].y * iw, z: near * iw };
                }
                for k in 0..n {
                    let (p, q) = (sv[k], sv[(k + 1) % n]);
                    area += p.x * q.y - q.x * p.y;
                }
                let front = area * front_sign < 0.0;
                if !front && !params.double_sided {
                    continue;
                }
                let col = self.shade(mesh, ti, lm, um, params, front, (a.w + b.w + c.w).max(near) * (1.0 / 3.0));
                for k in 1..n - 1 {
                    out(sv[0], sv[k], sv[k + 1], params.mode, col);
                }
            }
        }
    }

    /// Flächenfarbe: Licht (oder eingebrannt), Tönung, Nebel pro Dreieck.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    fn shade(&self, mesh: &Mesh, ti: usize, lm: Vec3, um: Vec3, p: &DrawParams, front: bool, w: f32) -> u16 {
        // Schatten: halb mit (getöntem) Schwarz mischen – ohne Tönung genau
        // `darken` (avg(p, 0) == darken(p)).
        if p.mode == DrawMode::Shadow {
            return self.tinted(0);
        }
        let base = p.color_override.unwrap_or(mesh.colors[ti]);
        let env = &self.env;
        let mut c = if p.unlit {
            if p.tint == [1.0, 1.0, 1.0] { base } else { base.mul_rgb(p.tint) }
        } else {
            let n = if front { mesh.normals[ti] } else { -mesh.normals[ti] };
            let ndl = n.dot(lm).max(0.0);
            let hemi = n.dot(um) * 0.5 + 0.5;
            let mut l = [0.0f32; 3];
            for k in 0..3 {
                let amb = env.ambient_ground[k] + (env.ambient_sky[k] - env.ambient_ground[k]) * hemi;
                l[k] = (amb + env.sun_color[k] * ndl) * p.tint[k];
            }
            base.mul_rgb(l)
        };
        if self.fog_on && w > env.fog_start {
            // smoothstep mit vorab berechnetem Kehrwert (keine Division je Dreieck).
            let f = ((w - env.fog_start) * self.fog_inv).clamp(0.0, 1.0);
            c = c.lerp(self.fog_color, f * f * (3.0 - 2.0 * f));
        }
        self.tinted(gfx::from_color(c))
    }
}

/// Bucht beim Verlassen die in einem Streifen gefüllten Zeilen und Pixel auf
/// [`RENDER_PROFILE`] (Zeiger auf die Zähler in `Scratch`, Startwerte).
struct Counted(*const u32, u32, *const u32, u32);

impl Drop for Counted {
    fn drop(&mut self) {
        // SAFETY: Die Zeiger zeigen auf Felder des `Scratch`, der während des
        // ganzen Aufrufs lebt; nur gelesen, nachdem nicht mehr geschrieben wird.
        let (rows, px) = unsafe { (core::ptr::read_volatile(self.0), core::ptr::read_volatile(self.2)) };
        RENDER_PROFILE[3].fetch_add(rows.wrapping_sub(self.1), Relaxed);
        RENDER_PROFILE[4].fetch_add(px.wrapping_sub(self.3), Relaxed);
    }
}

/// Bucht beim Verlassen die Zeit seit `.0` auf die geclippten Dreiecke.
struct EndClip(u32);

impl Drop for EndClip {
    fn drop(&mut self) {
        BUILD_PROFILE[6].fetch_add(clock().wrapping_sub(self.0), Relaxed);
        BUILD_PROFILE[7].fetch_add(1, Relaxed);
    }
}

/// Bucht beim Verlassen die Zeit seit `.0` auf [`BUILD_PROFILE`] (Dreiecke).
struct EndClock(u32);

impl Drop for EndClock {
    fn drop(&mut self) {
        BUILD_PROFILE[1].fetch_add(clock().wrapping_sub(self.0), Relaxed);
    }
}

/// Füllt `d` mit `v`, zwei Pixel je 32-Bit-Schreibzugriff.
#[inline]
fn fill_u16(d: &mut [u16], v: u16) {
    let (head, mid, tail) = unsafe { d.align_to_mut::<u32>() };
    head.fill(v);
    mid.fill(v as u32 | (v as u32) << 16);
    tail.fill(v);
}

/// Schnelles `ceil` (ohne Bibliotheksaufruf). Gültig für |x| < 2^31.
#[inline(always)]
fn ceil_i32(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) < x { i + 1 } else { i }
}

const FX: f32 = 65536.0;

/// Zeilen je Band für die Zuordnung der Dreiecke zu Streifen (die
/// Firmware-Streifen sind 8 bzw. 16 Zeilen hoch und liegen in einem Band).
const BIN_ROWS: usize = 16;

#[inline(always)]
fn fx(v: f32) -> i32 {
    (v * FX) as i32
}

#[inline(always)]
fn slope(dx: f32, dy: f32) -> f32 {
    if dy > 0.0 { (dx / dy).clamp(-8000.0, 8000.0) } else { 0.0 }
}

/// Ein für das Rastern vorbereitetes Dreieck (einmal pro Frame, gilt für alle
/// Streifen): sortierte Ecken, 16.16-Kanten ab der ersten Zeile, Tiefenebene.
#[derive(Clone, Copy, Debug)]
struct Prim {
    /// Zeilen `[y0, y1)` im Bild (für den schnellen Streifentest).
    y0: i16,
    y1: i16,
    /// Erste Zeile des Dreiecks (ungeclippt), der unteren Hälfte, Ende.
    y_top: i32,
    y_b: i32,
    y_bot: i32,
    x_long: i32,
    d_long: i32,
    x_ab: i32,
    d_ab: i32,
    x_bc: i32,
    d_bc: i32,
    ax: f32,
    ay: f32,
    za: f32,
    dzdx: f32,
    dzdy: f32,
    dz_step: i32,
    col: u16,
    mode: DrawMode,
    mid_left: bool,
}

impl Prim {
    /// Wie der Anfang des Rasterns, aber ohne Streifen (`w`×`h` = ganzes
    /// Bild). `None`, wenn das Dreieck nichts bedeckt.
    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    fn setup(w: i32, h: i32, v0: Sv, v1: Sv, v2: Sv, mode: DrawMode, col: u16) -> Option<Prim> {
        let sh = |v: Sv| Sv { x: v.x - 0.5, y: v.y - 0.5, z: v.z };
        let (mut a, mut b, mut c) = (sh(v0), sh(v1), sh(v2));
        if a.y > b.y {
            core::mem::swap(&mut a, &mut b);
        }
        if b.y > c.y {
            core::mem::swap(&mut b, &mut c);
        }
        if a.y > b.y {
            core::mem::swap(&mut a, &mut b);
        }
        let total_h = c.y - a.y;
        if total_h.is_nan() || total_h <= 0.0 {
            return None;
        }
        let y_top = ceil_i32(a.y);
        let y_bot = ceil_i32(c.y);
        let (y0, y1) = (y_top.max(0), y_bot.min(h));
        if y0 >= y1 {
            return None;
        }
        let x_lo = ceil_i32(a.x.min(b.x).min(c.x)).max(0);
        let x_hi = ceil_i32(a.x.max(b.x).max(c.x)).min(w);
        if x_lo >= x_hi {
            return None;
        }
        let (e1x, e1y) = (b.x - a.x, b.y - a.y);
        let (e2x, e2y) = (c.x - a.x, c.y - a.y);
        let area = e1x * e2y - e1y * e2x;
        if area > -1e-6 && area < 1e-6 {
            return None;
        }
        let inv_area = 1.0 / area;
        let dz1 = (b.z - a.z) * DEPTH_ONE;
        let dz2 = (c.z - a.z) * DEPTH_ONE;
        let dzdx = (dz1 * e2y - dz2 * e1y) * inv_area;
        let dzdy = (dz2 * e1x - dz1 * e2x) * inv_area;
        let dz_step = if dzdx >= 0.0 { (dzdx + 0.5) as i32 } else { (dzdx - 0.5) as i32 };
        let s_ac = slope(e2x, total_h);
        let s_ab = slope(e1x, e1y);
        let s_bc = slope(c.x - b.x, c.y - b.y);
        let y_b = ceil_i32(b.y);
        Some(Prim {
            y0: y0 as i16,
            y1: y1 as i16,
            y_top,
            y_b,
            y_bot,
            x_long: fx(a.x + s_ac * (y_top as f32 - a.y)),
            d_long: fx(s_ac),
            x_ab: fx(a.x + s_ab * (y_top as f32 - a.y)),
            d_ab: fx(s_ab),
            x_bc: fx(b.x + s_bc * (y_b as f32 - b.y)),
            d_bc: fx(s_bc),
            ax: a.x,
            ay: a.y,
            za: a.z * DEPTH_ONE,
            dzdx,
            dzdy,
            dz_step,
            col,
            mode,
            mid_left: b.x < a.x + s_ac * e1y,
        })
    }

    /// Rastert die Zeilen `band` (innerhalb der Zeilen von `t`). Die Kanten
    /// werden immer ab der ersten Dreieckszeile fortgeschrieben – jeder
    /// Streifen liefert bitgenau dasselbe wie ein Durchgang über das Bild.
    #[cfg_attr(target_os = "espidf", unsafe(link_section = ".iram1.pudding"))]
    fn raster(&self, t: &mut Target, band: (i32, i32), st: &mut Stats) -> bool {
        match self.mode {
            DrawMode::Opaque => self.raster_mode::<0>(t, band, st),
            // Schatten = halb mit (getöntem) Schwarz mischen, siehe `shade`.
            DrawMode::Shadow | DrawMode::Blend50 => self.raster_mode::<2>(t, band, st),
        }
    }

    /// [`Prim::raster`] für eine Zeichenart (`M`: 0 opak, 1 Schatten,
    /// 2 halb durchsichtig) – die Art steht so schon beim Übersetzen fest.
    #[inline(always)]
    fn raster_mode<const M: u8>(&self, t: &mut Target, band: (i32, i32), st: &mut Stats) -> bool {
        let y_start = self.y_top.max(band.0);
        let y_end = self.y_bot.min(band.1);
        if y_start >= y_end {
            return false;
        }
        let y_mid = self.y_b.clamp(y_start, y_end);
        let width = t.w;
        let row0 = t.row0;
        // Alle Zeilen liegen im Streifen: y ∈ [y_start, y_end) ⊆ Zeilen von `t`.
        assert!(
            y_start as usize >= row0
                && (y_end as usize - row0) * width <= t.color.len()
                && (y_end as usize - row0) * width <= t.depth.len()
        );
        let r = Rows {
            cbase: t.color.as_mut_ptr(),
            zbase: t.depth.as_mut_ptr(),
            width,
            row0,
            w: t.w as i32,
            mid_left: self.mid_left,
            za: self.za,
            dzdx: self.dzdx,
            dzdy: self.dzdy,
            ax: self.ax,
            ay: self.ay,
            // Tiefe als 16.16-Festkomma (u32): Pixeltiefe = obere 16 Bit –
            // dieselben Bits wie `(z >> 8) as u16` beim 24-Bit-Wert.
            dzz: (self.dz_step as u32) << 8,
            col: self.col,
            d_long: self.d_long,
        };
        let ahead = |from: i32, to: i32, d: i32| (to - from).wrapping_mul(d);
        let mut x_long = self.x_long.wrapping_add(ahead(self.y_top, y_start, self.d_long));
        let (mut rows, mut px) = (0u32, 0u32);
        // SAFETY (beide Hälften): Zeilen im Streifen (assert oben), Spalten in
        // [0, w) – siehe `Rows::run`.
        // Obere Hälfte (a → b).
        if y_mid > y_start {
            let x_short = self.x_ab.wrapping_add(ahead(self.y_top, y_start, self.d_ab));
            x_long = unsafe { r.run::<M>(y_start, y_mid, x_long, x_short, self.d_ab, &mut rows, &mut px) };
        }
        // Untere Hälfte (b → c).
        if y_end > y_mid {
            let x_short = self.x_bc.wrapping_add(ahead(self.y_b, y_mid, self.d_bc));
            unsafe { r.run::<M>(y_mid, y_end, x_long, x_short, self.d_bc, &mut rows, &mut px) };
        }
        st.rows += rows;
        st.px += px;
        true
    }
}

/// Was das Rastern der Zeilen eines Dreiecks braucht – als Werte (nicht über
/// Zeiger), damit alles in Registern bleibt.
struct Rows {
    cbase: *mut u16,
    zbase: *mut u16,
    width: usize,
    row0: usize,
    w: i32,
    mid_left: bool,
    za: f32,
    dzdx: f32,
    dzdy: f32,
    ax: f32,
    ay: f32,
    dzz: u32,
    col: u16,
    d_long: i32,
}

impl Rows {
    /// Rastert die Zeilen `[y0, y1)`: lange Kante ab `x_long`, kurze ab
    /// `x_short` (je 16.16, Schritt `d_long` bzw. `d_short`). Liefert die
    /// lange Kante nach der letzten Zeile.
    ///
    /// # Safety
    /// Alle Zeilen müssen im Streifen liegen (`(y − row0)·width` + Breite ≤
    /// Länge beider Puffer).
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    unsafe fn run<const M: u8>(
        &self,
        y0: i32,
        y1: i32,
        mut x_long: i32,
        mut x_short: i32,
        d_short: i32,
        rows: &mut u32,
        px: &mut u32,
    ) -> i32 {
        let (mut nr, mut np) = (0u32, 0u32);
        for y in y0..y1 {
            let (xl, xr) = if self.mid_left { (x_short, x_long) } else { (x_long, x_short) };
            let x0 = (xl.wrapping_add(0xFFFF) >> 16).max(0);
            let x1 = (xr.wrapping_add(0xFFFF) >> 16).min(self.w);
            if x0 < x1 {
                nr += 1;
                np += (x1 - x0) as u32;
                let z = (self.za + self.dzdx * (x0 as f32 - self.ax) + self.dzdy * (y as f32 - self.ay)) as i32;
                let o = (y as usize - self.row0) * self.width + x0 as usize;
                // SAFETY: 0 ≤ x0 < x1 ≤ w, Zeile im Streifen (Aufrufer).
                unsafe {
                    span::<M>(self.cbase.add(o), self.zbase.add(o), (x1 - x0) as usize, (z as u32) << 8, self.dzz, self.col);
                }
            }
            x_long = x_long.wrapping_add(self.d_long);
            x_short = x_short.wrapping_add(d_short);
        }
        *rows += nr;
        *px += np;
        x_long
    }
}

/// Ein Zeilenstück aus `n` Pixeln ab `cp`/`zp`: Tiefe `zz` (16.16, steigt je
/// Pixel um `dz`) gegen den Tiefenpuffer prüfen, dann je nach Art `M` Farbe
/// und Tiefe schreiben (opak), abdunkeln (Schatten) oder mischen.
///
/// # Safety
/// `cp` und `zp` müssen für `n` Pixel gültig sein.
#[inline(always)]
#[allow(unused_assignments)]
unsafe fn span<const M: u8>(cp: *mut u16, zp: *mut u16, n: usize, mut zz: u32, dz: u32, col: u16) {
    let mut i = 0;
    macro_rules! px {
        ($k:expr) => {{
            let d = (zz >> 16) as u16;
            let (c, z) = (cp.add(i + $k), zp.add(i + $k));
            if d > *z {
                match M {
                    0 => {
                        *z = d;
                        *c = col;
                    }
                    _ => *c = gfx::avg(*c, col),
                }
            }
            zz = zz.wrapping_add(dz);
        }};
    }
    // SAFETY (alle Zugriffe): i + k < n.
    unsafe {
        while i + 4 <= n {
            px!(0);
            px!(1);
            px!(2);
            px!(3);
            i += 4;
        }
        while i < n {
            px!(0);
            i += 1;
        }
    }
}

/// Rastert ein Dreieck in die Zeilen `band` (Top-Left-Füllregel, Pixel-
/// zentren bei .5, Kanten als 16.16-Fixpunkt ab dem oberen Endpunkt).
/// `band` muss innerhalb der Zeilen von `t` liegen.
fn fill_triangle(t: &mut Target, band: (i32, i32), v0: Sv, v1: Sv, v2: Sv, mode: DrawMode, col: u16) -> bool {
    match Prim::setup(t.w as i32, t.h as i32, v0, v1, v2, mode, col) {
        Some(p) => p.raster(t, band, &mut Stats::default()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use spatz::render::shapes;

    fn target<'a>(c: &'a mut [u16], d: &'a mut [u16]) -> Target<'a> {
        Target::new(c, d, gfx::W, gfx::H)
    }

    fn scene() -> (MeshStore, DrawList, Camera, Environment) {
        let mut store = MeshStore::new();
        let cube = store.add(shapes::box_centered(Vec3::splat(2.0), Color::hex(0xFF4080)));
        let mut list = DrawList::new();
        for i in 0..5 {
            list.add(cube, Mat4::translation(Vec3::new(i as f32 * 2.5 - 5.0, 0.0, -6.0 - i as f32)));
        }
        let mut cam = Camera { fov_y: 1.0, near: 0.1, far: 80.0, ..Camera::default() };
        cam.look_at(Vec3::new(0.0, 2.0, 4.0), Vec3::new(0.0, 0.0, -6.0));
        (store, list, cam, Environment { fog: true, fog_start: 8.0, fog_end: 30.0, ..Environment::default() })
    }

    /// Opake Zeilenstücke (entrollte Schleife, 16.16-Tiefe) ergeben dasselbe
    /// wie die einfache Rechnung Pixel für Pixel – bei jeder Ausrichtung,
    /// Länge und vorhandenen Tiefe.
    #[test]
    fn opaque_span_matches_per_pixel() {
        let mut seed = 12345u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        for n in 0..24usize {
            for off in 0..2usize {
                for _ in 0..20 {
                    // u32-Speicher → u16-Sicht garantiert 4-Byte-ausgerichtet.
                    let mut cw = vec![0u32; 16];
                    let mut zw = vec![0u32; 16];
                    for (c, z) in cw.iter_mut().zip(zw.iter_mut()) {
                        *c = rnd();
                        *z = rnd() & 0x7FFF_7FFF;
                    }
                    let zz = rnd() & 0x7FFF_FFFF;
                    let dz = (rnd() % 0x4_0000).wrapping_sub(0x2_0000);
                    let col = rnd() as u16;
                    // Referenz: Pixel für Pixel.
                    let (c0, z0): (Vec<u16>, Vec<u16>) = unsafe {
                        (
                            core::slice::from_raw_parts(cw.as_ptr() as *const u16, 32).to_vec(),
                            core::slice::from_raw_parts(zw.as_ptr() as *const u16, 32).to_vec(),
                        )
                    };
                    let (mut cr, mut zr) = (c0.clone(), z0.clone());
                    let mut z = zz;
                    for k in 0..n {
                        let d = (z >> 16) as u16;
                        if d > zr[off + k] {
                            zr[off + k] = d;
                            cr[off + k] = col;
                        }
                        z = z.wrapping_add(dz);
                    }
                    unsafe {
                        let cp = (cw.as_mut_ptr() as *mut u16).add(off);
                        let zp = (zw.as_mut_ptr() as *mut u16).add(off);
                        span::<0>(cp, zp, n, zz, dz, col);
                        let c1 = core::slice::from_raw_parts(cw.as_ptr() as *const u16, 32);
                        let z1 = core::slice::from_raw_parts(zw.as_ptr() as *const u16, 32);
                        assert_eq!(c1, &cr[..], "Farbe n={n} off={off}");
                        assert_eq!(z1, &zr[..], "Tiefe n={n} off={off}");
                    }
                }
            }
        }
    }

    /// Zwei Streifen – nacheinander oder auf zwei Threads gleichzeitig –
    /// ergeben exakt dasselbe Bild wie ein Durchgang.
    #[test]
    fn strips_equal_full_frame() {
        let (store, list, cam, env) = scene();
        let n = gfx::W * gfx::H;
        let (mut c1, mut d1) = (vec![0u16; n], vec![0u16; n]);
        let mut r = Renderer::new(16);
        r.begin(&cam, &env, gfx::W, gfx::H);
        r.sort(&store, &list);
        let mut s1 = Scratch::with_capacity(64);
        let mut t1 = target(&mut c1, &mut d1);
        r.clear(&mut t1);
        r.draw(&mut s1, &mut t1, &store, &list);
        assert!(s1.stats.tris_drawn > 10);

        // Nacheinander in zwei Streifen.
        let (mut c2, mut d2) = (vec![0u16; n], vec![0u16; n]);
        let (mut top, mut bottom) = target(&mut c2, &mut d2).split(77);
        let mut s2 = Scratch::default();
        for t in [&mut top, &mut bottom] {
            r.clear(t);
            r.draw(&mut s2, t, &store, &list);
        }
        assert_eq!(c1, c2);

        // Zwei Threads gleichzeitig (wie die beiden Kerne des ESP32-P4).
        let (mut c3, mut d3) = (vec![0u16; n], vec![0u16; n]);
        let (mut top, mut bottom) = target(&mut c3, &mut d3).split(gfx::H / 2);
        let (store, list, r) = (&store, &list, &r);
        std::thread::scope(|sc| {
            sc.spawn(move || {
                let mut s = Scratch::default();
                r.clear(&mut bottom);
                r.draw(&mut s, &mut bottom, store, list);
            });
            let mut s = Scratch::default();
            r.clear(&mut top);
            r.draw(&mut s, &mut top, store, list);
        });
        assert_eq!(c1, c3);
        let sky = c1[0];
        assert!(c1.iter().filter(|&&p| p != sky).count() > 1000, "Würfel sichtbar");
    }

    /// Keine Lücken: zwei Dreiecke eines Quadrats decken jedes Pixel genau einmal.
    #[test]
    fn shared_edges_have_no_gaps() {
        let (mut c, mut d) = (vec![0u16; gfx::W * gfx::H], vec![0u16; gfx::W * gfx::H]);
        let mut t = target(&mut c, &mut d);
        let p = |x, y| Sv { x, y, z: 0.5 };
        let (a, b, cc, dd) = (p(10.3, 10.7), p(200.1, 15.2), p(190.6, 140.9), p(20.2, 130.4));
        let band = (0, gfx::H as i32);
        fill_triangle(&mut t, band, a, b, cc, DrawMode::Blend50, gfx::rgb(255, 255, 255));
        fill_triangle(&mut t, band, a, cc, dd, DrawMode::Blend50, gfx::rgb(255, 255, 255));
        // Doppelt gezeichnet → heller als eine Mischung mit Schwarz.
        let once = gfx::avg(0, gfx::rgb(255, 255, 255));
        assert!(c.iter().all(|&px| px == 0 || px == once));
        assert!(c.iter().filter(|&&px| px == once).count() > 15000);
    }
}
