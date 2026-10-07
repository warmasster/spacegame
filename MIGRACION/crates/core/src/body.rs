//! Celestial bodies (planets, moons, asteroids) as data, and the registry everything asks what
//! holds where it is now: `BodyRegistry::field` says what pulls at a point and whose ground is
//! under it, the same for whatever asks (structures, people, rounds, particles, traffic) and
//! from nothing but the point. A body's pull reaches as far as its data says and no farther
//! (`ReachDef`), and fades to nothing over a band: past every body's reach nothing pulls, there
//! is no ground and no way up. Where the reaches of two meet, the one that reaches less far
//! takes the place of the other as far as it holds, so going from one to the other is smooth.
use crate::{
    deform::Deform,
    defs::DefError,
    surface::{Surface, SurfaceDef},
};
use glam::DVec3;
use serde::Deserialize;
use std::sync::{Arc, RwLock, RwLockReadGuard};

/// Index into a `BodyRegistry`.
pub type BodyId = u16;

/// How far a body's pull reaches: whole (falling with the square of the distance) up to
/// `to - band` m over its datum, then down smoothly to nothing at `to`. Far short of what a
/// real body's would be: space with nothing pulling begins near.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReachDef {
    pub to: f64,
    pub band: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyDef {
    pub name: String,
    /// World position of the centre (m).
    pub center: [f64; 3],
    pub radius: f64,
    /// Surface gravity (m/s²).
    pub gravity: f64,
    /// How far its pull reaches.
    pub reach: ReachDef,
    /// Where its axis points (world, any length): its north pole. North on its ground is the
    /// way toward it.
    pub north: [f64; 3],
    /// World seed mixed into the surface's own seed (0 = the surface's seed alone).
    #[serde(default)]
    pub seed: u32,
    /// Direction (from the centre) where the ground is levelled to height 0.
    #[serde(default)]
    pub level_at: Option<[f64; 3]>,
    /// Depth under the datum of the sphere the horizon test hides terrain behind (m).
    pub horizon_depth: f64,
    /// Share of the terrain node cache this body's terrain gets (1 = the whole setting).
    #[serde(default = "one")]
    pub terrain_cache: f64,
    #[serde(default)]
    pub surface: Option<SurfaceDef>,
}

fn one() -> f64 {
    1.0
}

pub struct Body {
    pub id: String,
    pub name: String,
    pub center: DVec3,
    pub radius: f64,
    pub gravity: f64,
    /// Its pull is nothing from this far over its datum on (m), and whole under `reach - band`.
    pub reach: f64,
    pub band: f64,
    /// Its axis (unit, world): toward its north pole.
    pub north: DVec3,
    pub horizon_depth: f64,
    pub terrain_cache: f64,
    pub surface: Option<Arc<dyn Surface>>,
    /// Run-time changes to the ground (blast craters), on top of the surface.
    deform: RwLock<Deform>,
}

impl Body {
    pub fn from_def(id: &str, d: &BodyDef) -> Result<Body, DefError> {
        let bad = |m: String| DefError::new(format!("body {id}"), m);
        if d.radius <= 0.0 || !d.radius.is_finite() {
            return Err(bad(format!("radius {}", d.radius)));
        }
        if !(d.reach.to > 0.0 && d.reach.band > 0.0 && d.reach.band <= d.reach.to) {
            return Err(bad(format!("reach to {} band {}: a band within its reach, both over 0", d.reach.to, d.reach.band)));
        }
        let north = DVec3::from_array(d.north).try_normalize().ok_or_else(|| bad("north: an axis of no length".into()))?;
        if let Some(s) = &d.surface {
            s.validate().map_err(bad)?;
        }
        Ok(Body {
            id: id.to_string(),
            name: d.name.clone(),
            center: DVec3::from_array(d.center),
            radius: d.radius,
            gravity: d.gravity,
            reach: d.reach.to,
            band: d.reach.band,
            north,
            horizon_depth: d.horizon_depth,
            terrain_cache: d.terrain_cache.clamp(0.01, 1.0),
            surface: d.surface.as_ref().map(|s| s.build(d.radius, d.seed, d.level_at.map(|v| DVec3::from_array(v).normalize().to_array()))),
            deform: RwLock::default(),
        })
    }

    /// The same body with the ground as it was made (none of its craters): for a game of its own
    /// in a process that has others (a server with a player's game in it, the tests).
    pub fn fresh(&self) -> Body {
        Body {
            id: self.id.clone(),
            name: self.name.clone(),
            center: self.center,
            radius: self.radius,
            gravity: self.gravity,
            reach: self.reach,
            band: self.band,
            north: self.north,
            horizon_depth: self.horizon_depth,
            terrain_cache: self.terrain_cache,
            surface: self.surface.clone(),
            deform: RwLock::default(),
        }
    }

    /// The ground's run-time changes (read).
    pub fn deform(&self) -> RwLockReadGuard<'_, Deform> {
        self.deform.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Change the ground (a crater...): every height query and the terrain see it.
    pub fn edit(&self, f: impl FnOnce(&mut Deform)) {
        f(&mut self.deform.write().unwrap_or_else(|e| e.into_inner()));
    }

    /// The way out from its centre at `p`: the vertical of its ground there. (Which way is up
    /// for something at `p` is another question: `BodyRegistry::field`.)
    pub fn up(&self, p: DVec3) -> DVec3 {
        (p - self.center).normalize_or(DVec3::Y)
    }

    /// North on the horizon where `up` is the way out from its centre, and how much of a north
    /// there is there (0..1: the sine of the angle to its axis; at its poles, none).
    pub fn north_at(&self, up: DVec3) -> (DVec3, f64) {
        let level = self.north - up * self.north.dot(up);
        let much = level.length();
        (if much > 0.0 { level / much } else { DVec3::ZERO }, much)
    }

    /// Ground height over the datum along the unit direction `dir`.
    pub fn height(&self, dir: DVec3) -> f64 {
        self.deform().apply(self.surface.as_ref().map_or(0.0, |s| s.height(dir.to_array())), dir, self.radius)
    }

    /// Ground height without detail under `min_feature` m (cheaper far from the ground).
    pub fn height_at(&self, dir: DVec3, min_feature: f64) -> f64 {
        self.deform().apply(self.surface.as_ref().map_or(0.0, |s| s.sample(dir.to_array(), min_feature).height), dir, self.radius)
    }

    /// World point `h` m over the ground along `dir`.
    pub fn above_ground(&self, dir: DVec3, h: f64) -> DVec3 {
        self.center + dir * (self.radius + self.height(dir) + h)
    }

    /// Height of `p` over the ground under it.
    pub fn altitude(&self, p: DVec3) -> f64 {
        let r = p - self.center;
        r.length() - self.radius - self.height(r.normalize_or(DVec3::Y))
    }

    /// Height of `p` over the datum sphere (cheap: no surface sample).
    pub fn datum_altitude(&self, p: DVec3) -> f64 {
        (p - self.center).length() - self.radius
    }

    /// A level way to count a turn from where `up` is the way out from its centre, the same
    /// for whoever asks: its north; at its poles, where there is none, any level way. For what
    /// is set down or told once (a structure placed, a facing sent to another game): what turns
    /// all the time must not count from it (`north_at` says how much of a north there is).
    pub fn turn_from(&self, up: DVec3) -> DVec3 {
        Some(self.north_at(up).0).filter(|n| *n != DVec3::ZERO).unwrap_or_else(|| up.any_orthonormal_vector())
    }

    /// What is left of its pull `alt` m over its datum (1 where it is whole, 0 from its reach
    /// on). Smooth all the way: its first two rates of change are nil at both ends of the band,
    /// so nothing going in or out of its reach is jolted.
    pub fn hold(&self, alt: f64) -> f64 {
        let x = ((self.reach - alt) / self.band).clamp(0.0, 1.0);
        x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
    }

    /// Its pull `r` m from its centre were its reach endless (m/s²): it falls with the square
    /// of the distance.
    fn whole(&self, r: f64) -> f64 {
        let r = r.max(self.radius * WITHIN);
        self.gravity * (self.radius / r) * (self.radius / r)
    }

    /// Its own pull `alt` m over its datum (m/s²), as if no other body were about: what a
    /// closed form round this one body is worked out with (an orbit's speed). Whatever moves
    /// through space asks `BodyRegistry::field`.
    pub fn own_pull(&self, alt: f64) -> f64 {
        self.whole(self.radius + alt) * self.hold(alt)
    }

    /// The highest over its datum that its pull is whole (m): the band begins there.
    pub fn whole_to(&self) -> f64 {
        self.reach - self.band
    }
}

/// Under this share of a body's radius from its centre its pull grows no more (nothing is
/// ever there: it only keeps the square law from blowing up).
const WITHIN: f64 = 0.5;

/// What holds at a point of space (`BodyRegistry::field`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    /// What pulls there (world, m/s²): nothing past every body's reach.
    pub pull: DVec3,
    /// The body whose ground is under the point: the nearest of those whose reach it is
    /// within. None: there is no ground.
    pub ground: Option<BodyId>,
    /// How much of the point the bodies have between them (0: past every reach, free space;
    /// 1: where some body's pull is whole).
    pub hold: f64,
    /// The body whose surface is nearest, however far: whose ground and sky are drawn. Nothing
    /// that moves is ruled by it.
    pub nearest: BodyId,
}

impl Field {
    /// How hard it pulls (m/s²).
    pub fn g(&self) -> f64 {
        self.pull.length()
    }

    /// The way up: against the pull. None where nothing pulls.
    pub fn up(&self) -> Option<DVec3> {
        (-self.pull).try_normalize()
    }
}

#[derive(Default)]
pub struct BodyRegistry {
    bodies: Vec<Body>,
    /// The bodies from the one whose pull reaches least far from its centre to the one that
    /// reaches farthest: the nearer reach takes the place of the farther one where both hold.
    by_reach: Vec<BodyId>,
}

impl BodyRegistry {
    /// The same bodies, each with its ground as it was made (`Body::fresh`): a game's own.
    pub fn fresh(&self) -> BodyRegistry {
        BodyRegistry { bodies: self.bodies.iter().map(Body::fresh).collect(), by_reach: self.by_reach.clone() }
    }

    pub fn new(bodies: Vec<Body>) -> BodyRegistry {
        assert!(bodies.len() < usize::from(BodyId::MAX));
        let mut by_reach: Vec<BodyId> = (0..bodies.len() as BodyId).collect();
        by_reach.sort_by(|&a, &b| {
            let far = |i: BodyId| bodies[usize::from(i)].radius + bodies[usize::from(i)].reach;
            far(a).total_cmp(&far(b))
        });
        BodyRegistry { bodies, by_reach }
    }

    /// The share each body has of point `p` (0..1, each body with any): `each(body, share,
    /// from its centre, how far from it)`. A body's share is what is left of its pull there
    /// (`Body::hold`) less what the bodies that reach less far have taken: inside the reach of
    /// a moonlet that is within a planet's, it is the moonlet that holds, and across the band
    /// of the one the other comes in as smoothly as that one goes out.
    pub fn shares<'a>(&'a self, p: DVec3, mut each: impl FnMut(BodyId, &'a Body, f64, DVec3, f64)) {
        let mut left = 1.0;
        for &i in &self.by_reach {
            let b = &self.bodies[usize::from(i)];
            let d = p - b.center;
            // (cheap for the bodies that are far: no root)
            let far = b.radius + b.reach;
            if d.length_squared() >= far * far {
                continue;
            }
            let r = d.length();
            let share = b.hold(r - b.radius) * left;
            if share > 0.0 {
                each(i, b, share, d, r);
                left -= share;
            }
        }
    }

    /// What holds at `p`: what pulls there and whose ground is under it. The one answer to
    /// "what gravity, what ground, which way up" for everything, from where the thing is now
    /// and nothing else: not where it was made, not how it got there, not how fast it goes.
    pub fn field(&self, p: DVec3) -> Field {
        let mut f = Field { pull: DVec3::ZERO, ground: None, hold: 0.0, nearest: 0 };
        let (mut low, mut lowest, mut left) = (f64::INFINITY, f64::INFINITY, 1.0);
        for &i in &self.by_reach {
            let b = &self.bodies[usize::from(i)];
            let d = p - b.center;
            let r = d.length();
            let alt = r - b.radius;
            if alt < lowest {
                (lowest, f.nearest) = (alt, i);
            }
            if alt >= b.reach {
                continue;
            }
            if alt < low {
                (low, f.ground) = (alt, Some(i));
            }
            let share = b.hold(alt) * left;
            if share > 0.0 {
                f.pull -= d * (b.whole(r) * share / r.max(f64::MIN_POSITIVE));
                f.hold += share;
                left -= share;
            }
        }
        f
    }

    pub fn get(&self, id: BodyId) -> &Body {
        &self.bodies[usize::from(id)]
    }

    pub fn find(&self, id: &str) -> Option<BodyId> {
        self.bodies.iter().position(|b| b.id == id).map(|i| i as BodyId)
    }

    pub fn iter(&self) -> impl Iterator<Item = (BodyId, &Body)> {
        self.bodies.iter().enumerate().map(|(i, b)| (i as BodyId, b))
    }

    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// First ground hit along a ray from `from` (unit `dir`) within `max` m: the body and the point.
    /// Marches by the altitude (never through a slope under ~60°), then bisects.
    pub fn raycast(&self, from: DVec3, dir: DVec3, max: f64) -> Option<(BodyId, DVec3)> {
        let alt = |t: f64| {
            let p = from + dir * t;
            let b = self.dominant(p);
            (b, self.get(b).altitude(p))
        };
        let (mut t, mut a) = (0.0, alt(0.0).1);
        if a <= 0.0 {
            return Some((self.dominant(from), from));
        }
        while t < max {
            let next = (t + (a * 0.5).max(t * 2e-3).max(0.05)).min(max);
            let (_, an) = alt(next);
            if an <= 0.0 {
                let (mut lo, mut hi) = (t, next);
                for _ in 0..40 {
                    let mid = 0.5 * (lo + hi);
                    if alt(mid).1 > 0.0 { lo = mid } else { hi = mid }
                }
                let b = alt(hi).0;
                return Some((b, from + dir * hi));
            }
            (t, a) = (next, an);
        }
        None
    }

    /// The body whose datum sphere is nearest `p`, however far: whose ground and sky are drawn
    /// from there, where a blast would dig. Not what rules what moves: that is `field`.
    pub fn dominant(&self, p: DVec3) -> BodyId {
        let mut best = (f64::INFINITY, 0);
        for (i, b) in self.bodies.iter().enumerate() {
            let alt = b.datum_altitude(p);
            if alt < best.0 {
                best = (alt, i as BodyId);
            }
        }
        best.1
    }
}
