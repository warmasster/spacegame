//! The signal store of one structure: every command, reading and derived value as a named `f64`
//! in SI, with its unit, range, quality and single writer. Names are resolved to ids once, when
//! the structure is built; at run time everything is by index into dense vectors, with a version
//! per signal so readers can skip what did not change. No allocation per tick.
use crate::units::{self, Unit};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SignalId(pub u32);

impl SignalId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Whether a value can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Quality {
    #[default]
    Ok,
    /// Not refreshed (its writer is asleep or unpowered): the last value stands.
    Stale,
    /// Its source is gone (the part was destroyed, the cable cut).
    Failed,
}

/// Who writes a signal. Exactly one per signal, checked when built.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Writer {
    /// Nobody yet (a constant or an input of the outside world).
    #[default]
    None,
    Control(u32),
    Machine(u32),
    Actuator(u32),
    Derived(u32),
    /// The structure itself (part health, sensors of the world).
    World,
}

#[derive(Clone, Debug)]
pub struct Meta {
    pub name: Box<str>,
    pub unit: Unit,
    /// How it is shown (unit name, e.g. "kN"); empty for SI / dimensionless.
    pub show: Box<str>,
    pub range: (f64, f64),
    pub writer: Writer,
}

#[derive(Clone, Debug, Default)]
pub struct Store {
    values: Vec<f64>,
    quality: Vec<Quality>,
    version: Vec<u32>,
    meta: Vec<Meta>,
    names: HashMap<Box<str>, u32>,
    /// Bumped on every change of any value (cheap "anything changed?").
    pub epoch: u64,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The signal called `name`, created (0, dimensionless) if new.
    pub fn define(&mut self, name: &str) -> SignalId {
        if let Some(&i) = self.names.get(name) {
            return SignalId(i);
        }
        let i = self.values.len() as u32;
        self.values.push(0.0);
        self.quality.push(Quality::Ok);
        self.version.push(0);
        self.meta.push(Meta { name: name.into(), unit: Unit::ONE, show: "".into(), range: (f64::NEG_INFINITY, f64::INFINITY), writer: Writer::None });
        self.names.insert(name.into(), i);
        SignalId(i)
    }

    /// Define with a unit (`show` is how it reads, e.g. "kN") and an initial SI value.
    pub fn define_unit(&mut self, name: &str, show: &str, init: f64) -> Result<SignalId, units::UnitError> {
        let id = self.define(name);
        let unit = units::unit(show)?;
        let m = &mut self.meta[id.index()];
        m.unit = unit;
        m.show = show.into();
        self.values[id.index()] = init;
        Ok(id)
    }

    pub fn set_range(&mut self, id: SignalId, lo: f64, hi: f64) {
        self.meta[id.index()].range = (lo, hi);
    }

    pub fn find(&self, name: &str) -> Option<SignalId> {
        self.names.get(name).map(|&i| SignalId(i))
    }

    pub fn meta(&self, id: SignalId) -> &Meta {
        &self.meta[id.index()]
    }

    pub fn name(&self, id: SignalId) -> &str {
        &self.meta[id.index()].name
    }

    /// Give `id` its writer: an error if someone else already writes it.
    pub fn claim(&mut self, id: SignalId, w: Writer) -> Result<(), String> {
        let m = &mut self.meta[id.index()];
        if m.writer != Writer::None && m.writer != w {
            return Err(format!("la señal '{}' ya tiene escritor ({:?}); no puede escribirla también {:?}", m.name, m.writer, w));
        }
        m.writer = w;
        Ok(())
    }

    #[inline]
    pub fn get(&self, id: SignalId) -> f64 {
        self.values[id.index()]
    }

    /// As a switch: on when ≥ 0.5.
    #[inline]
    pub fn on(&self, id: SignalId) -> bool {
        self.values[id.index()] >= 0.5
    }

    #[inline]
    pub fn quality(&self, id: SignalId) -> Quality {
        self.quality[id.index()]
    }

    #[inline]
    pub fn version(&self, id: SignalId) -> u32 {
        self.version[id.index()]
    }

    /// Write a value (bumps its version only if it changed). Quality back to Ok.
    #[inline]
    pub fn set(&mut self, id: SignalId, v: f64) {
        let i = id.index();
        if self.values[i] != v || self.quality[i] != Quality::Ok {
            self.values[i] = v;
            self.quality[i] = Quality::Ok;
            self.version[i] = self.version[i].wrapping_add(1);
            self.epoch += 1;
        }
    }

    pub fn set_quality(&mut self, id: SignalId, q: Quality) {
        let i = id.index();
        if self.quality[i] != q {
            self.quality[i] = q;
            self.version[i] = self.version[i].wrapping_add(1);
            self.epoch += 1;
        }
    }

    /// Every value in index order (snapshots, network sync).
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    pub fn qualities(&self) -> &[Quality] {
        &self.quality
    }

    /// Load values saved with `values()` (same definitions): versions move where they differ.
    pub fn load(&mut self, values: &[f64]) {
        for (i, &v) in values.iter().enumerate().take(self.values.len()) {
            self.set(SignalId(i as u32), v);
        }
    }

    pub fn ids(&self) -> impl Iterator<Item = SignalId> {
        (0..self.values.len() as u32).map(SignalId)
    }

    /// "62,4 %": the value in its display unit.
    pub fn format(&self, id: SignalId, decimals: usize, out: &mut String) {
        let m = &self.meta[id.index()];
        units::format(self.values[id.index()], &m.show, &m.unit, decimals, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_writer_and_versions() {
        let mut s = Store::new();
        let a = s.define_unit("motor.empuje", "kN", 0.0).unwrap();
        assert_eq!(s.define("motor.empuje"), a);
        s.claim(a, Writer::Machine(0)).unwrap();
        assert!(s.claim(a, Writer::Control(3)).is_err());
        let v0 = s.version(a);
        s.set(a, 12_000.0);
        assert_ne!(s.version(a), v0);
        let v1 = s.version(a);
        s.set(a, 12_000.0);
        assert_eq!(s.version(a), v1);
        let mut t = String::new();
        s.format(a, 1, &mut t);
        assert_eq!(t, "12,0 kN");
    }
}
