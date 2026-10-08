//! One ship in the world: its kind's plan made alive. Its structure (a `core::structure`
//! Structure, by id) holds what is physical; this holds what works: signals, controls and
//! indicators, machines, actuators and joints, networks, compartments, the flight computer and
//! the black box. Every tick reads the structure (which parts are hurt or gone, how it moves) and
//! writes back (the pose of its moving parts, its thrust, the damage it does to itself).
use crate::{
    atmos::Atmos,
    blackbox::BlackBox,
    flight::Flight,
    kind::{PortPlan, ShipKind},
    panels::Panels,
};
use glam::{Affine3A, Quat, Vec3};
use lunar_core::structure::state::Structure;
use lunar_machines::{
    Cx, Env, Machine, PortIo,
    actuator::{Actuator, joint, linkage::JointView},
    machine::Build,
    models,
    net::{Net, Solver},
    wear::{Provenance, hash},
};
use lunar_signals::{DerivedSet, Eval, Quality, SignalId, Store, Writer};
use std::{ops::Range, sync::Arc};

/// Systems tick (s): one a step of the world (`lunar_play::game::STEP`), so a ship's systems and
/// the world it moves in keep the same clock, whatever the frames of whoever draws it.
pub const TICK: f64 = 1.0 / 60.0;
/// Ticks one call catches up at most: a slow frame runs the systems a little slow rather than
/// doing more work (which would make the next frame slower still).
pub const MAX_TICKS: f64 = 4.0;
/// Seconds between the ticks of a ship nobody is near (`Pace::Slow`) and of one far away
/// (`Pace::Asleep`); what happens in between is settled at once (`Ship::settle`).
pub const SLOW_EVERY: f64 = 0.5;
pub const ASLEEP_EVERY: f64 = 4.0;
/// A ship stays busy this long after something moved, leaked or pushed (s).
const BUSY: f64 = 6.0;

/// How much of a ship's systems run: every tick; one tick now and then with the rest settled at
/// once (stores drain and fill as they would, nothing moves); or the same more seldom. A ship
/// goes slow only while nothing in it is happening (`Ship::busy`), so nothing is lost: what it
/// would have done ticking, it does settling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Full,
    Slow,
    Asleep,
}
/// What a ship glows with in the infrared with nothing lit (W), and what every newton of
/// thrust adds (W/N): what an `irst` finds it by (`tactical`).
const HEAT_IDLE: f32 = 5.0e4;
const HEAT_PER_NEWTON: f32 = 2.0e3;
/// A closure's latches draw it in over the last stretch at this rate (1/s), no faster than this
/// (rad/s; a fifth of it in m/s for sliding leaves).
const LATCH_PULL: f64 = 4.0;
const LATCH_SPEED: f64 = 0.3;

/// What the world tells a ship each tick (ship frame, at its position).
#[derive(Clone, Copy, Debug)]
pub struct World {
    /// Gravity (ship frame, m/s²): zero in free fall... the real field, from its body.
    pub gravity: Vec3,
    /// Toward the sun (ship frame) and the irradiance there (W/m²; 0 in shadow).
    pub sun: Vec3,
    pub irradiance: f64,
    /// Ambient pressure (Pa) and the radiative sink temperature (K).
    pub pressure: f64,
    pub sink: f64,
    /// Height of its lowest point over the ground under it (m; huge in space), its speed and
    /// its climb rate (m/s) relative to the body.
    pub altitude: f64,
    pub speed: f64,
    pub climb: f64,
    /// How far its centre of mass is from the centre of the body whose ground is under it (m;
    /// 0 where there is none): what going round that body at its speed takes of its weight.
    pub around: f64,
    /// Where its nose points on the compass (degrees from the north of the ground under it
    /// toward east, 0..360), or `NO_HEADING` where there is no north to count from: past every
    /// body's reach, at a pole, or with its nose straight up or down.
    pub heading: f64,
}

/// The height a ship is told where there is no ground under it (m): past every body's reach.
pub const NO_GROUND: f64 = 1.0e9;
/// The heading a ship is told where there is no north.
pub const NO_HEADING: f64 = -1.0;
/// How much of a north there must be (the sine of the angle to the body's axis) and how level
/// the nose (the cosine of its rise) for a heading to mean anything.
const SOME_NORTH: f64 = 1e-3;
const SOME_LEVEL: f64 = 0.02;

impl World {
    /// What the world is to structure `s` where it is now, its lowest point at `low` (world):
    /// what pulls it there (in its own frame; nothing past every body's reach), its height
    /// over the ground under it (`NO_GROUND` where there is none), its speed, and its climb
    /// against that pull. The sun, the air and the cold are the caller's to add.
    pub fn at(s: &Structure, bodies: &lunar_core::body::BodyRegistry, low: glam::DVec3) -> World {
        let com = s.to_world(s.com);
        let here = bodies.field(com);
        let heading = here.ground.and_then(|g| {
            let b = bodies.get(g);
            let up = b.up(com);
            let (north, much) = b.north_at(up);
            let nose = (s.rot * Vec3::Z).as_dvec3();
            let flat = nose - up * nose.dot(up);
            (much > SOME_NORTH && flat.length() > SOME_LEVEL).then(|| flat.dot(north.cross(up)).atan2(flat.dot(north)).to_degrees().rem_euclid(360.0))
        });
        World {
            gravity: s.rot.inverse() * here.pull.as_vec3(),
            altitude: here.ground.map_or(NO_GROUND, |g| bodies.get(g).altitude(low).max(0.0)),
            speed: s.vel.length(),
            climb: here.up().map_or(0.0, |up| s.vel.dot(up)),
            around: here.ground.map_or(0.0, |g| (com - bodies.get(g).center).length()),
            heading: heading.unwrap_or(NO_HEADING),
            ..World::default()
        }
    }
}

impl Default for World {
    fn default() -> Self {
        World { gravity: Vec3::new(0.0, -1.62, 0.0), sun: Vec3::Y, irradiance: 1361.0, pressure: 0.0, sink: 230.0, altitude: 0.0, speed: 0.0, climb: 0.0, around: 0.0, heading: NO_HEADING }
    }
}

/// An explosion one of its machines set off (ship frame), to hand to the structures.
#[derive(Clone, Copy, Debug)]
pub struct Burst {
    pub at: Vec3,
    pub energy: f64,
}

pub struct MachineRt {
    pub m: Box<dyn Machine>,
    pub ports: Range<usize>,
    pub part: Option<u32>,
    pub thrust_axis: Vec3,
    pub provenance: Provenance,
    pub health: SignalId,
    pub working: bool,
    /// Its part was alive last tick (to notice it being destroyed).
    pub(crate) was_alive: bool,
}

pub struct Ship {
    pub kind: Arc<ShipKind>,
    pub structure: u64,
    /// What its pieces' provenance was drawn from (another copy of it made with the same has the
    /// same pieces: `sync`).
    pub seed: u64,
    pub store: Store,
    pub(crate) derived: DerivedSet,
    pub panels: Panels,
    pub machines: Vec<MachineRt>,
    pub actuators: Vec<Actuator>,
    act_ports: Vec<Range<usize>>,
    pub ports: Vec<PortIo>,
    pub nets: Vec<Net>,
    /// Per net: (edge, conduit part) and (edge, switch signal, switch part).
    conduits: Vec<Vec<(usize, u32)>>,
    switches: Vec<Vec<(usize, SignalId, Option<u32>)>>,
    node_signals: Vec<Vec<(u32, SignalId)>>,
    solver: Solver,
    pub joints: Vec<JointView>,
    /// Every part each joint carries (its own and its children's).
    carried: Vec<Vec<u32>>,
    joint_signals: Vec<(SignalId, SignalId)>,
    /// The sprung legs: each one's joint, its place among the structure's springs, and the
    /// signal of what it carries (N). Set on the structure the first time it is seen.
    legs: Vec<(usize, SignalId)>,
    legs_set: bool,
    /// Per joint: where it was when what is in its way was last looked at; where something
    /// stopped it (the place, the way it was going: it goes no further that way until that is
    /// looked at again) and when; and the signal that says it is stopped.
    q_seen: Vec<f64>,
    stops: Vec<Option<(f64, f64)>>,
    stops_at: f64,
    /// Per joint: when something last stopped it, and where it was when that was last told
    /// (told once per place, not each time it is found still there).
    stopped_t: Vec<f64>,
    stopped_at: Vec<Option<f64>>,
    stop_place: (glam::DVec3, glam::Quat),
    stop_signals: Vec<SignalId>,
    /// The bones that gave way when that was last told to the structure, and when what gives
    /// way was last looked at against the ground.
    giving_was: Vec<u16>,
    gave_at: f64,
    /// Its cargo as built (the parts lashed to its clamps, sorted): in the way of its mechanisms
    /// like anything loose is.
    lashed: Vec<u32>,
    pub poses: Vec<Affine3A>,
    pub atmos: Atmos,
    power: crate::power::Power,
    pub flight: Flight,
    /// Its autopilot (a ship that flies has one) and its tactical system (one that mounts
    /// sensors or weapons), and the pictures its systems draw for its screens.
    /// What it glows with in the infrared now (W): its engines' push and its own warmth.
    pub heat: f32,
    pub autopilot: Option<crate::autopilot::Autopilot>,
    pub tactical: Option<crate::tactical::Tactical>,
    pub plots: crate::plots::Plots,
    pub blackbox: BlackBox,
    /// What it tells whoever is working it, as it happens (what it is about, the words, 0 note,
    /// 1 caution, 2 warning): a mechanism that stops at something in its way, a magnet over
    /// cargo still in its clamp. Whoever shows it takes it from here (`std::mem::take`); it is
    /// in the black box too.
    pub said: Vec<(String, String, u8)>,
    /// Per clamp that grips (a magnet): the signal that says what is under it is still held by
    /// another clamp, and whether it was so.
    clamp_lashed: Vec<Option<(SignalId, bool)>>,
    /// Per clamp that grips: the signal that says it left something under it that would take
    /// it past its rated load (`<id>.sobrecarga`), and whether it did.
    clamp_over: Vec<Option<(SignalId, bool)>>,
    /// Per clamp: the signal of what it holds weighs (`<id>.masa`, kg), and what those were
    /// last worked out from (the structure's version and weighings, how many things are held).
    clamp_mass: Vec<SignalId>,
    clamp_weighed: (u64, u64, usize),
    /// What a signal says some of its parts hold (`kind::Holder::level`: a tank's machine):
    /// the signal (kg), what it said when the parts were last told (nothing is done while it
    /// says the same), and each part with its share of it. What they weigh follows it.
    levels: Vec<(SignalId, f32, Vec<(u32, f32)>)>,
    /// The structure's version when they were last told (a part put back holds nothing).
    levels_seen: u64,
    /// What it weighs, as signals any panel may show (`mass`).
    pub mass: crate::mass::Mass,
    alarms: Vec<(String, lunar_signals::Program, usize, bool)>,
    alarm_state: Vec<f64>,
    /// The signal that says whether its own gravity works (`GravityDef::senal`), if it has one.
    gravity_on: Option<SignalId>,
    eval: Eval,
    pub t: f64,
    acc: f64,
    /// The pace it was run at last (`run`): only what was run in full is the same in every copy
    /// of it step by step (what goes slow ticks when its own count says).
    pub pace: Pace,
    /// Something is happening in it until then (ship time): a joint moving, air rushing, thrust.
    busy_until: f64,
    /// Its lowest point (ship frame y), and the structure's version and pose it was taken from.
    keel: (f32, u64, u64),
    /// Explosions to hand over (taken by the owner).
    pub bursts: Vec<Burst>,
    /// Plates torn out by the pressure behind them (taken by the owner, for their debris): the
    /// part and the way it was blown (ship frame).
    pub torn: Vec<(u32, Vec3)>,
    /// Joint pose changed since the structure was last posed.
    posed: bool,
    /// Per closure: its order, how open, latched shut (signals), and whether it is latched.
    closure_signals: Vec<(SignalId, SignalId, SignalId)>,
    pub latched: Vec<bool>,
    /// The closure each joint belongs to.
    closure_of: Vec<Option<usize>>,
    /// Where it is: height over the ground, speed, climb rate, gravity (signals).
    s_alt: SignalId,
    s_heading: SignalId,
    s_speed: SignalId,
    s_climb: SignalId,
    s_g: SignalId,
    /// Circuit priorities: the ports on each breaker's circuit and the signal saying its priority.
    priorities: Vec<(usize, SignalId)>,
    /// Per clamp: its order to let go (`<id>.soltar`), whether it holds (`<id>.sujeta`), and its
    /// lashings on the structure (found the first time they are needed).
    clamp_signals: Vec<(SignalId, SignalId)>,
    clamp_lashings: Option<Vec<Vec<u32>>>,
    /// Per clamp: open (its order to let go is up), and the loose structures it holds.
    pub(crate) clamp_open: Vec<bool>,
    /// When a magnet that is on and holds nothing looks again for something to take (ship time).
    clamp_retry: Vec<f64>,
    pub clamp_held: Vec<Vec<u64>>,
    /// What its clamps ask of whoever owns the structures (taken by the owner).
    pub clamp_asks: Vec<crate::cargo::Ask>,
}

fn port_io(p: &PortPlan) -> PortIo {
    PortIo::on(p.net, p.node)
}

impl Ship {
    /// A ship of `kind` on structure `structure` (already placed), with `seed` for its pieces'
    /// provenance.
    pub fn new(kind: Arc<ShipKind>, structure: u64, seed: u64) -> Result<Ship, String> {
        let err = |e: String| format!("nave {}: {e}", kind.id);
        let mut store = Store::new();
        // signals the data starts with
        for (name, q) in &kind.def.senales {
            let id = store.define(name);
            let unit = q.unit_name().to_string();
            if !unit.is_empty() {
                store.define_unit(name, &unit, 0.0).map_err(|e| err(e.0))?;
            }
            store.set(id, q.si().map_err(|e| err(e.0))?);
        }
        // networks
        let mut nets = Vec::new();
        let mut conduits = Vec::new();
        let mut switches = Vec::new();
        let mut node_signals = Vec::new();
        for plan in &kind.nets {
            let mut n = Net::new(&plan.name, plan.medium, plan.nominal);
            n.leak_rate = plan.leak;
            n.nodes = plan.nodes.len() as u32;
            let mut c = Vec::new();
            let mut s = Vec::new();
            for e in &plan.edges {
                let k = n.edge(e.a, e.b, e.kind);
                n.edges[k].measure = e.measure;
                if let Some(p) = e.part
                    && e.kind == lunar_machines::EdgeKind::Conduit
                {
                    c.push((k, p));
                }
                if let Some(sig) = &e.signal {
                    s.push((k, store.define(sig), e.part));
                }
            }
            let (_, pot_unit) = plan.medium.units();
            let mut ns = Vec::new();
            for (name, &node) in &plan.names {
                let id = store.define_unit(&format!("{}.{name}", plan.name), pot_unit, 0.0).map_err(|e| err(e.0))?;
                store.claim(id, Writer::World).map_err(err)?;
                ns.push((node, id));
            }
            nets.push(n);
            conduits.push(c);
            switches.push(s);
            node_signals.push(ns);
        }
        // machines
        let mut ports = Vec::new();
        let mut machines = Vec::new();
        for (k, plan) in kind.machines.iter().enumerate() {
            let mut params = plan.def.params.clone();
            let prov = Provenance::new(plan.def.fabricante.as_deref().unwrap_or("Genérico"), &plan.def.modelo, hash(&plan.id, seed));
            // an engine's own fingerprint and seed
            params.entry("huella".to_string()).or_insert_with(|| serde_json::json!(prov.fingerprint));
            params.entry("semilla".to_string()).or_insert_with(|| serde_json::json!(hash(&plan.id, seed) % 1_000_000));
            let mut b = Build::new(&plan.id, &params, &plan.def.ordenes, &mut store, Writer::Machine(k as u32));
            let m = models::create(&plan.def.modelo, &mut b).map_err(err)?;
            let start = ports.len();
            for spec in m.ports() {
                match plan.ports.iter().find(|p| p.role == spec.role) {
                    Some(p) => {
                        if kind.nets[usize::from(p.net)].medium != spec.medium {
                            return Err(err(format!("{}: el puerto '{}' es {} pero la red {} es {}", plan.id, spec.role, spec.medium, kind.nets[usize::from(p.net)].name, kind.nets[usize::from(p.net)].medium)));
                        }
                        ports.push(port_io(p));
                    }
                    // unwired: a port on no network
                    None => ports.push(PortIo { net: u16::MAX, ..PortIo::default() }),
                }
            }
            for p in &plan.ports {
                if !m.ports().iter().any(|s| s.role == p.role) {
                    return Err(err(format!("{}: el modelo {} no tiene puerto '{}' (tiene: {})", plan.id, plan.def.modelo, p.role, m.ports().iter().map(|s| s.role).collect::<Vec<_>>().join(", "))));
                }
            }
            let health = store.define_unit(&format!("{}.salud", plan.id), "%", 1.0).map_err(|e| err(e.0))?;
            store.claim(health, Writer::World).map_err(err)?;
            machines.push(MachineRt { m, ports: start..ports.len(), part: plan.part, thrust_axis: plan.thrust, provenance: prov, health, working: true, was_alive: true });
        }
        // joints
        let mut joints = Vec::new();
        let mut joint_signals = Vec::new();
        for j in &kind.joints {
            joints.push(JointView { q: f64::from(j.init), qd: 0.0, lo: f64::from(j.lo), hi: f64::from(j.hi), hinge: j.hinge, axis: j.axis.as_dvec3().to_array(), pivot: j.pivot.as_dvec3().to_array(), inertia: 1.0, gravity: 0.0 });
            let pos = store.define_unit(&format!("{}.pos", j.id), "%", 0.0).map_err(|e| err(e.0))?;
            let q = store.define(&format!("{}.q", j.id));
            store.claim(pos, Writer::World).map_err(err)?;
            store.claim(q, Writer::World).map_err(err)?;
            joint_signals.push((pos, q));
        }
        let mut stop_signals = Vec::new();
        for j in &kind.joints {
            let sig = store.define(&format!("{}.bloqueada", j.id));
            store.claim(sig, Writer::World).map_err(err)?;
            stop_signals.push(sig);
        }
        let mut legs = Vec::new();
        for (k, j) in kind.joints.iter().enumerate().filter(|(_, j)| j.spring.is_some()) {
            let load = store.define_unit(&format!("{}.carga", j.id), "N", 0.0).map_err(|e| err(e.0))?;
            store.claim(load, Writer::World).map_err(err)?;
            legs.push((k, load));
        }
        // closures: order (theirs, or a hand's), how open, latched
        let mut closure_signals = Vec::new();
        let mut closure_of = vec![None; kind.joints.len()];
        for (c, plan) in kind.closures.iter().enumerate() {
            let order = store.define(plan.order.as_deref().unwrap_or(&format!("{}.mano", plan.id)));
            let open = store.define_unit(&format!("{}.abierta", plan.id), "%", 0.0).map_err(|e| err(e.0))?;
            let shut = store.define(&format!("{}.cerrada", plan.id));
            store.claim(open, Writer::World).map_err(err)?;
            store.claim(shut, Writer::World).map_err(err)?;
            closure_signals.push((order, open, shut));
            for &j in &plan.joints {
                closure_of[j] = Some(c);
            }
        }
        let latched: Vec<bool> = kind.closures.iter().map(|c| c.latch && c.joints.iter().all(|&j| kind.joints[j].init.abs() < 1e-6)).collect();
        // cargo clamps: the order to let go, and whether it holds
        let mut clamp_signals = Vec::new();
        for c in &kind.clamps {
            let order = store.define(&format!("{}.soltar", c.id));
            let holds = store.define(&format!("{}.sujeta", c.id));
            store.claim(holds, Writer::World).map_err(err)?;
            store.set(holds, 1.0);
            clamp_signals.push((order, holds));
        }
        let mut lashed: Vec<u32> = kind.clamps.iter().flat_map(|c| c.cargo.iter().copied()).collect();
        lashed.sort_unstable();
        lashed.dedup();
        let mut clamp_lashed = Vec::new();
        for c in &kind.clamps {
            clamp_lashed.push(match c.zone {
                Some(z) if z.mode == crate::cargo::Mode::Grip => {
                    let sig = store.define(&format!("{}.anclada", c.id));
                    store.claim(sig, Writer::World).map_err(err)?;
                    Some((sig, false))
                }
                _ => None,
            });
        }
        // ... that it left something for its rated load, and what each clamp holds weighs
        let mut clamp_over = Vec::new();
        let mut clamp_mass = Vec::new();
        for c in &kind.clamps {
            clamp_over.push(match c.zone {
                Some(z) if z.mode == crate::cargo::Mode::Grip => {
                    let sig = store.define(&format!("{}.sobrecarga", c.id));
                    store.claim(sig, Writer::World).map_err(err)?;
                    Some((sig, false))
                }
                _ => None,
            });
            let mass = store.define_unit(&format!("{}.masa", c.id), "kg", 0.0).map_err(|e| err(e.0))?;
            store.claim(mass, Writer::World).map_err(err)?;
            clamp_mass.push(mass);
        }
        // what its machines say its tanks hold, for what those weigh (`contents`)
        let mut levels = Vec::new();
        for h in &kind.holders {
            let Some(name) = &h.level else { continue };
            let sig = store.find(name).ok_or_else(|| err(format!("contenido de {}: no hay señal '{name}' que diga cuánto lleva", h.id)))?;
            levels.push((sig, f32::NAN, h.holds.iter().copied().zip(h.shares.iter().copied()).collect()));
        }
        // (and what it all weighs, as signals: after the machines, whose counts it reads)
        let mass = crate::mass::Mass::new(&kind, &mut store).map_err(err)?;
        let mut carried: Vec<Vec<u32>> = kind.joints.iter().map(|j| j.parts.clone()).collect();
        for k in 0..kind.joints.len() {
            // a joint carries its children's parts too
            let mut p = kind.joints[k].parent;
            let own = kind.joints[k].parts.clone();
            while let Some(up) = p {
                carried[up].extend_from_slice(&own);
                p = kind.joints[up].parent;
            }
        }
        // (in order: what is in the way of a joint is looked up among them)
        for c in &mut carried {
            c.sort_unstable();
            c.dedup();
        }
        // actuators
        let mut actuators = Vec::new();
        let mut act_ports = Vec::new();
        for (k, plan) in kind.actuators.iter().enumerate() {
            let mut a = Actuator::new(&plan.id, &plan.def, &mut store, Writer::Actuator(k as u32), seed).map_err(err)?;
            a.bind(plan.joint, &joints[plan.joint]);
            let start = ports.len();
            for spec in a.ports() {
                match plan.ports.iter().find(|p| p.role == spec.role) {
                    Some(p) => ports.push(port_io(p)),
                    None => ports.push(PortIo { net: u16::MAX, ..PortIo::default() }),
                }
            }
            act_ports.push(start..ports.len());
            actuators.push(a);
        }
        // compartments, flight, the names of the derived signals, then panels (their indicators
        // may read any of these), then the derived expressions (they may read the controls)
        let mut world = |name: &str, unit: &str| -> Result<SignalId, String> {
            let id = store.define_unit(name, unit, 0.0).map_err(|e| err(e.0))?;
            store.claim(id, Writer::World).map_err(err)?;
            Ok(id)
        };
        let (s_alt, s_speed, s_climb, s_g) = (world("nave.altura", "m")?, world("nave.vel", "m/s")?, world("nave.vs", "m/s")?, world("nave.g", "m/s2")?);
        // (degrees as a bare number, to set against a wheel's; -1 where there is no north)
        let s_heading = world("nave.rumbo", "")?;
        let mut atmos = Atmos::new(&kind, &mut store).map_err(err)?;
        let flight = Flight::new(&kind, &mut store).map_err(err)?;
        let autopilot = crate::autopilot::Autopilot::new(&kind, &mut store).map_err(err)?;
        let tactical = crate::tactical::Tactical::new(&kind, &mut store).map_err(err)?;
        // (its electrical balance: signals any panel may show)
        let mut power = crate::power::Power::new(&kind, &mut store).map_err(err)?;
        let mut defs: Vec<(String, String)> = kind.def.derivadas.iter().map(|(a, b)| (a.clone(), b.clone())).collect();
        for p in &kind.panels {
            defs.extend(p.def.derivadas.iter().map(|(a, b)| (a.clone(), b.clone())));
        }
        for (name, _) in &defs {
            store.define(name);
        }
        let mut panels = Panels::new(&kind, &mut store, &mut ports).map_err(err)?;
        let derived = DerivedSet::build(&mut store, &defs).map_err(|e| err(e.0))?;
        panels.finish(&kind, &store).map_err(err)?;
        atmos.bind(&kind, &mut machines, &ports).map_err(err)?;
        power.bind(&kind, &ports, machines.iter().map(|m| m.ports.clone()));
        let gravity_on = match &kind.def.gravedad.senal {
            Some(name) => Some(store.find(name).ok_or_else(|| err(format!("gravedad: no hay señal '{name}'")))?),
            None => None,
        };
        let mut alarms = Vec::new();
        let mut slots = 0;
        for (text, cond) in &kind.def.alarmas {
            let p = lunar_signals::compile_in(cond, &store).map_err(|e| err(format!("alarma '{text}': {}", e.0)))?;
            let n = p.slots;
            alarms.push((text.clone(), p, slots, false));
            slots += n;
        }
        let mut ship = Ship {
            poses: vec![Affine3A::IDENTITY; kind.joints.len()],
            kind,
            structure,
            seed,
            store,
            derived,
            panels,
            machines,
            actuators,
            act_ports,
            ports,
            nets,
            conduits,
            switches,
            node_signals,
            solver: Solver::default(),
            q_seen: joints.iter().map(|j| j.q).collect(),
            stops: vec![None; joints.len()],
            joints,
            carried,
            joint_signals,
            legs,
            legs_set: false,
            stops_at: 0.0,
            stopped_t: vec![f64::MIN; stop_signals.len()],
            stopped_at: vec![None; stop_signals.len()],
            stop_place: (glam::DVec3::ZERO, Quat::IDENTITY),
            stop_signals,
            giving_was: Vec::new(),
            gave_at: 0.0,
            lashed,
            atmos,
            power,
            flight,
            heat: HEAT_IDLE,
            autopilot,
            tactical,
            plots: crate::plots::Plots::default(),
            blackbox: BlackBox::new(2000),
            alarms,
            alarm_state: vec![0.0; slots],
            gravity_on,
            eval: Eval::default(),
            t: 0.0,
            acc: 0.0,
            pace: Pace::Full,
            busy_until: BUSY,
            keel: (0.0, u64::MAX, u64::MAX),
            bursts: Vec::new(),
            torn: Vec::new(),
            posed: true,
            closure_signals,
            latched,
            closure_of,
            s_alt,
            s_heading,
            s_speed,
            s_climb,
            s_g,
            priorities: Vec::new(),
            said: Vec::new(),
            clamp_lashed,
            clamp_over,
            clamp_mass,
            clamp_weighed: (u64::MAX, u64::MAX, usize::MAX),
            levels,
            levels_seen: u64::MAX,
            mass,
            clamp_open: vec![false; clamp_signals.len()],
            clamp_retry: vec![0.0; clamp_signals.len()],
            clamp_held: vec![Vec::new(); clamp_signals.len()],
            clamp_asks: Vec::new(),
            clamp_signals,
            clamp_lashings: None,
        };
        ship.priorities = ship.circuit_ports();
        ship.update_poses();
        Ok(ship)
    }

    /// The ports downstream of every breaker "brk.X" (as far as the next switch) whose priority
    /// signal `prio.X` exists.
    fn circuit_ports(&self) -> Vec<(usize, SignalId)> {
        let mut out = Vec::new();
        for (n, plan) in self.kind.nets.iter().enumerate() {
            for e in &plan.edges {
                let Some(circuit) = e.id.strip_prefix("brk.") else {
                    continue;
                };
                let Some(sig) = self.store.find(&format!("prio.{circuit}")) else {
                    continue;
                };
                // its nodes: from the breaker's far side along everything but switches
                let mut seen = vec![false; plan.nodes.len()];
                let mut stack = vec![e.b as usize];
                while let Some(v) = stack.pop() {
                    if std::mem::replace(&mut seen[v], true) {
                        continue;
                    }
                    for x in &plan.edges {
                        if x.kind == lunar_machines::EdgeKind::Switch {
                            continue;
                        }
                        let (a, b) = (x.a as usize, x.b as usize);
                        if a == v && !seen[b] {
                            stack.push(b);
                        } else if b == v && !seen[a] {
                            stack.push(a);
                        }
                    }
                }
                for (k, p) in self.ports.iter().enumerate() {
                    if usize::from(p.net) == n && seen.get(p.node as usize).copied().unwrap_or(false) {
                        out.push((k, sig));
                    }
                }
            }
        }
        out
    }

    /// Run the systems for `dt` s of world time (fixed ticks inside, `MAX_TICKS` at most).
    pub fn update(&mut self, s: &mut Structure, w: &World, dt: f64) {
        self.acc = (self.acc + dt).min(TICK * MAX_TICKS);
        while self.acc >= TICK {
            self.acc -= TICK;
            self.tick(s, w, TICK);
        }
        if self.posed {
            s.set_pose(&self.poses);
            self.posed = false;
        }
    }

    /// The same at a pace: in full, or one tick every so often with the time between settled.
    pub fn run(&mut self, s: &mut Structure, w: &World, dt: f64, pace: Pace) {
        self.pace = pace;
        let every = match pace {
            Pace::Full => return self.update(s, w, dt),
            Pace::Slow => SLOW_EVERY,
            Pace::Asleep => ASLEEP_EVERY,
        };
        self.acc += dt;
        if self.acc >= every {
            let gap = self.acc - TICK;
            self.acc = 0.0;
            self.settle(s, w, gap);
            self.tick(s, w, TICK);
            if self.posed {
                s.set_pose(&self.poses);
                self.posed = false;
            }
        }
    }

    /// Where the eyes of whoever sits in seat `i` are now (ship frame): a seat on something that
    /// moves (a platform that lowers it out of the hull) goes with it.
    pub fn seat_eyes(&self, i: usize) -> Vec3 {
        let seat = &self.kind.seats[i];
        let rest = Vec3::from_array(seat.def.ojos);
        match self.kind.joints.iter().position(|j| j.parts.contains(&seat.part)) {
            Some(j) => self.poses[j].transform_point3(rest),
            None => rest,
        }
    }

    /// Something is happening in it (a joint moving, air rushing somewhere, engines pushing, a
    /// part just lost): it wants every tick.
    pub fn busy(&self) -> bool {
        self.t < self.busy_until
    }

    /// Keep it busy from now (someone works its controls).
    pub fn touch(&mut self) {
        self.busy_until = self.t + BUSY;
    }

    /// Its lowest point (ship frame y) as it is posed now.
    pub fn keel(&mut self, s: &Structure) -> f32 {
        if self.keel.1 != s.version || self.keel.2 != s.stance {
            let y = s.parts.iter().filter(|p| p.alive && !p.ghost).map(|p| p.center.y - p.radius).fold(f32::MAX, f32::min);
            self.keel = (y, s.version, s.stance);
        }
        self.keel.0
    }

    /// Catch up a long stretch at once (a ship that slept): machines settle, joints stay.
    pub fn settle(&mut self, s: &mut Structure, w: &World, dt: f64) {
        let env = self.env(w);
        for m in &mut self.machines {
            let mut cx = Cx { signals: &mut self.store, env, health: 1.0, working: m.working, t: self.t, dt: TICK };
            m.m.settle(&mut cx, &mut self.ports[m.ports.clone()], dt);
        }
        self.t += dt;
        let _ = s;
    }

    fn env(&self, w: &World) -> Env {
        Env { pressure: w.pressure, sink_temp: w.sink, sun: 0.0, irradiance: w.irradiance, gravity: f64::from(w.gravity.length()) }
    }

    /// One systems tick.
    fn tick(&mut self, s: &mut Structure, w: &World, dt: f64) {
        self.t += dt;
        let kind = self.kind.clone();
        // ---- 1. what the structure says: health of every machine's part, conduits, switches ----
        for (mi, m) in self.machines.iter_mut().enumerate() {
            let (alive, working, health) = match m.part {
                Some(p) => {
                    let part = &s.parts[p as usize];
                    (part.alive, part.alive && part.working, if part.alive { 1.0 - part.damage() } else { 0.0 })
                }
                None => (true, true, 1.0),
            };
            if m.was_alive && !alive {
                // destroyed: what it held may blow
                let e = m.m.rupture();
                if e > 0.0 {
                    let at = m.part.map_or(Vec3::ZERO, |p| s.parts[p as usize].center);
                    self.bursts.push(Burst { at, energy: e });
                }
                self.blackbox.log(self.t, &format!("{} destruida", kind.machines[mi].id), 2);
            }
            m.was_alive = alive;
            m.working = working;
            self.store.set(m.health, f64::from(health));
            if !alive {
                self.store.set_quality(m.health, Quality::Failed);
            }
        }
        for (n, net) in self.nets.iter_mut().enumerate() {
            let leaks = net.medium.leaks();
            for &(e, p) in &self.conduits[n] {
                let part = &s.parts[p as usize];
                let h = if part.alive { 1.0 - part.damage() } else { 0.0 };
                let edge = &mut net.edges[e];
                // a conduit conducts until it is cut; hurt, a pipe leaks
                edge.alive = part.alive && h > 0.08;
                edge.leak = if leaks {
                    if !edge.alive {
                        1.0
                    } else if h < 0.7 {
                        f64::from(((0.7 - h) / 0.7).powi(2))
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
            }
            for &(e, sig, part) in &self.switches[n] {
                let edge = &mut net.edges[e];
                edge.closed = self.store.on(sig);
                edge.alive = part.is_none_or(|p| s.parts[p as usize].alive);
            }
        }
        self.store.set(self.s_alt, w.altitude);
        self.store.set(self.s_heading, w.heading);
        self.store.set(self.s_speed, w.speed);
        self.store.set(self.s_climb, w.climb);
        self.store.set(self.s_g, f64::from(w.gravity.length()));
        // the gravity it makes for whoever it carries: all of it while what it runs on is there,
        // coming and going over its time (`lunar_core::structure::weight`)
        if !Arc::ptr_eq(&s.rooms, &kind.rooms) {
            s.rooms = kind.rooms.clone();
        }
        let gd = &kind.def.gravedad;
        let want = if gd.g > 0.0 && self.gravity_on.is_none_or(|id| self.store.on(id)) { 1.0 } else { 0.0 };
        let step = if gd.tiempo > 0.0 { (dt as f32 / gd.tiempo).min(1.0) } else { 1.0 };
        s.gravity = lunar_core::structure::state::OwnGravity { g: gd.g, on: s.gravity.on + (want - s.gravity.on).clamp(-step, step) };
        // ---- 2. controls write their values; the flight computer its commands ----
        self.panels.write(&kind, s, &mut self.store, &self.ports);
        let body = Body { mass: f64::from(s.mass), inertia: s.inertia, spin: s.rot.inverse() * s.spin, vel: (s.rot.inverse() * s.vel.as_vec3()), gravity: w.gravity, around: w.around as f32 };
        // (what it knows of what is round it first: the autopilot flies on it, the computer on both)
        if let Some(tac) = &mut self.tactical {
            tac.step(&mut self.store, &mut self.machines, s, self.t, dt, &mut self.plots);
        }
        let demand = match &mut self.autopilot {
            Some(ap) => ap.fly(&mut self.store, &self.flight, &self.machines, s, &body, self.tactical.as_ref(), self.t),
            None => None,
        };
        self.flight.command(&kind, &mut self.store, &self.machines, s, &body, demand.as_ref());
        // ---- 3. plan, solve, step ----
        for p in &mut self.ports {
            p.clear_ask();
        }
        let env = self.env(w);
        let sun_dir = w.sun;
        for m in &mut self.machines {
            let mut e = env;
            if let Some(p) = m.part {
                // sunlight on its working face (+Y of its part)
                let n = s.parts[p as usize].local.transform_vector3(Vec3::Y).normalize_or_zero();
                e.sun = w.irradiance * f64::from(n.dot(sun_dir).max(0.0));
            }
            let health = m.part.map_or(1.0, |p| 1.0 - s.parts[p as usize].damage());
            let mut cx = Cx { signals: &mut self.store, env: e, health, working: m.working, t: self.t, dt };
            m.m.plan(&mut cx, &mut self.ports[m.ports.clone()]);
        }
        self.update_joint_loads(s, w);
        for (k, a) in self.actuators.iter_mut().enumerate() {
            let plan = &kind.actuators[k];
            let (working, health) = match plan.part {
                Some(p) => (s.parts[p as usize].alive && s.parts[p as usize].working, 1.0 - s.parts[p as usize].damage()),
                None => (true, 1.0),
            };
            let mut cx = Cx { signals: &mut self.store, env, health, working, t: self.t, dt };
            a.plan(&mut cx, &self.joints[plan.joint], &mut self.ports[self.act_ports[k].clone()]);
        }
        self.panels.plan(&kind, s, &mut self.ports);
        // circuit priorities: LOW and HIGH over each load's own
        for &(k, sig) in &self.priorities {
            let p = &mut self.ports[k];
            match self.store.get(sig).round() as i32 {
                0 if p.demand > 0.0 => p.priority = 20,
                2 if p.demand > 0.0 => p.priority = 255,
                _ => {}
            }
        }
        for (n, net) in self.nets.iter_mut().enumerate() {
            self.solver.solve(n as u16, net, &mut self.ports);
        }
        for m in &mut self.machines {
            let health = m.part.map_or(1.0, |p| 1.0 - s.parts[p as usize].damage());
            let mut cx = Cx { signals: &mut self.store, env, health, working: m.working, t: self.t, dt };
            m.m.step(&mut cx, &self.ports[m.ports.clone()]);
        }
        // what the electrical networks gave and took this tick, as the ship's balance
        self.power.step(&self.ports, &mut self.store, dt);
        // potentials at the named junctions
        for (n, net) in self.nets.iter().enumerate() {
            for &(node, sig) in &self.node_signals[n] {
                let isl = net.island.get(node as usize).copied().unwrap_or(u32::MAX);
                let v = if isl == u32::MAX { 0.0 } else { net.islands[isl as usize].potential };
                self.store.set(sig, v);
            }
        }
        // ---- 4. actuators move their joints ----
        self.move_joints(s, dt);
        // ---- 5. air, derived signals, indicators, controls ----
        self.atmos.step(&kind, s, &mut self.store, &mut self.machines, &self.ports, &self.nets, &self.joints, dt);
        for (text, level) in std::mem::take(&mut self.atmos.said) {
            self.blackbox.log(self.t, &text, level);
        }
        // what the air did to the parts: shocked machines, plates torn out
        for (p, hp) in std::mem::take(&mut self.atmos.harm) {
            let part = &mut s.parts[p as usize];
            if !part.alive {
                continue;
            }
            part.hp -= hp;
            if part.hp <= 0.0 {
                part.hp = 0.0;
                part.alive = false;
                part.working = false;
                s.version += 1;
                let out = self.atmos.vents.iter().filter(|v| v.part == Some(p)).map(|v| v.dir).next();
                let away = out.unwrap_or_else(|| (kind.centers[p as usize] - s.center).normalize_or(Vec3::Y));
                self.torn.push((p, away));
            } else {
                part.working = part.hp > part.max_hp * 0.25;
            }
        }
        self.derived.tick(&mut self.store, dt, self.t);
        self.panels.after(&kind, s, &mut self.store, &self.ports, &self.nets, dt, self.t, &mut self.blackbox);
        // what its tanks hold weighs what their machines say they hold: a store whose machine
        // says what it said costs one comparison; one that says another amount tells its parts
        // (the structure weighs them again a quantum at a time, by their own blocks alone)
        let (mut flowed, anew) = (false, self.levels_seen != s.version);
        self.levels_seen = s.version;
        for (sig, last, parts) in &mut self.levels {
            let kg = self.store.get(*sig) as f32;
            if kg != *last || anew {
                flowed |= kg != *last;
                *last = kg;
                for &(p, share) in parts.iter() {
                    s.fill(p, kg * share);
                }
            }
        }
        self.mass.step(&self.machines, s, &mut self.store, f64::from(w.gravity.length()), flowed);
        self.clamps(s);
        // ---- 6. back to the structure: thrust, self-harm, bursts ----
        let (mut force, mut torque) = (Vec3::ZERO, Vec3::ZERO);
        for m in &mut self.machines {
            let Some(p) = m.part else { continue };
            let part = &mut s.parts[p as usize];
            let f = m.m.thrust();
            if f > 0.0 && part.alive {
                let dir = part.local.transform_vector3(m.thrust_axis).normalize_or_zero();
                let fv = dir * f as f32;
                force += fv;
                torque += (part.center - s.com).cross(fv);
            }
            // momentum wheels turn the hull without pushing it
            let t = m.m.torque();
            if t != [0.0; 3] {
                torque += part.local.transform_vector3(Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32));
            }
            let harm = m.m.harm();
            if harm > 0.0 && part.alive {
                part.hp -= part.max_hp * harm * dt as f32;
                if part.hp <= 0.0 {
                    part.alive = false;
                    s.version += 1;
                }
            }
            let e = m.m.take_burst();
            if e > 0.0 {
                self.bursts.push(Burst { at: part.center, energy: e });
            }
        }
        // (what its engines push with is what they glow with, for whoever looks in the infrared)
        self.heat = HEAT_IDLE + force.length() * HEAT_PER_NEWTON;
        if let Some(tac) = &mut self.tactical {
            tac.emission.heat = self.heat;
        }
        // the jets of air leaving it push it back
        let (f, t) = self.atmos.kick(s.com);
        force += f;
        torque += t;
        s.force = s.rot * force;
        s.torque = s.rot * torque;
        // ---- 7. alarms into the black box ----
        let mut off = 0;
        for (text, prog, slot, on) in &mut self.alarms {
            let n = prog.slots;
            let v = self.eval.run(prog, &self.store, &mut self.alarm_state[*slot..*slot + n], dt, self.t) >= 0.5;
            if v && !*on {
                self.blackbox.log(self.t, text, 1);
            }
            *on = v;
            off += n;
        }
        let _ = off;
        // something going on: a joint moving, air rushing, thrust, something blown
        if self.posed || force != Vec3::ZERO || torque != Vec3::ZERO || !self.bursts.is_empty() || !self.torn.is_empty() || !self.atmos.bursts.is_empty() || self.atmos.transfers.active || self.atmos.rate.iter().any(|r| r.abs() > 2.0) {
            self.busy_until = self.t + BUSY;
        }
    }

    /// Inertia and gravity of every joint from the parts it carries, as posed now.
    fn update_joint_loads(&mut self, s: &Structure, w: &World) {
        let g = w.gravity;
        for k in 0..self.joints.len() {
            let axis = self.poses_axis(k, &self.joints[k]);
            let j = &mut self.joints[k];
            let pivot = self.poses[k].transform_point3(Vec3::from_array(j.pivot.map(|x| x as f32)));
            let (mut inertia, mut grav) = (0.0f64, 0.0f64);
            for &p in &self.carried[k] {
                let part = &s.parts[p as usize];
                if !part.alive {
                    continue;
                }
                let m = f64::from(part.mass);
                if j.hinge {
                    let r = part.center - pivot;
                    let perp = r - axis * r.dot(axis);
                    inertia += m * f64::from(perp.length_squared() + part.radius * part.radius * 0.4);
                    grav += m * f64::from(r.cross(g).dot(axis));
                } else {
                    inertia += m;
                    grav += m * f64::from(g.dot(axis));
                }
            }
            j.inertia = inertia.max(1.0);
            j.gravity = grav;
        }
    }

    /// A joint's axis as its parent poses it.
    fn poses_axis(&self, k: usize, j: &JointView) -> Vec3 {
        let a = Vec3::from_array(j.axis.map(|x| x as f32));
        match self.kind.joints[k].parent {
            Some(p) => self.poses[p].transform_vector3(a).normalize_or(a),
            None => a,
        }
    }

    /// Its sprung legs on the structure as its springs, if they are not yet (a copy just made).
    pub fn legs_on(&mut self, s: &mut Structure) {
        if !self.legs.is_empty() && (!self.legs_set || s.springs.len() != self.legs.len()) {
            let g = self.store.get(self.s_g) as f32;
            self.set_legs(s, g);
        }
    }

    /// The sprung legs as the structure's springs (`lunar_core::structure::state::Spring`): each
    /// where its parent joint has it now. Built for its load if its data says one, else for its
    /// share of what the ship weighs under `g` m/s² (where it is first set down).
    fn set_legs(&mut self, s: &mut Structure, g: f32) {
        let kind = self.kind.clone();
        if s.springs.len() != self.legs.len() {
            let shares = self.leg_shares(s);
            s.springs.clear();
            for (i, &(k, _)) in self.legs.iter().enumerate() {
                let plan = &kind.joints[k];
                let Some((_, d)) = &plan.spring else { continue };
                let share = s.mass * g.max(0.3) * shares[i];
                let load = d.carga.as_ref().and_then(|q| q.si().ok()).map_or(share, |v| v as f32).max(1.0);
                let mut sp = lunar_core::structure::state::Spring {
                    bone: k as u16 + 1,
                    foot: Vec3::ZERO,
                    axis: Vec3::Y,
                    stroke: plan.hi - plan.lo,
                    preload: load * d.precarga.unwrap_or(0.25),
                    end: load * d.fin.unwrap_or(3.0),
                    gas: 1.2,
                    damp: [0.0; 2],
                    grip: d.agarre.unwrap_or(0.9),
                    active: true,
                    x: self.joints[k].q as f32,
                    load: 0.0,
                };
                // its damper: by the swing of its load on it where it rests
                let (_, rate) = sp.force(sp.at_rest(load));
                let critical = 2.0 * (rate * load / g.max(0.3)).sqrt();
                let [goes_in, comes_out] = d.freno.unwrap_or([0.45, 1.1]);
                sp.damp = [critical * goes_in, critical * comes_out];
                s.springs.push(sp);
            }
        }
        // where each is now: its parent's pose, its parts there
        for (i, &(k, _)) in self.legs.iter().enumerate() {
            let plan = &kind.joints[k];
            let Some((foot, _)) = &plan.spring else { continue };
            let parent = plan.parent.map_or(Affine3A::IDENTITY, |p| self.poses[p]);
            let Some(sp) = s.springs.get_mut(i) else { continue };
            sp.foot = parent.transform_point3(*foot);
            sp.axis = parent.transform_vector3(plan.axis).normalize_or(Vec3::Y);
            sp.active = plan.parts.iter().any(|&p| s.parts[p as usize].alive);
        }
        self.legs_set = true;
    }

    /// Tells the structure which of its bones give way to the ground (a ramp's): they do not
    /// hold the hull up.
    /// (Shut, such a thing is part of the hull like any other: it gives only while it is open.)
    fn set_giving(&self, s: &mut Structure) {
        let mut giving = std::mem::take(&mut s.giving);
        giving.clear();
        giving.extend(self.kind.joints.iter().enumerate().filter(|(k, j)| j.gives && (self.joints[*k].q - f64::from(j.init.min(j.lo))).abs() > 1e-3).map(|(k, _)| k as u16 + 1));
        if giving != self.giving_was {
            // (what holds the hull up changed)
            s.stance += 1;
        }
        s.giving = giving;
    }

    /// Whether joint `c` is `j` or rides on it.
    fn rides(&self, c: usize, j: usize) -> bool {
        let mut at = Some(c);
        while let Some(x) = at {
            if x == j {
                return true;
            }
            at = self.kind.joints[x].parent;
        }
        false
    }

    /// What moving joint `k` from where it is to `q` does to the parts it carries (ship frame).
    fn joint_delta(&self, k: usize, q: f64) -> Affine3A {
        let j = &self.joints[k];
        let axis = Vec3::from_array(j.axis.map(|x| x as f32));
        let pivot = Vec3::from_array(j.pivot.map(|x| x as f32));
        let motion = |q: f32| if j.hinge { Affine3A::from_translation(pivot) * Affine3A::from_quat(Quat::from_axis_angle(axis, q)) * Affine3A::from_translation(-pivot) } else { Affine3A::from_translation(axis * q) };
        let parent = self.kind.joints[k].parent.map_or(Affine3A::IDENTITY, |p| self.poses[p]);
        parent * motion(q as f32) * motion(j.q as f32).inverse() * parent.inverse()
    }

    /// Stops its mechanisms at what is in their way (`lunar_core::structure::obstruct`): a
    /// joint that moved its parts into the ground, into another structure or into someone
    /// (`people`: spheres in the world) goes back to where it touches and stays there — a
    /// ramp comes down as far as the ground, a hoist as far as the load under it, a door does
    /// not shut on what is in its doorway — until it is sent the other way or what was there is
    /// gone. A joint that gives way (a ramp) is also lifted by the ground as the hull comes
    /// down on its legs. Called by whoever owns the structures, after `update`.
    pub fn stop_at_obstacles(&mut self, set: &mut lunar_core::structure::set::Structures, bodies: &lunar_core::body::BodyRegistry, people: &[(glam::DVec3, f32)]) {
        use lunar_core::structure::obstruct::{What, depth};
        /// How deep a part may be in what it touches (m): resting on it.
        const TOUCH: f32 = 0.012;
        /// How far a joint that gives way is moved back in one go (rad or m).
        const GIVE: f64 = 0.12;
        // a joint says it is stopped while something keeps stopping it
        for j in 0..self.joints.len() {
            if self.stopped_at[j].is_some() && self.t - self.stopped_t[j] > 0.8 {
                self.stopped_at[j] = None;
                self.store.set(self.stop_signals[j], 0.0);
            }
        }
        // (nothing of it that could be stopped moved, and nothing of it lies on the ground:
        // nothing to look at. Sprung legs and what follows another joint are not looked at: the
        // ground and that other joint have them where they are)
        let kind = self.kind.clone();
        let looked = |j: usize| kind.joints[j].spring.is_none() && kind.joints[j].follows.is_none() && !self.carried[j].is_empty();
        let moved = (0..self.joints.len()).any(|j| looked(j) && (self.joints[j].q - self.q_seen[j]).abs() > 1e-7);
        if !moved && !kind.joints.iter().any(|j| j.gives) {
            return;
        }
        let Some(k) = set.index_of(self.structure) else { return };
        self.set_giving(&mut set.list[k]);
        self.giving_was.clone_from(&set.list[k].giving);
        // the stops hold while the hull stays where it was (and for a moment: what stopped a
        // joint may have gone)
        let (pos, rot) = (set.list[k].pos, set.list[k].rot);
        // (within a couple of centimetres and half a degree of where it was: a hull that only
        // trembles — under fire, in a pile — is where it was)
        let still = pos.distance_squared(self.stop_place.0) < 4e-4 && rot.dot(self.stop_place.1).abs() > 0.999_99;
        if !still || self.t - self.stops_at > 0.5 {
            if self.stops.iter().any(Option::is_some) {
                self.stops.iter_mut().for_each(|s| *s = None);
            }
            self.stops_at = self.t;
            self.stop_place = (pos, rot);
        }
        // (the hull where it was and no joint moved: nothing came to anything)
        if still && !moved {
            return;
        }
        // the ground under it, where it is now (none past every body's reach: nothing of it
        // can lie on any)
        let centre = set.list[k].to_world(set.list[k].center);
        let ground = bodies.field(centre).ground.map(|g| bodies.get(g));
        let up = set.list[k].rot.inverse() * ground.map_or(glam::DVec3::ZERO, |b| b.up(pos)).as_vec3();
        let mut changed = false;
        // what gives way to the ground as the hull moves is looked at a few times a second (it
        // holds nothing up: it may be a moment late), and not at all high over the ground
        let settle = !still && !self.giving_was.is_empty() && self.t - self.gave_at >= 0.05;
        if !moved && !settle {
            return;
        }
        // (falling or tumbling: whatever its mechanisms meet now is a crash, not something to
        // stop at. They are looked at again once it is slow)
        if set.list[k].vel.length_squared() > 1.0 || set.list[k].spin.length_squared() > 0.25 {
            for j in 0..self.joints.len() {
                self.q_seen[j] = self.joints[j].q;
            }
            return;
        }
        // (one coarse look at the ground for the whole ship: high over it, nothing of it
        // touches it)
        let grounded = ground.is_some_and(|b| (centre - b.center).length() - b.radius - b.height_at(b.up(centre), 8.0) < f64::from(set.list[k].radius) + 4.0);
        let settle = settle && grounded;
        if settle {
            self.gave_at = self.t;
        }
        for j in 0..self.joints.len() {
            let plan = &kind.joints[j];
            if plan.spring.is_some() || plan.follows.is_some() || self.carried[j].is_empty() {
                continue;
            }
            let (q0, q1) = (self.q_seen[j], self.joints[j].q);
            let moved = (q1 - q0).abs() > 1e-7;
            // nothing moved it, and nothing moved under it
            // (it gives way only while it is open: shut, it is hull)
            let gives = plan.gives && self.giving_was.contains(&(j as u16 + 1));
            if !moved && !(gives && settle) {
                continue;
            }

            // someone standing on what lifts them is carried, not in its way
            let near: Vec<(glam::DVec3, f32)> = if moved {
                people
                    .iter()
                    .copied()
                    .filter(|&(c, _)| {
                        let local = set.list[k].to_local(c);
                        let was = self.joint_delta(j, q0).transform_point3(local);
                        let motion = local - was;
                        !(motion.length_squared() > 1e-10 && motion.normalize().dot(up) > 0.5)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            // (what it holds on a bone that moves with this joint moves with it)
            let bones: Vec<u16> = if moved && !set.list[k].loads.is_empty() { (0..self.joints.len()).filter(|&c| self.rides(c, j)).map(|c| c as u16 + 1).collect() } else { Vec::new() };
            // (a joint that did not move: only the ground came to it, with the hull)
            // (the ground under what it moves, looked at once for all the tries)
            let plane = if grounded { lunar_core::structure::obstruct::ground_plane(set, k, &self.carried[j], Affine3A::IDENTITY, bodies) } else { None };
            let what = if moved { What { ground: plane.is_some(), others: true, own: &self.lashed, plane } } else { What { ground: plane.is_some(), others: false, own: &[], plane } };
            let at = |sh: &Ship, q: f64| depth(set, k, &sh.carried[j], &bones, sh.joint_delta(j, q), bodies, what, if moved { &near } else { &[] });
            let d1 = at(self, q1);
            let mut q = q1;
            let mut stopped = false;
            if d1 > TOUCH {
                if moved && at(self, q0) < d1 - 1e-4 {
                    // it went into it: back to where it touches
                    let (mut clear, mut inside) = (q0, q1);
                    if at(self, q0) <= TOUCH {
                        for _ in 0..6 {
                            let mid = (clear + inside) * 0.5;
                            if at(self, mid) <= TOUCH { clear = mid } else { inside = mid }
                        }
                    }
                    q = clear;
                    stopped = true;
                } else if gives {
                    // the ground came up under it: it gives, the way that clears it
                    let lim = |v: f64| v.clamp(self.joints[j].lo, self.joints[j].hi);
                    let (a, b) = (lim(q1 - GIVE), lim(q1 + GIVE));
                    let back = if at(self, a) <= at(self, b) { a } else { b };
                    if at(self, back) < d1 {
                        let (mut clear, mut inside) = (back, q1);
                        for _ in 0..6 {
                            let mid = (clear + inside) * 0.5;
                            if at(self, mid) <= TOUCH { clear = mid } else { inside = mid }
                        }
                        q = clear;
                        stopped = true;
                    }
                }
            }
            if stopped {
                let way = (q1 - q).signum();
                let jv = &mut self.joints[j];
                jv.q = q;
                jv.qd = 0.0;
                if !gives {
                    if self.stopped_at[j].is_none_or(|at| (at - q).abs() > 0.05) {
                        let text = format!("{}: se detiene, hay algo en su camino", plan.name);
                        self.blackbox.log(self.t, &text, 0);
                        self.said.push((plan.id.clone(), text, 1));
                    }
                    self.stopped_at[j] = Some(q);
                    self.stopped_t[j] = self.t;
                    self.store.set(self.stop_signals[j], 1.0);
                }
                self.stops[j] = Some((q, way));
                changed = true;
                self.busy_until = self.busy_until.max(self.t + 0.2);
            }
            self.q_seen[j] = self.joints[j].q;
            if stopped {
                // (its children ride it: where they are is from where it is now)
                self.follow();
                self.update_poses();
                self.posed = false;
                set.list[k].set_pose(&self.poses);
            }
        }
        if changed && !self.legs.is_empty() {
            let g = self.store.get(self.s_g) as f32;
            self.set_legs(&mut set.list[k], g);
        }
    }

    /// What share of the ship's weight each leg carries standing on level ground: by where its
    /// feet are round its centre of mass (the least uneven loads that hold it level). A leg is
    /// built for its share, so the hull sits level on legs all pressed in alike.
    fn leg_shares(&self, s: &Structure) -> Vec<f32> {
        let kind = &self.kind;
        let feet: Vec<Vec3> = self.legs.iter().map(|&(k, _)| kind.joints[k].spring.as_ref().map_or(Vec3::ZERO, |(f, _)| *f) - s.com).collect();
        let n = feet.len().max(1) as f32;
        // loads a + b·x + c·z with Σ = 1 and no moment about the centre of mass
        let (mut sx, mut sz, mut sxx, mut szz, mut sxz) = (0.0f32, 0.0, 0.0, 0.0, 0.0);
        for f in &feet {
            sx += f.x;
            sz += f.z;
            sxx += f.x * f.x;
            szz += f.z * f.z;
            sxz += f.x * f.z;
        }
        let m = glam::Mat3::from_cols(Vec3::new(n, sx, sz), Vec3::new(sx, sxx, sxz), Vec3::new(sz, sxz, szz));
        let even = vec![1.0 / n; feet.len()];
        if m.determinant().abs() < 1e-6 {
            return even;
        }
        let k = m.inverse() * Vec3::new(1.0, 0.0, 0.0);
        let shares: Vec<f32> = feet.iter().map(|f| k.x + k.y * f.x + k.z * f.z).collect();
        // (a centre of mass outside its feet: it will not stand anyway; no leg built for nothing)
        if shares.iter().any(|&v| v < 0.25 / n) { even } else { shares }
    }

    /// Set down on its legs as it will rest under `g` m/s² (each pressed in by its share of the
    /// weight): how far down that takes it (m), for whoever places it.
    pub fn rest_on_legs(&mut self, s: &mut Structure, g: f32) -> f32 {
        self.set_legs(s, g);
        if s.springs.is_empty() {
            return 0.0;
        }
        let shares = self.leg_shares(s);
        let mut sink = 0.0;
        for (i, &(k, _)) in self.legs.iter().enumerate() {
            let x = s.springs[i].at_rest(s.mass * g * shares[i]);
            s.springs[i].x = x;
            self.joints[k].q = f64::from(x);
            sink += x / self.legs.len() as f32;
        }
        self.update_poses();
        s.set_pose(&self.poses);
        self.posed = false;
        sink
    }

    fn move_joints(&mut self, s: &mut Structure, dt: f64) {
        let kind = self.kind.clone();
        let n = self.joints.len();
        // the sprung legs: as far in as the ground has them
        if !self.legs.is_empty() {
            self.legs_on(s);
            for (i, &(k, load)) in self.legs.iter().enumerate() {
                let sp = s.springs[i];
                self.store.set(load, f64::from(sp.load));
                let j = &mut self.joints[k];
                if (j.q - f64::from(sp.x)).abs() > 5e-4 {
                    j.q = f64::from(sp.x).clamp(j.lo, j.hi);
                    self.posed = true;
                }
                let (pos, q) = self.joint_signals[k];
                self.store.set(pos, (j.q - j.lo) / (j.hi - j.lo).max(1e-9));
                self.store.set(q, j.q);
            }
        }
        // each joint: the pushes of its actuators, integrated in small steps
        let steps = 4;
        let h = dt / steps as f64;
        let mut pushes: Vec<Option<lunar_machines::actuator::drive::Push>> = Vec::new();
        for k in 0..n {
            if kind.joints[k].spring.is_some() || kind.joints[k].follows.is_some() {
                continue;
            }
            let mine: Vec<usize> = (0..self.actuators.len()).filter(|&a| kind.actuators[a].joint == k).collect();
            let damping = f64::from(kind.joints[k].damping);
            let before = self.joints[k].q;
            let mut last = joint::Step::default();
            // a closure: latched shut it stays shut until ordered open; a hand works it if
            // nothing else does
            let closure = self.closure_of[k];
            let open_cmd = closure.is_some_and(|c| self.store.get(self.closure_signals[c].0) >= 0.5);
            if let Some(c) = closure {
                if self.latched[c] && open_cmd {
                    self.latched[c] = false;
                }
                if self.latched[c] {
                    let j = &mut self.joints[k];
                    j.q = 0.0;
                    j.qd = 0.0;
                    if before.abs() > 1e-7 {
                        self.posed = true;
                    }
                    self.closure_state(c);
                    let (pos, q) = self.joint_signals[k];
                    self.store.set(pos, 0.0);
                    self.store.set(q, 0.0);
                    continue;
                }
            }
            let hand = closure.is_some_and(|c| kind.closures[c].hand);
            for _ in 0..steps {
                pushes.clear();
                for &a in &mine {
                    let plan = &kind.actuators[a];
                    let (working, health) = match plan.part {
                        Some(p) => (s.parts[p as usize].alive && s.parts[p as usize].working, 1.0 - s.parts[p as usize].damage()),
                        None => (true, 1.0),
                    };
                    let mut cx = Cx { signals: &mut self.store, env: Env::default(), health, working, t: self.t, dt: h };
                    pushes.push(self.actuators[a].push(&mut cx, &self.joints[k], &self.ports[self.act_ports[a].clone()], h));
                }
                if hand {
                    // a hand pushes it toward where it is ordered, firmly
                    let j = &self.joints[k];
                    let target = if open_cmd { j.hi } else { j.lo };
                    let vmax = if j.hinge { 1.4 } else { 0.7 };
                    let v = ((target - j.q) * 4.0).clamp(-vmax, vmax);
                    let strength = j.inertia * 40.0 + j.gravity.abs() * 2.0;
                    pushes.push(Some(lunar_machines::actuator::drive::Push::Velocity { v, max: strength, hold: strength }));
                } else if mine.is_empty() {
                    // a free joint: gravity and its damping only
                    pushes.push(Some(lunar_machines::actuator::drive::Push::Force(0.0)));
                }
                // the last stretch: the latches take it from whatever drove it and draw it in,
                // slowing to nothing as it seats (no snap)
                if let Some(c) = closure {
                    let plan = &kind.closures[c];
                    let j = &self.joints[k];
                    if plan.latch && !open_cmd && j.q.abs() <= plan.capture {
                        let vmax = if j.hinge { LATCH_SPEED } else { LATCH_SPEED * 0.2 };
                        let v = (-j.q * LATCH_PULL).clamp(-vmax, vmax);
                        let strength = j.inertia * 80.0 + j.gravity.abs() * 3.0 + 1.0;
                        pushes.clear();
                        pushes.push(Some(lunar_machines::actuator::drive::Push::Velocity { v, max: strength, hold: strength }));
                    }
                }
                last = joint::integrate(&mut self.joints[k], &pushes, damping, 0.0, h);
                // what stopped it holds it there (it may go back the other way)
                if let Some((at, way)) = self.stops[k] {
                    let j = &mut self.joints[k];
                    if (j.q - at) * way > 0.0 {
                        j.q = at;
                        j.qd = 0.0;
                    } else if (j.q - at) * way < -1e-4 {
                        self.stops[k] = None;
                    }
                }
            }
            if last.hit && last.impact > 0.3 {
                self.blackbox.log(self.t, &format!("{}: golpe contra el tope", kind.joints[k].name), 0);
            }
            for &a in &mine {
                let plan = &kind.actuators[a];
                let (working, health) = match plan.part {
                    Some(p) => (s.parts[p as usize].alive && s.parts[p as usize].working, 1.0 - s.parts[p as usize].damage()),
                    None => (true, 1.0),
                };
                let mut cx = Cx { signals: &mut self.store, env: Env::default(), health, working, t: self.t, dt };
                self.actuators[a].after(&mut cx, &self.joints[k], last.applied, last.max);
            }
            // seated: the latches catch
            if let Some(c) = closure {
                let plan = &kind.closures[c];
                let j = &mut self.joints[k];
                let seated = if j.hinge { 2e-4 } else { 5e-5 };
                if plan.latch && !open_cmd && j.q.abs() <= seated {
                    j.q = 0.0;
                    j.qd = 0.0;
                    if plan.joints.iter().all(|&o| o == k || self.joints[o].q.abs() < 1e-6) {
                        self.latched[c] = true;
                        self.blackbox.log(self.t, &format!("{}: cerrada y trabada", plan.name), 0);
                    }
                }
                self.closure_state(c);
            }
            let j = &self.joints[k];
            let (pos, q) = self.joint_signals[k];
            self.store.set(pos, (j.q - j.lo) / (j.hi - j.lo).max(1e-9));
            self.store.set(q, j.q);
            if (j.q - before).abs() > 1e-7 {
                self.posed = true;
            }
        }
        self.follow();
        if self.posed {
            self.update_poses();
            // (a leg's parent moved: the leg is where that has it now)
            if !self.legs.is_empty() {
                let g = self.store.get(self.s_g) as f32;
                self.set_legs(s, g);
            }
        }
    }

    /// The joints that follow another (`JointPlan::follows`) go where that one is.
    fn follow(&mut self) {
        for k in 0..self.joints.len() {
            let Some((of, ratio)) = self.kind.joints[k].follows else { continue };
            let (q, qd) = (self.joints[of].q * f64::from(ratio), self.joints[of].qd * f64::from(ratio));
            let j = &mut self.joints[k];
            let q = q.clamp(j.lo, j.hi);
            if (j.q - q).abs() > 1e-7 {
                self.posed = true;
            }
            (j.q, j.qd) = (q, qd);
            let (s_pos, s_q) = self.joint_signals[k];
            self.store.set(s_pos, (q - j.lo) / (j.hi - j.lo).max(1e-9));
            self.store.set(s_q, q);
        }
    }

    /// A closure's signals from its leaves: how open (the most open leaf), latched.
    fn closure_state(&mut self, c: usize) {
        let plan = &self.kind.closures[c];
        let open = plan.joints.iter().map(|&j| (self.joints[j].q / self.joints[j].hi.max(1e-9)).clamp(0.0, 1.0)).fold(0.0, f64::max);
        let (_, s_open, s_shut) = self.closure_signals[c];
        self.store.set(s_open, open);
        self.store.set(s_shut, f64::from(u8::from(self.latched[c])));
    }

    /// The lashings of clamp `c` on `s`.
    fn lashings(&mut self, s: &Structure, c: usize) -> &[u32] {
        let kind = &self.kind;
        &self.clamp_lashings.get_or_insert_with(|| kind.clamps.iter().map(|p| crate::cargo::lashings(p, s)).collect())[c]
    }

    /// Cargo clamps: one whose order to let go goes up opens (its lashings part, what it holds
    /// is dropped); one whose order goes down shuts and takes what is loose in its zone; one
    /// that is destroyed drops what it held. Each says whether it holds anything.
    fn clamps(&mut self, s: &mut Structure) {
        for c in 0..self.kind.clamps.len() {
            let (order, holds) = self.clamp_signals[c];
            let open = self.store.get(order) >= 0.5;
            let there = self.kind.clamps[c].parts.iter().any(|&p| s.parts[p as usize].alive);
            let lashings = self.lashings(s, c);
            let mut holding = crate::cargo::holding(lashings, s);
            if open && holding {
                crate::cargo::release(lashings, s);
                holding = false;
            }
            if (open || !there) && !self.clamp_held[c].is_empty() {
                self.clamp_asks.push(crate::cargo::Ask::Drop(std::mem::take(&mut self.clamp_held[c])));
            }
            // (switched off, a magnet no longer says it left something for its rating)
            if let (true, Some((sig, true))) = (open || !there, self.clamp_over[c]) {
                self.store.set(sig, 0.0);
                self.clamp_over[c] = Some((sig, false));
            }
            if open != self.clamp_open[c] {
                self.clamp_open[c] = open;
                if !open && there && self.kind.clamps[c].zone.is_some() {
                    self.clamp_asks.push(crate::cargo::Ask::Take(c));
                    self.clamp_retry[c] = self.t + 0.4;
                }
            }
            // a magnet left on takes what comes under it later (set down on its load, say)
            if !open && there && !holding && self.clamp_held[c].is_empty() && self.t >= self.clamp_retry[c] && self.kind.clamps[c].zone.is_some_and(|z| z.mode == crate::cargo::Mode::Grip) {
                self.clamp_asks.push(crate::cargo::Ask::Take(c));
                self.clamp_retry[c] = self.t + 0.4;
                // ... and says so when what is under it is still in its own clamp
                let lashed = self.lashed_under(s, c);
                if let Some((sig, was)) = self.clamp_lashed[c] {
                    self.store.set(sig, f64::from(u8::from(lashed.is_some())));
                    if let (Some(d), false) = (lashed, was) {
                        let text = format!("{}: lo que tiene debajo sigue en su anclaje ({}): suéltalo primero", self.kind.clamps[c].name, self.kind.clamps[d].id.to_uppercase());
                        self.blackbox.log(self.t, &text, 1);
                        self.said.push((self.kind.clamps[c].id.clone(), text, 1));
                    }
                    self.clamp_lashed[c] = Some((sig, lashed.is_some()));
                }
            } else if let Some((sig, true)) = self.clamp_lashed[c] {
                if open || !there || holding || !self.clamp_held[c].is_empty() {
                    self.store.set(sig, 0.0);
                    self.clamp_lashed[c] = Some((sig, false));
                }
            }
            self.store.set(holds, f64::from(u8::from(holding || !self.clamp_held[c].is_empty())));
        }
        // what each holds weighs (kg): what it took (the ship's loads) and what is lashed to it
        // with what that holds. Worked out again only when something the ship weighs changed.
        let key = (s.version, s.weighings, self.clamp_held.iter().map(Vec::len).sum::<usize>());
        if key != self.clamp_weighed {
            self.clamp_weighed = key;
            for c in 0..self.kind.clamps.len() {
                let lashed = crate::cargo::holding(self.lashings(s, c), s);
                let taken: f32 = s.loads.iter().filter(|l| self.clamp_held[c].contains(&l.id)).map(|l| l.mass).sum();
                let own: f32 = if lashed { self.kind.clamps[c].cargo.iter().filter(|&&p| s.parts[p as usize].alive).map(|&p| s.parts[p as usize].mass + s.contents(p).map_or(0.0, |st| st.mass)).sum() } else { 0.0 };
                self.store.set(self.clamp_mass[c], f64::from(taken + own));
            }
        }
    }

    /// Clamp `c` (a magnet) left these under it (what each is, its kg): with them it would hold
    /// more than its rated load. It says so (`<id>.sobrecarga`, and once to whoever works it)
    /// until it leaves nothing or is switched off. Told by whoever serves its asks
    /// (`cargo::serve`).
    pub fn clamp_left(&mut self, c: usize, left: &[(String, f32)]) {
        let Some((sig, was)) = self.clamp_over.get(c).copied().flatten() else { return };
        let over = !left.is_empty();
        self.store.set(sig, f64::from(u8::from(over)));
        if over && !was {
            let plan = &self.kind.clamps[c];
            let rated = plan.zone.map_or(0.0, |z| z.rated.min(z.max));
            let what = left.iter().map(|(name, kg)| format!("{name} ({} kg)", lunar_core::structure::contents::figure(*kg))).collect::<Vec<_>>().join(", ");
            let text = format!("{}: deja {what}: pasaría de su carga nominal ({} kg)", plan.name, lunar_core::structure::contents::figure(rated));
            self.blackbox.log(self.t, &text, 1);
            self.said.push((plan.id.clone(), text, 1));
        }
        self.clamp_over[c] = Some((sig, over));
    }

    /// The clamp whose cargo, still lashed to it, is in the zone of clamp `c` (a magnet over a
    /// load nobody let go of): it will not take that.
    fn lashed_under(&mut self, s: &Structure, c: usize) -> Option<usize> {
        let zone = self.clamp_zone(s, c)?;
        // (a hand more than the zone: the plate is ON the load, the zone starts under it)
        let reach = zone.half + Vec3::splat(0.1);
        for d in 0..self.kind.clamps.len() {
            if d == c || !crate::cargo::holding(self.lashings(s, d), s) {
                continue;
            }
            let under = self.kind.clamps[d].cargo.iter().any(|&p| {
                let part = &s.parts[p as usize];
                part.alive && (part.center - zone.centre).abs().cmple(reach).all()
            });
            if under {
                return Some(d);
            }
        }
        None
    }

    /// Everything pushing it now: where (ship frame), which way it pushes the ship (unit) and how
    /// hard (N). What leaves each goes the other way.
    pub fn jets(&self, s: &Structure, out: &mut Vec<(Vec3, Vec3, f32)>) {
        out.clear();
        for m in &self.machines {
            let Some(p) = m.part else { continue };
            let f = m.m.thrust();
            let part = &s.parts[p as usize];
            if f > 0.0 && part.alive {
                out.push((part.center, part.local.transform_vector3(m.thrust_axis).normalize_or_zero(), f as f32));
            }
        }
    }

    pub fn clamp_of_part(&self, part: u32) -> Option<usize> {
        self.kind.clamps.iter().position(|c| c.parts.contains(&part))
    }

    /// Whether clamp `c` holds anything: cargo lashed to it as built, or something it took.
    pub fn clamp_holds(&mut self, s: &Structure, c: usize) -> bool {
        !self.clamp_held[c].is_empty() || crate::cargo::holding(self.lashings(s, c), s)
    }

    /// Clamp `c` worked by hand: holding, it opens (what it holds is let go); else it shuts on
    /// whatever is loose in its zone.
    pub fn work_clamp(&mut self, c: usize, holding: bool) {
        let order = self.clamp_signals[c].0;
        if holding {
            self.store.set(order, 1.0);
        } else {
            self.store.set(order, 0.0);
            self.clamp_open[c] = false;
            if self.kind.clamps[c].zone.is_some() {
                self.clamp_asks.push(crate::cargo::Ask::Take(c));
            }
        }
        self.touch();
    }

    /// Clamp `c` took these structures (its owner did, at its asking).
    pub fn clamp_took(&mut self, c: usize, ids: &[u64]) {
        self.clamp_held[c].extend_from_slice(ids);
        self.touch();
    }

    /// What its clamps held that is no longer theirs (destroyed, taken by another) is forgotten:
    /// `held` says whether a structure is still held by this ship.
    pub fn clamp_check(&mut self, held: &dyn Fn(u64) -> bool) {
        for list in &mut self.clamp_held {
            list.retain(|id| held(*id));
        }
    }

    /// Where clamp `c` takes what is loose, as it is posed now (ship frame): the centre and half
    /// extents of its zone and how it takes.
    pub fn clamp_zone(&self, s: &Structure, c: usize) -> Option<crate::cargo::Zone> {
        let plan = &self.kind.clamps[c];
        let z = plan.zone?;
        Some(match s.bones.get(usize::from(plan.bone)).filter(|_| plan.bone > 0) {
            Some(m) => crate::cargo::Zone { centre: m.transform_point3(z.centre), ..z },
            None => z,
        })
    }

    /// Order closure `id` open or shut by hand (its `<id>.mano`), if a hand works it.
    pub fn toggle_closure(&mut self, c: usize) -> bool {
        if !self.kind.closures[c].hand {
            return false;
        }
        let order = self.closure_signals[c].0;
        let v = self.store.get(order);
        self.store.set(order, if v >= 0.5 { 0.0 } else { 1.0 });
        true
    }

    /// The closure whose leaf is part `part`, if any.
    pub fn closure_of_part(&self, part: u32) -> Option<usize> {
        self.kind.closures.iter().position(|c| c.parts.contains(&part))
    }

    /// Bone transforms from the joints (parents first: the plan lists them in any order).
    pub(crate) fn update_poses(&mut self) {
        let kind = &self.kind;
        let n = self.joints.len();
        let mut done = vec![false; n];
        for _ in 0..n {
            for k in 0..n {
                if done[k] {
                    continue;
                }
                let parent = kind.joints[k].parent;
                if parent.is_some_and(|p| !done[p]) {
                    continue;
                }
                let j = &self.joints[k];
                let axis = Vec3::from_array(j.axis.map(|x| x as f32));
                let pivot = Vec3::from_array(j.pivot.map(|x| x as f32));
                let q = j.q as f32;
                let motion = if j.hinge { Affine3A::from_translation(pivot) * Affine3A::from_quat(Quat::from_axis_angle(axis, q)) * Affine3A::from_translation(-pivot) } else { Affine3A::from_translation(axis * q) };
                self.poses[k] = parent.map_or(Affine3A::IDENTITY, |p| self.poses[p]) * motion;
                done[k] = true;
            }
        }
        self.posed = true;
    }

    /// Every joint set straight, in the kind's order (a ship simulated elsewhere: its owner's
    /// figures over the network). Nothing is done when none of them moves.
    pub fn set_joints(&mut self, q: &[f32]) {
        let mut moved = false;
        for (j, q) in self.joints.iter_mut().zip(q) {
            let q = f64::from(*q).clamp(j.lo, j.hi);
            moved |= (j.q - q).abs() > 1e-5;
            j.q = q;
        }
        if moved {
            self.update_poses();
        }
    }

    /// What the joints were set to, put on the structure now (not on the next tick, which a
    /// ship far off runs seldom): whatever strikes it finds its parts where its owner has them.
    pub fn pose_now(&mut self, s: &mut Structure) {
        if self.posed {
            s.set_pose(&self.poses);
            self.posed = false;
        }
    }

    /// Set a joint straight (spawning with the ramp down, tests).
    pub fn set_joint(&mut self, id: &str, q: f64) {
        if let Some(k) = self.kind.joints.iter().position(|j| j.id == id) {
            let j = &mut self.joints[k];
            j.q = q.clamp(j.lo, j.hi);
            self.update_poses();
        }
    }

    pub fn signal(&self, name: &str) -> Option<f64> {
        self.store.find(name).map(|id| self.store.get(id))
    }

    /// Set a signal the data owns (initial states, tests). Machines' and controls' own signals
    /// are theirs: change those through their controls.
    pub fn set_signal(&mut self, name: &str, v: f64) {
        let id = self.store.define(name);
        self.store.set(id, v);
    }
}

/// What the flight computer knows of the hull it flies (ship frame).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub mass: f64,
    pub inertia: glam::Mat3,
    pub spin: Vec3,
    pub vel: Vec3,
    pub gravity: Vec3,
    /// How far it is from the centre of the body under it (m; 0: none).
    pub around: f32,
}
