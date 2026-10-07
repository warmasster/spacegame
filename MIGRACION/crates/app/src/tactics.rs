//! Between the ships' tactical systems (`lunar_ship::tactical`) and the world: a few times a
//! second every ship whose sensors are on is told what is round it — the other ships, what is
//! loose, the traffic, the missiles and the decoys in flight — each as it would look to a sensor
//! (what it reflects, how hot it is, what it radiates and at whom); and what the ships' weapons
//! let fly becomes rounds, guided missiles and decoys (`blasts`). A ship knows nothing of all
//! this: only contacts in its own frame.
//!
//! Cost: nothing while no ship has a sensor on. A look is one list of things (made once for
//! all) and, per looking ship, one pass over it: a range gate, the horizon of the body it is
//! over, a change of frame.
use crate::{
    blasts::{Blasts, Launch, RAIL, What},
    builds::Builds,
    ships::Ships,
};
use glam::DVec3;
use lunar_core::{
    body::BodyRegistry,
    guided::{Aim, Band, DECOY_ID, MISSILE_ID},
    traffic::Traffic,
};
use lunar_ship::tactical::{Class, Contact, Load};

/// Ids of traffic craft, as contacts and targets: this bit and their number.
pub const CRAFT_ID: u64 = 1 << 61;
/// Seconds between looks.
const EVERY: f64 = 0.25;
/// Nothing is told of from further than this (m).
const REACH: f64 = 450_000.0;
/// What is lighter than this is not worth a track (kg).
const SMALL: f32 = 150.0;
/// A guided missile's own radar, as a share of a ship's (heard some 15 km off), and what a
/// missile and a traffic craft look like.
const SEEKER: f32 = 0.004;
const MISSILE_RCS: f32 = 0.05;
const CRAFT_RCS: f32 = 12.0;

/// One thing a sensor might find, in the world.
#[derive(Clone, Copy, Debug)]
struct Thing {
    id: u64,
    pos: DVec3,
    vel: DVec3,
    rcs: f32,
    heat: f32,
    erp: f32,
    beam: DVec3,
    cone: f32,
    lock: Option<u64>,
    jam: f32,
    code: u32,
    class: Class,
}

#[derive(Default)]
pub struct Tactics {
    things: Vec<Thing>,
    /// Where each traffic craft was when last looked at, and when: how it moves is the difference.
    craft_was: Vec<DVec3>,
    craft_t: f64,
    next: f64,
    /// How many ships there were when their weapons were last fitted with what their rounds do.
    fitted: usize,
    /// What the weapons let fly this frame (reused).
    launches: Vec<Shot>,
}

/// What a structure of radius `r` reflects when nothing says otherwise (m²).
fn rcs_of(r: f32) -> f32 {
    0.08 * std::f32::consts::PI * r * r
}

impl Tactics {
    /// Every weapon of every kind of ship fires something there is.
    pub fn check(ships: &Ships, blasts: &Blasts) -> Result<(), String> {
        for kind in &ships.kinds {
            for m in kind.machines.iter().filter(|m| m.def.modelo == "arma") {
                let ammo = m.def.params.get("municion").and_then(|v| v.as_str()).unwrap_or_default();
                let known = match m.def.params.get("clase").and_then(|v| v.as_str()) {
                    Some("guiado") => blasts.guided.kind(ammo).is_some(),
                    Some("senuelo") => blasts.guided.decoy_kind(ammo).is_some(),
                    _ => blasts.shot_speed(ammo).is_some(),
                };
                if !known {
                    return Err(format!("{}: el arma '{}' lanza '{ammo}', que no existe (shots.jsonc, guiados.jsonc, senuelos.jsonc)", kind.id, m.id));
                }
            }
        }
        Ok(())
    }

    /// Where a target of the guided missiles is and how it shines.
    fn aim(id: u64, ships: &Ships, builds: &Builds, traffic: Option<&Traffic>, was: &[DVec3], dt: f64) -> Option<Aim> {
        if id & CRAFT_ID != 0 {
            let k = (id & !CRAFT_ID) as usize;
            let pos = *traffic?.pos.get(k)?;
            let vel = was.get(k).filter(|_| dt > 0.0).map_or(DVec3::ZERO, |w| (pos - *w) / dt);
            return Some(Aim { pos, vel, rcs: CRAFT_RCS, heat: 5.0e6 });
        }
        let s = builds.set.get(id)?;
        let ship = ships.by_structure(id).map(|n| &ships.list[n]);
        let rcs = ship.and_then(|sh| sh.kind.def.firma).map_or(rcs_of(s.radius), |f| f.rcs);
        Some(Aim { pos: s.to_world(s.center), vel: s.vel, rcs, heat: ship.map_or(0.0, |sh| sh.heat) })
    }

    /// Before the ships run: the guided missiles are told where what they follow is (every
    /// frame), and — every `EVERY` s, while any ship is looking — the ships what is round them.
    pub fn look(&mut self, ships: &mut Ships, builds: &Builds, bodies: &BodyRegistry, traffic: Option<&Traffic>, blasts: &mut Blasts) {
        let now = builds.set.now;
        // a gun's sight is worked out with what its rounds do
        if self.fitted != ships.list.len() {
            self.fitted = ships.list.len();
            for tac in ships.list.iter_mut().filter_map(|sh| sh.tactical.as_mut()) {
                for w in tac.weapons.iter_mut().filter(|w| w.load == Load::Round) {
                    if let Some((speed, reach)) = blasts.shot_speed(&w.ammo) {
                        (w.speed, w.reach) = (speed, w.reach.min(reach));
                    }
                }
            }
        }
        blasts.aims.clear();
        for k in 0..blasts.guided.list.len() {
            let Some(id) = blasts.guided.list[k].target.filter(|id| id & DECOY_ID == 0) else { continue };
            if blasts.aims.iter().any(|a| a.0 == id) {
                continue;
            }
            if let Some(a) = Self::aim(id, ships, builds, traffic, &self.craft_was, now - self.craft_t) {
                blasts.aims.push((id, a));
            }
        }
        if now < self.next {
            return;
        }
        self.next = now + EVERY;
        if !ships.list.iter().any(|sh| sh.tactical.as_ref().is_some_and(|t| t.looking)) {
            return;
        }
        // ---- everything there is to find, once for all ----
        self.things.clear();
        for s in &builds.set.list {
            let (pos, vel) = (s.to_world(s.center), s.vel);
            let Some(sh) = ships.by_structure(s.id).map(|n| &ships.list[n]) else {
                if s.mass >= SMALL && s.owner.is_none() {
                    self.things.push(Thing { id: s.id, pos, vel, rcs: rcs_of(s.radius), heat: 0.0, erp: 0.0, beam: DVec3::Z, cone: -1.0, lock: None, jam: 0.0, code: 0, class: Class::Debris });
                }
                continue;
            };
            let rcs = sh.kind.def.firma.map_or(rcs_of(s.radius), |f| f.rcs);
            let e = sh.tactical.as_ref().map(|t| t.emission).unwrap_or_default();
            self.things.push(Thing { id: s.id, pos, vel, rcs, heat: sh.heat, erp: e.erp, beam: (s.rot * e.beam).as_dvec3(), cone: e.cone, lock: e.lock, jam: e.jam, code: e.code, class: Class::Ship });
        }
        if let Some(t) = traffic {
            let dt = now - self.craft_t;
            let moved = self.craft_was.len() == t.pos.len() && dt > 0.0;
            for (k, &pos) in t.pos.iter().enumerate() {
                let vel = if moved { (pos - self.craft_was[k]) / dt } else { DVec3::ZERO };
                // (a craft standing on its pad is cold; its beacon answers all the same)
                let heat = if vel.length_squared() > 1.0 { 5.0e6 } else { 2.0e4 };
                self.things.push(Thing { id: CRAFT_ID | k as u64, pos, vel, rcs: CRAFT_RCS, heat, erp: 0.0, beam: DVec3::Z, cone: -1.0, lock: None, jam: 0.0, code: 2000 + (k as u32 % 5000), class: Class::Craft });
            }
            self.craft_was.clear();
            self.craft_was.extend_from_slice(&t.pos);
            self.craft_t = now;
        }
        for m in &blasts.guided.list {
            let def = &blasts.guided.defs[usize::from(m.kind)].1;
            let erp = if def.seeker == Band::Radar && m.target.is_some() { SEEKER } else { 0.0 };
            let heat = if m.t < def.burn { 3.0e6 } else { 3.0e4 };
            self.things.push(Thing { id: MISSILE_ID | u64::from(m.id), pos: m.pos, vel: m.vel, rcs: MISSILE_RCS, heat, erp, beam: DVec3::Z, cone: -1.0, lock: m.target, jam: 0.0, code: 0, class: Class::Missile });
        }
        for d in &blasts.guided.decoys {
            let (rcs, heat) = blasts.guided.shine(d);
            self.things.push(Thing { id: DECOY_ID | u64::from(d.id), pos: d.pos, vel: d.vel, rcs, heat, erp: 0.0, beam: DVec3::Z, cone: -1.0, lock: None, jam: 0.0, code: 0, class: Class::Decoy });
        }
        // ---- each looking ship: what of it is within reach and over its horizon, in its frame ----
        for sh in &mut ships.list {
            let Some(tac) = sh.tactical.as_mut().filter(|t| t.looking) else { continue };
            let Some(s) = builds.set.get(sh.structure) else { continue };
            let (own, inv) = (s.to_world(s.center), s.rot.inverse());
            let body = bodies.get(bodies.dominant(own));
            let (to_center, r2) = (body.center - own, (body.radius * 0.9995) * (body.radius * 0.9995));
            tac.sensed.clear();
            for th in &self.things {
                if th.id == sh.structure {
                    continue;
                }
                let d = th.pos - own;
                let far = d.length_squared();
                if far > REACH * REACH {
                    continue;
                }
                // behind the body: the nearest the line to it comes to the body's middle
                let along = (to_center.dot(d) / far.max(1.0)).clamp(0.0, 1.0);
                if (to_center - d * along).length_squared() < r2 {
                    continue;
                }
                tac.sensed.push(Contact {
                    id: th.id,
                    pos: s.to_local(th.pos),
                    vel: inv * (th.vel - s.vel).as_vec3(),
                    rcs: th.rcs,
                    heat: th.heat,
                    erp: th.erp,
                    beam: inv * th.beam.as_vec3(),
                    cone: th.cone,
                    locks: th.lock == Some(sh.structure),
                    jam: th.jam,
                    code: th.code,
                    class: th.class,
                });
            }
            tac.fresh = true;
        }
    }

    /// After the ships ran: what their weapons let fly leaves each from its own muzzle, with
    /// the speed of the ship there, through the one way anything is let fly (`Blasts::launch`,
    /// which tells the other games). A ship simulated in another player's game fires nothing
    /// here: what it fires comes told, as theirs fired it.
    pub fn fire(&mut self, ships: &mut Ships, builds: &mut Builds, bodies: &BodyRegistry, blasts: &mut Blasts) {
        let mut launches = std::mem::take(&mut self.launches);
        for sh in &mut ships.list {
            let Some(tac) = sh.tactical.as_mut().filter(|t| !t.fired.is_empty()) else { continue };
            let mut fired = std::mem::take(&mut tac.fired);
            if let Some(s) = builds.set.get(sh.structure).filter(|s| !s.remote) {
                for f in &fired {
                    let w = &tac.weapons[usize::from(f.weapon)];
                    let rt = &sh.machines[w.machine];
                    let Some(part) = rt.part.and_then(|p| s.parts.get(p as usize)) else { continue };
                    let axis = part.local.transform_vector3(rt.thrust_axis).normalize_or(glam::Vec3::Z);
                    let from = s.to_world(part.center + axis * (part.radius + 0.35));
                    let (dir, vel, by) = ((s.rot * axis).as_dvec3(), s.velocity_at(from), Some(sh.structure));
                    match w.load {
                        Load::Round => {
                            for _ in 0..f.count {
                                launches.push(Shot::Named(w.ammo.clone(), from, dir, vel, by));
                            }
                        }
                        Load::Guided => {
                            if let Some(kind) = blasts.guided.kind(&w.ammo) {
                                launches.push(Shot::Launch(Launch { what: What::Guided(kind), from, dir, speed: RAIL, vel, target: f.target, by }));
                            }
                        }
                        Load::Decoy => {
                            if let Some(kind) = blasts.guided.decoy_kind(&w.ammo) {
                                let speed = blasts.guided.decoy_defs[usize::from(kind)].1.salida;
                                for _ in 0..f.count {
                                    launches.push(Shot::Launch(Launch { what: What::Decoy(kind), from, dir, speed, vel, target: None, by }));
                                }
                            }
                        }
                    }
                }
            }
            fired.clear();
            tac.fired = fired;
        }
        for l in launches.drain(..) {
            match l {
                Shot::Named(id, from, dir, vel, by) => blasts.fire_from(&id, from, dir, vel, by, bodies, builds),
                Shot::Launch(l) => blasts.launch(l, bodies, builds),
            };
        }
        self.launches = launches;
    }
}

/// What a ship's weapon lets fly, gathered while the ships are read and let fly after.
pub enum Shot {
    /// A round of a shot named in the data (scattered as that shot is).
    Named(String, DVec3, DVec3, DVec3, Option<u64>),
    Launch(Launch),
}

