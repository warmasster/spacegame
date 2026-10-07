//! Particles: looks as data (`particles.jsonc`, one style per name) and a CPU simulation with no
//! allocation after creation. Ballistic flight under the gravity where each particle is, with
//! inherited drift kept in double precision apart from its expansion (drag and appearance).
//! On the ground a particle bounces and slides,
//! skims along it slowing down (dust sheets), or vanishes. The renderer only reads
//! `list` and the styles.
use crate::body::{BodyId, BodyRegistry};
use glam::{DVec3, Vec3};
use rayon::prelude::*;
use serde::Deserialize;

/// Styles the renderer's table holds (the same number in `render/shaders/particles.wgsl`).
pub const MAX_STYLES: usize = 24;

/// What a particle does when it meets the ground.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OnGround {
    /// Bounces (`bounce`) and keeps `slide` of its speed along the ground per hit.
    #[default]
    Bounce,
    /// Runs along it, losing speed at `ground_drag` per second: a sheet of dust.
    Skim,
    /// Gone where it lands.
    Vanish,
}

/// How a particle looks and moves.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleDef {
    /// Albedo (sRGB 0..1) at birth and at death: what the sun lights (dust, smoke, rock).
    #[serde(default)]
    pub albedo: [[f32; 3]; 2],
    /// Emitted light (linear, HDR) at birth and at death (fire, sparks).
    #[serde(default)]
    pub emission: [[f32; 3]; 2],
    /// Opacity at birth and at death.
    pub opacity: [f32; 2],
    /// Share of life spent fading in.
    #[serde(default)]
    pub fade_in: f32,
    /// Share of life spent fading out at the end.
    #[serde(default = "fifth")]
    pub fade_out: f32,
    /// Size factor at birth and at death.
    #[serde(default = "unit2")]
    pub grow: [f32; 2],
    /// Velocity damping (1/s): 0 in vacuum.
    #[serde(default)]
    pub drag: f32,
    /// Share of the body's gravity felt (0: floats).
    #[serde(default = "one")]
    pub gravity: f32,
    /// Speed kept off the ground along the normal (0 stays, 1 elastic).
    #[serde(default)]
    pub bounce: f32,
    /// Speed kept along the ground on a hit.
    #[serde(default = "half")]
    pub slide: f32,
    /// Drawn stretched along the velocity: extra length (sizes) per m/s.
    #[serde(default)]
    pub stretch: f32,
    /// How cloudy: 0 a hard-edged chunk, 1 a soft noisy puff.
    #[serde(default = "one")]
    pub puff: f32,
    #[serde(default)]
    pub on_ground: OnGround,
    /// Speed lost per second while skimming (1/s).
    #[serde(default)]
    pub ground_drag: f32,
    /// A soft round glow that only adds light (its emission, fading to nothing at the edge; the
    /// bloom does the rest): the bright heart of a blast.
    #[serde(default)]
    pub glow: bool,
    /// A glow is seen out to this many of its radii and fades from 60 % of it (0: always): the
    /// bigger the blast, the farther its light is seen, at its true size.
    #[serde(default)]
    pub glow_reach: f32,
}

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn fifth() -> f32 {
    0.2
}
fn unit2() -> [f32; 2] {
    [1.0, 1.0]
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: DVec3,
    pub drift: DVec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    /// Radius at birth (m); the style grows it.
    pub size: f32,
    /// 0..1, varies the look.
    pub seed: f32,
    /// Distance from the body's centre to the ground under the particle (refreshed in turns).
    pub ground: f64,
    /// Height of the centre over that ground (m), as of the last step.
    pub height: f32,
    pub body: BodyId,
    pub style: u8,
}

impl Particle {
    /// Share of life gone (0..1).
    pub fn t(&self) -> f32 {
        (self.age / self.life).min(1.0)
    }

    /// Radius now.
    pub fn radius(&self, s: &StyleDef) -> f32 {
        self.size * (s.grow[0] + (s.grow[1] - s.grow[0]) * self.t())
    }
}

pub struct Particles {
    pub list: Vec<Particle>,
    pub styles: Vec<StyleDef>,
    capacity: usize,
    /// Next particle whose ground is refreshed.
    cursor: usize,
    /// How far ahead of the picture the particles' own time is (s): the game steps them, and
    /// whoever draws between steps draws them that far back (`lag` of the renderer). What is made
    /// while the picture is being made (dust under a foot, a plume's puff, from where things are
    /// drawn) is moved ahead by as much, to the particles' time, so it is drawn where it was made.
    pub ahead: f64,
}

/// Ground samples per frame (the rest keep the last one).
const GROUND_REFRESH: usize = 384;

impl Particles {
    pub fn new(styles: Vec<StyleDef>, capacity: usize) -> Particles {
        assert!(styles.len() <= MAX_STYLES);
        Particles { list: Vec::with_capacity(capacity), styles, capacity, cursor: 0, ahead: 0.0 }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// A new budget (settings): the youngest go if it shrank.
    pub fn set_capacity(&mut self, n: usize) {
        self.capacity = n;
        self.list.truncate(n);
        self.list.reserve(n.saturating_sub(self.list.len()));
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Room left; a full list drops new particles (never the ones already flying).
    pub fn spawn(&mut self, mut p: Particle) -> bool {
        if self.list.len() == self.capacity {
            return false;
        }
        if self.ahead != 0.0 {
            p.pos += (p.drift + p.vel.as_dvec3()) * self.ahead;
        }
        self.list.push(p);
        true
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn update(&mut self, dt: f32, bodies: &BodyRegistry) {
        let styles = &self.styles;
        // the ground under a share of the particles, in turns
        let n = self.list.len();
        if n > 0 {
            let from = self.cursor % n;
            let to = (from + GROUND_REFRESH).min(n);
            self.list[from..to].par_iter_mut().for_each(|p| {
                let b = bodies.get(p.body);
                let d = b.up(p.pos);
                p.ground = b.radius + b.height_at(d, f64::from(p.size).max(0.5));
            });
            self.cursor = to;
        }
        self.list.par_iter_mut().with_min_len(256).for_each(|p| {
            let s = &styles[usize::from(p.style)];
            let b = bodies.get(p.body);
            let up = b.up(p.pos);
            let up32 = up.as_vec3();
            // (pulled as anything is where it is now: past every body's reach, not at all)
            if s.gravity != 0.0 {
                p.vel += (bodies.field(p.pos).pull * f64::from(s.gravity * dt)).as_vec3();
            }
            p.vel *= (-s.drag * dt).exp();
            p.pos += (p.drift + p.vel.as_dvec3()) * f64::from(dt);
            p.age += dt;
            // ground: rest on it, bounce off it, slide along it
            let r = p.radius(s) * 0.35;
            let floor = p.ground + f64::from(r);
            let h = (p.pos - b.center).length();
            if h < floor {
                p.pos += up * (floor - h);
                p.vel = (p.drift + p.vel.as_dvec3()).as_vec3();
                p.drift = DVec3::ZERO;
                let vn = p.vel.dot(up32);
                if vn < 0.0 {
                    let vt = p.vel - up32 * vn;
                    p.vel = match s.on_ground {
                        OnGround::Bounce => vt * s.slide - up32 * (vn * s.bounce),
                        OnGround::Skim => vt * (-s.ground_drag * dt).exp(),
                        OnGround::Vanish => {
                            p.age = p.life;
                            Vec3::ZERO
                        }
                    };
                }
            }
            p.height = (h.max(floor) - p.ground) as f32;
        });
        self.list.retain(|p| p.age < p.life);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::{Body, BodyDef};

    fn moon() -> BodyRegistry {
        let def: BodyDef = crate::defs::parse("b", r#"{ "name": "b", "center": [0, 0, 0], "radius": 1000, "gravity": 1.62, "reach": { "to": 500, "band": 200 }, "north": [0, 0, -1], "horizon_depth": 10 }"#).unwrap();
        BodyRegistry::new(vec![Body::from_def("b", &def).unwrap()])
    }

    fn style(bounce: f32, drag: f32) -> StyleDef {
        StyleDef {
            albedo: [[0.5; 3]; 2],
            emission: [[0.0; 3]; 2],
            opacity: [1.0, 0.0],
            fade_in: 0.0,
            fade_out: 0.2,
            grow: [1.0, 1.0],
            drag,
            gravity: 1.0,
            bounce,
            slide: 0.5,
            stretch: 0.0,
            puff: 1.0,
            on_ground: OnGround::Bounce,
            ground_drag: 0.0,
            glow: false,
            glow_reach: 0.0,
        }
    }

    fn launch(v: f32) -> Particle {
        Particle { pos: DVec3::Y * 1000.5, drift: DVec3::ZERO, vel: Vec3::Y * v, age: 0.0, life: 60.0, size: 0.2, seed: 0.0, ground: 1000.0, height: 0.5, body: 0, style: 0 }
    }

    #[test]
    fn ballistic_in_vacuum_then_settles() {
        let bodies = moon();
        let mut ps = Particles::new(vec![style(0.0, 0.0)], 4);
        ps.spawn(launch(16.2));
        let mut top = 0_f64;
        for _ in 0..900 {
            ps.update(1.0 / 60.0, &bodies);
            top = top.max(ps.list[0].pos.y - 1000.5);
        }
        // energy with gravity falling off: v²/2 = g R² (1/r0 - 1/r1), so r1 = 1086.5 m
        let gm = 1.62 * 1000.0 * 1000.0;
        let r1 = 1.0 / (1.0 / 1000.5 - 16.2 * 16.2 / 2.0 / gm);
        assert!((top - (r1 - 1000.5)).abs() < 0.5, "top {top}");
        for _ in 0..1500 {
            ps.update(1.0 / 60.0, &bodies);
        }
        let p = ps.list[0];
        assert!(p.vel.length() < 1e-3, "still moving {}", p.vel.length());
        assert!((p.pos.length() - 1000.0 - 0.07).abs() < 0.01);
    }

    #[test]
    fn drag_slows_puffs_and_life_ends() {
        let bodies = moon();
        let mut ps = Particles::new(vec![style(0.0, 3.0)], 4);
        let mut p = launch(0.0);
        p.vel = Vec3::X * 20.0;
        p.life = 1.0;
        ps.spawn(p);
        for _ in 0..30 {
            ps.update(1.0 / 60.0, &bodies);
        }
        assert!(ps.list[0].vel.x < 20.0 * (-1.5_f32).exp() * 1.01);
        for _ in 0..40 {
            ps.update(1.0 / 60.0, &bodies);
        }
        assert!(ps.is_empty());
    }

    #[test]
    fn full_list_keeps_what_flies() {
        let mut ps = Particles::new(vec![style(0.0, 0.0)], 2);
        assert!(ps.spawn(launch(1.0)) && ps.spawn(launch(2.0)));
        assert!(!ps.spawn(launch(3.0)));
        assert_eq!(ps.len(), 2);
    }
}
