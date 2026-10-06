//! Heat: coolant loops (the thermal mass of a loop is its store; its potential, the temperature),
//! radiators that reject heat to space by area (deployed or not) and temperature, and the
//! thermoelectric converter that turns a hot loop into power and rejects the rest to a cold one.
//! Heat moves only where coolant flows: every heat exchanger also asks its coolant line for flow.
use crate::{
    machine::{Build, Cx, Machine, PortSpec, SIGMA, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

/// A coolant loop: the heat it holds is its store, its temperature the loop's potential.
pub struct Loop {
    ports: [PortSpec; 1],
    cap: f64,
    t_min: f64,
    energy: f64,
    o_t: SignalId,
}

impl Loop {
    pub fn new(b: &mut Build) -> Result<Loop, String> {
        let mass = b.q("masa", "kg", 60.0)?;
        let cp = b.q("cp", "J/kg/K", 3500.0)?;
        let cap = mass * cp;
        let t0 = b.q("t_inicial", "K", 293.15)?;
        Ok(Loop { ports: [PortSpec { role: "calor", medium: Medium::Termico }], cap, t_min: b.q("t_congelacion", "K", 200.0)?, energy: cap * t0, o_t: b.out("t", "°C")? })
    }

    fn temp(&self) -> f64 {
        self.energy / self.cap
    }
}

impl Machine for Loop {
    fn kind(&self) -> &'static str {
        "bucle"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let p = &mut io[0];
        p.potential = self.temp();
        p.store_in = 1e9;
        p.store_out = ((self.temp() - self.t_min) * self.cap / cx.dt.max(1e-3)).max(0.0);
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        self.energy += io[0].stored * cx.dt;
        if !cx.working {
            // a burst loop loses its coolant: what is left cools fast
            self.energy = approach(self.energy, self.cap * 250.0, 20.0, cx.dt);
        }
        cx.signals.set(self.o_t, self.temp());
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.energy);
    }
    fn load(&mut self, s: &[f64]) {
        if let [e, ..] = *s {
            self.energy = e;
        }
    }
}

/// A radiator: εσA(T⁴ − Tsink⁴), with the area between stowed and deployed following a signal
/// (its actuator's position), less where it is hurt, and only with coolant flowing through it.
pub struct Radiator {
    ports: [PortSpec; 2],
    stowed: f64,
    deployed: f64,
    emiss: f64,
    flow: f64,
    deploy: Option<SignalId>,
    o_q: SignalId,
    o_area: SignalId,
}

impl Radiator {
    pub fn new(b: &mut Build) -> Result<Radiator, String> {
        let deploy = b.text("despliegue").map(str::to_string);
        Ok(Radiator {
            ports: [PortSpec { role: "calor", medium: Medium::Termico }, PortSpec { role: "flujo", medium: Medium::Refrigerante }],
            stowed: b.q("area_plegado", "m2", 4.0)?,
            deployed: b.q("area_desplegado", "m2", 14.0)?,
            emiss: b.f("emisividad", 0.88),
            flow: b.q("caudal", "kg/s", 0.3)?,
            deploy: deploy.map(|d| b.store.define(&d)),
            o_q: b.out("q", "kW")?,
            o_area: b.out("area", "m2")?,
        })
    }
}

impl Machine for Radiator {
    fn kind(&self) -> &'static str {
        "radiador"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working {
            return;
        }
        io[1].demand = self.flow;
        io[1].priority = 100;
        let open = self.deploy.map_or(1.0, |d| cx.signals.get(d).clamp(0.0, 1.0));
        let area = (self.stowed + (self.deployed - self.stowed) * open) * f64::from(cx.health);
        let t = io[0].level.max(0.0);
        let q = self.emiss * SIGMA * area * (t.powi(4) - cx.env.sink_temp.powi(4)).max(0.0);
        io[0].demand = q * io[1].share;
        io[0].priority = 50;
        cx.signals.set(self.o_area, area);
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        cx.signals.set(self.o_q, io[0].got);
    }
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}

/// A thermoelectric (or Brayton) converter: takes heat from a hot loop, gives a share of the
/// Carnot fraction as power and rejects the rest into a cold loop. Both loops need flow.
pub struct Converter {
    ports: [PortSpec; 4],
    rated_heat: f64,
    carnot: f64,
    volts: f64,
    flow: f64,
    always: bool,
    heat_in: f64,
    power: f64,
    cmd: SignalId,
    o_p: SignalId,
    o_eff: SignalId,
}

impl Converter {
    pub fn new(b: &mut Build) -> Result<Converter, String> {
        Ok(Converter {
            ports: [
                PortSpec { role: "caliente", medium: Medium::Termico },
                PortSpec { role: "frio", medium: Medium::Termico },
                PortSpec { role: "salida", medium: Medium::Electrico },
                PortSpec { role: "flujo", medium: Medium::Refrigerante },
            ],
            rated_heat: b.q("calor_nominal", "W", 100_000.0)?,
            carnot: b.f("fraccion_carnot", 0.35),
            volts: b.q("tension", "V", 120.0)?,
            flow: b.q("caudal", "kg/s", 0.5)?,
            always: b.flag("siempre", true),
            heat_in: 0.0,
            power: 0.0,
            cmd: b.cmd("encender"),
            o_p: b.out("p", "kW")?,
            o_eff: b.out("eficiencia", "%")?,
        })
    }
}

impl Machine for Converter {
    fn kind(&self) -> &'static str {
        "conversor_termico"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working || !(self.always || cx.signals.on(self.cmd)) {
            return;
        }
        io[3].demand = self.flow;
        io[3].priority = 100;
        let (th, tc) = (io[0].level, io[1].level);
        let eff = if th > tc && th > 0.0 { self.carnot * (1.0 - tc / th) } else { 0.0 };
        // it draws heat as fast as the hot side pushes it, up to its rating
        let draw = (self.rated_heat * ((th - tc - 20.0) / 300.0).clamp(0.0, 1.0)) * io[3].share * f64::from(cx.health);
        io[0].demand = draw;
        io[0].priority = 60;
        // what it took last tick: power out, the rest to the cold loop
        io[2].produce = self.heat_in * eff;
        io[2].potential = self.volts;
        io[2].regulated = io[2].produce > 0.0;
        io[1].produce = self.heat_in * (1.0 - eff);
        cx.signals.set(self.o_eff, eff);
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        self.heat_in = io[0].got;
        self.power = io[2].gave;
        cx.signals.set(self.o_p, self.power);
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.heat_in, self.power]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, b, ..] = *s {
            (self.heat_in, self.power) = (a, b);
        }
    }
}
