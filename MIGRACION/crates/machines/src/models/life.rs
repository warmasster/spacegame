//! Life support. Air moves through ducts as a network (fans give flow, vents take it into their
//! compartments); machines in the ducts treat the air that passes (scrubbers take CO₂ out,
//! injectors add O₂ and N₂ from the gas line, electrolysers make O₂). What they do to the air is a
//! `Treat`, applied by the owner to the air of their duct network (or of their compartment).
use crate::{
    machine::{Build, Cx, Machine, PortSpec, Treat, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

/// A fan: flow into its duct network at its head while its motor is fed.
pub struct Fan {
    ports: [PortSpec; 2],
    flow: f64,
    head: f64,
    motor: f64,
    always: bool,
    speed: f64,
    cmd: SignalId,
    o_q: SignalId,
}

impl Fan {
    pub fn new(b: &mut Build) -> Result<Fan, String> {
        Ok(Fan {
            ports: [PortSpec { role: "motor", medium: Medium::Electrico }, PortSpec { role: "aire", medium: Medium::Aire }],
            flow: b.q("caudal", "m3/s", 0.35)?,
            head: b.q("presion", "Pa", 350.0)?,
            motor: b.q("potencia_motor", "W", 250.0)?,
            always: b.flag("siempre", false),
            speed: 0.0,
            cmd: b.cmd("marcha"),
            o_q: b.out("caudal", "L/s")?,
        })
    }
}

impl Machine for Fan {
    fn kind(&self) -> &'static str {
        "ventilador"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if cx.working && (self.always || cx.signals.on(self.cmd)) {
            io[0].demand = self.motor;
            io[0].priority = 90;
        }
        let q = self.flow * self.speed * (0.4 + 0.6 * f64::from(cx.health));
        if q > 0.0 {
            io[1].produce = q;
            io[1].potential = self.head * self.speed;
            io[1].regulated = true;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let on = cx.working && (self.always || cx.signals.on(self.cmd)) && io[0].share >= 0.5 && io[0].fed;
        self.speed = approach(self.speed, if on { 1.0 } else { 0.0 }, 1.2, cx.dt);
        cx.signals.set(self.o_q, io[1].gave);
    }
    fn heat(&self) -> f64 {
        self.motor * self.speed * 0.3
    }
    fn settling(&self) -> Option<bool> {
        Some(self.speed > 0.02 && self.speed < 0.95)
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

/// A vent grille: it lets duct air into its compartment (what it gets is the exchange), with a
/// damper that may close it.
pub struct Vent {
    ports: [PortSpec; 1],
    flow: f64,
    damper: Option<SignalId>,
    got: f64,
    o_q: SignalId,
}

impl Vent {
    pub fn new(b: &mut Build) -> Result<Vent, String> {
        let damper = b.text("compuerta").map(str::to_string);
        Ok(Vent {
            ports: [PortSpec { role: "aire", medium: Medium::Aire }],
            flow: b.q("caudal", "m3/s", 0.1)?,
            damper: damper.map(|d| b.store.define(&d)),
            got: 0.0,
            o_q: b.out("caudal", "L/s")?,
        })
    }
}

impl Machine for Vent {
    fn kind(&self) -> &'static str {
        "rejilla"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let open = self.damper.is_none_or(|d| cx.signals.on(d));
        if open {
            io[0].demand = self.flow * if cx.working { 1.0 } else { 3.0 };
            io[0].priority = 10;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        self.got = io[0].got;
        cx.signals.set(self.o_q, self.got);
    }
    fn air(&self) -> f64 {
        self.got
    }
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}

/// A CO₂ scrubber in a duct: removes CO₂ from the air that flows through it, up to its capacity.
pub struct Scrubber {
    ports: [PortSpec; 2],
    rate: f64,
    power: f64,
    flow: f64,
    active: f64,
    cmd: SignalId,
    o_rate: SignalId,
}

impl Scrubber {
    pub fn new(b: &mut Build) -> Result<Scrubber, String> {
        Ok(Scrubber {
            ports: [PortSpec { role: "energia", medium: Medium::Electrico }, PortSpec { role: "aire", medium: Medium::Aire }],
            rate: b.q("capacidad", "mol/s", 0.02)?,
            power: b.q("potencia", "W", 1500.0)?,
            flow: b.q("caudal", "m3/s", 0.1)?,
            active: 0.0,
            cmd: b.cmd("marcha"),
            o_rate: b.out("eficacia", "%")?,
        })
    }
}

impl Machine for Scrubber {
    fn kind(&self) -> &'static str {
        "depurador"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if cx.working && cx.signals.on(self.cmd) {
            io[0].demand = self.power;
            io[0].priority = 140;
            io[1].demand = self.flow;
            io[1].priority = 30;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let on = cx.working && cx.signals.on(self.cmd) && io[0].share >= 0.5 && io[0].fed;
        self.active = if on { io[1].share * f64::from(cx.health) } else { 0.0 };
        cx.signals.set(self.o_rate, self.active);
    }
    fn treat(&self) -> Treat {
        Treat { co2: self.rate * self.active, heat: self.power * 0.5 * f64::from(u8::from(self.active > 0.0)), ..Default::default() }
    }
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}

/// Gas make-up: takes O₂ (or N₂) from the gas line and injects it into the duct air while the
/// pressure it watches is under its set point, or (for O₂) the partial pressure it watches is.
pub struct Injector {
    ports: [PortSpec; 2],
    gas_o2: bool,
    rate: f64,
    set: f64,
    sensor: Option<SignalId>,
    /// A signal with the set point (a keypad), instead of the fixed one.
    set_signal: Option<SignalId>,
    /// A signal that must be on for it to feed (the room sealed: no make-up gas into a leak).
    permit: Option<SignalId>,
    flow: f64,
    cmd: SignalId,
    o_flow: SignalId,
}

impl Injector {
    pub fn new(b: &mut Build) -> Result<Injector, String> {
        let sensor = b.text("sensor").map(str::to_string);
        let set_signal = b.text("consigna_senal").map(str::to_string);
        let permit = b.text("permiso").map(str::to_string);
        Ok(Injector {
            ports: [PortSpec { role: "gas", medium: Medium::Gas }, PortSpec { role: "aire", medium: Medium::Aire }],
            gas_o2: b.text("gas").unwrap_or("o2") == "o2",
            rate: b.q("caudal", "kg/s", 0.01)?,
            set: b.q("consigna", "Pa", 21e3)?,
            sensor: sensor.map(|s| b.store.define(&s)),
            set_signal: set_signal.map(|s| b.store.define(&s)),
            permit: permit.map(|s| b.store.define(&s)),
            flow: 0.0,
            cmd: b.cmd("marcha"),
            o_flow: b.out("caudal", "g/s")?,
        })
    }
}

impl Machine for Injector {
    fn kind(&self) -> &'static str {
        "inyector"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working || !cx.signals.on(self.cmd) || self.permit.is_some_and(|s| !cx.signals.on(s)) {
            return;
        }
        let p = self.sensor.map_or(0.0, |s| cx.signals.get(s));
        let set = self.set_signal.map_or(self.set, |s| cx.signals.get(s)).max(1.0);
        // proportional below the set point
        let want = ((set - p) / (set * 0.05)).clamp(0.0, 1.0);
        io[0].demand = self.rate * want;
        io[0].priority = 60;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        self.flow = io[0].got;
        cx.signals.set(self.o_flow, self.flow);
    }
    fn treat(&self) -> Treat {
        if self.gas_o2 { Treat { o2: self.flow / 0.032, ..Default::default() } } else { Treat { n2: self.flow / 0.028, ..Default::default() } }
    }
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}

/// An electrolyser: power (and water, taken as granted) into O₂ for its duct air.
pub struct Electrolyser {
    ports: [PortSpec; 2],
    rate: f64,
    power: f64,
    active: f64,
    cmd: SignalId,
    o_rate: SignalId,
}

impl Electrolyser {
    pub fn new(b: &mut Build) -> Result<Electrolyser, String> {
        Ok(Electrolyser {
            ports: [PortSpec { role: "energia", medium: Medium::Electrico }, PortSpec { role: "aire", medium: Medium::Aire }],
            rate: b.q("produccion", "mol/s", 0.012)?,
            power: b.q("potencia", "W", 4000.0)?,
            active: 0.0,
            cmd: b.cmd("marcha"),
            o_rate: b.out("produccion", "mol/s")?,
        })
    }
}

impl Machine for Electrolyser {
    fn kind(&self) -> &'static str {
        "generador_o2"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if cx.working && cx.signals.on(self.cmd) {
            io[0].demand = self.power;
            io[0].priority = 130;
            io[1].demand = 0.02;
            io[1].priority = 30;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let on = cx.working && cx.signals.on(self.cmd) && io[0].fed;
        self.active = if on { io[0].share * io[1].share * f64::from(cx.health) } else { 0.0 };
        cx.signals.set(self.o_rate, self.rate * self.active);
    }
    fn treat(&self) -> Treat {
        Treat { o2: self.rate * self.active, heat: self.power * 0.3 * self.active, ..Default::default() }
    }
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}
