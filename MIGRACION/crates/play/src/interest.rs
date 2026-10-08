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
//!   What has come to rest is told so once, surely and exactly (`rests`), and not again until it
//!   moves.
//!
//! The same rule for every kind of thing that will be (ships, pieces, crates, asteroids): it only
//! asks where a thing is, how big it is and how it moves.
//!
//! With many things, they are put once a step in an `Index` that every player's interest asks
//! (`Interest::update_in`): what is small and slow in cells, where a player looks only at the 27
//! round it (what is farther cannot be near enough nor come so within the horizon); what is big or
//! fast in a list everyone looks at; and a player who goes fast looks at everything. It knows the
//! same as looking at everything (`tests`).
use glam::DVec3;
use lunar_core::structure::set::Structures;

/// How far what is known only from afar reaches (m): ships past what is known of them in full,
/// and players past what a snapshot carries (`net::Far`): a radar's reach and some.
pub const TRACK_REACH: f64 = 150_000.0;
/// Steps between two words of what is known from afar (5 a second).
pub const TRACK_EVERY: u64 = 12;

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
    /// Of the rounds let fly and ended farther than `near` from a player (and that do not fly
    /// past within it), one in this many is told to them: a far cannonade, as its tracers.
    pub tracers: u32,
}

impl Default for Rule {
    fn default() -> Rule {
        Rule { near: 2000.0, per_metre: 500.0, most: 400_000.0, keep: 1.3, linger: 1.5, horizon: 8.0, rest_told: 3, tracers: 4 }
    }
}

impl Rule {
    /// How near a structure of radius `r` is known from (m).
    pub fn reach(&self, r: f64) -> f64 {
        (self.near + self.per_metre * r.max(0.0)).min(self.most)
    }
}

/// With fewer structures than this, every player looks at all of them (cheaper than the cells).
pub const INDEX_FROM: usize = 256;

/// Where every structure is, once a step, for every player's interest to ask.
#[derive(Clone, Debug, Default)]
pub struct Index {
    /// Side of a cell (m), and the eye's own speed up to which a player may look in cells only.
    cell: f64,
    slow: f64,
    /// What is small and slow, by cell (its cell's key, its place in the structures).
    small: Vec<(u64, u32)>,
    /// What is big or fast: everyone looks at it.
    big: Vec<u32>,
    /// Of each structure (by its place): its centre (world) and how near it is known from.
    at: Vec<(DVec3, f64)>,
    /// In use (enough structures for it to pay).
    built: bool,
}

/// A cell's key: a number for each cell, the same wherever it is asked for (two cells may share
/// one: what is in the other is looked at too, and found not near).
fn cell_key(p: DVec3, cell: f64) -> u64 {
    let c = (p / cell).floor();
    key3(c.x as i64, c.y as i64, c.z as i64)
}

fn key3(x: i64, y: i64, z: i64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h.wrapping_mul(0xBF58_476D_1CE4_E5B9)
}

impl Index {
    /// Every structure of `set` put where it is, by `rule`.
    pub fn build(&mut self, rule: &Rule, set: &Structures) {
        // (cells twice the reach of the smallest; of each cell's side, what the eye itself may
        // go over the horizon is kept aside)
        self.cell = rule.near * 2.0;
        let margin = rule.near * 0.5;
        self.slow = margin / rule.horizon.max(1e-9);
        (self.small.clear(), self.big.clear(), self.at.clear());
        self.built = set.list.len() >= INDEX_FROM;
        for (i, s) in set.list.iter().enumerate() {
            let at = s.to_world(s.center);
            let reach = rule.reach(f64::from(s.radius));
            self.at.push((at, reach));
            if !self.built {
                continue;
            }
            if reach + s.vel.length() * rule.horizon <= self.cell - margin {
                self.small.push((cell_key(at, self.cell), i as u32));
            } else {
                self.big.push(i as u32);
            }
        }
        self.small.sort_unstable();
    }

    /// The places of the structures `eye` going at `vel` may come to know: all of them, or what
    /// is big or fast and what is in the cells round it.
    fn each(&self, eye: DVec3, vel: DVec3, n: usize, keys: &mut Vec<u64>, mut f: impl FnMut(usize)) {
        if !self.built || vel.length() > self.slow {
            (0..n).for_each(f);
            return;
        }
        self.big.iter().for_each(|&i| f(i as usize));
        let c = (eye / self.cell).floor();
        let (x, y, z) = (c.x as i64, c.y as i64, c.z as i64);
        keys.clear();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    keys.push(key3(x + dx, y + dy, z + dz));
                }
            }
        }
        keys.sort_unstable();
        keys.dedup();
        for &k in keys.iter() {
            let from = self.small.partition_point(|e| e.0 < k);
            for e in self.small[from..].iter().take_while(|e| e.0 == k) {
                f(e.1 as usize);
            }
        }
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
    /// (reused) what a snapshot takes, by priority; the cells round the eye.
    order: Vec<(f32, u32)>,
    keys: Vec<u64>,
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
                // (what is told whole comes as it is, at rest or not: `Event::Made`)
                let resting = set.get(id).is_some_and(|s| s.resting);
                self.known.insert(i, Known { id, out: 0.0, prio: f32::MAX, resting, rest_told: 0 });
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

    /// What known has come to rest since it was last asked, into `out` (to be told so, exactly);
    /// and what was at rest and moves again, known to move.
    pub fn rests(&mut self, set: &Structures, out: &mut Vec<u64>) {
        for kn in &mut self.known {
            let Some(s) = set.get(kn.id).filter(|s| !s.anchored) else { continue };
            if s.resting && !kn.resting {
                out.push(kn.id);
            }
            kn.resting = s.resting;
        }
    }

    /// The known things to go in the next snapshot, highest priority first, into `out` (their
    /// places in `known`): what moves (what rests was told so, `rests`; what is anchored never
    /// moves: its place came with it).
    pub fn take(&mut self, set: &Structures, _rule: &Rule, out: &mut Vec<usize>) {
        self.order.clear();
        for (i, kn) in self.known.iter_mut().enumerate() {
            let Some(s) = set.get(kn.id).filter(|s| !s.anchored) else { continue };
            if s.resting || kn.resting {
                continue;
            }
            self.order.push((kn.prio, i as u32));
        }
        self.order.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        out.clear();
        out.extend(self.order.iter().map(|o| o.1 as usize));
    }

    /// The same as `update`, asking `index` (made of `set` this step, `Index::build`) instead of
    /// looking at every structure: what is known and what comes and goes is the same.
    #[allow(clippy::too_many_arguments)]
    pub fn update_in(&mut self, rule: &Rule, set: &Structures, index: &Index, eye: DVec3, vel: DVec3, pinned: &[u64], dt: f32, came: &mut Vec<u64>, went: &mut Vec<u64>) {
        let mut k = 0;
        while k < self.known.len() {
            if set.index_of(self.known[k].id).is_none() {
                went.push(self.known[k].id);
                self.known.remove(k);
            } else {
                k += 1;
            }
        }
        let near = |i: usize| {
            let s = &set.list[i];
            let (at, reach) = index.at[i];
            let d = at.distance(eye);
            let rel = s.vel - vel;
            let to = at - eye;
            let t = (-to.dot(rel) / rel.length_squared().max(1e-9)).clamp(0.0, rule.horizon);
            (pinned.contains(&s.id) || d < reach || (to + rel * t).length() < reach, d < reach * rule.keep)
        };
        // (what is known: still near, or for how long not)
        for kn in &mut self.known {
            let Some(i) = set.index_of(kn.id) else { continue };
            let (near, kept) = near(i);
            if near || kept {
                kn.out = 0.0;
            } else {
                kn.out += dt;
                if kn.out > rule.linger {
                    went.push(kn.id);
                }
            }
        }
        // (what comes: of what may)
        let from = came.len();
        let known = &self.known;
        index.each(eye, vel, set.list.len(), &mut self.keys, |i| {
            let id = set.list[i].id;
            if known.binary_search_by_key(&id, |k| k.id).is_err() && near(i).0 {
                came.push(id);
            }
        });
        for &id in pinned {
            if set.index_of(id).is_some() && self.known.binary_search_by_key(&id, |k| k.id).is_err() && !came[from..].contains(&id) {
                came.push(id);
            }
        }
        if !went.is_empty() {
            self.known.retain(|k| !went.contains(&k.id));
        }
        for &id in &came[from..] {
            if let Err(i) = self.known.binary_search_by_key(&id, |k| k.id) {
                let resting = set.get(id).is_some_and(|s| s.resting);
                self.known.insert(i, Known { id, out: 0.0, prio: f32::MAX, resting, rest_told: 0 });
            }
        }
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
