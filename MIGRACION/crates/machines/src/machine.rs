//! The machine contract. A machine is a physical model behind a part: it reads commands from
//! signals, asks its networks for what it needs (and offers what it makes) before they are
//! solved, then integrates its physics with what it got and writes its telemetry. It may push
//! (thrust), shine (light) or answer to damage. Its state is plain numbers (`save`/`load`), so it
//! travels with its part, over the network and into save games.
//!
//! A new model is one file in `models/` and one line in `models::create`.
use crate::net::{Medium, PortIo};
use lunar_signals::{Q, SignalId, Store, Writer, units};
use serde_json::{Map, Value};

/// The world round a machine, in its structure's frame.
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// Ambient pressure (Pa) and the temperature heat radiates to (K).
    pub pressure: f64,
    pub sink_temp: f64,
    /// Sunlight on the part's working face (W/m², after the angle) and the full irradiance.
    pub sun: f64,
    pub irradiance: f64,
    /// Gravity (m/s²).
    pub gravity: f64,
}

impl Default for Env {
    fn default() -> Self {
        Env { pressure: 0.0, sink_temp: 230.0, sun: 0.0, irradiance: 1361.0, gravity: 1.62 }
    }
}

/// What a machine has to work with on a tick.
pub struct Cx<'a> {
    pub signals: &'a mut Store,
    pub env: Env,
    /// Health of its part (0 wrecked .. 1 new) and whether it still works at all.
    pub health: f32,
    pub working: bool,
    /// Time now (s) and the step (s).
    pub t: f64,
    pub dt: f64,
}

/// What a machine does to the air it treats (its duct network's, or its compartment's), per
/// second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Treat {
    /// CO₂ it takes out (mol/s, up to what there is).
    pub co2: f64,
    /// O₂ and N₂ it adds (mol/s).
    pub o2: f64,
    pub n2: f64,
    /// Heat into the air (W).
    pub heat: f64,
}

/// What a machine declares about itself when built.
#[derive(Clone, Debug)]
pub struct PortSpec {
    /// Role in the model ("salida", "alimentacion", "oxidante"...); the data says which network.
    pub role: &'static str,
    pub medium: Medium,
}

pub trait Machine: Send + Sync {
    fn kind(&self) -> &'static str;
    /// Its ports, in the order of the `io` slices it gets.
    fn ports(&self) -> &[PortSpec];
    /// Before the networks are solved: fill what each port asks and offers (cleared already).
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]);
    /// After: integrate with what it got, write telemetry.
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]);
    /// A long stretch at once (coarse or sleeping structures): by default, steps of at most 1 s
    /// with what its ports got when the networks were last solved. Nothing is asked again and
    /// nothing is solved: what was flowing goes on flowing, so a store drains or fills at the rate
    /// it was doing, and what was not fed stays unfed (a converter on a dead bus gives nothing).
    fn settle(&mut self, cx: &mut Cx, io: &mut [PortIo], dt_long: f64) {
        let n = dt_long.ceil().clamp(1.0, 600.0) as usize;
        let dt = dt_long / n as f64;
        let t0 = cx.t;
        for k in 0..n {
            cx.dt = dt;
            cx.t = t0 + dt * k as f64;
            self.step(cx, io);
        }
    }
    /// Thrust it pushes with now (N), along its part's thrust axis.
    fn thrust(&self) -> f64 {
        0.0
    }
    /// Torque it turns its part's structure with now (N·m, its part's axes): momentum wheels.
    fn torque(&self) -> [f64; 3] {
        [0.0; 3]
    }
    /// Light it gives (0..1 of its rated output).
    fn light(&self) -> f32 {
        0.0
    }
    /// Heat it gives off into its compartment (W).
    fn heat(&self) -> f64 {
        0.0
    }
    /// Duct air it lets into its compartment (m³/s): vents.
    fn air(&self) -> f64 {
        0.0
    }
    /// What it does to the air.
    fn treat(&self) -> Treat {
        Treat::default()
    }
    /// The places (compartments and vessels, by name) it moves gas between: a compressor's
    /// manifold. Its owner finds them once.
    fn places(&self) -> &[String] {
        &[]
    }
    /// Gas it moves now: from which of its places to which, and how fast (mol/s).
    fn moving(&self) -> Option<(usize, usize, f64)> {
        None
    }
    /// The gas it holds by composition, for its owner to fill and draw: a tank of recovered air.
    fn vessel(&mut self) -> Option<&mut crate::gas::Vessel> {
        None
    }
    /// It hurts its own part (share of its hit points per second): an engine past its limits.
    fn harm(&self) -> f32 {
        0.0
    }
    /// Whether it is on its way from one state to another now — starting, spinning up, charging,
    /// filling: what whoever works it waits for. None: nothing about it takes time. Nobody reads
    /// it in play; the procedures' test (`lunar_ship::procedures`) holds every model that says
    /// Some to a procedure with a budget in seconds, and every other to being declared instant.
    fn settling(&self) -> Option<bool> {
        None
    }
    /// An explosion it sets off now (J), once.
    fn take_burst(&mut self) -> f64 {
        0.0
    }
    /// What it let fly since it was last asked (rounds, missiles, decoys: a weapon or a
    /// dispenser), once. What each is, is its data's to say (`municion`); where and which way,
    /// its part's.
    fn take_shots(&mut self) -> u32 {
        0
    }
    /// The explosion if its part is destroyed (J): a tank with propellant, a bottle under pressure.
    fn rupture(&self) -> f64 {
        0.0
    }
    /// Plain state.
    fn save(&self, out: &mut Vec<f64>);
    fn load(&mut self, s: &[f64]);
}

/// Builds a machine from its data: parameters with units, telemetry signals named after it,
/// command signals by role (the data may rename them).
pub struct Build<'a> {
    pub id: &'a str,
    pub params: &'a Map<String, Value>,
    pub store: &'a mut Store,
    pub writer: Writer,
    /// Signals the data names for each command role (`"ordenes": { "arranque": "x.y" }`).
    pub orders: &'a Map<String, Value>,
    /// Prefix of its telemetry (default: its id).
    pub prefix: String,
}

impl<'a> Build<'a> {
    pub fn new(id: &'a str, params: &'a Map<String, Value>, orders: &'a Map<String, Value>, store: &'a mut Store, writer: Writer) -> Build<'a> {
        let prefix = params.get("telemetria").and_then(Value::as_str).unwrap_or(id).to_string();
        Build { id, params, store, writer, orders, prefix }
    }

    fn err(&self, what: impl std::fmt::Display) -> String {
        format!("{}: {what}", self.id)
    }

    /// A quantity parameter in SI (`"45 kN"` or a bare number), checked against `unit`.
    pub fn q(&self, key: &str, unit: &str, default: f64) -> Result<f64, String> {
        match self.params.get(key) {
            None => Ok(default),
            Some(v) => {
                let q: Q = serde_json::from_value(v.clone()).map_err(|e| self.err(format!("{key}: {e}")))?;
                q.si_as(unit).map_err(|e| self.err(format!("{key}: {}", e.0)))
            }
        }
    }

    /// A difference of temperatures or any offset unit ("50 °C" of rise is 50 K).
    pub fn dq(&self, key: &str, default: f64) -> Result<f64, String> {
        match self.params.get(key) {
            None => Ok(default),
            Some(Value::Number(n)) => Ok(n.as_f64().unwrap_or(default)),
            Some(Value::String(s)) => units::parse(s).map(|(v, u)| v - u.offset).map_err(|e| self.err(e.0)),
            Some(_) => Err(self.err(format!("{key}: se esperaba una cantidad"))),
        }
    }

    pub fn f(&self, key: &str, default: f64) -> f64 {
        self.params.get(key).and_then(Value::as_f64).unwrap_or(default)
    }

    pub fn flag(&self, key: &str, default: bool) -> bool {
        self.params.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(Value::as_str)
    }

    /// A curve `[[x, y], ...]` sorted by x.
    pub fn curve(&self, key: &str, default: &[(f64, f64)]) -> Result<Vec<(f64, f64)>, String> {
        match self.params.get(key) {
            None => Ok(default.to_vec()),
            Some(v) => {
                let pts: Vec<[f64; 2]> = serde_json::from_value(v.clone()).map_err(|e| self.err(format!("{key}: {e}")))?;
                let mut c: Vec<(f64, f64)> = pts.into_iter().map(|[x, y]| (x, y)).collect();
                c.sort_by(|a, b| a.0.total_cmp(&b.0));
                if c.is_empty() {
                    return Err(self.err(format!("{key}: curva vacía")));
                }
                Ok(c)
            }
        }
    }

    /// A telemetry signal `<prefix>.<field>` shown in `unit` (claimed: only this machine writes it).
    pub fn out(&mut self, field: &str, unit: &str) -> Result<SignalId, String> {
        let name = format!("{}.{field}", self.prefix);
        let id = self.store.define_unit(&name, unit, 0.0).map_err(|e| self.err(e.0))?;
        self.store.claim(id, self.writer.clone()).map_err(|e| self.err(e))?;
        Ok(id)
    }

    /// A command signal for `role`: the one the data names, else `<id>.<role>`. Read only.
    pub fn cmd(&mut self, role: &str) -> SignalId {
        let name = match self.orders.get(role).and_then(Value::as_str) {
            Some(n) => n.to_string(),
            None => format!("{}.{role}", self.id),
        };
        self.store.define(&name)
    }
}

/// Linear interpolation in a sorted curve, held flat past its ends.
pub fn interp(c: &[(f64, f64)], x: f64) -> f64 {
    if x <= c[0].0 {
        return c[0].1;
    }
    for w in c.windows(2) {
        if x <= w[1].0 {
            let t = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
            return w[0].1 + (w[1].1 - w[0].1) * t;
        }
    }
    c[c.len() - 1].1
}

/// First-order approach of `x` to `target` with time constant `tau` over `dt`.
pub fn approach(x: f64, target: f64, tau: f64, dt: f64) -> f64 {
    if tau <= 0.0 {
        return target;
    }
    x + (target - x) * (1.0 - (-dt / tau).exp())
}

/// Stefan-Boltzmann (W/m²K⁴).
pub const SIGMA: f64 = 5.670_374e-8;
/// Standard gravity for Isp (m/s²).
pub const G0: f64 = 9.806_65;
