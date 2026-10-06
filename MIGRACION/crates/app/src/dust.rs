//! Dust: what boots and jets raise from the ground of an airless body.
//!
//! With no air to hold it, dust does not hang: every grain flies its own arc and falls back.
//! Under a jet it leaves as a sheet, low and fast, straight out from where the jet strikes (the
//! photographs of the lunar landings show it a few degrees over the ground, with the horizon
//! clear above it); a boot kicks a small fan of it ahead. One law for every jet, the pack's and
//! an engine's alike: a jet raises dust within a reach that goes with the root of its thrust
//! (the pressure it lands on the ground falls with the square of the distance), about 40 m for
//! the 45 kN of a lander's descent engine.
//!
//! Its cost is bounded: only jets near the eye, a ceiling of grains per second shared among them.
//!
//! Dust is raised by loose ground only, and it is here that a ground is said to be loose
//! (`Grounds`, from `assets/defs/huellas.jsonc`): the same ground takes the marks of what touches
//! it (`footprints`), which are left from what is raised here — the boots set down (`treads`) and
//! the jets that strike the ground (`jets`).
use crate::{pilot::Pilot, ships::Ships};
use glam::{DVec3, Vec3};
use lunar_core::{
    body::{BodyId, BodyRegistry},
    effects::Effects,
    structure::set::Structures,
};

/// Reach (m) of a jet of `REF_THRUST` N; the reach of another goes with the root of its thrust.
const REF_REACH: f64 = 40.0;
const REF_THRUST: f64 = 45_000.0;
/// Ships farther than this from the eye (m) raise none; grains a second at most, all together.
const SEEN_WITHIN: f64 = 700.0;
const BUDGET: f32 = 600.0;

/// How loose the ground of each body is: 0 hard (rock, ice: no dust, no marks) .. 1 regolith.
/// By the body's id; `other`: every body not named.
#[derive(Clone, Debug, PartialEq)]
pub struct Grounds {
    pub named: Vec<(String, f32)>,
    pub other: f32,
}

impl Default for Grounds {
    /// Every ground loose (what the game was before any was said to be hard).
    fn default() -> Grounds {
        Grounds { named: Vec::new(), other: 1.0 }
    }
}

impl Grounds {
    pub fn loose(&self, id: &str) -> f32 {
        self.named.iter().find(|(name, _)| name == id).map_or(self.other, |g| g.1).clamp(0.0, 1.0)
    }
}

/// A boot set down on loose ground: where (as it was said: the foot's place), the body, how it
/// was going (world, any length: its speed) and how hard (1 a step, more a landing).
#[derive(Clone, Copy, Debug)]
pub struct Tread {
    pub at: DVec3,
    pub body: BodyId,
    pub heading: DVec3,
    pub hard: f32,
}

/// Treads kept for whoever leaves their marks (if nobody takes them, the oldest are forgotten).
const TREADS: usize = 64;
/// A boot farther than this from the ground (m) is not on it (a deck, a roof): it leaves no tread.
const ON_GROUND: f64 = 0.3;

pub struct Dust {
    /// What ground is loose: what raises dust and takes marks.
    pub grounds: Grounds,
    /// The boots set down on loose ground since they were last taken (`footprints`).
    pub treads: Vec<Tread>,
    sheet: Option<u8>,
    grains: Option<u8>,
    rng: u64,
    /// Jets found this frame: where each strikes the ground, the body, how hard (0..1) and its
    /// thrust (N); and the grains owed to each from the frames before.
    jets: Vec<(DVec3, BodyId, f32, f32)>,
    owed: f32,
    scratch: Vec<(Vec3, Vec3, f32)>,
}

impl Dust {
    pub fn new(fx: &Effects) -> Dust {
        Dust { grounds: Grounds::default(), treads: Vec::with_capacity(TREADS), sheet: fx.style("chorro_polvo"), grains: fx.style("polvo_fino"), rng: 0x9E37_79B9_7F4A_7C15, jets: Vec::new(), owed: 0.0, scratch: Vec::new() }
    }

    /// How loose the ground of `body` is (0 hard .. 1): what dust and marks go by.
    pub fn loose(&self, bodies: &BodyRegistry, body: BodyId) -> f32 {
        self.grounds.loose(&bodies.get(body).id)
    }

    /// This frame's jets that strike the ground: where, the body, how hard (0..1) and the thrust (N).
    pub fn jets(&self) -> &[(DVec3, BodyId, f32, f32)] {
        &self.jets
    }

    /// 0..1.
    fn chance(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A jet of `thrust` N leaving `from` along `along` (world, unit): if it strikes the ground
    /// within its reach, it is one of this frame's.
    fn jet(&mut self, bodies: &BodyRegistry, from: DVec3, along: DVec3, thrust: f64) {
        if thrust < 20.0 {
            return;
        }
        let body = bodies.dominant(from);
        // (hard ground has no dust to give)
        if self.loose(bodies, body) <= 0.0 {
            return;
        }
        let b = bodies.get(body);
        let up = b.up(from);
        let down = -along.dot(up);
        // (a jet across the ground, or up, strikes nothing)
        if down < 0.25 {
            return;
        }
        let reach = REF_REACH * (thrust / REF_THRUST).sqrt();
        let far = b.altitude(from).max(0.0) / down;
        if far >= reach {
            return;
        }
        let hard = (1.0 - far / reach) as f32;
        let at = from + along * far;
        self.jets.push((b.above_ground(b.up(at), 0.03), body, hard * hard, thrust as f32));
    }

    /// This frame's only jet (tests of what the jets leave on the ground).
    #[cfg(test)]
    pub fn strike(&mut self, bodies: &BodyRegistry, from: DVec3, along: DVec3, thrust: f64) {
        self.jets.clear();
        self.jet(bodies, from, along, thrust);
    }

    /// A boot set down at `at` going `heading` (world, level; any length): a small fan of dust
    /// ahead of it. `hard` 1 for a step, more for coming down from a jump.
    pub fn step(&mut self, fx: &mut Effects, bodies: &BodyRegistry, at: DVec3, heading: DVec3, hard: f32) {
        let body = bodies.dominant(at);
        let loose = self.loose(bodies, body);
        if loose <= 0.0 {
            return;
        }
        let b = bodies.get(body);
        let up = b.up(at);
        let ground = b.above_ground(up, 0.02);
        // on the ground itself, it leaves its mark too
        if (at - ground).dot(up).abs() < ON_GROUND {
            if self.treads.len() == TREADS {
                self.treads.remove(0);
            }
            self.treads.push(Tread { at, body, heading, hard });
        }
        let Some(style) = self.grains else { return };
        let ahead = (heading - up * heading.dot(up)).normalize_or(up.any_orthonormal_vector());
        let side = up.cross(ahead);
        for _ in 0..((3.0 + 3.0 * hard) * loose) as usize {
            // boots throw it at half a metre to two a second, 20° to 60° up
            let speed = f64::from(0.5 + 1.5 * self.chance()) * f64::from(hard.sqrt());
            let lift = f64::from(20.0 + 40.0 * self.chance()).to_radians();
            let turn = f64::from(self.chance() - 0.5) * if hard > 1.2 { std::f64::consts::TAU } else { 1.6 };
            let dir = ahead * turn.cos() + side * turn.sin();
            let vel = (dir * lift.cos() + up * lift.sin()) * speed;
            let (size, life) = (0.07 + 0.08 * self.chance(), 1.0 + 1.2 * self.chance());
            fx.puff(style, bodies, body, ground + dir * 0.12, vel.as_vec3(), size, life);
        }
    }

    /// This frame's dust: under the pack's jets, under every thruster of the ships near `eye`.
    pub fn frame(&mut self, dt: f32, fx: &mut Effects, bodies: &BodyRegistry, pilot: &Pilot, pack_thrust: f64, ships: &Ships, set: &Structures, eye: DVec3) {
        self.jets.clear();
        // the pack: straight down from the back, out in the open
        if pilot.pack_on && pilot.jet > 0.0 && pilot.ride.is_none() && !pilot.flying {
            let (_, up) = pilot.feet();
            self.jet(bodies, pilot.position - up * 0.9, -up, pack_thrust * pilot.jet);
        }
        for sh in &ships.list {
            let Some(s) = set.get(sh.structure) else { continue };
            if s.force == Vec3::ZERO || s.held.is_some() || s.to_world(s.center).distance(eye) - f64::from(s.radius) > SEEN_WITHIN {
                continue;
            }
            let mut found = std::mem::take(&mut self.scratch);
            sh.jets(s, &mut found);
            for &(at, push, thrust) in &found {
                // (what pushes the ship one way leaves it the other)
                self.jet(bodies, s.to_world(at), -(s.rot * push).as_dvec3(), f64::from(thrust));
            }
            self.scratch = found;
        }
        self.emit(dt, fx, bodies);
    }

    /// The grains this frame's jets raise.
    fn emit(&mut self, dt: f32, fx: &mut Effects, bodies: &BodyRegistry) {
        let (Some(sheet), Some(grains)) = (self.sheet, self.grains) else { return };
        if self.jets.is_empty() {
            self.owed = 0.0;
            return;
        }
        // grains a second each would like, and what the budget leaves of it
        let want = |hard: f32, thrust: f32| hard * (60.0 * (thrust / 1000.0).sqrt()).clamp(30.0, 240.0);
        let all: f32 = self.jets.iter().map(|j| want(j.2, j.3)).sum();
        let share = (BUDGET / all.max(1.0)).min(1.0);
        let jets = std::mem::take(&mut self.jets);
        for &(at, body, hard, thrust) in &jets {
            self.owed += want(hard, thrust) * share * dt;
            let b = bodies.get(body);
            let up = b.up(at);
            let (x, z) = {
                let x = up.any_orthonormal_vector();
                (x, up.cross(x))
            };
            // how big a jet it is: what it throws, how far and how fast
            let size = (thrust / 1000.0).powf(0.25);
            while self.owed >= 1.0 {
                self.owed -= 1.0;
                let turn = f64::from(self.chance()) * std::f64::consts::TAU;
                let out = x * turn.cos() + z * turn.sin();
                let from = at + out * f64::from(0.2 + 0.5 * size * self.chance());
                // most of it a sheet a few degrees over the ground; some of it thrown higher
                let high = self.chance() < 0.22;
                let lift = f64::from(if high { 8.0 + 14.0 * self.chance() } else { 1.0 + 3.0 * self.chance() }).to_radians();
                let speed = f64::from(5.0 * size * (0.5 + self.chance()) * (0.4 + 0.6 * hard.sqrt()));
                let vel = (out * lift.cos() + up * lift.sin()) * speed;
                // (what is thrown higher is finer and gone sooner: it must not hang like a balloon)
                let fine = if high { 0.45 } else { 1.0 };
                let (r, life) = (0.22 * size * fine * (0.6 + 0.8 * self.chance()), (1.2 + 2.2 * self.chance()) * (0.5 + 0.5 * fine));
                fx.puff(if high { grains } else { sheet }, bodies, body, from, vel.as_vec3(), r, life);
            }
        }
        self.jets = jets;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_core::{
        body::{Body, BodyDef},
        defs,
        effects::EffectDefs,
    };
    use std::path::Path;

    fn moon() -> BodyRegistry {
        let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
    }

    fn effects() -> Effects {
        Effects::new(&EffectDefs::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")).unwrap(), 20_000)
    }

    #[test]
    fn a_jet_raises_dust_within_a_reach_that_goes_with_the_root_of_its_thrust() {
        let (bodies, fx) = (moon(), effects());
        let b = bodies.get(0);
        let up = DVec3::Y;
        let mut dust = Dust::new(&fx);
        assert!(dust.sheet.is_some() && dust.grains.is_some(), "faltan los estilos de polvo");
        let mut reaches = |thrust: f64, h: f64, along: DVec3| {
            dust.jets.clear();
            dust.jet(&bodies, b.above_ground(up, h), along, thrust);
            dust.jets.first().map(|j| j.2)
        };
        // a lander's engine: from 40 m down; the harder the nearer
        assert!(reaches(45_000.0, 45.0, -up).is_none());
        let (far, near) = (reaches(45_000.0, 30.0, -up).unwrap(), reaches(45_000.0, 5.0, -up).unwrap());
        assert!(far > 0.0 && near > far * 4.0, "{far} a 30 m, {near} a 5 m");
        // the pack: a few metres
        assert!(reaches(630.0, 3.0, -up).is_some() && reaches(630.0, 6.0, -up).is_none());
        // four times the thrust, twice the reach
        assert!(reaches(180_000.0, 75.0, -up).is_some() && reaches(180_000.0, 85.0, -up).is_none());
        // a jet across the ground, or up, strikes nothing; nor does a whisper of one
        let side = up.any_orthonormal_vector();
        assert!(reaches(45_000.0, 2.0, side).is_none() && reaches(45_000.0, 2.0, up).is_none());
        assert!(reaches(10.0, 0.5, -up).is_none());
    }

    #[test]
    fn dust_leaves_low_and_outward_and_never_more_than_its_budget() {
        let (bodies, mut fx) = (moon(), effects());
        let b = bodies.get(0);
        let up = DVec3::Y;
        let mut dust = Dust::new(&fx);
        // twenty big engines a metre over the ground, for a second
        let from: Vec<DVec3> = (0..20).map(|k| b.above_ground((up + DVec3::X * 1e-6 * f64::from(k)).normalize(), 1.0)).collect();
        for _ in 0..60 {
            dust.jets.clear();
            for f in &from {
                dust.jet(&bodies, *f, -b.up(*f), 65_000.0);
            }
            assert_eq!(dust.jets.len(), 20);
            dust.emit(1.0 / 60.0, &mut fx, &bodies);
        }
        let raised = fx.particles.len();
        assert!(raised as f32 <= BUDGET * 1.05 && raised as f32 > BUDGET * 0.5, "{raised} granos en un segundo (tope {BUDGET})");
        // one small jet: its dust goes out along the ground, most of it a few degrees over it
        fx.particles.clear();
        let at = b.above_ground(up, 1.0);
        for _ in 0..240 {
            dust.jets.clear();
            dust.jet(&bodies, at, -up, 4000.0);
            dust.emit(1.0 / 60.0, &mut fx, &bodies);
        }
        let n = fx.particles.len();
        assert!(n > 100, "{n} granos");
        let rise = |v: Vec3| f64::from(v.dot(up.as_vec3())).atan2(f64::from((v - up.as_vec3() * v.dot(up.as_vec3())).length())).to_degrees();
        let low = fx.particles.list.iter().filter(|p| rise(p.vel) < 5.0).count();
        assert!(fx.particles.list.iter().all(|p| rise(p.vel) > 0.0 && rise(p.vel) < 31.0), "polvo hacia abajo o hacia arriba");
        assert!(low * 10 > n * 6, "solo {low} de {n} a ras de suelo");
    }

    #[test]
    fn a_boot_kicks_a_small_fan_of_dust_ahead() {
        let (bodies, mut fx) = (moon(), effects());
        let b = bodies.get(0);
        let up = DVec3::Y;
        let at = b.above_ground(up, 0.0);
        let ahead = up.any_orthonormal_vector();
        let mut dust = Dust::new(&fx);
        dust.step(&mut fx, &bodies, at, ahead * 1.4, 1.0);
        let n = fx.particles.len();
        assert!((3..=8).contains(&n), "{n} granos por paso");
        for p in fx.particles.list.iter() {
            let v = p.vel.as_dvec3();
            assert!(v.dot(ahead) > 0.0, "polvo hacia atrás: {v:.2?}");
            assert!(v.length() >= 0.4 && v.length() <= 2.1, "a {:.2} m/s", v.length());
            let lift = v.dot(up).atan2((v - up * v.dot(up)).length()).to_degrees();
            assert!((19.0..=61.0).contains(&lift), "a {lift:.0}°");
        }
    }
}
