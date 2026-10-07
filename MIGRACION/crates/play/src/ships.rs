//! Ships in play (lunar-ship): each one a structure of the world with its systems. Every frame
//! each ship gets the world as it is where it is now (`lunar_ship::World::at`: what pulls it
//! there, the sun, its height over the ground under it, its speed — on any body, in orbit, or
//! past every body's reach where nothing pulls and there is no ground), runs its
//! systems, and shows its panels, screens, lamps and rams. What its machines blow up goes to the
//! structures as a blast.
use crate::builds::Builds;
use glam::{DVec3, Vec3};
use lunar_core::{
    body::{BodyId, BodyRegistry},
    effect_defs::BlastDef,
    effects::Effects,
    font::Font,
};
use lunar_core::structure::state::Structure;
use lunar_ship::{Pace, Ship, ShipKind, World};
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// A ship this near the eye (m) runs every tick; one farther than `ASLEEP_FROM` and at rest is
/// only looked at now and then. In between, and near but with nothing happening in it, it ticks
/// twice a second with the rest settled (`lunar_ship::Pace`).
const FULL_WITHIN: f64 = 120.0;
const ASLEEP_FROM: f64 = 4000.0;
/// Irradiance of the sun at 1 AU (W/m²).
const SOLAR: f64 = 1361.0;
/// Energy (J) that throws the pieces of a plate the air tore out.
const TORN_ENERGY: f32 = 3.0e4;

pub struct Ships {
    pub kinds: Vec<Arc<ShipKind>>,
    pub list: Vec<Ship>,
    pub font: Font,
    seed: u64,
    /// Ships paired with their structure last frame, and how many of them ran in full (tools).
    paired: usize,
    pub full: usize,
    /// The explosions the ships' machines set off this frame (the effect, where, how big), for
    /// whoever tells the other players of them (`multi`); emptied every frame.
    pub booms: Vec<(&'static str, DVec3, f32)>,
}

/// Ambient pressure at `p` (Pa): bodies have no atmosphere yet; this is where one goes.
fn ambient_pressure(_bodies: &BodyRegistry, _p: DVec3) -> f64 {
    0.0
}

/// The sun's light at `p`, 0 inside the shadow of any body.
fn sunlight(bodies: &BodyRegistry, p: DVec3, sun: DVec3) -> f64 {
    let dark = bodies.iter().any(|(_, b)| {
        let d = p - b.center;
        let t = d.dot(sun);
        t < 0.0 && (d - sun * t).length() <= b.radius
    });
    if dark { 0.0 } else { SOLAR }
}

impl Ships {
    pub fn new(kinds: Vec<Arc<ShipKind>>, font: Font) -> Ships {
        Ships { kinds, list: Vec::new(), font, seed: 0x5eed_0001, paired: usize::MAX, full: 0, booms: Vec::new() }
    }

    pub fn kind(&self, id: &str) -> Option<Arc<ShipKind>> {
        self.kinds.iter().find(|k| k.id == id).cloned()
    }

    /// A ship of kind `id` standing on body `body` at unit direction `dir`, its nose `yaw` rad from
    /// north toward east. Returns its structure id.
    pub fn spawn(&mut self, builds: &mut Builds, bodies: &BodyRegistry, id: &str, body: BodyId, dir: DVec3, yaw: f64) -> Result<u64, String> {
        let kind = self.kind(id).ok_or_else(|| format!("nave desconocida '{id}'"))?;
        let sid = builds.set.place(&kind.blueprint, bodies, body, dir, yaw, f64::from(kind.lift))?;
        self.attach(builds, kind, sid)?;
        // on its gear as it stands (posed), over the ground under each foot; then down on its
        // legs as far as its weight presses them in here
        builds.set.rest_on_ground(sid, bodies);
        if let (Some(n), Some(k)) = (self.by_structure(sid), builds.set.index_of(sid)) {
            let s = &mut builds.set.list[k];
            let (g, up) = (bodies.field(s.pos).g() as f32, bodies.get(body).up(s.pos));
            let sink = self.list[n].rest_on_legs(s, g);
            s.pos -= up * f64::from(sink);
        }
        self.dock(builds, sid)?;
        Ok(sid)
    }

    /// The ships `sid` carries docked as built (`ShipDef::lleva`): each made over its cradle and
    /// taken by it.
    fn dock(&mut self, builds: &mut Builds, sid: u64) -> Result<(), String> {
        let Some(n) = self.by_structure(sid) else { return Ok(()) };
        let kind = self.list[n].kind.clone();
        for carried in &kind.def.lleva {
            let c = kind.clamps.iter().position(|c| c.id == carried.anclaje).ok_or_else(|| format!("{}: lleva '{}' en un anclaje que no tiene: '{}'", kind.id, carried.nave, carried.anclaje))?;
            let zone = kind.clamps[c].zone.ok_or_else(|| format!("{}: el anclaje '{}' no toma nada", kind.id, carried.anclaje))?;
            let (at, rot) = {
                let s = builds.set.get(sid).ok_or("sin estructura")?;
                (s.to_world(zone.centre), s.rot)
            };
            let id = self.spawn_free(builds, &carried.nave, at, rot)?;
            // its middle over the cradle's, then set down in it and held
            if let Some(k) = builds.set.index_of(id) {
                let s = &mut builds.set.list[k];
                s.pos = at - (s.rot * s.center).as_dvec3();
            }
            let n = self.by_structure(sid).ok_or("sin nave")?;
            self.list[n].work_clamp(c, false);
            lunar_ship::cargo::serve(&mut self.list[n], &mut builds.set);
        }
        Ok(())
    }

    /// A ship of kind `id` free at `pos` turned `rot` (in orbit, in deep space, or falling).
    pub fn spawn_free(&mut self, builds: &mut Builds, id: &str, pos: DVec3, rot: glam::Quat) -> Result<u64, String> {
        let kind = self.kind(id).ok_or_else(|| format!("nave desconocida '{id}'"))?;
        let sid = builds.set.spawn(&kind.blueprint, pos, rot)?;
        self.attach(builds, kind, sid)
    }

    /// The systems of a ship of kind `id` on structure `sid`, which is there already (made from
    /// what another player's game told of it), its pieces drawn from `seed` as theirs were.
    pub fn adopt(&mut self, builds: &mut Builds, id: &str, sid: u64, seed: u64) -> Result<u64, String> {
        let kind = self.kind(id).ok_or_else(|| format!("nave desconocida '{id}'"))?;
        let next = std::mem::replace(&mut self.seed, seed);
        let made = self.attach_seeded(builds, kind, sid);
        self.seed = next;
        made
    }

    /// The systems of a new ship on structure `sid`.
    fn attach(&mut self, builds: &mut Builds, kind: Arc<ShipKind>, sid: u64) -> Result<u64, String> {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.attach_seeded(builds, kind, sid)
    }

    fn attach_seeded(&mut self, builds: &mut Builds, kind: Arc<ShipKind>, sid: u64) -> Result<u64, String> {
        let mut ship = Ship::new(kind, sid, self.seed)?;
        if let Some(s) = builds.set.list.iter_mut().find(|s| s.id == sid) {
            s.owner = Some(ship.kind.id.as_str().into());
            // its moving parts as they start (ramp down, gear down, nacelles up)
            ship.update(s, &World::default(), 0.0);
            self.list.push(ship);
        }
        Ok(sid)
    }

    /// Run every ship's systems for `dt` s, all at once (each on its own structure, a thread
    /// each as there are cores), each at its pace: in full the ones in `awake` (the ones players
    /// ride, the one a tool works on), the ones near any of `watchers` (the players, a camera) and
    /// the ones with something going on; the rest a tick now and then.
    /// `people`: whoever stands about (spheres in the world): what moves stops at them.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f64, builds: &mut Builds, bodies: &BodyRegistry, fx: &mut Effects, sun: DVec3, watchers: &[DVec3], awake: &[u64], people: &[(DVec3, f32)]) {
        // ships whose structure is gone are gone
        if self.list.len() != self.paired {
            let alive: HashSet<u64> = builds.set.list.iter().map(|s| s.id).collect();
            self.list.retain(|sh| alive.contains(&sh.structure));
        }
        let now = builds.set.now;
        let mut by_id: HashMap<u64, &mut Structure> = builds.set.list.iter_mut().filter(|s| s.owner.is_some()).map(|s| (s.id, s)).collect();
        let mut pairs: Vec<(&mut Ship, &mut Structure)> = self.list.iter_mut().filter_map(|sh| by_id.remove(&sh.structure).map(|s| (sh, s))).collect();
        self.paired = pairs.len();
        let pace = |sh: &Ship, s: &Structure| {
            let c = s.to_world(s.center);
            let d = watchers.iter().map(|w| c.distance(*w)).fold(f64::MAX, f64::min) - f64::from(s.radius);
            let moving = !s.anchored && !s.resting;
            if awake.contains(&s.id) || sh.busy() || moving || s.awake_until > now || d < FULL_WITHIN {
                Pace::Full
            } else if d < ASLEEP_FROM {
                Pace::Slow
            } else {
                Pace::Asleep
            }
        };
        let run = |(sh, s): &mut (&mut Ship, &mut Structure)| {
            let com = s.to_world(s.com);
            let low = s.to_world(Vec3::new(0.0, sh.keel(s), 0.0));
            // what holds where it is now (what pulls it, the ground under it if any), and the
            // sun, the air and the cold there
            let w = World { sun: s.rot.inverse() * sun.as_vec3(), irradiance: sunlight(bodies, com, sun), pressure: ambient_pressure(bodies, com), sink: 230.0, ..World::at(s, bodies, low) };
            let p = pace(sh, s);
            sh.run(s, &w, dt, p);
            u8::from(p == Pace::Full)
        };
        // a few ships are quicker one after another than handed out to threads
        self.full = if pairs.len() >= 8 { pairs.par_iter_mut().map(run).map(usize::from).sum() } else { pairs.iter_mut().map(run).map(usize::from).sum() };
        let mut blasts: Vec<(DVec3, BlastDef)> = Vec::new();
        let mut torn: Vec<(u64, u32, Vec3)> = Vec::new();
        self.booms.clear();
        for (sh, s) in &mut pairs {
            // (a copy of a ship simulated in another player's game: what blows and tears in it
            // is theirs to say)
            if s.remote {
                sh.torn.clear();
                sh.bursts.clear();
                continue;
            }
            for (p, dir) in sh.torn.drain(..) {
                torn.push((sh.structure, p, dir));
            }
            for burst in sh.bursts.drain(..) {
                let at = s.to_world(burst.at);
                let radius = (2.0 + (burst.energy / 1e6).cbrt() * 3.0).min(40.0) as f32;
                blasts.push((at, BlastDef { energy: burst.energy as f32, radius }));
            }
        }
        drop(pairs);
        // plates torn out by the air behind them fly out in pieces
        for (sid, p, dir) in torn {
            builds.blow_out(sid, p, dir, TORN_ENERGY);
        }
        for (at, d) in blasts {
            let scale = (d.radius / 4.0).clamp(0.5, 4.0);
            let _ = fx.explode_scaled("impacto_grande", bodies, bodies.dominant(at), at, scale);
            self.booms.push(("impacto_grande", at, scale));
            builds.blast(at, d);
        }
        self.clamps(builds);
        // what moved stops at what is in its way: the ground, what is loose, whoever is there
        for sh in &mut self.list {
            sh.stop_at_obstacles(&mut builds.set, bodies, people);
        }
    }

    /// What the ships' clamps ask: take what is loose in their zone, let go of what they hold
    /// (`lunar_ship::cargo::serve`).
    fn clamps(&mut self, builds: &mut Builds) {
        for sh in &mut self.list {
            // (what the clamps of a ship simulated elsewhere take and let go is told by whoever
            // simulates it)
            if builds.set.get(sh.structure).is_some_and(|s| s.remote) {
                sh.clamp_asks.clear();
                continue;
            }
            lunar_ship::cargo::serve(sh, &mut builds.set);
        }
    }

    /// The ship on structure `id`.
    pub fn by_structure(&self, id: u64) -> Option<usize> {
        self.list.iter().position(|s| s.structure == id)
    }
}
