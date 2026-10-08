//! Long-range projectiles (`missiles.jsonc`): point masses flying under the gravity of the bodies,
//! stepped at a fixed rate so every flight is the same. Each step's path is swept against the
//! ground and the structures (a broadphase grid); every half second the path ahead is predicted
//! and whatever it will strike is woken in time, so a strike lands on a full simulation even a
//! hundred kilometres from anyone watching. Impacts are resolved against the structures' data,
//! never against what happens to be drawn. Launches are solved by shooting: the speed that lands
//! on the point aimed at.
use crate::{
    body::BodyRegistry,
    defs::{self, DefError},
    structure::{broadphase::Grid, schedule::LINGER, set::Structures},
};
use glam::DVec3;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissileDef {
    pub name: String,
    #[serde(default)]
    pub key: Option<String>,
    /// Climb over the horizon at launch (deg).
    pub angle: f64,
    /// Fastest launch a solution may ask for (m/s).
    pub max_speed: f64,
    /// kg: its speed at impact adds ½ m v² to the warhead.
    pub mass: f64,
    /// Explosion where it strikes (explosions/<id>)...
    #[serde(default)]
    pub warhead: Option<String>,
    /// ...or the explosive it carries (its explosion is made from it).
    #[serde(default)]
    pub charge: Option<crate::detonation::ChargeDef>,
    /// How it is drawn in flight.
    #[serde(default)]
    pub look: Option<crate::effect_defs::RoundLook>,
    /// Particle style it leaves behind, and how many per second.
    #[serde(default)]
    pub trail: Option<String>,
    #[serde(default)]
    pub trail_rate: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Ground,
    /// A structure, by id.
    Structure(u64),
}

#[derive(Clone, Copy, Debug)]
pub struct Missile {
    pub kind: usize,
    /// Whose it is, as its caller numbers what it lets fly (0: nobody's); it comes back with
    /// its strike.
    pub tag: u32,
    pub pos: DVec3,
    pub vel: DVec3,
    /// Flight time (s).
    pub t: f64,
    /// What the path ahead strikes and at what flight time, as last predicted.
    pub predicted: Option<(Target, f64)>,
    /// Flight time at which the path ahead is looked at again.
    pub next_look: f64,
}

/// A missile that landed.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub kind: usize,
    pub tag: u32,
    pub at: DVec3,
    pub vel: DVec3,
    pub target: Target,
    /// Its kinetic energy (J).
    pub energy: f64,
}

/// Fixed flight step (s).
pub const STEP: f64 = 1.0 / 120.0;
const LOOK_EVERY: f64 = 0.5;
const LOOK_AHEAD: f64 = 20.0;
const LOOK_STEP: f64 = 0.05;

/// What pulls at `p` (m/s²).
pub fn gravity(bodies: &BodyRegistry, p: DVec3) -> DVec3 {
    bodies.field(p).pull
}

/// One step of velocity Verlet.
fn advance(bodies: &BodyRegistry, p: DVec3, v: DVec3, dt: f64) -> (DVec3, DVec3) {
    let a0 = gravity(bodies, p);
    let p1 = p + v * dt + a0 * (0.5 * dt * dt);
    let a1 = gravity(bodies, p1);
    (p1, v + (a0 + a1) * (0.5 * dt))
}

pub struct Missiles {
    pub defs: Vec<(String, MissileDef)>,
    pub list: Vec<Missile>,
    grid: Grid,
    near: Vec<u32>,
    acc: f64,
}

impl Missiles {
    pub fn load(path: &Path) -> Result<Missiles, DefError> {
        let d: BTreeMap<String, MissileDef> = defs::load(path)?;
        Ok(Missiles::new(d.into_iter().collect()))
    }

    pub fn new(defs: Vec<(String, MissileDef)>) -> Missiles {
        Missiles { defs, list: Vec::new(), grid: Grid::new(512.0), near: Vec::new(), acc: 0.0 }
    }

    /// Launch velocity from `from` that lands on `to` (climbing at the kind's angle, in the plane
    /// of the vertical and the target), or None when it is out of reach.
    pub fn solve(&self, kind: usize, from: DVec3, to: DVec3, bodies: &BodyRegistry) -> Option<DVec3> {
        let d = &self.defs[kind].1;
        let c = bodies.get(bodies.dominant(from)).center;
        let up = (from - c).normalize();
        let toward = to - from;
        let horiz = (toward - up * toward.dot(up)).normalize_or(up.any_orthonormal_vector());
        let a = d.angle.to_radians();
        let dir = horiz * a.cos() + up * a.sin();
        let (want, r_to) = ((from - c).angle_between(to - c), (to - c).length());
        // downrange angle where the flight comes back down to the target's height
        let range = |speed: f64| {
            let (mut p, mut v) = (from, dir * speed);
            for _ in 0..200_000 {
                let (p1, v1) = advance(bodies, p, v, 0.02);
                let (r0, r1) = ((p - c).length(), (p1 - c).length());
                if r1 < r_to && (p1 - c).dot(v1) < 0.0 {
                    let f = ((r0 - r_to) / (r0 - r1).max(1e-9)).clamp(0.0, 1.0);
                    return (p + (p1 - p) * f - c).angle_between(from - c);
                }
                (p, v) = (p1, v1);
            }
            f64::INFINITY
        };
        if range(d.max_speed) < want {
            return None;
        }
        let (mut lo, mut hi) = (1.0, d.max_speed);
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if range(mid) < want { lo = mid } else { hi = mid }
        }
        Some(dir * (0.5 * (lo + hi)))
    }

    /// Fire kind `id` from `from` to land on `to`.
    pub fn launch(&mut self, id: &str, from: DVec3, to: DVec3, bodies: &BodyRegistry) -> Result<(), String> {
        let kind = self.defs.iter().position(|(k, _)| k == id).ok_or_else(|| format!("unknown missile '{id}'"))?;
        let vel = self.solve(kind, from, to, bodies).ok_or_else(|| format!("{}: out of reach", self.defs[kind].1.name))?;
        self.fire(kind, from, vel, 0);
        Ok(())
    }

    /// Fire kind `kind` from `from` at `vel` (worked out already: `solve`), numbered `tag`.
    pub fn fire(&mut self, kind: usize, from: DVec3, vel: DVec3, tag: u32) {
        self.list.push(Missile { kind, tag, pos: from, vel, t: 0.0, predicted: None, next_look: 0.0 });
    }

    /// Fly every missile `dt` on (in fixed steps); the ones that land go to `strikes`.
    pub fn update(&mut self, dt: f64, bodies: &BodyRegistry, set: &mut Structures, strikes: &mut Vec<Strike>) {
        if self.list.is_empty() {
            self.acc = 0.0;
            return;
        }
        self.acc += dt;
        self.grid.build(&set.list);
        while self.acc >= STEP {
            self.acc -= STEP;
            let mut k = 0;
            while k < self.list.len() {
                let m = self.list[k];
                if m.t >= m.next_look {
                    self.look_ahead(k, bodies, set);
                }
                let (p1, v1) = advance(bodies, m.pos, m.vel, STEP);
                match self.sweep(m.pos, p1, bodies, set) {
                    Some((at, target)) => {
                        let mass = self.defs[m.kind].1.mass;
                        strikes.push(Strike { kind: m.kind, tag: m.tag, at, vel: v1, target, energy: 0.5 * mass * v1.length_squared() });
                        self.list.swap_remove(k);
                    }
                    None => {
                        let m = &mut self.list[k];
                        (m.pos, m.vel, m.t) = (p1, v1, m.t + STEP);
                        k += 1;
                    }
                }
            }
        }
    }

    /// What the path a→b strikes first, ground or structure, and where (structures where they
    /// are now: the ones in flight are not led).
    fn sweep(&mut self, a: DVec3, b: DVec3, bodies: &BodyRegistry, set: &Structures) -> Option<(DVec3, Target)> {
        let len = a.distance(b);
        let dir = (b - a) / len;
        let ground = bodies.raycast(a, dir, len).map(|(_, p)| p);
        let reach = ground.map_or(len, |p| p.distance(a));
        self.grid.along(a, b, 0.0, &mut self.near);
        let hit = set.raycast_among(&self.near, a, dir, reach);
        match (hit, ground) {
            (Some((i, _, p)), _) => Some((p, Target::Structure(set.list[i].id))),
            (None, Some(p)) => Some((p, Target::Ground)),
            _ => None,
        }
    }

    /// Fly missile `k`'s path ahead in coarse steps: whatever it will strike is woken until a
    /// while after the strike.
    fn look_ahead(&mut self, k: usize, bodies: &BodyRegistry, set: &mut Structures) {
        let m = self.list[k];
        self.list[k].next_look = m.t + LOOK_EVERY;
        let (mut p, mut v, mut t) = (m.pos, m.vel, 0.0);
        while t < LOOK_AHEAD {
            let (p1, v1) = advance(bodies, p, v, LOOK_STEP);
            if let Some((_, target)) = self.sweep(p, p1, bodies, set) {
                if let Target::Structure(id) = target {
                    set.wake(id, set.now + t + LINGER);
                }
                self.list[k].predicted = Some((target, m.t + t));
                return;
            }
            (p, v, t) = (p1, v1, t + LOOK_STEP);
        }
        self.list[k].predicted = None;
    }
}
