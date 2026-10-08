//! What homes and what fools what homes (`guiados.jsonc`, `senuelos.jsonc`).
//!
//! A **guided missile** is a point mass with a motor and a seeker. While its motor burns it
//! pushes along the line to what it follows; all its flight it steers by proportional
//! navigation on the miss it would make if nobody did anything more (`N` times that miss over
//! the time to go, squared), up to what it can pull. Its seeker follows one thing and sees only
//! what is in front of it: something brighter in its band beside its target takes it, and what
//! leaves its field is lost. It goes off against what it strikes or when it passes as near as
//! its fuse reaches.
//!
//! A **decoy** is a thing let go that shines for a while in one band: chaff reflects (a cloud of
//! dipoles that opens and, opening, thins: with no air to stop it it keeps going with the speed
//! it was thrown with, so it works for as long as the cloud is dense enough), a flare burns, an
//! active decoy answers the radar that looks at it. Each is a point with a signature that grows
//! (`florece`) and dies away (`dura`).
//!
//! Neither knows what a ship is: whoever owns them says where each target is (`Aim`) and takes
//! the strikes. Fixed pools, flat loops, nothing allocated in flight.
use crate::{
    body::BodyRegistry,
    defs::{self, DefError},
    missiles::gravity,
};
use glam::DVec3;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

/// Ids of decoys, as targets and contacts: this bit and their number.
pub const DECOY_ID: u64 = 1 << 63;
/// Ids of guided missiles, as contacts: this bit and their number.
pub const MISSILE_ID: u64 = 1 << 62;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    /// It follows what reflects its own radar (it radiates: a warner hears it).
    Radar,
    /// It follows what is hot.
    Calor,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuidedDef {
    pub name: String,
    pub seeker: Band,
    /// Its motor: push (m/s²) and how long it burns (s).
    pub accel: f64,
    pub burn: f64,
    /// The most it steers with (m/s²), its navigation constant, how long it lives (s).
    pub turn: f64,
    #[serde(default = "nav")]
    pub nav: f64,
    pub life: f64,
    /// Half the field of its seeker (deg), and how near it has to pass to go off (m).
    pub fov: f64,
    pub fuse: f64,
    /// Seconds after leaving its rail before it can strike anything (clear of who fired it).
    #[serde(default = "arm")]
    pub arm: f64,
    pub mass: f64,
    /// The explosive it carries (its explosion is made from it).
    pub charge: crate::detonation::ChargeDef,
    #[serde(default)]
    pub look: Option<crate::effect_defs::RoundLook>,
    #[serde(default)]
    pub trail: Option<String>,
    #[serde(default)]
    pub trail_rate: f32,
}

fn nav() -> f64 {
    4.0
}
fn arm() -> f64 {
    0.6
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecoyDef {
    pub name: String,
    /// What it shines at its brightest: what it reflects (m²) and what it radiates (W).
    #[serde(default)]
    pub rcs: f32,
    #[serde(default)]
    pub heat: f32,
    /// Seconds it takes to open, and to die away after.
    pub florece: f32,
    pub dura: f32,
    /// How fast it leaves the dispenser (m/s).
    pub salida: f32,
    /// What it is seen as: a particle style, how many puffs a second, their size (m) and life (s).
    #[serde(default)]
    pub look: Option<String>,
    #[serde(default)]
    pub rate: f32,
    #[serde(default = "one")]
    pub size: f32,
    #[serde(default = "one")]
    pub life: f32,
}

fn one() -> f32 {
    1.0
}

/// Where what a seeker follows is, how it moves and how bright it is in each band.
#[derive(Clone, Copy, Debug)]
pub struct Aim {
    pub pos: DVec3,
    pub vel: DVec3,
    pub rcs: f32,
    pub heat: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Guided {
    pub kind: u16,
    pub pos: DVec3,
    pub vel: DVec3,
    /// Flight time (s).
    pub t: f64,
    /// What it follows (a structure, a decoy...: whatever its owner's `aim` answers for).
    pub target: Option<u64>,
    /// The structure that fired it (it cannot strike it before it is armed).
    pub shooter: u64,
    /// Seconds since it last saw what it follows, and until its seeker looks again.
    pub blind: f32,
    pub look_in: f32,
    pub id: u32,
    /// Whose it is, as its launcher numbers what it lets fly (0: nobody's); it comes back with
    /// its hit.
    pub tag: u32,
    /// What its seeker and motor push it with besides what pulls it, as of its last step (m/s²).
    pub push: DVec3,
    /// Led from outside (a copy of one flown elsewhere): no seeker of its own, this push instead.
    pub led: Option<DVec3>,
}

#[derive(Clone, Copy, Debug)]
pub struct Decoy {
    pub kind: u16,
    pub pos: DVec3,
    pub vel: DVec3,
    pub age: f32,
    pub id: u32,
}

/// A guided missile that went off: where, how fast it was going, against what.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub kind: u16,
    pub at: DVec3,
    pub vel: DVec3,
    /// The structure struck, if it struck one (a near miss names none).
    pub structure: Option<u64>,
    pub energy: f64,
    /// The missile's own id (`Guided::id`) and its launcher's number for it (`Guided::tag`).
    pub id: u32,
    pub tag: u32,
}

/// Fixed flight step (s) and how often a seeker looks again (s).
pub const STEP: f64 = 1.0 / 60.0;
const LOOK: f32 = 0.15;
/// A seeker is taken by something this many times brighter than what it follows, if it is
/// within this many metres of it or this near its line of sight (cosine).
const TAKEN: (f32, f64, f64) = (1.5, 400.0, 0.9986);

pub struct Flight {
    pub defs: Vec<(String, GuidedDef)>,
    pub decoy_defs: Vec<(String, DecoyDef)>,
    pub list: Vec<Guided>,
    pub decoys: Vec<Decoy>,
    capacity: (usize, usize),
    acc: f64,
    next: u32,
}

impl Flight {
    pub fn load(dir: &Path) -> Result<Flight, DefError> {
        let read = |name: &str| defs::file(dir, name);
        let g: BTreeMap<String, GuidedDef> = if read("guiados").exists() { defs::load(&read("guiados"))? } else { BTreeMap::new() };
        let d: BTreeMap<String, DecoyDef> = if read("senuelos").exists() { defs::load(&read("senuelos"))? } else { BTreeMap::new() };
        Ok(Flight::new(g.into_iter().collect(), d.into_iter().collect()))
    }

    pub fn new(defs: Vec<(String, GuidedDef)>, decoy_defs: Vec<(String, DecoyDef)>) -> Flight {
        let capacity = (256, 512);
        Flight { defs, decoy_defs, list: Vec::with_capacity(capacity.0), decoys: Vec::with_capacity(capacity.1), capacity, acc: 0.0, next: 1 }
    }

    pub fn kind(&self, id: &str) -> Option<u16> {
        self.defs.iter().position(|(k, _)| k == id).map(|k| k as u16)
    }

    pub fn decoy_kind(&self, id: &str) -> Option<u16> {
        self.decoy_defs.iter().position(|(k, _)| k == id).map(|k| k as u16)
    }

    /// A missile of `kind` leaving `from` at `vel` (its launcher's speed and its own push off the
    /// rail), fired by structure `shooter` at `target`. False when the pool is full.
    pub fn launch(&mut self, kind: u16, from: DVec3, vel: DVec3, target: Option<u64>, shooter: u64) -> bool {
        if self.list.len() >= self.capacity.0 {
            return false;
        }
        self.next = self.next.wrapping_add(1).max(1);
        self.list.push(Guided { kind, pos: from, vel, t: 0.0, target, shooter, blind: 0.0, look_in: 0.0, id: self.next, tag: 0, push: DVec3::ZERO, led: None });
        true
    }

    /// Missile `g` as it was (a game kept on disk taken up again): in flight again, its id not
    /// given again.
    pub fn put_back(&mut self, g: Guided) {
        self.next = self.next.max(g.id);
        self.list.push(g);
    }

    /// Decoy `d` as it was, the same way.
    pub fn put_back_decoy(&mut self, d: Decoy) {
        self.next = self.next.max(d.id);
        self.decoys.push(d);
    }

    /// A decoy of `kind` let go at `from` with `vel`. False when the pool is full.
    pub fn release(&mut self, kind: u16, from: DVec3, vel: DVec3) -> bool {
        if self.decoys.len() >= self.capacity.1 {
            return false;
        }
        self.next = self.next.wrapping_add(1).max(1);
        self.decoys.push(Decoy { kind, pos: from, vel, age: 0.0, id: self.next });
        true
    }

    /// What decoy `d` shines with now: (m², W).
    pub fn shine(&self, d: &Decoy) -> (f32, f32) {
        let def = &self.decoy_defs[usize::from(d.kind)].1;
        let k = if d.age < def.florece { d.age / def.florece.max(1e-3) } else { (1.0 - (d.age - def.florece) / def.dura.max(1e-3)).max(0.0) };
        (def.rcs * k, def.heat * k)
    }

    /// Where decoy `id` is and what it shines with.
    pub fn decoy_aim(&self, id: u64) -> Option<Aim> {
        let n = (id & !DECOY_ID) as u32;
        let d = self.decoys.iter().find(|d| d.id == n)?;
        let (rcs, heat) = self.shine(d);
        Some(Aim { pos: d.pos, vel: d.vel, rcs, heat })
    }

    /// `dt` on, in fixed steps. `aim(id)`: where a target is (none: gone). `structures(from,
    /// dir, len)`: the first structure a step's path strikes, where and which. Missiles that go
    /// off are appended to `hits`.
    pub fn update(&mut self, dt: f64, bodies: &BodyRegistry, aim: impl Fn(u64) -> Option<Aim>, mut structures: impl FnMut(DVec3, DVec3, f64) -> Option<(DVec3, u64)>, hits: &mut Vec<Hit>) {
        if self.list.is_empty() && self.decoys.is_empty() {
            self.acc = 0.0;
            return;
        }
        self.acc = (self.acc + dt).min(STEP * 8.0);
        while self.acc >= STEP {
            self.acc -= STEP;
            // decoys drift and age
            let mut k = 0;
            while k < self.decoys.len() {
                let def = &self.decoy_defs[usize::from(self.decoys[k].kind)].1;
                let d = &mut self.decoys[k];
                d.vel += gravity(bodies, d.pos) * STEP;
                d.pos += d.vel * STEP;
                d.age += STEP as f32;
                if d.age > def.florece + def.dura {
                    self.decoys.swap_remove(k);
                } else {
                    k += 1;
                }
            }
            let mut k = 0;
            while k < self.list.len() {
                match self.fly(k, bodies, &aim, &mut structures) {
                    Some(hit) => {
                        hits.push(hit);
                        self.list.swap_remove(k);
                    }
                    None if self.list[k].t > self.defs[usize::from(self.list[k].kind)].1.life => {
                        self.list.swap_remove(k);
                    }
                    None => k += 1,
                }
            }
        }
    }

    /// The newest missile flown `secs` on at once, as `update` flies it (one let go that long
    /// ago somewhere else, seen here only now): what it would strike on the way is not looked for.
    pub fn catch_up(&mut self, secs: f64, bodies: &BodyRegistry, aim: impl Fn(u64) -> Option<Aim>) {
        let Some(k) = self.list.len().checked_sub(1) else { return };
        for _ in 0..((secs / STEP).round() as usize).min(240) {
            let _ = self.fly(k, bodies, &aim, &mut |_, _, _| None);
        }
    }

    /// One step of missile `k`: its seeker, its steering, its motor; what it strikes.
    fn fly(&mut self, k: usize, bodies: &BodyRegistry, aim: &impl Fn(u64) -> Option<Aim>, structures: &mut impl FnMut(DVec3, DVec3, f64) -> Option<(DVec3, u64)>) -> Option<Hit> {
        let mut m = self.list[k];
        let def = &self.defs[usize::from(m.kind)].1;
        let seen = |a: &Aim| if def.seeker == Band::Radar { a.rcs } else { a.heat };
        let nose = m.vel.normalize_or(DVec3::Y);
        // (a decoy it follows is this module's own: its owner answers for the rest)
        let mut target = m.target.and_then(|id| if id & DECOY_ID != 0 { self.decoy_aim(id) } else { aim(id) });
        // the seeker: what it follows must be in front of it; something brighter beside it takes it
        m.look_in -= STEP as f32;
        if m.look_in <= 0.0 {
            m.look_in = LOOK;
            if let Some(a) = target {
                let los = (a.pos - m.pos).normalize_or(nose);
                let mut best = seen(&a) * TAKEN.0;
                for d in &self.decoys {
                    let (rcs, heat) = self.shine(d);
                    let s = if def.seeker == Band::Radar { rcs } else { heat };
                    if s <= best {
                        continue;
                    }
                    let near = d.pos.distance_squared(a.pos) < TAKEN.1 * TAKEN.1 || (d.pos - m.pos).normalize_or(nose).dot(los) > TAKEN.2;
                    if near {
                        best = s;
                        m.target = Some(DECOY_ID | u64::from(d.id));
                        target = Some(Aim { pos: d.pos, vel: d.vel, rcs, heat });
                    }
                }
            }
        }
        let fov = def.fov.to_radians().cos();
        let visible = target.filter(|a| seen(a) > 0.0 && m.t < 0.5 || (a.pos - m.pos).normalize_or(nose).dot(nose) >= fov && seen(a) > 0.0);
        m.blind = if visible.is_some() { 0.0 } else { m.blind + STEP as f32 };
        if m.blind > 1.0 {
            // lost: it flies on with nothing to follow
            m.target = None;
        }
        let pull = gravity(bodies, m.pos);
        let mut a = pull;
        let mut fuse = None;
        if let Some(led) = m.led {
            a += led;
        } else if let Some(t) = visible {
            let (r, v) = (t.pos - m.pos, t.vel - m.vel);
            let range = r.length().max(1e-3);
            let closing = (-v.dot(r) / range).max(1.0);
            let go = (range / closing).clamp(STEP, 30.0);
            // the miss if nobody did anything more, across the line to it
            let miss = r + v * go;
            let los = r / range;
            let across = miss - los * miss.dot(los);
            a += (across * (def.nav / (go * go))).clamp_length_max(def.turn);
            // (its motor along the line to it: the steering takes care of the miss. Nothing here
            // is said of how fast either goes over the ground: two things in the same orbit
            // meet as two things at rest do)
            if m.t < def.burn {
                a += los * def.accel;
            }
            if range < def.fuse || (v.dot(r) > 0.0 && range < def.fuse * 3.0) {
                fuse = Some(m.pos);
            }
        } else if m.t < def.burn {
            a += nose * def.accel;
        }
        m.push = a - pull;
        let p1 = m.pos + m.vel * STEP + a * (0.5 * STEP * STEP);
        let v1 = m.vel + a * STEP;
        let hit = |at: DVec3, structure: Option<u64>| Hit { kind: m.kind, at, vel: v1, structure, energy: 0.5 * def.mass * v1.length_squared(), id: m.id, tag: m.tag };
        if m.t >= def.arm {
            if let Some(at) = fuse {
                return Some(hit(at, None));
            }
            // what the step's path strikes: the ground, or a structure near it
            let len = p1.distance(m.pos);
            if len > 0.0 {
                let dir = (p1 - m.pos) / len;
                let ground = bodies.raycast(m.pos, dir, len).map(|(_, p)| p);
                let reach = ground.map_or(len, |p| p.distance(m.pos));
                // (who fired it is not struck: a round does not hit its own gun)
                if let Some((p, id)) = structures(m.pos, dir, reach).filter(|(_, id)| *id != m.shooter || m.t > def.arm * 4.0) {
                    return Some(hit(p, Some(id)));
                }
                if let Some(p) = ground {
                    return Some(hit(p, None));
                }
            }
        }
        (m.pos, m.vel, m.t) = (p1, v1, m.t + STEP);
        self.list[k] = m;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detonation::ChargeDef;

    fn space() -> BodyRegistry {
        let def: crate::body::BodyDef = crate::defs::parse("b", r#"{ "name": "b", "center": [0, -1e9, 0], "radius": 1000, "gravity": 0.0, "reach": { "to": 500, "band": 200 }, "north": [0, 0, -1], "horizon_depth": 10 }"#).unwrap();
        BodyRegistry::new(vec![crate::body::Body::from_def("b", &def).unwrap()])
    }

    fn pool(seeker: Band) -> Flight {
        let missile = GuidedDef { name: "m".into(), seeker, accel: 300.0, burn: 4.0, turn: 250.0, nav: 4.0, life: 30.0, fov: 50.0, fuse: 8.0, arm: 0.3, mass: 80.0, charge: ChargeDef { tnt: 5.0, casing: 5.0 }, look: None, trail: None, trail_rate: 0.0 };
        let chaff = DecoyDef { name: "c".into(), rcs: 60.0, heat: 0.0, florece: 0.4, dura: 6.0, salida: 30.0, look: None, rate: 0.0, size: 1.0, life: 1.0 };
        Flight::new(vec![("m".into(), missile)], vec![("c".into(), chaff)])
    }

    #[test]
    fn it_meets_a_target_that_crosses_and_turns() {
        let bodies = space();
        let mut f = pool(Band::Calor);
        f.launch(0, DVec3::ZERO, DVec3::new(0.0, 0.0, 50.0), Some(7), 1);
        let mut hits = Vec::new();
        let mut t = 0.0;
        // 6 km off, crossing at 300 m/s and pulling 30 m/s² to one side
        let target = |t: f64| Aim { pos: DVec3::new(2000.0 + 300.0 * t, 15.0 * t * t, 6000.0), vel: DVec3::new(300.0, 30.0 * t, 0.0), rcs: 5.0, heat: 5.0e7 };
        while hits.is_empty() && t < 30.0 {
            f.update(STEP, &bodies, |_| Some(target(t)), |_, _, _| None, &mut hits);
            t += STEP;

        }
        assert_eq!(hits.len(), 1, "no llegó en 30 s");
        assert!(hits[0].at.distance(target(t).pos) < 30.0, "pasó a {} m", hits[0].at.distance(target(t).pos));
    }

    #[test]
    fn chaff_takes_a_radar_seeker_and_not_one_that_follows_heat() {
        for (seeker, fooled) in [(Band::Radar, true), (Band::Calor, false)] {
            let bodies = space();
            let mut f = pool(seeker);
            f.launch(0, DVec3::ZERO, DVec3::new(0.0, 0.0, 50.0), Some(7), 1);
            // the target lets a cloud go beside itself and turns away
            f.release(0, DVec3::new(0.0, 0.0, 4000.0), DVec3::new(-40.0, 0.0, 0.0));
            let mut hits = Vec::new();
            let target = |t: f64| Aim { pos: DVec3::new(120.0 * t, 0.0, 4000.0), vel: DVec3::new(120.0, 0.0, 0.0), rcs: 5.0, heat: 5.0e7 };
            let mut t = 0.0;
            let mut followed = None;
            while hits.is_empty() && t < 30.0 {
                f.update(STEP, &bodies, |id| (id == 7).then(|| target(t)), |_, _, _| None, &mut hits);
                followed = f.list.first().map_or(followed, |m| m.target);
                t += STEP;
            }
            let on_decoy = followed.is_some_and(|id| id & DECOY_ID != 0);
            assert_eq!(on_decoy, fooled, "{seeker:?}");
        }
    }
}
