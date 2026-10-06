//! What the ship weighs, the same for any ship: its mass with everything it holds and carries,
//! how much of that is propellant and how full its tanks are, the change of speed that
//! propellant is worth (the rocket equation, with its engines' exhaust velocity), how its
//! engines' push compares with its weight where it is, and how far off their line of push its
//! centre of mass lies. Nothing is weighed here: the structure weighs
//! (`lunar_core::structure::contents`), the tanks' machines count (`<id>.masa`), and this reads
//! both, only when one of them has changed: with nothing flowing and nothing moved it costs a
//! tick three comparisons.
//!
//! Signals (SI; each read in the unit it is shown in): `nave.masa` and `nave.masa_seca` (t: with
//! and without its propellant; what it carries is in both), `nave.propelente` (kg),
//! `nave.propelente_nivel` (0..1 of what its tanks take), `nave.dv` (m/s), `nave.empuje_peso`
//! (its engines at full over its weight where it is; `NO_WEIGHT` where nothing pulls),
//! `nave.aceleracion` (m/s², the same over its mass) and `nave.centrado` (cm). Shown by the
//! "masa" section of panels (`assets/defs/secciones.jsonc`, `sections`) and by the
//! multi-function displays' "masa" page (`mfd_auto`).
//!
//! Propellant is what a machine on a propellant network counts (`kind::Holder::level`); the
//! engines are the flight computer's (`vuelo.motores`), each by its rated thrust and specific
//! impulse in vacuum.
use crate::{
    kind::{ShipKind, resolve},
    ship::MachineRt,
};
use glam::Vec3;
use lunar_core::structure::state::Structure;
use lunar_machines::{machine::G0, net::Medium};
use lunar_signals::{Q, SignalId, Store, Writer};

/// What `nave.empuje_peso` says where nothing pulls on the ship (deep space): the most a
/// display of two decimals shows.
pub const NO_WEIGHT: f64 = 99.99;
/// The specific impulse (s) and the least throttle of an engine whose data says neither (the
/// model's own: `lunar_machines::models::engine`).
const ISP: f64 = 311.0;
const LEAST: f64 = 0.4;
/// Gravity is looked at again when it has changed by this share (a ship climbing: every few
/// hundred metres).
const G_STEP: f64 = 2e-3;

/// An engine: its machine, its rated thrust (N) and that at its least throttle.
#[derive(Clone, Copy, Debug)]
struct Engine {
    machine: usize,
    rated: f64,
    least: f64,
}

/// A ship's figures as they are now (`Mass::figures`), SI.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Figures {
    pub mass: f64,
    pub dry: f64,
    pub propellant: f64,
    /// What its tanks take (kg).
    pub capacity: f64,
    pub dv: f64,
    /// Its engines at full and at their least (N), those that are still there.
    pub thrust: f64,
    pub least: f64,
    /// How far from their line of push its centre of mass is (m).
    pub arm: f64,
    /// Exhaust velocity of its engines together (m/s): what `dv` is worked out with.
    pub ve: f64,
}

pub struct Mass {
    /// Its stores of propellant: the signal of each (kg) and the parts they are weighed in.
    tanks: Vec<SignalId>,
    parts: Vec<u32>,
    engines: Vec<Engine>,
    /// Exhaust velocity of its engines together (m/s): thrust over mass flow.
    ve: f64,
    out: [SignalId; 8],
    /// What the figures were last worked out from: the structure's weighings and gravity.
    seen: Option<u64>,
    g: f64,
    now: Figures,
}

impl Mass {
    /// Its signals, made before anything that reads them (panels, derived signals). After the
    /// machines: their `<id>.masa` are looked up here.
    pub fn new(kind: &ShipKind, store: &mut Store) -> Result<Mass, String> {
        let mut out = [SignalId::default(); 8];
        for (k, (name, unit)) in [("nave.masa", "t"), ("nave.masa_seca", "t"), ("nave.propelente", "kg"), ("nave.propelente_nivel", "%"), ("nave.dv", "m/s"), ("nave.empuje_peso", ""), ("nave.aceleracion", "m/s2"), ("nave.centrado", "cm")].into_iter().enumerate() {
            out[k] = store.define_unit(name, unit, 0.0).map_err(|e| e.0)?;
            store.claim(out[k], Writer::World)?;
        }
        // propellant: what a machine with a port on a propellant network counts
        let (mut tanks, mut parts) = (Vec::new(), Vec::new());
        for h in &kind.holders {
            let Some(level) = &h.level else { continue };
            let feeds = kind.machines.iter().find(|m| m.id == h.id).is_some_and(|m| m.ports.iter().any(|p| kind.nets[usize::from(p.net)].medium == Medium::Propelente));
            if let (true, Some(sig)) = (feeds, store.find(level)) {
                tanks.push(sig);
                parts.extend_from_slice(&h.holds);
            }
        }
        // the engines: the flight computer's
        let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
        let mut engines = Vec::new();
        let (mut thrust, mut flow) = (0.0, 0.0);
        for pat in kind.def.vuelo.iter().flat_map(|f| &f.motores) {
            for m in resolve(&ids, pat) {
                let p = &kind.machines[m as usize].def.params;
                let rated = f64::from(crate::exhaust::rated(&kind.machines[m as usize].def));
                let isp = p.get("isp_vacio").cloned().and_then(|v| serde_json::from_value::<Q>(v).ok()).and_then(|q| q.si_as("s").ok()).unwrap_or(ISP);
                let least = p.get("estrangulamiento").and_then(|v| v.get(0)).and_then(serde_json::Value::as_f64).unwrap_or(LEAST);
                thrust += rated;
                flow += rated / (isp.max(1.0) * G0);
                engines.push(Engine { machine: m as usize, rated, least: rated * least });
            }
        }
        let ve = if flow > 0.0 { thrust / flow } else { 0.0 };
        Ok(Mass { tanks, parts, engines, ve, out, seen: None, g: f64::NAN, now: Figures { ve, ..Figures::default() } })
    }

    /// The figures now. `flowed`: one of the stores said another amount this tick.
    pub fn step(&mut self, machines: &[MachineRt], s: &Structure, store: &mut Store, g: f64, flowed: bool) {
        let weighed = self.seen != Some(s.weighings);
        let pulled = !(g - self.g).abs().le(&(self.g.abs() * G_STEP));
        if !(flowed || weighed || pulled) {
            return;
        }
        if flowed || weighed {
            // what the tanks hold to the gram, on what the structure weighs without them
            let mut f = self.now;
            f.propellant = self.tanks.iter().map(|t| store.get(*t)).sum();
            if weighed {
                let (mut held, mut capacity) = (0.0, 0.0);
                for st in self.parts.iter().filter_map(|&p| s.contents(p)) {
                    held += f64::from(st.weighed);
                    capacity += f64::from(st.capacity);
                }
                (f.dry, f.capacity) = (f64::from(s.mass) - held, capacity);
                (f.thrust, f.least, f.arm) = self.push(machines, s);
                self.seen = Some(s.weighings);
            }
            f.mass = f.dry + f.propellant;
            f.dv = if f.dry > 0.0 && f.mass > f.dry { self.ve * (f.mass / f.dry).ln() } else { 0.0 };
            self.now = f;
        }
        self.g = g;
        let f = &self.now;
        let accel = if f.mass > 0.0 { f.thrust / f.mass } else { 0.0 };
        let ratio = if g > 1e-6 { (accel / g).min(NO_WEIGHT) } else { NO_WEIGHT };
        let level = if f.capacity > 0.0 { (f.propellant / f.capacity).clamp(0.0, 1.0) } else { 0.0 };
        for (sig, v) in self.out.iter().zip([f.mass, f.dry, f.propellant, level, f.dv, if f.thrust > 0.0 { ratio } else { 0.0 }, accel, f.arm]) {
            store.set(*sig, v);
        }
    }

    /// What its engines that are still there push with together at full and at their least (N),
    /// and how far from their line of push the centre of mass is (m), as they point now.
    fn push(&self, machines: &[MachineRt], s: &Structure) -> (f64, f64, f64) {
        let (mut full, mut least) = (0.0, 0.0);
        let (mut force, mut torque) = (Vec3::ZERO, Vec3::ZERO);
        for e in &self.engines {
            let m = &machines[e.machine];
            let Some(part) = m.part.map(|p| &s.parts[p as usize]).filter(|p| p.alive) else { continue };
            let f = part.local.transform_vector3(m.thrust_axis).normalize_or_zero() * e.rated as f32;
            force += f;
            torque += (part.center - s.com).cross(f);
            full += e.rated;
            least += e.least;
        }
        // (the part of their torque across their push: what trimming them has to take)
        let along = force.normalize_or_zero();
        let across = torque - along * torque.dot(along);
        (full, least, if force.length() > 1.0 { f64::from(across.length() / force.length()) } else { 0.0 })
    }

    pub fn figures(&self) -> Figures {
        self.now
    }
}
