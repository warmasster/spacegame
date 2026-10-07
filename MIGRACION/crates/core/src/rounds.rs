//! Rounds in flight: a pool of ballistic projectiles with a fixed capacity (nothing allocated
//! after creation), each pulled as anything is pulled where it is now (`BodyRegistry::field`:
//! past every body's reach it flies straight and there is no ground to hit). Every step's segment is
//! checked against the ground and handed to whoever holds structures (a closure). The ground costs
//! nothing far from it: under `GROUND_CEILING` a cheap height sample, and the exact point only on a
//! hit. What they look like is a particle style: `looks` writes one particle per round into a
//! reused list, drawn with the particles in their single instanced draw. Thousands in a space
//! battle are a loop over a flat array and one draw.
use crate::{
    body::{BodyId, BodyRegistry},
    particles::Particle,
    structure::{
        motion::{Crossing, SurfaceHit, Sweep},
        schedule::{Among, LINGER},
        set::Structures,
    },
};
use glam::DVec3;

/// Over this height above the datum (m) no ground can be hit (higher than any relief).
const GROUND_CEILING: f64 = 25_000.0;

#[derive(Clone, Copy, Debug)]
pub struct Round {
    pub pos: DVec3,
    pub vel: DVec3,
    pub nose: DVec3,
    /// What was fired (the caller's index: a shot definition).
    pub kind: u16,
    pub body: BodyId,
    /// Seconds left before it is spent.
    pub left: f32,
    /// Look: particle style and radius (m).
    pub style: u8,
    pub size: f32,
    pub seed: f32,
    /// Whose it is, as its caller numbers what it lets fly (0: nobody's); it comes back with
    /// its impact.
    pub tag: u32,
}

/// Where a round ended: on the ground or on what the structure test found.
#[derive(Clone, Copy, Debug)]
pub struct Impact {
    pub kind: u16,
    pub tag: u32,
    pub at: DVec3,
    /// Unit direction of flight, and its speed (m/s).
    pub dir: DVec3,
    pub speed: f64,
    /// The structure test hit, not the ground.
    pub structure: bool,
    pub surface: Option<SurfaceHit>,
}

pub struct Flight<'a> {
    pub rounds: &'a mut Rounds,
    pub impacts: &'a mut Vec<Impact>,
    pub sweep: &'a mut Sweep,
}

impl Among for Flight<'_> {
    fn wake(&mut self, set: &mut Structures, dt: f64) {
        if self.rounds.is_empty() {
            return;
        }
        self.sweep.forecast(set, dt);
        for round in &self.rounds.list {
            self.sweep.candidates(round.pos, round.pos + round.vel * dt, |index| {
                set.list[index].awake_until = set.list[index].awake_until.max(set.now + LINGER);
            });
        }
    }

    fn before(&mut self, set: &Structures) {
        if !self.rounds.is_empty() {
            self.sweep.begin(set);
        }
    }

    fn slice(&mut self, set: &Structures, bodies: &BodyRegistry, dt: f64) {
        if self.rounds.is_empty() {
            return;
        }
        self.sweep.end(set);
        self.rounds.fly(dt, bodies, |from, to| self.sweep.hit(set, from, to), self.impacts);
    }

    fn at(&self) -> DVec3 {
        self.rounds.list.first().map_or(DVec3::splat(f64::INFINITY), |round| round.pos)
    }
}

pub struct Rounds {
    pub list: Vec<Round>,
    capacity: usize,
}

impl Rounds {
    pub fn new(capacity: usize) -> Rounds {
        Rounds { list: Vec::with_capacity(capacity), capacity }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// A new round; false when the pool is full (it is not fired).
    pub fn fire(&mut self, r: Round) -> bool {
        if self.list.len() == self.capacity {
            return false;
        }
        self.list.push(r);
        true
    }

    /// `dt` on. `structures(from, dir, len)` is the first structure hit on a segment, if any.
    /// Impacts are appended to `out`; spent and landed rounds leave the pool.
    pub fn step(&mut self, dt: f32, bodies: &BodyRegistry, mut structures: impl FnMut(DVec3, DVec3, f64) -> Option<DVec3>, out: &mut Vec<Impact>) {
        self.fly(
            f64::from(dt),
            bodies,
            |from, to| {
                let length = from.distance(to);
                let dir = (to - from).normalize_or(DVec3::Y);
                structures(from, dir, length).map(|at| Crossing { surface: SurfaceHit { id: 0, point: at.as_vec3(), dir: dir.as_vec3() }, fraction: if length > 0.0 { from.distance(at) / length } else { 0.0 }, at })
            },
            out,
        );
    }

    fn fly(&mut self, dt64: f64, bodies: &BodyRegistry, mut structures: impl FnMut(DVec3, DVec3) -> Option<Crossing>, out: &mut Vec<Impact>) {
        let mut k = 0;
        while k < self.list.len() {
            let r = &mut self.list[k];
            let here = bodies.field(r.pos);
            r.vel += here.pull * dt64;
            let from = r.pos;
            let to = from + r.vel * dt64;
            let len = to.distance(from);
            let dir = if len > 0.0 { (to - from) / len } else { DVec3::Y };
            r.left -= dt64 as f32;
            let mut surface = structures(from, to);
            let mut hit = surface.map(|h| (h.at, true));
            if let Some(b) = here.ground.map(|g| bodies.get(g)).filter(|b| b.datum_altitude(to) < GROUND_CEILING) {
                let up = b.up(to);
                // a coarse sample first (as fine as the step is long), the exact point only on a hit
                let ground = b.radius + b.height_at(up, len.max(0.5));
                if (to - b.center).length() < ground + 0.5 {
                    if let Some((_, point)) = bodies.raycast(from, dir, len)
                        && hit.is_none_or(|(at, _)| from.distance(point) < from.distance(at))
                    {
                        hit = Some((point, false));
                        surface = None;
                    }
                }
            }
            let speed = r.vel.length();
            if let Some((at, structure)) = hit {
                out.push(Impact { kind: r.kind, tag: r.tag, at, dir, speed, structure, surface: surface.map(|h| h.surface) });
                self.list.swap_remove(k);
                continue;
            }
            r.pos = to;
            // (whose light and ground it is drawn with: the nearest)
            r.body = here.nearest;
            if r.left <= 0.0 {
                self.list.swap_remove(k);
                continue;
            }
            k += 1;
        }
    }

    /// One particle per round (`out` cleared and refilled): how they are drawn.
    pub fn looks(&self, out: &mut Vec<Particle>) {
        out.clear();
        out.extend(self.list.iter().map(|r| Particle { pos: r.pos, drift: DVec3::ZERO, vel: r.nose.as_vec3(), age: 0.0, life: 1.0, size: r.size, seed: r.seed, ground: 0.0, height: 1e6, body: r.body, style: r.style }));
    }
}

/// A round of `kind` leaving `from` along unit `dir` at `speed` m/s, for `range` m.
#[allow(clippy::too_many_arguments)]
pub fn round(kind: u16, from: DVec3, dir: DVec3, speed: f32, range: f32, body: BodyId, style: u8, size: f32, seed: f32) -> Round {
    Round { pos: from, vel: dir * f64::from(speed), nose: dir * f64::from(speed), kind, body, left: range / speed.max(1.0), style, size, seed, tag: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::{Body, BodyDef};

    fn moon() -> BodyRegistry {
        let def: BodyDef = crate::defs::parse("b", r#"{ "name": "b", "center": [0, 0, 0], "radius": 1000, "gravity": 1.62, "reach": { "to": 500, "band": 200 }, "north": [0, 0, -1], "horizon_depth": 10 }"#).unwrap();
        BodyRegistry::new(vec![Body::from_def("b", &def).unwrap()])
    }

    #[test]
    fn a_round_falls_and_lands_on_the_ground() {
        let bodies = moon();
        let mut pool = Rounds::new(4);
        let from = DVec3::Y * 1002.0;
        assert!(pool.fire(round(3, from, DVec3::X, 20.0, 1000.0, 0, 0, 0.1, 0.0)));
        let mut out = Vec::new();
        for _ in 0..600 {
            pool.step(1.0 / 60.0, &bodies, |_, _, _| None, &mut out);
        }
        assert!(pool.is_empty());
        assert_eq!(out.len(), 1);
        let i = out[0];
        assert_eq!(i.kind, 3);
        assert!(!i.structure);
        assert!(bodies.get(0).altitude(i.at).abs() < 0.05, "{}", bodies.get(0).altitude(i.at));
        // 20 m/s, 2 m up, on a 1 km ball the ground drops x²/2R too: 2 + x²/2000 = 0.81 (x/20)²
        assert!((i.at.x - 36.2).abs() < 1.0, "{}", i.at.x);
    }

    #[test]
    fn structures_stop_rounds_and_spent_rounds_vanish() {
        let bodies = moon();
        let mut pool = Rounds::new(2);
        let up = DVec3::Y * 1500.0;
        pool.fire(round(0, up, DVec3::X, 100.0, 50.0, 0, 0, 0.1, 0.0));
        pool.fire(round(1, up + DVec3::Z * 10.0, DVec3::X, 100.0, 1000.0, 0, 0, 0.1, 0.0));
        assert!(!pool.fire(round(2, up, DVec3::X, 100.0, 1000.0, 0, 0, 0.1, 0.0)), "the pool is full");
        let mut out = Vec::new();
        // a wall 20 m out in front of the second one
        let wall = |from: DVec3, dir: DVec3, len: f64| {
            let t = (up.x + 20.0 - from.x) / dir.x;
            (from.z > up.z + 5.0 && t >= 0.0 && t <= len).then(|| from + dir * t)
        };
        for _ in 0..120 {
            pool.step(1.0 / 60.0, &bodies, wall, &mut out);
        }
        assert!(pool.is_empty());
        assert_eq!(out.len(), 1);
        assert!(out[0].structure && out[0].kind == 1);
        let mut looks = Vec::new();
        pool.looks(&mut looks);
        assert!(looks.is_empty());
    }
}
