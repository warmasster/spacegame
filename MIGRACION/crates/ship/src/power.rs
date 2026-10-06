//! The ship's electrical balance, the same for any ship: what its generators give, what its loads
//! take, what its batteries make up (or take in), how full they are and how long they would last
//! at the present drain. Nothing is solved for it: the figures are read off the electrical
//! networks' own ports after each solve. What only passes through a machine from one electrical
//! network to another (a converter between buses) is neither made nor spent: only what it loses
//! on the way is spent. Signals (SI): `energia.generacion`, `energia.consumo`,
//! `energia.baterias` (out of them positive), `energia.balance` (generation less consumption),
//! `energia.carga` (0..1) and `energia.autonomia` (s; `NO_DRAIN` when nothing is draining them).
//! Shown by the "energia" section of panels (`assets/defs/secciones.jsonc`, `sections`) and by
//! the multi-function displays' "energia" page (`mfd_auto`).
use crate::kind::ShipKind;
use lunar_machines::net::{Medium, PortIo};
use lunar_signals::{SignalId, Store};
use std::ops::Range;

/// What `energia.autonomia` says when the batteries are not being drained (s): the most a
/// four-digit display of minutes shows.
pub const NO_DRAIN: f64 = 9999.0 * 60.0;
/// The time over which the batteries' drain is averaged (s): a load switched on is seen in the
/// time left within a few seconds, a blink of it is not.
const AVERAGE: f64 = 6.0;

pub struct Power {
    /// The ports on electrical networks (places in the ship's list of ports).
    ports: Vec<usize>,
    /// The machines with ports on more than one electrical network (their ports): what they take
    /// on one and give on another passes through them.
    links: Vec<Range<usize>>,
    /// Each battery's charge (0..1).
    charge: Vec<SignalId>,
    out: [SignalId; 6],
    /// The mean charge at the last tick (none before the first), how fast it falls (1/s) and
    /// over how long that has been averaged (s, up to `AVERAGE`).
    was: Option<f64>,
    drain: f64,
    seen: f64,
}

impl Power {
    /// Its signals, made before anything that reads them (panels, derived signals).
    pub fn new(kind: &ShipKind, store: &mut Store) -> Result<Power, String> {
        let mut def = |name: &str, unit: &str, init: f64| store.define_unit(name, unit, init).map_err(|e| e.0);
        let out = [def("energia.generacion", "kW", 0.0)?, def("energia.consumo", "kW", 0.0)?, def("energia.baterias", "kW", 0.0)?, def("energia.balance", "kW", 0.0)?, def("energia.carga", "%", 0.0)?, def("energia.autonomia", "min", NO_DRAIN)?];
        // (the ship's own figures: one writer, like every signal)
        for id in out {
            store.claim(id, lunar_signals::Writer::World)?;
        }
        let charge = kind.machines.iter().filter(|m| m.def.modelo == "bateria").filter_map(|m| store.find(&format!("{}.soc", m.id))).collect();
        Ok(Power { ports: Vec::new(), links: Vec::new(), charge, out, was: None, drain: 0.0, seen: 0.0 })
    }

    /// Once every port of the ship is there (machines', actuators', panels'): which are
    /// electrical, and which machines (`owners`: each one's ports) join two electrical networks.
    pub fn bind(&mut self, kind: &ShipKind, ports: &[PortIo], owners: impl Iterator<Item = Range<usize>>) {
        let electric = |k: usize| kind.nets.get(usize::from(ports[k].net)).is_some_and(|n| n.medium == Medium::Electrico);
        self.ports = (0..ports.len()).filter(|&k| electric(k)).collect();
        self.links = owners
            .filter(|own| {
                let mut nets = own.clone().filter(|&k| electric(k)).map(|k| ports[k].net);
                nets.next().is_some_and(|first| nets.any(|n| n != first))
            })
            .collect();
    }

    /// After the networks are solved: this tick's figures.
    pub fn step(&mut self, ports: &[PortIo], store: &mut Store, dt: f64) {
        let (mut given, mut taken, mut stored) = (0.0, 0.0, 0.0);
        for &k in &self.ports {
            let p = &ports[k];
            given += p.gave;
            taken += p.got;
            stored += p.stored;
        }
        // (what passes through a converter is not made again nor spent: its loss is)
        for own in &self.links {
            let (mut inn, mut out) = (0.0, 0.0);
            for &k in self.ports.iter().filter(|k| own.contains(k)) {
                inn += ports[k].got;
                out += ports[k].gave;
            }
            let through = inn.min(out);
            given -= through;
            taken -= through;
        }
        let charge = if self.charge.is_empty() { 0.0 } else { self.charge.iter().map(|s| store.get(*s)).sum::<f64>() / self.charge.len() as f64 };
        if let (Some(was), true) = (self.was, dt > 0.0) {
            // (a plain mean until `AVERAGE` has gone by, so that it is right from the start)
            self.seen = (self.seen + dt).min(AVERAGE);
            self.drain += ((was - charge) / dt - self.drain) * (dt / self.seen).min(1.0);
        }
        self.was = Some(charge);
        let left = if self.drain > 1e-9 { (charge / self.drain).min(NO_DRAIN) } else { NO_DRAIN };
        for (sig, v) in self.out.iter().zip([given, taken, -stored, given - taken, charge, left]) {
            store.set(*sig, v);
        }
    }
}
