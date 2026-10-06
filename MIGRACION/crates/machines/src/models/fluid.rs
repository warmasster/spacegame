//! Fluid machines: hydraulic power units (motor, pump and reservoir), accumulators, pumps of any
//! fluid, propellant tanks and gas bottles.
use crate::{
    machine::{Build, Cx, Machine, PortSpec, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

/// A hydraulic power unit: an electric motor driving a pressure-compensated pump that draws from
/// its own reservoir. Flow falls off as the line nears its set pressure; whatever the system
/// leaks comes out of the reservoir, and an empty reservoir makes the pump cavitate.
pub struct HydraulicUnit {
    ports: [PortSpec; 2],
    flow: f64,
    set: f64,
    motor: f64,
    eff: f64,
    tank: f64,
    always: bool,
    // state
    level: f64,
    speed: f64,
    temp: f64,
    /// Share of its flow it delivered lately (a pressure-compensated pump on the stops idles).
    duty: f64,
    cmd: SignalId,
    o_p: SignalId,
    o_q: SignalId,
    o_level: SignalId,
    o_state: SignalId,
    o_t: SignalId,
}

impl HydraulicUnit {
    pub fn new(b: &mut Build) -> Result<HydraulicUnit, String> {
        let tank = b.q("deposito", "m3", 0.012)?;
        Ok(HydraulicUnit {
            ports: [PortSpec { role: "motor", medium: Medium::Electrico }, PortSpec { role: "presion", medium: Medium::Hidraulico }],
            flow: b.q("caudal", "m3/s", 40e-3 / 60.0)?,
            set: b.q("presion", "Pa", 21e6)?,
            motor: b.q("potencia_motor", "W", 6000.0)?,
            eff: b.f("eficiencia", 0.82),
            tank,
            always: b.flag("siempre", false),
            level: tank * b.f("nivel_inicial", 0.92).clamp(0.0, 1.0),
            speed: 0.0,
            temp: 293.15,
            duty: 1.0,
            cmd: b.cmd("marcha"),
            o_p: b.out("presion", "MPa")?,
            o_q: b.out("caudal", "L/min")?,
            o_level: b.out("nivel", "%")?,
            o_state: b.out("estado", "")?,
            o_t: b.out("t_aceite", "°C")?,
        })
    }
}

impl Machine for HydraulicUnit {
    fn kind(&self) -> &'static str {
        "unidad_hidraulica"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let want = cx.working && (self.always || cx.signals.on(self.cmd));
        if !want {
            return;
        }
        // the motor asks for the work it does: little against a full line, all of it filling one
        io[0].demand = self.motor * self.speed.max(0.2) * (0.18 + 0.82 * self.duty);
        io[0].priority = 120;
        let suction = (self.level / (self.tank * 0.08)).clamp(0.0, 1.0);
        let q = self.flow * self.speed * suction * (0.3 + 0.7 * f64::from(cx.health));
        if q > 0.0 {
            io[1].produce = q;
            io[1].potential = self.set;
            io[1].regulated = true;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let want = cx.working && (self.always || cx.signals.on(self.cmd));
        let power = if want { io[0].share.min(1.0) } else { 0.0 };
        // the motor spins up and down in about half a second
        self.speed = approach(self.speed, if power >= 0.5 { power } else { 0.0 }, 0.4, cx.dt);
        self.level = (self.level - io[1].leaked * cx.dt).max(0.0);
        let delivered = if self.flow > 0.0 { (io[1].gave / self.flow).clamp(0.0, 1.0) } else { 0.0 };
        self.duty = approach(self.duty, if power > 0.0 { delivered } else { 0.0 }, 0.3, cx.dt);
        // pump losses warm the oil
        let work = io[1].gave * io[1].level.max(0.0);
        self.temp += ((work / self.eff - work) * 0.4 - (self.temp - 293.15) * 15.0) * cx.dt / 9000.0;
        let s = &mut cx.signals;
        s.set(self.o_p, io[1].level);
        s.set(self.o_q, io[1].gave);
        s.set(self.o_level, self.level / self.tank);
        s.set(self.o_t, self.temp);
        let cavitating = want && self.level < self.tank * 0.05;
        s.set(self.o_state, if !cx.working { 3.0 } else if cavitating { 4.0 } else if self.speed > 0.5 { 2.0 } else if want { 1.0 } else { 0.0 });
    }
    fn heat(&self) -> f64 {
        self.motor * 0.15 * self.speed
    }
    fn settling(&self) -> Option<bool> {
        // its motor coming up to speed, or its pump filling the line (on the stops it idles)
        Some(self.speed > 0.05 && (self.speed < 0.95 || self.duty > 0.5))
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.level, self.speed, self.temp, self.duty]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, b, c, ..] = *s {
            (self.level, self.speed, self.temp) = (a, b, c);
        }
        if let Some(&d) = s.get(3) {
            self.duty = d;
        }
    }
}

/// A gas-charged accumulator (hydraulic or pneumatic line): stores fluid against its gas charge,
/// p = p0·V/(V − Vf). It steadies the line and keeps pressure a while after the pumps stop.
pub struct Accumulator {
    ports: [PortSpec; 1],
    volume: f64,
    pre: f64,
    max: f64,
    rate: f64,
    fluid: f64,
    o_p: SignalId,
    o_fill: SignalId,
}

impl Accumulator {
    pub fn new(b: &mut Build, medium: Medium) -> Result<Accumulator, String> {
        let volume = b.q("volumen", "m3", 0.004)?;
        let pre = b.q("precarga", "Pa", 10e6)?;
        Ok(Accumulator {
            ports: [PortSpec { role: "presion", medium }],
            volume,
            pre,
            max: b.q("presion_max", "Pa", 23e6)?,
            rate: b.q("caudal_max", "m3/s", 2e-3)?,
            fluid: 0.0,
            o_p: b.out("presion", "MPa")?,
            o_fill: b.out("carga", "%")?,
        })
    }

    fn pressure(&self) -> f64 {
        if self.fluid <= 0.0 { 0.0 } else { self.pre * self.volume / (self.volume - self.fluid).max(self.volume * 0.02) }
    }
}

impl Machine for Accumulator {
    fn kind(&self) -> &'static str {
        "acumulador"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working {
            return;
        }
        let p = &mut io[0];
        let pr = self.pressure();
        p.potential = pr;
        if self.fluid > 0.0 {
            p.store_out = (self.fluid / cx.dt.max(1e-3)).min(self.rate);
        }
        // it fills only against a higher line pressure: the pump's regulator sets it
        if pr < self.max && p.level >= pr * 0.98 {
            p.store_in = ((self.volume * 0.9 - self.fluid) / cx.dt.max(1e-3)).clamp(0.0, self.rate);
        }
        if p.store_in == 0.0 && pr < self.pre {
            p.store_in = self.rate * 0.5;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        if cx.working {
            self.fluid = (self.fluid + io[0].stored * cx.dt).clamp(0.0, self.volume * 0.95);
        } else {
            // a burst bottle loses its charge
            self.fluid = approach(self.fluid, 0.0, 0.3, cx.dt);
        }
        cx.signals.set(self.o_p, self.pressure());
        cx.signals.set(self.o_fill, self.fluid / (self.volume * 0.9));
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.fluid);
    }
    fn load(&mut self, s: &[f64]) {
        if let [f, ..] = *s {
            self.fluid = f;
        }
    }
}

/// An electric pump of any fluid (coolant, propellant boost, water): flow up to its rating while
/// its motor is fed, at its head.
pub struct Pump {
    ports: [PortSpec; 2],
    flow: f64,
    head: f64,
    motor: f64,
    always: bool,
    speed: f64,
    cmd: SignalId,
    o_q: SignalId,
    o_on: SignalId,
}

impl Pump {
    pub fn new(b: &mut Build, medium: Medium) -> Result<Pump, String> {
        Ok(Pump {
            ports: [PortSpec { role: "motor", medium: Medium::Electrico }, PortSpec { role: "salida", medium }],
            flow: b.q("caudal", "kg/s", 1.0)?,
            head: b.q("presion", "Pa", 300e3)?,
            motor: b.q("potencia_motor", "W", 2500.0)?,
            always: b.flag("siempre", false),
            speed: 0.0,
            cmd: b.cmd("marcha"),
            o_q: b.out("caudal", "kg/s")?,
            o_on: b.out("on", "")?,
        })
    }
}

impl Machine for Pump {
    fn kind(&self) -> &'static str {
        "bomba"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let want = cx.working && (self.always || cx.signals.on(self.cmd));
        if want {
            io[0].demand = self.motor;
            io[0].priority = 110;
        }
        let q = self.flow * self.speed * (0.35 + 0.65 * f64::from(cx.health));
        if q > 0.0 {
            io[1].produce = q;
            io[1].potential = self.head;
            io[1].regulated = true;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let want = cx.working && (self.always || cx.signals.on(self.cmd));
        let fed = want && io[0].share >= 0.5;
        self.speed = approach(self.speed, if fed { 1.0 } else { 0.0 }, 0.6, cx.dt);
        cx.signals.set(self.o_q, io[1].gave);
        cx.signals.set(self.o_on, if self.speed > 0.5 { 1.0 } else { 0.0 });
    }
    fn heat(&self) -> f64 {
        self.motor * 0.1 * self.speed
    }
    fn settling(&self) -> Option<bool> {
        Some(self.speed > 0.02 && self.speed < 0.98)
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

/// A tank of propellant (or water, or any stored fluid by mass): it feeds its line at the pressure
/// its pressurant keeps, refuels through `repostar`, and leaks when its part is hurt. A burst tank
/// with fuel in it is a bomb (`burst`).
pub struct Tank {
    ports: [PortSpec; 1],
    cap: f64,
    pressure: f64,
    rate: f64,
    refuel: f64,
    mass: f64,
    leak: f64,
    cmd_refuel: SignalId,
    o_m: SignalId,
    o_level: SignalId,
    o_p: SignalId,
    o_leak: SignalId,
}

impl Tank {
    pub fn new(b: &mut Build, medium: Medium) -> Result<Tank, String> {
        let cap = b.q("capacidad", "kg", 1000.0)?;
        Ok(Tank {
            ports: [PortSpec { role: "salida", medium }],
            cap,
            pressure: b.q("presion", "Pa", 2.2e6)?,
            rate: b.q("caudal_max", "kg/s", 8.0)?,
            refuel: b.q("repostaje", "kg/s", 10.0)?,
            mass: b.q("masa_inicial", "kg", cap * 0.9)?.min(cap),
            leak: 0.0,
            cmd_refuel: b.cmd("repostar"),
            o_m: b.out("masa", "kg")?,
            o_level: b.out("nivel", "%")?,
            o_p: b.out("presion", "kPa")?,
            o_leak: b.out("fuga", "kg/s")?,
        })
    }

}

impl Machine for Tank {
    fn kind(&self) -> &'static str {
        "deposito"
    }
    fn rupture(&self) -> f64 {
        // what it holds, burning: a share of its chemical energy goes into the blast
        if self.mass > 20.0 { self.mass * 2.0e5 } else { 0.0 }
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working {
            return;
        }
        let p = &mut io[0];
        if self.mass > 0.0 {
            p.potential = self.pressure;
            p.store_out = (self.mass / cx.dt.max(1e-3)).min(self.rate);
        }
        // a transfer pump on its line can fill it
        if self.mass < self.cap {
            p.store_in = self.rate * 0.5;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        // the ground hose (repostar) fills it straight
        let refuel = if cx.signals.on(self.cmd_refuel) { self.refuel } else { 0.0 };
        self.mass = (self.mass + (io[0].stored + refuel) * cx.dt).clamp(0.0, self.cap);
        // a hurt tank leaks, a wrecked one pours out
        let h = f64::from(cx.health);
        self.leak = if !cx.working { 45.0 } else if h < 0.7 { 4.0 * ((0.7 - h) / 0.7).powi(2) } else { 0.0 };
        self.mass = (self.mass - self.leak * cx.dt).max(0.0);
        if self.mass == 0.0 {
            self.leak = 0.0;
        }
        let s = &mut cx.signals;
        s.set(self.o_m, self.mass);
        s.set(self.o_level, self.mass / self.cap);
        s.set(self.o_p, if self.mass > 0.0 { self.pressure } else { 0.0 });
        s.set(self.o_leak, self.leak);
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.mass);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, ..] = *s {
            self.mass = a;
        }
    }
}

/// A high-pressure gas bottle (O₂, N₂, He, the pressurant of a tank): its pressure falls with
/// what is left (ideal gas); a regulator feeds its line at a set pressure while it can.
pub struct Bottle {
    ports: [PortSpec; 1],
    cap: f64,
    p_full: f64,
    out_p: f64,
    rate: f64,
    mass: f64,
    o_m: SignalId,
    o_p: SignalId,
}

impl Bottle {
    pub fn new(b: &mut Build) -> Result<Bottle, String> {
        let cap = b.q("capacidad", "kg", 80.0)?;
        Ok(Bottle {
            ports: [PortSpec { role: "salida", medium: Medium::Gas }],
            cap,
            p_full: b.q("presion_llena", "Pa", 20e6)?,
            out_p: b.q("presion_salida", "Pa", 800e3)?,
            rate: b.q("caudal_max", "kg/s", 0.5)?,
            mass: b.q("masa_inicial", "kg", cap)?.min(cap),
            o_m: b.out("masa", "kg")?,
            o_p: b.out("presion", "MPa")?,
        })
    }
}

impl Machine for Bottle {
    fn kind(&self) -> &'static str {
        "botella"
    }
    fn rupture(&self) -> f64 {
        // the stored gas expanding: p·V·ln(p/p0), roughly
        let p = self.p_full * self.mass / self.cap;
        if p > 1e6 { p * 0.05 * (p / 1e5).ln() } else { 0.0 }
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let inside = self.p_full * self.mass / self.cap;
        if !cx.working || inside < self.out_p * 0.2 {
            return;
        }
        io[0].potential = inside.min(self.out_p);
        io[0].store_out = (self.mass / cx.dt.max(1e-3)).min(self.rate);
        io[0].store_in = if inside < self.p_full { self.rate * 0.2 } else { 0.0 };
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        self.mass = (self.mass + io[0].stored * cx.dt).clamp(0.0, self.cap);
        if !cx.working {
            self.mass = approach(self.mass, 0.0, 2.0, cx.dt);
        }
        cx.signals.set(self.o_m, self.mass);
        cx.signals.set(self.o_p, self.p_full * self.mass / self.cap);
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.mass);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, ..] = *s {
            self.mass = a;
        }
    }
}
