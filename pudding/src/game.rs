//! Spiellogik: verbindet Welt, Spielfigur, Kamera, Effekte und HUD und
//! implementiert `spatz::app::Game`.

use crate::assets::{Assets, pal};
use crate::autopilot::Autopilot;
use crate::camera_rig::{CameraRig, INTRO_TIME};
use crate::hud::{self, FinishStats, HudData, Messages};
use crate::jelly::PuddingJelly;
use crate::level::LevelDef;
use crate::particles::{Burst, Particles};
use crate::player::{Controls, MELT_TIME, Player, STOMP_VEL};
use crate::world::{ItemKind, Spinner, World};
use spatz::app::{Control, Game};
use spatz::input::{Input, Key, PAD_BACK, PAD_START};
use spatz::math::{Vec2, sin, smoothstep};
use spatz::prelude::*;
use spatz::render::{Framebuffer, RenderStats};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Playing,
    Dead { t: f32 },
    Finished { t: f32 },
}

pub struct PuddingGame {
    pub store: MeshStore,
    pub assets: Assets,
    pub world: World,
    pub player: Player,
    /// Der Pudding als Weichkörper (rein optisch).
    pub jelly: PuddingJelly,
    pub rig: CameraRig,
    pub particles: Particles,
    pub messages: Messages,
    pub renderer: Renderer,
    list: DrawList,
    pub env: Environment,
    pub phase: Phase,
    /// Spielzeit des aktuellen Laufs.
    pub time: f32,
    pub checkpoint: Vec3,
    pub autopilot: Option<Autopilot>,
    pub paused: bool,
    pub title_t: f32,
    pub hint: Option<usize>,
    pub show_hud: bool,
    pub total_cherries: u32,
    pub finish_time: Option<f32>,
    /// Startpunkt der Einfahrt in den Kühlschrank.
    finish_from: Vec3,
    fx_timer: f32,
    rng: Rng,
}

/// Zufälliger Spruch aus einer Liste (für Abwechslung bei Meldungen).
fn pick(rng: &mut Rng, options: &[&'static str]) -> &'static str {
    options[rng.below(options.len() as u32) as usize]
}

fn dist_point_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

impl PuddingGame {
    pub fn new(def: &LevelDef, demo: bool) -> Self {
        let mut store = MeshStore::new();
        let assets = Assets::build(&mut store);
        let mut world = World::build(def, &mut store, &assets);
        let mut rig = CameraRig::new(world.spawn, world.spawn_yaw);
        rig.intro = INTRO_TIME;
        let total_cherries = world.items.iter().filter(|i| i.kind == ItemKind::Cherry).count() as u32;
        let env = Environment {
            sun_dir: Vec3::new(0.4, -1.0, -0.55).normalize_or_zero(),
            sun_color: [0.62, 0.58, 0.50],
            ambient_sky: [0.62, 0.62, 0.72],
            ambient_ground: [0.44, 0.38, 0.40],
            sky_zenith: Color::hex(0x6A9CFF),
            sky_horizon: Color::hex(0xFFD8EC),
            sky_ground: Color::hex(0xE8B8C8),
            fog: true,
            fog_start: 30.0,
            fog_end: 120.0,
            sun_disc: true,
            sun_disc_color: Color::hex(0xFFF8DA),
        };
        world.bake_static(&mut store, &env);
        // Obergrenze der Zeichenaufrufe pro Frame → Puffer vorab reservieren,
        // damit im laufenden Spiel keine Heap-Allokationen mehr anfallen.
        let max_items = world.platforms.len() * 2
            + world.decos.len()
            + world.clouds.len()
            + world.items.len() * 2
            + world.donuts.len() * 9
            + world.mallows.len()
            + world.spinners.len() * 2
            + world.checkpoints.len()
            + 32;
        let mut renderer = Renderer::new();
        renderer.reserve(store.max_vertices(), max_items, 1080);
        let list = DrawList::with_capacity(max_items, crate::particles::MAX_PARTICLES);
        let player = Player::new(world.spawn, world.spawn_yaw);
        let jelly = PuddingJelly::new(store.get_mut(assets.pudding), assets.pudding_parts.clone());
        Self {
            checkpoint: world.spawn,
            jelly,
            store,
            assets,
            world,
            player,
            rig,
            particles: Particles::new(),
            messages: Messages::new(),
            renderer,
            list,
            env,
            phase: Phase::Playing,
            time: 0.0,
            autopilot: demo.then(Autopilot::new),
            paused: false,
            title_t: 4.0,
            hint: None,
            show_hud: true,
            total_cherries,
            finish_time: None,
            finish_from: Vec3::ZERO,
            fx_timer: 0.0,
            rng: Rng::new(7),
        }
    }

    /// Neustart des Levels (ohne Neuladen der Meshes).
    pub fn restart(&mut self) {
        self.world.reset();
        self.player = Player::new(self.world.spawn, self.world.spawn_yaw);
        self.jelly.reset();
        self.rig.snap(self.world.spawn, self.world.spawn_yaw);
        self.rig.intro = INTRO_TIME;
        self.checkpoint = self.world.spawn;
        self.phase = Phase::Playing;
        self.time = 0.0;
        self.title_t = 4.0;
        self.finish_time = None;
        self.particles.clear();
        self.messages.clear();
        if let Some(ap) = &mut self.autopilot {
            *ap = Autopilot::new();
        }
    }

    fn die(&mut self) {
        self.player.alive = false;
        self.player.deaths += 1;
        self.phase = Phase::Dead { t: 0.0 };
    }

    fn respawn(&mut self) {
        let pos = self.checkpoint + Vec3::new(0.0, 0.3, 0.0);
        self.player.respawn(pos, self.rig.yaw);
        self.jelly.reset();
        self.rig.snap(pos, self.rig.yaw);
        self.phase = Phase::Playing;
        if let Some(ap) = &mut self.autopilot {
            ap.resync(&self.world, pos);
        }
    }

    fn effects_for(&mut self, ev: crate::player::PlayerEvents) {
        let feet = self.player.body.pos;
        if ev.jumped {
            self.particles.burst(
                feet,
                Burst {
                    count: 5,
                    colors: &[pal::CREAM],
                    speed: 2.0,
                    up: 0.5,
                    life: 0.35,
                    size: 0.14,
                    ..Burst::default()
                },
            );
        }
        if let Some(speed) = ev.landed {
            // Die Plattform unter dem Pudding federt nach (nur Optik).
            if let Some(g) = self.player.body.ground {
                if let Some(p) = self.world.platforms.iter_mut().find(|p| p.collider == g) {
                    p.land(speed);
                }
            }
            if speed > 6.0 {
                let n = ((speed - 4.0) * 1.2) as usize;
                self.particles.burst(
                    feet,
                    Burst {
                        count: n.min(14),
                        colors: &[pal::CREAM, pal::VANILLA],
                        speed: 3.5,
                        up: 1.0,
                        life: 0.45,
                        size: 0.15,
                        ..Burst::default()
                    },
                );
            }
        }
        if let Some(c) = ev.bounced_on {
            if let Some(m) = self.world.mallows.iter_mut().find(|m| m.collider == c) {
                m.squash_vel += 11.0;
                m.oval.kick(2.2);
            }
            self.messages.push("BOING!", 0xFFB0D8);
            self.particles.burst(
                feet,
                Burst {
                    count: 12,
                    colors: &[pal::MALLOW, pal::MALLOW_SIDE],
                    speed: 4.0,
                    up: 3.0,
                    life: 0.5,
                    size: 0.16,
                    ..Burst::default()
                },
            );
        }
    }

    /// Kontakte mit Gegenständen, Gegnern, Hindernissen, Checkpoints, Ziel.
    fn interact(&mut self, dt: f32) {
        let p = &mut self.player;
        let pos = p.body.pos;

        for it in &mut self.world.items {
            if it.taken || !p.body.touches(it.pos, 0.45) {
                continue;
            }
            it.taken = true;
            match it.kind {
                ItemKind::Cherry => {
                    p.cherries += 1;
                    self.particles.burst(
                        it.pos,
                        Burst {
                            count: 10,
                            colors: &[pal::CHERRY, Color::hex(0xFFB0C0), pal::WHITE],
                            speed: 3.5,
                            up: 2.0,
                            life: 0.5,
                            size: 0.12,
                            gravity: 4.0,
                            ..Burst::default()
                        },
                    );
                    if p.cherries % 10 == 0 {
                        let t = spatz::render::overlay::fmt::<48>(format_args!("{} KIRSCHEN!", p.cherries));
                        self.messages.push(t.as_str(), 0xFF6080);
                    }
                }
                ItemKind::Ice => {
                    p.melt = (p.melt - 0.45).max(0.0);
                    self.messages.push("BRRR! SCHÖN KÜHL!", 0x9EE0FF);
                    self.particles.burst(
                        it.pos,
                        Burst {
                            count: 16,
                            colors: &[pal::ICE, pal::WHITE],
                            speed: 4.0,
                            up: 1.5,
                            life: 0.7,
                            size: 0.13,
                            gravity: 2.0,
                            ..Burst::default()
                        },
                    );
                }
            }
        }

        for d in &mut self.world.donuts {
            if !d.alive || d.squash > 0.0 {
                continue;
            }
            let horiz = (pos.xz() - d.pos.xz()).length();
            if horiz < 0.42 + 0.7 && pos.y < d.pos.y + 0.7 && pos.y + 0.9 > d.pos.y {
                if p.body.vel.y < -0.5 && pos.y > d.pos.y + 0.2 {
                    d.squash = 0.001;
                    let v = p.body.vel;
                    p.launch(Vec3::new(v.x, STOMP_VEL, v.z));
                    self.messages
                        .push(pick(&mut self.rng, &["PLOPP!", "ZERMATSCHT!", "MAMPF!", "DONUT-PÜREE!"]), 0xFF7EB6);
                    self.particles.burst(
                        d.pos + Vec3::new(0.0, 0.3, 0.0),
                        Burst {
                            count: 18,
                            colors: &pal::SPRINKLES,
                            speed: 5.0,
                            up: 3.0,
                            life: 0.7,
                            size: 0.12,
                            ..Burst::default()
                        },
                    );
                } else if p.invuln <= 0.0 {
                    let away = (pos.xz() - d.pos.xz()).normalize_or_zero();
                    let away = if away.length_sq() < 0.5 { Vec2::new(0.0, 1.0) } else { away };
                    p.launch(Vec3::new(away.x * 8.0, 7.0, away.y * 8.0));
                    p.invuln = 1.2;
                    p.melt = (p.melt + 0.08).min(0.99);
                    self.messages.push(pick(&mut self.rng, &["AUTSCH!", "AUA!", "HEY, DAS KLEBT!"]), 0xFF5050);
                    self.particles.burst(
                        pos + Vec3::new(0.0, 0.5, 0.0),
                        Burst { count: 8, colors: &[pal::VANILLA], speed: 4.0, ..Burst::default() },
                    );
                }
            }
        }

        for s in &self.world.spinners {
            let bar_y = s.pos.y + Spinner::BAR_Y;
            if p.invuln > 0.0 || pos.y > bar_y + Spinner::BAR_R || pos.y + 0.9 < bar_y - Spinner::BAR_R {
                continue;
            }
            let (a, b) = s.segment();
            if dist_point_segment(pos.xz(), a, b) < 0.42 + Spinner::BAR_R {
                let rel = pos.xz() - s.pos.xz();
                let push = Vec2::new(rel.y, -rel.x).normalize_or_zero() * s.speed.signum();
                let out = rel.normalize_or_zero();
                let dir = (push + out * 0.4).normalize_or_zero();
                p.launch(Vec3::new(dir.x * 9.0, 6.5, dir.y * 9.0));
                p.invuln = 1.0;
                p.melt = (p.melt + 0.05).min(0.99);
                self.messages.push(pick(&mut self.rng, &["WUSCH!", "ZACK!", "HOMERUN!"]), 0xFF5050);
            }
        }

        for cp in &mut self.world.checkpoints {
            if cp.active {
                continue;
            }
            if (pos.xz() - cp.pos.xz()).length() < 1.1 && (pos.y - cp.pos.y).abs() < 2.0 {
                cp.active = true;
                cp.wobble = 0.6;
                self.checkpoint = cp.pos;
                p.melt = 0.0;
                self.messages.push("CHECKPOINT! FRISCH GEKÜHLT", 0x7CFF7C);
                self.particles.burst(
                    cp.pos + Vec3::new(0.0, 1.5, 0.0),
                    Burst {
                        count: 24,
                        colors: &pal::SPRINKLES,
                        speed: 5.0,
                        up: 4.0,
                        life: 1.0,
                        size: 0.14,
                        gravity: 6.0,
                        ..Burst::default()
                    },
                );
            }
        }

        self.hint = self.world.hints.iter().position(|h| (h.pos - pos).length() < h.r);

        if let Some(f) = &self.world.fridge {
            let g = f.goal_point();
            if (pos.xz() - g.xz()).length() < 1.4 && (pos.y - f.pos.y).abs() < 1.2 {
                self.finish_time = Some(self.time);
                self.finish_from = pos;
                self.phase = Phase::Finished { t: 0.0 };
                p.body.vel = Vec3::ZERO;
                return;
            }
        }

        // Schmelzen; Schweißtropfen bei großer Hitze.
        p.melt += dt / MELT_TIME;
        if p.melt > 0.6 {
            self.fx_timer -= dt;
            if self.fx_timer <= 0.0 {
                self.fx_timer = 0.3 - p.melt * 0.15;
                self.particles.burst(
                    pos + Vec3::new(0.0, 0.9, 0.0),
                    Burst {
                        count: 1,
                        colors: &[Color::hex(0x8ED8FF)],
                        speed: 1.5,
                        up: 2.0,
                        life: 0.6,
                        size: 0.1,
                        ..Burst::default()
                    },
                );
            }
        }
        if p.melt >= 1.0 {
            self.messages.push(pick(&mut self.rng, &["GESCHMOLZEN!", "PUDDING-SUPPE!", "ZU HEISS!"]), 0xFFB040);
            self.particles.burst(
                pos,
                Burst {
                    count: 20,
                    colors: &[pal::VANILLA, pal::CARAMEL],
                    speed: 3.0,
                    up: 0.5,
                    life: 1.0,
                    size: 0.2,
                    ..Burst::default()
                },
            );
            self.die();
            return;
        }
        if pos.y < self.world.sea_y + 0.2 {
            self.messages.push(pick(&mut self.rng, &["PLATSCH!", "BLUBB!", "SCHOKO-BAD!"]), 0xD08040);
            let at = Vec3::new(pos.x, self.world.sea_y + 0.3, pos.z);
            self.particles.burst(
                at,
                Burst {
                    count: 26,
                    colors: &[pal::SEA, pal::CHOCO, pal::VANILLA],
                    speed: 4.0,
                    up: 7.0,
                    life: 1.0,
                    size: 0.24,
                    gravity: 14.0,
                    ..Burst::default()
                },
            );
            self.die();
        }
    }

    fn update_finish(&mut self, t: f32, input: &Input, dt: f32) {
        self.phase = Phase::Finished { t };
        let Some(f) = &mut self.world.fridge else { return };
        // 0..0.6 s Tür auf, 0.6..1.5 s hineinhüpfen, 1.5..2.1 s Tür zu.
        f.door = if t < 1.5 { smoothstep(0.0, 0.6, t) } else { 1.0 - smoothstep(1.5, 2.1, t) };
        let inside = f.pos + Quat::from_rotation_y(f.yaw).rotate(Vec3::new(0.0, 0.62, 0.1));
        let k = smoothstep(0.6, 1.5, t);
        let p = &mut self.player;
        p.prev_pos = p.body.pos;
        let hop = sin(k * spatz::math::PI) * 1.2;
        p.body.pos = self.finish_from.lerp(inside, k) + Vec3::new(0.0, hop, 0.0);
        p.yaw = f.yaw + spatz::math::PI;
        p.prev_yaw = p.yaw;
        p.alive = t < 1.9;
        self.fx_timer -= dt;
        if t > 2.0 && self.fx_timer <= 0.0 {
            self.fx_timer = 0.12;
            let top = f.pos + Vec3::new(self.rng.range(-3.0, 3.0), 5.5, self.rng.range(-1.0, 3.0));
            self.particles.burst(
                top,
                Burst {
                    count: 6,
                    colors: &pal::SPRINKLES,
                    speed: 2.0,
                    up: 1.0,
                    life: 2.5,
                    size: 0.16,
                    gravity: 2.5,
                    spread: 1.0,
                },
            );
        }
        let demo_restart = self.autopilot.is_some() && t > 7.0;
        if t > 2.2 && (input.pressed(Key::Enter) || input.pad.pressed(PAD_START) || demo_restart) {
            self.restart();
        }
    }

    fn animate_sea(&mut self, cam: Vec3) {
        const CELLS: usize = crate::assets::SEA_CELLS;
        const SIZE: f32 = crate::assets::SEA_SIZE;
        let cell = SIZE / CELLS as f32;
        let cx = (cam.x / cell).round() * cell;
        let cz = (cam.z / cell).round() * cell;
        let t = self.world.time;
        let sea_y = self.world.sea_y;
        let x0 = cx - SIZE * 0.5;
        let z0 = cz - SIZE * 0.5;
        // Separierbare Wellen: nur ~130 Sinus-Aufrufe pro Frame.
        let mut sx = [0.0f32; CELLS + 1];
        let mut sz = [0.0f32; CELLS + 1];
        let mut sd = [0.0f32; 2 * CELLS + 1];
        for i in 0..=CELLS {
            sx[i] = 0.35 * sin(0.21 * (x0 + i as f32 * cell) + 1.1 * t);
            sz[i] = 0.3 * sin(0.17 * (z0 + i as f32 * cell) - 0.8 * t);
        }
        for (k, v) in sd.iter_mut().enumerate() {
            *v = 0.25 * sin(0.13 * (x0 + z0 + k as f32 * cell) + 0.6 * t);
        }
        let m = self.store.get_mut(self.assets.sea);
        for j in 0..=CELLS {
            for i in 0..=CELLS {
                let v = &mut m.positions[j * (CELLS + 1) + i];
                v.x = x0 + i as f32 * cell;
                v.z = z0 + j as f32 * cell;
                v.y = sea_y + sx[i] + sz[j] + sd[i + j];
            }
        }
        spatz::render::shapes::recompute_normals(m);
        m.bounds = Sphere::new(Vec3::new(cx, sea_y, cz), SIZE * 0.72);
    }

    fn draw_shadows(&mut self, alpha: f32) {
        let a = &self.assets;
        if self.player.alive {
            let p = self.player.render_pos(alpha);
            if let Some((top, _)) = self.world.collision.ground_below(p + Vec3::new(0.0, 0.25, 0.0), 40.0) {
                let s = (0.5 * (1.0 - (p.y - top) / 12.0)).clamp(0.22, 0.5) * (1.0 + 0.2 * self.player.melt);
                self.list.push(
                    a.shadow,
                    Mat4::from_trs(Vec3::new(p.x, top + 0.05, p.z), Quat::IDENTITY, Vec3::new(s, 1.0, s)),
                    DrawParams::SHADOW,
                );
            }
        }
        for d in &self.world.donuts {
            if d.alive {
                let p = d.prev_pos.lerp(d.pos, alpha);
                let s = 0.62 - (p.y - d.start.y) * 0.3;
                self.list.push(
                    a.shadow,
                    Mat4::from_trs(Vec3::new(p.x, d.start.y + 0.04, p.z), Quat::IDENTITY, Vec3::new(s, 1.0, s)),
                    DrawParams::SHADOW,
                );
            }
        }
    }

    /// Beleuchtet den verformten Pudding für eine Kameraposition (Glanzlicht
    /// hängt vom Blickwinkel ab). `prepare` erledigt das mit der Spielkamera.
    pub fn relight_player(&mut self, alpha: f32, eye: Vec3) {
        let model = self.player.model(alpha);
        self.jelly.light(self.store.get_mut(self.assets.pudding), &model, &self.env, eye);
    }

    /// Baut die Zeichenliste für den Frame und liefert die Kamera.
    pub fn prepare(&mut self, alpha: f32) -> Camera {
        let cam = self.rig.camera(alpha);
        self.animate_sea(cam.position);
        self.list.clear();
        self.world.draw(&mut self.list, &self.assets, alpha);
        if self.player.alive {
            self.jelly.skin(self.store.get_mut(self.assets.pudding), alpha);
            self.relight_player(alpha, cam.position);
        }
        self.player.draw(&mut self.list, &self.assets, alpha, &self.jelly);
        self.draw_shadows(alpha);
        // Der See liegt immer hinter allem → zuletzt zeichnen (spart Überzeichnung).
        let sea = DrawParams { sort_bias: 1.0e6, ..DrawParams::OPAQUE };
        self.list.push(self.assets.sea, Mat4::IDENTITY, sea);
        self.particles.draw(&mut self.list, alpha);
        cam
    }

    /// Szene in den Framebuffer rendern (ohne HUD).
    pub fn render_scene(&mut self, fb: &mut Framebuffer, alpha: f32) {
        let cam = self.prepare(alpha);
        self.renderer.begin(fb, &cam, &self.env);
        self.renderer.draw_list(fb, &self.store, &self.list);
        self.renderer.finish(fb);
    }

    /// Für Profiling: Zeichenliste und Speicher des letzten `prepare`.
    pub fn parts(&mut self) -> (&mut Renderer, &MeshStore, &DrawList, &Environment) {
        (&mut self.renderer, &self.store, &self.list, &self.env)
    }

    pub fn draw_hud(&self, fb: &mut Framebuffer) {
        let finished = match (self.phase, self.finish_time) {
            (Phase::Finished { .. }, Some(t)) => Some(FinishStats {
                time: t,
                cherries: self.player.cherries,
                total: self.total_cherries,
                deaths: self.player.deaths,
            }),
            _ => None,
        };
        let data = HudData {
            cherries: self.player.cherries,
            total_cherries: self.total_cherries,
            time: self.finish_time.unwrap_or(self.time),
            melt: self.player.melt,
            hint: self.hint.map(|i| self.world.hints[i].text.as_str()),
            finished,
            finish_t: if let Phase::Finished { t } = self.phase { t } else { 0.0 },
            paused: self.paused,
            title_t: self.title_t,
            demo: self.autopilot.is_some(),
            dead_t: if let Phase::Dead { t } = self.phase { Some(t) } else { None },
        };
        hud::draw(fb, &data, &self.messages);
    }

    /// Ein Simulationsschritt mit gegebenen Steuerbefehlen (auch für Tests).
    pub fn step(&mut self, c: &Controls, input: &Input, dt: f32) {
        self.title_t = (self.title_t - dt).max(0.0);
        match self.phase {
            Phase::Playing => {
                self.world.update(dt);
                self.time += dt;
                let ev = self.player.update(c, self.rig.yaw, &self.world, dt);
                self.effects_for(ev);
                self.interact(dt);
                self.jelly.step(&self.player, dt);
            }
            Phase::Dead { t } => {
                self.world.update(dt);
                if t + dt > 1.1 {
                    self.respawn();
                } else {
                    self.phase = Phase::Dead { t: t + dt };
                }
            }
            Phase::Finished { t } => {
                self.world.update(dt);
                self.update_finish(t + dt, input, dt);
                self.jelly.step(&self.player, dt);
            }
        }
        let turn = if self.autopilot.is_some() { 0.0 } else { c.cam_turn };
        self.rig.update(&self.player, turn, dt);
        self.particles.update(dt);
        self.messages.update(dt);
    }
}

impl Game for PuddingGame {
    fn update(&mut self, input: &Input, dt: f32, ctl: &mut Control) {
        if input.pressed(Key::Escape) || input.pad.pressed(PAD_BACK) {
            ctl.quit = true;
            return;
        }
        let finished = matches!(self.phase, Phase::Finished { .. });
        if (input.pressed(Key::P) || input.pad.pressed(PAD_START)) && !finished {
            self.paused = !self.paused;
        }
        if input.pressed(Key::H) {
            self.show_hud = !self.show_hud;
        }
        if input.pressed(Key::Tab) {
            // Autopilot jederzeit ein-/ausschalten.
            self.autopilot = match self.autopilot {
                Some(_) => None,
                None => {
                    let mut ap = Autopilot::new();
                    ap.resync(&self.world, self.player.body.pos);
                    Some(ap)
                }
            };
        }
        if self.paused {
            if input.pressed(Key::R) {
                self.paused = false;
                let melt = self.player.melt;
                self.respawn();
                self.player.melt = melt;
            }
            return;
        }
        if input.pressed(Key::R) && self.phase == Phase::Playing {
            // Zurück zum Checkpoint – die Hitze bleibt aber (kein Gratis-Abkühlen).
            let melt = self.player.melt;
            self.respawn();
            self.player.melt = melt;
        }
        let c = match &mut self.autopilot {
            Some(ap) => ap.controls(&self.world, &self.player, self.rig.yaw, dt),
            None => Controls::from_input(input),
        };
        self.step(&c, input, dt);
    }

    fn render(&mut self, fb: &mut Framebuffer, alpha: f32) -> RenderStats {
        self.render_scene(fb, alpha);
        if self.show_hud {
            self.draw_hud(fb);
        }
        self.renderer.stats
    }
}
