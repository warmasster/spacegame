//! What a ship knows of what is round it and what it does about it (`docs/COMBATE.md`): its
//! sensors make tracks, one of them is chosen and maybe locked, its weapons are let fire at it,
//! its decoys are let go, and all of it is drawn for its screens. Any ship that mounts one of
//! these machines has it (found by their model: nothing is named):
//!
//! - `radar`: finds what is inside its sector and within reach — the reach it was built for
//!   against a square metre, times the fourth root of how much it radiates (its range scale) and
//!   of what the thing reflects. It gives the others away: whoever has an `alertador` hears it
//!   from further than it sees (one way against there and back);
//! - `irst`: finds what is hot, without radiating;
//! - `alertador`: hears the radars that sweep it, the ones locked on it and the missiles homing
//!   on it, by bearing;
//! - `transpondedor`: answers with its code, and makes whatever answers with one a track, radar
//!   or no radar (traffic is seen by its beacon);
//! - `perturbador`: noise that shortens the reach of the radars looking at it, and a beacon for
//!   every warner;
//! - `arma`: cannons, rails and dispensers, in groups (`grupo`); the master switch, the group
//!   chosen and the trigger say which may fire.
//!
//! The world is told to it as `Contact`s (`sensed`, ship frame, whenever its owner looks: a few
//! times a second) and it tells the world back what it radiates (`emission`) and what it let
//! fly (`fired`). It knows nothing of structures, of traffic or of who is a player.
//!
//! Who is what is only what is known: a track with no code is UNKNOWN, with one CIVIL, with the
//! ship's own FRIEND; HOSTILE is whatever locks it or homes on it, or what the pilot says. Any
//! track can be locked and fired at: the master arm is the only safety.
//!
//! Cost: nothing while nothing is switched on; a scan is a loop over the contacts given and a
//! table of `TRACKS` tracks in place; a tick is a few signals. Nothing is allocated once warm.
use crate::{
    kind::ShipKind,
    plots::{Plots, Shape},
    ship::MachineRt,
};
use glam::Vec3;
use lunar_core::structure::state::Structure;
use lunar_machines::models::combat::Radio;
use lunar_signals::{SignalId, Store, Writer};

/// Tracks kept at once.
pub const TRACKS: usize = 32;
/// A track nobody has seen for this long is dropped (s); a lock coasts this long.
const STALE: f64 = 3.0;
const COAST: f64 = 1.5;
/// What a radar's sidelobes leak, as a share of its beam.
const SIDELOBE: f32 = 0.003;
/// How far a beacon is heard (m).
const BEACON_REACH: f32 = 400_000.0;
/// A decoy takes a lock from a target it is this near (m, and the cosine of the angle between
/// them) when it reflects this many times more.
const GATE: (f32, f32, f32) = (350.0, 0.9994, 1.6);
/// A cannon's solution holds this near the lead point (rad), this far (s of flight at most).
const ON_AIM: f32 = 0.012;
const MAX_FLIGHT: f32 = 4.0;

/// What a thing is, as far as the world knows (a ship never reads it to tell friend from foe:
/// only to draw a missile as one once its warner says so, and to model what decoys do).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Class {
    #[default]
    Ship,
    /// Light traffic: a flight plan, not a simulated ship.
    Craft,
    Missile,
    Decoy,
    Debris,
}

/// One thing round the ship, as its owner tells it (ship frame, relative to the ship).
#[derive(Clone, Copy, Debug, Default)]
pub struct Contact {
    pub id: u64,
    pub pos: Vec3,
    pub vel: Vec3,
    /// What it reflects (m²) and what it radiates in the infrared (W).
    pub rcs: f32,
    pub heat: f32,
    /// Its radar: the share of a reference emitter's power it radiates (0: silent), the way it
    /// looks (ship frame, unit) and the cosine of its half sector (−1: all round).
    pub erp: f32,
    pub beam: Vec3,
    pub cone: f32,
    /// Its radar (or its seeker) is locked on this ship.
    pub locks: bool,
    /// Noise it puts out (share of a reference jammer).
    pub jam: f32,
    /// The code its transponder answers with (0: none).
    pub code: u32,
    pub class: Class,
}

/// What the ship radiates, for whoever tells the others of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Emission {
    /// Its radar: share of a reference emitter's power (0 silent), which way (ship frame) and
    /// the cosine of its half sector.
    pub erp: f32,
    pub beam: Vec3,
    pub cone: f32,
    /// What it is locked on.
    pub lock: Option<u64>,
    pub jam: f32,
    pub code: u32,
    /// Heat of its engines and its plant (W).
    pub heat: f32,
}

/// Something a weapon let fly this tick: the weapon (its place in `Tactical::weapons`), how many,
/// and what it was locked on then.
#[derive(Clone, Copy, Debug)]
pub struct Fire {
    pub weapon: u16,
    pub count: u16,
    pub target: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Iff {
    #[default]
    Unknown,
    Friend,
    Civil,
    Hostile,
}

/// How a track is known.
pub const BY_RADAR: u8 = 1;
pub const BY_HEAT: u8 = 2;
pub const BY_BEACON: u8 = 4;
pub const BY_WARNER: u8 = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct Track {
    pub id: u64,
    pub pos: Vec3,
    pub vel: Vec3,
    pub seen: f64,
    pub by: u8,
    pub iff: Iff,
    pub class: Class,
    pub rcs: f32,
    pub heat: f32,
    /// 0 nothing, 1 its radar sweeps this ship, 2 locked on it, 3 a missile homing on it.
    pub threat: u8,
    /// How loud its radar is heard (W/m² of a reference emitter at 1 m).
    pub loud: f32,
}

/// What a weapon lets fly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    /// Rounds that fly where they were sent.
    Round,
    /// Something that steers.
    Guided,
    /// Decoys.
    Decoy,
}

pub struct Weapon {
    pub machine: usize,
    pub group: u8,
    pub load: Load,
    /// What it fires (an id of the shots, the guided missiles or the decoys).
    pub ammo: String,
    /// How fast that leaves it (m/s) and how far it is worth firing (m).
    pub speed: f32,
    pub reach: f32,
    fire: SignalId,
    rounds: SignalId,
    ready: SignalId,
}

struct Sensor {
    machine: usize,
    what: Radio,
    on: SignalId,
    share: SignalId,
    /// The range scale it is on (m; a radar's).
    range: SignalId,
    /// Reach against a square metre at full power (radar) or a megawatt (infrared), m.
    reach: f32,
    /// Cosine of its half sector.
    cone: f32,
    /// How faint a radar it hears (warner): the reference emitter at this many metres.
    hears: f32,
}

/// The pilot's commands: buttons (taken on the way down) and switches.
struct Commands {
    next: SignalId,
    prev: SignalId,
    nearest: SignalId,
    boresight: SignalId,
    lock: SignalId,
    drop: SignalId,
    hostile: SignalId,
    master: SignalId,
    select: SignalId,
    trigger: SignalId,
    auto: SignalId,
    decoy: SignalId,
    program: SignalId,
    code: SignalId,
    was: [bool; 8],
}

struct Outputs {
    count: SignalId,
    selected: SignalId,
    locked: SignalId,
    range: SignalId,
    closure: SignalId,
    az: SignalId,
    el: SignalId,
    iff: SignalId,
    speed: SignalId,
    flight: SignalId,
    error: SignalId,
    on_aim: SignalId,
    threats: SignalId,
    locked_on: SignalId,
    missile: SignalId,
    threat_az: SignalId,
    group: SignalId,
    rounds: SignalId,
    ready: SignalId,
    firing: SignalId,
    decoys: SignalId,
    emitting: SignalId,
}

pub struct Tactical {
    sensors: Vec<Sensor>,
    pub weapons: Vec<Weapon>,
    /// The weapon groups, by name, in the order the selector counts them.
    pub groups: Vec<String>,
    cmd: Commands,
    out: Outputs,
    /// What its owner last told it of what is round it, and whether that is new.
    pub sensed: Vec<Contact>,
    pub fresh: bool,
    /// Its sensors are on: its owner should look round for it.
    pub looking: bool,
    pub tracks: [Track; TRACKS],
    pub len: usize,
    /// The track chosen (its id) and whether it is locked.
    pub chosen: Option<u64>,
    pub locked: bool,
    /// Tracks the pilot called hostile.
    hostile: [u64; 16],
    hostile_len: usize,
    pub emission: Emission,
    /// What its weapons let fly, for its owner to take.
    pub fired: Vec<Fire>,
    /// Where to point to hit the chosen track with the group chosen (ship frame, unit), if it
    /// can be hit.
    pub lead: Option<Vec3>,
    /// The way to the worst threat (ship frame, unit).
    pub threat: Option<Vec3>,
    /// Which launcher of a group of singles goes next; the one a pull of the trigger is told to
    /// (and until when: one pull, one launcher); and the decoy program's clock.
    turn: usize,
    pull: Option<(usize, f64)>,
    trigger_was: bool,
    /// How the ship stood and moved at the last tick (world): its tracks are kept in its own
    /// frame, so between sweeps they turn as it turns and drift as it speeds up.
    stood: Option<(glam::Quat, glam::DVec3)>,
    program_t: f64,
    program_left: u32,
    drawn: f64,
}

fn num(kind: &ShipKind, m: usize, key: &str, or: f64) -> f64 {
    let v = kind.machines[m].def.params.get(key);
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(or),
        Some(v @ serde_json::Value::String(_)) => serde_json::from_value::<lunar_signals::Q>(v.clone()).ok().and_then(|q| q.si().ok()).unwrap_or(or),
        _ => or,
    }
}

impl Tactical {
    /// The tactical system of a ship of `kind`, or none if it mounts nothing of it.
    pub fn new(kind: &ShipKind, store: &mut Store) -> Result<Option<Tactical>, String> {
        let mut sensors = Vec::new();
        let mut weapons: Vec<Weapon> = Vec::new();
        let mut groups: Vec<String> = Vec::new();
        for (m, plan) in kind.machines.iter().enumerate() {
            let id = &plan.id;
            if let Some(what) = Radio::of(&plan.def.modelo) {
                let reach = num(kind, m, "alcance", if what == Radio::Radar { 60_000.0 } else { 40_000.0 }) as f32;
                let sector = num(kind, m, "sector", if what == Radio::Radar { 60.0 } else { 180.0 }) as f32;
                sensors.push(Sensor {
                    machine: m,
                    what,
                    on: store.define(&format!("{id}.on")),
                    share: store.define(&format!("{id}.cuota")),
                    range: store.define(&format!("{id}.alcance")),
                    reach,
                    cone: sector.to_radians().cos(),
                    hears: num(kind, m, "escucha", 250_000.0) as f32,
                });
            } else if plan.def.modelo == "arma" {
                let ammo = plan.def.params.get("municion").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let load = match plan.def.params.get("clase").and_then(|v| v.as_str()) {
                    None | Some("proyectil") => Load::Round,
                    Some("guiado") => Load::Guided,
                    Some("senuelo") => Load::Decoy,
                    Some(c) => return Err(format!("{id}: clase de arma desconocida '{c}' (proyectil, guiado, senuelo)")),
                };
                let group = plan.def.params.get("grupo").and_then(|v| v.as_str()).unwrap_or(&ammo).to_string();
                let g = match groups.iter().position(|x| *x == group) {
                    Some(g) => g,
                    None => {
                        groups.push(group);
                        groups.len() - 1
                    }
                };
                let fire = store.define(plan.def.ordenes.get("fuego").and_then(|v| v.as_str()).unwrap_or(&format!("{id}.fuego")));
                store.claim(fire, Writer::World)?;
                weapons.push(Weapon {
                    machine: m,
                    group: g as u8,
                    load,
                    ammo,
                    speed: num(kind, m, "velocidad", 1000.0) as f32,
                    reach: num(kind, m, "alcance", 3000.0) as f32,
                    fire,
                    rounds: store.define(&format!("{id}.municion")),
                    ready: store.define(&format!("{id}.lista")),
                });
            }
        }
        if sensors.is_empty() && weapons.is_empty() {
            return Ok(None);
        }
        // (the decoys are let go by their own button, not by the trigger: their groups come
        // after the ones the selector counts)
        let mut order: Vec<usize> = (0..groups.len()).collect();
        order.sort_by_key(|&g| weapons.iter().any(|w| usize::from(w.group) == g && w.load == Load::Decoy));
        for w in &mut weapons {
            w.group = order.iter().position(|&g| g == usize::from(w.group)).unwrap_or(0) as u8;
        }
        let groups: Vec<String> = order.iter().map(|&g| groups[g].clone()).collect();
        let mut world = |name: &str, unit: &str| -> Result<SignalId, String> {
            let id = store.define_unit(name, unit, 0.0).map_err(|e| e.0)?;
            store.claim(id, Writer::World)?;
            Ok(id)
        };
        let out = Outputs {
            count: world("tac.contactos", "")?,
            selected: world("tac.elegido", "")?,
            locked: world("tac.fijado", "")?,
            range: world("tac.obj.dist", "km")?,
            closure: world("tac.obj.cierre", "m/s")?,
            az: world("tac.obj.az", "°")?,
            el: world("tac.obj.el", "°")?,
            iff: world("tac.obj.iff", "")?,
            speed: world("tac.obj.vel", "m/s")?,
            flight: world("tac.obj.vuelo", "s")?,
            error: world("tac.obj.error", "°")?,
            on_aim: world("tac.en_tiro", "")?,
            threats: world("rwr.amenazas", "")?,
            locked_on: world("rwr.fijado", "")?,
            missile: world("rwr.misil", "")?,
            threat_az: world("rwr.az", "°")?,
            group: world("armas.grupo", "")?,
            rounds: world("armas.municion", "")?,
            ready: world("armas.listas", "")?,
            firing: world("armas.disparando", "")?,
            decoys: world("cm.quedan", "")?,
            emitting: world("tac.emite", "")?,
        };
        let cmd = Commands {
            next: store.define("tac.siguiente"),
            prev: store.define("tac.anterior"),
            nearest: store.define("tac.cercano"),
            boresight: store.define("tac.morro"),
            lock: store.define("tac.fijar"),
            drop: store.define("tac.soltar"),
            hostile: store.define("tac.hostil"),
            master: store.define("armas.maestro"),
            select: store.define("armas.seleccion"),
            trigger: store.define("armas.gatillo"),
            auto: store.define("armas.auto"),
            decoy: store.define("cm.soltar"),
            program: store.define("cm.programa"),
            code: store.define("iff.codigo"),
            was: [false; 8],
        };
        Ok(Some(Tactical {
            sensors,
            weapons,
            groups,
            cmd,
            out,
            sensed: Vec::new(),
            fresh: false,
            looking: false,
            tracks: [Track::default(); TRACKS],
            len: 0,
            chosen: None,
            locked: false,
            hostile: [0; 16],
            hostile_len: 0,
            emission: Emission::default(),
            fired: Vec::new(),
            lead: None,
            threat: None,
            turn: 0,
            pull: None,
            trigger_was: false,
            stood: None,
            program_t: 0.0,
            program_left: 0,
            drawn: -1.0,
        }))
    }

    /// The signals it reads (what its pilot commands it with, and what switches its machines).
    pub fn reads(&self) -> Vec<SignalId> {
        let c = &self.cmd;
        let mut out = vec![c.next, c.prev, c.nearest, c.boresight, c.lock, c.drop, c.hostile, c.master, c.select, c.trigger, c.auto, c.decoy, c.program, c.code];
        out.extend(self.sensors.iter().map(|s| s.on));
        out
    }

    /// The track chosen, if it is still held.
    pub fn target(&self) -> Option<&Track> {
        let id = self.chosen?;
        self.tracks[..self.len].iter().find(|t| t.id == id)
    }

    fn is_hostile(&self, id: u64) -> bool {
        self.hostile[..self.hostile_len].contains(&id)
    }

    fn call_hostile(&mut self, id: u64) {
        if self.is_hostile(id) {
            return;
        }
        if self.hostile_len == self.hostile.len() {
            self.hostile.copy_within(1.., 0);
            self.hostile_len -= 1;
        }
        self.hostile[self.hostile_len] = id;
        self.hostile_len += 1;
    }

    fn call_off(&mut self, id: u64) {
        if let Some(k) = self.hostile[..self.hostile_len].iter().position(|h| *h == id) {
            self.hostile.copy_within(k + 1..self.hostile_len, k);
            self.hostile_len -= 1;
        }
    }

    /// Where the part of machine `m` is and which way it looks (ship frame).
    fn pose(machines: &[MachineRt], s: &Structure, m: usize) -> Option<(Vec3, Vec3)> {
        let rt = &machines[m];
        let part = s.parts.get(rt.part? as usize).filter(|p| p.alive)?;
        Some((part.center, part.local.transform_vector3(rt.thrust_axis).normalize_or(Vec3::Z)))
    }

    /// What the sensors make of what the owner told: the tracks, the threats.
    fn scan(&mut self, store: &Store, machines: &[MachineRt], s: &Structure, t: f64) {
        let own_code = store.get(self.cmd.code).round().max(0.0) as u32;
        let beacon = self.sensors.iter().any(|x| x.what == Radio::Transponder && store.on(x.on));
        for k in 0..self.sensed.len() {
            let c = self.sensed[k];
            let r = c.pos.length().max(1.0);
            let dir = c.pos / r;
            let (mut by, mut threat, mut loud) = (0u8, 0u8, 0.0f32);
            for x in &self.sensors {
                if !store.on(x.on) {
                    continue;
                }
                let Some((_, axis)) = Self::pose(machines, s, x.machine) else { continue };
                match x.what {
                    Radio::Radar => {
                        let share = store.get(x.share) as f32;
                        if share > 0.0 && dir.dot(axis) >= x.cone {
                            // (noise in its face shortens the reach as power would lengthen it)
                            let reach = x.reach * (share * c.rcs.max(1e-4) / (1.0 + c.jam * 40.0)).powf(0.25);
                            if r <= reach {
                                by |= BY_RADAR;
                            }
                        }
                    }
                    Radio::Infrared => {
                        if dir.dot(axis) >= x.cone && r <= x.reach * (c.heat / 1.0e6).max(0.0).sqrt() {
                            by |= BY_HEAT;
                        }
                    }
                    Radio::Warner => {
                        let power = c.erp.max(c.jam);
                        if power > 0.0 {
                            let in_beam = c.jam > 0.0 || c.cone <= -1.0 || (-dir).dot(c.beam) >= c.cone;
                            let heard = power * if in_beam { 1.0 } else { SIDELOBE } / (r * r);
                            if heard >= 1.0 / (x.hears * x.hears) {
                                by |= BY_WARNER;
                                loud = loud.max(heard * x.hears * x.hears);
                                threat = threat.max(if c.locks && c.class == Class::Missile { 3 } else if c.locks { 2 } else { 1 });
                            }
                        }
                    }
                    _ => {}
                }
            }
            if beacon && c.code != 0 && r <= BEACON_REACH {
                by |= BY_BEACON;
            }
            // (a missile homing on the ship that is seen at all is known for what it is)
            if c.class == Class::Missile && c.locks && by & (BY_RADAR | BY_HEAT) != 0 {
                threat = 3;
            }
            if by == 0 {
                continue;
            }
            if threat >= 2 {
                self.call_hostile(c.id);
            }
            let iff = if self.is_hostile(c.id) {
                Iff::Hostile
            } else if c.code != 0 && c.code == own_code {
                Iff::Friend
            } else if c.code != 0 {
                Iff::Civil
            } else {
                Iff::Unknown
            };
            let track = Track { id: c.id, pos: c.pos, vel: c.vel, seen: t, by, iff, class: c.class, rcs: c.rcs, heat: c.heat, threat, loud };
            match self.tracks[..self.len].iter().position(|x| x.id == c.id) {
                Some(k) => self.tracks[k] = track,
                None if self.len < TRACKS => {
                    self.tracks[self.len] = track;
                    self.len += 1;
                }
                None => {
                    // full: it takes the place of the farthest one that is not chosen, if it is nearer
                    let far = (0..self.len).filter(|&k| Some(self.tracks[k].id) != self.chosen).max_by(|&a, &b| self.tracks[a].pos.length_squared().total_cmp(&self.tracks[b].pos.length_squared()));
                    if let Some(k) = far.filter(|&k| self.tracks[k].pos.length_squared() > c.pos.length_squared()) {
                        self.tracks[k] = track;
                    }
                }
            }
        }
        // a decoy blooming beside what the radar is locked on takes the lock (unless the
        // infrared holds the target too: two bands are not fooled by one decoy)
        if self.locked
            && let Some(tg) = self.target().copied()
            && tg.by & BY_HEAT == 0
        {
            let (r, dir) = (tg.pos.length().max(1.0), tg.pos.normalize_or(Vec3::Z));
            let thief = self.tracks[..self.len].iter().filter(|x| x.id != tg.id && x.seen == t && x.by & BY_RADAR != 0).find(|x| {
                let xr = x.pos.length().max(1.0);
                (xr - r).abs() < GATE.0 && (x.pos / xr).dot(dir) > GATE.1 && x.rcs > tg.rcs * GATE.2
            });
            if let Some(x) = thief {
                self.chosen = Some(x.id);
            }
        }
        // what nobody sees any more is dropped; a lock holds only what a radar holds
        let mut k = 0;
        while k < self.len {
            let tr = self.tracks[k];
            let held = t - tr.seen <= if Some(tr.id) == self.chosen && self.locked { COAST } else { STALE };
            if held {
                k += 1;
            } else {
                self.len -= 1;
                self.tracks[k] = self.tracks[self.len];
            }
        }
        if let Some(id) = self.chosen {
            match self.tracks[..self.len].iter().find(|x| x.id == id) {
                None => {
                    self.chosen = None;
                    self.locked = false;
                }
                // (seen now, and not by the radar: the lock is gone; not seen now, it coasts)
                Some(tr) if self.locked && tr.seen == t && tr.by & BY_RADAR == 0 => self.locked = false,
                _ => {}
            }
        }
        // nearest first: the order the pilot steps through
        self.tracks[..self.len].sort_unstable_by(|a, b| a.pos.length_squared().total_cmp(&b.pos.length_squared()));
    }

    /// The buttons: taken on the way down.
    fn commands(&mut self, store: &Store) {
        let now = [store.on(self.cmd.next), store.on(self.cmd.prev), store.on(self.cmd.nearest), store.on(self.cmd.boresight), store.on(self.cmd.lock), store.on(self.cmd.drop), store.on(self.cmd.hostile), store.on(self.cmd.decoy)];
        let was = std::mem::replace(&mut self.cmd.was, now);
        let pressed = |k: usize| now[k] && !was[k];
        let at = self.chosen.and_then(|id| self.tracks[..self.len].iter().position(|t| t.id == id));
        if self.len > 0 {
            if pressed(0) {
                self.choose(at.map_or(0, |k| (k + 1) % self.len));
            }
            if pressed(1) {
                self.choose(at.map_or(self.len - 1, |k| (k + self.len - 1) % self.len));
            }
            if pressed(2) {
                self.choose(0);
            }
            if pressed(3) {
                // what is nearest the nose
                let k = (0..self.len).max_by(|&a, &b| self.tracks[a].pos.normalize_or_zero().z.total_cmp(&self.tracks[b].pos.normalize_or_zero().z));
                if let Some(k) = k {
                    self.choose(k);
                }
            }
        }
        if pressed(4) {
            // a lock is the radar's: on what it holds now
            self.locked = !self.locked && self.target().is_some_and(|t| t.by & BY_RADAR != 0);
        }
        if pressed(5) {
            self.chosen = None;
            self.locked = false;
        }
        if pressed(6)
            && let Some(id) = self.chosen
        {
            if self.is_hostile(id) { self.call_off(id) } else { self.call_hostile(id) }
        }
        if pressed(7) {
            self.program_left = self.program_left.max(1);
        }
    }

    fn choose(&mut self, k: usize) {
        let id = self.tracks[k].id;
        if self.chosen != Some(id) {
            self.chosen = Some(id);
            self.locked = false;
        }
    }

    /// One tick: what its owner told (if new), the pilot's buttons, the solution on the track
    /// chosen, who may fire, what was let fly, what it radiates, and its pictures.
    pub fn step(&mut self, store: &mut Store, machines: &mut [MachineRt], s: &Structure, t: f64, dt: f64, plots: &mut Plots) {
        self.looking = self.sensors.iter().any(|x| !matches!(x.what, Radio::Jammer) && store.on(x.on));
        let scanned = std::mem::take(&mut self.fresh);
        // what it holds is in its own frame: as the ship turns and speeds up between two sweeps
        // (a quarter of a second: ten degrees of a fighter's turn) its tracks go the other way.
        // Without this whatever flies or aims by a track chases where it was
        if let Some((rot, vel)) = self.stood {
            let turned = s.rot.inverse() * rot;
            let gained = s.rot.inverse() * (s.vel - vel).as_vec3();
            for tr in &mut self.tracks[..self.len] {
                tr.pos = turned * tr.pos;
                tr.vel = turned * tr.vel - gained;
            }
        }
        self.stood = Some((s.rot, s.vel));
        if scanned {
            self.scan(store, machines, s, t);
        }
        if !self.looking && self.len > 0 {
            self.len = 0;
            self.chosen = None;
            self.locked = false;
        }
        self.commands(store);
        // between sweeps every track goes on as it was going
        if !scanned {
            for tr in &mut self.tracks[..self.len] {
                tr.pos += tr.vel * dt as f32;
            }
        }
        // ---- the weapons: the group chosen, its solution, who may fire ----
        let aimed = self.weapons.iter().filter(|w| w.load != Load::Decoy).map(|w| w.group).max().map_or(0, |g| usize::from(g) + 1);
        let group = if aimed == 0 { 0 } else { (store.get(self.cmd.select).round().max(0.0) as usize).min(aimed - 1) } as u8;
        let master = store.on(self.cmd.master);
        let target = self.target().copied();
        let gun = self.weapons.iter().find(|w| w.group == group && w.load == Load::Round).map(|w| (w.speed, w.reach));
        self.lead = None;
        let (mut flight, mut error, mut on_aim) = (0.0f32, 0.0f32, false);
        if let Some(tg) = target {
            let aim = match gun {
                // where it will be when the round gets there: |p + v t| = s t
                Some((speed, _)) => intercept(tg.pos, tg.vel, speed).map(|tt| (tt, (tg.pos + tg.vel * tt).normalize_or(Vec3::Z))),
                None => Some((0.0, tg.pos.normalize_or(Vec3::Z))),
            };
            if let Some((tt, dir)) = aim {
                self.lead = Some(dir);
                flight = tt;
                error = dir.dot(Vec3::Z).clamp(-1.0, 1.0).acos();
                on_aim = match gun {
                    Some((_, reach)) => error < ON_AIM && tt < MAX_FLIGHT && tg.pos.length() < reach,
                    None => self.locked && error < 0.5,
                };
            }
        }
        let trigger = master && (store.on(self.cmd.trigger) || (store.on(self.cmd.auto) && on_aim && target.is_some_and(|t| t.iff == Iff::Hostile)));
        // decoys: by hand, or by themselves while something is locked on the ship
        let program = store.get(self.cmd.program).round() as i32;
        let worst = self.tracks[..self.len].iter().map(|x| x.threat).max().unwrap_or(0);
        if program >= 1 && worst >= if program >= 2 { 2 } else { 3 } && self.program_left == 0 && t >= self.program_t {
            self.program_left = 2;
        }
        let mut drop_decoy = false;
        if self.program_left > 0 && t >= self.program_t {
            self.program_left -= 1;
            self.program_t = t + 0.35;
            drop_decoy = true;
        }
        let (mut rounds, mut ready, mut firing, mut decoys) = (0.0, 0u32, false, 0.0);
        let singles = self.weapons.iter().filter(|w| w.group == group && w.load == Load::Guided).count();
        // a pull of the trigger goes to one launcher of the group, and to no other while it is held
        if trigger && !self.trigger_was && singles > 0 {
            self.pull = Some((self.turn % singles, t + 0.25));
        }
        if self.pull.is_some_and(|(_, until)| t > until || !trigger) {
            self.pull = None;
        }
        self.trigger_was = trigger;
        let mut single = 0;
        for w in &self.weapons {
            let left = store.get(w.rounds);
            let fire = match w.load {
                Load::Decoy => {
                    decoys += left;
                    drop_decoy
                }
                _ if w.group != group => false,
                Load::Round => trigger,
                Load::Guided => {
                    single += 1;
                    self.pull.is_some_and(|(k, _)| k == single - 1)
                }
            };
            if w.load != Load::Decoy && w.group == group {
                rounds += left;
                ready += u32::from(store.on(w.ready));
            }
            store.set(w.fire, if fire { 1.0 } else { 0.0 });
        }
        // ---- what was let fly since the last tick ----
        for (k, w) in self.weapons.iter().enumerate() {
            let n = machines[w.machine].m.take_shots();
            if n > 0 {
                firing = true;
                self.fired.push(Fire { weapon: k as u16, count: n.min(u32::from(u16::MAX)) as u16, target: if self.locked || w.load == Load::Guided { self.chosen } else { None } });
                if w.load == Load::Guided {
                    self.turn = self.turn.wrapping_add(1);
                }
            }
        }
        // (a rail with nothing on it gives its turn to the next)
        if singles > 0 && !trigger {
            let mut k = 0;
            for w in self.weapons.iter().filter(|w| w.group == group && w.load == Load::Guided) {
                if k == self.turn % singles && store.get(w.rounds) < 1.0 {
                    self.turn = self.turn.wrapping_add(1);
                    break;
                }
                k += 1;
            }
        }
        // ---- what it radiates ----
        let mut e = Emission { code: 0, cone: -1.0, ..Emission::default() };
        for x in &self.sensors {
            if !store.on(x.on) {
                continue;
            }
            match x.what {
                Radio::Radar => {
                    let share = store.get(x.share) as f32;
                    if share > e.erp {
                        e.erp = share * (x.reach / 60_000.0).powi(2);
                        e.cone = x.cone;
                        e.beam = Self::pose(machines, s, x.machine).map_or(Vec3::Z, |p| p.1);
                    }
                }
                Radio::Jammer => e.jam = e.jam.max(store.get(x.share) as f32),
                Radio::Transponder => e.code = store.get(self.cmd.code).round().max(0.0) as u32,
                _ => {}
            }
        }
        e.lock = if self.locked && e.erp > 0.0 { self.chosen } else { None };
        e.heat = self.emission.heat;
        self.emission = e;
        // ---- signals ----
        let o = &self.out;
        store.set(o.count, self.len as f64);
        store.set(o.locked, if self.locked { 1.0 } else { 0.0 });
        store.set(o.emitting, if e.erp > 0.0 { 1.0 } else { 0.0 });
        match target {
            Some(tg) => {
                let r = tg.pos.length().max(1.0);
                let dir = tg.pos / r;
                store.set(o.selected, 1.0);
                store.set(o.range, f64::from(r));
                store.set(o.closure, f64::from(-tg.vel.dot(dir)));
                store.set(o.az, f64::from((-dir.x).atan2(dir.z)));
                store.set(o.el, f64::from(dir.y.clamp(-1.0, 1.0).asin()));
                store.set(o.iff, f64::from(tg.iff as u8));
                store.set(o.speed, f64::from(tg.vel.length()));
                store.set(o.flight, f64::from(flight));
                store.set(o.error, f64::from(error));
            }
            None => {
                for sig in [o.selected, o.range, o.closure, o.az, o.el, o.iff, o.speed, o.flight, o.error] {
                    store.set(sig, 0.0);
                }
            }
        }
        store.set(o.on_aim, if on_aim { 1.0 } else { 0.0 });
        let threats = self.tracks[..self.len].iter().filter(|x| x.threat > 0).count();
        store.set(o.threats, threats as f64);
        store.set(o.locked_on, if worst >= 2 { 1.0 } else { 0.0 });
        store.set(o.missile, if worst >= 3 { 1.0 } else { 0.0 });
        let loudest = self.tracks[..self.len].iter().filter(|x| x.threat > 0).max_by(|a, b| (a.threat, a.loud).partial_cmp(&(b.threat, b.loud)).unwrap_or(std::cmp::Ordering::Equal));
        self.threat = loudest.map(|x| x.pos.normalize_or(Vec3::Z));
        store.set(o.threat_az, self.threat.map_or(0.0, |d| f64::from((-d.x).atan2(d.z))));
        store.set(o.group, f64::from(group));
        store.set(o.rounds, rounds);
        store.set(o.ready, f64::from(ready));
        store.set(o.firing, if firing { 1.0 } else { 0.0 });
        store.set(o.decoys, decoys);
        // ---- its pictures: when it has looked, or ten times a second while aiming ----
        if scanned || (self.len > 0 && t - self.drawn >= 0.1) || (self.len == 0 && t - self.drawn >= 1.0) {
            self.drawn = t;
            self.draw(store, plots, group, on_aim, s.rot.inverse() * s.vel.as_vec3());
        }
    }

    /// `going`: how the ship moves over what is under it (its own frame, m/s).
    fn draw(&self, store: &Store, plots: &mut Plots, group: u8, on_aim: bool, going: Vec3) {
        const GRID: [u8; 3] = [40, 110, 80];
        const TEXT: [u8; 3] = [150, 230, 255];
        let color = |t: &Track| match (t.class == Class::Missile && t.threat >= 3, t.iff) {
            (true, _) | (_, Iff::Hostile) => [255, 60, 40],
            (_, Iff::Friend) => [90, 255, 120],
            (_, Iff::Civil) => [90, 200, 255],
            (_, Iff::Unknown) => [255, 190, 60],
        };
        let shape = |t: &Track| match (t.threat >= 3, t.iff) {
            (true, _) => Shape::Triangle,
            (_, Iff::Hostile) => Shape::Diamond,
            (_, Iff::Friend | Iff::Civil) => Shape::Circle,
            (_, Iff::Unknown) => Shape::Square,
        };
        // ---- the situation: the ship in the middle, its nose up, what it holds round it ----
        let radar = self.sensors.iter().find(|x| x.what == Radio::Radar);
        let scale = radar.map_or(20_000.0, |x| {
            let range = store.get(x.range) as f32;
            if range > 0.0 { range } else { x.reach }
        });
        let p = plots.open("tac.radar");
        p.clear();
        for k in 1..=3 {
            p.arc([0.0, 0.0], k as f32 / 3.0, 0.0, std::f32::consts::TAU, 12 + 4 * k, GRID, if k == 3 { 0.6 } else { 0.25 });
        }
        p.line([0.0, -0.06], [0.0, 0.06], TEXT, 0.8);
        p.line([-0.05, -0.02], [0.05, -0.02], TEXT, 0.8);
        if let Some(x) = radar {
            // its sector, bright while it radiates
            let (half, lit) = (x.cone.clamp(-1.0, 1.0).acos(), store.get(x.share) > 0.0);
            let c = if lit { [70, 220, 130] } else { GRID };
            p.line([0.0, 0.0], [-half.sin(), half.cos()], c, 0.35);
            p.line([0.0, 0.0], [half.sin(), half.cos()], c, 0.35);
        }
        for t in &self.tracks[..self.len] {
            let chosen = Some(t.id) == self.chosen;
            let flat = glam::Vec2::new(-t.pos.x, t.pos.z) / scale;
            // (known by bearing alone, or off the scale: on the rim)
            let only_bearing = t.by & (BY_RADAR | BY_HEAT | BY_BEACON) == 0;
            let at = if only_bearing || flat.length() > 1.0 { flat.normalize_or(glam::Vec2::Y) * if only_bearing { 0.96 } else { 1.0 } } else { flat };
            let v = glam::Vec2::new(-t.vel.x, t.vel.z);
            let tail = if only_bearing { glam::Vec2::ZERO } else { v.clamp_length_max(600.0) / 600.0 * 0.16 };
            let m = p.mark(at.to_array(), if only_bearing { Shape::Caret } else { shape(t) }, color(t), if chosen { 0.05 } else { 0.035 });
            m.boxed = chosen;
            m.tail = tail.to_array();
        }
        p.word([-0.98, 0.9], 0.075, TEXT, 0.0, format_args!("{:.0} km", scale / 1000.0));
        p.word([0.98, 0.9], 0.075, TEXT, 1.0, format_args!("{} TRZ", self.len));
        if let Some(t) = self.target() {
            p.word([-0.98, -0.97], 0.075, color(t), 0.0, format_args!("{:.1} km", t.pos.length() / 1000.0));
            p.word([0.98, -0.97], 0.075, if self.locked { [255, 60, 40] } else { TEXT }, 1.0, format_args!("{}", if self.locked { "FIJADO" } else { "ELEGIDO" }));
        }
        // ---- the warner: who radiates at the ship, by bearing; the louder the nearer the middle ----
        let p = plots.open("tac.rwr");
        p.clear();
        p.arc([0.0, 0.0], 1.0, 0.0, std::f32::consts::TAU, 24, GRID, 0.6);
        p.arc([0.0, 0.0], 0.5, 0.0, std::f32::consts::TAU, 16, GRID, 0.25);
        for k in 0..12 {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            p.line([a.sin() * 0.92, a.cos() * 0.92], [a.sin(), a.cos()], GRID, 0.5);
        }
        p.line([0.0, -0.06], [0.0, 0.06], TEXT, 0.8);
        p.line([-0.05, -0.02], [0.05, -0.02], TEXT, 0.8);
        let mut worst = 0;
        for t in self.tracks[..self.len].iter().filter(|t| t.threat > 0) {
            let flat = glam::Vec2::new(-t.pos.x, t.pos.z).normalize_or(glam::Vec2::Y);
            // (heard a hundred times over its floor: half way in)
            let r = (1.0 - (t.loud.max(1.0).log10() / 4.0)).clamp(0.25, 0.92);
            let (sh, c) = match t.threat {
                3 => (Shape::Triangle, [255, 60, 40]),
                2 => (Shape::Diamond, [255, 60, 40]),
                _ => (Shape::Square, [255, 190, 60]),
            };
            let m = p.mark((flat * r).to_array(), sh, c, 0.06);
            m.boxed = t.threat >= 2;
            worst = worst.max(t.threat);
        }
        match worst {
            3 => p.word([0.0, -0.99], 0.11, [255, 60, 40], 0.5, format_args!("MISIL")),
            2 => p.word([0.0, -0.99], 0.11, [255, 60, 40], 0.5, format_args!("FIJADO")),
            1 => p.word([0.0, -0.99], 0.09, [255, 190, 60], 0.5, format_args!("RADAR")),
            _ => {}
        }
        // ---- the sight: ahead of the nose, 30° to the rim; the track chosen, where to point ----
        let p = plots.open("tac.mira");
        p.clear();
        const SPAN: f32 = 0.5236;
        let fore = |d: Vec3| {
            let (az, el) = ((-d.x).atan2(d.z), d.y.clamp(-1.0, 1.0).asin());
            let v = glam::Vec2::new(az, el) / SPAN;
            // (behind, or off its field: on the rim, the way to turn)
            (if d.z <= 0.0 || v.length() > 1.0 { v.normalize_or(glam::Vec2::Y) } else { v }).to_array()
        };
        p.arc([0.0, 0.0], 1.0, 0.0, std::f32::consts::TAU, 24, GRID, 0.6);
        p.arc([0.0, 0.0], 1.0 / 3.0, 0.0, std::f32::consts::TAU, 12, GRID, 0.25);
        p.line([-0.12, 0.0], [-0.04, 0.0], TEXT, 0.9);
        p.line([0.04, 0.0], [0.12, 0.0], TEXT, 0.9);
        p.line([0.0, 0.04], [0.0, 0.12], TEXT, 0.9);
        // where the ship is going (a ring with wings: on the rim when it is not ahead), and how fast
        let speed = going.length();
        if speed > 1.0 {
            let at = fore(going / speed);
            p.mark(at, Shape::Circle, [90, 255, 120], 0.05);
            p.line([at[0] - 0.11, at[1]], [at[0] - 0.05, at[1]], [90, 255, 120], 0.9);
            p.line([at[0] + 0.05, at[1]], [at[0] + 0.11, at[1]], [90, 255, 120], 0.9);
        }
        p.word([0.0, 0.9], 0.08, [90, 255, 120], 0.5, format_args!("{speed:.0} m/s"));
        if let Some(t) = self.target() {
            let c = color(t);
            let m = p.mark(fore(t.pos.normalize_or(Vec3::Z)), shape(t), c, 0.06);
            m.boxed = true;
            if let Some(lead) = self.lead {
                // where to put the nose
                p.mark(fore(lead), Shape::Cross, if on_aim { [90, 255, 120] } else { TEXT }, 0.09);
            }
            let r = t.pos.length();
            p.word([-0.98, 0.9], 0.08, c, 0.0, format_args!("{:.2} km", r / 1000.0));
            p.word([0.98, 0.9], 0.08, TEXT, 1.0, format_args!("{:+.0} m/s", -t.vel.dot(t.pos / r.max(1.0))));
            p.word([-0.98, -0.97], 0.08, c, 0.0, format_args!("{}", match t.iff { Iff::Hostile => "HOSTIL", Iff::Friend => "AMIGO", Iff::Civil => "CIVIL", Iff::Unknown => "DESCON." }));
            if on_aim {
                p.word([0.0, -0.75], 0.12, [90, 255, 120], 0.5, format_args!("EN TIRO"));
            }
        }
        p.word([0.98, -0.97], 0.08, TEXT, 1.0, format_args!("{}", self.groups.get(usize::from(group)).map_or("", String::as_str)));
    }
}

/// When a round leaving at `speed` (relative to the ship) meets what is at `p` going at `v`
/// (both relative to the ship): the smallest t > 0 with |p + v t| = speed · t.
pub fn intercept(p: Vec3, v: Vec3, speed: f32) -> Option<f32> {
    let a = v.length_squared() - speed * speed;
    let b = 2.0 * p.dot(v);
    let c = p.length_squared();
    if a.abs() < 1e-6 {
        return (b < 0.0).then(|| -c / b);
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let (t0, t1) = ((-b - root) / (2.0 * a), (-b + root) / (2.0 * a));
    let t = if t0 > 0.0 && t1 > 0.0 { t0.min(t1) } else { t0.max(t1) };
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_meets_a_crossing_target_where_it_will_be() {
        // 1 km ahead, crossing at 100 m/s; rounds at 1000 m/s
        let (p, v) = (Vec3::new(0.0, 0.0, 1000.0), Vec3::new(100.0, 0.0, 0.0));
        let t = intercept(p, v, 1000.0).unwrap();
        let hit = p + v * t;
        assert!((hit.length() - 1000.0 * t).abs() < 0.05);
        assert!((t - 1.005).abs() < 0.01, "{t}");
        // running away faster than the round: never
        assert!(intercept(p, Vec3::new(0.0, 0.0, 1200.0), 1000.0).is_none());
    }
}
