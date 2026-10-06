//! Electrical machines: batteries, generators (APU, fuel cell), DC-DC converters, solar panels,
//! loads and lights. Power flows in W; the bus potential is in volts.
use crate::{
    machine::{Build, Cx, Machine, PortSpec, approach, interp},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

const ELEC: Medium = Medium::Electrico;

/// Open-circuit voltage of a Li-ion cell by state of charge.
const OCV: [(f64, f64); 9] = [(0.0, 3.0), (0.05, 3.3), (0.1, 3.45), (0.2, 3.55), (0.4, 3.64), (0.6, 3.75), (0.8, 3.92), (0.95, 4.06), (1.0, 4.18)];

/// A battery of Li-ion cells (series × parallel). Terminal voltage OCV(SoC) − I·R, R growing in
/// the cold and near empty; coulomb counting; I²R heat into its thermal mass; cut-offs at
/// minimum voltage and temperature limits.
pub struct Battery {
    ports: [PortSpec; 1],
    series: f64,
    ah: f64,
    r_cell: f64,
    parallel: f64,
    c_out: f64,
    c_in: f64,
    heat_cap: f64,
    v_min: f64,
    t_max: f64,
    t_min_charge: f64,
    always: bool,
    /// How many times what goes into it counts toward its charge (`ganancia_carga`). 1: what
    /// goes in is what it holds. A ship's data may set it higher so that nobody waits hours
    /// for a battery to fill (docs/TIEMPOS.md): what comes out of it is always what it had.
    gain: f64,
    // state
    soc: f64,
    temp: f64,
    current: f64,
    cmd: SignalId,
    o_soc: SignalId,
    o_v: SignalId,
    o_i: SignalId,
    o_t: SignalId,
    o_state: SignalId,
}

impl Battery {
    pub fn new(b: &mut Build) -> Result<Battery, String> {
        Ok(Battery {
            ports: [PortSpec { role: "bornes", medium: ELEC }],
            series: b.f("celdas_serie", 7.0),
            parallel: b.f("celdas_paralelo", 100.0),
            ah: b.q("capacidad_celda", "C", 5.0 * 3600.0)? / 3600.0,
            r_cell: b.q("r_celda", "ohm", 0.025)?,
            c_out: b.f("c_descarga", 2.0),
            c_in: b.f("c_carga", 0.7),
            heat_cap: b.q("masa_termica", "J/K", 40_000.0)?,
            v_min: b.f("v_min_celda", 3.0),
            t_max: b.q("t_max", "K", 333.15)?,
            t_min_charge: b.q("t_min_carga", "K", 273.15)?,
            always: b.flag("siempre", false),
            gain: b.f("ganancia_carga", 1.0).max(0.0),
            soc: b.f("soc_inicial", 0.9).clamp(0.0, 1.0),
            temp: b.q("t_inicial", "K", 293.15)?,
            current: 0.0,
            cmd: b.cmd("conectar"),
            o_soc: b.out("soc", "%")?,
            o_v: b.out("v", "V")?,
            o_i: b.out("i", "A")?,
            o_t: b.out("t", "°C")?,
            o_state: b.out("estado", "")?,
        })
    }

    fn capacity(&self) -> f64 {
        // the cold takes capacity away
        let cold = (1.0 - 0.012 * (293.15 - self.temp).max(0.0)).clamp(0.35, 1.0);
        self.ah * self.parallel * cold
    }

    fn resistance(&self) -> f64 {
        let cold = 1.0 + 2.0 * ((288.0 - self.temp) / 40.0).max(0.0);
        let empty = 1.0 + 1.5 * ((0.15 - self.soc) / 0.15).max(0.0);
        self.r_cell * cold * empty * self.series / self.parallel
    }

    fn ocv(&self) -> f64 {
        interp(&OCV, self.soc) * self.series
    }
}

impl Machine for Battery {
    fn kind(&self) -> &'static str {
        "bateria"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let on = cx.working && (self.always || cx.signals.on(self.cmd));
        if !on {
            return;
        }
        let v = (self.ocv() - self.current * self.resistance()).max(0.0);
        let p = &mut io[0];
        p.potential = v;
        let health = f64::from(cx.health).max(0.05);
        let cap = self.capacity();
        let empty = self.soc <= 0.002 || v < self.v_min * self.series;
        let hot = self.temp > self.t_max;
        if !empty && !hot {
            p.store_out = v * self.c_out * cap * health;
        }
        if self.soc < 0.999 && self.temp > self.t_min_charge && !hot {
            p.store_in = v * self.c_in * cap * health;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let p = &io[0];
        let on = cx.working && (self.always || cx.signals.on(self.cmd));
        let v = if on { (self.ocv() - self.current * self.resistance()).max(1.0) } else { self.ocv() };
        // discharge positive
        let i = if on { -p.stored / v } else { 0.0 };
        self.current = i;
        let eff = if i < 0.0 { 0.99 * self.gain } else { 1.0 };
        self.soc = (self.soc - i * eff * cx.dt / (3600.0 * self.capacity())).clamp(0.0, 1.0);
        // I²R heats it; it cools toward 20 °C
        let heat = i * i * self.resistance();
        self.temp += (heat - (self.temp - 293.15) * 8.0) * cx.dt / self.heat_cap;
        let s = &mut cx.signals;
        s.set(self.o_soc, self.soc);
        s.set(self.o_v, if on { v } else { 0.0 });
        s.set(self.o_i, i);
        s.set(self.o_t, self.temp);
        let state = if !cx.working {
            3.0
        } else if !on {
            0.0
        } else if i > 0.5 {
            1.0
        } else if i < -0.5 {
            2.0
        } else {
            4.0
        };
        s.set(self.o_state, state);
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.soc, self.temp, self.current]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [soc, t, i, ..] = *s {
            (self.soc, self.temp, self.current) = (soc, t, i);
        }
    }
}

/// A generator that burns fuel: APU or fuel cell. Start: it draws start power for `arranque` s
/// (fails without it or without fuel), then runs regulated at its voltage; a flame-out (no feed)
/// latches a failure until switched off and on. Its starter is on a circuit of its own where the
/// data wires one (port "arranque": its breaker pulled, it does not start), else on the bus its
/// output feeds.
pub struct Generator {
    ports: [PortSpec; 3],
    power: f64,
    fuel_rate: f64,
    start_time: f64,
    start_power: f64,
    volts: f64,
    fueled: bool,
    // state: 0 off, 1 starting, 2 running, 3 failed
    state: f64,
    timer: f64,
    load: f64,
    cmd: SignalId,
    o_state: SignalId,
    o_p: SignalId,
    o_fuel: SignalId,
}

impl Generator {
    pub fn new(b: &mut Build) -> Result<Generator, String> {
        Ok(Generator {
            ports: [PortSpec { role: "salida", medium: ELEC }, PortSpec { role: "combustible", medium: Medium::Propelente }, PortSpec { role: "arranque", medium: ELEC }],
            power: b.q("potencia", "W", 15_000.0)?,
            fuel_rate: b.q("consumo", "kg/s", 0.012)?,
            start_time: b.q("arranque", "s", 8.0)?,
            start_power: b.q("energia_arranque", "W", 3000.0)?,
            volts: b.q("tension", "V", 28.0)?,
            fueled: !b.flag("sin_combustible", false),
            state: 0.0,
            timer: 0.0,
            load: 0.0,
            cmd: b.cmd("marcha"),
            o_state: b.out("estado", "")?,
            o_p: b.out("p", "kW")?,
            o_fuel: b.out("consumo", "kg/s")?,
        })
    }

    /// The port its starter draws from: its own circuit if it is wired, else its output's bus.
    fn starter(io: &[PortIo]) -> usize {
        if io[2].net != u16::MAX { 2 } else { 0 }
    }
}

impl Machine for Generator {
    fn kind(&self) -> &'static str {
        "generador"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let want = cx.signals.on(self.cmd) && cx.working;
        if !want {
            self.state = 0.0;
            self.timer = 0.0;
            return;
        }
        if self.state == 0.0 {
            self.state = 1.0;
            self.timer = 0.0;
        }
        let out = if self.state == 2.0 { 1.0 } else { 0.0 };
        if self.state == 1.0 {
            let k = Self::starter(io);
            io[k].demand = self.start_power;
            io[k].priority = 250;
        }
        if self.state == 2.0 {
            io[0].produce = self.power * (0.4 + 0.6 * f64::from(cx.health));
            io[0].potential = self.volts;
            io[0].regulated = true;
        }
        if self.fueled && self.state >= 1.0 && self.state < 3.0 {
            let frac = if self.state == 1.0 { 0.3 } else { 0.25 + 0.75 * self.load * out };
            io[1].demand = self.fuel_rate * frac;
            io[1].priority = 200;
        }
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let fed = !self.fueled || io[1].share >= 0.6;
        match self.state as u8 {
            1 => {
                if io[Self::starter(io)].share < 0.5 || !fed {
                    self.timer = 0.0;
                    if !fed {
                        self.state = 3.0;
                    }
                } else {
                    self.timer += cx.dt;
                    if self.timer >= self.start_time {
                        self.state = 2.0;
                    }
                }
            }
            2 => {
                if !fed {
                    self.state = 3.0;
                }
                let cap = io[0].produce.max(1.0);
                self.load = approach(self.load, io[0].gave / cap, 1.5, cx.dt);
            }
            _ => self.load = approach(self.load, 0.0, 1.0, cx.dt),
        }
        let s = &mut cx.signals;
        s.set(self.o_state, self.state);
        s.set(self.o_p, io[0].gave);
        s.set(self.o_fuel, io[1].got);
    }
    fn heat(&self) -> f64 {
        self.load * self.power * 0.6
    }
    fn settling(&self) -> Option<bool> {
        Some(self.state == 1.0)
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.state, self.timer, self.load]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, b, c, ..] = *s {
            (self.state, self.timer, self.load) = (a, b, c);
        }
    }
}

/// Any electric consumer: avionics, heaters, fans' motors, a radio... `potencia` while switched
/// on (`encender`, or always), `espera` while off but fed. With `luz`, it is a light: its
/// brightness follows the share it gets and the bus voltage.
pub struct Load {
    ports: [PortSpec; 1],
    power: f64,
    standby: f64,
    priority: u8,
    always: bool,
    light: bool,
    nominal: f64,
    heat_share: f64,
    // state
    on: bool,
    bright: f64,
    flicker: f64,
    cmd: SignalId,
    o_on: SignalId,
    o_p: SignalId,
}

impl Load {
    pub fn new(b: &mut Build, light: bool) -> Result<Load, String> {
        let always = b.flag("siempre", false);
        Ok(Load {
            ports: [PortSpec { role: "alimentacion", medium: ELEC }],
            power: b.q("potencia", "W", if light { 40.0 } else { 100.0 })?,
            standby: b.q("espera", "W", 0.0)?,
            priority: b.f("prioridad", 1.0) as u8,
            always,
            light,
            nominal: b.q("tension", "V", 28.0)?,
            heat_share: b.f("calor", if light { 0.7 } else { 1.0 }),
            on: false,
            bright: 0.0,
            flicker: 0.0,
            cmd: b.cmd("encender"),
            o_on: b.out("on", "")?,
            o_p: b.out("p", "W")?,
        })
    }
}

impl Machine for Load {
    fn kind(&self) -> &'static str {
        if self.light { "luz" } else { "carga" }
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        let want = self.always || cx.signals.on(self.cmd);
        self.on = want && cx.working;
        io[0].demand = if self.on { self.power } else if cx.working { self.standby } else { 0.0 };
        io[0].priority = self.priority;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let p = &io[0];
        let fed = self.on && p.fed && p.share >= 0.5;
        if self.light {
            let v = if self.nominal > 0.0 { (p.level / self.nominal).clamp(0.0, 1.2) } else { 1.0 };
            let mut target = if fed { (p.share * v * v).min(1.0) } else { 0.0 };
            // a damaged lamp flickers
            if fed && cx.health < 0.6 {
                self.flicker = (self.flicker + cx.dt * (7.0 + 13.0 * f64::from(1.0 - cx.health))).fract();
                if self.flicker < 0.25 * f64::from(1.0 - cx.health) {
                    target *= 0.15;
                }
            }
            self.bright = approach(self.bright, target, 0.03, cx.dt);
        }
        cx.signals.set(self.o_on, if fed { 1.0 } else { 0.0 });
        cx.signals.set(self.o_p, p.got);
    }
    fn light(&self) -> f32 {
        self.bright as f32
    }
    fn heat(&self) -> f64 {
        if self.on { self.power * self.heat_share * f64::from(u8::from(self.bright > 0.0 || !self.light)) } else { 0.0 }
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.push(self.bright);
    }
    fn load(&mut self, s: &[f64]) {
        if let [b, ..] = *s {
            self.bright = b;
        }
    }
}

/// A solar panel: irradiance on its face × area × efficiency (falling when hot), unregulated.
pub struct Solar {
    ports: [PortSpec; 1],
    area: f64,
    eff: f64,
    volts: f64,
    deploy: Option<SignalId>,
    o_p: SignalId,
}

impl Solar {
    pub fn new(b: &mut Build) -> Result<Solar, String> {
        let deploy = b.text("despliegue").map(str::to_string);
        Ok(Solar {
            ports: [PortSpec { role: "salida", medium: ELEC }],
            area: b.q("area", "m2", 4.0)?,
            eff: b.f("eficiencia", 0.29),
            volts: b.q("tension", "V", 28.0)?,
            deploy: deploy.map(|d| b.store.define(&d)),
            o_p: b.out("p", "W")?,
        })
    }
}

impl Machine for Solar {
    fn kind(&self) -> &'static str {
        "panel_solar"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working {
            return;
        }
        let open = self.deploy.map_or(1.0, |d| cx.signals.get(d).clamp(0.0, 1.0));
        io[0].produce = cx.env.sun * self.area * self.eff * f64::from(cx.health) * open;
        io[0].potential = self.volts;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        cx.signals.set(self.o_p, io[0].gave);
    }
    fn save(&self, _: &mut Vec<f64>) {}
    fn load(&mut self, _: &[f64]) {}
}

/// A DC-DC converter (or a transformer-rectifier) from one bus to another: it offers on its
/// output what its input gave last tick, less its losses, and asks its input for what its output
/// sold. One tick of lag, no loops.
pub struct Converter {
    ports: [PortSpec; 2],
    rating: f64,
    eff: f64,
    volts: f64,
    always: bool,
    sold: f64,
    bought: f64,
    cmd: SignalId,
    o_p: SignalId,
}

impl Converter {
    pub fn new(b: &mut Build) -> Result<Converter, String> {
        Ok(Converter {
            ports: [PortSpec { role: "entrada", medium: ELEC }, PortSpec { role: "salida", medium: ELEC }],
            rating: b.q("potencia", "W", 5000.0)?,
            eff: b.f("eficiencia", 0.94),
            volts: b.q("tension", "V", 28.0)?,
            always: b.flag("siempre", true),
            sold: 0.0,
            bought: 0.0,
            cmd: b.cmd("encender"),
            o_p: b.out("p", "W")?,
        })
    }
}

impl Machine for Converter {
    fn kind(&self) -> &'static str {
        "convertidor"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working || !(self.always || cx.signals.on(self.cmd)) {
            return;
        }
        let rating = self.rating * f64::from(cx.health).max(0.1);
        // ask the input for what the output sold (and a little headroom to grow)
        io[0].demand = ((self.sold / self.eff) * 1.25).max(0.1 * rating).min(rating / self.eff);
        io[0].priority = 220;
        io[1].produce = (self.bought * self.eff).min(rating);
        io[1].potential = self.volts;
        io[1].regulated = io[1].produce > 0.0;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        self.bought = io[0].got;
        self.sold = io[1].gave;
        cx.signals.set(self.o_p, self.sold);
    }
    fn heat(&self) -> f64 {
        self.bought * (1.0 - self.eff)
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.sold, self.bought]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [a, b, ..] = *s {
            (self.sold, self.bought) = (a, b);
        }
    }
}
