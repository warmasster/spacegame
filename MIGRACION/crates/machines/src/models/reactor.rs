//! A fission reactor (`docs/MANDOS_Y_MAQUINAS.md` §5.2): point kinetics with one group of delayed
//! precursors (stiff: integrated implicitly in sub-steps), rods and a negative temperature
//! coefficient, fuel heat into the coolant only while coolant flows, decay heat after a SCRAM
//! (Way–Wigner), automatic SCRAM on fuel temperature, short period, loss of flow or loss of
//! control power (fail-safe), and a rearm that needs a cool core.
use crate::{
    machine::{Build, Cx, Machine, PortSpec},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

pub const OFF: f64 = 0.0;
pub const STARTING: f64 = 1.0;
pub const ONLINE: f64 = 2.0;
pub const SCRAM: f64 = 3.0;
/// The period (s) under which the rod drive stops pulling: over the trip's (3 s), with room for
/// the drive's own lag.
const PERIOD_HOLD: f64 = 5.5;
/// The period (s) its drive climbs at and the one that trips it, as built: `PERIOD_HOLD` and
/// these two go together. A reactor's data may ask for another climb (`periodo`: a quicker
/// start-up than any real core's, for play); its hold and its trip keep these proportions.
const PERIOD_CLIMB: f64 = 6.0;
const PERIOD_TRIP: f64 = 3.0;
/// The setback: it begins `SETBACK + SETBACK_BAND` kelvin under the fuel temperature that trips
/// the reactor and is whole `SETBACK` under it.
const SETBACK: f64 = 45.0;
const SETBACK_BAND: f64 = 70.0;

pub struct Reactor {
    ports: [PortSpec; 3],
    p_nom: f64,
    beta: f64,
    lambda: f64,
    gen_time: f64,
    worth: f64,
    crit: f64,
    alpha: f64,
    t_ref: f64,
    c_fuel: f64,
    ua: f64,
    flow: f64,
    control_w: f64,
    t_scram: f64,
    t_rearm: f64,
    rod_speed: f64,
    drop: f64,
    /// The period its drive climbs at (s).
    climb: f64,
    // state
    n: f64,
    c: f64,
    t_fuel: f64,
    rods: f64,
    state: f64,
    since_scram: f64,
    operated: f64,
    p0: f64,
    period: f64,
    heat_out: f64,
    /// The power its rod drive is after now (share of nominal; after the setback).
    want: f64,
    // commands
    c_run: SignalId,
    c_power: SignalId,
    c_scram: SignalId,
    c_rearm: SignalId,
    // telemetry
    o_p: SignalId,
    o_n: SignalId,
    o_rho: SignalId,
    o_tf: SignalId,
    o_tc: SignalId,
    o_rods: SignalId,
    o_state: SignalId,
    o_period: SignalId,
    o_cause: SignalId,
    o_limit: SignalId,
}

impl Reactor {
    pub fn new(b: &mut Build) -> Result<Reactor, String> {
        let beta = b.f("beta", 0.0065);
        let lambda = b.q("lambda", "1/s", 0.08)?;
        let gen_time = b.q("tiempo_generacion", "s", 1e-3)?;
        Ok(Reactor {
            ports: [
                PortSpec { role: "calor", medium: Medium::Termico },
                PortSpec { role: "flujo", medium: Medium::Refrigerante },
                PortSpec { role: "control", medium: Medium::Electrico },
            ],
            p_nom: b.q("potencia_termica", "W", 100_000.0)?,
            beta,
            lambda,
            gen_time,
            worth: b.f("valor_barras", 0.02),
            crit: b.f("barras_criticas", 0.55),
            alpha: b.q("coef_temperatura", "1/K", -3e-5)?,
            t_ref: b.q("t_referencia", "K", 600.0)?,
            c_fuel: b.q("capacidad_combustible", "J/K", 60_000.0)?,
            ua: b.q("ua", "W/K", 1200.0)?,
            flow: b.q("caudal", "kg/s", 0.6)?,
            control_w: b.q("potencia_control", "W", 400.0)?,
            t_scram: b.q("t_scram", "K", 1023.15)?,
            t_rearm: b.q("t_rearme", "K", 573.15)?,
            rod_speed: b.f("velocidad_barras", 0.05),
            drop: b.q("t_caida", "s", 1.2)?,
            climb: b.q("periodo", "s", PERIOD_CLIMB)?.max(0.5),
            n: 1e-4,
            c: 1e-4 * beta / (lambda * gen_time),
            t_fuel: 293.15,
            rods: 0.0,
            state: OFF,
            since_scram: 1e6,
            operated: 0.0,
            p0: 0.0,
            period: f64::INFINITY,
            heat_out: 0.0,
            want: 0.0,
            c_run: b.cmd("marcha"),
            c_power: b.cmd("potencia"),
            c_scram: b.cmd("scram"),
            c_rearm: b.cmd("rearme"),
            o_p: b.out("p_termica", "kW")?,
            o_n: b.out("n", "%")?,
            o_rho: b.out("rho", "")?,
            o_tf: b.out("t_comb", "°C")?,
            o_tc: b.out("t_ref", "°C")?,
            o_rods: b.out("barras", "%")?,
            o_state: b.out("estado", "")?,
            o_period: b.out("periodo", "s")?,
            o_cause: b.out("causa", "")?,
            o_limit: b.out("limitado", "")?,
        })
    }

    fn scram(&mut self, cause: f64, cx: &mut Cx) {
        if self.state != SCRAM {
            self.state = SCRAM;
            self.since_scram = 0.0;
            self.p0 = self.n * self.p_nom;
            cx.signals.set(self.o_cause, cause);
        }
    }

    fn decay(&self) -> f64 {
        // Way–Wigner, after operating `operated` s at p0
        if self.p0 <= 0.0 {
            return 0.0;
        }
        let t = self.since_scram.max(1.0);
        0.066 * self.p0 * (t.powf(-0.2) - (t + self.operated.max(1.0)).powf(-0.2))
    }
}

impl Machine for Reactor {
    fn kind(&self) -> &'static str {
        "reactor"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if self.state != OFF || cx.signals.on(self.c_run) {
            io[2].demand = self.control_w;
            io[2].priority = 240;
        }
        io[1].demand = self.flow;
        io[1].priority = 200;
        io[0].produce = self.heat_out;
        io[0].potential = self.t_fuel;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let dt = cx.dt;
        let t_cool = if io[0].fed { io[0].level } else { self.t_fuel };
        let flow = io[1].share.clamp(0.0, 1.0) * f64::from(u8::from(io[1].fed));
        let control = io[2].share >= 0.5 && io[2].fed;
        // what is asked
        let run = cx.signals.on(self.c_run) && cx.working;
        if cx.signals.on(self.c_scram) {
            self.scram(1.0, cx);
        }
        match self.state {
            s if s == OFF && run => {
                if control && flow >= 0.5 {
                    self.state = STARTING;
                }
            }
            s if (s == STARTING || s == ONLINE) && !run => {
                self.scram(2.0, cx);
            }
            s if s == SCRAM && cx.signals.on(self.c_rearm) && self.t_fuel < self.t_rearm && self.rods <= 0.0 => {
                self.state = OFF;
                self.p0 = 0.0;
            }
            _ => {}
        }
        // the trips
        if self.state == STARTING || self.state == ONLINE {
            if !control {
                self.scram(3.0, cx);
            } else if self.t_fuel > self.t_scram {
                self.scram(4.0, cx);
            } else if self.n > 0.1 && flow < 0.5 {
                self.scram(5.0, cx);
            } else if self.period > 0.0 && self.period < PERIOD_TRIP * self.climb / PERIOD_CLIMB && self.n > 0.01 {
                self.scram(6.0, cx);
            } else if cx.health < 0.15 {
                self.scram(7.0, cx);
            }
        }
        // the rods: dropped in a SCRAM, else driven toward the power asked
        if self.state == SCRAM || self.state == OFF {
            self.rods = (self.rods - dt / self.drop).max(0.0);
        } else {
            // the rod drive aims at a reactivity: a steady period while climbing, gentle near
            // the power asked, negative above it
            let asked = if self.state == STARTING { 0.05 } else { cx.signals.get(self.c_power).clamp(0.0, 1.1) };
            // a setback before the trip: as the fuel nears its limit the drive asks for less
            // (down to a tenth of it), so that heat with nowhere to go — coolant too hot, a
            // converter that takes no more — throttles the reactor instead of tripping it
            let room = ((self.t_scram - SETBACK - self.t_fuel) / SETBACK_BAND).clamp(0.1, 1.0);
            let want = asked * room;
            self.want = want;
            cx.signals.set(self.o_limit, if room < 0.98 && self.state == ONLINE { 1.0 } else { 0.0 });
            let rho_want = if self.n < want * 0.9 {
                self.beta / (1.0 + self.lambda * self.climb)
            } else if self.n > want * 1.1 {
                -self.beta / (1.0 + self.lambda * 20.0)
            } else {
                (want - self.n) / want.max(1e-3) * 0.002
            };
            let feedback = self.alpha * (self.t_fuel - self.t_ref);
            let target = (self.crit + (rho_want - feedback) / (2.0 * self.worth)).clamp(0.0, 1.0);
            // (the drive watches the period as the trip does: while the power climbs faster
            // than `PERIOD_HOLD` it pulls no further — it gives a little back —, so a big step
            // in the power asked is climbed in a steady period instead of tripping on the
            // prompt jump)
            let target = if self.period > 0.0 && self.period < PERIOD_HOLD * self.climb / PERIOD_CLIMB { target.min(self.rods - self.rod_speed * dt * 0.5) } else { target };
            let step = self.rod_speed * dt;
            self.rods += (target - self.rods).clamp(-step, step);
            if self.state == STARTING && self.n >= 0.04 {
                self.state = ONLINE;
            }
        }
        // kinetics, implicit in 5 ms steps
        let rho = self.worth * (self.rods - self.crit) * 2.0 + self.alpha * (self.t_fuel - self.t_ref);
        let steps = (dt / 0.005).ceil().max(1.0) as usize;
        let h = dt / steps as f64;
        let n_before = self.n;
        for _ in 0..steps {
            // (I - hA) x' = x, A = [[(rho-beta)/L, lambda], [beta/L, -lambda]]
            let a11 = (rho - self.beta) / self.gen_time;
            let a12 = self.lambda;
            let a21 = self.beta / self.gen_time;
            let a22 = -self.lambda;
            let m11 = 1.0 - h * a11;
            let m12 = -h * a12;
            let m21 = -h * a21;
            let m22 = 1.0 - h * a22;
            let det = m11 * m22 - m12 * m21;
            let n = (m22 * self.n - m12 * self.c) / det;
            let c = (-m21 * self.n + m11 * self.c) / det;
            // the neutron source keeps a floor
            self.n = n.max(1e-5);
            self.c = c.max(0.0);
        }
        self.period = if self.n > n_before * 1.000_001 { dt / (self.n / n_before).ln() } else { f64::INFINITY };
        // heat: fission + decay into the fuel; out to the coolant only where it flows
        if self.state == SCRAM {
            self.since_scram += dt;
        }
        if self.state == ONLINE {
            self.operated += dt;
        }
        let p_th = self.n * self.p_nom + self.decay();
        let ua = self.ua * flow.max(0.04);
        self.heat_out = (ua * (self.t_fuel - t_cool)).max(0.0) * f64::from(u8::from(io[0].fed));
        self.t_fuel += (p_th - self.heat_out - (self.t_fuel - 293.15) * 2.0) * dt / self.c_fuel;
        let s = &mut cx.signals;
        s.set(self.o_p, p_th);
        s.set(self.o_n, self.n);
        s.set(self.o_rho, rho / self.beta);
        s.set(self.o_tf, self.t_fuel);
        s.set(self.o_tc, t_cool);
        s.set(self.o_rods, self.rods);
        s.set(self.o_state, self.state);
        s.set(self.o_period, if self.period.is_finite() { self.period } else { 0.0 });
    }
    fn heat(&self) -> f64 {
        // what the shield lets through into the compartment
        (self.n * self.p_nom + self.decay()) * 0.01
    }
    fn settling(&self) -> Option<bool> {
        // starting, or climbing to the power its drive is after
        Some(self.state == STARTING || (self.state == ONLINE && self.n < self.want * 0.9))
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.n, self.c, self.t_fuel, self.rods, self.state, self.since_scram, self.operated, self.p0, self.heat_out]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [n, c, t, r, st, ss, op, p0, ho, ..] = *s {
            (self.n, self.c, self.t_fuel, self.rods, self.state, self.since_scram, self.operated, self.p0, self.heat_out) = (n, c, t, r, st, ss, op, p0, ho);
        }
    }
}
