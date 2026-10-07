//! Who knows what (`docs/PLAN_AUTORITATIVO.md` §3.6): what each player's game is told of, out of
//! everything the server has, and in what order the room of each snapshot goes.
//!
//! - **near enough** for its size: a structure is known within `Rule::near` plus `per_metre` for
//!   each metre of its radius (a crate a couple of kilometres off, a station from much farther),
//!   and forgotten only once it has been past that by `keep` for `linger` s (no flicker at the
//!   edge);
//! - **coming**: what will be that near within `horizon` s at the speed it closes (a missile, a
//!   ship at orbital speed, an asteroid falling in) is known before it arrives;
//! - **pinned**: what the player rides, sits in or floats by is known whatever its size;
//! - **in turn**: each step every known thing that moves gains priority by how much it matters
//!   (big, near, fast); a snapshot takes the highest first and those start again from nothing.
//!   What has come to rest goes a few times more, then not until it moves.
//!
//! The same rule for every kind of thing that will be (ships, pieces, crates, asteroids): it only
//! asks where a thing is, how big it is and how it moves.
use glam::DVec3;
use lunar_core::structure::set::Structures;

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Known within this (m), plus this for each metre of its radius, and never farther than `most`.
    pub near: f64,
    pub per_metre: f64,
    pub most: f64,
    /// Forgotten past `keep` times that, after `linger` s out there.
    pub keep: f64,
    pub linger: f32,
    /// Known this long (s) before it comes near enough, at the speed it closes.
    pub horizon: f64,
    /// Told again this many times once it is at rest (one could be lost).
    pub rest_told: u8,
}

impl Default for Rule {
    fn default() -> Rule {
        Rule { near: 2000.0, per_metre: 500.0, most: 400_000.0, keep: 1.3, linger: 1.5, horizon: 8.0, rest_told: 3 }
    }
}

impl Rule {
    /// How near a structure of radius `r` is known from (m).
    pub fn reach(&self, r: f64) -> f64 {
        (self.near + self.per_metre * r.max(0.0)).min(self.most)
    }
}

/// A structure known by one player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Known {
    pub id: u64,
    /// How long it has been past the rule's reach (s).
    pub out: f32,
    /// How much it is owed a place in a snapshot.
    pub prio: f32,
    /// At rest when last told, and how many times it has been told so since.
    pub resting: bool,
    pub rest_told: u8,
}

/// What one player knows, sorted by id.
#[derive(Clone, Debug, Default)]
pub struct Interest {
    pub known: Vec<Known>,
    /// (reused) what a snapshot takes, by priority.
    order: Vec<(f32, u32)>,
}

impl Interest {
    pub fn knows(&self, id: u64) -> bool {
        self.known.binary_search_by_key(&id, |k| k.id).is_ok()
    }

    /// Forgets everything (a player put elsewhere at a stroke: what is round it now comes anew).
    pub fn clear(&mut self) {
        self.known.clear();
    }

    /// What is known from `eye` going at `vel` (world), `pinned` known whatever its size and
    /// distance, after `dt` s: what comes to be known into `came`, what is forgotten (or is no
    /// more) into `went`. `skip(id)`: known without being told (a player's game has it as the
    /// server does: what the scenario sets, never touched).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, rule: &Rule, set: &Structures, eye: DVec3, vel: DVec3, pinned: &[u64], dt: f32, came: &mut Vec<u64>, went: &mut Vec<u64>) {
        // (what is no more is forgotten)
        let mut k = 0;
        while k < self.known.len() {
            if set.index_of(self.known[k].id).is_none() {
                went.push(self.known[k].id);
                self.known.remove(k);
            } else {
                k += 1;
            }
        }
        let from = came.len();
        for s in &set.list {
            let at = s.to_world(s.center);
            let reach = rule.reach(f64::from(s.radius));
            let d = at.distance(eye);
            let pin = pinned.contains(&s.id);
            // (what closes: where it is nearest within the horizon)
            let coming = || {
                let rel = s.vel - vel;
                let to = at - eye;
                let t = (-to.dot(rel) / rel.length_squared().max(1e-9)).clamp(0.0, rule.horizon);
                (to + rel * t).length() < reach
            };
            let near = pin || d < reach || coming();
            match self.known.binary_search_by_key(&s.id, |k| k.id) {
                Ok(i) => {
                    let kn = &mut self.known[i];
                    if near || d < reach * rule.keep {
                        kn.out = 0.0;
                    } else {
                        kn.out += dt;
                        if kn.out > rule.linger {
                            went.push(s.id);
                        }
                    }
                }
                Err(_) if near => came.push(s.id),
                Err(_) => {}
            }
        }
        // (forgotten, and come: kept sorted)
        if !went.is_empty() {
            self.known.retain(|k| !went.contains(&k.id));
        }
        for &id in &came[from..] {
            if let Err(i) = self.known.binary_search_by_key(&id, |k| k.id) {
                self.known.insert(i, Known { id, out: 0.0, prio: f32::MAX, resting: false, rest_told: 0 });
            }
        }
    }

    /// Each known thing that moves owed a little more of a snapshot (seen from `eye`): the bigger,
    /// the nearer and the faster, the more; what rides us or we ride, the most.
    pub fn owe(&mut self, set: &Structures, eye: DVec3, pinned: &[u64], dt: f32) {
        for kn in &mut self.known {
            let Some(s) = set.get(kn.id) else { continue };
            let d = s.to_world(s.center).distance(eye);
            let speed = s.vel.length() as f32;
            let w = if pinned.contains(&kn.id) { 1000.0 } else { (1.0 + s.radius) / (1.0 + d as f32 / 50.0) * (1.0 + speed / 30.0) };
            kn.prio = (kn.prio + w * dt).min(f32::MAX / 2.0);
        }
    }

    /// The known things to go in the next snapshot, highest priority first, into `out` (their
    /// places in `known`): what moves, and what has just come to rest a few times more.
    pub fn take(&mut self, set: &Structures, rule: &Rule, out: &mut Vec<usize>) {
        self.order.clear();
        for (i, kn) in self.known.iter_mut().enumerate() {
            // (what is anchored never moves: its place came with it)
            let Some(s) = set.get(kn.id).filter(|s| !s.anchored) else { continue };
            let still = s.resting;
            if still {
                if !kn.resting {
                    (kn.resting, kn.rest_told) = (true, 0);
                }
                if kn.rest_told >= rule.rest_told {
                    continue;
                }
            } else {
                kn.resting = false;
            }
            self.order.push((kn.prio, i as u32));
        }
        self.order.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        out.clear();
        out.extend(self.order.iter().map(|o| o.1 as usize));
    }

    /// Known thing `i` went in a snapshot.
    pub fn told(&mut self, i: usize) {
        let kn = &mut self.known[i];
        kn.prio = 0.0;
        if kn.resting {
            kn.rest_told = kn.rest_told.saturating_add(1);
        }
    }
}
