//! Eine laufende Partie: verbindet Welt, Spielfigur, Weichkörper, Kamera,
//! Partikel und Meldungen. Gleiche Spiellogik wie am PC, dazu
//! Klang-Ereignisse und Spielstand (Checkpoint, Kirschen, Zeit).

use crate::assets::{Assets, pal};
use crate::audio::Sfx;
use crate::autopilot::Autopilot;
use crate::camera_rig::{CameraRig, INTRO_TIME};
use crate::hud::Messages;
use crate::jelly::PuddingJelly;
use crate::campaign::Difficulty;
use crate::level::{GoalStyle, LevelDef, Theme};
use crate::particles::{Burst, Particles};
use crate::player::{Controls, Player, STOMP_VEL};
use crate::save::Progress;
use crate::world::{ItemKind, Spinner, World};
use alloc::vec::Vec;
use spatz::math::{Vec2, abs, round, sign, sin, smoothstep};
use spatz::prelude::*;

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
    pub list: DrawList,
    pub env: Environment,
    pub phase: Phase,
    /// Spielzeit des aktuellen Laufs.
    pub time: f32,
    pub checkpoint: Vec3,
    /// Index des zuletzt erreichten Checkpoints (für den Spielstand).
    pub checkpoint_idx: Option<usize>,
    /// Wird gesetzt, wenn ein Checkpoint erreicht wurde (→ automatisch speichern).
    pub checkpoint_reached: bool,
    pub autopilot: Option<Autopilot>,
    pub title_t: f32,
    pub hint: Option<usize>,
    pub total_cherries: u32,
    pub finish_time: Option<f32>,
    /// Klang-Ereignisse dieses Schritts (die App leitet sie an den Synth).
    pub events: Vec<Sfx>,
    /// Schwierigkeit (Tempo, Schmelzzeit, Checkpoints).
    pub difficulty: Difficulty,
    /// Umgebung des Kapitels (aus dem Level).
    pub theme: Theme,
    finish_from: Vec3,
    fx_timer: f32,
    /// Seit wann der Pudding auf einer glühenden Herdplatte steht.
    hot_t: f32,
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

/// Licht und Himmel je Kapitel.
pub fn environment_for(theme: Theme) -> Environment {
    let base = environment();
    match theme {
        Theme::Kitchen => base,
        Theme::Fridge => Environment {
            sun_dir: Vec3::new(0.3, -1.0, -0.45).normalize_or_zero(),
            sun_color: [0.66, 0.72, 0.80],
            ambient_sky: [0.44, 0.50, 0.58],
            ambient_ground: [0.24, 0.28, 0.34],
            sky_zenith: Color::hex(0x7FB4E4),
            sky_horizon: Color::hex(0xB8D8F0),
            sky_ground: Color::hex(0x98B8D4),
            fog_start: 40.0,
            fog_end: 60.0,
            ..base
        },
        Theme::Freezer => Environment {
            sun_dir: Vec3::new(-0.35, -1.0, -0.45).normalize_or_zero(),
            sun_color: [0.62, 0.72, 0.86],
            ambient_sky: [0.40, 0.50, 0.66],
            ambient_ground: [0.22, 0.30, 0.42],
            sky_zenith: Color::hex(0x3F7FD0),
            sky_horizon: Color::hex(0x98C8EC),
            sky_ground: Color::hex(0x78A8D2),
            fog_start: 36.0,
            fog_end: 54.0,
            ..base
        },
        Theme::Buffet => Environment {
            sun_dir: Vec3::new(0.5, -1.0, -0.4).normalize_or_zero(),
            sun_color: [0.82, 0.68, 0.60],
            ambient_sky: [0.48, 0.40, 0.54],
            ambient_ground: [0.28, 0.20, 0.26],
            sky_zenith: Color::hex(0x7A48D8),
            sky_horizon: Color::hex(0xE6A6CE),
            sky_ground: Color::hex(0xB884A8),
            fog_start: 42.0,
            ..base
        },
    }
}

/// Farbe des Sees unter dem Level (Schokolade, Milch, Eiswasser, Sahne).
fn sea_color(theme: Theme) -> Color {
    match theme {
        Theme::Kitchen => pal::SEA,
        Theme::Fridge => Color::hex(0xF2EEE2),
        Theme::Freezer => Color::hex(0x5E9CD4),
        Theme::Buffet => Color::hex(0xFFEEDA),
    }
}

/// Licht und Himmel (Nebel endet auf dem Mikrocontroller früher als am PC).
pub fn environment() -> Environment {
    Environment {
        sun_dir: Vec3::new(0.4, -1.0, -0.55).normalize_or_zero(),
        sun_color: [0.80, 0.74, 0.62],
        ambient_sky: [0.44, 0.44, 0.54],
        ambient_ground: [0.24, 0.20, 0.24],
        sky_zenith: Color::hex(0x3F78E0),
        sky_horizon: Color::hex(0xC8DCF4),
        sky_ground: Color::hex(0xB0A8C8),
        fog: true,
        fog_start: 42.0,
        fog_end: 64.0,
        sun_disc: false,
        sun_disc_color: Color::hex(0xFFF8DA),
    }
}

impl PuddingGame {
    pub fn new(def: &LevelDef, demo: bool) -> Self {
        let mut store = MeshStore::new();
        let mut assets = Assets::build(&mut store);
        let mut world = World::build(def, &mut store, &assets);
        let theme = world.theme;
        if let Some(f) = &world.fridge
            && f.style != GoalStyle::Kitchen
        {
            let (body, door) = crate::assets::goal(f.style);
            assets.fridge = store.add(body);
            assets.fridge_door = store.add(door);
        }
        if theme != Theme::Kitchen {
            let sea = spatz::render::shapes::grid(crate::assets::SEA_CELLS, crate::assets::SEA_SIZE, sea_color(theme));
            assets.sea = store.add(sea);
        }
        let mut rig = CameraRig::new(world.spawn, world.spawn_yaw);
        rig.intro = INTRO_TIME;
        let total_cherries = world.items.iter().filter(|i| i.kind == ItemKind::Cherry).count() as u32;
        let env = environment_for(theme);
        world.bake_static(&mut store, &env);
        let max_items = Self::max_items(&world);
        let list = DrawList::with_capacity(max_items, crate::particles::MAX_PARTICLES);
        let mut player = Player::new(world.spawn, world.spawn_yaw);
        player.frost = theme == Theme::Freezer;
        let jelly = PuddingJelly::new(store.get_mut(assets.pudding), assets.pudding_parts.clone());
        Self {
            checkpoint: world.spawn,
            checkpoint_idx: None,
            checkpoint_reached: false,
            jelly,
            store,
            assets,
            world,
            player,
            rig,
            particles: Particles::new(),
            messages: Messages::new(),
            list,
            env,
            phase: Phase::Playing,
            time: 0.0,
            autopilot: demo.then(Autopilot::new),
            title_t: 4.0,
            hint: None,
            total_cherries,
            finish_time: None,
            events: Vec::with_capacity(16),
            difficulty: Difficulty::Easy,
            theme,
            finish_from: Vec3::ZERO,
            fx_timer: 0.0,
            hot_t: 0.0,
            rng: Rng::new(7),
        }
    }

    /// Obergrenze der Zeichenaufrufe pro Frame (Puffer vorab reservieren →
    /// keine Heap-Allokationen im laufenden Spiel).
    pub fn max_items(world: &World) -> usize {
        world.platforms.len() * 2
            + world.decos.len()
            + world.clouds.len()
            + world.items.len() * 2
            + world.donuts.len() * 9
            + world.mallows.len()
            + world.spinners.len() * 2
            + world.checkpoints.len()
            + 32
    }

    /// Neustart des Levels (ohne Neuladen der Meshes).
    pub fn restart(&mut self) {
        self.world.reset();
        self.player = Player::new(self.world.spawn, self.world.spawn_yaw);
        self.player.frost = self.theme == Theme::Freezer;
        self.jelly.reset();
        self.rig.snap(self.world.spawn, self.world.spawn_yaw);
        self.rig.intro = INTRO_TIME;
        self.checkpoint = self.world.spawn;
        self.checkpoint_idx = None;
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

    /// Setzt einen gespeicherten Spielstand fort: Kirschen, Zeit, Platscher,
    /// zuletzt erreichter Checkpoint.
    pub fn resume(&mut self, p: &Progress) {
        self.restart();
        let mut k = 0;
        for it in &mut self.world.items {
            if it.kind == ItemKind::Cherry {
                if k < 64 && p.cherries_taken & (1u64 << k) != 0 {
                    it.taken = true;
                    it.pop = 10.0;
                    self.player.cherries += 1;
                }
                k += 1;
            }
        }
        self.time = p.time_ms as f32 / 1000.0;
        self.player.deaths = p.deaths as u32;
        self.title_t = 0.0;
        self.rig.intro = 0.0;
        if p.checkpoint > 0 {
            let idx = (p.checkpoint as usize - 1).min(self.world.checkpoints.len().saturating_sub(1));
            if let Some(cp) = self.world.checkpoints.get(idx) {
                let pos = cp.pos;
                for c in self.world.checkpoints.iter_mut().take(idx + 1) {
                    c.active = true;
                }
                self.checkpoint = pos;
                self.checkpoint_idx = Some(idx);
                self.respawn();
            }
        }
    }

    /// Aktueller Spielstand (für das Speichern).
    pub fn progress(&self, level: u8) -> Progress {
        let mut mask = 0u64;
        let mut k = 0;
        for it in &self.world.items {
            if it.kind == ItemKind::Cherry {
                if it.taken && k < 64 {
                    mask |= 1u64 << k;
                }
                k += 1;
            }
        }
        Progress {
            level,
            checkpoint: self.checkpoint_idx.map(|i| i as u8 + 1).unwrap_or(0),
            cherries_taken: mask,
            time_ms: (self.time * 1000.0) as u32,
            deaths: self.player.deaths.min(u16::MAX as u32) as u16,
            difficulty: self.difficulty.as_u8(),
        }
    }

    /// Schwierigkeit setzen: Tempo von Gegnern und Plattformen, Checkpoints.
    /// Vor [`PuddingGame::restart`] bzw. [`PuddingGame::resume`] aufrufen.
    pub fn set_difficulty(&mut self, d: Difficulty) {
        let old = self.difficulty;
        if d != old {
            self.world.scale_speeds(
                d.enemy_speed() / old.enemy_speed(),
                d.platform_speed() / old.platform_speed(),
                d.spinner_speed() / old.spinner_speed(),
            );
            self.difficulty = d;
        }
        let n = self.world.checkpoints.len();
        for (i, c) in self.world.checkpoints.iter_mut().enumerate() {
            c.enabled = d.checkpoint_enabled(i, n);
        }
    }

    /// Sekunden bis zum Schmelzen bzw. Einfrieren (im Kühlschrank langsamer).
    pub fn melt_time(&self) -> f32 {
        self.difficulty.melt_time() * if self.theme == Theme::Fridge { 1.5 } else { 1.0 }
    }

    fn die(&mut self) {
        self.player.alive = false;
        self.player.deaths += 1;
        self.phase = Phase::Dead { t: 0.0 };
    }

    /// Zurück zum letzten Checkpoint (die Hitze bleibt beim Zurücksetzen per Menü).
    pub fn respawn(&mut self) {
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
            self.events.push(Sfx::Jump);
            self.particles.burst(
                feet,
                Burst {
                    count: 4,
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
            if let Some(g) = self.player.body.ground
                && let Some(p) = self.world.platforms.iter_mut().find(|p| p.collider == g)
            {
                p.land(speed);
            }
            if speed > 3.0 {
                self.events.push(if speed > 12.0 { Sfx::LandHard } else { Sfx::Land });
            }
            if speed > 6.0 {
                let n = ((speed - 4.0) * 0.8) as usize;
                self.particles.burst(
                    feet,
                    Burst {
                        count: n.min(10),
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
            self.events.push(Sfx::Boing);
            self.messages.push("BOING!", 0xFFB0D8);
            self.particles.burst(
                feet,
                Burst {
                    count: 10,
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
                    self.events.push(if p.cherries.is_multiple_of(10) { Sfx::CherryBonus } else { Sfx::Cherry });
                    self.particles.burst(
                        it.pos,
                        Burst {
                            count: 8,
                            colors: &[pal::CHERRY, Color::hex(0xFFB0C0), pal::WHITE],
                            speed: 3.5,
                            up: 2.0,
                            life: 0.5,
                            size: 0.12,
                            gravity: 4.0,
                            ..Burst::default()
                        },
                    );
                    if p.cherries.is_multiple_of(10) {
                        let t = spatz::render::overlay::fmt::<48>(format_args!("{} KIRSCHEN!", p.cherries));
                        self.messages.push(t.as_str(), 0xFF6080);
                    }
                }
                ItemKind::Ice => {
                    p.melt = (p.melt - 0.45).max(0.0);
                    self.events.push(Sfx::Ice);
                    self.messages.push("BRRR! SCHÖN KÜHL!", 0x9EE0FF);
                    self.particles.burst(
                        it.pos,
                        Burst {
                            count: 12,
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
                ItemKind::Cocoa => {
                    p.melt = (p.melt - 0.5).max(0.0);
                    self.events.push(Sfx::Ice);
                    self.messages.push(pick(&mut self.rng, &["MMH, WARMER KAKAO!", "SCHÖN WARM!", "AAAH, KAKAO!"]), 0xFFB070);
                    self.particles.burst(
                        it.pos,
                        Burst {
                            count: 12,
                            colors: &[Color::hex(0xFF6A4A), pal::CREAM, Color::hex(0x8A4A28)],
                            speed: 3.0,
                            up: 2.5,
                            life: 0.8,
                            size: 0.12,
                            gravity: 1.0,
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
                    self.events.push(Sfx::Stomp);
                    self.messages
                        .push(pick(&mut self.rng, &["PLOPP!", "ZERMATSCHT!", "MAMPF!", "DONUT-PÜREE!"]), 0xFF7EB6);
                    self.particles.burst(
                        d.pos + Vec3::new(0.0, 0.3, 0.0),
                        Burst {
                            count: 14,
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
                    self.events.push(Sfx::Hit);
                    self.messages.push(pick(&mut self.rng, &["AUTSCH!", "AUA!", "HEY, DAS KLEBT!"]), 0xFF5050);
                    self.particles.burst(
                        pos + Vec3::new(0.0, 0.5, 0.0),
                        Burst { count: 6, colors: &[pal::VANILLA], speed: 4.0, ..Burst::default() },
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
                let push = Vec2::new(rel.y, -rel.x).normalize_or_zero() * sign(s.speed);
                let out = rel.normalize_or_zero();
                let dir = (push + out * 0.4).normalize_or_zero();
                p.launch(Vec3::new(dir.x * 9.0, 6.5, dir.y * 9.0));
                p.invuln = 1.0;
                p.melt = (p.melt + 0.05).min(0.99);
                self.events.push(Sfx::Whoosh);
                self.messages.push(pick(&mut self.rng, &["WUSCH!", "ZACK!", "HOMERUN!"]), 0xFF5050);
            }
        }

        for (ci, cp) in self.world.checkpoints.iter_mut().enumerate() {
            if cp.active || !cp.enabled {
                continue;
            }
            if (pos.xz() - cp.pos.xz()).length() < 1.1 && abs(pos.y - cp.pos.y) < 2.0 {
                cp.active = true;
                cp.wobble = 0.6;
                self.checkpoint = cp.pos;
                self.checkpoint_idx = Some(ci);
                self.checkpoint_reached = true;
                p.melt = 0.0;
                self.events.push(Sfx::Checkpoint);
                self.messages.push("CHECKPOINT! GESPEICHERT", 0x7CFF7C);
                self.particles.burst(
                    cp.pos + Vec3::new(0.0, 1.5, 0.0),
                    Burst {
                        count: 18,
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
            if (pos.xz() - g.xz()).length() < 1.4 && abs(pos.y - f.pos.y) < 1.2 {
                self.finish_time = Some(self.time);
                self.finish_from = pos;
                self.phase = Phase::Finished { t: 0.0 };
                p.body.vel = Vec3::ZERO;
                self.events.push(Sfx::Finish);
                return;
            }
        }

        // Herdplatten und brüchiges Eis unter den Füßen.
        if let (true, Some(g)) = (p.body.on_ground, p.body.ground) {
            self.world.step_on(g);
            if self.world.is_hot(g) {
                if self.hot_t == 0.0 {
                    self.events.push(Sfx::Hit);
                    self.messages.push(pick(&mut self.rng, &["AUTSCH, HEISS!", "HEISSE PLATTE!", "AUA, RUNTER DA!"]), 0xFF7030);
                }
                self.hot_t += dt;
                p.melt += dt * 0.5;
                if (self.hot_t * 10.0) as i32 != ((self.hot_t - dt) * 10.0) as i32 {
                    self.particles.burst(
                        pos + Vec3::new(0.0, 0.3, 0.0),
                        Burst {
                            count: 2,
                            colors: &[pal::WHITE, Color::hex(0xFFD0A0)],
                            speed: 1.2,
                            up: 3.0,
                            life: 0.6,
                            size: 0.14,
                            gravity: -1.0,
                            ..Burst::default()
                        },
                    );
                }
            } else {
                self.hot_t = 0.0;
            }
        } else {
            self.hot_t = 0.0;
        }

        // Schmelzen; Schweißtropfen bei großer Hitze.
        p.melt += dt / (self.difficulty.melt_time() * if self.theme == Theme::Fridge { 1.5 } else { 1.0 });
        if p.melt > 0.6 {
            self.fx_timer -= dt;
            if self.fx_timer <= 0.0 {
                self.fx_timer = 0.3 - p.melt * 0.15;
                self.particles.burst(
                    pos + Vec3::new(0.0, 0.9, 0.0),
                    Burst {
                        count: 1,
                        colors: {
                            const SNOW: [Color; 1] = [pal::WHITE];
                            const SWEAT: [Color; 1] = [Color::hex(0x8ED8FF)];
                            if p.frost { &SNOW } else { &SWEAT }
                        },
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
            self.events.push(Sfx::Melt);
            if p.frost {
                self.messages.push(pick(&mut self.rng, &["EINGEFROREN!", "PUDDING-EIS!", "ZU KALT!"]), 0x9EE0FF);
            } else {
                self.messages.push(pick(&mut self.rng, &["GESCHMOLZEN!", "PUDDING-SUPPE!", "ZU HEISS!"]), 0xFFB040);
            }
            self.particles.burst(
                pos,
                Burst {
                    count: 14,
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
            self.events.push(Sfx::Splash);
            let words: &[&str] = match self.theme {
                Theme::Kitchen => &["PLATSCH!", "BLUBB!", "SCHOKO-BAD!"],
                Theme::Fridge => &["PLATSCH!", "MILCHBAD!", "BLUBB!"],
                Theme::Freezer => &["PLATSCH!", "EISKALT!", "BIBBER!"],
                Theme::Buffet => &["PLATSCH!", "SAHNE-BAD!", "BLUBB!"],
            };
            self.messages.push(pick(&mut self.rng, words), 0xD08040);
            let at = Vec3::new(pos.x, self.world.sea_y + 0.3, pos.z);
            self.particles.burst(
                at,
                Burst {
                    count: 18,
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

    fn update_finish(&mut self, t: f32, dt: f32) {
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
            self.fx_timer = 0.15;
            let top = f.pos + Vec3::new(self.rng.range(-3.0, 3.0), 5.5, self.rng.range(-1.0, 3.0));
            self.particles.burst(
                top,
                Burst {
                    count: 5,
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
    }

    /// Schokoladensee als Wellengitter um die Kamera.
    fn animate_sea(&mut self, cam: Vec3, alpha: f32) {
        const CELLS: usize = crate::assets::SEA_CELLS;
        const SIZE: f32 = crate::assets::SEA_SIZE;
        let cell = SIZE / CELLS as f32;
        let cx = round(cam.x / cell) * cell;
        let cz = round(cam.z / cell) * cell;
        let t = self.world.render_time(alpha);
        let sea_y = self.world.sea_y;
        let x0 = cx - SIZE * 0.5;
        let z0 = cz - SIZE * 0.5;
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

    /// Beleuchtet den verformten Pudding für eine Kameraposition.
    pub fn relight_player(&mut self, alpha: f32, eye: Vec3) {
        let model = self.player.model(alpha);
        self.jelly.light(self.store.get_mut(self.assets.pudding), &model, &self.env, eye);
    }

    /// Baut die Zeichenliste für den Frame (mit Kamera `cam`).
    pub fn prepare_with(&mut self, alpha: f32, cam: &Camera) {
        self.animate_sea(cam.position, alpha);
        self.list.clear();
        self.world.draw(&mut self.list, &self.assets, alpha);
        if self.player.alive {
            self.jelly.skin(self.store.get_mut(self.assets.pudding), alpha);
            self.relight_player(alpha, cam.position);
        }
        self.player.draw(&mut self.list, &self.assets, alpha, &self.jelly);
        self.draw_shadows(alpha);
        // Der See liegt immer hinter allem → zuletzt zeichnen.
        let sea = DrawParams { sort_bias: 1.0e6, ..DrawParams::OPAQUE };
        self.list.push(self.assets.sea, Mat4::IDENTITY, sea);
        self.particles.draw(&mut self.list, alpha);
    }

    /// Zeichenliste mit der Spielkamera; liefert die Kamera.
    pub fn prepare(&mut self, alpha: f32) -> Camera {
        let cam = self.game_camera(alpha);
        self.prepare_with(alpha, &cam);
        cam
    }

    /// Kamera der Verfolger-Steuerung (kürzere Sichtweite als am PC).
    pub fn game_camera(&self, alpha: f32) -> Camera {
        let mut cam = self.rig.camera(alpha);
        cam.far = 80.0;
        cam
    }

    /// Ein Simulationsschritt mit gegebenen Steuerbefehlen.
    pub fn step(&mut self, c: &Controls, dt: f32) {
        self.title_t = (self.title_t - dt).max(0.0);
        match self.phase {
            Phase::Playing => {
                self.world.update(dt);
                self.time += dt;
                let ev = self.player.update(c, self.rig.yaw, &self.world, dt);
                self.effects_for(ev);
                self.interact(dt);
                self.jelly.step(&self.player, dt);
                if self.jelly.giggle_started() {
                    self.events.push(Sfx::Giggle);
                }
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
                self.update_finish(t + dt, dt);
                self.jelly.step(&self.player, dt);
            }
        }
        let turn = if self.autopilot.is_some() { 0.0 } else { c.cam_turn };
        self.rig.update(&self.player, turn, dt, &self.world.collision);
        self.particles.update(dt);
        self.messages.update(dt);
    }

    /// Steuerbefehle vom Autopiloten (Demo/Test) oder vom Spieler.
    pub fn controls(&mut self, user: Controls, dt: f32) -> Controls {
        match &mut self.autopilot {
            Some(ap) => ap.controls(&self.world, &self.player, self.rig.yaw, dt),
            None => user,
        }
    }

    pub fn finished_t(&self) -> Option<f32> {
        if let Phase::Finished { t } = self.phase { Some(t) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{self, LEVELS};

    /// Der Autopilot spielt jedes Level durch (prüft, dass alle Level
    /// schaffbar sind – auch nach Änderungen an Physik oder Leveln).
    #[test]
    fn autopilot_beats_every_level() {
        for (i, (name, text)) in LEVELS.iter().enumerate() {
            let def = level::parse(text).unwrap();
            let mut g = PuddingGame::new(&def, true);
            let dt = 1.0 / 120.0;
            for _ in 0..(120 * 120) {
                let c = g.controls(Controls::default(), dt);
                g.step(&c, dt);
                if g.finish_time.is_some() {
                    break;
                }
            }
            let t = g.finish_time.unwrap_or_else(|| {
                panic!("Level {} ({name}) nicht geschafft: steckt bei {:?} fest", i + 1, g.player.body.pos)
            });
            std::println!(
                "Level {} {name}: {:.1} s, {} Platscher, {}/{} Kirschen",
                i + 1,
                t,
                g.player.deaths,
                g.player.cherries,
                g.total_cherries
            );
            assert_eq!(g.player.deaths, 0, "Level {} soll ohne Platscher schaffbar sein", i + 1);
        }
    }

    #[test]
    fn progress_roundtrip_restores_cherries_and_checkpoint() {
        let def = level::parse(LEVELS[0].1).unwrap();
        let mut g = PuddingGame::new(&def, true);
        let dt = 1.0 / 120.0;
        while g.checkpoint_idx.is_none() {
            let c = g.controls(Controls::default(), dt);
            g.step(&c, dt);
        }
        let p = g.progress(0);
        let cherries = g.player.cherries;
        let mut h = PuddingGame::new(&def, false);
        h.resume(&p);
        assert_eq!(h.player.cherries, cherries);
        assert_eq!(h.checkpoint_idx, g.checkpoint_idx);
        assert!((h.player.body.pos - (g.checkpoint + Vec3::new(0.0, 0.3, 0.0))).length() < 0.01);
        assert_eq!(h.progress(0).cherries_taken, p.cherries_taken);
    }
}
