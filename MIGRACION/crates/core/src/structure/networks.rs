//! Networks over the joints: energy, propellant, air... each a resource that flows between parts
//! that have its port, still work, and are joined by joints that carry it. Every island of a
//! network shares out what its parts produce and store among what they ask, highest priority
//! first; cut a joint or wreck the reactor and the islands left without supply go dark.
//!
//! What a part does on its networks is data (`module.params`: produce, consume, store, priority);
//! a `Machine` (by `module.kind`) may change it with the circumstances (a solar panel and the sun).
//! The state lives in the parts (`store`, `supplied`), so a structure that splits keeps it.
use super::{catalog::Catalog, graph::Groups, state::Structure};
use glam::Vec3;
use serde::Deserialize;
use std::collections::BTreeMap;

/// One part's exchange with one network (per second; stores in units).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    /// Network bit index.
    pub network: u8,
    pub produce: f32,
    pub consume: f32,
    pub capacity: f32,
    /// Higher is served first.
    pub priority: u8,
}

/// The `module.params` every machine understands: network name → rate (or capacity).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FlowsDef {
    #[serde(default)]
    pub produce: BTreeMap<String, f32>,
    #[serde(default)]
    pub consume: BTreeMap<String, f32>,
    #[serde(default)]
    pub store: BTreeMap<String, f32>,
    #[serde(default = "one")]
    pub priority: u8,
}

fn one() -> u8 {
    1
}

/// What the world around tells a machine (structure frame).
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// Toward the sun (zero: no sun).
    pub sun: Vec3,
}

/// A part's behaviour on its networks: the flows of its data, changed by circumstances.
pub trait Machine: Send + Sync {
    fn flows(&self, s: &Structure, part: usize, base: &[Flow], env: &Env, out: &mut Vec<Flow>);
}

/// Exactly what the data says.
pub struct Steady;

impl Machine for Steady {
    fn flows(&self, _: &Structure, _: usize, base: &[Flow], _: &Env, out: &mut Vec<Flow>) {
        out.extend_from_slice(base);
    }
}

/// Produces as much as the sun falls square on its face (its +Y), nothing at night.
pub struct Solar;

impl Machine for Solar {
    fn flows(&self, s: &Structure, part: usize, base: &[Flow], env: &Env, out: &mut Vec<Flow>) {
        let n = s.parts[part].local.transform_vector3(Vec3::Y).normalize_or_zero();
        let light = n.dot(env.sun).max(0.0);
        out.extend(base.iter().map(|f| Flow { produce: f.produce * light, ..*f }));
    }
}

/// The machine a module kind names (anything unknown behaves as its data says).
pub fn machine(kind: &str) -> Box<dyn Machine> {
    match kind {
        "solar" => Box::new(Solar),
        _ => Box::new(Steady),
    }
}

/// Every part kind's data flows and machine, resolved against the catalog's networks.
pub struct Machines {
    base: Vec<Vec<Flow>>,
    machines: Vec<Option<Box<dyn Machine>>>,
}

impl Machines {
    pub fn new(cat: &Catalog) -> Result<Machines, String> {
        let mut base = Vec::with_capacity(cat.parts.len());
        let mut machines = Vec::with_capacity(cat.parts.len());
        for k in &cat.parts {
            let Some(m) = &k.def.module else {
                base.push(Vec::new());
                machines.push(None);
                continue;
            };
            let d: FlowsDef = serde_json::from_value(serde_json::Value::Object(m.params.clone())).unwrap_or_default();
            let mut flows: Vec<Flow> = Vec::new();
            let bit = |name: &str| cat.network(name).map(|b| b.trailing_zeros() as u8).filter(|b| k.ports & (1 << b) != 0).ok_or_else(|| format!("part {}: no '{name}' port", k.id));
            for (name, &r) in &d.produce {
                flows.push(Flow { network: bit(name)?, produce: r, priority: d.priority, ..Default::default() });
            }
            for (name, &r) in &d.consume {
                flows.push(Flow { network: bit(name)?, consume: r, priority: d.priority, ..Default::default() });
            }
            if d.store.len() > 1 {
                return Err(format!("part {}: one store at most", k.id));
            }
            for (name, &c) in &d.store {
                flows.push(Flow { network: bit(name)?, capacity: c, priority: d.priority, ..Default::default() });
            }
            base.push(flows);
            machines.push(Some(machine(&m.kind)));
        }
        Ok(Machines { base, machines })
    }

    /// Capacity of the store of a part kind (0: none).
    pub fn capacity(&self, kind: u16) -> f32 {
        self.base[usize::from(kind)].iter().map(|f| f.capacity).sum()
    }
}

/// Reused between ticks.
#[derive(Default)]
pub struct Solver {
    groups: Groups,
    flows: Vec<(u32, Flow)>,
    scratch: Vec<Flow>,
    prios: Vec<u8>,
}

impl Solver {
    /// `dt` s of every network of `s`: supplies shared out, stores charged or drained, each part's
    /// `supplied` set (0..1). Returns whether anything turned on or off (the look changes).
    pub fn tick(&mut self, s: &mut Structure, cat: &Catalog, m: &Machines, env: &Env, dt: f32) -> bool {
        if dt <= 0.0 {
            return false;
        }
        self.flows.clear();
        for (i, p) in s.parts.iter().enumerate() {
            let Some(machine) = m.machines[usize::from(p.kind)].as_ref().filter(|_| p.alive && p.working) else {
                continue;
            };
            self.scratch.clear();
            machine.flows(s, i, &m.base[usize::from(p.kind)], env, &mut self.scratch);
            self.flows.extend(self.scratch.iter().map(|f| (i as u32, *f)));
        }
        // nothing in it makes, takes or keeps anything (a ship's own systems are its own, a loose
        // piece has none): nothing to share out
        if self.flows.is_empty() {
            return false;
        }
        let before: Vec<bool> = s.parts.iter().map(|p| p.supplied >= 0.5).collect();
        for p in &mut s.parts {
            p.supplied = 1.0;
        }
        for net in 0..cat.networks.len() as u8 {
            let bit = 1u32 << net;
            let ports = |i: usize| s.parts[i].alive && s.parts[i].working && cat.parts[usize::from(s.parts[i].kind)].ports & bit != 0;
            self.groups.build(s.parts.len(), ports, s.joints.iter().filter(|j| j.alive && j.networks & bit != 0).map(|j| (j.a, j.b)));
            for g in 0..self.groups.count as u32 {
                let mine = |&&(i, f): &&(u32, Flow)| f.network == net && self.groups.label[i as usize] == g;
                let made: f32 = self.flows.iter().filter(mine).map(|(_, f)| f.produce).sum();
                let cap: f32 = self.flows.iter().filter(mine).map(|(_, f)| f.capacity).sum();
                let level = |i: u32, f: &Flow| {
                    let st = s.parts[i as usize].store;
                    if st.is_nan() { f.capacity } else { st }
                };
                let stored: f32 = self.flows.iter().filter(mine).filter(|(_, f)| f.capacity > 0.0).map(|(i, f)| level(*i, f)).sum();
                // what can be given this tick, served by priority
                self.prios.clear();
                self.prios.extend(self.flows.iter().filter(mine).filter(|(_, f)| f.consume > 0.0).map(|(_, f)| f.priority));
                self.prios.sort_unstable_by(|a, b| b.cmp(a));
                self.prios.dedup();
                let want = |prio: u8| self.flows.iter().filter(mine).filter(|(_, f)| f.priority == prio).map(|(_, f)| f.consume).sum::<f32>();
                // over the interval: what is made and stored, served by priority; the rest (or the
                // shortfall) into (or out of) the stores, by room or level
                let mut left = made + stored / dt;
                for &prio in &self.prios {
                    let w = want(prio);
                    left -= w * (left / w).clamp(0.0, 1.0);
                }
                let target = (left * dt).clamp(0.0, cap);
                if cap > 0.0 {
                    for &(i, f) in self.flows.iter().filter(mine) {
                        if f.capacity > 0.0 {
                            s.parts[i as usize].store = target * f.capacity / cap;
                        }
                    }
                }
                // as it ends: with something stored everyone is fed, else only what is made
                let mut now = if target > 1e-3 { f32::INFINITY } else { made };
                for &prio in &self.prios {
                    let w = want(prio);
                    let share = (now / w).clamp(0.0, 1.0);
                    now -= w * share;
                    for &(i, f) in self.flows.iter().filter(mine) {
                        if f.priority == prio && f.consume > 0.0 {
                            s.parts[i as usize].supplied = s.parts[i as usize].supplied.min(share);
                        }
                    }
                }
            }
        }
        s.parts.iter().zip(before).any(|(p, b)| (p.supplied >= 0.5) != b)
    }
}
