//! Laufzeit-Welt: aus der Level-Beschreibung erzeugte Objekte, ihre
//! Bewegung pro Simulationsschritt und ihr Zeichnen.

use crate::assets::{self, Assets};
use crate::level::{Def, LevelDef, RouteStep};
use spatz::anim::Spring;
use spatz::math::{TAU, Vec2, atan2, cos, sin};
use spatz::physics::{Collider, CollisionWorld, Shape};
use spatz::prelude::*;

pub const TAG_STATIC: u32 = 0;
pub const TAG_MALLOW: u32 = 1;
pub const TAG_FRIDGE: u32 = 2;
pub const TAG_MOVING: u32 = 3;

#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub delta: Vec3,
    pub period: f32,
    pub phase: f32,
}

impl Motion {
    /// Bewegungsanteil 0..1 (weich hin und her).
    pub fn fraction(&self, t: f32) -> f32 {
        0.5 - 0.5 * cos(TAU * (t / self.period + self.phase))
    }
}

/// Ab dieser Blicktiefe werden Kleinteile (Streusel, Tropfen, Deko) weggelassen.
pub const DETAIL_LOD: f32 = 34.0;
pub const SMALL_LOD: f32 = 75.0;
/// Ab hier werden Kirschen als einfache Oktaeder gezeichnet.
pub const CHERRY_LOD: f32 = 20.0;

pub struct Platform {
    pub mesh: MeshId,
    /// Optionales Detail-Mesh (nur aus der Nähe gezeichnet).
    pub detail: Option<MeshId>,
    pub collider: usize,
    pub origin: Vec3,
    pub motion: Option<Motion>,
    pub offset: Vec3,
    pub prev_offset: Vec3,
    /// Nachfedern nach einer Landung (rein optisch): < 0 = gestaucht.
    pub wobble: Spring,
    /// Weichheit (Kuchen weich, Kekse/Schokolade härter, große Teile träger).
    pub softness: f32,
}

impl Platform {
    /// Eigenfrequenz des Nachfederns (härtere Plattformen schwingen schneller).
    fn wobble_omega(&self) -> f32 {
        (2.6 + 2.0 * (1.0 - self.softness)) * TAU
    }

    /// Lässt die Plattform nach einer Landung nachfedern.
    pub fn land(&mut self, speed: f32) {
        self.wobble.kick(-(speed * 0.085 * self.softness).min(1.8));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Cherry,
    Ice,
}

pub struct Item {
    pub kind: ItemKind,
    pub pos: Vec3,
    pub taken: bool,
    /// Zeit seit dem Einsammeln (Animation).
    pub pop: f32,
    pub phase: f32,
}

pub struct Donut {
    pub start: Vec3,
    pub delta: Vec2,
    pub speed: f32,
    pub s: f32,
    pub dir: f32,
    pub hop: f32,
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub facing: f32,
    pub prev_facing: f32,
    pub alive: bool,
    /// > 0: wird gerade plattgemacht (Sekunden seit dem Treffer).
    pub squash: f32,
    /// Teig-Wabbeln nach jedem Hüpfer (rein optisch).
    pub jiggle: Spring,
}

pub struct Mallow {
    pub pos: Vec3,
    pub collider: usize,
    pub squash: f32,
    pub squash_vel: f32,
    /// Seitliches Ovalschwingen nach dem Abprallen (rein optisch).
    pub oval: Spring,
}

pub struct Spinner {
    pub pos: Vec3,
    pub length: f32,
    pub speed: f32,
    pub angle: f32,
    pub prev_angle: f32,
    pub bar: MeshId,
}

impl Spinner {
    /// Höhe des Balkens über dem Boden.
    pub const BAR_Y: f32 = 0.5;
    pub const BAR_R: f32 = 0.17;

    /// Endpunkte des Balkens (XZ).
    pub fn segment(&self) -> (Vec2, Vec2) {
        let d = Vec2::new(cos(self.angle), -sin(self.angle)) * (self.length * 0.5);
        let c = self.pos.xz();
        (c - d, c + d)
    }
}

pub struct Checkpoint {
    pub pos: Vec3,
    pub active: bool,
    pub wobble: f32,
}

pub struct Fridge {
    pub pos: Vec3,
    pub yaw: f32,
    pub door: f32,
    pub prev_door: f32,
}

impl Fridge {
    /// Punkt vor der Tür, an dem das Ziel ausgelöst wird.
    pub fn goal_point(&self) -> Vec3 {
        self.pos + Quat::from_rotation_y(self.yaw).rotate(Vec3::new(0.0, 0.0, 1.6))
    }
}

pub struct Deco {
    pub mesh: MeshId,
    pub model: Mat4,
    /// Mesh gehört nur dieser Deko → Licht kann eingebrannt werden.
    pub unique: bool,
    pub baked: bool,
}

pub struct Cloud {
    pub base: Vec3,
    pub scale: f32,
    pub phase: f32,
}

pub struct Hint {
    pub pos: Vec3,
    pub r: f32,
    pub text: String,
}

pub struct World {
    pub collision: CollisionWorld,
    pub platforms: Vec<Platform>,
    /// Indizes der bewegten Plattformen (in Level-Reihenfolge).
    pub moving: Vec<usize>,
    pub items: Vec<Item>,
    pub donuts: Vec<Donut>,
    pub mallows: Vec<Mallow>,
    pub spinners: Vec<Spinner>,
    pub checkpoints: Vec<Checkpoint>,
    pub fridge: Option<Fridge>,
    pub decos: Vec<Deco>,
    pub clouds: Vec<Cloud>,
    pub hints: Vec<Hint>,
    pub spawn: Vec3,
    pub spawn_yaw: f32,
    pub sea_y: f32,
    pub route: Vec<RouteStep>,
    pub time: f32,
}

impl World {
    pub fn build(def: &LevelDef, store: &mut MeshStore, a: &Assets) -> World {
        let mut w = World {
            collision: CollisionWorld::new(),
            platforms: Vec::new(),
            moving: Vec::new(),
            items: Vec::new(),
            donuts: Vec::new(),
            mallows: Vec::new(),
            spinners: Vec::new(),
            checkpoints: Vec::new(),
            fridge: None,
            decos: Vec::new(),
            clouds: Vec::new(),
            hints: Vec::new(),
            spawn: Vec3::ZERO,
            spawn_yaw: 0.0,
            sea_y: -8.0,
            route: Vec::new(),
            time: 0.0,
        };
        let box_shape = |pos: Vec3, size: Vec3| {
            Shape::Box(Aabb::new(
                Vec3::new(pos.x - size.x * 0.5, pos.y - size.z, pos.z - size.y * 0.5),
                Vec3::new(pos.x + size.x * 0.5, pos.y, pos.z + size.y * 0.5),
            ))
        };
        let mut seed = 1u32;
        for d in &def.defs {
            seed = seed.wrapping_add(7919);
            match d {
                Def::Spawn { pos, yaw } => {
                    w.spawn = *pos;
                    w.spawn_yaw = *yaw;
                }
                Def::Sea { y } => w.sea_y = *y,
                Def::Cake { pos, size, style } => {
                    let (base, detail) = assets::cake(*size, *style, seed);
                    let (mesh, detail) = (store.add(base), store.add(detail));
                    w.add_platform(mesh, Some(detail), *pos, box_shape(*pos, *size), None, 1.0);
                }
                Def::Cookie { pos, r, h } => {
                    let (base, chips) = assets::cookie(*r, *h, seed);
                    let (mesh, chips) = (store.add(base), store.add(chips));
                    let shape = Shape::Cylinder { center: pos.xz(), radius: *r, y_min: pos.y - h, y_max: pos.y };
                    w.add_platform(mesh, Some(chips), *pos, shape, None, 0.5);
                }
                Def::Choco { pos, size } => {
                    let mesh = store.add(assets::choco_bar(*size));
                    w.add_platform(mesh, None, *pos, box_shape(*pos, *size), None, 0.35);
                }
                Def::Wafer { pos, size, delta, period, phase } => {
                    let mesh = store.add(assets::wafer(*size));
                    let m = Motion { delta: *delta, period: *period, phase: *phase };
                    w.add_platform(mesh, None, *pos, box_shape(*pos, *size), Some(m), 0.55);
                }
                Def::Cherry(p) => w.items.push(Item {
                    kind: ItemKind::Cherry,
                    pos: *p,
                    taken: false,
                    pop: 0.0,
                    phase: p.x * 0.7 + p.z * 0.3,
                }),
                Def::Ice(p) => {
                    w.items.push(Item { kind: ItemKind::Ice, pos: *p, taken: false, pop: 0.0, phase: p.x + p.z })
                }
                Def::Donut { pos, delta, speed } => w.donuts.push(Donut {
                    start: *pos,
                    delta: *delta,
                    speed: *speed,
                    s: 0.0,
                    dir: 1.0,
                    hop: 0.0,
                    pos: *pos,
                    prev_pos: *pos,
                    facing: 0.0,
                    prev_facing: 0.0,
                    alive: true,
                    squash: 0.0,
                    jiggle: Spring::REST,
                }),
                Def::Mallow(p) => {
                    let c = w.collision.add(Collider::new(
                        Shape::Cylinder { center: p.xz(), radius: 0.72, y_min: p.y, y_max: p.y + 0.62 },
                        TAG_MALLOW,
                    ));
                    w.mallows.push(Mallow { pos: *p, collider: c, squash: 0.0, squash_vel: 0.0, oval: Spring::REST });
                }
                Def::Spinner { pos, length, speed } => {
                    w.collision.add(Collider::new(
                        Shape::Cylinder { center: pos.xz(), radius: 0.3, y_min: pos.y, y_max: pos.y + 1.1 },
                        TAG_STATIC,
                    ));
                    let bar = store.add(assets::spinner_bar(*length));
                    w.spinners.push(Spinner {
                        pos: *pos,
                        length: *length,
                        speed: *speed,
                        angle: 0.0,
                        prev_angle: 0.0,
                        bar,
                    });
                }
                Def::Checkpoint(p) => w.checkpoints.push(Checkpoint { pos: *p, active: false, wobble: 0.0 }),
                Def::Fridge { pos, yaw } => {
                    let local = Aabb::new(Vec3::new(-1.1, 0.0, -0.7), Vec3::new(1.1, 3.3, 0.95));
                    let world_box = local.transformed(&Mat4::from_trs(*pos, Quat::from_rotation_y(*yaw), Vec3::ONE));
                    w.collision.add(Collider::new(Shape::Box(world_box), TAG_FRIDGE));
                    w.fridge = Some(Fridge { pos: *pos, yaw: *yaw, door: 0.0, prev_door: 0.0 });
                }
                Def::Cane { pos, h } => {
                    let mesh = store.add(assets::candy_cane(*h));
                    let model = Mat4::from_trs(*pos, Quat::from_rotation_y(seed as f32), Vec3::ONE);
                    w.decos.push(Deco { mesh, model, unique: true, baked: false });
                }
                Def::Lolly { pos, h, color } => {
                    let mesh = store.add(assets::lolly(*h, Color::hex(*color)));
                    let model = Mat4::from_trs(*pos, Quat::from_rotation_y(0.4), Vec3::ONE);
                    w.decos.push(Deco { mesh, model, unique: true, baked: false });
                }
                Def::Cupcake { pos, s } => w.decos.push(Deco {
                    mesh: a.cupcake,
                    model: Mat4::from_trs(*pos, Quat::from_rotation_y(seed as f32 * 0.1), Vec3::splat(*s)),
                    unique: false,
                    baked: false,
                }),
                Def::Cloud { pos, s } => w.clouds.push(Cloud { base: *pos, scale: *s, phase: pos.x * 0.37 }),
                Def::Hint { pos, r, text } => w.hints.push(Hint { pos: *pos, r: *r, text: text.clone() }),
                Def::Route(step) => w.route.push(*step),
            }
        }
        w
    }

    fn add_platform(
        &mut self,
        mesh: MeshId,
        detail: Option<MeshId>,
        origin: Vec3,
        shape: Shape,
        motion: Option<Motion>,
        softness: f32,
    ) {
        let tag = if motion.is_some() { TAG_MOVING } else { TAG_STATIC };
        // Große Plattformen sind träger und wackeln weniger.
        let ext = shape.aabb().max - shape.aabb().min;
        let softness = softness / (1.0 + 0.08 * ext.x.max(ext.z));
        let collider = self.collision.add(Collider::new(shape, tag));
        if motion.is_some() {
            self.moving.push(self.platforms.len());
        }
        self.platforms.push(Platform {
            mesh,
            detail,
            collider,
            origin,
            motion,
            offset: Vec3::ZERO,
            prev_offset: Vec3::ZERO,
            wobble: Spring::REST,
            softness,
        });
    }

    /// Brennt das Licht in alle unbeweglich ausgerichteten Meshes ein
    /// (Plattformen werden nur verschoben, nie gedreht). Spart pro Frame die
    /// Beleuchtung tausender Flächen.
    pub fn bake_static(&mut self, store: &mut MeshStore, env: &Environment) {
        for p in &self.platforms {
            spatz::render::bake_lighting(store.get_mut(p.mesh), &Mat4::IDENTITY, env);
            if let Some(d) = p.detail {
                spatz::render::bake_lighting(store.get_mut(d), &Mat4::IDENTITY, env);
            }
        }
        for d in &mut self.decos {
            if d.unique && !d.baked {
                spatz::render::bake_lighting(store.get_mut(d.mesh), &d.model, env);
                d.baked = true;
            }
        }
    }

    /// Setzt alle veränderlichen Zustände zurück (Neustart ohne Neuladen).
    pub fn reset(&mut self) {
        self.time = 0.0;
        for p in &mut self.platforms {
            if p.offset != Vec3::ZERO {
                self.collision.colliders[p.collider].move_by(-p.offset);
            }
            self.collision.colliders[p.collider].delta = Vec3::ZERO;
            p.offset = Vec3::ZERO;
            p.prev_offset = Vec3::ZERO;
            p.wobble = Spring::REST;
        }
        for m in &mut self.mallows {
            m.squash = 0.0;
            m.squash_vel = 0.0;
            m.oval = Spring::REST;
        }
        for i in &mut self.items {
            i.taken = false;
            i.pop = 0.0;
        }
        for d in &mut self.donuts {
            d.alive = true;
            d.squash = 0.0;
            d.s = 0.0;
            d.dir = 1.0;
            d.pos = d.start;
            d.prev_pos = d.start;
            d.jiggle = Spring::REST;
        }
        for c in &mut self.checkpoints {
            c.active = false;
        }
        if let Some(f) = &mut self.fridge {
            f.door = 0.0;
            f.prev_door = 0.0;
        }
    }

    /// Bewegungsanteil (0..1) der `i`-ten bewegten Plattform.
    pub fn moving_fraction(&self, i: usize) -> f32 {
        self.moving.get(i).and_then(|&p| self.platforms[p].motion).map(|m| m.fraction(self.time)).unwrap_or(0.0)
    }

    /// Ein Simulationsschritt für alle Weltobjekte.
    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        let t = self.time;
        for p in &mut self.platforms {
            p.prev_offset = p.offset;
            if let Some(m) = p.motion {
                let new = m.delta * m.fraction(t);
                self.collision.colliders[p.collider].move_by(new - p.offset);
                p.offset = new;
            }
            if p.wobble != Spring::REST {
                p.wobble.step(p.wobble_omega(), 0.12, dt);
                p.wobble.limit(0.12);
                if p.wobble.amplitude(p.wobble_omega()) < 2e-4 {
                    p.wobble = Spring::REST;
                }
            }
        }
        for d in &mut self.donuts {
            d.prev_pos = d.pos;
            d.prev_facing = d.facing;
            if !d.alive {
                continue;
            }
            d.jiggle.step(3.6 * TAU, 0.13, dt);
            d.jiggle.limit(0.35);
            if d.squash > 0.0 {
                d.squash += dt;
                if d.squash > 0.6 {
                    d.alive = false;
                }
                continue;
            }
            let len = d.delta.length().max(0.01);
            d.s += d.dir * d.speed * dt / len;
            if d.s >= 1.0 {
                d.s = 1.0;
                d.dir = -1.0;
            } else if d.s <= 0.0 {
                d.s = 0.0;
                d.dir = 1.0;
            }
            let hops = d.hop as i32;
            d.hop += dt * 2.4;
            if d.hop as i32 != hops {
                // Aufgekommen: der Teig wabbelt platt und federt nach.
                d.jiggle.kick(-2.6);
            }
            let hop_h = sin(d.hop * spatz::math::PI).abs() * 0.45;
            d.pos = d.start + Vec3::new(d.delta.x, 0.0, d.delta.y) * d.s + Vec3::new(0.0, hop_h, 0.0);
            let dir = d.delta * d.dir;
            let target = atan2(-dir.x, -dir.y);
            d.facing += spatz::math::angle_diff(d.facing, target) * spatz::math::damp_factor(0.08, dt);
        }
        for m in &mut self.mallows {
            // Schwach gedämpfte Feder zurück in die Ruhelage: Marshmallow-Schwabbeln.
            let acc = -m.squash * 260.0 - m.squash_vel * 3.6;
            m.squash_vel += acc * dt;
            m.squash += m.squash_vel * dt;
            m.oval.step(2.7 * TAU, 0.09, dt);
            m.oval.limit(0.25);
        }
        for s in &mut self.spinners {
            s.prev_angle = s.angle;
            s.angle += s.speed * dt;
        }
        for c in &mut self.checkpoints {
            c.wobble = (c.wobble - dt).max(0.0);
        }
        for i in &mut self.items {
            if i.taken {
                i.pop += dt;
            }
        }
        if let Some(f) = &mut self.fridge {
            f.prev_door = f.door;
        }
    }

    /// Fügt alle Weltobjekte der Zeichenliste hinzu.
    pub fn draw(&self, list: &mut DrawList, a: &Assets, alpha: f32) {
        let t = self.time;
        // Plattformen tragen eingebranntes Licht → unbeleuchtet zeichnen.
        let baked = DrawParams::unlit();
        for p in &self.platforms {
            let off = p.prev_offset.lerp(p.offset, alpha);
            let model = if p.wobble == Spring::REST {
                Mat4::translation(p.origin + off)
            } else {
                // Nachfedern um die Oberseite (lokaler Ursprung): die Lauffläche
                // bleibt, wo sie ist, der Körper staucht und wölbt sich.
                let w = p.wobble.x;
                Mat4::from_trs(p.origin + off, Quat::IDENTITY, Vec3::new(1.0 - 0.5 * w, 1.0 + w, 1.0 - 0.5 * w))
            };
            list.push(p.mesh, model, baked);
            if let Some(d) = p.detail {
                list.push(d, model, baked.with_lod(DETAIL_LOD));
            }
        }
        for d in &self.decos {
            let params = if d.baked { baked } else { DrawParams::OPAQUE };
            list.push(d.mesh, d.model, params.with_lod(SMALL_LOD));
        }
        for c in &self.clouds {
            let p = c.base + Vec3::new(sin(t * 0.07 + c.phase) * 3.0, sin(t * 0.3 + c.phase) * 0.3, 0.0);
            list.add(a.cloud, Mat4::from_trs(p, Quat::from_rotation_y(c.phase), Vec3::splat(c.scale)));
        }
        for i in &self.items {
            if i.taken && i.pop > 0.25 {
                continue;
            }
            let bob = sin(t * 2.5 + i.phase) * 0.12;
            let grow = if i.taken { 1.0 + i.pop * 3.0 } else { 1.0 };
            let shrink = if i.taken { (1.0 - i.pop * 4.0).max(0.0) } else { 1.0 };
            let s = Vec3::new(grow * shrink, grow * shrink, grow * shrink);
            let p = i.pos + Vec3::new(0.0, bob + if i.taken { i.pop * 2.0 } else { 0.0 }, 0.0);
            match i.kind {
                ItemKind::Cherry => {
                    let m = Mat4::from_trs(p, Quat::from_rotation_y(t * 2.0 + i.phase), s);
                    // Zwei Detailstufen: nah rund, fern als Oktaeder.
                    list.push(a.cherry, m, DrawParams::lod(CHERRY_LOD));
                    list.push(a.cherry_far, m, DrawParams::OPAQUE.with_lod_range(CHERRY_LOD, SMALL_LOD));
                }
                ItemKind::Ice => {
                    let r = Quat::from_rotation_y(t * 1.3 + i.phase) * Quat::from_rotation_x(0.5);
                    list.push(a.ice_core, Mat4::from_trs(p, r, s), DrawParams::lod(SMALL_LOD));
                    list.push(a.ice, Mat4::from_trs(p, r, s), DrawParams::BLEND.with_lod(SMALL_LOD));
                }
            }
        }
        for d in &self.donuts {
            if !d.alive {
                continue;
            }
            let pos = d.prev_pos.lerp(d.pos, alpha);
            let yaw = d.prev_facing + spatz::math::angle_diff(d.prev_facing, d.facing) * alpha;
            let (sy, sxz) = if d.squash > 0.0 {
                let k = (d.squash / 0.15).min(1.0);
                (1.0 - 0.8 * k, 1.0 + 0.5 * k)
            } else if d.jiggle != Spring::REST {
                let sy = 1.0 + d.jiggle.x;
                (sy, 1.0 / spatz::math::sqrt(sy))
            } else {
                (1.0, 1.0)
            };
            let base = Mat4::from_trs(
                pos + Vec3::new(0.0, 0.24 * sy, 0.0),
                Quat::from_rotation_y(yaw),
                Vec3::new(sxz, sy, sxz),
            );
            list.add(a.donut, base);
            if d.squash <= 0.0 {
                for side in [-1.0f32, 1.0] {
                    let eye = base * Mat4::translation(Vec3::new(0.17 * side, 0.12, -0.52));
                    list.push(a.eye_white, eye, DrawParams::unlit());
                    list.push(
                        a.pupil,
                        eye * Mat4::translation(Vec3::new(-0.02 * side, -0.03, -0.012)),
                        DrawParams::unlit(),
                    );
                    let brow = base
                        * Mat4::from_trs(
                            Vec3::new(0.16 * side, 0.27, -0.55),
                            Quat::from_rotation_z(0.45 * side),
                            Vec3::ONE,
                        );
                    list.add(a.brow, brow);
                }
            }
        }
        for m in &self.mallows {
            let sy = (1.0 - m.squash).clamp(0.4, 1.6);
            let sxz = 1.0 / sy.max(0.5).sqrt();
            let o = m.oval.x;
            list.add(a.mallow, Mat4::from_trs(m.pos, Quat::IDENTITY, Vec3::new(sxz * (1.0 + o), sy, sxz * (1.0 - o))));
        }
        for s in &self.spinners {
            list.add(a.spinner_post, Mat4::translation(s.pos));
            let ang = s.prev_angle + (s.angle - s.prev_angle) * alpha;
            list.add(
                s.bar,
                Mat4::from_trs(s.pos + Vec3::new(0.0, Spinner::BAR_Y, 0.0), Quat::from_rotation_y(ang), Vec3::ONE),
            );
        }
        for c in &self.checkpoints {
            let wob = sin(c.wobble * 25.0) * c.wobble * 0.6;
            let m = Mat4::from_trs(c.pos, Quat::from_rotation_y(t * 0.8) * Quat::from_rotation_z(wob), Vec3::ONE);
            list.add(if c.active { a.popsicle_on } else { a.popsicle_off }, m);
        }
        if let Some(f) = &self.fridge {
            let base = Mat4::from_trs(f.pos, Quat::from_rotation_y(f.yaw), Vec3::ONE);
            list.add(a.fridge, base);
            let door = f.prev_door + (f.door - f.prev_door) * alpha;
            let hinge =
                base * Mat4::from_trs(Vec3::new(-1.08, 0.0, 0.79), Quat::from_rotation_y(-door * 1.9), Vec3::ONE);
            list.add(a.fridge_door, hinge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level;

    #[test]
    fn builtin_level_builds() {
        let def = level::parse(level::BUILTIN).unwrap();
        let mut store = MeshStore::new();
        let a = Assets::build(&mut store);
        let mut w = World::build(&def, &mut store, &a);
        assert!(w.fridge.is_some());
        assert!(w.moving.len() >= 2);
        assert!(!w.route.is_empty());
        for _ in 0..600 {
            w.update(1.0 / 120.0);
        }
        // Bewegte Plattformen bleiben im erlaubten Bereich.
        for &i in &w.moving {
            let p = &w.platforms[i];
            let m = p.motion.unwrap();
            assert!(p.offset.length() <= m.delta.length() + 1e-3);
        }
        w.reset();
        assert_eq!(w.time, 0.0);
    }
}
