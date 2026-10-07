//! The structures of the scenario in play (core::structure): set round the site at the start,
//! hurt by blasts and shots, their loose pieces falling and settling, what breaks shown by its
//! material's effect, all handed to the renderer every frame.
//!
//! With other players, what is done to a structure they also have (`Structure::shared`) is
//! decided once and done everywhere in one order: a hit or a torn plate decided here goes to
//! `strikes` (whoever tells the others takes it from there) and is done when it comes back
//! (`strike_done`), here as everywhere. What follows from it that hangs on what each game
//! simulates of its own (a part that bursts or not by what it held) is said by one game — the
//! owner of the struck ship, else whoever decided the strike — and told as any blast is; the
//! others only see it go off.
use glam::{DVec3, Vec3};
use lunar_core::{
    body::BodyRegistry,
    effects::{BlastDef, Effects, ShotDef},
    scenario::ScenarioDef,
    scene::Site,
    structure::{
        Library,
        breakup::{Event, Rules},
        schedule::{Among, DistancePolicy, LodPolicy, SimStats},
        set::Structures,
    },
};
use lunar_render::Renderer;
use std::sync::Arc;

/// Break effects shown per frame at most (a big blast breaks dozens of parts at once).
const EFFECTS_PER_FRAME: usize = 24;
/// Radius (m) of the part a material's break effect is written for; bigger parts scale it up.
const BREAK_EFFECT_SIZE: f32 = 0.6;

pub struct Builds {
    pub set: Structures,
    rules: Rules,
    policy: Box<dyn LodPolicy>,
    events: Vec<Event>,
    /// What burst last frame (where, the part kind that says what it lets go of): it goes off
    /// this one, so a row of drums goes up one after another.
    /// Parts that burst since the last frame: where, of what kind, on what structure and where
    /// in it, their dice, and whether the strike that broke them was decided here.
    bursts: Vec<(DVec3, u16, u64, Vec3, u64, bool)>,
    hits: u64,
    now: f64,
    /// How many ran at each level last frame.
    pub stats: SimStats,
    /// What was decided here to be done to shared structures, not done yet: for whoever tells
    /// the others (`take_strikes`).
    strikes: Vec<Strike>,
    /// The structures the scenario sets round the site have ids below this: the same in every
    /// game that starts with it (how the network names them).
    pub scenario_end: u64,
}

/// Something decided in one game to be done to a structure every game has, in its own frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Strike {
    /// A hit (a round, a blast's share).
    Hit { id: u64, hit: lunar_core::structure::damage::Hit },
    /// Part `part` blown out along `push` with `energy` J (the air behind a plate).
    Blow { id: u64, part: u32, push: glam::Vec3, energy: f32 },
}

impl Builds {
    /// `effects`: the explosion ids that exist (the materials' break effects must be among them).
    pub fn new(lib: Arc<Library>, scenario: &ScenarioDef, site: &Site, bodies: &BodyRegistry, effects: &[&str]) -> Result<Builds, String> {
        for (id, m) in &lib.catalog.materials {
            if let Some(e) = &m.breaks
                && !effects.contains(&e.as_str())
            {
                return Err(format!("material {id}: unknown break effect '{e}'"));
            }
        }
        let rules = Rules::standard(&lib.catalog)?;
        let mut set = Structures::new(lib);
        for p in &scenario.structures {
            set.place(&p.build, bodies, site.body, site.at(p.east, p.north), p.yaw.to_radians(), p.lift)?;
        }
        let scenario_end = set.next_free();
        Ok(Builds { scenario_end, set, rules, policy: Box::new(DistancePolicy::default()), events: Vec::new(), bursts: Vec::new(), hits: 0, now: 0.0, stats: SimStats::default(), strikes: Vec::new() })
    }

    pub fn blast(&mut self, at: DVec3, d: BlastDef) {
        self.hits += 1;
        self.set.blast(at, d.energy, d.radius, &self.rules, &mut self.events, self.hits);
    }

    /// Part `part` of structure `id` blown out along `push` (its frame) with `energy` J, decided
    /// here (told instead, if every game must do it).
    pub fn blow_out(&mut self, id: u64, part: u32, push: glam::Vec3, energy: f32) {
        if self.set.get(id).is_some_and(|s| s.shared) {
            self.strikes.push(Strike::Blow { id, part, push, energy });
            return;
        }
        self.hits += 1;
        self.set.blow_out(id, part, push, energy, &self.rules, &mut self.events, self.hits);
    }

    /// A shot from `from` along `dir`: where it struck a structure within `max` m.
    pub fn shoot(&mut self, from: DVec3, dir: DVec3, s: &ShotDef, max: f64, seed: u64) -> Option<DVec3> {
        self.set.shoot(from, dir, s.energy, s.area, max, &self.rules, &mut self.events, seed)
    }

    /// A hit on structure `id` given in its own frame, decided here (a round of ours struck it):
    /// done, or told instead if every game must do it.
    pub fn hit(&mut self, id: u64, hit: &lunar_core::structure::damage::Hit) {
        if self.set.get(id).is_some_and(|s| s.remote || s.shared) {
            self.strikes.push(Strike::Hit { id, hit: *hit });
            return;
        }
        self.hits += 1;
        self.set.hit(id, hit, &self.rules, &mut self.events, self.hits);
    }

    /// What was decided here to be done to shared structures since this was last asked, into
    /// `out` (in the order it was decided).
    pub fn take_strikes(&mut self, out: &mut Vec<Strike>) {
        out.extend(self.set.told.drain(..).map(|(id, hit)| Strike::Hit { id, hit }));
        out.append(&mut self.strikes);
    }

    /// A strike decided in some game (this one too) done here, with the dice it was told with:
    /// every game that does it the same, in the same order, ends with the same structure.
    /// `ours`: it was decided here (what it sets off round the struck structure is ours to say).
    /// `pose`: how the struck structure's articulations were where it was decided (none: as
    /// they are here).
    pub fn strike_done(&mut self, s: &Strike, seed: u64, ours: bool, pose: &[glam::Affine3A]) {
        let from = self.events.len();
        let (rules, events) = (&self.rules, &mut self.events);
        match *s {
            Strike::Hit { id, hit } => self.set.posed(id, pose, |set| {
                set.hit(id, &hit, rules, events, seed);
            }),
            Strike::Blow { id, part, push, energy } => self.set.posed(id, pose, |set| set.blow_out(id, part, push, energy, rules, events, seed)),
        }
        self.burst_of(from, ours);
    }

    /// How structure `id`'s articulations are posed now (what a strike decided here is told
    /// with: `strike_done`); empty if it has none.
    pub fn pose_of(&self, id: u64) -> &[glam::Affine3A] {
        self.set.get(id).map_or(&[], |s| &s.bones[1..])
    }

    /// What burst of what was done since event `from`, kept with whose say the rest of it is.
    fn burst_of(&mut self, from: usize, ours: bool) {
        let mut k = from;
        while k < self.events.len() {
            if let Event::Burst { at, kind, id, local, seed } = self.events[k] {
                self.bursts.push((at, kind, id, local, seed, ours));
                self.events.remove(k);
            } else {
                k += 1;
            }
        }
    }

    /// Structures run at their level (watched from `eye`), and what lives among them is stepped
    /// with them (`among`: the world has no other clock); what broke since the last frame shows
    /// its material's effect.
    pub fn update(&mut self, dt: f64, bodies: &BodyRegistry, fx: &mut Effects, eye: DVec3, among: &mut [&mut dyn Among]) {
        self.now += dt;
        // what was let go of (a clamp opened, what held it destroyed) comes off as its own body
        self.set.separate();
        let bursts = std::mem::take(&mut self.bursts);
        for (at, kind, id, local, seed, ours) in bursts {
            let Some(b) = self.set.lib.catalog.parts[usize::from(kind)].def.burst.clone() else { continue };
            // (where it is now, on what it was part of)
            let at = self.set.get(id).map_or(at, |s| s.to_world(local));
            let _ = fx.explode_scaled(&b.effect, bodies, bodies.dominant(at), at, 1.0);
            // (on what every game has, one says it: the rest only see it; anything else, here)
            let say = self.set.get(id).is_none_or(|s| !s.shared || s.owned || (!s.remote && ours));
            if say {
                self.hits += 1;
                self.set.blast(at, b.energy, b.radius, &self.rules, &mut self.events, seed ^ self.hits);
            }
        }
        self.stats = self.set.simulate_with(self.now, dt, bodies, self.policy.as_ref(), &[eye], among);
        let cat = &self.set.lib.catalog;
        let mut shown = 0;
        for e in self.events.drain(..) {
            let (at, size, material) = match e {
                Event::Shattered { at, size, material } | Event::Chipped { at, size, material } => (at, size, material),
                Event::Parted { .. } => continue,
                Event::Burst { at, kind, id, local, seed } => {
                    self.bursts.push((at, kind, id, local, seed, true));
                    continue;
                }
            };
            if shown >= EFFECTS_PER_FRAME {
                continue;
            }
            shown += 1;
            // the part is gone into its effect (core::structure::fracture), as big as the part
            if let Some(effect) = &cat.materials[usize::from(material)].1.breaks {
                let _ = fx.explode_scaled(effect, bodies, bodies.dominant(at), at, (size / BREAK_EFFECT_SIZE).clamp(0.5, 4.0));
            }
        }
    }

    /// This frame's structures to the renderer, seen from `eye`.
    pub fn show(&self, r: &mut Renderer, eye: DVec3) {
        r.set_structures(&self.set.list, &self.set.lib, eye);
    }
}
