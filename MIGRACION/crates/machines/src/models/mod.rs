//! Every machine model, by the name the data gives it (`"modelo"`). A new model is a file here
//! and one line in `create`.
pub mod air;
pub mod combat;
pub mod electric;
pub mod engine;
pub mod fluid;
pub mod inertial;
pub mod life;
pub mod powerpack;
pub mod reactor;
pub mod thermal;

use crate::{
    machine::{Build, Cx, Machine, PortSpec},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

/// The model `kind` built from its data.
pub fn create(kind: &str, b: &mut Build) -> Result<Box<dyn Machine>, String> {
    let medium = |b: &Build, or: Medium| b.text("medio").and_then(Medium::parse).unwrap_or(or);
    Ok(match kind {
        "bateria" => Box::new(electric::Battery::new(b)?),
        "generador" | "apu" | "pila_combustible" => Box::new(electric::Generator::new(b)?),
        "carga" | "avionica" | "calefactor" => Box::new(electric::Load::new(b, false)?),
        "luz" => Box::new(electric::Load::new(b, true)?),
        "panel_solar" => Box::new(electric::Solar::new(b)?),
        "convertidor" => Box::new(electric::Converter::new(b)?),
        "unidad_hidraulica" => Box::new(fluid::HydraulicUnit::new(b)?),
        "acumulador" => {
            let m = medium(b, Medium::Hidraulico);
            Box::new(fluid::Accumulator::new(b, m)?)
        }
        "bomba" => {
            let m = medium(b, Medium::Refrigerante);
            Box::new(fluid::Pump::new(b, m)?)
        }
        "deposito" => {
            let m = medium(b, Medium::Propelente);
            Box::new(fluid::Tank::new(b, m)?)
        }
        "botella" => Box::new(fluid::Bottle::new(b)?),
        "bucle" => Box::new(thermal::Loop::new(b)?),
        "radiador" => Box::new(thermal::Radiator::new(b)?),
        "conversor_termico" => Box::new(thermal::Converter::new(b)?),
        "reactor" => Box::new(reactor::Reactor::new(b)?),
        "motor_cohete" => Box::new(engine::Rocket::new(b)?),
        "rcs" => Box::new(engine::Thruster::new(b)?),
        "ventilador" => Box::new(life::Fan::new(b)?),
        "rejilla" => Box::new(life::Vent::new(b)?),
        "depurador" => Box::new(life::Scrubber::new(b)?),
        "inyector" => Box::new(life::Injector::new(b)?),
        "generador_o2" => Box::new(life::Electrolyser::new(b)?),
        "compresor" => Box::new(air::Compressor::new(b)?),
        "deposito_aire" => Box::new(air::Tank::new(b)?),
        "giroscopo" => Box::new(inertial::Gyro::new(b)?),
        "electroiman" => Box::new(inertial::Magnet::new(b)?),
        "ordenador" => Box::new(Computer::new(b)?),
        "reactor_unificado" => Box::new(powerpack::Unified::new(b)?),
        "radar" => Box::new(combat::Device::new(b, combat::Radio::Radar)?),
        "alertador" => Box::new(combat::Device::new(b, combat::Radio::Warner)?),
        "irst" => Box::new(combat::Device::new(b, combat::Radio::Infrared)?),
        "transpondedor" => Box::new(combat::Device::new(b, combat::Radio::Transponder)?),
        "perturbador" => Box::new(combat::Device::new(b, combat::Radio::Jammer)?),
        "arma" => Box::new(combat::Launcher::new(b)?),
        "pasivo" => Box::new(Passive),
        _ => return Err(format!("{}: modelo de máquina desconocido '{kind}' (modelos: {})", b.id, KINDS.join(", "))),
    })
}

pub const KINDS: [&str; 36] = [
    "reactor_unificado", "radar", "alertador", "irst", "transpondedor", "perturbador", "arma",
    "giroscopo", "electroiman", "compresor", "deposito_aire",
    "bateria", "generador", "carga", "luz", "panel_solar", "convertidor", "unidad_hidraulica", "acumulador", "bomba", "deposito", "botella", "bucle", "radiador",
    "conversor_termico", "reactor", "motor_cohete", "rcs", "ventilador", "rejilla", "depurador", "inyector", "generador_o2", "ordenador", "pasivo", "avionica",
];

/// A computer: power in, a data network out (what plugs into it works only while it runs).
pub struct Computer {
    ports: [PortSpec; 2],
    power: f64,
    up: bool,
    boot: f64,
    timer: f64,
    cmd: SignalId,
    always: bool,
    o_on: SignalId,
}

impl Computer {
    pub fn new(b: &mut Build) -> Result<Computer, String> {
        Ok(Computer {
            ports: [PortSpec { role: "energia", medium: Medium::Electrico }, PortSpec { role: "datos", medium: Medium::Datos }],
            power: b.q("potencia", "W", 300.0)?,
            up: false,
            boot: b.q("arranque", "s", 3.0)?,
            timer: 0.0,
            cmd: b.cmd("encender"),
            always: b.flag("siempre", true),
            o_on: b.out("on", "")?,
        })
    }
}

impl Machine for Computer {
    fn kind(&self) -> &'static str {
        "ordenador"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if cx.working && (self.always || cx.signals.on(self.cmd)) {
            io[0].demand = self.power;
            io[0].priority = 200;
        }
        if self.up {
            io[1].produce = 1.0;
            io[1].potential = 1.0;
            io[1].regulated = true;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let fed = cx.working && (self.always || cx.signals.on(self.cmd)) && io[0].fed && io[0].share >= 0.5;
        if fed {
            self.timer += cx.dt;
        } else {
            self.timer = 0.0;
        }
        self.up = fed && self.timer >= self.boot;
        cx.signals.set(self.o_on, if self.up { 1.0 } else { 0.0 });
    }
    fn heat(&self) -> f64 {
        if self.up { self.power } else { 0.0 }
    }
    fn settling(&self) -> Option<bool> {
        // booting
        Some(self.timer > 0.0 && !self.up)
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.timer);
    }
    fn load(&mut self, s: &[f64]) {
        if let [t, ..] = *s {
            self.timer = t;
        }
    }
}

/// A part with no behaviour of its own that still sits on networks (a junction box, a manifold).
pub struct Passive;

impl Machine for Passive {
    fn kind(&self) -> &'static str {
        "pasivo"
    }
    fn ports(&self) -> &[PortSpec] {
        &[]
    }
    fn plan(&mut self, _: &mut Cx, _: &mut [PortIo]) {}
    fn step(&mut self, _: &mut Cx, _: &[PortIo]) {}
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}
