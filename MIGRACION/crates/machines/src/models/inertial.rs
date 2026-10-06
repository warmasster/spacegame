//! Machines that turn or hold without throwing mass overboard:
//!
//! - `giroscopo`: a cluster of momentum wheels (reaction wheels, or control moment gyros: the
//!   model is the same seen from the hull). It turns the hull by storing the opposite momentum:
//!   a torque τ on the hull changes what it holds by −τ·dt. It can give at most `par` and hold
//!   at most `momento` on each axis; full on an axis, it gives nothing more that way (the hull
//!   stops answering) until something else — thrusters — turns the hull the other way while the
//!   wheels unload. Without power the wheels run down on their bearings and hand what they held
//!   back to the hull: it starts to turn by itself. (ISS: four gyros of 4 760 N·m·s and 258 N·m;
//!   one's bearing failing stopped it in 72 minutes instead of 12 hours. NASA TM 20100021932.)
//! - `electroiman`: an electropermanent magnet. A pulse through its coil flips a magnet inside
//!   it: on, the flux goes out through its poles and holds; off, it closes inside. Holding costs
//!   nothing — it holds with the ship dead — but every switch, on or off, takes a pulse from its
//!   capacitor, which the bus charges: with no charge it can neither take nor let go. (Knaian,
//!   "Electropermanent magnetic connectors and actuators", MIT 2010; OpenGrab EPM v3.)
use crate::{
    machine::{Build, Cx, Machine, PortSpec, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

pub struct Gyro {
    ports: [PortSpec; 1],
    /// Momentum it holds per axis at most (N·m·s), torque it gives at most (N·m).
    h_max: f64,
    t_max: f64,
    /// Power to keep the wheels turning and at full torque (W); seconds to spin up; seconds its
    /// momentum takes to leak into the hull when it is not fed.
    p_idle: f64,
    p_torque: f64,
    spin_up: f64,
    run_down: f64,
    priority: u8,
    always: bool,
    // state
    h: [f64; 3],
    /// How fast the wheels turn (0 stopped .. 1 at speed): the torque it can give goes with it.
    speed: f64,
    /// On the hull now (N·m, its part's axes).
    tau: [f64; 3],
    cmd_on: SignalId,
    cmd: [SignalId; 3],
    o_h: [SignalId; 3],
    o_load: SignalId,
    o_speed: SignalId,
    o_on: SignalId,
}

impl Gyro {
    pub fn new(b: &mut Build) -> Result<Gyro, String> {
        Ok(Gyro {
            ports: [PortSpec { role: "energia", medium: Medium::Electrico }],
            h_max: b.f("momento", 2000.0).max(1e-3),
            t_max: b.f("par", 200.0).max(1e-3),
            p_idle: b.q("potencia_reposo", "W", 80.0)?,
            p_torque: b.q("potencia_par", "W", 400.0)?,
            spin_up: b.q("arranque", "s", 20.0)?,
            run_down: b.q("parada", "s", 240.0)?,
            priority: b.f("prioridad", 2.0) as u8,
            always: b.flag("siempre", false),
            h: [0.0; 3],
            speed: 0.0,
            tau: [0.0; 3],
            cmd_on: b.cmd("marcha"),
            cmd: [b.cmd("par_x"), b.cmd("par_y"), b.cmd("par_z")],
            o_h: [b.out("h_x", "")?, b.out("h_y", "")?, b.out("h_z", "")?],
            o_load: b.out("carga", "%")?,
            o_speed: b.out("giro", "%")?,
            o_on: b.out("on", "")?,
        })
    }

    /// The torque it would give on one axis for `want`, holding `h` there: capped, and nothing
    /// in the direction it is full in.
    fn give(&self, want: f64, h: f64, dt: f64) -> f64 {
        let cap = self.t_max * self.speed;
        let t = want.clamp(-cap, cap);
        // the hull's torque takes the opposite out of the wheels
        let after = h - t * dt;
        if after.abs() <= self.h_max { t } else { (h - after.clamp(-self.h_max, self.h_max)) / dt.max(1e-9) }
    }
}

impl Machine for Gyro {
    fn kind(&self) -> &'static str {
        "giroscopo"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if cx.working && (self.always || cx.signals.on(self.cmd_on)) {
            let effort = self.tau.iter().map(|t| t.abs()).sum::<f64>() / self.t_max;
            io[0].demand = self.p_idle + self.p_torque * effort.min(1.5) + if self.speed < 0.98 { self.p_torque } else { 0.0 };
            io[0].priority = self.priority;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let fed = cx.working && (self.always || cx.signals.on(self.cmd_on)) && io[0].fed && io[0].share >= 0.5;
        self.speed = approach(self.speed, if fed { 1.0 } else { 0.0 }, if fed { self.spin_up / 3.0 } else { self.run_down / 3.0 }, cx.dt);
        for k in 0..3 {
            let t = if fed {
                self.give(cx.signals.get(self.cmd[k]), self.h[k], cx.dt)
            } else {
                // running down: what it holds goes back into the hull (a wrecked one, at once)
                let tau = if cx.working { self.run_down } else { 2.0 };
                self.h[k] * (1.0 - (-cx.dt / tau).exp()) / cx.dt.max(1e-9)
            };
            self.h[k] -= t * cx.dt;
            self.tau[k] = t;
            cx.signals.set(self.o_h[k], self.h[k]);
        }
        let load = self.h.iter().fold(0.0f64, |m, h| m.max(h.abs())) / self.h_max;
        cx.signals.set(self.o_load, load);
        cx.signals.set(self.o_speed, self.speed);
        cx.signals.set(self.o_on, if fed { 1.0 } else { 0.0 });
    }
    fn torque(&self) -> [f64; 3] {
        self.tau
    }
    fn heat(&self) -> f64 {
        self.p_idle * self.speed * 0.5
    }
    fn settling(&self) -> Option<bool> {
        // its wheels coming up to speed (or running down)
        Some(self.speed > 0.02 && self.speed < 0.95)
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend_from_slice(&self.h);
        out.push(self.speed);
    }
    fn load(&mut self, s: &[f64]) {
        if let [x, y, z, speed, ..] = *s {
            self.h = [x, y, z];
            self.speed = speed;
        }
    }
}

pub struct Magnet {
    ports: [PortSpec; 1],
    /// Its capacitor (J), what a switch takes of it (J) and how fast the bus fills it (W).
    capacity: f64,
    pulse: f64,
    charge_power: f64,
    // state
    charge: f64,
    on: bool,
    cmd: SignalId,
    o_on: SignalId,
    o_charge: SignalId,
    o_ready: SignalId,
}

impl Magnet {
    pub fn new(b: &mut Build) -> Result<Magnet, String> {
        let capacity = b.f("condensador", 1500.0).max(1.0);
        Ok(Magnet {
            ports: [PortSpec { role: "energia", medium: Medium::Electrico }],
            capacity,
            pulse: b.f("pulso", 600.0).clamp(0.0, capacity),
            charge_power: b.q("potencia_carga", "W", 300.0)?,
            charge: capacity * b.f("carga_inicial", 1.0).clamp(0.0, 1.0),
            on: b.flag("activo", false),
            cmd: b.cmd("agarrar"),
            o_on: b.out("activo", "")?,
            o_charge: b.out("carga", "%")?,
            o_ready: b.out("listo", "")?,
        })
    }
}

impl Machine for Magnet {
    fn kind(&self) -> &'static str {
        "electroiman"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if cx.working && self.charge < self.capacity {
            io[0].demand = self.charge_power;
            io[0].priority = 1;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        if cx.working {
            self.charge = (self.charge + io[0].got * cx.dt).min(self.capacity);
            // a switch either way is a pulse; without it the magnet stays as it is
            if cx.signals.on(self.cmd) != self.on && self.charge >= self.pulse {
                self.on = !self.on;
                self.charge -= self.pulse;
            }
        }
        cx.signals.set(self.o_on, if self.on { 1.0 } else { 0.0 });
        cx.signals.set(self.o_charge, self.charge / self.capacity);
        cx.signals.set(self.o_ready, if cx.working && self.charge >= self.pulse { 1.0 } else { 0.0 });
    }
    fn settling(&self) -> Option<bool> {
        // its capacitor short of a pulse: it can neither take nor let go until it has charged
        Some(self.charge < self.pulse)
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.charge);
        out.push(f64::from(u8::from(self.on)));
    }
    fn load(&mut self, s: &[f64]) {
        if let [charge, on, ..] = *s {
            self.charge = charge;
            self.on = on >= 0.5;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Env;
    use lunar_signals::{Store, Writer};
    use serde_json::{Map, Value, json};

    fn fed(demand: f64) -> [PortIo; 1] {
        [PortIo { got: demand, share: 1.0, fed: true, level: 28.0, ..PortIo::on(0, 0) }]
    }

    fn params(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn a_gyro_turns_the_hull_until_it_is_full_and_hands_it_back_when_it_dies() {
        let mut store = Store::default();
        let (p, o) = (params(json!({ "momento": 100.0, "par": 50.0, "arranque": "3 s", "parada": "30 s", "siempre": true })), Map::new());
        let mut g = Gyro::new(&mut Build::new("giro", &p, &o, &mut store, Writer::Machine(0))).unwrap();
        let want = store.define("giro.par_x");
        let step = |g: &mut Gyro, store: &mut Store, power: bool, working: bool| {
            let mut io = if power { fed(500.0) } else { [PortIo::on(0, 0)] };
            let mut cx = Cx { signals: store, env: Env::default(), health: 1.0, working, t: 0.0, dt: 0.02 };
            g.plan(&mut cx, &mut io);
            if power {
                io[0].got = io[0].demand;
            }
            g.step(&mut cx, &io);
        };
        // spun up, it gives what is asked up to its torque
        for _ in 0..500 {
            step(&mut g, &mut store, true, true);
        }
        store.set(want, 80.0);
        step(&mut g, &mut store, true, true);
        assert!((g.torque()[0] - 50.0).abs() < 0.5, "da {:.1} N·m", g.torque()[0]);
        // and takes the opposite momentum, until it is full: then nothing more that way
        for _ in 0..200 {
            step(&mut g, &mut store, true, true);
        }
        assert!((g.h[0] + 100.0).abs() < 1e-6 && g.torque()[0].abs() < 1e-6, "h {:.1}, par {:.1}", g.h[0], g.torque()[0]);
        assert!((store.get(store.find("giro.carga").unwrap()) - 1.0).abs() < 1e-6);
        // the other way it answers at once (and unloads)
        store.set(want, -80.0);
        step(&mut g, &mut store, true, true);
        assert!((g.torque()[0] + 50.0).abs() < 0.5 && g.h[0] > -100.0);
        // unpowered, what it holds goes back into the hull little by little: the sum is its momentum
        store.set(want, 0.0);
        let held = g.h[0];
        let mut given = 0.0;
        for _ in 0..20_000 {
            step(&mut g, &mut store, false, true);
            given += g.torque()[0] * 0.02;
        }
        assert!((given - held).abs() < 0.5 && g.h[0].abs() < 0.5, "devuelve {given:.1} de {held:.1}");
    }

    #[test]
    fn a_magnet_holds_without_power_and_switches_only_with_a_charged_pulse() {
        let mut store = Store::default();
        let (p, o) = (params(json!({ "condensador": 1000.0, "pulso": 600.0, "potencia_carga": "200 W" })), Map::new());
        let mut m = Magnet::new(&mut Build::new("iman", &p, &o, &mut store, Writer::Machine(0))).unwrap();
        let (cmd, on) = (store.define("iman.agarrar"), store.find("iman.activo").unwrap());
        let step = |m: &mut Magnet, store: &mut Store, power: bool| {
            let mut io = [PortIo { fed: power, share: 1.0, ..PortIo::on(0, 0) }];
            let mut cx = Cx { signals: store, env: Env::default(), health: 1.0, working: true, t: 0.0, dt: 0.02 };
            m.plan(&mut cx, &mut io);
            io[0].got = if power { io[0].demand } else { 0.0 };
            m.step(&mut cx, &io);
        };
        // charged as built: on at the first asking, and it stays on with the ship dead
        store.set(cmd, 1.0);
        step(&mut m, &mut store, false);
        assert_eq!(store.get(on), 1.0);
        for _ in 0..500 {
            step(&mut m, &mut store, false);
        }
        assert_eq!(store.get(on), 1.0, "sin corriente sigue sujetando");
        // but it cannot let go: the pulse took most of its charge and nothing fills it
        store.set(cmd, 0.0);
        for _ in 0..50 {
            step(&mut m, &mut store, false);
        }
        assert_eq!(store.get(on), 1.0, "suelta sin carga en el condensador");
        // fed, it charges (200 J short at 200 W: a second) and then lets go
        let mut steps = 0;
        while store.get(on) > 0.5 && steps < 500 {
            step(&mut m, &mut store, true);
            steps += 1;
        }
        assert!((40..=60).contains(&steps), "tarda {steps} pasos en poder soltar");
    }
}
