//! Air moved by machines (`docs/AIRE.md`): compressors draw one place down into another, tanks
//! keep what is recovered, each gas by its moles. The machines only say what they do
//! (`Machine::moving`: from which of their places to which, how fast; `Machine::vessel`: the gas a
//! tank holds); here it is done, to the air of the compartments and to the vessels, so that what
//! leaves one place is exactly what reaches the other.
//!
//! A ship with no such machine has none of this; one whose compressors are stopped and whose
//! tanks are whole walks two short lists a tick and moves nothing.
use crate::{
    atmos::{Air, room_of},
    kind::ShipKind,
    ship::MachineRt,
};
use lunar_machines::gas::Mix;

/// Where gas is taken from or put: a compartment, or the vessel of a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Room(usize),
    Vessel(usize),
}

/// A machine that moves gas, and what each of its places is (None: a name nothing answers to).
struct Mover {
    machine: usize,
    places: Vec<Option<Place>>,
}

#[derive(Default)]
pub struct Transfers {
    movers: Vec<Mover>,
    /// The machines that are vessels, and the compartment each stands in.
    vessels: Vec<(usize, Option<usize>)>,
    /// Gas a machine moved this tick.
    pub active: bool,
    /// Ticks in which a machine moved gas, and all it has moved (mol): what tests count.
    pub ticks: u64,
    pub moved: f64,
}

impl Transfers {
    /// The machines of `kind` that move or keep gas, their places found by name: a compartment's
    /// id, or the id of a machine that is a vessel.
    pub fn bind(kind: &ShipKind, machines: &mut [MachineRt]) -> Transfers {
        let vessel: Vec<bool> = machines.iter_mut().map(|m| m.m.vessel().is_some()).collect();
        let place = |name: &str| -> Option<Place> {
            if let Some(c) = kind.compartments.iter().position(|c| c.id == name) {
                return Some(Place::Room(c));
            }
            kind.machines.iter().position(|m| m.id == name).filter(|&k| vessel[k]).map(Place::Vessel)
        };
        let movers = machines.iter().enumerate().filter(|(_, m)| !m.m.places().is_empty()).map(|(k, m)| Mover { machine: k, places: m.m.places().iter().map(|n| place(n)).collect() }).collect();
        let vessels = (0..machines.len()).filter(|&k| vessel[k]).map(|k| (k, kind.machines[k].part.and_then(|p| room_of(kind, kind.centers[p as usize])))).collect();
        Transfers { movers, vessels, ..Default::default() }
    }

    /// Nothing here moves or keeps gas.
    pub fn is_empty(&self) -> bool {
        self.movers.is_empty() && self.vessels.is_empty()
    }

    /// The names a mover's data gave that are no place of this ship (for whoever builds it).
    pub fn unknown(&self, machines: &[MachineRt]) -> Vec<String> {
        self.movers.iter().flat_map(|m| m.places.iter().zip(machines[m.machine].m.places()).filter(|(p, _)| p.is_none()).map(|(_, n)| n.clone())).collect()
    }

    /// One tick: every mover's gas from its source to its destination, and what a holed vessel
    /// loses into where it stands.
    pub fn step(&mut self, air: &mut [Air], machines: &mut [MachineRt], dt: f64) {
        self.active = false;
        for k in 0..self.movers.len() {
            let Some((from, to, rate)) = machines[self.movers[k].machine].m.moving() else { continue };
            let places = &self.movers[k].places;
            let (Some(Some(from)), Some(Some(to))) = (places.get(from).copied(), places.get(to).copied()) else { continue };
            if from == to || rate <= 0.0 {
                continue;
            }
            let moved = take(air, machines, from, rate * dt);
            let n = moved.0.moles();
            if n <= 0.0 {
                continue;
            }
            give(air, machines, to, &moved.0, moved.1);
            self.active = true;
            self.ticks += 1;
            self.moved += n;
        }
        for k in 0..self.vessels.len() {
            let (m, room) = self.vessels[k];
            let Some(v) = machines[m].m.vessel() else { continue };
            if v.leak <= 0.0 || v.gas.moles() <= 0.0 {
                continue;
            }
            let (out, t) = (v.gas.take((v.leak * dt).min(1.0)), v.t);
            // (out of every compartment it is lost)
            if let Some(c) = room {
                air[c].give(&Air { o2: out.o2, n2: out.n2, co2: out.co2, t });
            }
            self.active = true;
        }
    }

    /// Everything the vessels hold (mol of O₂, N₂, CO₂).
    pub fn held(&self, machines: &mut [MachineRt]) -> Mix {
        let mut all = Mix::default();
        for &(m, _) in &self.vessels {
            if let Some(v) = machines[m].m.vessel() {
                all.give(&v.gas);
            }
        }
        all
    }
}

/// Up to `n` mol out of `place` (no more than there is), as it is there: the mix and its
/// temperature.
fn take(air: &mut [Air], machines: &mut [MachineRt], place: Place, n: f64) -> (Mix, f64) {
    match place {
        Place::Room(c) => {
            let a = &mut air[c];
            let there = a.moles();
            if there <= 1e-9 {
                return (Mix::default(), a.t);
            }
            let got = a.take((n / there).min(1.0));
            (Mix { o2: got.o2, n2: got.n2, co2: got.co2 }, got.t)
        }
        Place::Vessel(m) => match machines[m].m.vessel() {
            Some(v) => {
                let there = v.gas.moles();
                if there <= 1e-9 {
                    return (Mix::default(), v.t);
                }
                (v.gas.take((n / there).min(1.0)), v.t)
            }
            None => (Mix::default(), 0.0),
        },
    }
}

/// `gas` at `t` K into `place` (a vessel's walls take its heat: it stays at its own).
fn give(air: &mut [Air], machines: &mut [MachineRt], place: Place, gas: &Mix, t: f64) {
    match place {
        Place::Room(c) => air[c].give(&Air { o2: gas.o2, n2: gas.n2, co2: gas.co2, t }),
        Place::Vessel(m) => {
            if let Some(v) = machines[m].m.vessel() {
                v.gas.give(gas);
            }
        }
    }
}
