//! Der Pudding als echter Weichkörper.
//!
//! Ein grobes Partikelgitter (4 Höhenstufen × 6 Ringpunkte + Achse) wird mit
//! XPBD simuliert – im mitbewegten Bezugssystem der Spielfigur. Was die Figur
//! tut, spürt die Gelatine als Trägheitskraft, genau wie ein echter Pudding
//! auf einem Teller:
//!
//! * Anfahren/Bremsen: die Oberseite bleibt zurück bzw. schwappt nach vorn,
//! * Umdrehen: die Oberseite hinkt hinterher und verdreht sich,
//! * Landen: der Körper sackt zusammen, wölbt sich seitlich (Volumen bleibt
//!   erhalten), federt hoch und schwingt aus,
//! * Abspringen: erst gestaucht, in der Luft schwerelos und gestreckt,
//! * die Unterseite klebt am Boden, die Sahnehaube und die Kirsche sind eigene
//!   Massen und schwingen phasenversetzt nach.
//!
//! Das Darstellungs-Mesh wird glatt auf das Gitter abgebildet (kubisch über
//! die Höhe); Augen und Mund kleben mit der lokalen Verzerrung auf der
//! Oberfläche. Alles rein optisch – der Spielzustand bleibt unberührt.

use crate::assets::{CHERRY_POS, CREAM_POS, PUDDING_H, PuddingParts, quat_from_to};
use crate::player::{GRAVITY, Player};
use alloc::vec;
use alloc::vec::Vec;
use spatz::math::{Vec2, angle_diff, cos, sin};
use spatz::prelude::*;
use spatz::render::shapes::recompute_normals;
use spatz::render::{Material, light_faces};
use spatz::softbody::{CageBinding, FrameMotion, LatheCage, SoftBody};

/// Gelatine glänzt und lässt Licht durch (die Schattenseite leuchtet warm).
const LOOK: Material = Material { gloss: 0.7, translucency: 0.45 };

/// Höhenstufen des Gitters (Höhe, Radius) – folgen der Puddingform.
const LEVELS: [(f32, f32); 4] = [(0.0, 0.445), (0.28, 0.452), (0.56, 0.41), (PUDDING_H, 0.30)];
const RING: usize = 6;
/// Nachgiebigkeit der Gelatine (abgestimmt: ~2,6 Hz Schwanken, ~5,6 Hz Federn).
const GEL_COMPLIANCE: f32 = 2.5e-4;
/// Sahne sitzt weicher auf dem Karamell, die Kirsche hängt fest an der Sahne
/// und wird nur schwach aufrecht gehalten (Pendel).
const CREAM_COMPLIANCE: f32 = 1.2e-3;
const CHERRY_STEM_COMPLIANCE: f32 = 2e-5;
const CHERRY_UPRIGHT_COMPLIANCE: f32 = 6e-3;
/// Wie stark die Bewegungen der Figur als Trägheitskraft durchschlagen
/// (1 = streng physikalisch; das Spiel springt viel wilder als ein Teller).
const INERTIA: f32 = 0.3;
/// Größte Geschwindigkeitsänderung pro Schritt, die durchschlägt (m/s).
const MAX_DV: f32 = 10.0;
/// Anteil der Drehung der Figur, den die Gelatine träge zurückbleibt.
const SPIN: f32 = 0.35;
/// Versteifung bei großer Dehnung (siehe `SoftParams::stiffening`).
const STIFFENING: f32 = 40.0;
/// Kicheranfall im Stand: Dauer, Abstand der „Hi“s, Wartezeit davor.
pub const GIGGLE_TIME: f32 = 0.8;
const GIGGLE_TICK: f32 = 0.11;
const GIGGLE_AFTER: f32 = 2.2;

/// Anbaupunkte im Gesicht (Ruhelage, Modellraum).
pub const EYE_POS: [Vec3; 2] = [Vec3::new(-0.16, 0.56, -0.43), Vec3::new(0.16, 0.56, -0.43)];
pub const MOUTH_POS: Vec3 = Vec3::new(0.0, 0.33, -0.445);

pub struct PuddingJelly {
    body: SoftBody,
    cage: LatheCage,
    cream: usize,
    cherry: usize,
    /// Partikellagen des vorigen Schritts und interpoliert für den Frame.
    prev: Vec<Vec3>,
    lerp: Vec<Vec3>,
    parts: PuddingParts,
    rest_mesh: Vec<Vec3>,
    /// Unbeleuchtete Flächenfarben (das Mesh bekommt pro Frame beleuchtete).
    base_colors: Vec<Color>,
    body_bind: Vec<CageBinding>,
    top_bind: [CageBinding; 4],
    face_bind: [[CageBinding; 4]; 3],
    face: [Mat4; 3],
    prev_vel: Vec3,
    fresh: bool,
    idle: f32,
    /// Restdauer des Kicherns (für Augen und Mund).
    pub giggle: f32,
    giggle_tick: f32,
    /// Ein Kicheranfall hat gerade begonnen (für den Kicher-Klang).
    giggle_event: bool,
    step_phase: f32,
}

impl PuddingJelly {
    /// Baut den Weichkörper zum Pudding-Mesh (`parts` aus den Assets) und
    /// vergrößert dessen Hüllkugel um den größten Ausschlag.
    pub fn new(mesh: &mut Mesh, parts: PuddingParts) -> Self {
        mesh.bounds.radius += 0.5;
        let mut body = SoftBody::new();
        body.params.stiffening = STIFFENING;
        // Auf dem ESP32-P4 kostet der Weichkörper den Großteil der Simulation;
        // 2 statt 3 Unterschritte wackeln praktisch gleich (gemessen: mittlerer
        // Ausschlag 0,045 statt 0,052), sparen aber ein Drittel.
        #[cfg(target_os = "espidf")]
        {
            body.params.substeps = 2;
        }
        let cage = LatheCage::build(&mut body, &LEVELS, RING, 1.0, GEL_COMPLIANCE);
        // Die Unterseite klebt am Boden.
        body.add_anchor(cage.center(0));
        for i in 0..RING {
            body.add_anchor(cage.node(0, i));
        }
        let top = LEVELS.len() - 1;
        let cream = body.add_particle(CREAM_POS, 0.6);
        let cherry = body.add_particle(CHERRY_POS, 0.25);
        body.add_edge(cream, cage.center(top), CREAM_COMPLIANCE);
        body.add_edge(cherry, cream, CHERRY_STEM_COMPLIANCE);
        for i in 0..RING {
            body.add_edge(cream, cage.node(top, i), CREAM_COMPLIANCE);
            body.add_edge(cherry, cage.node(top, i), CHERRY_UPRIGHT_COMPLIANCE);
        }
        let rest_mesh = mesh.positions.clone();
        let body_bind = rest_mesh[parts.body.clone()].iter().map(|&p| cage.bind(&body, p)).collect();
        let top_bind = cage.bind_frame(&body, Vec3::new(0.0, PUDDING_H, 0.0));
        let face_bind =
            [cage.bind_frame(&body, EYE_POS[0]), cage.bind_frame(&body, EYE_POS[1]), cage.bind_frame(&body, MOUTH_POS)];
        let n = body.len();
        Self {
            prev: body.rest.clone(),
            lerp: vec![Vec3::ZERO; n],
            body,
            cage,
            cream: cream as usize,
            cherry: cherry as usize,
            parts,
            rest_mesh,
            base_colors: mesh.colors.clone(),
            body_bind,
            top_bind,
            face_bind,
            face: [Mat4::IDENTITY; 3],
            prev_vel: Vec3::ZERO,
            fresh: true,
            idle: 0.0,
            giggle: 0.0,
            giggle_tick: 0.0,
            giggle_event: false,
            step_phase: 0.0,
        }
    }

    /// `true` einmalig, wenn seit der letzten Abfrage ein Kicheranfall begann.
    pub fn giggle_started(&mut self) -> bool {
        core::mem::take(&mut self.giggle_event)
    }

    /// Zurück in die Ruhelage (Neustart, Checkpoint).
    pub fn reset(&mut self) {
        self.body.reset();
        self.prev.copy_from_slice(&self.body.rest);
        self.prev_vel = Vec3::ZERO;
        self.fresh = true;
        self.idle = 0.0;
        self.giggle = 0.0;
    }

    /// Ein Simulationsschritt (fester Takt) passend zur Bewegung der Figur.
    pub fn step(&mut self, p: &Player, dt: f32) {
        let moved = p.body.pos - p.prev_pos;
        if moved.length() > 3.0 {
            // Teleport (Respawn o. Ä.): nicht als Ruck werten.
            self.reset();
        }
        self.prev.copy_from_slice(&self.body.pos);
        // Beschleunigung der Figur aus der tatsächlichen Bewegung (inkl.
        // Landungen, Stufen, mitfahrender Plattformen), begrenzt.
        let v = moved / dt;
        let mut dv = if self.fresh { Vec3::ZERO } else { v - self.prev_vel };
        self.prev_vel = v;
        self.fresh = false;
        let l = dv.length();
        if l > MAX_DV {
            dv *= MAX_DV / l;
        }
        // Welt → Modellraum (x = rechts, z = hinten).
        let (s, c) = (sin(p.yaw), cos(p.yaw));
        let a = Vec3::new(0.0, -GRAVITY, 0.0) - dv / dt;
        let accel = Vec3::new(a.x * c - a.z * s, a.y, a.x * s + a.z * c) * INERTIA;
        // Geschmolzen ist er weicher.
        self.body.params.softness = 1.0 + 1.5 * p.melt;
        self.giggles(p, dt);
        // Die Figur ist der „Teller“: die Unterseite bleibt immer haften
        // (auch in der Luft – sonst kippt der ganze Pudding), der Boden
        // drückt nur, wenn die Figur auch steht.
        self.body.step(
            dt,
            &FrameMotion {
                accel,
                spin: angle_diff(p.prev_yaw, p.yaw) * SPIN,
                anchored: true,
                ground: p.body.on_ground.then_some(0.0),
            },
        );
    }

    /// Kicheranfall im Stand und Watscheln beim Laufen: kleine Stöße, auf die
    /// die Gelatine mit echtem Nachschwingen antwortet.
    fn giggles(&mut self, p: &Player, dt: f32) {
        let speed = Vec2::new(p.body.vel.x, p.body.vel.z).length();
        let still = p.body.on_ground && speed < 0.3;
        self.idle = if still { self.idle + dt } else { 0.0 };
        if self.idle > GIGGLE_AFTER {
            self.idle = 0.0;
            self.giggle = GIGGLE_TIME;
            self.giggle_tick = 0.0;
            self.giggle_event = true;
        }
        let inv_h = 1.0 / PUDDING_H;
        if self.giggle > 0.0 {
            self.giggle -= dt;
            self.giggle_tick -= dt;
            if self.giggle_tick <= 0.0 {
                self.giggle_tick += GIGGLE_TICK;
                let k = 0.4 + (self.giggle / GIGGLE_TIME).max(0.0);
                let side = if (self.giggle / GIGGLE_TICK) as i32 % 2 == 0 { 1.0 } else { -1.0 };
                self.body.push(|r| {
                    let h = r.y * inv_h;
                    Vec3::new(side * 0.6 * k * h * h, 1.1 * k * h, 0.0)
                });
            }
        }
        if p.body.on_ground && speed > 1.0 {
            let before = self.step_phase as i32;
            self.step_phase += speed * dt * 0.55;
            let now = self.step_phase as i32;
            if now != before {
                let side = if now % 2 == 0 { 1.0 } else { -1.0 };
                self.body.push(|r| {
                    let h = r.y * inv_h;
                    Vec3::new(side * 0.3 * h * h, 0.35 * h, 0.0)
                });
            }
            if self.step_phase > 1000.0 {
                self.step_phase -= 1000.0;
            }
        }
    }

    /// Verformt das Pudding-Mesh für den Frame (zwischen zwei Schritten
    /// interpoliert) und merkt sich die Rahmen für Augen und Mund.
    pub fn skin(&mut self, mesh: &mut Mesh, alpha: f32) {
        for ((dst, &a), &b) in self.lerp.iter_mut().zip(&self.prev).zip(&self.body.pos) {
            *dst = a + (b - a) * alpha;
        }
        let pos = &self.lerp;
        let out = &mut mesh.positions;
        for (k, b) in self.body_bind.iter().enumerate() {
            out[self.parts.body.start + k] = self.cage.map(pos, b);
        }
        // Sahne: eigenes Partikel, kippt und dehnt sich mit der Oberseite.
        let top = self.cage.frame(pos, &self.top_bind);
        let cream = pos[self.cream];
        for v in self.parts.cream.clone() {
            out[v] = cream + top.transform_vector(self.rest_mesh[v] - CREAM_POS);
        }
        // Kirsche: Pendel auf der Sahne, der Stiel zeigt von der Sahne weg.
        let dir = (pos[self.cherry] - cream).normalize_or(Vec3::Y);
        let q = quat_from_to(Vec3::Y, dir);
        let cherry = pos[self.cherry];
        for v in self.parts.cherry.clone() {
            out[v] = cherry + q.rotate(self.rest_mesh[v] - CHERRY_POS);
        }
        recompute_normals(mesh);
        for (f, b) in self.face.iter_mut().zip(&self.face_bind) {
            *f = self.cage.frame(pos, b);
        }
    }

    /// Beleuchtet das verformte Mesh vorab mit Glanz und Durchscheinen (danach
    /// unbeleuchtet zeichnen). `model` = Lage der Figur, `eye` = Kamera.
    pub fn light(&self, mesh: &mut Mesh, model: &Mat4, env: &Environment, eye: Vec3) {
        let Mesh { colors, normals, .. } = mesh;
        light_faces(colors, &self.base_colors, normals, model, env, eye, &LOOK);
    }

    /// Lokaler Rahmen (Modellraum) für Auge 0/1 bzw. Mund 2 aus dem letzten [`Self::skin`].
    pub fn face(&self, i: usize) -> Mat4 {
        self.face[i]
    }

    /// Relatives Volumen (1 = Ruhelage) – für Tests.
    #[cfg(test)]
    pub fn volume_ratio(&self) -> f32 {
        self.body.volume_ratio()
    }

    /// Auslenkung der Oberseite gegenüber der Ruhelage – für Tests.
    #[cfg(test)]
    pub fn top_offset(&self) -> Vec3 {
        let i = self.cage.center(LEVELS.len() - 1) as usize;
        self.body.pos[i] - self.body.rest[i]
    }
}

#[cfg(test)]
mod tests {
    use crate::level;
    use crate::play::PuddingGame;
    use crate::player::Controls;

    /// Über den ganzen Autopilot-Lauf: der Weichkörper bleibt stabil, behält
    /// sein Volumen und schwingt sichtbar, aber nicht übertrieben.
    #[test]
    fn jelly_stays_sane_over_a_full_run() {
        let def = level::parse(level::BUILTIN).unwrap();
        let mut game = PuddingGame::new(&def, true);
        let (mut max_side, mut max_down, mut max_up, mut max_vol) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut sum_side = 0.0f32;
        let mut n = 0;
        for _ in 0..(120 * 60) {
            let c = game.controls(Controls::default(), 1.0 / 120.0);
            game.step(&c, 1.0 / 120.0);
            let o = game.jelly.top_offset();
            assert!(o.x.is_finite() && o.y.is_finite() && o.z.is_finite());
            let side = (o.x * o.x + o.z * o.z).sqrt();
            max_side = max_side.max(side);
            sum_side += side;
            n += 1;
            max_down = max_down.max(-o.y);
            max_up = max_up.max(o.y);
            max_vol = max_vol.max((game.jelly.volume_ratio() - 1.0).abs());
            if game.finish_time.is_some() {
                break;
            }
        }
        std::println!(
            "Oberseite: seitlich max {max_side:.3} mittel {:.3}, runter {max_down:.3}, hoch {max_up:.3}; Volumen max {:.1} %",
            sum_side / n as f32,
            max_vol * 100.0
        );
        assert!(max_side > 0.04, "soll sichtbar wackeln");
        assert!(max_side < 0.4 && max_down < 0.4 && max_up < 0.3, "nicht übertreiben");
        assert!(max_vol < 0.15, "Volumen bleibt (fast) erhalten");
    }
}
