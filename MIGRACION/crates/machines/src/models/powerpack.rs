//! A unified power pack: one sealed unit that is reactor, coolant loop, converter and radiator
//! at once — what a small warship carries instead of a plant room. It has one electrical output
//! at bus voltage and asks for control power only to start; once on line it feeds itself.
//!
//! Made to be worked without a manual (`docs/COMBATE.md`):
//! - MARCHA starts it and PARO stops it: a stop is a stop, not a trip, and needs no rearming;
//! - what it cannot shed as heat it does not make: with its radiators stowed, or hurt, it gives
//!   less (`limitado`) long before it would trip;
//! - a trip (`estado` 3) says why (`causa`) and rearms by itself once it has cooled: if MARCHA
//!   is still up it starts again.
//!
//! Its heat is one lump: what the fission makes and the bus does not take warms it, its radiator
//! (so much of it as is out: `despliegue`) sheds it by Stefan–Boltzmann.
use crate::{
    machine::{Build, Cx, Machine, PortSpec, SIGMA, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

pub const OFF: f64 = 0.0;
pub const STARTING: f64 = 1.0;
pub const ONLINE: f64 = 2.0;
pub const TRIPPED: f64 = 3.0;

/// Why it tripped (`causa`): nothing, the button, too hot, hurt.
pub const BY_HAND: f64 = 1.0;
pub const TOO_HOT: f64 = 2.0;
pub const HURT: f64 = 3.0;

pub struct Unified {
    ports: [PortSpec; 2],
    /// Electrical output at full power (W), its voltage, and the share of its heat it turns
    /// into that.
    power: f64,
    volts: f64,
    efficiency: f64,
    /// Share of full power it makes with nothing asked of it (its own pumps, its control).
    idle: f64,
    start_time: f64,
    start_power: f64,
    /// Heat capacity of the lump (J/K).
    capacity: f64,
    /// Radiator: area out (m²), emissivity, and the share of it that sheds heat stowed.
    area: f64,
    emissivity: f64,
    stowed: f64,
    /// It trips over `t_max`, gives less from `t_limit` up, and rearms under `t_rearm` (K).
    t_max: f64,
    t_limit: f64,
    t_rearm: f64,
    // state
    state: f64,
    timer: f64,
    temp: f64,
    load: f64,
    cause: f64,
    // commands
    c_run: SignalId,
    c_power: Option<SignalId>,
    c_scram: SignalId,
    c_deploy: Option<SignalId>,
    // telemetry
    o_state: SignalId,
    o_cause: SignalId,
    o_p: SignalId,
    o_heat: SignalId,
    o_t: SignalId,
    o_load: SignalId,
    o_limit: SignalId,
    o_room: SignalId,
    o_rad: SignalId,
}

impl Unified {
    pub fn new(b: &mut Build) -> Result<Unified, String> {
        let t_max = b.q("t_max", "K", 1150.0)?;
        Ok(Unified {
            ports: [PortSpec { role: "salida", medium: Medium::Electrico }, PortSpec { role: "control", medium: Medium::Electrico }],
            power: b.q("potencia", "W", 40_000.0)?,
            volts: b.q("tension", "V", 28.0)?,
            efficiency: b.f("rendimiento", 0.3).clamp(0.05, 0.9),
            idle: b.f("ralenti", 0.04).clamp(0.0, 0.5),
            start_time: b.q("arranque", "s", 6.0)?.max(0.1),
            start_power: b.q("potencia_control", "W", 300.0)?,
            capacity: b.q("capacidad", "J/K", 40_000.0)?.max(1.0),
            area: b.q("area_radiador", "m2", 3.4)?,
            emissivity: b.f("emisividad", 0.9),
            stowed: b.f("replegado", 0.25).clamp(0.0, 1.0),
            t_max,
            t_limit: b.q("t_limite", "K", t_max - 170.0)?.min(t_max - 1.0),
            t_rearm: b.q("t_rearme", "K", 900.0)?,
            state: OFF,
            timer: 0.0,
            temp: 293.15,
            load: 0.0,
            cause: 0.0,
            c_run: b.cmd("marcha"),
            c_power: b.orders.get("potencia").and_then(|v| v.as_str()).map(|n| b.store.define(n)),
            c_scram: b.cmd("scram"),
            c_deploy: b.text("despliegue").map(str::to_string).map(|n| b.store.define(&n)),
            o_state: b.out("estado", "")?,
            o_cause: b.out("causa", "")?,
            o_p: b.out("p", "kW")?,
            o_heat: b.out("p_termica", "kW")?,
            o_t: b.out("t", "°C")?,
            o_load: b.out("carga", "%")?,
            o_limit: b.out("limitado", "")?,
            o_room: b.out("disponible", "kW")?,
            o_rad: b.out("radiador", "%")?,
        })
    }

    /// Share of its radiator that sheds heat now.
    fn out(&self, cx: &Cx) -> f64 {
        let d = self.c_deploy.map_or(1.0, |s| cx.signals.get(s).clamp(0.0, 1.0));
        self.stowed + (1.0 - self.stowed) * d
    }

    /// Share of full power it may give at this temperature: all of it under `t_limit`, a tenth
    /// at `t_max`.
    fn room(&self) -> f64 {
        ((self.t_max - self.temp) / (self.t_max - self.t_limit)).clamp(0.1, 1.0)
    }

    /// What it can give now (W).
    fn available(&self, cx: &Cx) -> f64 {
        let asked = self.c_power.map_or(1.0, |s| cx.signals.get(s).clamp(0.0, 1.1));
        self.power * asked * self.room() * (0.4 + 0.6 * f64::from(cx.health))
    }

    fn trip(&mut self, cause: f64) {
        if self.state != TRIPPED {
            self.state = TRIPPED;
            self.cause = cause;
            self.timer = 0.0;
        }
    }
}

impl Machine for Unified {
    fn kind(&self) -> &'static str {
        "reactor_unificado"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if self.state == STARTING {
            io[1].demand = self.start_power;
            io[1].priority = 240;
        }
        if self.state == ONLINE {
            io[0].produce = self.available(cx);
            io[0].potential = self.volts;
            io[0].regulated = true;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let dt = cx.dt;
        let run = cx.signals.on(self.c_run) && cx.working;
        let pressed = cx.signals.on(self.c_scram);
        if pressed && self.state != OFF {
            self.trip(BY_HAND);
        }
        match self.state {
            s if s == OFF => {
                self.cause = if pressed { BY_HAND } else { self.cause };
                if run && !pressed {
                    self.state = STARTING;
                    self.timer = 0.0;
                    self.cause = 0.0;
                }
            }
            s if s == STARTING => {
                if !run {
                    self.state = OFF;
                } else if io[1].fed && io[1].share >= 0.5 {
                    self.timer += dt;
                    if self.timer >= self.start_time {
                        self.state = ONLINE;
                    }
                } else {
                    // (no control power: it waits, it does not trip)
                    self.timer = 0.0;
                }
            }
            s if s == ONLINE => {
                if !run {
                    self.state = OFF;
                } else if self.temp > self.t_max {
                    self.trip(TOO_HOT);
                } else if cx.health < 0.15 {
                    self.trip(HURT);
                }
            }
            _ => {
                // tripped: it rearms by itself once cool, with the button let go and (if it was
                // hurt) mended
                if !pressed && self.temp < self.t_rearm && cx.health >= 0.15 {
                    self.state = OFF;
                }
            }
        }
        // heat: what it makes less what the bus takes, out by its radiator
        let gave = if self.state == ONLINE { io[0].gave.max(0.0) } else { 0.0 };
        let thermal = if self.state == ONLINE {
            (gave / self.efficiency).max(self.power * self.idle / self.efficiency)
        } else if self.state == STARTING {
            self.power * self.idle / self.efficiency * (self.timer / self.start_time)
        } else {
            0.0
        };
        let out = self.out(cx);
        let shed = self.emissivity * SIGMA * self.area * out * (self.temp.powi(4) - cx.env.sink_temp.powi(4));
        self.temp = (self.temp + (thermal - gave - shed) * dt / self.capacity).max(cx.env.sink_temp.min(293.15));
        let cap = self.power.max(1.0);
        self.load = approach(self.load, gave / cap, 0.6, dt);
        let room = if self.state == ONLINE { self.available(cx) } else { 0.0 };
        let s = &mut cx.signals;
        s.set(self.o_state, self.state);
        s.set(self.o_cause, self.cause);
        s.set(self.o_p, gave);
        s.set(self.o_heat, thermal);
        s.set(self.o_t, self.temp);
        s.set(self.o_load, self.load);
        s.set(self.o_limit, if self.state == ONLINE && self.room() < 0.98 { 1.0 } else { 0.0 });
        s.set(self.o_room, room);
        s.set(self.o_rad, out);
    }
    fn heat(&self) -> f64 {
        // what its shield lets through into where it stands
        if self.state == ONLINE { self.power * 0.004 } else { 0.0 }
    }
    fn settling(&self) -> Option<bool> {
        Some(self.state == STARTING)
    }
    fn rupture(&self) -> f64 {
        // its coolant flashing: a small blast, more if it was hot
        2.0e6 + (self.temp - 293.15).max(0.0) * self.capacity * 0.05
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.state, self.timer, self.temp, self.load, self.cause]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [st, ti, te, lo, ca, ..] = *s {
            (self.state, self.timer, self.temp, self.load, self.cause) = (st, ti, te, lo, ca);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Env;
    use lunar_signals::{Store, Writer};
    use serde_json::{Map, json};

    struct Rig {
        store: Store,
        m: Unified,
        io: [PortIo; 2],
        t: f64,
        run: SignalId,
        scram: SignalId,
        out: SignalId,
    }

    fn rig() -> Rig {
        let mut store = Store::new();
        let mut params = Map::new();
        params.insert("despliegue".into(), json!("rad.pos"));
        let orders = Map::new();
        let m = Unified::new(&mut Build::new("rx", &params, &orders, &mut store, Writer::Machine(0))).unwrap();
        let (run, scram, out) = (store.define("rx.marcha"), store.define("rx.scram"), store.define("rad.pos"));
        Rig { store, m, io: [PortIo::on(0, 0), PortIo::on(0, 1)], t: 0.0, run, scram, out }
    }

    impl Rig {
        /// `secs` with the bus taking `take` W (or all it gives, if less) and control power there.
        fn go(&mut self, secs: f64, take: f64) {
            let dt = 0.02;
            for _ in 0..(secs / dt) as usize {
                for p in &mut self.io {
                    p.clear_ask();
                }
                let mut cx = Cx { signals: &mut self.store, env: Env::default(), health: 1.0, working: true, t: self.t, dt };
                self.m.plan(&mut cx, &mut self.io);
                self.io[0].gave = take.min(self.io[0].produce);
                self.io[1].fed = true;
                self.io[1].share = 1.0;
                self.m.step(&mut cx, &self.io);
                self.t += dt;
            }
        }
        fn get(&self, name: &str) -> f64 {
            self.store.get(self.store.find(name).unwrap())
        }
    }

    #[test]
    fn starts_in_seconds_and_stops_without_a_trip() {
        let mut r = rig();
        r.store.set(r.run, 1.0);
        r.store.set(r.out, 1.0);
        r.go(7.0, 0.0);
        assert_eq!(r.get("rx.estado"), ONLINE);
        r.go(120.0, 40_000.0);
        assert_eq!(r.get("rx.estado"), ONLINE, "a plena potencia con el radiador fuera se queda en línea ({} °C)", r.get("rx.t") - 273.15);
        assert!(r.get("rx.limitado") < 0.5);
        // PARO: off at once, no cause, and MARCHA starts it again with nothing to rearm
        r.store.set(r.run, 0.0);
        r.go(0.1, 0.0);
        assert_eq!(r.get("rx.estado"), OFF);
        assert_eq!(r.get("rx.causa"), 0.0);
        r.store.set(r.run, 1.0);
        r.go(7.0, 0.0);
        assert_eq!(r.get("rx.estado"), ONLINE);
    }

    #[test]
    fn stowed_it_gives_less_instead_of_tripping() {
        let mut r = rig();
        r.store.set(r.run, 1.0);
        r.store.set(r.out, 0.0);
        r.go(600.0, 40_000.0);
        assert_eq!(r.get("rx.estado"), ONLINE, "{} °C", r.get("rx.t") - 273.15);
        assert!(r.get("rx.limitado") > 0.5);
        let p = r.get("rx.p");
        assert!(p > 8_000.0 && p < 34_000.0, "replegado da {p} W");
    }

    #[test]
    fn a_trip_says_why_and_rearms_itself() {
        let mut r = rig();
        r.store.set(r.run, 1.0);
        r.store.set(r.out, 1.0);
        r.go(30.0, 30_000.0);
        r.store.set(r.scram, 1.0);
        r.go(0.1, 0.0);
        assert_eq!(r.get("rx.estado"), TRIPPED);
        assert_eq!(r.get("rx.causa"), BY_HAND);
        // let go: cool, rearmed and, MARCHA still up, on line again within the minute
        r.store.set(r.scram, 0.0);
        r.go(45.0, 0.0);
        assert_eq!(r.get("rx.estado"), ONLINE);
    }
}
