//! Traffic: thousands of ships going about their business round a site, light enough that
//! every one of them is always somewhere. None is simulated as a ship (no systems, no physics):
//! each is a flight plan — off its pad straight up to its level, along a straight track to
//! another field, straight down onto a pad, a wait, and off again — and where it is at a time is
//! read off its plan. A share of them stay in low orbit and cross the sky.
//!
//! They never meet. A pad holds one ship, kept for whoever is on its way to it. Levels are fixed
//! heights over the highest ground of the region, so two flights at different levels never touch;
//! a flight takes a level only if, at that level, no other comes within `SEPARATION` of it at
//! any time (closest approach of two straight tracks), and nobody's climb or descent crosses its
//! track while it is there, nor its own climb and descent anyone's. If no level is free it waits.
//! All of it is plain geometry worked once per flight, at take-off.
//!
//! Positions are kept in the site's map (metres east and north on its tangent plane) and turned
//! into the world through the site, so it works round any site of any body.
use crate::{
    body::{BodyId, BodyRegistry},
    scene::{Site, basis},
};
use glam::{DQuat, DVec2, DVec3, Quat};
use rayon::prelude::*;
use serde::Deserialize;

/// The traffic of a scenario.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficDef {
    /// Ships in all.
    pub naves: usize,
    /// The fields lie within this far of the site (m).
    pub radio: f64,
    /// Fields, and pads on each (a square of them, `plaza` m apart).
    pub bases: usize,
    pub plazas: usize,
    pub plaza: f64,
    /// The home field (the first) is this far east and north of the site (m): clear of what
    /// stands there.
    pub casa: [f64; 2],
    /// Levels: the lowest and the highest over the region's highest ground (m), and their spacing.
    pub niveles: [f64; 2],
    pub nivel_paso: f64,
    /// Cruise speeds (m/s) and the speed up and down (m/s).
    pub velocidad: [f64; 2],
    pub ascenso: f64,
    /// Seconds on the ground between flights.
    pub espera: [f64; 2],
    /// Share of the ships in low orbit, and their heights (m).
    pub orbita: f64,
    pub orbita_altura: [f64; 2],
    /// How much the fields gather toward the site (0.5: evenly over the region; 2: half of them
    /// within a quarter of its radius), and the share of the flights that go to the home field.
    #[serde(default = "even")]
    pub concentracion: f64,
    #[serde(default)]
    pub casa_reparto: f64,
}

fn even() -> f64 {
    0.5
}

/// No two ships nearer than this in the air (m, centre to centre).
pub const SEPARATION: f64 = 160.0;
/// A pad is free this long after the ship on it has lifted off (s).
const PAD_CLEAR: f64 = 25.0;
/// Seconds before a ship that found no way out looks again.
const RETRY: f64 = 7.0;
/// Flights planned per update at most (the rest wait a frame).
const PLANS_PER_UPDATE: usize = 12;

/// xorshift: the same traffic every run.
#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, [a, b]: [f64; 2]) -> f64 {
        a + (b - a) * self.f()
    }
}

/// A pad: where on the map, how far its ground is from the body's centre, and who has it.
#[derive(Clone, Copy, Debug)]
pub struct Pad {
    pub at: DVec2,
    pub r: f64,
    /// Free from this time on (a ship lifted off it), and whether one stands on it or comes to it.
    free_at: f64,
    taken: bool,
}

/// One flight: up from `from`, along to `to` at its level, down.
#[derive(Clone, Copy, Debug, Default)]
pub struct Flight {
    pub from: DVec2,
    pub to: DVec2,
    /// Distance from the body's centre on the ground at each end, and at its level.
    pub r_from: f64,
    pub r_to: f64,
    pub r_cruise: f64,
    pub level: u32,
    /// Lift-off, top of climb, top of descent, touch-down.
    pub t0: f64,
    pub t1: f64,
    pub t2: f64,
    pub t3: f64,
}

impl Flight {
    fn velocity(&self) -> DVec2 {
        (self.to - self.from) / (self.t2 - self.t1).max(1e-6)
    }

    /// Where it is at `t`: on the map, how far from the body's centre.
    pub fn at(&self, t: f64) -> (DVec2, f64) {
        // a ship eases off the ground and onto it
        let ease = |x: f64| x * x * (3.0 - 2.0 * x);
        if t <= self.t0 {
            (self.from, self.r_from)
        } else if t < self.t1 {
            (self.from, self.r_from + (self.r_cruise - self.r_from) * ease((t - self.t0) / (self.t1 - self.t0)))
        } else if t < self.t2 {
            (self.from + self.velocity() * (t - self.t1), self.r_cruise)
        } else if t < self.t3 {
            (self.to, self.r_cruise + (self.r_to - self.r_cruise) * ease((t - self.t2) / (self.t3 - self.t2)))
        } else {
            (self.to, self.r_to)
        }
    }

    /// The times it is within `band` m (of height) of the level at `r`, going up and coming down.
    fn crossings(&self, r: f64, band: f64) -> [Option<(f64, f64)>; 2] {
        // eased climbs: the times are found on the eased curve by bisection
        let when = |lo: f64, hi: f64, r0: f64, r1: f64, target: f64| -> f64 {
            let (mut a, mut b) = (lo, hi);
            for _ in 0..24 {
                let m = (a + b) * 0.5;
                let x = (m - lo) / (hi - lo).max(1e-9);
                let at = r0 + (r1 - r0) * x * x * (3.0 - 2.0 * x);
                if (at < target) == (r1 > r0) {
                    a = m;
                } else {
                    b = m;
                }
            }
            (a + b) * 0.5
        };
        let span = |lo: f64, hi: f64, r0: f64, r1: f64| -> Option<(f64, f64)> {
            let (bottom, top) = (r0.min(r1), r0.max(r1));
            if r + band < bottom || r - band > top {
                return None;
            }
            let (x, y) = (when(lo, hi, r0, r1, (r - band).clamp(bottom, top)), when(lo, hi, r0, r1, (r + band).clamp(bottom, top)));
            Some((x.min(y), x.max(y)))
        };
        [span(self.t0, self.t1, self.r_from, self.r_cruise), span(self.t2, self.t3, self.r_cruise, self.r_to)]
    }

    /// Whether it and `o` come within `SEPARATION` of each other.
    fn conflicts(&self, o: &Flight, level_gap: f64) -> bool {
        // most are nowhere near, in time or on the map
        if self.t3 < o.t0 || o.t3 < self.t0 {
            return false;
        }
        let (a0, a1) = (self.from.min(self.to) - SEPARATION, self.from.max(self.to) + SEPARATION);
        let (b0, b1) = (o.from.min(o.to), o.from.max(o.to));
        if a1.x < b0.x || b1.x < a0.x || a1.y < b0.y || b1.y < a0.y {
            return false;
        }
        // both along their tracks at the same level: the closest they come while both are
        if self.level == o.level {
            let (a, b) = (self.t1.max(o.t1), self.t2.min(o.t2));
            if a < b {
                let (va, vb) = (self.velocity(), o.velocity());
                let d0 = (self.from + va * (a - self.t1)) - (o.from + vb * (a - o.t1));
                let dv = va - vb;
                let t = if dv.length_squared() > 1e-9 { (-d0.dot(dv) / dv.length_squared()).clamp(0.0, b - a) } else { 0.0 };
                if (d0 + dv * t).length() < SEPARATION {
                    return true;
                }
            }
        }
        // one going up or down through the other's level while the other passes by
        let through = |column: &Flight, track: &Flight| -> bool {
            let ends = [column.from, column.to];
            for (k, span) in column.crossings(track.r_cruise, SEPARATION.max(level_gap * 0.5)).into_iter().enumerate() {
                let Some((ta, tb)) = span else { continue };
                let (a, b) = (ta.max(track.t1), tb.min(track.t2));
                if a > b {
                    continue;
                }
                // the track's nearest to the column's foot in that while
                let v = track.velocity();
                let d0 = track.from + v * (a - track.t1) - ends[k];
                let t = if v.length_squared() > 1e-9 { (-d0.dot(v) / v.length_squared()).clamp(0.0, b - a) } else { 0.0 };
                if (d0 + v * t).length() < SEPARATION {
                    return true;
                }
            }
            false
        };
        through(self, o) || through(o, self)
    }
}

/// What a ship is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Doing {
    /// On pad `pad` of field `base`, until `until` (then it looks for a way out).
    Parked { base: u32, pad: u32, until: f64 },
    /// On its flight, to pad `pad` of field `base`.
    Flying { base: u32, pad: u32 },
    /// Round the body: the plane's axes, the radius, the turn rate and the phase.
    Orbit { a: DVec3, b: DVec3, r: f64, omega: f64, phase: f64 },
}

pub struct Traffic {
    pub body: BodyId,
    site: Site,
    def: TrafficDef,
    /// The fields' pads.
    pub pads: Vec<Vec<Pad>>,
    levels: Vec<f64>,
    doing: Vec<Doing>,
    flights: Vec<Flight>,
    /// Flights in the air or due, by level (ship indices).
    by_level: Vec<Vec<u32>>,
    /// Which model each ship is (an index the owner maps to its models).
    pub kind: Vec<u8>,
    pub pos: Vec<DVec3>,
    pub rot: Vec<Quat>,
    rng: Rng,
    /// Flights flown to their end so far, and flights that had to wait for a free level.
    pub landed: u64,
    pub held: u64,
}

impl Traffic {
    /// `kinds`: how many models there are to choose from. `warm`: seconds of traffic already
    /// gone by when it starts at time 0 (so the sky is not empty at first).
    pub fn new(bodies: &BodyRegistry, site: &Site, def: &TrafficDef, kinds: usize, seed: u64, warm: f64) -> Traffic {
        let body = site.body;
        let b = bodies.get(body);
        let mut rng = Rng(seed | 1);
        let per_side = (def.plazas as f64).sqrt().ceil().max(1.0) as usize;
        // the fields: the home one by the site, the rest thicker toward it
        let field = def.plaza * per_side as f64;
        let mut centres: Vec<DVec2> = vec![DVec2::from_array(def.casa)];
        let mut tries = 0;
        while centres.len() < def.bases.max(1) && tries < def.bases * 200 {
            tries += 1;
            // thicker toward the site: half of them within a quarter of the way out
            let (r, a) = (def.radio * rng.f().powf(def.concentracion.max(0.5)), rng.f() * std::f64::consts::TAU);
            let c = DVec2::new(a.cos(), a.sin()) * r;
            // no field over another, nor over the site
            if c.length() > field * 2.0 && centres.iter().all(|o| o.distance(c) > field * 1.6) {
                centres.push(c);
            }
        }
        let ground = |p: DVec2| b.radius + b.height(site.at(p.x, p.y));
        let pads: Vec<Vec<Pad>> = centres
            .par_iter()
            .map(|c| {
                (0..def.plazas)
                    .map(|k| {
                        let (i, j) = ((k % per_side) as f64 - (per_side as f64 - 1.0) * 0.5, (k / per_side) as f64 - (per_side as f64 - 1.0) * 0.5);
                        let at = *c + DVec2::new(i, j) * def.plaza;
                        Pad { at, r: ground(at), free_at: f64::MIN, taken: false }
                    })
                    .collect()
            })
            .collect();
        // the highest ground of the region (a coarse look is enough: levels start well over it)
        let cells = 48;
        let top = (0..cells * cells)
            .into_par_iter()
            .map(|k| {
                let (i, j) = ((k % cells) as f64 / (cells - 1) as f64 * 2.0 - 1.0, (k / cells) as f64 / (cells - 1) as f64 * 2.0 - 1.0);
                ground(DVec2::new(i, j) * def.radio * 1.05)
            })
            .reduce(|| f64::MIN, f64::max)
            .max(pads.iter().flatten().map(|p| p.r).fold(f64::MIN, f64::max));
        let n_levels = ((def.niveles[1] - def.niveles[0]) / def.nivel_paso.max(1.0)).floor().max(0.0) as usize + 1;
        let levels: Vec<f64> = (0..n_levels).map(|k| top + def.niveles[0] + def.nivel_paso * k as f64).collect();
        let n = def.naves;
        let orbiters = ((n as f64) * def.orbita).round() as usize;
        let mut t = Traffic {
            body,
            site: *site,
            def: def.clone(),
            pads,
            levels,
            doing: Vec::with_capacity(n),
            flights: vec![Flight::default(); n],
            by_level: vec![Vec::new(); n_levels],
            kind: (0..n).map(|k| (k % kinds.max(1)) as u8).collect(),
            pos: vec![DVec3::ZERO; n],
            rot: vec![Quat::IDENTITY; n],
            rng,
            landed: 0,
            held: 0,
        };
        // each ship on a pad of its own, its first flight due some time in the wait (so they do
        // not all leave at once); the orbiters each on its own height
        let spots: Vec<(u32, u32)> = (0..t.pads.len()).flat_map(|bi| (0..t.pads[bi].len()).map(move |pi| (bi as u32, pi as u32))).collect();
        for k in 0..n {
            if k < orbiters {
                // (an orbit is where the body's pull is whole: no higher than that)
                let h = (def.orbita_altura[0] + (def.orbita_altura[1] - def.orbita_altura[0]) * (k as f64 + 0.5) / orbiters as f64).min(b.whole_to());
                let r = b.radius + h;
                // a plane through near the site: it crosses its sky
                let lean = DVec3::new(t.rng.f() - 0.5, t.rng.f() - 0.5, t.rng.f() - 0.5) * 0.5;
                let a = (site.dir + lean).normalize();
                let any = DVec3::new(t.rng.f() - 0.5, t.rng.f() - 0.5, t.rng.f() - 0.5).normalize_or(DVec3::X);
                let bb = a.cross(any).normalize_or(a.any_orthonormal_vector());
                let omega = (b.own_pull(h) / r).sqrt() * if t.rng.f() < 0.5 { -1.0 } else { 1.0 };
                t.doing.push(Doing::Orbit { a, b: bb, r, omega, phase: t.rng.f() * std::f64::consts::TAU });
                continue;
            }
            // spread over the pads (a prime stride: every field gets its share)
            let (base, pad) = spots[(k * 7919) % spots.len().max(1)];
            let (base, pad) = match t.pads[base as usize][pad as usize].taken {
                false => (base, pad),
                true => spots.iter().copied().find(|&(bi, pi)| !t.pads[bi as usize][pi as usize].taken).unwrap_or((base, pad)),
            };
            t.pads[base as usize][pad as usize].taken = true;
            let until = -warm + t.rng.f() * def.espera[1];
            t.doing.push(Doing::Parked { base, pad, until });
            let p = t.pads[base as usize][pad as usize];
            t.flights[k] = Flight { from: p.at, to: p.at, r_from: p.r, r_to: p.r, r_cruise: p.r, ..Flight::default() };
        }
        let steps = (warm / 2.0).ceil() as usize;
        for s in 0..steps {
            t.update(bodies, -warm + 2.0 * s as f64);
        }
        t.place(bodies, 0.0);
        t
    }

    pub fn len(&self) -> usize {
        self.doing.len()
    }

    pub fn is_empty(&self) -> bool {
        self.doing.is_empty()
    }

    /// Ships in the air (on a flight) at `t`, and in orbit.
    pub fn flying(&self, t: f64) -> (usize, usize) {
        let air = (0..self.len()).filter(|&k| matches!(self.doing[k], Doing::Flying { .. }) && t > self.flights[k].t0).count();
        (air, self.doing.iter().filter(|d| matches!(d, Doing::Orbit { .. })).count())
    }

    /// A way out for ship `k` standing on `(base, pad)` at `now`: a field, a pad there, a level
    /// nobody is in the way at. None: nothing free now.
    fn plan(&mut self, k: usize, base: u32, pad: u32, now: f64) -> Option<(Flight, u32, u32)> {
        let from = self.pads[base as usize][pad as usize];
        // somewhere else, the nearer fields more often
        let n = self.pads.len();
        if n < 2 {
            return None;
        }
        let mut to_base = base as usize;
        // the home field is where many go (while it has room)
        if base != 0 && self.rng.f() < self.def.casa_reparto && self.pads[0].iter().any(|p| !p.taken) {
            to_base = 0;
        }
        for _ in 0..6 {
            if to_base != base as usize {
                break;
            }
            let pick = (self.rng.f() * n as f64) as usize % n;
            if pick == base as usize {
                continue;
            }
            let d = self.pads[pick][0].at.distance(from.at);
            // far fields are taken less often (but they are)
            if self.rng.f() < (0.25 + 0.75 * (1.0 - d / (2.0 * self.def.radio)).clamp(0.0, 1.0)) {
                to_base = pick;
                break;
            }
        }
        if to_base == base as usize {
            return None;
        }
        let speed = self.rng.range(self.def.velocidad);
        let dist = self.pads[to_base][0].at.distance(from.at);
        let first = (self.rng.f() * self.levels.len() as f64) as usize % self.levels.len().max(1);
        let gap = self.def.nivel_paso;
        for step in 0..self.levels.len() {
            let level = (first + step) % self.levels.len();
            let r_cruise = self.levels[level];
            let up = (r_cruise - from.r) / self.def.ascenso;
            let (t0, t1) = (now, now + up);
            let t2 = t1 + dist / speed;
            // a pad free by the time it gets there
            let Some(to_pad) = (0..self.pads[to_base].len()).find(|&i| {
                let p = &self.pads[to_base][i];
                !p.taken && p.free_at < t2
            }) else {
                return None;
            };
            let to = self.pads[to_base][to_pad];
            let t2 = t1 + to.at.distance(from.at) / speed;
            let t3 = t2 + (r_cruise - to.r) / self.def.ascenso;
            let f = Flight { from: from.at, to: to.at, r_from: from.r, r_to: to.r, r_cruise, level: level as u32, t0, t1, t2, t3 };
            // nobody in the way: at its level, and through the levels it climbs and comes down by
            let clear = self.by_level.iter().all(|list| list.iter().all(|&o| o as usize == k || self.flights[o as usize].t3 < now || !f.conflicts(&self.flights[o as usize], gap)));
            if clear {
                return Some((f, to_base as u32, to_pad as u32));
            }
        }
        None
    }

    /// The traffic at `t`: ships that have landed wait, ships that have waited leave (if there is
    /// a way), and every one is put where its plan says.
    pub fn update(&mut self, bodies: &BodyRegistry, t: f64) {
        let mut plans = 0;
        for k in 0..self.len() {
            match self.doing[k] {
                Doing::Flying { base, pad } if t >= self.flights[k].t3 => {
                    let wait = self.rng.range(self.def.espera);
                    self.doing[k] = Doing::Parked { base, pad, until: self.flights[k].t3 + wait };
                    let level = self.flights[k].level as usize;
                    self.by_level[level].retain(|&o| o as usize != k);
                    self.landed += 1;
                }
                Doing::Parked { base, pad, until } if t >= until && plans < PLANS_PER_UPDATE => {
                    plans += 1;
                    match self.plan(k, base, pad, t) {
                        Some((f, to_base, to_pad)) => {
                            let here = &mut self.pads[base as usize][pad as usize];
                            here.taken = false;
                            here.free_at = f.t0 + PAD_CLEAR;
                            self.pads[to_base as usize][to_pad as usize].taken = true;
                            self.by_level[f.level as usize].push(k as u32);
                            self.flights[k] = f;
                            self.doing[k] = Doing::Flying { base: to_base, pad: to_pad };
                        }
                        None => {
                            self.held += 1;
                            self.doing[k] = Doing::Parked { base, pad, until: t + RETRY };
                        }
                    }
                }
                _ => {}
            }
        }
        self.place(bodies, t);
    }

    /// Every ship where it is at `t`.
    fn place(&mut self, bodies: &BodyRegistry, t: f64) {
        let b = bodies.get(self.body);
        let (site, doing, flights) = (&self.site, &self.doing, &self.flights);
        self.pos.par_iter_mut().zip(self.rot.par_iter_mut()).enumerate().with_min_len(256).for_each(|(k, (pos, rot))| match doing[k] {
            Doing::Orbit { a, b: bb, r, omega, phase } => {
                let (s, c) = (phase + omega * t).sin_cos();
                let dir = a * c + bb * s;
                *pos = b.center + dir * r;
                *rot = basis(dir, (bb * c - a * s) * omega.signum());
            }
            _ => {
                let f = &flights[k];
                let (at, r) = f.at(t);
                let dir = site.at(at.x, at.y);
                *pos = b.center + dir * r;
                // nose along its track (as it came in, once down)
                let track = f.to - f.from;
                let ahead = if track.length_squared() > 1e-6 { site.east * track.x + site.north * track.y } else { site.north };
                *rot = basis(dir, ahead);
            }
        });
    }

    /// The nearest two ships are to each other now (m), and which (a slow look at every pair
    /// near each other: for tests and tools).
    pub fn closest(&self) -> (f64, usize, usize) {
        self.closest_of(|_| true)
    }

    /// The same among the ships `which` picks.
    pub fn closest_of(&self, which: impl Fn(usize) -> bool) -> (f64, usize, usize) {
        let cell = SEPARATION * 2.0;
        let key = |p: DVec3| ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64);
        let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<u32>> = std::collections::HashMap::new();
        for (k, p) in self.pos.iter().enumerate().filter(|(k, _)| which(*k)) {
            grid.entry(key(*p)).or_default().push(k as u32);
        }
        let mut best = (f64::MAX, 0, 0);
        for (k, p) in self.pos.iter().enumerate().filter(|(k, _)| which(*k)) {
            let (x, y, z) = key(*p);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let Some(list) = grid.get(&(x + dx, y + dy, z + dz)) else {
                            continue;
                        };
                        for &o in list {
                            if (o as usize) > k {
                                let d = p.distance(self.pos[o as usize]);
                                if d < best.0 {
                                    best = (d, k, o as usize);
                                }
                            }
                        }
                    }
                }
            }
        }
        best
    }

    /// Ship `k` is off the ground (on a flight or in orbit).
    pub fn airborne(&self, k: usize, t: f64) -> bool {
        match self.doing[k] {
            Doing::Orbit { .. } => true,
            Doing::Flying { .. } => t > self.flights[k].t0 && t < self.flights[k].t3,
            Doing::Parked { .. } => false,
        }
    }

    /// Ship `k` is along its track at its level (not climbing nor coming down).
    pub fn cruising(&self, k: usize, t: f64) -> bool {
        matches!(self.doing[k], Doing::Flying { .. }) && t > self.flights[k].t1 && t < self.flights[k].t2
    }

    /// How far ship `k` is over the ground under it (m).
    pub fn height(&self, bodies: &BodyRegistry, k: usize) -> f64 {
        bodies.get(self.body).altitude(self.pos[k])
    }

    /// The turn that takes the body's frame into a ship's (for whoever wants its axes).
    pub fn frame(&self, k: usize) -> DQuat {
        self.rot[k].as_dquat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        body::{Body, BodyDef},
        defs,
    };

    fn world() -> (BodyRegistry, Site) {
        let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
        let site = Site::new(0, bodies.get(0), DVec3::Y);
        (bodies, site)
    }

    fn def(naves: usize) -> TrafficDef {
        TrafficDef {
            naves,
            radio: 30_000.0,
            bases: 24,
            plazas: 49,
            plaza: 60.0,
            casa: [400.0, 300.0],
            niveles: [300.0, 1500.0],
            nivel_paso: 100.0,
            velocidad: [80.0, 220.0],
            ascenso: 30.0,
            espera: [10.0, 60.0],
            orbita: 0.05,
            orbita_altura: [20_000.0, 60_000.0],
            concentracion: 1.5,
            casa_reparto: 0.15,
        }
    }

    #[test]
    fn ships_fly_land_wait_and_never_meet() {
        let (bodies, site) = world();
        let mut t = Traffic::new(&bodies, &site, &def(800), 3, 42, 0.0);
        assert_eq!(t.len(), 800);
        // each on a pad of its own (or in orbit)
        let (near, a, b) = t.closest();
        assert!(near > 30.0, "{a} y {b} empiezan a {near:.1} m");
        // started with ten minutes behind it, the sky is busy from the first moment
        let warm = Traffic::new(&bodies, &site, &def(800), 3, 42, 600.0);
        assert!(warm.flying(0.0).0 > 150, "solo {} en el aire al empezar", warm.flying(0.0).0);
        let (mut worst, mut most) = (f64::MAX, 0);
        let dt = 0.5;
        let steps = (20.0 * 60.0 / dt) as usize;
        for s in 0..steps {
            let now = s as f64 * dt;
            t.update(&bodies, now);
            // never one through another: on the ground and over their pads, a pad apart
            let (d, a, b) = t.closest();
            assert!(d > 45.0, "a los {now:.0} s las naves {a} y {b} están a {d:.1} m ({:?} / {:?})", t.doing[a], t.doing[b]);
            // along their tracks, nothing nearer than the separation (less what the samples of
            // two tracks miss between steps)
            let (d, a, b) = t.closest_of(|k| t.cruising(k, now));
            assert!(d > SEPARATION * 0.6, "a los {now:.0} s las naves {a} y {b}, en ruta, están a {d:.1} m ({:?} / {:?})", t.flights[a], t.flights[b]);
            // and one along its track never near one going up or down
            let (d, a, b) = t.closest_of(|k| t.airborne(k, now));
            assert!(d > 45.0 && (d > SEPARATION * 0.6 || !(t.cruising(a, now) || t.cruising(b, now))), "a los {now:.0} s las naves {a} y {b} están a {d:.1} m en el aire");
            worst = worst.min(d);
            most = most.max(t.flying(now).0);
            if s % 240 == 0 {
                // nobody under the ground
                for k in (0..t.len()).step_by(37) {
                    assert!(t.height(&bodies, k) > -0.5, "la nave {k} está a {:.1} m del suelo", t.height(&bodies, k));
                }
            }
        }
        println!("20 min de tráfico: {} vuelos acabados, {} esperas por falta de nivel, hasta {most} en el aire; lo más cerca {worst:.0} m", t.landed, t.held);
        assert!(t.landed > 400, "solo {} vuelos acabados", t.landed);
        assert!(most > 200, "solo {most} naves en el aire a la vez");
        assert_eq!(t.flying(0.0).1, 40);
    }

    #[test]
    fn a_flight_is_up_along_and_down() {
        let f = Flight { from: DVec2::ZERO, to: DVec2::new(3000.0, 4000.0), r_from: 1000.0, r_to: 1100.0, r_cruise: 2000.0, level: 3, t0: 10.0, t1: 40.0, t2: 90.0, t3: 120.0 };
        assert_eq!(f.at(0.0), (DVec2::ZERO, 1000.0));
        assert_eq!(f.at(25.0), (DVec2::ZERO, 1500.0));
        assert_eq!(f.at(65.0), (DVec2::new(1500.0, 2000.0), 2000.0));
        assert_eq!(f.at(200.0), (DVec2::new(3000.0, 4000.0), 1100.0));
        // the same track a little later at the same level is in the way; another level is not
        let late = Flight { t0: f.t0 + 0.5, t1: f.t1 + 0.5, t2: f.t2 + 0.5, t3: f.t3 + 0.5, ..f };
        assert!(f.conflicts(&late, 100.0));
        assert!(!f.conflicts(&Flight { level: 4, r_cruise: 2100.0, from: DVec2::new(9000.0, 0.0), to: DVec2::new(9000.0, 4000.0), ..late }, 100.0));
        // one crossing its track at the same level far behind it is not; one climbing through its
        // level right under it as it passes is
        let crossing = Flight { from: DVec2::new(0.0, 4000.0), to: DVec2::new(3000.0, 0.0), ..f };
        assert!(f.conflicts(&crossing, 100.0), "they meet in the middle");
        let behind = Flight { t0: f.t0 + 60.0, t1: f.t1 + 60.0, t2: f.t2 + 60.0, t3: f.t3 + 60.0, ..crossing };
        assert!(!f.conflicts(&behind, 100.0));
        let climber = Flight { from: DVec2::new(1500.0, 2000.0), to: DVec2::new(1500.0, 9000.0), r_from: 1000.0, r_to: 1000.0, r_cruise: 3000.0, level: 13, t0: 35.0, t1: 95.0, t2: 200.0, t3: 260.0 };
        assert!(f.conflicts(&climber, 100.0), "it climbs through the level as the other passes over its pad");
    }
}
