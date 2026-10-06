//! How much simulation each structure gets. A `LodPolicy` gives every structure a level from where
//! the watchers are (players, cameras); anything can wake one (a hit, an incoming projectile) for a
//! while. Active structures step every frame, coarse ones a few times a second with the time
//! gathered, dormant ones not at all: when they wake, the time they slept is caught up at once
//! (ballistically for anything in flight), so nothing jumps and no state is lost.
//!
//! This is the world's only clock. What lives among the structures without being one of them
//! (`Among`: someone on foot, anything else that walks, floats or is carried among them) has no
//! clock of its own: the world steps it here, slice by slice, right after the structures.
use super::{networks::Env, set::Structures, state::Structure};
use crate::body::BodyRegistry;
use glam::{DVec3, Quat};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SimLevel {
    #[default]
    Active,
    Coarse,
    Dormant,
}

pub trait LodPolicy: Send + Sync {
    /// The level `s` gets with the watchers at `watchers` (world points).
    fn level(&self, s: &Structure, watchers: &[DVec3]) -> SimLevel;
}

/// By the distance to the nearest watcher.
pub struct DistancePolicy {
    /// Active within (m).
    pub active: f64,
    /// Coarse within (m); dormant beyond.
    pub coarse: f64,
}

impl Default for DistancePolicy {
    fn default() -> Self {
        DistancePolicy { active: 1500.0, coarse: 15_000.0 }
    }
}

impl LodPolicy for DistancePolicy {
    fn level(&self, s: &Structure, watchers: &[DVec3]) -> SimLevel {
        let c = s.to_world(s.center);
        let d = watchers.iter().map(|w| w.distance(c)).fold(f64::MAX, f64::min) - f64::from(s.radius);
        if d < self.active {
            SimLevel::Active
        } else if d < self.coarse {
            SimLevel::Coarse
        } else {
            SimLevel::Dormant
        }
    }
}

/// Seconds between steps of a coarse structure.
pub const COARSE_STEP: f64 = 0.25;
/// Seconds between network ticks of an active structure, and of a coarse one.
pub const NET_ACTIVE: f64 = 0.05;
pub const NET_COARSE: f64 = 1.0;
/// Seconds a woken structure stays active.
pub const LINGER: f64 = 8.0;

/// What lives among the structures without being one of them. The world steps it with them,
/// slice by slice and right after them (`Structures::simulate_with`), so it and they are always
/// of the same instant: nothing of it lags a structure, however fast that goes and however long
/// the frame is. Nothing that moves among structures is stepped any other way.
///
/// And what it is among is stepped as it is: the structures round it get every slice it gets,
/// wherever whoever watches the world is looking from (`at`). Else a structure beside it, seen
/// from afar, would wait for its next coarse step while it went on: a frame apart again.
pub trait Among {
    /// `dt` s on (more than none), among the structures as they are now.
    fn slice(&mut self, set: &Structures, bodies: &BodyRegistry, dt: f64);

    /// Where it is (world): what is round it is simulated as round any watcher.
    fn at(&self) -> DVec3;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimStats {
    pub active: u32,
    pub coarse: u32,
    pub dormant: u32,
    /// Structure steps run this call.
    pub steps: u32,
}

/// Where a free structure is after `t` s of flight from its state now, pulled all the while as
/// it is pulled where it is (`pull`, world, m/s²): the catch-up of a sleeper.
pub fn coast(s: &mut Structure, pull: DVec3, t: f64) {
    if s.anchored || s.resting || s.held.is_some() || t <= 0.0 {
        return;
    }
    let com = s.to_world(s.com);
    let com_new = com + s.vel * t + pull * (0.5 * t * t);
    s.vel += pull * t;
    s.rot = (Quat::from_scaled_axis(s.spin * t as f32) * s.rot).normalize();
    s.pos = com_new - (s.rot * s.com).as_dvec3();
}

impl Structures {
    /// Keep `id` fully simulated until `until` (sim time).
    pub fn wake(&mut self, id: u64, until: f64) {
        if let Some(s) = self.list.iter_mut().find(|s| s.id == id) {
            s.awake_until = s.awake_until.max(until);
        }
    }

    /// Advance the world by `dt` to time `now`, each structure at its level: the active ones
    /// together (they collide), the coarse ones each with the time it gathered.
    pub fn simulate(&mut self, now: f64, dt: f64, bodies: &BodyRegistry, policy: &dyn LodPolicy, watchers: &[DVec3]) -> SimStats {
        self.simulate_with(now, dt, bodies, policy, watchers, &mut [])
    }

    /// The same, and `among` with them: each of them stepped after every slice the active
    /// structures take (`Among`).
    pub fn simulate_with(&mut self, now: f64, dt: f64, bodies: &BodyRegistry, policy: &dyn LodPolicy, watchers: &[DVec3], among: &mut [&mut dyn Among]) -> SimStats {
        self.now = now;
        let mut st = SimStats::default();
        let (mut together, mut alone) = std::mem::take(&mut self.due);
        together.clear();
        alone.clear();
        // (whoever lives among the structures watches the ones round it)
        let mut watch = std::mem::take(&mut self.watch);
        watch.clear();
        watch.extend_from_slice(watchers);
        watch.extend(among.iter().map(|a| a.at()));
        let watchers = &watch[..];
        for (k, s) in self.list.iter_mut().enumerate() {
            if s.clock.is_nan() {
                s.clock = now - dt;
            }
            if s.net_clock.is_nan() {
                s.net_clock = now - dt;
            }
            let mut level = policy.level(s, watchers);
            if s.awake_until > now {
                level = SimLevel::Active;
            }
            // whatever it slept through, caught up before it moves on
            if level != SimLevel::Dormant && s.sim == SimLevel::Dormant {
                let com = s.to_world(s.com);
                coast(s, bodies.field(com).pull, now - dt - s.clock);
                s.clock = now - dt;
            }
            s.sim = level;
            match level {
                SimLevel::Active => {
                    st.active += 1;
                    // behind (it was coarse): alone up to the frame first
                    let lag = now - dt - s.clock;
                    if lag > 1e-6 {
                        alone.push((k, lag as f32));
                    }
                    together.push(k);
                    s.clock = now;
                    st.steps += 1;
                }
                SimLevel::Coarse => {
                    st.coarse += 1;
                    if now - s.clock >= COARSE_STEP {
                        alone.push((k, (now - s.clock) as f32));
                        s.clock = now;
                        st.steps += 1;
                    }
                }
                SimLevel::Dormant => st.dormant += 1,
            }
        }
        for &(k, t) in &alone {
            self.physics(&[k], t, bodies);
        }
        self.physics_among(&together, dt as f32, bodies, among);
        self.due = (together, alone);
        self.watch = watch;
        // what is held goes where its holder went
        self.follow();
        // networks: often while active, now and then while coarse, and the whole nap of a
        // sleeper at once when it wakes
        let (cat, machines, solver, sun) = (&self.lib.catalog, &self.machines, &mut self.solver, self.sun);
        for s in &mut self.list {
            let every = match s.sim {
                SimLevel::Active => NET_ACTIVE,
                SimLevel::Coarse => NET_COARSE,
                SimLevel::Dormant => f64::INFINITY,
            };
            if now - s.net_clock >= every {
                let env = Env { sun: s.rot.inverse() * sun.as_vec3() };
                if solver.tick(s, cat, machines, &env, (now - s.net_clock) as f32) {
                    s.version += 1;
                }
                s.net_clock = now;
            }
        }
        st
    }
}
