//! Rockets (`docs/MANDOS_Y_MAQUINAS.md` §5.1): the main engine with its start sequence (purge,
//! ignition, spool-up), throttling between its limits, feed pressure that must beat the chamber,
//! a wall that heats and cuts the engine when too hot, a thrust fingerprint of its own (the
//! tolerances of its build: no two engines push alike) and the risk of a damaged one blowing up;
//! and RCS thrusters, one nozzle each.
use crate::{
    machine::{Build, Cx, G0, Machine, PortSpec, approach, interp},
    net::{Medium, PortIo},
    wear::Rng,
};
use lunar_signals::SignalId;

pub const OFF: f64 = 0.0;
pub const PURGE: f64 = 1.0;
pub const IGNITION: f64 = 2.0;
pub const SPOOL: f64 = 3.0;
pub const RUNNING: f64 = 4.0;
pub const SPINDOWN: f64 = 5.0;
pub const FAILED: f64 = 6.0;

pub struct Rocket {
    ports: [PortSpec; 3],
    f_vac: f64,
    isp_vac: f64,
    isp_sl: f64,
    pc0: f64,
    mix: f64,
    t_min: f64,
    t_max: f64,
    isp_curve: Vec<(f64, f64)>,
    t_purge: f64,
    tau_up: f64,
    tau_down: f64,
    igniter: f64,
    valves: f64,
    t_eq: f64,
    tau_wall: f64,
    t_warn: f64,
    t_cut: f64,
    t_harm: f64,
    print: [f64; 8],
    pulse_mode: bool,
    // state
    state: f64,
    timer: f64,
    level: f64,
    t_wall: f64,
    restarts: f64,
    thrust: f64,
    flow: f64,
    rng: Rng,
    blown: bool,
    // commands
    c_arm: SignalId,
    c_start: SignalId,
    c_throttle: SignalId,
    c_stop: SignalId,
    // telemetry
    o_f: SignalId,
    o_m: SignalId,
    o_pc: SignalId,
    o_tw: SignalId,
    o_state: SignalId,
    o_isp: SignalId,
    o_restarts: SignalId,
    o_on: SignalId,
}

impl Rocket {
    pub fn new(b: &mut Build) -> Result<Rocket, String> {
        let print = match b.params.get("huella") {
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("{}: huella: {e}", b.id))?,
            None => [0.0; 8],
        };
        let lim = b.curve("estrangulamiento_curva", &[(0.4, 0.96), (1.0, 1.0)])?;
        let throttle: [f64; 2] = match b.params.get("estrangulamiento") {
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("{}: estrangulamiento: {e}", b.id))?,
            None => [0.4, 1.05],
        };
        let seed = b.f("semilla", 1.0) as u64;
        Ok(Rocket {
            ports: [
                PortSpec { role: "energia", medium: Medium::Electrico },
                PortSpec { role: "propelente", medium: Medium::Propelente },
                PortSpec { role: "oxidante", medium: Medium::Propelente },
            ],
            f_vac: b.q("empuje_vacio", "N", 45_000.0)?,
            isp_vac: b.q("isp_vacio", "s", 311.0)?,
            isp_sl: b.q("isp_mar", "s", 265.0)?,
            pc0: b.q("pc_nominal", "Pa", 1.0e6)?,
            mix: b.f("mezcla", 0.0),
            t_min: throttle[0],
            t_max: throttle[1],
            isp_curve: lim,
            t_purge: b.q("t_purga", "s", 0.6)?,
            tau_up: b.q("tau_subida", "s", 0.8)?,
            tau_down: b.q("tau_bajada", "s", 0.35)?,
            igniter: b.q("encendedor", "W", 150.0)?,
            valves: b.q("valvulas", "W", 40.0)?,
            t_eq: b.q("t_equilibrio", "K", 1000.0)?,
            tau_wall: b.q("tau_pared", "s", 6.0)?,
            t_warn: b.q("t_aviso", "K", 1100.0)?,
            t_cut: b.q("t_corte", "K", 1250.0)?,
            t_harm: b.q("t_dano", "K", 1400.0)?,
            print,
            pulse_mode: b.flag("arranque_pulso", false),
            state: OFF,
            timer: 0.0,
            level: 0.0,
            t_wall: 250.0,
            restarts: b.f("reencendidos", 12.0),
            thrust: 0.0,
            flow: 0.0,
            rng: Rng::new(seed ^ 0x5eed_e61e),
            blown: false,
            c_arm: b.cmd("armado"),
            c_start: b.cmd("arranque"),
            c_throttle: b.cmd("acelerador"),
            c_stop: b.cmd("paro"),
            o_f: b.out("empuje", "kN")?,
            o_m: b.out("caudal", "kg/s")?,
            o_pc: b.out("pc", "MPa")?,
            o_tw: b.out("t_pared", "°C")?,
            o_state: b.out("estado", "")?,
            o_isp: b.out("isp", "s")?,
            o_restarts: b.out("reencendidos", "")?,
            o_on: b.out("encendido", "")?,
        })
    }

    fn m0(&self) -> f64 {
        self.f_vac / (self.isp_vac * G0)
    }

    fn throttle(&self, cx: &Cx) -> f64 {
        cx.signals.get(self.c_throttle).clamp(self.t_min, self.t_max)
    }

    /// Feed pressure: the weaker line.
    fn feed(&self, io: &[PortIo]) -> f64 {
        if self.mix > 0.0 { io[1].level.min(io[2].level) } else { io[1].level }
    }
}

impl Machine for Rocket {
    fn kind(&self) -> &'static str {
        "motor_cohete"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let armed = cx.signals.on(self.c_arm) && cx.working;
        if armed {
            io[0].demand = self.valves + if self.state == IGNITION { self.igniter } else { 0.0 };
            io[0].priority = 230;
        }
        // propellant: what the level asks (the purge bleeds a little)
        let want = match self.state {
            s if s == PURGE => 0.05,
            s if s == IGNITION => 0.2,
            s if s == SPOOL || s == RUNNING || s == SPINDOWN => self.level.max(0.05),
            _ => 0.0,
        };
        let m = self.m0() * want;
        if m > 0.0 {
            if self.mix > 0.0 {
                io[1].demand = m / (1.0 + self.mix);
                io[2].demand = m * self.mix / (1.0 + self.mix);
                io[2].priority = 180;
            } else {
                io[1].demand = m;
            }
            io[1].priority = 180;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let dt = cx.dt;
        let armed = cx.signals.on(self.c_arm) && cx.working;
        let start_cmd = cx.signals.get(self.c_start);
        let start = if self.pulse_mode { start_cmd >= 0.5 } else { start_cmd >= 1.5 };
        let stop = !armed || cx.signals.on(self.c_stop) || (!self.pulse_mode && start_cmd < 0.5);
        let power = io[0].fed && io[0].share >= 0.5;
        let fed = if self.mix > 0.0 { io[1].share.min(io[2].share) } else { io[1].share };
        let t = self.throttle(cx);
        let pc = self.pc0 * self.level;
        let p_need = 1.2 * self.pc0 * t;
        let feed_ok = (self.feed(io) / p_need.max(1.0)).clamp(0.0, 1.0);
        self.timer += dt;
        match self.state {
            s if s == OFF => {
                if armed && start && self.restarts > 0.0 && power {
                    self.state = PURGE;
                    self.timer = 0.0;
                }
            }
            s if s == PURGE => {
                if stop {
                    self.state = OFF;
                } else if self.timer >= self.t_purge {
                    self.state = IGNITION;
                    self.timer = 0.0;
                }
            }
            s if s == IGNITION => {
                if self.timer >= 0.4 {
                    if power && feed_ok > 0.5 && fed > 0.5 {
                        self.state = SPOOL;
                        self.restarts -= 1.0;
                    } else {
                        self.state = FAILED;
                    }
                    self.timer = 0.0;
                }
            }
            s if s == SPOOL || s == RUNNING => {
                if stop {
                    self.state = SPINDOWN;
                } else if self.t_wall > self.t_cut {
                    self.state = SPINDOWN;
                } else if fed < 0.3 || feed_ok < 0.3 {
                    // starved: flame-out
                    self.state = FAILED;
                } else if self.state == SPOOL && self.level >= t * 0.95 {
                    self.state = RUNNING;
                }
            }
            s if s == SPINDOWN => {
                if self.level < 0.01 {
                    self.state = OFF;
                }
            }
            s if s == FAILED => {
                if !armed {
                    self.state = OFF;
                }
            }
            _ => {}
        }
        // the flow follows the throttle (or dies away), limited by feed
        let target = if self.state == SPOOL || self.state == RUNNING { t * feed_ok.max(0.0) * fed.clamp(0.0, 1.0) } else { 0.0 };
        let tau = if target > self.level { self.tau_up } else { self.tau_down };
        self.level = approach(self.level, target, tau, dt);
        if self.level < 1e-4 {
            self.level = 0.0;
        }
        // thrust: ṁ·Isp·g0, Isp by throttle and ambient pressure, the engine's own fingerprint on top
        let ambient = (cx.env.pressure / 101_325.0).clamp(0.0, 1.0);
        let isp = (self.isp_vac + (self.isp_sl - self.isp_vac) * ambient) * interp(&self.isp_curve, self.level.max(self.t_min));
        self.flow = self.m0() * self.level;
        let wobble: f64 = self.print.iter().enumerate().map(|(k, w)| w * (cx.t * std::f64::consts::TAU * (3.0 + 2.7 * k as f64) + k as f64 * 1.3).sin()).sum::<f64>() * 0.004;
        let health = 0.25 + 0.75 * f64::from(cx.health);
        self.thrust = self.flow * isp * G0 * (1.0 + wobble) * health;
        // the wall
        let t_target = 250.0 + (self.t_eq - 250.0) * self.level.powf(0.8) * (1.0 + 0.6 * (self.level - 1.0).max(0.0) * 5.0);
        let tau_w = if t_target > self.t_wall { self.tau_wall } else { self.tau_wall * 6.0 };
        self.t_wall = approach(self.t_wall, t_target, tau_w, dt);
        // a damaged engine that runs may blow up
        let risky = cx.health < 0.3 && self.level > 0.01;
        if risky && self.rng.f64() < dt * 0.04 * (1.3 - f64::from(cx.health)) {
            self.blown = true;
        }
        let s = &mut cx.signals;
        s.set(self.o_f, self.thrust);
        s.set(self.o_m, self.flow);
        s.set(self.o_pc, pc);
        s.set(self.o_tw, self.t_wall);
        s.set(self.o_state, self.state);
        s.set(self.o_isp, if self.level > 0.0 { isp } else { 0.0 });
        s.set(self.o_restarts, self.restarts);
        s.set(self.o_on, if self.level > 0.05 { 1.0 } else { 0.0 });
    }
    fn thrust(&self) -> f64 {
        self.thrust
    }
    fn heat(&self) -> f64 {
        self.flow * 2.0e4
    }
    fn harm(&self) -> f32 {
        if self.t_wall > self.t_harm { ((self.t_wall - self.t_harm) / 400.0).min(0.2) as f32 } else { 0.0 }
    }
    fn settling(&self) -> Option<bool> {
        // its start sequence, and running down
        Some(self.state == PURGE || self.state == IGNITION || self.state == SPOOL || self.state == SPINDOWN)
    }
    fn take_burst(&mut self) -> f64 {
        if self.blown {
            self.blown = false;
            self.state = FAILED;
            self.level = 0.0;
            return 4.0e7;
        }
        0.0
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.state, self.timer, self.level, self.t_wall, self.restarts, self.rng.0 as f64, f64::from(u8::from(self.blown))]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [st, tm, lv, tw, rs, rng, bl, ..] = *s {
            (self.state, self.timer, self.level, self.t_wall, self.restarts, self.rng.0, self.blown) = (st, tm, lv, tw, rs, rng as u64, bl > 0.5);
        }
    }
}

impl Rocket {
    /// Over its warning temperature.
    pub fn warning(&self) -> bool {
        self.t_wall > self.t_warn
    }
}

/// One RCS nozzle: thrust by its command (0..1, from the flight computer), with a minimum
/// impulse bit (short commands pulse), fed from its line.
pub struct Thruster {
    ports: [PortSpec; 2],
    force: f64,
    isp: f64,
    heater: f64,
    min_on: f64,
    duty: f64,
    pulse: f64,
    thrust: f64,
    cmd: SignalId,
    master: SignalId,
    o_f: SignalId,
}

impl Thruster {
    pub fn new(b: &mut Build) -> Result<Thruster, String> {
        Ok(Thruster {
            ports: [PortSpec { role: "propelente", medium: Medium::Propelente }, PortSpec { role: "energia", medium: Medium::Electrico }],
            force: b.q("empuje", "N", 3500.0)?,
            isp: b.q("isp", "s", 280.0)?,
            heater: b.q("calefactor", "W", 25.0)?,
            min_on: b.q("pulso_min", "s", 0.02)?,
            duty: 0.0,
            pulse: 0.0,
            thrust: 0.0,
            cmd: b.cmd("orden"),
            master: b.cmd("maestro"),
            o_f: b.out("empuje", "N")?,
        })
    }
}

impl Machine for Thruster {
    fn kind(&self) -> &'static str {
        "rcs"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let on = cx.working && cx.signals.on(self.master);
        if !on {
            self.duty = 0.0;
            return;
        }
        io[1].demand = self.heater;
        io[1].priority = 150;
        let cmd = cx.signals.get(self.cmd).clamp(0.0, 1.0);
        // below one impulse bit per step it fires in pulses
        let bit = self.min_on / cx.dt.max(1e-3);
        self.pulse += cmd;
        self.duty = if cmd >= bit.min(1.0) {
            self.pulse = 0.0;
            cmd
        } else if self.pulse >= bit.min(1.0) {
            self.pulse -= bit.min(1.0);
            bit.min(1.0)
        } else {
            0.0
        };
        io[0].demand = self.force / (self.isp * G0) * self.duty;
        io[0].priority = 170;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let valves = io[1].share >= 0.5 && io[1].fed;
        let fed = if io[0].demand > 0.0 { io[0].share } else { 0.0 };
        self.thrust = if valves { self.force * self.duty * fed * (0.3 + 0.7 * f64::from(cx.health)) } else { 0.0 };
        cx.signals.set(self.o_f, self.thrust);
    }
    fn thrust(&self) -> f64 {
        self.thrust
    }
    fn kept(&self, _: &mut Vec<f64>) {}
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.pulse);
    }
    fn load(&mut self, s: &[f64]) {
        if let [p, ..] = *s {
            self.pulse = p;
        }
    }
}
