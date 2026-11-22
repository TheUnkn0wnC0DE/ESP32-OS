//! Einfaches Partikelsystem (Staub, Spritzer, Funken, Konfetti) als Sprites.

use alloc::vec::Vec;
use spatz::prelude::*;

pub struct Particle {
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub color: Color,
    pub gravity: f32,
    pub drag: f32,
}

pub struct Particles {
    pub list: Vec<Particle>,
    rng: Rng,
}

pub const MAX_PARTICLES: usize = 900;

/// Parameter einer Partikelwolke.
#[derive(Clone, Copy)]
pub struct Burst<'a> {
    pub count: usize,
    pub colors: &'a [Color],
    pub speed: f32,
    pub up: f32,
    pub life: f32,
    pub size: f32,
    pub gravity: f32,
    pub spread: f32,
}

impl Default for Burst<'_> {
    fn default() -> Self {
        Self {
            count: 10,
            colors: &[Color::WHITE],
            speed: 3.0,
            up: 2.0,
            life: 0.6,
            size: 0.15,
            gravity: 9.0,
            spread: 0.2,
        }
    }
}

impl Particles {
    pub fn new() -> Self {
        Self { list: Vec::with_capacity(MAX_PARTICLES), rng: Rng::new(0xC0FFEE) }
    }

    pub fn burst(&mut self, pos: Vec3, b: Burst) {
        for _ in 0..b.count {
            if self.list.len() >= MAX_PARTICLES {
                self.list.swap_remove(0);
            }
            let r = &mut self.rng;
            let dir = Vec3::new(r.range(-1.0, 1.0), r.range(-0.2, 1.0), r.range(-1.0, 1.0)).normalize_or(Vec3::Y);
            let off = Vec3::new(r.range(-1.0, 1.0), r.range(-1.0, 1.0), r.range(-1.0, 1.0)) * b.spread;
            let life = b.life * r.range(0.6, 1.2);
            let color = b.colors[r.below(b.colors.len() as u32) as usize];
            let p = pos + off;
            self.list.push(Particle {
                pos: p,
                prev: p,
                vel: dir * (b.speed * r.range(0.5, 1.0)) + Vec3::new(0.0, b.up, 0.0),
                life,
                max_life: life,
                size: b.size * r.range(0.7, 1.3),
                color,
                gravity: b.gravity,
                drag: 1.5,
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.list.len() {
            let p = &mut self.list[i];
            p.life -= dt;
            if p.life <= 0.0 {
                self.list.swap_remove(i);
                continue;
            }
            p.prev = p.pos;
            p.vel.y -= p.gravity * dt;
            p.vel *= 1.0 - (p.drag * dt).min(0.5);
            p.pos += p.vel * dt;
            i += 1;
        }
    }

    pub fn draw(&self, list: &mut DrawList, alpha: f32) {
        for p in &self.list {
            let t = (p.life / p.max_life).clamp(0.0, 1.0);
            list.sprite(p.prev.lerp(p.pos, alpha), p.size * (0.4 + 0.6 * t), p.color);
        }
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

impl Default for Particles {
    fn default() -> Self {
        Self::new()
    }
}
