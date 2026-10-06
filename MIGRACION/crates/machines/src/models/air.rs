//! Moving air on purpose (`docs/AIRE.md`): a compressor that draws one place down into another
//! through its manifold, and the tank that keeps what it recovers.
//!
//! A *place* is anything that says its pressure as `<name>.p` (Pa) and whether it is tight as
//! `<name>.estanco`: a compartment of the ship, or a vessel (which also says how much it takes,
//! `<name>.p_max`). The compressor knows its places by the names its data gives (`lugares`); its
//! two selectors say from which to which, and it says how fast it moves gas (`Machine::moving`).
//! Whoever owns the air moves it, with the composition it has where it is taken from.
//!
//! The compressor is a multi-stage piston machine with intercooling:
//! - what its first stage sweeps (`caudal`, at its intake) times the gas's density there, less
//!   what the gas left in its clearance volume takes back at each stroke
//!   (ηv = 1 − c·(r^(1/n) − 1) per stage): the lower the intake pressure, the less it moves;
//! - no more than its motor affords: the isothermal work of the compression, R·T·ln(r) per mole,
//!   over its isothermal efficiency;
//! - its intake valves stop opening under `presion_minima`: it never reaches a hard vacuum;
//! - gas that is already above what it delivers into goes through by itself (its by-pass line,
//!   an orifice of bore `paso`): the motor does not turn.
use crate::{
    gas::{M_AIR, Mix, R, Vessel, orifice_mdot},
    machine::{Build, Cx, Machine, PortSpec, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;
use serde_json::Value;

/// The temperature gas is handled at (K): what comes out of the intercoolers, what a tank's walls
/// hold it at.
pub const T_REF: f64 = 294.0;
/// Polytropic exponent of the compression in a cylinder.
const N_POLY: f64 = 1.3;
/// What its valves and passages take, as a share of R·T per mole moved.
const LOSS: f64 = 0.1;
/// How it stands (`<id>.estado`).
pub const STOPPED: f64 = 0.0;
pub const PUMPING: f64 = 1.0;
pub const BYPASS: f64 = 2.0;
pub const AT_LIMIT: f64 = 3.0;
pub const FULL: f64 = 4.0;
pub const NO_POWER: f64 = 5.0;
pub const OPEN: f64 = 6.0;
pub const SAME: f64 = 7.0;

/// A tank of recovered air: a vessel that keeps what is put in it, O₂, N₂ and CO₂ each by its
/// moles. It says what a compartment says (`p`, `o2`, `n2`, `co2`, `estanco`) and how much it
/// takes (`p_max`), so whatever handles a compartment handles it. Holed, it leaks into where it
/// stands; destroyed under pressure, it bursts.
pub struct Tank {
    vessel: Vessel,
    p_max: f64,
    /// What it held when it last said so (nothing is written while nothing changes).
    told: f64,
    o_p: SignalId,
    o_o2: SignalId,
    o_n2: SignalId,
    o_co2: SignalId,
    o_mass: SignalId,
    o_level: SignalId,
    o_max: SignalId,
    o_tight: SignalId,
}

impl Tank {
    pub fn new(b: &mut Build) -> Result<Tank, String> {
        let volume = b.q("volumen", "m3", 0.42)?;
        let p_max = b.q("presion_max", "Pa", 12e6)?;
        // what it starts with: cabin air at this pressure, this share of it oxygen
        let p0 = b.q("presion_inicial", "Pa", 0.0)?.clamp(0.0, p_max);
        let o2 = b.f("o2", 0.3).clamp(0.0, 1.0);
        let n = p0 * volume / (R * T_REF);
        Ok(Tank {
            vessel: Vessel { gas: Mix { o2: n * o2, n2: n * (1.0 - o2), co2: 0.0 }, volume, t: T_REF, leak: 0.0 },
            p_max,
            told: -1.0,
            o_p: b.out("p", "MPa")?,
            o_o2: b.out("o2", "kPa")?,
            o_n2: b.out("n2", "kPa")?,
            o_co2: b.out("co2", "kPa")?,
            o_mass: b.out("masa", "kg")?,
            o_level: b.out("nivel", "%")?,
            o_max: b.out("p_max", "MPa")?,
            o_tight: b.out("estanco", "")?,
        })
    }
}

impl Machine for Tank {
    fn kind(&self) -> &'static str {
        "deposito_aire"
    }
    fn ports(&self) -> &[PortSpec] {
        &[]
    }
    fn plan(&mut self, _: &mut Cx, _: &mut [PortIo]) {}
    fn step(&mut self, cx: &mut Cx, _: &[PortIo]) {
        // a hurt vessel leaks, a wrecked one lets everything go
        let h = f64::from(cx.health);
        self.vessel.leak = if !cx.working {
            1.5
        } else if h < 0.7 {
            0.02 * ((0.7 - h) / 0.7).powi(2)
        } else {
            0.0
        };
        let n = self.vessel.gas.moles();
        let key = n + if cx.working { 0.0 } else { -1.0 };
        if key == self.told {
            return;
        }
        self.told = key;
        let p = self.vessel.pressure();
        let share = |x: f64| if n > 0.0 { p * x / n } else { 0.0 };
        let s = &mut cx.signals;
        s.set(self.o_p, p);
        s.set(self.o_o2, share(self.vessel.gas.o2));
        s.set(self.o_n2, share(self.vessel.gas.n2));
        s.set(self.o_co2, share(self.vessel.gas.co2));
        s.set(self.o_mass, self.vessel.gas.mass());
        s.set(self.o_level, p / self.p_max);
        s.set(self.o_max, self.p_max);
        s.set(self.o_tight, f64::from(u8::from(cx.working)));
    }
    fn rupture(&self) -> f64 {
        // the stored gas expanding: p·V·ln(p/p0)
        let p = self.vessel.pressure();
        if p > 1e6 { p * self.vessel.volume * (p / 1e5).ln() } else { 0.0 }
    }
    fn vessel(&mut self) -> Option<&mut Vessel> {
        Some(&mut self.vessel)
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.vessel.gas.o2, self.vessel.gas.n2, self.vessel.gas.co2]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, b, c, ..] = *s {
            self.vessel.gas = Mix { o2: a, n2: b, co2: c };
            self.told = -1.0;
        }
    }
}

/// What a place says of itself.
#[derive(Clone, Copy)]
struct PlaceSignals {
    p: SignalId,
    tight: SignalId,
    p_max: SignalId,
}

/// What it is doing this tick, from what its selectors and its places say.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Off,
    /// (mol/s it would move at full speed, W its motor takes for that)
    Pump(f64, f64),
    /// (mol/s through its by-pass)
    Bypass(f64),
    Idle(f64),
}

pub struct Compressor {
    ports: [PortSpec; 1],
    names: Vec<String>,
    places: Vec<PlaceSignals>,
    /// What its first stage sweeps (m³/s at its intake).
    flow: f64,
    motor: f64,
    eff: f64,
    stages: f64,
    clearance: f64,
    p_min: f64,
    intake_max: f64,
    room_max: f64,
    /// Area of its by-pass (m²).
    bypass: f64,
    standby: f64,
    // state
    speed: f64,
    limited: bool,
    full: bool,
    mode: Mode,
    from: usize,
    to: usize,
    rate: f64,
    power: f64,
    quiet: bool,
    cmd: SignalId,
    c_from: SignalId,
    c_to: SignalId,
    o_state: SignalId,
    o_flow: SignalId,
    o_ps: SignalId,
    o_pd: SignalId,
    o_power: SignalId,
    o_speed: SignalId,
}

impl Compressor {
    pub fn new(b: &mut Build) -> Result<Compressor, String> {
        let names: Vec<String> = match b.params.get("lugares") {
            Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            Some(_) => return Err(format!("{}: 'lugares' es una lista de nombres (compartimentos, depósitos)", b.id)),
            None => Vec::new(),
        };
        if names.len() < 2 {
            return Err(format!("{}: un compresor necesita al menos dos 'lugares' entre los que mover el aire", b.id));
        }
        let places = names.iter().map(|n| PlaceSignals { p: b.store.define(&format!("{n}.p")), tight: b.store.define(&format!("{n}.estanco")), p_max: b.store.define(&format!("{n}.p_max")) }).collect();
        let bore = b.q("paso", "m", 0.010)?;
        Ok(Compressor {
            ports: [PortSpec { role: "motor", medium: Medium::Electrico }],
            names,
            places,
            flow: b.q("caudal", "m3/s", 0.12)?,
            motor: b.q("potencia", "W", 15_000.0)?,
            // (over 1 it is no machine that could be built: the ships' data takes it there on
            // purpose, so that nobody waits half an hour for a hold to empty — docs/TIEMPOS.md)
            eff: b.f("eficiencia", 0.65).max(0.05),
            stages: b.f("etapas", 4.0).max(1.0),
            clearance: b.f("espacio_muerto", 0.06).clamp(0.0, 1.0),
            p_min: b.q("presion_minima", "Pa", 5e3)?.max(1.0),
            intake_max: b.q("admision_max", "Pa", 150e3)?,
            room_max: b.q("presion_max", "Pa", 101e3)?,
            bypass: std::f64::consts::PI * 0.25 * bore * bore,
            standby: b.q("potencia_reposo", "W", 40.0)?,
            speed: 0.0,
            limited: false,
            full: false,
            mode: Mode::Off,
            from: 0,
            to: 0,
            rate: 0.0,
            power: 0.0,
            quiet: false,
            cmd: b.cmd("marcha"),
            c_from: b.cmd("origen"),
            c_to: b.cmd("destino"),
            o_state: b.out("estado", "")?,
            o_flow: b.out("caudal", "g/s")?,
            o_ps: b.out("p_origen", "kPa")?,
            o_pd: b.out("p_destino", "kPa")?,
            o_power: b.out("potencia", "kW")?,
            o_speed: b.out("giro", "%")?,
        })
    }

    /// The lowest intake pressure it still moves gas from, against `pd` at its delivery: where
    /// its intake valves stop opening, or where the gas left in its clearances fills the stroke.
    pub fn ultimate(&self, pd: f64) -> f64 {
        let per_stage = (1.0 + 1.0 / self.clearance.max(1e-6)).powf(N_POLY);
        self.p_min.max(pd / per_stage.powf(self.stages))
    }

    /// What it moves (mol/s) and what its motor takes for it (W), from `ps` into `pd`, at full
    /// speed.
    pub fn pumping(&self, ps: f64, pd: f64) -> (f64, f64) {
        let ult = self.ultimate(pd);
        if ps <= ult {
            return (0.0, 0.0);
        }
        let r = (pd / ps).max(1.0);
        let per_stage = r.powf(1.0 / self.stages);
        let filled = (1.0 - self.clearance * (per_stage.powf(1.0 / N_POLY) - 1.0)).max(0.0);
        // (its valves open less and less over the last tenth above where they stop)
        let lift = ((ps - ult) / (0.1 * ult)).clamp(0.0, 1.0);
        let swept = self.flow * ps / (R * T_REF) * filled * lift;
        let work = R * T_REF * (r.ln() + LOSS);
        let rate = swept.min(self.motor * self.eff / work);
        (rate, rate * work / self.eff)
    }

    fn place(&self, cx: &Cx, sel: SignalId) -> usize {
        (cx.signals.get(sel).round().max(0.0) as usize).min(self.places.len() - 1)
    }
}

impl Machine for Compressor {
    fn kind(&self) -> &'static str {
        "compresor"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working || !cx.signals.on(self.cmd) {
            self.mode = Mode::Off;
            return;
        }
        self.quiet = false;
        (self.from, self.to) = (self.place(cx, self.c_from), self.place(cx, self.c_to));
        let (a, b) = (self.places[self.from], self.places[self.to]);
        let (ps, pd) = (cx.signals.get(a.p), cx.signals.get(b.p));
        let own = cx.signals.get(b.p_max);
        let limit = if own > 0.0 { own } else { self.room_max };
        // (a little slack either side of each limit, so it does not hunt)
        self.full = pd >= limit * if self.full { 0.98 } else { 1.0 };
        let ult = self.ultimate(pd);
        self.limited = ps <= ult * if self.limited { 1.08 } else { 1.02 };
        self.mode = if self.from == self.to {
            Mode::Idle(SAME)
        } else if !cx.signals.on(b.tight) {
            Mode::Idle(OPEN)
        } else if self.full {
            Mode::Idle(FULL)
        } else if ps > self.intake_max {
            // too much for its intake: through the by-pass, as long as it goes by itself
            if ps > pd { Mode::Bypass(orifice_mdot(self.bypass, ps, T_REF, M_AIR, pd) / M_AIR) } else { Mode::Idle(AT_LIMIT) }
        } else if self.limited {
            Mode::Idle(AT_LIMIT)
        } else {
            let (rate, power) = self.pumping(ps, pd);
            Mode::Pump(rate, power)
        };
        io[0].priority = 70;
        io[0].demand = self.standby
            + match self.mode {
                // the motor asks for the work it does (and a fifth of it to come up to speed)
                Mode::Pump(_, power) => power * self.speed.max(0.2),
                _ => 0.0,
            };
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        if self.mode == Mode::Off {
            if self.quiet {
                return;
            }
            // stopped: said once
            (self.speed, self.rate, self.power) = (0.0, 0.0, 0.0);
            let s = &mut cx.signals;
            for o in [self.o_state, self.o_flow, self.o_power, self.o_speed] {
                s.set(o, 0.0);
            }
            self.quiet = true;
            return;
        }
        let fed = io[0].fed && io[0].share >= 0.5;
        let health = 0.3 + 0.7 * f64::from(cx.health);
        let turning = fed && matches!(self.mode, Mode::Pump(..));
        self.speed = approach(self.speed, if turning { 1.0 } else { 0.0 }, 1.2, cx.dt);
        let (state, rate, power) = match self.mode {
            _ if !fed => (NO_POWER, 0.0, 0.0),
            Mode::Pump(rate, power) => (PUMPING, rate * self.speed * health, power * self.speed),
            Mode::Bypass(rate) => (BYPASS, rate, 0.0),
            Mode::Idle(why) => (why, 0.0, 0.0),
            Mode::Off => (STOPPED, 0.0, 0.0),
        };
        self.rate = rate;
        self.power = if fed { power + self.standby } else { 0.0 };
        let (a, b) = (self.places[self.from], self.places[self.to]);
        let (ps, pd) = (cx.signals.get(a.p), cx.signals.get(b.p));
        let s = &mut cx.signals;
        s.set(self.o_state, state);
        s.set(self.o_flow, rate * M_AIR);
        s.set(self.o_ps, ps);
        s.set(self.o_pd, pd);
        s.set(self.o_power, self.power);
        s.set(self.o_speed, self.speed);
    }
    fn heat(&self) -> f64 {
        // all the work of an isothermal compression leaves as heat
        self.power
    }
    fn places(&self) -> &[String] {
        &self.names
    }
    fn settling(&self) -> Option<bool> {
        // moving gas: it stops by itself where its source or its destination says
        Some(matches!(self.mode, Mode::Pump(..) | Mode::Bypass(..)))
    }
    fn moving(&self) -> Option<(usize, usize, f64)> {
        (self.rate > 0.0).then_some((self.from, self.to, self.rate))
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.speed);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, ..] = *s {
            self.speed = a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_signals::{Store, Writer};
    use serde_json::{Map, json};

    /// A compressor between a room of `vr` m³ (place 1) and a tank (place 0), their pressures in
    /// signals as a ship has them.
    struct Rig {
        store: Store,
        c: Compressor,
        io: [PortIo; 1],
        room: f64,
        tank: f64,
        vr: f64,
        vt: f64,
        t: f64,
    }

    fn rig(extra: Value) -> Rig {
        let mut store = Store::new();
        let mut params = Map::new();
        params.insert("lugares".into(), json!(["deposito", "sala"]));
        if let Value::Object(m) = extra {
            params.extend(m);
        }
        let orders = Map::new();
        let c = {
            let mut b = Build::new("compresor", &params, &orders, &mut store, Writer::Machine(0));
            Compressor::new(&mut b).unwrap()
        };
        for (name, v) in [("deposito.estanco", 1.0), ("sala.estanco", 1.0), ("deposito.p_max", 12e6)] {
            let id = store.define(name);
            store.set(id, v);
        }
        Rig { store, c, io: [PortIo::on(0, 0)], room: 70e3 * 58.0 / (R * T_REF), tank: 0.0, vr: 58.0, vt: 0.42, t: 0.0 }
    }

    impl Rig {
        fn set(&mut self, name: &str, v: f64) {
            let id = self.store.define(name);
            self.store.set(id, v);
        }
        fn p_room(&self) -> f64 {
            self.room * R * T_REF / self.vr
        }
        fn p_tank(&self) -> f64 {
            self.tank * R * T_REF / self.vt
        }
        /// One tick of `dt` with (or without) power; the moles it moved.
        fn tick(&mut self, dt: f64, powered: bool) -> f64 {
            let (pr, pt) = (self.p_room(), self.p_tank());
            self.set("sala.p", pr);
            self.set("deposito.p", pt);
            self.io[0].clear_ask();
            let mut cx = Cx { signals: &mut self.store, env: Default::default(), health: 1.0, working: true, t: self.t, dt };
            self.c.plan(&mut cx, &mut self.io);
            self.io[0].fed = powered;
            self.io[0].share = if powered { 1.0 } else { 0.0 };
            self.c.step(&mut cx, &self.io);
            self.t += dt;
            let Some((from, to, rate)) = self.c.moving() else { return 0.0 };
            let (src, dst) = if from == 1 { (&mut self.room, &mut self.tank) } else { (&mut self.tank, &mut self.room) };
            assert_ne!(from, to);
            let dn = (rate * dt).min(*src);
            *src -= dn;
            *dst += dn;
            dn
        }
    }

    #[test]
    fn it_pumps_a_room_down_slower_and_slower_and_stops_at_its_ultimate() {
        let mut r = rig(json!({}));
        r.set("compresor.origen", 1.0);
        r.set("compresor.destino", 0.0);
        r.set("compresor.marcha", 1.0);
        let n0 = r.room;
        let (mut rates, mut marks) = (Vec::new(), Vec::new());
        let mut energy = 0.0;
        let mut done = None;
        for k in 0..(90.0 * 60.0) as usize {
            let before = r.p_room();
            let dn = r.tick(1.0, true);
            energy += r.c.heat();
            for mark in [60e3, 40e3, 20e3, 10e3] {
                if before > mark && r.p_room() <= mark {
                    marks.push((mark / 1e3, k as f64 / 60.0));
                    rates.push(dn);
                }
            }
            if dn == 0.0 && k > 60 {
                done = Some(k as f64 / 60.0);
                break;
            }
        }
        eprintln!("70 kPa → {:.2} kPa in {done:?} min; tank {:.2} MPa; {:.2} kWh; recovered {:.1} %; marks (kPa, min) {marks:?}; mol/s there {rates:?}", r.p_room() / 1e3, r.p_tank() / 1e6, energy / 3.6e6, 100.0 * r.tank / n0);
        assert!(rates.windows(2).all(|w| w[1] < w[0]), "slower as the pressure drops: {rates:?}");
        let t = done.expect("it stops by itself");
        assert!((20.0..50.0).contains(&t), "{t} min");
        assert!((5.0e3..5.6e3).contains(&r.p_room()), "it stops at its ultimate pressure: {} Pa", r.p_room());
        assert_eq!(r.store.get(r.store.find("compresor.estado").unwrap()), AT_LIMIT);
        assert!(r.tank / n0 > 0.9, "most of the air recovered");
        // the motor never took more than it is rated for, and what it took is the work done
        assert!(energy / 3.6e6 > 4.0 && energy / 3.6e6 < 12.0, "{} kWh", energy / 3.6e6);
    }

    #[test]
    fn without_power_it_moves_nothing() {
        let mut r = rig(json!({}));
        r.set("compresor.origen", 1.0);
        r.set("compresor.marcha", 1.0);
        for _ in 0..200 {
            assert_eq!(r.tick(0.1, false), 0.0);
        }
        assert_eq!(r.store.get(r.store.find("compresor.estado").unwrap()), NO_POWER);
        // and stopped, it asks for nothing
        r.set("compresor.marcha", 0.0);
        r.tick(0.1, true);
        assert_eq!(r.io[0].demand, 0.0);
        assert_eq!(r.store.get(r.store.find("compresor.estado").unwrap()), STOPPED);
    }

    #[test]
    fn a_full_tank_stops_it_and_what_it_holds_goes_back_by_itself() {
        let mut r = rig(json!({}));
        r.tank = 11.99e6 * r.vt / (R * T_REF);
        r.set("compresor.origen", 1.0);
        r.set("compresor.marcha", 1.0);
        let mut moved = 0.0;
        for _ in 0..600 {
            moved += r.tick(0.1, true);
        }
        assert!(r.p_tank() <= 12.0e6 * 1.001 && moved > 0.0, "{} MPa", r.p_tank() / 1e6);
        assert_eq!(r.store.get(r.store.find("compresor.estado").unwrap()), FULL);
        // the other way: from the tank into the room, the motor does not turn
        r.room = 0.0;
        r.set("compresor.origen", 0.0);
        r.set("compresor.destino", 1.0);
        let dn = r.tick(0.1, true);
        assert!(dn > 0.0);
        assert_eq!(r.store.get(r.store.find("compresor.estado").unwrap()), BYPASS);
        assert!(r.c.heat() < 100.0, "only its valves: {} W", r.c.heat());
        // a room that is not tight is not filled
        r.set("sala.estanco", 0.0);
        assert_eq!(r.tick(0.1, true), 0.0);
        assert_eq!(r.store.get(r.store.find("compresor.estado").unwrap()), OPEN);
    }
}
