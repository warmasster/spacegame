//! The air of every compartment (ported from the web model: src/shared/ship/modules/atmos.ts,
//! airflow.ts and decomp.ts). Moles of O₂, N₂ and CO₂ and a gas temperature per compartment;
//! pressure from the ideal gas law.
//!
//! Gas moves through every opening — hull parts holed or cracked, walls holed between
//! compartments, doors and ramps open, vent valves — with the compressible orifice equation:
//! choked at large pressure ratios, so a 1 m² breach empties a cabin in about a second while a
//! cracked plate hisses for minutes. The flows are sub-stepped and never overshoot; the gas left
//! behind expands and cools, what arrives mixes its temperature in. Ducts with fans mix the
//! compartments their vents feed, and the machines in them treat that air (scrubbers, injectors,
//! electrolysers); machines out of the ducts treat their own room. Compartments joined by big
//! openings are one group: a group with no way out to vacuum is sealed (`<c>.estanco`), what
//! make-up injectors wait for (`permiso`). The gas drifts back to the temperature the ship holds
//! it at, warmed by the machines in the room.
//!
//! What the moving air does: every opening with a place and a real pressure difference across it
//! is a `Vent` (mass flow, density, speed at its throat, thrust) — a sink in the room it empties
//! (the whole cross-section of a narrow room moves toward it), a jet past it (into the next room,
//! or a plume into vacuum that thins as it spreads), a kick on the ship. `push` is what it does to
//! a body anywhere, `accel` the acceleration of a body with a drag area.
//!
//! Explosive decompression: a fast drop (kPa per second) is a shock to the machines in the room —
//! more by the opening, by how fragile each is (`descompresion`), a little luck — announced once.
//! Hull plates and walls carry the pressure difference across them; a damaged one holds less
//! (rating × (hp/max)²) and, overloaded, tears in a few seconds (it creaks first: a message), then
//! goes and the room it held decompresses in turn.
use crate::{
    geom::Role,
    kind::{SPACE, ShipKind},
    ship::MachineRt,
    transfer::Transfers,
};
use glam::Vec3;
use lunar_core::structure::state::Structure;
use lunar_machines::{
    Net, PortIo,
    actuator::linkage::JointView,
    gas::{CD, M_CO2, M_N2, M_O2, psi},
    net::Medium,
};
use lunar_signals::{SignalId, Store, Writer};

// (the gas itself — its constants, the flow through an orifice — is the machines': theirs and the
// compartments' are one)
pub use lunar_machines::gas::{M_AIR, R, orifice_mdot};

/// Tuning of what the moving air does (1 = plain physics), as in the web model.
pub mod tune {
    /// Drag on bodies in the flow: plain physics only yanks what stands right at a breach, for a
    /// fraction of a second; this makes the crew feel it across the room.
    pub const DRAG: f64 = 9.0;
    /// Most the air can accelerate a body (m/s²).
    pub const MAX_ACCEL: f64 = 40.0;
    /// The first moments of an explosive decompression: the pull in the room it emptied is this
    /// many times stronger, fading over `BURST_S` seconds.
    pub const BURST: f64 = 2.5;
    pub const BURST_S: f64 = 1.2;
    /// Reaction of the jets on the ship.
    pub const THRUST: f64 = 0.6;
    /// Pressure difference (Pa) below which the air through an opening counts as still; the rush
    /// fades in over the next three times that.
    pub const STILL: f64 = 1500.0;
}

/// Drag area per mass (m²/kg) of a suited body standing, crouched.
pub const DRAG_STANDING: f64 = 0.9 / 180.0;
pub const DRAG_CROUCHED: f64 = 0.5 / 180.0;

/// Pressure drop rate (Pa/s) where a decompression starts to be violent, and where it is at its
/// worst; a violent drop of `ANNOUNCE` Pa is announced.
const DROP_RATE: [f64; 2] = [30e3, 150e3];
const ANNOUNCE: f64 = 12e3;
/// Machines near the opening take up to 1 + `NEAR` times the shock, fading over `NEAR_R` m.
const NEAR: f64 = 1.0;
const NEAR_R: f32 = 1.5;
/// Share of a plate's integrity an overloaded plate loses per second, per unit of overload (≤ 2).
const TEAR: f64 = 0.05;
/// Seconds the published shock takes to fade.
const FADE: f64 = 1.5;
/// The gas drifts to the temperature the ship holds it at, over this time (s); each machine's
/// heat (W) warms a room by heat / (`WARM` × volume) K over that.
const T_HOLD: f64 = 294.0;
const T_RELAX: f64 = 90.0;
const WARM: f64 = 8.0;
/// Make-up gas comes out of the bottles this cold (K).
const T_BOTTLE: f64 = 282.0;
/// Of a fragile machine (`descompresion` 1), the share of its hit points a full cabin's violent
/// decompression takes; machines say how fragile they are, this when they do not.
const FRAGILE: f32 = 0.15;

#[derive(Clone, Copy, Debug, Default)]
pub struct Air {
    pub o2: f64,
    pub n2: f64,
    pub co2: f64,
    /// K.
    pub t: f64,
}

/// O₂ partial pressure a crew breathes well (Pa): air at sea level.
pub const O2_BREATH: f64 = 21.2e3;

impl Air {
    pub fn moles(&self) -> f64 {
        self.o2 + self.n2 + self.co2
    }
    pub fn pressure(&self, volume: f64) -> f64 {
        self.moles() * R * self.t / volume.max(1e-3)
    }
    /// Mean molar mass (kg/mol).
    pub fn molar_mass(&self) -> f64 {
        let n = self.moles();
        if n <= 0.0 { M_AIR } else { (self.o2 * M_O2 + self.n2 * M_N2 + self.co2 * M_CO2) / n }
    }
    pub fn mass(&self) -> f64 {
        self.o2 * M_O2 + self.n2 * M_N2 + self.co2 * M_CO2
    }
    /// Standard cabin air at `p` Pa and 21 °C in `volume` m³.
    ///
    /// Breathable at any cabin pressure: O₂ at the partial pressure of air at sea level
    /// (`O2_BREATH`, the life support's set point), the rest nitrogen (pure O₂ below that).
    pub fn standard(p: f64, volume: f64) -> Air {
        let n = p * volume / (R * T_HOLD);
        let f = (O2_BREATH / p.max(1.0)).clamp(0.21, 1.0);
        Air { o2: n * f, n2: n * (1.0 - f), co2: 0.0, t: T_HOLD }
    }
    /// Take a share `k` (0..1) of it out.
    pub(crate) fn take(&mut self, k: f64) -> Air {
        let k = k.clamp(0.0, 1.0);
        let out = Air { o2: self.o2 * k, n2: self.n2 * k, co2: self.co2 * k, t: self.t };
        self.o2 -= out.o2;
        self.n2 -= out.n2;
        self.co2 -= out.co2;
        out
    }
    /// Mix `b` in (its temperature too).
    pub(crate) fn give(&mut self, b: &Air) {
        let (na, nb) = (self.moles(), b.moles());
        if nb <= 0.0 {
            return;
        }
        self.t = (self.t * na + b.t * nb) / (na + nb);
        self.o2 += b.o2;
        self.n2 += b.n2;
        self.co2 += b.co2;
    }
}

/// Gas through an orifice: mass flow (kg/s), the speed it leaves at (m/s, sonic when choked) and
/// the thrust of the jet on whatever holds the orifice (N: momentum plus the pressure left at the
/// throat).
#[derive(Clone, Copy, Debug, Default)]
pub struct Orifice {
    pub mdot: f64,
    pub speed: f64,
    pub thrust: f64,
}

/// Compressible orifice (γ = 1.4) of `area` m² from upstream `pu` Pa, `tu` K, molar mass `mu`
/// into `pd` Pa.
pub fn orifice(area: f64, pu: f64, tu: f64, mu: f64, pd: f64) -> Orifice {
    if pu <= 0.0 || area <= 0.0 {
        return Orifice::default();
    }
    let r = (pd / pu).max(0.0);
    let mdot = CD * area * pu * (mu / (R * tu)).sqrt() * psi(r);
    let re = r.max(0.5283);
    let speed = ((7.0 * R * tu / mu) * (1.0 - re.powf(0.2857))).max(0.0).sqrt();
    let thrust = mdot * speed + (re * pu - pd).max(0.0) * CD * area;
    Orifice { mdot, speed, thrust }
}

/// A way gas can go now: between compartments `a` and `b` (or vacuum, `SPACE`).
#[derive(Clone, Copy, Debug)]
pub struct Path {
    pub a: usize,
    pub b: usize,
    pub area: f64,
    /// Where it is (ship frame) and its normal from `a` toward `b`; none: a valve with no place.
    pub at: Option<Vec3>,
    pub n: Vec3,
    /// The part it goes through (a breach, a crack).
    pub part: Option<u32>,
    /// A valve, open on purpose (as far as its signal says): toward vacuum its room is no longer
    /// sealed, however little it is open.
    pub valve: bool,
}

/// A valve of a compartment (`kind::VentPlan`): wide open it is `area` m² toward `to`; its
/// signal says how open it is (a switch: 0 or 1; a wheel: anything between).
#[derive(Clone, Copy, Debug)]
struct Valve {
    room: usize,
    signal: SignalId,
    area: f64,
    to: usize,
    /// Where it lets the gas out, and the way from its room toward `to`.
    at: Option<(Vec3, Vec3)>,
}

/// Mass flow through the hand valves (kg/s) at which their hiss is as loud as it gets.
const HISS: f64 = 0.12;

/// Gas going through one opening right now.
#[derive(Clone, Copy, Debug)]
pub struct Vent {
    /// Compartment it comes from, where it goes (`SPACE`: out of the ship).
    pub up: usize,
    pub down: usize,
    /// Centre of the opening (ship frame) and the way the gas goes through it.
    pub at: Vec3,
    pub dir: Vec3,
    pub area: f64,
    /// Radius of a round hole of that area (m).
    pub r0: f64,
    /// Mass flow (kg/s), upstream density (kg/m³), speed at the throat (m/s).
    pub mdot: f64,
    pub rho: f64,
    pub speed: f64,
    /// Push of the jet on the ship, against `dir` (N).
    pub thrust: f64,
    pub part: Option<u32>,
    /// 0..1: how much of a rush it is (a trickle moves nothing and shows nothing).
    pub fade: f64,
}

struct Signals {
    p: SignalId,
    o2: SignalId,
    n2: SignalId,
    co2: SignalId,
    t: SignalId,
    leak: SignalId,
    out: SignalId,
    sealed: SignalId,
    shock: SignalId,
    /// What the weakest plate round it still holds (0..1, 0 a hole).
    integrity: SignalId,
}

pub struct Atmos {
    pub air: Vec<Air>,
    pub volume: Vec<f64>,
    /// Area open to vacuum of each compartment's group (m²), and whether the group is sealed.
    pub hole: Vec<f64>,
    pub sealed: Vec<bool>,
    /// How fast the pressure falls (Pa/s, smoothed), gas lost to vacuum (kg/s), how violent the
    /// decompression is right now (0..1), per compartment.
    pub rate: Vec<f64>,
    pub out: Vec<f64>,
    pub shock: Vec<f64>,
    /// People breathing in each compartment (set by the owner).
    pub crew: Vec<u32>,
    /// Every way gas can go now, and the openings with a rush through them (ship frame).
    pub paths: Vec<Path>,
    pub vents: Vec<Vent>,
    /// Explosive decompressions going on: (compartment, how violent, seconds left of the slug).
    pub bursts: Vec<(usize, f64, f64)>,
    /// Taken by the owner each tick: messages (text, level), harm to parts (part, hit points).
    pub said: Vec<(String, u8)>,
    pub harm: Vec<(u32, f32)>,
    sig: Vec<Signals>,
    /// Compartment of each machine (by its part's centre), and its air network port.
    machine_room: Vec<Option<usize>>,
    machine_air: Vec<Option<usize>>,
    /// Signal-driven openings.
    valves: Vec<Valve>,
    /// How loud the air through the valves with a place hisses (`sonido.silbido`, 0..1), if the
    /// ship has any, and what was last said.
    hiss: Option<(SignalId, f64)>,
    /// Gas moved by machines: compressors, tanks (`transfer`).
    pub transfers: Transfers,
    /// Centre and size of each compartment's boxes (ship frame).
    centre: Vec<Vec3>,
    size: Vec<Vec3>,
    /// Parts that carry a pressure difference: (part, one side, other side or SPACE, rating Pa).
    plates: Vec<(u32, usize, usize, f64)>,
    /// Machines' parts in each compartment and how fragile each is.
    inside: Vec<Vec<(u32, f32)>>,
    /// Plates tearing now (announced once each).
    yielding: Vec<u32>,
    last_p: Vec<f64>,
    lost: Vec<f64>,
    told: Vec<bool>,
    heat: Vec<f64>,
    root: Vec<usize>,
    out_kg: Vec<f64>,
    live: Vec<usize>,
    scratch: Vec<(usize, f64)>,
    seed: u64,
}

/// The compartment holding a ship-frame point.
pub fn room_of(kind: &ShipKind, p: Vec3) -> Option<usize> {
    kind.compartments.iter().position(|c| c.boxes.iter().any(|[lo, hi]| p.cmpge(*lo).all() && p.cmple(*hi).all()))
}

/// Pressure difference (Pa) a part holds intact, by what it is.
fn rating(role: Role) -> f64 {
    match role {
        Role::Hull | Role::Floor => 400e3,
        Role::Glass => 190e3,
        Role::Bulkhead => 160e3,
        _ => 250e3,
    }
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Atmos {
    pub fn new(kind: &ShipKind, store: &mut Store) -> Result<Atmos, String> {
        let mut sig = Vec::new();
        let mut air = Vec::new();
        let mut volume = Vec::new();
        let mut centre = Vec::new();
        let mut size = Vec::new();
        for c in &kind.compartments {
            let mut d = |field: &str, unit: &str| -> Result<SignalId, String> {
                let id = store.define_unit(&format!("{}.{field}", c.id), unit, 0.0).map_err(|e| e.0)?;
                store.claim(id, Writer::World)?;
                Ok(id)
            };
            sig.push(Signals {
                p: d("p", "kPa")?,
                o2: d("o2", "kPa")?,
                n2: d("n2", "kPa")?,
                co2: d("co2", "kPa")?,
                t: d("t", "°C")?,
                leak: d("fuga", "kPa/s")?,
                out: d("salida", "kg/s")?,
                sealed: d("estanco", "")?,
                shock: d("choque", "")?,
                integrity: d("integridad", "")?,
            });
            air.push(if c.pressure > 0.0 { Air::standard(c.pressure, f64::from(c.volume)) } else { Air { t: 250.0, ..Default::default() } });
            volume.push(f64::from(c.volume));
            let (lo, hi) = c.boxes.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(l, h), [a, z]| (l.min(*a), h.max(*z)));
            centre.push((lo + hi) * 0.5);
            size.push((hi - lo).max(Vec3::splat(0.1)));
        }
        let mut machine_room = Vec::new();
        let mut machine_air = Vec::new();
        let n = kind.compartments.len();
        let mut inside = vec![Vec::new(); n];
        for m in &kind.machines {
            let at = m.part.map_or(Vec3::splat(f32::MAX), |p| kind.centers[p as usize]);
            let room = room_of(kind, at);
            if let (Some(c), Some(p)) = (room, m.part) {
                inside[c].push((p, m.def.descompresion.unwrap_or(FRAGILE)));
            }
            machine_room.push(room);
            machine_air.push(None);
        }
        let mut valves = Vec::new();
        let mut plates = Vec::new();
        for (c, plan) in kind.compartments.iter().enumerate() {
            for v in &plan.vents {
                valves.push(Valve { room: c, signal: store.define(&v.signal), area: f64::from(v.area), to: v.to, at: v.at });
            }
            for &p in &plan.hull {
                plates.push((p, c, SPACE, rating(kind.roles[p as usize])));
            }
            for &(p, other) in &plan.walls {
                if other > c {
                    plates.push((p, c, other, rating(kind.roles[p as usize])));
                }
            }
        }
        let p0: Vec<f64> = air.iter().zip(&volume).map(|(a, v)| a.pressure(*v)).collect();
        // the hiss of the hand valves: a loop of the ship's own (`sonido.<id>`), where there are any
        let hiss = if valves.iter().any(|v| v.at.is_some()) {
            let id = store.define("sonido.silbido");
            store.claim(id, Writer::World)?;
            Some((id, 0.0))
        } else {
            None
        };
        Ok(Atmos {
            air,
            volume,
            hole: vec![0.0; n],
            sealed: vec![true; n],
            rate: vec![0.0; n],
            out: vec![0.0; n],
            shock: vec![0.0; n],
            crew: vec![0; n],
            paths: Vec::new(),
            vents: Vec::new(),
            bursts: Vec::new(),
            said: Vec::new(),
            harm: Vec::new(),
            sig,
            machine_room,
            machine_air,
            valves,
            hiss,
            transfers: Transfers::default(),
            centre,
            size,
            plates,
            inside,
            yielding: Vec::new(),
            last_p: p0,
            lost: vec![0.0; n],
            told: vec![false; n],
            heat: vec![0.0; n],
            root: vec![0; n],
            out_kg: vec![0.0; n],
            live: Vec::new(),
            scratch: Vec::new(),
            seed: 0x9e37_79b9_7f4a_7c15,
        })
    }

    /// Wire each machine to its air network port (after the ports exist), and find the ones that
    /// move or keep gas. An error: a compressor whose data names a place that is none.
    pub fn bind(&mut self, kind: &ShipKind, machines: &mut [MachineRt], ports: &[PortIo]) -> Result<(), String> {
        for (k, m) in machines.iter().enumerate() {
            self.machine_air[k] = m.ports.clone().find(|&i| ports[i].net != u16::MAX && kind.nets[usize::from(ports[i].net)].medium == Medium::Aire);
        }
        self.transfers = Transfers::bind(kind, machines);
        match self.transfers.unknown(machines).first() {
            Some(name) => Err(format!("'{name}' no es un compartimento ni un depósito de aire (lugares de un compresor)")),
            None => Ok(()),
        }
    }

    /// The openings gas is going through now (what an idle ship has none of).
    pub fn flowing(&self) -> usize {
        self.live.len()
    }

    /// Area of a hole a part leaves (m²): all of its broad face when gone, a crack when hurt.
    fn hole_of(part: &lunar_core::structure::state::Part) -> f64 {
        let face = f64::from(part.shape.area()) * 0.5;
        if !part.alive {
            face
        } else {
            let h = 1.0 - f64::from(part.damage());
            if h < 0.35 { face * 0.02 * (0.35 - h) / 0.35 } else { 0.0 }
        }
    }

    /// Pressure (Pa) of compartment `c`; vacuum outside.
    pub fn pressure(&self, c: usize) -> f64 {
        if c == SPACE { 0.0 } else { self.air[c].pressure(self.volume[c]) }
    }

    fn rnd(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.root[i] != i {
            self.root[i] = self.root[self.root[i]];
            i = self.root[i];
        }
        i
    }

    /// The way from compartment `a` toward `b` (or out of the ship) through a point.
    fn normal(&self, a: usize, b: usize, at: Vec3) -> Vec3 {
        let d = match (a == SPACE, b == SPACE) {
            (false, false) => self.centre[b] - self.centre[a],
            (false, true) => at - self.centre[a],
            (true, false) => self.centre[b] - at,
            (true, true) => Vec3::Y,
        };
        d.normalize_or(Vec3::Y)
    }

    /// Every way the gas can go now.
    fn gather(&mut self, kind: &ShipKind, s: &Structure, store: &Store, joints: &[JointView]) {
        self.paths.clear();
        for k in 0..self.plates.len() {
            let (p, a, b, _) = self.plates[k];
            let area = Self::hole_of(&s.parts[p as usize]);
            if area > 1e-7 {
                let at = kind.centers[p as usize];
                let n = self.normal(a, b, at);
                self.paths.push(Path { a, b, area, at: Some(at), n, part: Some(p), valve: false });
            }
        }
        for (k, j) in joints.iter().enumerate() {
            if let Some(o) = &kind.joints[k].opening {
                let area = ((j.q - j.lo) / (j.hi - j.lo).max(1e-9)).clamp(0.0, 1.0) * f64::from(o.area);
                if area > 1e-7 {
                    self.paths.push(Path { a: o.a, b: o.b, area, at: Some(o.at), n: o.n, part: None, valve: false });
                }
            }
        }
        // a valve is as open as its signal says: all of it (a switch), or a share (a wheel)
        for v in &self.valves {
            let open = store.get(v.signal);
            if open > 0.0 {
                self.paths.push(Path { a: v.room, b: v.to, area: v.area * open.min(1.0), at: v.at.map(|x| x.0), n: v.at.map_or(Vec3::Y, |x| x.1), part: None, valve: true });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, kind: &ShipKind, s: &Structure, store: &mut Store, machines: &mut [MachineRt], ports: &[PortIo], nets: &[Net], joints: &[JointView], dt: f64) {
        let n = self.air.len();
        if n == 0 {
            return;
        }
        let before: Vec<f64> = (0..n).map(|c| self.pressure(c)).collect();
        for b in &mut self.bursts {
            b.2 -= dt;
        }
        self.bursts.retain(|b| b.2 > 0.0);
        self.gather(kind, s, store, joints);
        // ---- groups joined by big openings, their way out to vacuum ----
        for i in 0..n {
            self.root[i] = i;
        }
        for k in 0..self.paths.len() {
            let g = self.paths[k];
            if g.b != SPACE && g.a != SPACE && g.area > 0.05 {
                let (ra, rb) = (self.find(g.a), self.find(g.b));
                self.root[ra] = rb;
            }
        }
        let mut vac = vec![0.0; n];
        for k in 0..self.paths.len() {
            let g = self.paths[k];
            let inner = if g.b == SPACE {
                g.a
            } else if g.a == SPACE {
                g.b
            } else {
                continue;
            };
            let r = self.find(inner);
            // (a valve open to vacuum counts as a way out whatever its size: nothing is made up
            // into a room that is being vented)
            vac[r] += if g.valve { g.area.max(0.01) } else { g.area };
        }
        for i in 0..n {
            let r = self.find(i);
            self.hole[i] = vac[r];
            self.sealed[i] = vac[r] < 0.01;
            store.set(self.sig[i].sealed, f64::from(u8::from(self.sealed[i])));
        }
        // ---- ventilation: each air network mixes the compartments its vents feed ----
        self.heat.fill(0.0);
        for (ni, net) in nets.iter().enumerate() {
            if net.medium != Medium::Aire {
                continue;
            }
            for isl in 0..net.islands.len() as u32 {
                // vents of this island: (compartment, flow m³/s)
                self.scratch.clear();
                let on_island = |pi: usize| usize::from(ports[pi].net) == ni && net.island.get(ports[pi].node as usize) == Some(&isl);
                for (k, m) in machines.iter().enumerate() {
                    let Some(pi) = self.machine_air[k].filter(|&pi| on_island(pi)) else {
                        continue;
                    };
                    let _ = pi;
                    let q = m.m.air();
                    // (a vent's damper shuts while its room is open to space — vented, breached:
                    // nothing is blown into a leak and the other rooms do not empty through the
                    // ducts. A room vents in the time its own valve takes: docs/TIEMPOS.md)
                    if let (Some(room), true) = (self.machine_room[k], q > 0.0)
                        && self.sealed[room]
                    {
                        self.scratch.push((room, q));
                    }
                }
                if self.scratch.is_empty() {
                    continue;
                }
                // the duct air: what each vent's room gives, by flow, mixed; treated; given back
                let mut duct = Air { t: T_HOLD, ..Default::default() };
                let mut total = 0.0;
                for &(room, q) in &self.scratch {
                    let share = (q * dt / self.volume[room]).min(0.5);
                    let got = self.air[room].take(share);
                    duct.give(&got);
                    total += q;
                }
                let mut warm = 0.0;
                for (k, m) in machines.iter().enumerate() {
                    if !self.machine_air[k].is_some_and(on_island) {
                        continue;
                    }
                    let tr = m.m.treat();
                    duct.co2 = (duct.co2 - tr.co2 * dt).max(0.0);
                    duct.give(&Air { o2: tr.o2 * dt, n2: tr.n2 * dt, co2: 0.0, t: T_BOTTLE });
                    warm += tr.heat;
                }
                for &(room, q) in &self.scratch {
                    let k = q / total;
                    let back = Air { o2: duct.o2 * k, n2: duct.n2 * k, co2: duct.co2 * k, t: duct.t };
                    self.air[room].give(&back);
                    self.heat[room] += warm * k;
                }
            }
        }
        // ---- machines out of the ducts treat their own room; heat; people breathe ----
        for (k, m) in machines.iter().enumerate() {
            let Some(room) = self.machine_room[k] else {
                continue;
            };
            if self.machine_air[k].is_none() {
                let tr = m.m.treat();
                let a = &mut self.air[room];
                a.co2 = (a.co2 - tr.co2 * dt).max(0.0);
                a.give(&Air { o2: tr.o2 * dt, n2: tr.n2 * dt, co2: 0.0, t: T_BOTTLE });
                self.heat[room] += tr.heat;
            }
            self.heat[room] += m.m.heat();
        }
        for c in 0..n {
            let a = &mut self.air[c];
            let people = f64::from(self.crew[c]);
            let use_o2 = (0.012 * people * dt).min(a.o2);
            a.o2 -= use_o2;
            a.co2 += use_o2 * 0.85;
        }
        // ---- what compressors move, what tanks leak ----
        if !self.transfers.is_empty() {
            self.transfers.step(&mut self.air, machines, dt);
        }
        // ---- flows through the openings, sub-stepped: a big breach empties a cabin in ~1 s ----
        self.out_kg.fill(0.0);
        self.live.clear();
        let mut fastest: f64 = 0.0;
        for (k, g) in self.paths.iter().enumerate() {
            let (pa, pb) = (self.pressure(g.a), self.pressure(g.b));
            let (up, pd) = if pa >= pb { (g.a, pb) } else { (g.b, pa) };
            if up == SPACE || (pa - pb).abs() < 1.0 {
                continue;
            }
            self.live.push(k);
            let a = &self.air[up];
            let nu = a.moles();
            if nu < 1e-9 {
                continue;
            }
            let rate = orifice_mdot(g.area, a.pressure(self.volume[up]), a.t.max(150.0), a.molar_mass(), pd) / a.molar_mass() / nu;
            fastest = fastest.max(rate);
        }
        let steps = ((fastest * dt / 0.18).ceil() as usize).clamp(1, 48);
        let h = dt / steps as f64;
        for _ in 0..steps {
            for i in 0..self.live.len() {
                let g = self.paths[self.live[i]];
                let (pa, pb) = (self.pressure(g.a), self.pressure(g.b));
                if (pa - pb).abs() <= 1.0 {
                    continue;
                }
                let (up, down) = if pa > pb { (g.a, g.b) } else { (g.b, g.a) };
                if up == SPACE {
                    continue;
                }
                let a = self.air[up];
                let nu = a.moles();
                if nu <= 1e-9 {
                    continue;
                }
                let mu = a.molar_mass();
                let mut dn = orifice_mdot(g.area, a.pressure(self.volume[up]), a.t.max(150.0), mu, pa.min(pb)) / mu * h;
                // never overshoot: at most half of what would equalise (or half the gas, into vacuum)
                if down == SPACE {
                    dn = dn.min(nu * 0.5);
                } else {
                    let eq = (pa - pb).abs() / (R * (a.t / self.volume[up] + self.air[down].t / self.volume[down]));
                    dn = dn.min(eq * 0.5);
                }
                let k = dn / nu;
                let moved = self.air[up].take(k);
                // the gas left behind expands and cools
                self.air[up].t = (a.t * (1.0 - k).powf(0.4)).max(120.0);
                if down == SPACE {
                    self.out_kg[up] += moved.mass();
                } else {
                    self.air[down].give(&moved);
                }
            }
        }
        // ---- temperature: back to what the ship holds it at, warmed by its machines; publish ----
        for c in 0..n {
            let a = &mut self.air[c];
            if a.moles() < 1e-6 {
                *a = Air { t: 250.0, ..Default::default() };
            } else {
                let target = T_HOLD + (self.heat[c] / (WARM * self.volume[c])).min(60.0);
                a.t += (target - a.t) * (dt / T_RELAX).min(1.0);
            }
            let a = self.air[c];
            let p = a.pressure(self.volume[c]);
            let fall = (self.last_p[c] - p) / dt.max(1e-6);
            self.rate[c] += (fall - self.rate[c]) * (dt * 4.0).min(1.0);
            self.last_p[c] = p;
            self.out[c] = self.out_kg[c] / dt.max(1e-6);
            let sg = &self.sig[c];
            let frac = |x: f64| if a.moles() > 0.0 { x / a.moles() } else { 0.0 };
            store.set(sg.p, p);
            store.set(sg.o2, p * frac(a.o2));
            store.set(sg.n2, p * frac(a.n2));
            store.set(sg.co2, p * frac(a.co2));
            store.set(sg.t, a.t);
            store.set(sg.leak, self.rate[c].max(0.0));
            store.set(sg.out, self.out[c]);
        }
        self.decompress(kind, s, &before, dt);
        self.find_vents();
        self.hiss(store);
        for c in 0..n {
            store.set(self.sig[c].shock, self.shock[c]);
            // the hull round it: the weakest plate (a hole is 0)
            let plan = &kind.compartments[c];
            let held = plan.hull.iter().chain(plan.walls.iter().map(|(p, _)| p)).map(|&p| {
                let q = &s.parts[p as usize];
                if q.alive { (q.hp / q.max_hp.max(1e-6)).clamp(0.0, 1.0) } else { 0.0 }
            });
            store.set(self.sig[c].integrity, f64::from(held.fold(1.0f32, f32::min)));
        }
    }

    /// Explosive decompression: the shock of a fast drop on the machines in the room; plates that
    /// no longer hold the pressure across them tear.
    fn decompress(&mut self, kind: &ShipKind, s: &Structure, before: &[f64], dt: f64) {
        for c in 0..self.air.len() {
            let p = self.pressure(c);
            let drop = (before[c] - p).max(0.0);
            let rate = drop / dt.max(1e-6);
            let v = smooth(DROP_RATE[0], DROP_RATE[1], rate);
            self.shock[c] = (self.shock[c] - dt / FADE).max(0.0);
            if v <= 0.0 {
                if rate < DROP_RATE[0] / 3.0 {
                    self.lost[c] = 0.0;
                    self.told[c] = false;
                }
                continue;
            }
            self.lost[c] += drop;
            if self.lost[c] >= ANNOUNCE {
                self.shock[c] = self.shock[c].max(v);
                if !self.told[c] {
                    self.told[c] = true;
                    self.said.push((format!("¡DESCOMPRESIÓN EXPLOSIVA · {}!", kind.compartments[c].name), 2));
                    self.bursts.push((c, v.min(1.0), tune::BURST_S));
                }
            }
            // the shock this tick: the share of a full cabin lost, weighted by how violently; the
            // machines by the opening get the worst of it
            let dose = drop / 70e3 * v;
            for k in 0..self.inside[c].len() {
                let (part, fragile) = self.inside[c][k];
                let pt = &s.parts[part as usize];
                if !pt.alive {
                    continue;
                }
                let d = self.paths.iter().filter(|g| g.a == c || g.b == c).filter_map(|g| g.at).map(|at| at.distance(pt.center)).fold(f32::INFINITY, f32::min);
                let near = if d.is_finite() { 1.0 + NEAR * f64::from((-d / NEAR_R).exp()) } else { 1.0 };
                let luck = 0.7 + 0.6 * self.rnd();
                let dmg = f64::from(pt.max_hp * fragile) * dose * near * luck;
                if dmg > 0.0 {
                    self.harm.push((part, dmg as f32));
                }
            }
        }
        // plates under a pressure they can no longer hold tear, faster the more overloaded
        for k in 0..self.plates.len() {
            let (part, a, b, holds_new) = self.plates[k];
            let pt = &s.parts[part as usize];
            let load = (self.pressure(a) - self.pressure(b)).abs();
            let health = if pt.alive { f64::from(pt.hp / pt.max_hp.max(1e-6)).max(0.0) } else { 0.0 };
            let holds = holds_new * health * health;
            let strain = if !pt.alive || load < 1000.0 {
                0.0
            } else if holds <= 0.0 {
                2.0
            } else {
                (load / holds - 1.0).max(0.0)
            };
            if strain <= 0.0 {
                self.yielding.retain(|&y| y != part);
                continue;
            }
            self.harm.push((part, (f64::from(pt.max_hp) * TEAR * strain.min(2.0) * dt) as f32));
            if !self.yielding.contains(&part) {
                self.yielding.push(part);
                self.said.push((format!("{} cediendo por la presión: suéldalo o saca el aire", kind.parts[part as usize]), 2));
            }
        }
    }

    /// Plates tearing under pressure now.
    pub fn yielding(&self) -> &[u32] {
        &self.yielding
    }

    /// The air was set from outside (`air`: another copy of the ship said what it has, `sync`):
    /// it is what it is from now on, with nothing rushing — the jump is not a drop to be taken
    /// for a decompression.
    pub fn settled(&mut self) {
        for c in 0..self.air.len() {
            self.last_p[c] = self.air[c].pressure(self.volume[c]);
            (self.rate[c], self.lost[c]) = (0.0, 0.0);
        }
    }

    /// Every opening with a place and a rush through it.
    fn find_vents(&mut self) {
        self.vents.clear();
        for k in 0..self.paths.len() {
            let g = self.paths[k];
            let Some(at) = g.at else { continue };
            let (pa, pb) = (self.pressure(g.a), self.pressure(g.b));
            let fwd = pa >= pb;
            let (up, down) = if fwd { (g.a, g.b) } else { (g.b, g.a) };
            let (pu, pd) = (pa.max(pb), pa.min(pb));
            if up == SPACE || pu - pd <= tune::STILL {
                continue;
            }
            let a = self.air[up];
            let tu = a.t.max(150.0);
            let o = orifice(g.area, pu, tu, M_AIR, pd);
            if o.mdot <= 0.0 {
                continue;
            }
            self.vents.push(Vent {
                up,
                down,
                at,
                dir: if fwd { g.n } else { -g.n },
                area: g.area,
                r0: (g.area / std::f64::consts::PI).sqrt(),
                mdot: o.mdot,
                rho: pu * M_AIR / (R * tu),
                speed: o.speed,
                thrust: o.thrust * tune::THRUST,
                part: g.part,
                fade: ((pu - pd - tune::STILL) / (3.0 * tune::STILL)).min(1.0).powi(2),
            });
        }
    }

    /// How loud the hand valves hiss: by the gas going through those that have a place. Written
    /// only when it changes (with every valve shut, nothing is looked at).
    fn hiss(&mut self, store: &mut Store) {
        let Some((signal, said)) = self.hiss else { return };
        let mut mdot = 0.0;
        for g in self.paths.iter().filter(|g| g.valve && g.at.is_some()) {
            let (pa, pb) = (self.pressure(g.a), self.pressure(g.b));
            let (up, pu, pd) = if pa >= pb { (g.a, pa, pb) } else { (g.b, pb, pa) };
            if up != SPACE && pu - pd > 50.0 {
                mdot += orifice_mdot(g.area, pu, self.air[up].t.max(150.0), M_AIR, pd);
            }
        }
        let level = (mdot / HISS).sqrt().min(1.0);
        if (level - said).abs() > 0.01 || (level == 0.0 && said != 0.0) {
            store.set(signal, level);
            self.hiss = Some((signal, level));
        }
    }

    /// Cross-section of compartment `c` across direction `d` (m²): its volume over its length.
    fn section(&self, c: usize, d: Vec3) -> f64 {
        let s = self.size[c];
        let len = f64::from(d.x.abs() * s.x + d.y.abs() * s.y + d.z.abs() * s.z);
        (self.volume[c] / len.max(0.3)).max(0.5)
    }

    /// What the moving air does to a body at ship-frame point `p` in compartment `comp` (none:
    /// outside the hull): its dynamic pressure along the flow (Pa, ship axes). Times a drag area it
    /// is a force.
    pub fn push(&self, p: Vec3, comp: Option<usize>) -> Vec3 {
        let mut out = Vec3::ZERO;
        let here = comp.unwrap_or(SPACE);
        // the room that just blew: the slug of air goes all at once
        let gain = self.bursts.iter().filter(|b| Some(b.0) == comp).map(|b| 1.0 + tune::BURST * b.1 * b.2 / tune::BURST_S).fold(1.0, f64::max) as f32;
        for e in &self.vents {
            if comp.is_some() && e.up == here {
                // sink: hemispherical near the opening, the whole section of the room further off
                let d = e.at - p;
                let r = f64::from(d.length()).max(0.05);
                let du = d / r as f32;
                let q = e.mdot / e.rho;
                let u = e.speed.min(q / (2.0 * std::f64::consts::PI * r * r).min(self.section(here, du)));
                // toward the opening, and out through it at its lip
                let k = (e.r0 / r) as f32;
                out += (du + e.dir * k).normalize_or_zero() * (0.5 * e.rho * u * u * e.fade) as f32;
            } else if e.down == here {
                // jet: into the next room (it slows as it mixes), or a plume into vacuum (it thins)
                let rel = p - e.at;
                let s = f64::from(rel.dot(e.dir));
                if s <= 0.0 {
                    continue;
                }
                let vac = e.down == SPACE;
                let b = e.r0 + if vac { 0.6 } else { 0.2 } * s;
                let lat2 = (f64::from(rel.dot(rel)) - s * s).max(0.0);
                if lat2 > 9.0 * b * b {
                    continue;
                }
                let u = e.speed * if vac { 1.0 } else { (6.0 * e.r0 / s).min(1.0) } * (-lat2 / (b * b)).exp();
                let rho = if vac { e.rho * (e.r0 / b).powi(2) } else { e.rho };
                out += e.dir * (0.5 * rho * u * u * e.fade) as f32;
            }
        }
        out * gain
    }

    /// Acceleration (m/s², same axes) of a body with drag area per mass `cda` in the push `q` (Pa).
    pub fn accel(q: Vec3, cda: f64) -> Vec3 {
        let a = q * (tune::DRAG * cda) as f32;
        let l = f64::from(a.length());
        if l > tune::MAX_ACCEL { a * (tune::MAX_ACCEL / l) as f32 } else { a }
    }

    /// The push of all the jets on the ship (ship frame): force and torque about `com`.
    pub fn kick(&self, com: Vec3) -> (Vec3, Vec3) {
        let (mut f, mut t) = (Vec3::ZERO, Vec3::ZERO);
        for e in &self.vents {
            let fv = -e.dir * e.thrust as f32;
            f += fv;
            t += (e.at - com).cross(fv);
        }
        (f, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orifice_chokes_and_matches_a_cabin_breach() {
        // 70 kPa cabin air at 294 K through 1 m² into vacuum: choked, ~sonic
        let o = orifice(1.0, 70e3, 294.0, M_AIR, 0.0);
        assert!(o.speed > 300.0 && o.speed < 320.0, "{o:?}");
        assert!(o.mdot > 100.0 && o.mdot < 130.0, "{o:?}");
        // the same mass flow from any downstream pressure under the critical ratio
        let o2 = orifice(1.0, 70e3, 294.0, M_AIR, 30e3);
        assert!((o2.mdot - o.mdot).abs() < 1e-9);
        // and less above it, none at equal pressures
        assert!(orifice(1.0, 70e3, 294.0, M_AIR, 60e3).mdot < o.mdot * 0.8);
        assert!(orifice(1.0, 70e3, 294.0, M_AIR, 70e3).mdot.abs() < 1e-9);
    }

    #[test]
    fn air_keeps_its_moles_and_mixes_heat() {
        let mut a = Air::standard(70e3, 60.0);
        let n = a.moles();
        let mut cold = Air { t: 200.0, ..a.take(0.5) };
        assert!((a.moles() + cold.moles() - n).abs() < 1e-9);
        cold.give(&a);
        assert!((cold.moles() - n).abs() < 1e-9);
        assert!(cold.t > 200.0 && cold.t < 294.0);
    }
}
