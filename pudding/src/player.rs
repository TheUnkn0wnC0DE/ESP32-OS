//! Die Spielfigur: ein wackeliger Vanillepudding.
//!
//! Steuerungsgefühl: Beschleunigung statt Sofort-Tempo, Coyote-Time,
//! Sprungpuffer, variable Sprunghöhe (Taste kurz = kleiner Hüpfer),
//! schnelleres Fallen. Optik: echter Weichkörper (siehe `jelly.rs`),
//! Glubschaugen mit Feder-Physik, Schmelzen.

use crate::assets::Assets;
use crate::jelly::{GIGGLE_TIME, PuddingJelly};
use crate::world::{TAG_MALLOW, World};
use spatz::anim::fast_sin;
use spatz::input::{Input, Key, PAD_A, PAD_LB, PAD_RB};
use spatz::math::{Vec2, angle_diff, atan2, cos, damp_factor, sin};
use spatz::physics::CharacterBody;
use spatz::prelude::*;

pub const RUN_SPEED: f32 = 7.0;
pub const GROUND_ACCEL: f32 = 60.0;
pub const AIR_ACCEL: f32 = 26.0;
pub const FRICTION: f32 = 48.0;
pub const GRAVITY: f32 = 32.0;
pub const FALL_MULT: f32 = 1.25;
pub const JUMP_CUT_MULT: f32 = 2.4;
pub const JUMP_VEL: f32 = 12.5;
pub const MAX_FALL: f32 = 28.0;
pub const COYOTE: f32 = 0.10;
pub const JUMP_BUFFER: f32 = 0.13;
pub const MALLOW_VEL: f32 = 21.0;
pub const STOMP_VEL: f32 = 11.0;
/// Sekunden bis zum vollständigen Schmelzen.
pub const MELT_TIME: f32 = 80.0;

/// Abstrakte Steuerbefehle (Tastatur, Gamepad oder Autopilot).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// Seitwärts (-1 links .. 1 rechts), relativ zur Kamera.
    pub move_x: f32,
    /// Vorwärts (-1 zurück .. 1 vor), relativ zur Kamera.
    pub move_y: f32,
    pub jump_pressed: bool,
    pub jump_held: bool,
    /// Kameradrehung (-1 links .. 1 rechts).
    pub cam_turn: f32,
}

impl Controls {
    pub fn from_input(i: &Input) -> Self {
        let mut mx = i.axis(Key::A, Key::D) + i.axis(Key::Left, Key::Right);
        let mut my = i.axis(Key::S, Key::W) + i.axis(Key::Down, Key::Up);
        let mut turn = i.axis(Key::Q, Key::E);
        if i.pad.connected {
            mx += i.pad.left.0;
            my += i.pad.left.1;
            turn += i.pad.right.0;
            if i.pad.down(PAD_LB) {
                turn -= 1.0;
            }
            if i.pad.down(PAD_RB) {
                turn += 1.0;
            }
        }
        let jump_keys = [Key::Space, Key::J, Key::K];
        Controls {
            move_x: mx.clamp(-1.0, 1.0),
            move_y: my.clamp(-1.0, 1.0),
            jump_pressed: i.any_pressed(&jump_keys) || i.pad.pressed(PAD_A),
            jump_held: i.any_down(&jump_keys) || i.pad.down(PAD_A),
            cam_turn: turn.clamp(-1.0, 1.0),
        }
    }
}

/// Ereignisse eines Schritts (für Effekte und Logik im Spiel).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerEvents {
    pub jumped: bool,
    pub landed: Option<f32>,
    pub bounced_on: Option<usize>,
}

pub struct Player {
    pub body: CharacterBody,
    pub prev_pos: Vec3,
    pub yaw: f32,
    pub prev_yaw: f32,
    coyote: f32,
    jump_buffer: f32,
    jumping: bool,
    /// Pupillen-Versatz (Glubschaugen) und dessen Geschwindigkeit.
    pub eye: Vec2,
    eye_vel: Vec2,
    prev_vel: Vec3,
    /// 0 = frisch, 1 = geschmolzen.
    pub melt: f32,
    /// Unverwundbar nach Treffer (Sekunden).
    pub invuln: f32,
    pub alive: bool,
    pub cherries: u32,
    pub deaths: u32,
    pub air_time: f32,
}

impl Player {
    pub fn new(pos: Vec3, yaw: f32) -> Self {
        let mut body = CharacterBody::new(pos, 0.42, 0.9);
        body.step_height = 0.3;
        Self {
            body,
            prev_pos: pos,
            yaw,
            prev_yaw: yaw,
            coyote: 0.0,
            jump_buffer: 0.0,
            jumping: false,
            eye: Vec2::new(0.0, -0.03),
            eye_vel: Vec2::ZERO,
            prev_vel: Vec3::ZERO,
            melt: 0.0,
            invuln: 0.0,
            alive: true,
            cherries: 0,
            deaths: 0,
            air_time: 0.0,
        }
    }

    /// Setzt die Figur an einen Ort zurück (Zähler bleiben erhalten).
    pub fn respawn(&mut self, pos: Vec3, yaw: f32) {
        let (cherries, deaths) = (self.cherries, self.deaths);
        *self = Player::new(pos, yaw);
        self.cherries = cherries;
        self.deaths = deaths;
    }

    /// Stoß von außen (Treffer, Trampolin, Gegner plattmachen).
    pub fn launch(&mut self, vel: Vec3) {
        self.body.vel = vel;
        self.body.on_ground = false;
        self.body.ground = None;
        self.jumping = false;
        self.coyote = 0.0;
    }

    pub fn update(&mut self, c: &Controls, cam_yaw: f32, world: &World, dt: f32) -> PlayerEvents {
        let mut ev = PlayerEvents::default();
        self.prev_pos = self.body.pos;
        self.prev_yaw = self.yaw;
        self.invuln = (self.invuln - dt).max(0.0);

        // Wunschrichtung relativ zur Kamera.
        let fwd = Vec2::new(-sin(cam_yaw), -cos(cam_yaw));
        let right = Vec2::new(cos(cam_yaw), -sin(cam_yaw));
        let wish = (right * c.move_x + fwd * c.move_y).clamp_length(1.0);
        let target = wish * RUN_SPEED;
        let v = Vec2::new(self.body.vel.x, self.body.vel.z);
        let accel =
            if self.body.on_ground { if wish.length_sq() > 0.01 { GROUND_ACCEL } else { FRICTION } } else { AIR_ACCEL };
        let dv = target - v;
        let max = accel * dt;
        let nv = if dv.length() > max { v + dv.normalize_or_zero() * max } else { target };
        self.body.vel.x = nv.x;
        self.body.vel.z = nv.y;

        // Springen mit Puffer und Coyote-Time.
        self.jump_buffer = if c.jump_pressed { JUMP_BUFFER } else { (self.jump_buffer - dt).max(0.0) };
        self.coyote = if self.body.on_ground { COYOTE } else { (self.coyote - dt).max(0.0) };
        if self.jump_buffer > 0.0 && self.coyote > 0.0 {
            self.body.vel.y = JUMP_VEL;
            self.body.on_ground = false;
            self.body.ground = None;
            self.jump_buffer = 0.0;
            self.coyote = 0.0;
            self.jumping = true;
            ev.jumped = true;
        }

        // Schwerkraft: schneller fallen, kurzer Hüpfer beim Loslassen.
        let mut g = GRAVITY;
        if self.body.vel.y < 0.0 {
            g *= FALL_MULT;
            self.jumping = false;
        } else if self.jumping && !c.jump_held {
            g *= JUMP_CUT_MULT;
        }
        self.body.vel.y = (self.body.vel.y - g * dt).max(-MAX_FALL);

        let res = self.body.step(&world.collision, dt);
        if res.hit_ceiling {
            self.jumping = false;
        }
        if res.landed {
            ev.landed = Some(res.landing_speed);
        }
        self.air_time = if self.body.on_ground { 0.0 } else { self.air_time + dt };

        // Marshmallow-Trampolin.
        if let (true, Some(g)) = (self.body.on_ground, self.body.ground) {
            if world.collision.colliders[g].tag == TAG_MALLOW {
                self.launch(Vec3::new(self.body.vel.x, MALLOW_VEL, self.body.vel.z));
                ev.bounced_on = Some(g);
            }
        }

        // Blickrichtung folgt der Bewegung.
        let hv = Vec2::new(self.body.vel.x, self.body.vel.z);
        let speed = hv.length();
        if speed > 0.6 {
            let target_yaw = atan2(-hv.x, -hv.y);
            self.yaw += angle_diff(self.yaw, target_yaw) * damp_factor(0.05, dt);
        }

        // Glubschaugen: Pupillen hängen der Beschleunigung hinterher.
        let a = (self.body.vel - self.prev_vel) / dt;
        self.prev_vel = self.body.vel;
        let (s, co) = (sin(self.yaw), cos(self.yaw));
        let right_m = Vec3::new(co, 0.0, -s);
        let a_local = Vec2::new(a.dot(right_m), a.y).clamp_length(80.0);
        let rest = Vec2::new(0.0, -0.025);
        let eye_acc = (rest - self.eye) * 320.0 - self.eye_vel * 6.0 - a_local * 0.01;
        self.eye_vel += eye_acc * dt;
        self.eye += self.eye_vel * dt;
        let max_r = 0.055;
        if self.eye.length() > max_r {
            let n = self.eye.normalize_or_zero();
            self.eye = n * max_r;
            let vn = self.eye_vel.dot(n);
            if vn > 0.0 {
                self.eye_vel -= n * (vn * 1.6);
            }
        }
        ev
    }

    /// Fügt die Figur der Zeichenliste hinzu. Das Mesh hat `jelly` vorher
    /// verformt ([`PuddingJelly::skin`]); Augen und Mund kleben mit der lokalen
    /// Verzerrung auf der Oberfläche.
    pub fn draw(&self, list: &mut DrawList, a: &Assets, alpha: f32, jelly: &PuddingJelly) {
        if !self.alive {
            return;
        }
        if self.invuln > 0.0 && ((self.invuln * 12.0) as i32) % 2 == 0 {
            return;
        }
        let model = self.model(alpha);
        // Licht samt Glanz ist schon eingerechnet ([`PuddingJelly::light`]);
        // Schmelzen färbt zusätzlich.
        let tint = [1.0 + 0.12 * self.melt, 1.0, 1.0 - 0.25 * self.melt];
        list.push(a.pudding, model, DrawParams { tint, unlit: true, ..DrawParams::OPAQUE });
        let scaled = |f: Mat4, s: Vec3| model * f * Mat4::from_trs(Vec3::ZERO, Quat::IDENTITY, s);
        // Beim Kichern: Augen zu Schlitzen, Mund lacht im Takt.
        let giggle = (jelly.giggle / GIGGLE_TIME).clamp(0.0, 1.0);
        let squint = 1.0 - 0.6 * (giggle * 5.0).min(1.0);
        for i in 0..2 {
            let eye = scaled(jelly.face(i), Vec3::new(1.0, squint, 1.0));
            list.push(a.eye_white, eye, DrawParams::unlit());
            let p = Vec3::new(self.eye.x, self.eye.y, -0.012);
            list.push(a.pupil, eye * Mat4::translation(p), DrawParams::unlit());
        }
        let laugh = if giggle > 0.0 { 0.5 + 0.5 * fast_sin(jelly.giggle * 57.0).abs() } else { 0.0 };
        let open = (self.body.vel.y.abs() / 10.0).min(1.0).max(laugh);
        let mouth = scaled(jelly.face(2), Vec3::new(1.4 - 0.5 * open, 0.45 + 0.8 * open, 1.0));
        list.push(a.mouth, mouth, DrawParams::unlit());
    }

    /// Lage der Figur für den Frame: interpoliert, Schmelzen macht sie flacher
    /// und breiter (der Weichkörper wird zudem weicher).
    pub fn model(&self, alpha: f32) -> Mat4 {
        let pos = self.prev_pos.lerp(self.body.pos, alpha);
        let yaw = self.prev_yaw + angle_diff(self.prev_yaw, self.yaw) * alpha;
        let melt = Vec3::new(1.0 + 0.18 * self.melt, 1.0 - 0.32 * self.melt, 1.0 + 0.18 * self.melt);
        Mat4::from_trs(pos, Quat::from_rotation_y(yaw), melt)
    }

    /// Interpolierte Position für Kamera/Schatten.
    pub fn render_pos(&self, alpha: f32) -> Vec3 {
        self.prev_pos.lerp(self.body.pos, alpha)
    }
}
