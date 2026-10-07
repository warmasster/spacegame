//! Hits applied to the structures in the world: the `DamageModel` says who takes what; parts crack,
//! chip and shatter by their material's `Fracture`; joints give under the blast and under the
//! strain of the parts they hold; whatever no longer holds to an anchor (or to the heaviest piece)
//! flies off as a structure of its own. Reports what happened for the effects.
use super::{
    catalog::Catalog,
    damage::{DamageModel, Hit, JointHit, PartHit, StandardDamage},
    fracture::{self, Fracture, Rng},
    schedule::LINGER,
    set::Structures,
    state::{Joint, Part, Structure},
};
use glam::{Affine3A, DVec3, Vec3};
use std::sync::Arc;

pub struct Rules {
    pub damage: Box<dyn DamageModel>,
    /// One per material of the catalog.
    fractures: Vec<Box<dyn Fracture>>,
    /// Share of the damage that becomes motion of what breaks off.
    pub kick: f32,
    /// Loose pieces kept at once (the oldest go first).
    pub max_debris: usize,
}

impl Rules {
    /// The standard damage, and each material's fracture model.
    pub fn standard(cat: &Catalog) -> Result<Rules, String> {
        let fractures = cat.materials.iter().map(|(id, m)| fracture::model(m.fracture.as_deref()).map_err(|e| format!("material {id}: {e}"))).collect::<Result<_, _>>()?;
        Ok(Rules { damage: Box::new(StandardDamage::default()), fractures, kick: 0.04, max_debris: 600 })
    }

    /// The standard damage with one fracture model for every material (tests and tools).
    pub fn with_fracture(cat: &Catalog, model: &str) -> Result<Rules, String> {
        let fractures = cat.materials.iter().map(|_| fracture::model(Some(model))).collect::<Result<_, _>>()?;
        Ok(Rules { fractures, ..Rules::standard(cat)? })
    }

    fn fracture(&self, cat: &Catalog, kind: u16) -> &dyn Fracture {
        self.fractures[usize::from(cat.parts[usize::from(kind)].material)].as_ref()
    }
}

/// A piece come off a structure keeps its blueprint's look if it has at least one part in this
/// many of it.
const SHARED_LOOK_SHARE: usize = 4;

/// What happened, for the effects (world points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A part broke into pieces.
    Shattered { at: DVec3, size: f32, material: u16 },
    /// A chip came off a part.
    Chipped { at: DVec3, size: f32, material: u16 },
    /// A joint gave.
    Parted { at: DVec3 },
    /// A part that held something under pressure, or that burns, was destroyed: what its kind
    /// says it lets go of (`PartKindDef::burst`) goes off there — at `local` of structure `id`,
    /// with dice of its own drawn from the hit's (`seed`: the same in every game that did the hit).
    Burst { at: DVec3, kind: u16, id: u64, local: Vec3, seed: u64 },
}

/// A piece about to become a loose structure.
struct Piece {
    kind: u16,
    shape: super::convex::Convex,
    /// Part frame → world.
    local: Affine3A,
    pos: DVec3,
    rot: glam::Quat,
    vel: DVec3,
    energy: f32,
    push: Vec3,
}

impl Structures {
    /// A blast of `energy` J reaching `radius` m at world point `at`, on every structure in
    /// reach, set off here: on what is `remote` or `shared` it is told, not done (`told`).
    pub fn blast(&mut self, at: DVec3, energy: f32, radius: f32, rules: &Rules, events: &mut Vec<Event>, seed: u64) {
        let mut rng = Rng::new(seed);
        let n = self.list.len();
        for k in 0..n {
            let s = &self.list[k];
            if s.to_world(s.center).distance(at) > f64::from(s.radius + radius) {
                continue;
            }
            let hit = Hit { point: s.to_local(at), dir: Vec3::ZERO, energy, radius, area: 0.0 };
            // (a copy of what is simulated elsewhere, or what every game does in one order: told)
            if s.remote || s.shared {
                self.told.push((s.id, hit));
                continue;
            }
            self.apply(k, &hit, rules, events, &mut rng);
        }
        self.tidy(rules);
    }

    /// A hit on structure `id` given in its own frame (decided where it struck by someone else:
    /// who fired, in another player's game), applied here whatever `remote` says. False if there
    /// is no such structure.
    pub fn hit(&mut self, id: u64, hit: &Hit, rules: &Rules, events: &mut Vec<Event>, seed: u64) -> bool {
        let Some(k) = self.index_of(id) else {
            return false;
        };
        self.apply(k, hit, rules, events, &mut Rng::new(seed));
        self.tidy(rules);
        true
    }

    /// What `f` does to structure `id`, done with its articulations posed as `bones` (as they
    /// were where it was decided: a hit told by another game), its own pose back after. What a
    /// hit does hangs on where each part is; this way it is the same in every game that does it,
    /// however each one's copy happens to be posed (a dish turning, a leg folding). Without such
    /// a structure, or with bones that do not fit it, `f` as it is.
    pub fn posed<R>(&mut self, id: u64, bones: &[Affine3A], f: impl FnOnce(&mut Structures) -> R) -> R {
        let was = match self.index_of(id) {
            Some(k) if !bones.is_empty() && self.list[k].bones.len() == bones.len() + 1 && self.list[k].bones[1..] != *bones => {
                let s = &mut self.list[k];
                let was = s.bones[1..].to_vec();
                s.set_pose(bones);
                Some(was)
            }
            _ => None,
        };
        let r = f(self);
        if let (Some(was), Some(k)) = (was, self.index_of(id)) {
            self.list[k].set_pose(&was);
        }
        r
    }

    /// A projectile from `from` along `dir` (unit) with `energy` J and cross-section `area` m²:
    /// where it struck a structure, if it did within `max` m.
    #[allow(clippy::too_many_arguments)]
    pub fn shoot(&mut self, from: DVec3, dir: DVec3, energy: f32, area: f32, max: f64, rules: &Rules, events: &mut Vec<Event>, seed: u64) -> Option<DVec3> {
        let (k, _, at) = self.raycast(from, dir, max)?;
        let s = &self.list[k];
        let hit = Hit { point: s.to_local(at - dir * 0.05), dir: s.dir_to_local(dir), energy, radius: 0.0, area };
        if s.remote || s.shared {
            self.told.push((s.id, hit));
            return Some(at);
        }
        self.apply(k, &hit, rules, events, &mut Rng::new(seed));
        self.tidy(rules);
        Some(at)
    }

    fn apply(&mut self, k: usize, hit: &Hit, rules: &Rules, events: &mut Vec<Event>, rng: &mut Rng) {
        let lib = self.lib.clone();
        let cat = &lib.catalog;
        let (mut parts, mut joints): (Vec<PartHit>, Vec<JointHit>) = (Vec::new(), Vec::new());
        rules.damage.spread(&self.list[k], cat, hit, &mut parts, &mut joints);
        if parts.is_empty() && joints.is_empty() {
            return;
        }
        let mut pieces: Vec<Piece> = Vec::new();
        let now = self.now;
        let s = &mut self.list[k];
        s.resting = false;
        s.awake_until = s.awake_until.max(now + LINGER);
        for h in &parts {
            let i = h.part as usize;
            if !s.parts[i].alive {
                continue;
            }
            s.parts[i].hp -= h.energy * cat.parts[usize::from(s.parts[i].kind)].def.fragility;
            for j in s.joints.iter_mut().filter(|j| j.alive && (j.a == h.part || j.b == h.part)) {
                j.hp -= h.energy * cat.joints[usize::from(j.kind)].1.transfer;
            }
            let p = &s.parts[i];
            let (kind, material) = (p.kind, cat.parts[usize::from(p.kind)].material);
            let m = cat.material(kind);
            let inv = p.local.inverse();
            let (at, dir) = (inv.transform_point3(h.at), inv.transform_vector3(h.push).normalize_or(Vec3::Y));
            let world = s.to_world(p.center);
            let piece = |shape, energy| Piece { kind, shape, local: p.local, pos: s.pos, rot: s.rot, vel: s.vel, energy, push: h.push };
            if p.hp <= 0.0 {
                let total = p.shape.volume();
                for shard in rules.fracture(cat, kind).shatter(&p.shape, m, at, h.energy / p.max_hp, rng) {
                    let share = shard.volume() / total;
                    pieces.push(piece(shard, h.energy * share));
                }
                events.push(Event::Shattered { at: world, size: p.radius, material });
                // (a container with nothing left in it has nothing to let go of)
                if !p.fragment && cat.parts[usize::from(kind)].def.burst.is_some() && !s.spent(h.part) {
                    events.push(Event::Burst { at: world, kind, id: s.id, local: p.center, seed: rng.seed() });
                }
                s.parts[i].alive = false;
            } else if let Some((keep, chip)) = rules.fracture(cat, kind).chip(&p.shape, m, at, dir, h.energy / p.max_hp, rng) {
                let share = chip.volume() / p.shape.volume();
                pieces.push(piece(chip, h.energy * share));
                events.push(Event::Chipped { at: world, size: p.radius * share.cbrt(), material });
                s.parts[i].reshape(cat, Arc::new(keep));
                s.model = false;
                s.shapes += 1;
                // joints the part no longer reaches
                let p = &s.parts[i];
                for j in s.joints.iter_mut().filter(|j| j.alive && (j.a == h.part || j.b == h.part)) {
                    if p.shape.distance(inv.transform_point3(j.at)) > 0.15 {
                        j.hp = 0.0;
                    }
                }
            }
            let p = &mut s.parts[i];
            p.working = p.alive && !p.fragment && p.hp > p.max_hp * cat.parts[usize::from(p.kind)].def.function;
        }
        for h in &joints {
            s.joints[h.joint as usize].hp -= h.energy;
        }
        for j in s.joints.iter_mut().filter(|j| j.alive) {
            if j.hp <= 0.0 || !s.parts[j.a as usize].alive || !s.parts[j.b as usize].alive {
                j.alive = false;
                if j.hp <= 0.0 {
                    events.push(Event::Parted { at: s.pos + (s.rot * j.at).as_dvec3() });
                }
            }
        }
        s.refresh();
        for p in pieces {
            self.loose(k, cat, p, rules.kick, rng);
        }
        self.detach(k, &parts, rules.kick);
    }

    /// Part `part` of structure `id` blown out whole (torn by the air behind it, burst by what it
    /// held): it breaks into pieces flung along `push` (structure frame) with `energy` J, and shows
    /// its material's break. The part is gone from the structure, whether it was already or not.
    #[allow(clippy::too_many_arguments)]
    pub fn blow_out(&mut self, id: u64, part: u32, push: Vec3, energy: f32, rules: &Rules, events: &mut Vec<Event>, seed: u64) {
        let Some(k) = self.list.iter().position(|s| s.id == id) else {
            return;
        };
        let lib = self.lib.clone();
        let cat = &lib.catalog;
        let mut rng = Rng::new(seed);
        let s = &mut self.list[k];
        let i = part as usize;
        if i >= s.parts.len() || s.parts[i].fragment {
            return;
        }
        let p = s.parts[i].clone();
        let (kind, material) = (p.kind, cat.parts[usize::from(p.kind)].material);
        let m = cat.material(kind);
        let push = push.normalize_or(Vec3::Y);
        let at = p.local.inverse().transform_point3(p.center);
        let total = p.shape.volume().max(1e-9);
        let mut pieces = Vec::new();
        for shard in rules.fracture(cat, kind).shatter(&p.shape, m, at, (energy / p.max_hp.max(1.0)).max(1.0), &mut rng) {
            let share = shard.volume() / total;
            pieces.push(Piece { kind, shape: shard, local: p.local, pos: s.pos, rot: s.rot, vel: s.vel, energy: energy * share, push });
        }
        events.push(Event::Shattered { at: s.to_world(p.center), size: p.radius, material });
        if !p.fragment && p.alive && cat.parts[usize::from(kind)].def.burst.is_some() && !s.spent(part) {
            events.push(Event::Burst { at: s.to_world(p.center), kind, id: s.id, local: p.center, seed: rng.seed() });
        }
        s.parts[i].alive = false;
        s.parts[i].working = false;
        for j in s.joints.iter_mut().filter(|j| j.a == part || j.b == part) {
            j.alive = false;
        }
        s.resting = false;
        // what it alone held comes off (`separate`)
        s.parted = true;
        s.refresh();
        for pc in pieces {
            self.loose(k, cat, pc, rules.kick, &mut rng);
        }
        self.tidy(rules);
    }

    /// Structures some of whose joints were let go of (`Structure::parted`: a clamp opened, a
    /// lashing cut): what no longer holds to the rest comes off as a structure of its own, moving
    /// as it was. How many came off.
    pub fn separate(&mut self) -> usize {
        let before = self.list.len();
        for k in 0..before {
            // (a copy of what is simulated elsewhere too: what was done to it was done the same in
            // every game, and what no longer holds comes off the same)
            if std::mem::take(&mut self.list[k].parted) {
                self.detach(k, &[], 0.0);
                self.list[k].resting = false;
            }
        }
        self.list.len() - before
    }

    /// The name a piece coming off structure `k` now gets, and whether it is shared (as `k` is):
    /// from `k`'s and from how many came off it before (`Structure::lineage`). None for what has
    /// no name.
    fn child_of(&mut self, k: usize) -> (u64, bool) {
        let s = &mut self.list[k];
        if s.lineage == 0 {
            return (0, false);
        }
        s.born += 1;
        // (splitmix64 of the two: a name of 61 bits, never 0)
        let mut z = s.lineage ^ u64::from(s.born).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        ((z & ((1 << 61) - 1)).max(1), s.shared)
    }

    /// A broken piece of structure `k` as a loose structure, thrown by its share of the damage.
    fn loose(&mut self, k: usize, cat: &Catalog, p: Piece, kick: f32, rng: &mut Rng) {
        let (lineage, shared) = self.child_of(k);
        let (_, c) = p.shape.mass_props();
        let shape = p.shape.transformed(Affine3A::from_translation(-c));
        let (_, turn, _) = p.local.to_scale_rotation_translation();
        let pos = p.pos + (p.rot * p.local.transform_point3(c)).as_dvec3();
        let mut part = Part::new(cat, p.kind, Affine3A::IDENTITY, Some(shape), true);
        part.working = false;
        let speed = (2.0 * kick * p.energy / part.mass.max(0.01)).sqrt().min(45.0);
        let away = (p.rot * p.push + rng.dir() * 0.6).normalize_or(Vec3::Y);
        let id = self.next_id();
        let name: Arc<str> = cat.parts[usize::from(p.kind)].def.name.as_str().into();
        let mut s = Structure::assemble(id, name, pos, p.rot * turn, false, vec![part], Vec::new());
        s.vel = p.vel + (away * speed).as_dvec3();
        s.spin = rng.dir() * speed / s.radius.max(0.1) * 0.3;
        (s.clock, s.awake_until) = (self.now, self.now + LINGER);
        (s.lineage, s.shared) = (lineage, shared);
        self.list.push(s);
    }

    /// Groups of structure `k` that no longer hold to an anchor (or to the heaviest group) become
    /// structures of their own, thrown by the damage their parts took.
    fn detach(&mut self, k: usize, hits: &[PartHit], kick: f32) {
        let mut groups = std::mem::take(&mut self.groups);
        let s = &self.list[k];
        groups.build(s.parts.len(), |i| s.parts[i].alive && !s.parts[i].ghost, s.joints.iter().filter(|j| j.alive).map(|j| (j.a, j.b)));
        if groups.count == 0 {
            self.groups = groups;
            return;
        }
        let anchored_groups: Vec<bool> = (0..groups.count).map(|g| s.anchored && s.parts.iter().enumerate().any(|(i, p)| p.alive && p.anchor && groups.label[i] == g as u32)).collect();
        let held = anchored_groups.iter().any(|&a| a);
        let mass = |g: u32| s.parts.iter().enumerate().filter(|(i, p)| p.alive && groups.label[*i] == g).map(|(_, p)| p.mass).sum::<f32>();
        let heaviest = (0..groups.count as u32).max_by(|&a, &b| mass(a).total_cmp(&mass(b))).unwrap_or(0);
        let stays = |g: u32| {
            if held { anchored_groups[g as usize] } else { g == heaviest }
        };
        let mut out = Vec::new();
        for g in (0..groups.count as u32).filter(|&g| !stays(g)) {
            let s = &self.list[k];
            let idx: Vec<usize> = (0..s.parts.len()).filter(|&i| groups.label[i] == g).collect();
            let remap = |i: u32| idx.iter().position(|&x| x == i as usize).map(|x| x as u32);
            // a piece leaves its joints behind: it holds still as it is posed now
            let parts: Vec<Part> = idx
                .iter()
                .map(|&i| {
                    let mut p = s.parts[i].clone();
                    p.rest = p.local;
                    p.bone = 0;
                    p
                })
                .collect();
            let joints: Vec<Joint> = s.joints.iter().filter(|j| j.alive).filter_map(|j| Some(Joint { a: remap(j.a)?, b: remap(j.b)?, ..*j })).collect();
            let energy: f32 = hits.iter().filter(|h| groups.label[h.part as usize] == g).map(|h| h.energy).sum();
            let push: Vec3 = hits.iter().filter(|h| groups.label[h.part as usize] == g).map(|h| h.push * h.energy).sum();
            out.push((parts, joints, energy, push, idx));
        }
        for (parts, joints, energy, push, idx) in out {
            let id = self.next_id();
            let (lineage, shared) = self.child_of(k);
            let s = &mut self.list[k];
            for &i in &idx {
                s.parts[i].alive = false;
                // what was only carried is not missing from what carried it
                s.parts[i].left = self.lib.catalog.parts[usize::from(s.parts[i].kind)].def.carried;
            }
            // what its parts hold goes with them (`contents`), and weighs with it from now on
            let stored = super::contents::moved(&s.stored, &idx);
            if !stored.is_empty() {
                s.stored.retain(|st| !idx.contains(&(st.part as usize)));
            }
            let mut n = Structure::build(id, s.name.clone(), s.pos, s.rot, false, parts, joints, stored);
            // a big piece keeps the blueprint's look, posed as it was when it came off (nothing is
            // meshed: it draws the same vertices with the rest of the parts gone); a small one
            // gets a look of its own, which costs less than drawing the whole blueprint's for it
            n.model = s.model && n.parts.len() * SHARED_LOOK_SHARE >= s.parts.len();
            if n.model {
                n.bones = s.bones.clone();
            }
            let speed = (2.0 * kick * energy / n.mass.max(1.0)).sqrt().min(30.0);
            // it moves as that place of the structure did (its turn included), plus the blow
            let arm = (n.to_world(n.com) - s.to_world(s.com)).as_vec3();
            n.vel = s.vel + s.spin.cross(arm).as_dvec3() + (s.rot * push.normalize_or(Vec3::Y) * speed).as_dvec3();
            n.spin = s.spin;
            (n.clock, n.awake_until) = (s.clock, s.awake_until);
            (n.lineage, n.shared) = (lineage, shared);
            self.list.push(n);
        }
        let s = &mut self.list[k];
        if !held {
            s.anchored = false;
        }
        for j in s.joints.iter_mut() {
            j.alive = j.alive && s.parts[j.a as usize].alive && s.parts[j.b as usize].alive;
        }
        s.refresh();
        self.groups = groups;
    }

    /// Drop what has nothing left and the oldest loose pieces over the limit.
    fn tidy(&mut self, rules: &Rules) {
        self.list.retain(Structure::alive);
        let debris = |s: &Structure| s.parts.len() == 1 && s.parts[0].fragment;
        let n = self.list.iter().filter(|s| debris(s)).count();
        if n > rules.max_debris {
            let mut ids: Vec<u64> = self.list.iter().filter(|s| debris(s)).map(|s| s.id).collect();
            ids.sort_unstable();
            let cut = ids[n - rules.max_debris - 1];
            self.list.retain(|s| !debris(s) || s.id > cut);
        }
    }
}
