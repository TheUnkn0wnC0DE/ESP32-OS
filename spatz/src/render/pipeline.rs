//! Render-Pipeline: Culling → Vertex-Transformation → Clipping → Beleuchtung
//! → Rasterisierung → Himmel + Nebel (ein Nachbearbeitungs-Durchlauf).

use super::camera::Camera;
use super::clip::{ClipPlane, ClipVertex, MAX_CLIP_VERTS, clip_polygon};
use super::framebuffer::{DEPTH_SCALE, Framebuffer};
use super::mesh::{Mesh, MeshId, MeshStore};
use super::raster::{Blend50, ScreenVertex, Shadow, Solid, ceil_i32, fill_rect_depth, fill_triangle};
use crate::color::{Color, blend256};
use crate::math::{Frustum, Mat4, Vec3, Vec4, smoothstep, tan};
use alloc::vec::Vec;

const FOG_LUT_SIZE: usize = 1024;
/// Guard-Band in Vielfachen der halben Bildgröße.
const GUARD: f32 = 8.0;

const OUT_NEAR: u8 = 1;
const OUT_LEFT: u8 = 2;
const OUT_RIGHT: u8 = 4;
const OUT_TOP: u8 = 8;
const OUT_BOTTOM: u8 = 16;
const OUT_GUARD: u8 = 32;
const REJECT_MASK: u8 = OUT_NEAR | OUT_LEFT | OUT_RIGHT | OUT_TOP | OUT_BOTTOM;
const CLIP_MASK: u8 = OUT_NEAR | OUT_GUARD;

/// Licht, Himmel und Nebel einer Szene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// Richtung, in die das Sonnenlicht fällt (normiert).
    pub sun_dir: Vec3,
    pub sun_color: [f32; 3],
    /// Umgebungslicht von oben bzw. unten (Hemisphären-Licht).
    pub ambient_sky: [f32; 3],
    pub ambient_ground: [f32; 3],
    pub sky_zenith: Color,
    pub sky_horizon: Color,
    /// Farbe unterhalb des Horizonts (Dunst).
    pub sky_ground: Color,
    pub fog: bool,
    pub fog_start: f32,
    pub fog_end: f32,
    pub sun_disc: bool,
    pub sun_disc_color: Color,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(0.35, -1.0, 0.45).normalize_or_zero(),
            sun_color: [0.75, 0.72, 0.62],
            ambient_sky: [0.48, 0.52, 0.62],
            ambient_ground: [0.30, 0.26, 0.26],
            sky_zenith: Color::hex(0x3A78D8),
            sky_horizon: Color::hex(0xBFE0F5),
            sky_ground: Color::hex(0x9DB8C8),
            fog: true,
            fog_start: 40.0,
            fog_end: 120.0,
            sun_disc: true,
            sun_disc_color: Color::hex(0xFFF6D0),
        }
    }
}

impl Environment {
    /// Himmelsfarbe für eine Blickrichtung mit Höhenwinkel-Sinus `e`.
    pub fn sky_color(&self, e: f32) -> Color {
        if e >= 0.0 {
            self.sky_horizon.lerp(self.sky_zenith, smoothstep(0.0, 0.65, e))
        } else {
            self.sky_horizon.lerp(self.sky_ground, smoothstep(0.0, -0.2, e))
        }
    }
}

/// Wie ein Objekt gezeichnet wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawMode {
    /// Undurchsichtig (Tiefentest + Tiefenschreiben).
    Opaque,
    /// 50 % durchscheinend, nach den opaken Objekten.
    Blend50,
    /// Halbiert die Helligkeit darunter (Blob-Schatten).
    Shadow,
}

/// Parameter pro Zeichenaufruf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawParams {
    pub mode: DrawMode,
    /// Multiplikator auf das Licht (z. B. Treffer-Blinken).
    pub tint: [f32; 3],
    /// Ohne Beleuchtung (leuchtende Objekte).
    pub unlit: bool,
    /// Rückseiten nicht verwerfen.
    pub double_sided: bool,
    /// Ersetzt alle Flächenfarben.
    pub color_override: Option<Color>,
    /// Nur zeichnen, wenn näher als diese Blicktiefe (Detailstufe/LOD).
    pub cull_distance: f32,
    /// Nur zeichnen, wenn mindestens so weit weg (Gegenstück für Low-Poly-LODs).
    pub cull_near: f32,
    /// Verschiebt die Sortiertiefe (z. B. riesige Hintergrundflächen zuletzt).
    pub sort_bias: f32,
}

impl Default for DrawParams {
    fn default() -> Self {
        Self::OPAQUE
    }
}

impl DrawParams {
    pub const OPAQUE: Self = Self {
        mode: DrawMode::Opaque,
        tint: [1.0, 1.0, 1.0],
        unlit: false,
        double_sided: false,
        color_override: None,
        cull_distance: f32::INFINITY,
        cull_near: f32::NEG_INFINITY,
        sort_bias: 0.0,
    };
    pub const SHADOW: Self = Self { mode: DrawMode::Shadow, ..Self::OPAQUE };
    pub const BLEND: Self = Self { mode: DrawMode::Blend50, ..Self::OPAQUE };

    pub fn tinted(tint: [f32; 3]) -> Self {
        Self { tint, ..Self::OPAQUE }
    }
    pub fn unlit() -> Self {
        Self { unlit: true, ..Self::OPAQUE }
    }
    pub fn with_color(c: Color) -> Self {
        Self { color_override: Some(c), ..Self::OPAQUE }
    }
    /// Detailstufe: nur bis zur Blicktiefe `d` zeichnen.
    pub fn lod(d: f32) -> Self {
        Self { cull_distance: d, ..Self::OPAQUE }
    }
    /// Gleiche Parameter mit Detailstufe.
    pub fn with_lod(mut self, d: f32) -> Self {
        self.cull_distance = d;
        self
    }
    /// Nur im Tiefenbereich `[near, far)` zeichnen (LOD-Stufen tauschen).
    pub fn with_lod_range(mut self, near: f32, far: f32) -> Self {
        self.cull_near = near;
        self.cull_distance = far;
        self
    }
}

/// Zähler pro Frame (für das Performance-Overlay).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub objects: u32,
    pub objects_culled: u32,
    pub tris_in: u32,
    pub tris_culled: u32,
    pub tris_clipped: u32,
    pub tris_drawn: u32,
    pub rows: u32,
}

/// Ein Eintrag in der [`DrawList`].
#[derive(Clone, Copy, Debug)]
pub struct DrawItem {
    pub mesh: MeshId,
    pub model: Mat4,
    pub params: DrawParams,
}

/// Bildschirmausgerichtetes Quadrat (Partikel).
#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    pub pos: Vec3,
    pub size: f32,
    pub color: Color,
}

/// Sammelt Zeichenaufrufe eines Frames; der [`Renderer`] sortiert sie
/// (opak vorne→hinten, transparent hinten→vorne). Wiederverwendbar ohne
/// Allokationen pro Frame.
#[derive(Default)]
pub struct DrawList {
    pub items: Vec<DrawItem>,
    pub sprites: Vec<Sprite>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }
    /// Mit vorab reservierter Kapazität → keine Allokationen im laufenden Spiel.
    pub fn with_capacity(items: usize, sprites: usize) -> Self {
        Self { items: Vec::with_capacity(items), sprites: Vec::with_capacity(sprites) }
    }
    pub fn clear(&mut self) {
        self.items.clear();
        self.sprites.clear();
    }
    #[inline]
    pub fn push(&mut self, mesh: MeshId, model: Mat4, params: DrawParams) {
        self.items.push(DrawItem { mesh, model, params });
    }
    #[inline]
    pub fn add(&mut self, mesh: MeshId, model: Mat4) {
        self.push(mesh, model, DrawParams::OPAQUE);
    }
    #[inline]
    pub fn sprite(&mut self, pos: Vec3, size: f32, color: Color) {
        self.sprites.push(Sprite { pos, size, color });
    }
}

/// Der Software-Renderer. Hält nur Scratch-Speicher und Frame-Zustand.
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
    fog_lut: [u16; FOG_LUT_SIZE],
    fog_shift: u32,
    sky_rows: Vec<u32>,
    vc: VertexCache,
    order: Vec<(f32, u32)>,
    /// Statistik des laufenden Frames.
    pub stats: RenderStats,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            w: 1,
            h: 1,
            cx: 0.5,
            cy: 0.5,
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
            fog_lut: [0; FOG_LUT_SIZE],
            fog_shift: 0,
            sky_rows: Vec::new(),
            vc: VertexCache::default(),
            order: Vec::new(),
            stats: RenderStats::default(),
        }
    }

    /// Reserviert Scratch-Speicher vorab (größtes Mesh, Anzahl Zeichenaufrufe,
    /// Bildhöhe), damit im laufenden Spiel keine Allokationen mehr anfallen.
    pub fn reserve(&mut self, max_vertices: usize, max_items: usize, max_height: usize) {
        self.vc.reserve(max_vertices);
        self.order.reserve(max_items.saturating_sub(self.order.len()));
        self.sky_rows.reserve(max_height.saturating_sub(self.sky_rows.len()));
    }

    /// Startet einen Frame: Matrizen, Frustum, Nebel-Tabelle, Tiefenpuffer.
    pub fn begin(&mut self, fb: &mut Framebuffer, cam: &Camera, env: &Environment) {
        self.w = fb.width();
        self.h = fb.height();
        self.cx = self.w as f32 * 0.5;
        self.cy = self.h as f32 * 0.5;
        let tan_y = tan(cam.fov_y * 0.5);
        let tan_x = tan_y * self.w as f32 / self.h as f32;
        self.focal = self.cy / tan_y;
        self.near = cam.near;
        // Hinter dem Nebelende ist alles unsichtbar → dort gar nicht erst zeichnen.
        self.far = if env.fog && env.fog_end > env.fog_start { cam.far.min(env.fog_end) } else { cam.far };
        let (r, u, f) = cam.basis();
        let p = cam.position;
        self.rx = Vec4::new(r.x, r.y, r.z, -r.dot(p)) * self.focal;
        self.ry = Vec4::new(u.x, u.y, u.z, -u.dot(p)) * -self.focal;
        self.rw = Vec4::new(f.x, f.y, f.z, -f.dot(p));
        self.cam_pos = p;
        self.cam_fwd = f;
        self.frustum = Frustum::new(p, r, u, f, tan_x, tan_y, cam.near, self.far);
        self.env = *env;
        self.update_fog_lut();
        self.update_sky_rows(u, f);
        // Der Nebel-Durchlauf des letzten Frames leert den Tiefenpuffer mit;
        // nur wenn das nicht geschah (erster Frame, Abbruch), hier löschen.
        if !fb.depth_is_clean() {
            fb.clear_depth();
        }
        fb.mark_depth_dirty();
        self.stats = RenderStats::default();
    }

    /// Brennweite in Pixeln (nach `begin`).
    pub fn focal_px(&self) -> f32 {
        self.focal
    }

    /// Projiziert einen Weltpunkt in Pixelkoordinaten (`None` hinter der Kamera).
    pub fn project(&self, p: Vec3) -> Option<(f32, f32, f32)> {
        let v = Vec4::from_vec3(p, 1.0);
        let w = self.rw.dot(v);
        if w < self.near {
            return None;
        }
        Some((self.cx + self.rx.dot(v) / w, self.cy + self.ry.dot(v) / w, w))
    }

    fn update_fog_lut(&mut self) {
        let env = &self.env;
        self.fog_on = env.fog && env.fog_end > env.fog_start;
        // Auf x86_64 rechnet der SSE2-Pfad den Nebel direkt – Tabelle unnötig.
        if !self.fog_on || cfg!(target_arch = "x86_64") {
            return;
        }
        let z_start = self.near * DEPTH_SCALE / env.fog_start.max(self.near);
        let mut shift = 0u32;
        while (FOG_LUT_SIZE as f32) * ((1u64 << shift) as f32) < z_start && shift < 30 {
            shift += 1;
        }
        self.fog_shift = shift;
        let step = (1u64 << shift) as f32;
        for (i, e) in self.fog_lut.iter_mut().enumerate() {
            let zc = (i as f32 + 0.5) * step;
            let w = self.near * DEPTH_SCALE / zc;
            let t = smoothstep(env.fog_start, env.fog_end, w);
            *e = (t * 256.0) as u16;
        }
        // Eintrag 0 = Himmel (z = 0) → reine Himmelsfarbe; letzter Eintrag =
        // alles Nähere → kein Nebel. So braucht der Durchlauf keine Verzweigung.
        self.fog_lut[0] = 256;
        self.fog_lut[FOG_LUT_SIZE - 1] = 0;
    }

    fn update_sky_rows(&mut self, up: Vec3, fwd: Vec3) {
        self.sky_rows.resize(self.h, 0);
        for (y, row) in self.sky_rows.iter_mut().enumerate() {
            let d = fwd * self.focal + up * (self.cy - (y as f32 + 0.5));
            *row = self.env.sky_color(d.y / d.length()).0;
        }
    }

    /// Zeichnet ein Mesh sofort. Gibt `false` zurück, wenn es gecullt wurde.
    pub fn draw_mesh(&mut self, fb: &mut Framebuffer, mesh: &Mesh, model: &Mat4, params: &DrawParams) -> bool {
        #[cfg(target_arch = "x86_64")]
        if crate::cpu::has_v3() {
            // SAFETY: Die benötigten CPU-Funktionen wurden zur Laufzeit geprüft.
            return unsafe { self.draw_mesh_v3(fb, mesh, model, params) };
        }
        self.draw_mesh_impl(fb, mesh, model, params)
    }

    /// Dieselbe Pipeline, kompiliert für x86-64-v3 (ohne FMA → bitgleiche
    /// Gleitkommaergebnisse). Nur nach `cpu::has_v3()` aufrufen.
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt")]
    unsafe fn draw_mesh_v3(&mut self, fb: &mut Framebuffer, mesh: &Mesh, model: &Mat4, params: &DrawParams) -> bool {
        self.draw_mesh_impl(fb, mesh, model, params)
    }

    // Nur auf x86_64 fest einbetten (dort für die AVX2-Variante nötig); auf
    // anderen Zielen (z. B. ARM-Mikrocontroller) würde das nur Code aufblähen.
    #[cfg_attr(target_arch = "x86_64", inline(always))]
    fn draw_mesh_impl(&mut self, fb: &mut Framebuffer, mesh: &Mesh, model: &Mat4, params: &DrawParams) -> bool {
        self.stats.objects += 1;
        if !self.frustum.sphere_visible(&mesh.bounds.transformed(model)) {
            self.stats.objects_culled += 1;
            return false;
        }
        let comb =
            |r: Vec4| Vec4::new(r.dot(model.cols[0]), r.dot(model.cols[1]), r.dot(model.cols[2]), r.dot(model.cols[3]));
        let (mx, my, mw) = (comb(self.rx), comb(self.ry), comb(self.rw));
        let (cx, cy, near) = (self.cx, self.cy, self.near);
        let (gx, gy) = (cx * GUARD, cy * GUARD);

        // 1) Vertices transformieren, Outcodes, Projektion (SoA, SSE2 4-fach).
        let xf = Xform { mx, my, mw, cx, cy, near, gx, gy };
        self.vc.transform(&mesh.positions, &xf);

        // 2) Licht in den Modellraum holen (2 Skalarprodukte pro Fläche).
        let (c0, c1, c2) = (model.cols[0].xyz(), model.cols[1].xyz(), model.cols[2].xyz());
        let (l0, l1, l2) = (c0.length().max(1e-12), c1.length().max(1e-12), c2.length().max(1e-12));
        let to_sun = -self.env.sun_dir;
        let lm = Vec3::new(c0.dot(to_sun) / l0, c1.dot(to_sun) / l1, c2.dot(to_sun) / l2);
        let um = Vec3::new(c0.y / l0, c1.y / l1, c2.y / l2);
        // Spiegelnde Transformationen drehen die Orientierung um.
        let front_sign = if c0.cross(c1).dot(c2) < 0.0 { -1.0 } else { 1.0 };

        let planes: [ClipPlane; 5] = [
            [0.0, 0.0, 1.0, -near],
            [1.0, 0.0, gx, 0.0],
            [-1.0, 0.0, gx, 0.0],
            [0.0, 1.0, gy, 0.0],
            [0.0, -1.0, gy, 0.0],
        ];
        let env = &self.env;
        let vc = &self.vc;
        let fl = &vc.fl;
        let sv = |i: usize| ScreenVertex { x: vc.sx[i], y: vc.sy[i], z: vc.sz[i] };
        let cv = |i: usize| ClipVertex { x: vc.cx[i], y: vc.cy[i], w: vc.cw[i] };
        let mut st = RenderStats::default();

        // 3) Dreiecke.
        for (ti, t) in mesh.tris.iter().enumerate() {
            let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let (f0, f1, f2) = (fl[i0], fl[i1], fl[i2]);
            st.tris_in += 1;
            if f0 & f1 & f2 & REJECT_MASK != 0 {
                st.tris_culled += 1;
                continue;
            }
            if (f0 | f1 | f2) & CLIP_MASK == 0 {
                let (a, b, c) = (sv(i0), sv(i1), sv(i2));
                let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
                let front = area * front_sign < 0.0;
                if !front && !params.double_sided {
                    st.tris_culled += 1;
                    continue;
                }
                let col = shade(env, mesh, ti, lm, um, params, front);
                st.rows += raster(fb, a, b, c, params.mode, col);
                st.tris_drawn += 1;
            } else {
                st.tris_clipped += 1;
                let mut poly = [ClipVertex::default(); MAX_CLIP_VERTS];
                poly[0] = cv(i0);
                poly[1] = cv(i1);
                poly[2] = cv(i2);
                let n = clip_polygon(&mut poly, 3, &planes);
                if n < 3 {
                    continue;
                }
                let mut sv = [ScreenVertex::default(); MAX_CLIP_VERTS];
                let mut area = 0.0;
                for k in 0..n {
                    let iw = 1.0 / poly[k].w;
                    sv[k] = ScreenVertex { x: cx + poly[k].x * iw, y: cy + poly[k].y * iw, z: near * iw };
                }
                for k in 0..n {
                    let (a, b) = (sv[k], sv[(k + 1) % n]);
                    area += a.x * b.y - b.x * a.y;
                }
                let front = area * front_sign < 0.0;
                if !front && !params.double_sided {
                    st.tris_culled += 1;
                    continue;
                }
                let col = shade(env, mesh, ti, lm, um, params, front);
                for k in 1..n - 1 {
                    st.rows += raster(fb, sv[0], sv[k], sv[k + 1], params.mode, col);
                }
                st.tris_drawn += 1;
            }
        }
        self.stats.tris_in += st.tris_in;
        self.stats.tris_culled += st.tris_culled;
        self.stats.tris_clipped += st.tris_clipped;
        self.stats.tris_drawn += st.tris_drawn;
        self.stats.rows += st.rows;
        true
    }

    /// Zeichnet ein bildschirmausgerichtetes Quadrat mit Weltgröße `size`.
    pub fn draw_sprite(&mut self, fb: &mut Framebuffer, pos: Vec3, size: f32, color: Color) {
        let v = Vec4::from_vec3(pos, 1.0);
        let w = self.rw.dot(v);
        if w < self.near || w > self.far {
            return;
        }
        let iw = 1.0 / w;
        let sx = self.cx + self.rx.dot(v) * iw;
        let sy = self.cy + self.ry.dot(v) * iw;
        let r = (size * 0.5 * self.focal * iw).max(0.5);
        let z = (self.near * iw * DEPTH_SCALE) as i32;
        fill_rect_depth(
            fb,
            ceil_i32(sx - r - 0.5),
            ceil_i32(sy - r - 0.5),
            ceil_i32(sx + r - 0.5),
            ceil_i32(sy + r - 0.5),
            z,
            color.0,
        );
    }

    /// Zeichnet eine komplette [`DrawList`] in sinnvoller Reihenfolge.
    pub fn draw_list(&mut self, fb: &mut Framebuffer, store: &MeshStore, list: &DrawList) {
        #[cfg(target_arch = "x86_64")]
        if crate::cpu::has_v3() {
            // SAFETY: Die benötigten CPU-Funktionen wurden zur Laufzeit geprüft.
            return unsafe { self.draw_list_v3(fb, store, list) };
        }
        self.draw_list_impl(fb, store, list)
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt")]
    unsafe fn draw_list_v3(&mut self, fb: &mut Framebuffer, store: &MeshStore, list: &DrawList) {
        self.draw_list_impl(fb, store, list)
    }

    #[cfg_attr(target_arch = "x86_64", inline(always))]
    fn draw_list_impl(&mut self, fb: &mut Framebuffer, store: &MeshStore, list: &DrawList) {
        let mut order = core::mem::take(&mut self.order);
        order.clear();
        for (i, it) in list.items.iter().enumerate() {
            let c = it.model.transform_point(store.get(it.mesh).bounds.center);
            let depth = (c - self.cam_pos).dot(self.cam_fwd);
            if depth > it.params.cull_distance || depth < it.params.cull_near {
                continue; // Detailstufe: außerhalb des LOD-Bereichs
            }
            order.push((depth + it.params.sort_bias, i as u32));
        }
        order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        // Drei Durchgänge: opak (vorne→hinten), danach Partikel, Schatten
        // (vorne→hinten) und Durchscheinendes (hinten→vorne). Ein einziger
        // Aufrufort hält den eingebetteten Pipeline-Code klein.
        let n = order.len();
        for (pass, mode) in [DrawMode::Opaque, DrawMode::Shadow, DrawMode::Blend50].into_iter().enumerate() {
            if pass == 1 {
                for s in &list.sprites {
                    self.draw_sprite(fb, s.pos, s.size, s.color);
                }
            }
            for j in 0..n {
                let (_, i) = order[if pass == 2 { n - 1 - j } else { j }];
                let it = &list.items[i as usize];
                if it.params.mode == mode {
                    self.draw_mesh_impl(fb, store.get(it.mesh), &it.model, &it.params);
                }
            }
        }
        self.order = order;
    }

    /// Schließt den Frame ab: Himmel in leere Pixel, Nebel über alles
    /// Gezeichnete, Sonnenscheibe. Leert dabei gleich den Tiefenpuffer für den
    /// nächsten Frame (spart einen eigenen Durchlauf über den Puffer).
    pub fn finish(&mut self, fb: &mut Framebuffer) {
        #[cfg(target_arch = "x86_64")]
        if crate::cpu::has_v3() {
            // SAFETY: Die benötigten CPU-Funktionen wurden zur Laufzeit geprüft.
            return unsafe { self.finish_v3(fb) };
        }
        self.finish_impl(fb)
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt")]
    unsafe fn finish_v3(&mut self, fb: &mut Framebuffer) {
        self.finish_impl(fb)
    }

    #[cfg_attr(target_arch = "x86_64", inline(always))]
    fn finish_impl(&mut self, fb: &mut Framebuffer) {
        let fog_on = self.fog_on;
        let shift = self.fog_shift;
        let lut = &self.fog_lut;
        // Zeilen der Sonne behalten ihre Tiefe, bis die Sonne gezeichnet ist.
        let sun = if self.env.sun_disc { self.sun_geometry() } else { None };
        let (keep0, keep1) = sun.as_ref().map(|s| (s.y0 as usize, s.y1 as usize)).unwrap_or((0, 0));
        #[cfg(target_arch = "x86_64")]
        let avx2 = crate::cpu::has_avx2();
        #[cfg(target_arch = "x86_64")]
        let fogp = FogParams {
            k: self.near * DEPTH_SCALE,
            start: self.env.fog_start,
            inv_range: 1.0 / (self.env.fog_end - self.env.fog_start),
            z_start: (self.near * DEPTH_SCALE / self.env.fog_start.max(self.near)) as i32,
        };
        for y in 0..fb.height() {
            let sky = self.sky_rows[y];
            let clear = y < keep0 || y >= keep1;
            let (crow, zrow) = fb.rows_mut(y);
            #[cfg(target_arch = "x86_64")]
            if fog_on {
                if avx2 {
                    // SAFETY: AVX2 wurde zur Laufzeit erkannt (CPU + Betriebssystem).
                    unsafe {
                        if clear {
                            fog_row_avx2::<true>(crow, zrow, sky, &fogp);
                        } else {
                            fog_row_avx2::<false>(crow, zrow, sky, &fogp);
                        }
                    }
                } else if clear {
                    fog_row_sse2::<true>(crow, zrow, sky, &fogp);
                } else {
                    fog_row_sse2::<false>(crow, zrow, sky, &fogp);
                }
                continue;
            }
            if fog_on {
                // Verzweigungen sind hier gut vorhersagbar (zusammenhängende
                // Flächen) und sparen die Mischung für nahe Pixel ganz.
                for (c, &z) in crow.iter_mut().zip(zrow.iter()) {
                    if z <= 0 {
                        *c = sky;
                        continue;
                    }
                    let idx = (z as u32 >> shift) as usize;
                    if idx < FOG_LUT_SIZE - 1 {
                        let f = lut[idx] as u32;
                        if f != 0 {
                            *c = blend256(*c, sky, f);
                        }
                    }
                }
            } else {
                for (c, &z) in crow.iter_mut().zip(zrow.iter()) {
                    if z <= 0 {
                        *c = sky;
                    }
                }
            }
            if clear {
                zrow.fill(0);
            }
        }
        if let Some(s) = sun {
            self.draw_sun(fb, &s);
            for y in keep0..keep1 {
                fb.rows_mut(y).1.fill(0);
            }
        }
        fb.mark_depth_clean();
    }

    /// Lage der Sonnenscheibe auf dem Bildschirm (falls sichtbar).
    fn sun_geometry(&self) -> Option<SunGeom> {
        let d = -self.env.sun_dir;
        let w = self.rw.xyz().dot(d);
        if w <= 0.05 {
            return None;
        }
        let sx = self.cx + self.rx.xyz().dot(d) / w;
        let sy = self.cy + self.ry.xyz().dot(d) / w;
        let r = self.h as f32 * 0.035 + 1.0;
        let halo = r * 3.5;
        Some(SunGeom {
            sx,
            sy,
            r,
            halo,
            x0: ceil_i32(sx - halo).max(0),
            x1: ceil_i32(sx + halo).min(self.w as i32),
            y0: ceil_i32(sy - halo).clamp(0, self.h as i32),
            y1: ceil_i32(sy + halo).clamp(0, self.h as i32),
        })
    }

    fn draw_sun(&self, fb: &mut Framebuffer, g: &SunGeom) {
        let SunGeom { sx, sy, r, halo, x0, x1, y0, y1 } = *g;
        let sun = self.env.sun_disc_color.0;
        for y in y0..y1 {
            let (crow, zrow) = fb.rows_mut(y as usize);
            let dy = y as f32 + 0.5 - sy;
            for x in x0..x1 {
                if zrow[x as usize] != 0 {
                    continue;
                }
                let dx = x as f32 + 0.5 - sx;
                let d2 = dx * dx + dy * dy;
                let c = &mut crow[x as usize];
                if d2 < r * r {
                    *c = sun;
                } else if d2 < halo * halo {
                    let t = 1.0 - (crate::math::sqrt(d2) - r) / (halo - r);
                    *c = blend256(*c, sun, (t * t * 150.0) as u32);
                }
            }
        }
    }
}

/// Konstanten der Vertex-Transformation eines Meshes.
struct Xform {
    mx: Vec4,
    my: Vec4,
    mw: Vec4,
    cx: f32,
    cy: f32,
    near: f32,
    gx: f32,
    gy: f32,
}

/// Transformierte Vertices eines Meshes als Struktur aus Arrays (SoA):
/// Clip-Raum (`cx, cy, cw`), Bildschirm (`sx, sy, sz`) und Outcodes.
#[derive(Default)]
struct VertexCache {
    cx: Vec<f32>,
    cy: Vec<f32>,
    cw: Vec<f32>,
    sx: Vec<f32>,
    sy: Vec<f32>,
    sz: Vec<f32>,
    fl: Vec<u8>,
}

impl VertexCache {
    fn reserve(&mut self, n: usize) {
        for v in [&mut self.cx, &mut self.cy, &mut self.cw, &mut self.sx, &mut self.sy, &mut self.sz] {
            v.reserve(n.saturating_sub(v.len()));
        }
        self.fl.reserve(n.saturating_sub(self.fl.len()));
    }

    #[cfg_attr(target_arch = "x86_64", inline(always))]
    fn transform(&mut self, pos: &[Vec3], k: &Xform) {
        let n = pos.len();
        for v in [&mut self.cx, &mut self.cy, &mut self.cw, &mut self.sx, &mut self.sy, &mut self.sz] {
            v.resize(n, 0.0);
        }
        self.fl.resize(n, 0);
        #[cfg(target_arch = "x86_64")]
        let start = self.transform_sse2(pos, k);
        #[cfg(not(target_arch = "x86_64"))]
        let start = 0;
        for i in start..n {
            self.transform_one(i, pos[i], k);
        }
    }

    /// Skalare Referenz – exakt dieselbe Rechenreihenfolge wie der SIMD-Pfad.
    #[inline(always)]
    fn transform_one(&mut self, i: usize, p: Vec3, k: &Xform) {
        let (mx, my, mw) = (k.mx, k.my, k.mw);
        let (cx, cy, near, gx, gy) = (k.cx, k.cy, k.near, k.gx, k.gy);
        let x = mx.x * p.x + mx.y * p.y + mx.z * p.z + mx.w;
        let y = my.x * p.x + my.y * p.y + my.z * p.z + my.w;
        let w = mw.x * p.x + mw.y * p.y + mw.z * p.z + mw.w;
        let mut f = 0u8;
        if w < near {
            f |= OUT_NEAR;
        }
        if x < -cx * w {
            f |= OUT_LEFT;
        }
        if x > cx * w {
            f |= OUT_RIGHT;
        }
        if y < -cy * w {
            f |= OUT_TOP;
        }
        if y > cy * w {
            f |= OUT_BOTTOM;
        }
        if x < -gx * w || x > gx * w || y < -gy * w || y > gy * w {
            f |= OUT_GUARD;
        }
        // Bildschirmwerte werden nur für Vertices vor der Near-Ebene gelesen.
        let iw = 1.0 / w;
        self.sx[i] = cx + x * iw;
        self.sy[i] = cy + y * iw;
        self.sz[i] = near * iw;
        self.cx[i] = x;
        self.cy[i] = y;
        self.cw[i] = w;
        self.fl[i] = f;
    }

    /// 4 Vertices pro Schritt mit SSE2 (bitgleich zur skalaren Variante:
    /// gleiche Operationen in gleicher Reihenfolge, kein FMA). Gibt die Zahl
    /// erledigter Vertices zurück.
    #[cfg(target_arch = "x86_64")]
    #[inline(always)]
    fn transform_sse2(&mut self, pos: &[Vec3], k: &Xform) -> usize {
        use core::arch::x86_64::*;
        let groups = pos.len() / 4;
        // SAFETY: SSE2 gehört zur x86_64-Basis. `Vec3` ist `repr(C)` aus drei
        // f32 → 4 Vertices = 12 zusammenhängende f32. Alle Arrays haben Länge
        // `pos.len()`, geschrieben wird nur in `[4g, 4g+4)` für g < groups.
        unsafe {
            let s = |v: f32| _mm_set1_ps(v);
            let (mxx, mxy, mxz, mxw) = (s(k.mx.x), s(k.mx.y), s(k.mx.z), s(k.mx.w));
            let (myx, myy, myz, myw) = (s(k.my.x), s(k.my.y), s(k.my.z), s(k.my.w));
            let (mwx, mwy, mwz, mww) = (s(k.mw.x), s(k.mw.y), s(k.mw.z), s(k.mw.w));
            let (cxv, cyv, nearv) = (s(k.cx), s(k.cy), s(k.near));
            let (ncx, ncy, ngx, ngy, gxv, gyv) = (s(-k.cx), s(-k.cy), s(-k.gx), s(-k.gy), s(k.gx), s(k.gy));
            let one = s(1.0);
            let bit = |b: u8| _mm_set1_epi32(b as i32);
            let (b_near, b_left, b_right, b_top, b_bottom, b_guard) =
                (bit(OUT_NEAR), bit(OUT_LEFT), bit(OUT_RIGHT), bit(OUT_TOP), bit(OUT_BOTTOM), bit(OUT_GUARD));
            let src = pos.as_ptr() as *const f32;
            for g in 0..groups {
                let i = g * 4;
                let a = _mm_loadu_ps(src.add(i * 3)); // x0 y0 z0 x1
                let b = _mm_loadu_ps(src.add(i * 3 + 4)); // y1 z1 x2 y2
                let c = _mm_loadu_ps(src.add(i * 3 + 8)); // z2 x3 y3 z3
                let px = _mm_shuffle_ps::<136>(_mm_shuffle_ps::<48>(a, a), _mm_shuffle_ps::<18>(b, c));
                let py = _mm_shuffle_ps::<136>(_mm_shuffle_ps::<1>(a, b), _mm_shuffle_ps::<35>(b, c));
                let pz = _mm_shuffle_ps::<136>(_mm_shuffle_ps::<18>(a, b), _mm_shuffle_ps::<48>(c, c));
                let row = |r0, r1, r2, r3| {
                    _mm_add_ps(_mm_add_ps(_mm_add_ps(_mm_mul_ps(r0, px), _mm_mul_ps(r1, py)), _mm_mul_ps(r2, pz)), r3)
                };
                let x = row(mxx, mxy, mxz, mxw);
                let y = row(myx, myy, myz, myw);
                let w = row(mwx, mwy, mwz, mww);
                let m = |cmp: __m128, b: __m128i| _mm_and_si128(_mm_castps_si128(cmp), b);
                let guard = _mm_or_ps(
                    _mm_or_ps(_mm_cmplt_ps(x, _mm_mul_ps(ngx, w)), _mm_cmpgt_ps(x, _mm_mul_ps(gxv, w))),
                    _mm_or_ps(_mm_cmplt_ps(y, _mm_mul_ps(ngy, w)), _mm_cmpgt_ps(y, _mm_mul_ps(gyv, w))),
                );
                let f = _mm_or_si128(
                    _mm_or_si128(
                        _mm_or_si128(m(_mm_cmplt_ps(w, nearv), b_near), m(_mm_cmplt_ps(x, _mm_mul_ps(ncx, w)), b_left)),
                        _mm_or_si128(
                            m(_mm_cmpgt_ps(x, _mm_mul_ps(cxv, w)), b_right),
                            m(_mm_cmplt_ps(y, _mm_mul_ps(ncy, w)), b_top),
                        ),
                    ),
                    _mm_or_si128(m(_mm_cmpgt_ps(y, _mm_mul_ps(cyv, w)), b_bottom), m(guard, b_guard)),
                );
                let f8 = _mm_packus_epi16(_mm_packs_epi32(f, f), _mm_setzero_si128());
                core::ptr::write_unaligned(self.fl.as_mut_ptr().add(i) as *mut i32, _mm_cvtsi128_si32(f8));
                let iw = _mm_div_ps(one, w);
                _mm_storeu_ps(self.sx.as_mut_ptr().add(i), _mm_add_ps(cxv, _mm_mul_ps(x, iw)));
                _mm_storeu_ps(self.sy.as_mut_ptr().add(i), _mm_add_ps(cyv, _mm_mul_ps(y, iw)));
                _mm_storeu_ps(self.sz.as_mut_ptr().add(i), _mm_mul_ps(nearv, iw));
                _mm_storeu_ps(self.cx.as_mut_ptr().add(i), x);
                _mm_storeu_ps(self.cy.as_mut_ptr().add(i), y);
                _mm_storeu_ps(self.cw.as_mut_ptr().add(i), w);
            }
        }
        groups * 4
    }
}

#[derive(Clone, Copy)]
struct SunGeom {
    sx: f32,
    sy: f32,
    r: f32,
    halo: f32,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
}

#[cfg(target_arch = "x86_64")]
struct FogParams {
    /// `w = k / z`
    k: f32,
    start: f32,
    inv_range: f32,
    /// Tiefenwerte darüber (näher als `fog_start`) bleiben unverändert.
    z_start: i32,
}

/// Himmel + Nebel für eine Zeile mit SSE2, 4 Pixel pro Schritt.
/// Schnellpfade: komplett nahe Blöcke werden übersprungen, reine
/// Himmelsblöcke direkt geschrieben; sonst wird der Nebelanteil aus
/// `w = k/z` mit Smoothstep berechnet und kanalweise in 16 Bit gemischt.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn fog_row_sse2<const CLEAR: bool>(crow: &mut [u32], zrow: &mut [i32], sky: u32, p: &FogParams) {
    use core::arch::x86_64::*;
    let n = crow.len().min(zrow.len());
    let blocks = n / 4;
    // SAFETY: SSE2 gehört zur x86_64-Basis; Zugriffe nur in [0, blocks*4).
    unsafe {
        let zero = _mm_setzero_si128();
        let skyv = _mm_set1_epi32(sky as i32);
        let sky16 = _mm_unpacklo_epi8(skyv, zero);
        let near_lim = _mm_set1_epi32(p.z_start);
        let k = _mm_set1_ps(p.k);
        let start = _mm_set1_ps(p.start);
        let inv_range = _mm_set1_ps(p.inv_range);
        let one = _mm_set1_ps(1.0);
        let three = _mm_set1_ps(3.0);
        let two = _mm_set1_ps(2.0);
        let s256 = _mm_set1_ps(256.0);
        let c256 = _mm_set1_epi16(256);
        let zp = zrow.as_mut_ptr() as *mut __m128i;
        let cp = crow.as_mut_ptr() as *mut __m128i;
        for b in 0..blocks {
            let z = _mm_loadu_si128(zp.add(b));
            if CLEAR {
                // Tiefe gleich für den nächsten Frame leeren (Cache-Zeile ist schon geladen).
                _mm_storeu_si128(zp.add(b), zero);
            }
            // Alle 4 näher als der Nebelbeginn → nichts zu tun.
            if _mm_movemask_epi8(_mm_cmpgt_epi32(z, near_lim)) == 0xFFFF {
                continue;
            }
            // Alle 4 Himmel → direkt schreiben.
            if _mm_movemask_epi8(_mm_cmpeq_epi32(z, zero)) == 0xFFFF {
                _mm_storeu_si128(cp.add(b), skyv);
                continue;
            }
            // Nebelanteil f = smoothstep((k/z - start) * inv_range) * 256
            // (z = 0 → k/0 = ∞ → f = 256 → reiner Himmel).
            let w = _mm_mul_ps(k, _mm_rcp_ps(_mm_cvtepi32_ps(z)));
            let t = _mm_min_ps(one, _mm_max_ps(_mm_setzero_ps(), _mm_mul_ps(_mm_sub_ps(w, start), inv_range)));
            let s = _mm_mul_ps(_mm_mul_ps(t, t), _mm_sub_ps(three, _mm_mul_ps(two, t)));
            let f = _mm_cvttps_epi32(_mm_mul_ps(s, s256));
            // Gewichte auf 16-Bit-Kanäle verteilen: (f0 f0 f0 f0 f1 f1 f1 f1), (f2.. f3..)
            let f16 = _mm_packs_epi32(f, f);
            let fl = _mm_unpacklo_epi16(f16, f16);
            let fa = _mm_unpacklo_epi32(fl, fl);
            let fb = _mm_unpackhi_epi32(fl, fl);
            let px = _mm_loadu_si128(cp.add(b));
            let lo = _mm_unpacklo_epi8(px, zero);
            let hi = _mm_unpackhi_epi8(px, zero);
            let lo = _mm_srli_epi16(
                _mm_add_epi16(_mm_mullo_epi16(lo, _mm_sub_epi16(c256, fa)), _mm_mullo_epi16(sky16, fa)),
                8,
            );
            let hi = _mm_srli_epi16(
                _mm_add_epi16(_mm_mullo_epi16(hi, _mm_sub_epi16(c256, fb)), _mm_mullo_epi16(sky16, fb)),
                8,
            );
            _mm_storeu_si128(cp.add(b), _mm_packus_epi16(lo, hi));
        }
    }
    // Rest (0–3 Pixel) skalar.
    for i in blocks * 4..n {
        let z = zrow[i];
        if CLEAR {
            zrow[i] = 0;
        }
        if z <= 0 {
            crow[i] = sky;
        } else if z <= p.z_start {
            let w = p.k / z as f32;
            let t = ((w - p.start) * p.inv_range).clamp(0.0, 1.0);
            crow[i] = blend256(crow[i], sky, (t * t * (3.0 - 2.0 * t) * 256.0) as u32);
        }
    }
}

/// AVX2-Variante von [`fog_row_sse2`]: 8 Pixel pro Schritt, pro 128-Bit-Hälfte
/// exakt dieselben Operationen; Restpixel laufen über den SSE2-Pfad.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fog_row_avx2<const CLEAR: bool>(crow: &mut [u32], zrow: &mut [i32], sky: u32, p: &FogParams) {
    use core::arch::x86_64::*;
    let n = crow.len().min(zrow.len());
    let blocks = n / 8;
    // SAFETY: Aufrufer garantiert AVX2; Zugriffe nur in [0, blocks*8).
    unsafe {
        let zero = _mm256_setzero_si256();
        let skyv = _mm256_set1_epi32(sky as i32);
        let sky16 = _mm256_unpacklo_epi8(skyv, zero);
        let near_lim = _mm256_set1_epi32(p.z_start);
        let k = _mm256_set1_ps(p.k);
        let start = _mm256_set1_ps(p.start);
        let inv_range = _mm256_set1_ps(p.inv_range);
        let one = _mm256_set1_ps(1.0);
        let three = _mm256_set1_ps(3.0);
        let two = _mm256_set1_ps(2.0);
        let s256 = _mm256_set1_ps(256.0);
        let c256 = _mm256_set1_epi16(256);
        let zp = zrow.as_mut_ptr() as *mut __m256i;
        let cp = crow.as_mut_ptr() as *mut __m256i;
        for b in 0..blocks {
            let z = _mm256_loadu_si256(zp.add(b));
            if CLEAR {
                _mm256_storeu_si256(zp.add(b), zero);
            }
            if _mm256_movemask_epi8(_mm256_cmpgt_epi32(z, near_lim)) == -1 {
                continue;
            }
            if _mm256_movemask_epi8(_mm256_cmpeq_epi32(z, zero)) == -1 {
                _mm256_storeu_si256(cp.add(b), skyv);
                continue;
            }
            let w = _mm256_mul_ps(k, _mm256_rcp_ps(_mm256_cvtepi32_ps(z)));
            let t = _mm256_min_ps(
                one,
                _mm256_max_ps(_mm256_setzero_ps(), _mm256_mul_ps(_mm256_sub_ps(w, start), inv_range)),
            );
            let s = _mm256_mul_ps(_mm256_mul_ps(t, t), _mm256_sub_ps(three, _mm256_mul_ps(two, t)));
            let f = _mm256_cvttps_epi32(_mm256_mul_ps(s, s256));
            let f16 = _mm256_packs_epi32(f, f);
            let fl = _mm256_unpacklo_epi16(f16, f16);
            let fa = _mm256_unpacklo_epi32(fl, fl);
            let fb = _mm256_unpackhi_epi32(fl, fl);
            let px = _mm256_loadu_si256(cp.add(b));
            let lo = _mm256_unpacklo_epi8(px, zero);
            let hi = _mm256_unpackhi_epi8(px, zero);
            let lo = _mm256_srli_epi16::<8>(_mm256_add_epi16(
                _mm256_mullo_epi16(lo, _mm256_sub_epi16(c256, fa)),
                _mm256_mullo_epi16(sky16, fa),
            ));
            let hi = _mm256_srli_epi16::<8>(_mm256_add_epi16(
                _mm256_mullo_epi16(hi, _mm256_sub_epi16(c256, fb)),
                _mm256_mullo_epi16(sky16, fb),
            ));
            _mm256_storeu_si256(cp.add(b), _mm256_packus_epi16(lo, hi));
        }
    }
    let done = blocks * 8;
    fog_row_sse2::<CLEAR>(&mut crow[done..n], &mut zrow[done..n], sky, p);
}

/// Brennt die Beleuchtung in die Flächenfarben eines Meshes ein, das mit
/// `model` (nur Rotation zählt) platziert wird. Danach mit
/// [`DrawParams::unlit`] zeichnen: spart die Lichtberechnung pro Fläche und
/// Frame – ideal für statische Level-Geometrie.
pub fn bake_lighting(mesh: &mut Mesh, model: &Mat4, env: &Environment) {
    let to_sun = -env.sun_dir;
    for (c, &n) in mesh.colors.iter_mut().zip(mesh.normals.iter()) {
        let nw = model.transform_vector(n).normalize_or(n);
        let ndl = nw.dot(to_sun).max(0.0);
        let hemi = nw.y * 0.5 + 0.5;
        let mut l = [0.0f32; 3];
        for k in 0..3 {
            let amb = env.ambient_ground[k] + (env.ambient_sky[k] - env.ambient_ground[k]) * hemi;
            l[k] = amb + env.sun_color[k] * ndl;
        }
        *c = c.mul_rgb(l);
    }
}

/// Flächenfarbe inkl. Hemisphären- und Sonnenlicht.
#[inline(always)]
fn shade(env: &Environment, mesh: &Mesh, ti: usize, lm: Vec3, um: Vec3, p: &DrawParams, front: bool) -> u32 {
    let base = p.color_override.unwrap_or(mesh.colors[ti]);
    if p.unlit {
        return if p.tint == [1.0, 1.0, 1.0] { base.0 } else { base.mul_rgb(p.tint).0 };
    }
    let n = if front { mesh.normals[ti] } else { -mesh.normals[ti] };
    let ndl = n.dot(lm).max(0.0);
    let hemi = n.dot(um) * 0.5 + 0.5;
    let mut l = [0.0f32; 3];
    for k in 0..3 {
        let amb = env.ambient_ground[k] + (env.ambient_sky[k] - env.ambient_ground[k]) * hemi;
        l[k] = (amb + env.sun_color[k] * ndl) * p.tint[k];
    }
    base.mul_rgb(l).0
}

/// Oberflächenmaterial für [`light_faces`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Material {
    /// Glanzlicht der Sonne (0 = matt, 1 = voll), z. B. Gelatine, Glasur.
    pub gloss: f32,
    /// Durchscheinen: von hinten beleuchtete Flächen bekommen diesen Anteil
    /// Sonnenlicht (Licht dringt durch, wie bei Pudding oder Eis).
    pub translucency: f32,
}

/// Beleuchtet alle Flächen eines Meshes vorab – zusätzlich mit Glanzlicht
/// (Blinn-Phong) und Durchscheinen – und schreibt die Farben nach `out`.
/// Danach das Mesh mit [`DrawParams::unlit`] zeichnen. Gedacht für einzelne,
/// sich verformende „Helden“-Objekte: so bleibt die Dreiecksschleife des
/// Renderers für alle anderen unverändert schnell. `eye` = Kameraposition.
pub fn light_faces(
    out: &mut [Color],
    base: &[Color],
    normals: &[Vec3],
    model: &Mat4,
    env: &Environment,
    eye: Vec3,
    m: &Material,
) {
    // Licht-, Himmels- und Halbvektor in den Modellraum holen (wie im Renderer).
    let (c0, c1, c2) = (model.cols[0].xyz(), model.cols[1].xyz(), model.cols[2].xyz());
    let (l0, l1, l2) = (c0.length().max(1e-12), c1.length().max(1e-12), c2.length().max(1e-12));
    let local = |v: Vec3| Vec3::new(c0.dot(v) / l0, c1.dot(v) / l1, c2.dot(v) / l2);
    let to_sun = -env.sun_dir;
    let to_eye = (eye - model.cols[3].xyz()).normalize_or(Vec3::Y);
    let lm = local(to_sun);
    let um = local(Vec3::Y);
    let hm = local((to_sun + to_eye).normalize_or(Vec3::Y)).normalize_or(Vec3::Y);
    for ((o, &b), &n) in out.iter_mut().zip(base).zip(normals) {
        let nl = n.dot(lm);
        // Durchscheinen: von hinten beleuchtete Flächen leuchten gedämpft mit.
        let ndl = nl.max(0.0) + m.translucency * (-nl).max(0.0);
        let hemi = n.dot(um) * 0.5 + 0.5;
        let mut l = [0.0f32; 3];
        for k in 0..3 {
            let amb = env.ambient_ground[k] + (env.ambient_sky[k] - env.ambient_ground[k]) * hemi;
            l[k] = amb + env.sun_color[k] * ndl;
        }
        let c = b.mul_rgb(l);
        // Glanz mit Exponent 16 Richtung Sonnenscheibenfarbe.
        let mut s = n.dot(hm).max(0.0);
        s *= s;
        s *= s;
        s *= s;
        s *= s;
        let spec = (s * m.gloss).min(1.0);
        *o = if spec > 0.004 { c.lerp(env.sun_disc_color, spec) } else { c };
    }
}

#[inline(always)]
fn raster(fb: &mut Framebuffer, a: ScreenVertex, b: ScreenVertex, c: ScreenVertex, mode: DrawMode, col: u32) -> u32 {
    match mode {
        DrawMode::Opaque => fill_triangle(fb, a, b, c, &Solid(col)),
        DrawMode::Blend50 => fill_triangle(fb, a, b, c, &Blend50(col)),
        DrawMode::Shadow => fill_triangle(fb, a, b, c, &Shadow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes;

    fn setup() -> (Framebuffer, Renderer, Camera, Environment) {
        let fb = Framebuffer::new(160, 90);
        let mut cam = Camera::default();
        cam.look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
        let env = Environment { fog: false, sun_disc: false, ..Environment::default() };
        (fb, Renderer::new(), cam, env)
    }

    #[test]
    fn cube_is_drawn_in_the_center_and_sky_elsewhere() {
        let (mut fb, mut r, cam, env) = setup();
        let mesh = shapes::box_centered(Vec3::ONE, Color::hex(0xFF0000));
        r.begin(&mut fb, &cam, &env);
        assert!(r.draw_mesh(&mut fb, &mesh, &Mat4::IDENTITY, &DrawParams::unlit()));
        r.finish(&mut fb);
        assert_eq!(fb.pixel(80, 45), 0xFF0000, "Würfel fehlt in der Bildmitte");
        assert_ne!(fb.pixel(2, 2), 0xFF0000, "Ecke sollte Himmel sein");
        // Genau eine Seite zeigt zur Kamera → 2 von 12 Dreiecken gezeichnet.
        assert_eq!(r.stats.tris_drawn, 2);
    }

    #[test]
    fn objects_behind_the_camera_are_culled() {
        let (mut fb, mut r, cam, env) = setup();
        let mesh = shapes::box_centered(Vec3::ONE, Color::WHITE);
        r.begin(&mut fb, &cam, &env);
        assert!(!r.draw_mesh(&mut fb, &mesh, &Mat4::translation(Vec3::new(0.0, 0.0, 20.0)), &DrawParams::OPAQUE));
        assert_eq!(r.stats.objects_culled, 1);
    }

    #[test]
    fn near_plane_clipping_keeps_ground_visible() {
        // Großer Boden, der hinter der Kamera beginnt → muss geclippt werden.
        let (mut fb, mut r, mut cam, env) = setup();
        cam.look_at(Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, -5.0));
        let ground =
            shapes::cuboid(Vec3::new(-50.0, -1.0, -50.0), Vec3::new(50.0, 0.0, 50.0), [Color::hex(0x00FF00); 6]);
        r.begin(&mut fb, &cam, &env);
        r.draw_mesh(&mut fb, &ground, &Mat4::IDENTITY, &DrawParams::unlit());
        r.finish(&mut fb);
        assert!(r.stats.tris_clipped > 0);
        assert_eq!(fb.pixel(80, 85), 0x00FF00, "Boden unten im Bild fehlt");
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn sse2_fog_matches_scalar_formula() {
        let near = 0.1;
        let p = FogParams {
            k: near * DEPTH_SCALE,
            start: 20.0,
            inv_range: 1.0 / 60.0,
            z_start: (near * DEPTH_SCALE / 20.0) as i32,
        };
        let sky = 0x80C0FF;
        let mut zrow = std::vec::Vec::new();
        for i in 0..203 {
            zrow.push(if i % 7 == 0 { 0 } else { (near * DEPTH_SCALE / (5.0 + i as f32 * 0.6)) as i32 });
        }
        let mut colors = std::vec![0x203040u32; zrow.len()];
        let mut zcopy = zrow.clone();
        fog_row_sse2::<true>(&mut colors, &mut zcopy, sky, &p);
        assert!(zcopy.iter().all(|&z| z == 0), "Tiefe muss mitgelöscht werden");
        for (i, (&c, &z)) in colors.iter().zip(&zrow).enumerate() {
            let expect = if z == 0 {
                sky
            } else {
                let w = p.k / z as f32;
                let t = ((w - p.start) * p.inv_range).clamp(0.0, 1.0);
                blend256(0x203040, sky, (t * t * (3.0 - 2.0 * t) * 256.0) as u32)
            };
            let d = |s: u32| ((c >> s) & 0xFF) as i32 - ((expect >> s) & 0xFF) as i32;
            assert!(d(0).abs() <= 2 && d(8).abs() <= 2 && d(16).abs() <= 2, "Pixel {i}: {c:06X} vs {expect:06X}");
        }
    }
}
