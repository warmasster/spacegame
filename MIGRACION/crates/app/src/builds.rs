//! The structures of the scenario in play (core::structure): set round the site at the start,
//! hurt by blasts and shots, their loose pieces falling and settling, what breaks shown by its
//! material's effect, all handed to the renderer every frame.
use glam::DVec3;
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
    bursts: Vec<(DVec3, u16)>,
    hits: u64,
    now: f64,
    /// How many ran at each level last frame.
    pub stats: SimStats,
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
        Ok(Builds { set, rules, policy: Box::new(DistancePolicy::default()), events: Vec::new(), bursts: Vec::new(), hits: 0, now: 0.0, stats: SimStats::default() })
    }

    pub fn blast(&mut self, at: DVec3, d: BlastDef) {
        self.hits += 1;
        self.set.blast(at, d.energy, d.radius, &self.rules, &mut self.events, self.hits);
    }

    /// Part `part` of structure `id` blown out along `push` (its frame) with `energy` J.
    pub fn blow_out(&mut self, id: u64, part: u32, push: glam::Vec3, energy: f32) {
        self.hits += 1;
        self.set.blow_out(id, part, push, energy, &self.rules, &mut self.events, self.hits);
    }

    /// A shot from `from` along `dir`: where it struck a structure within `max` m.
    pub fn shoot(&mut self, from: DVec3, dir: DVec3, s: &ShotDef, max: f64, seed: u64) -> Option<DVec3> {
        self.set.shoot(from, dir, s.energy, s.area, max, &self.rules, &mut self.events, seed)
    }

    /// A hit on structure `id` given in its own frame (where it struck was decided in another
    /// player's game: `multi`), done to it here: this game simulates it.
    pub fn hit(&mut self, id: u64, hit: &lunar_core::structure::damage::Hit) {
        self.hits += 1;
        self.set.hit(id, hit, &self.rules, &mut self.events, self.hits);
    }

    /// Structures run at their level (watched from `eye`), and what lives among them is stepped
    /// with them (`among`: the world has no other clock); what broke since the last frame shows
    /// its material's effect.
    pub fn update(&mut self, dt: f64, bodies: &BodyRegistry, fx: &mut Effects, eye: DVec3, among: &mut [&mut dyn Among]) {
        self.now += dt;
        // what was let go of (a clamp opened, what held it destroyed) comes off as its own body
        self.set.separate();
        for (at, kind) in std::mem::take(&mut self.bursts) {
            let Some(b) = self.set.lib.catalog.parts[usize::from(kind)].def.burst.clone() else { continue };
            let _ = fx.explode_scaled(&b.effect, bodies, bodies.dominant(at), at, 1.0);
            self.hits += 1;
            self.set.blast(at, b.energy, b.radius, &self.rules, &mut self.events, self.hits);
        }
        self.stats = self.set.simulate_with(self.now, dt, bodies, self.policy.as_ref(), &[eye], among);
        let cat = &self.set.lib.catalog;
        let mut shown = 0;
        for e in self.events.drain(..) {
            let (at, size, material) = match e {
                Event::Shattered { at, size, material } | Event::Chipped { at, size, material } => (at, size, material),
                Event::Parted { .. } => continue,
                Event::Burst { at, kind } => {
                    self.bursts.push((at, kind));
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
