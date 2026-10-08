//! What a warship carries besides what flies it (`docs/COMBATE.md`): the boxes that sense and
//! radiate — radar, radar warning receiver, infrared search, transponder, jammer — and whatever
//! lets something fly: a cannon, a missile rail, a dispenser of decoys.
//!
//! None of them knows what a target is. A `Device` is power, a warm-up and what it radiates; a
//! `Launcher` is rounds left, a rate and a barrel that heats. What they see, whom they answer and
//! what they fire at is the ship's (`lunar_ship::tactical`), from their data (`params`) and their
//! signals.
use crate::{
    machine::{Build, Cx, Machine, PortSpec, approach},
    net::{Medium, PortIo},
};
use lunar_signals::SignalId;

/// Which box a `Device` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radio {
    Radar,
    Warner,
    Infrared,
    Transponder,
    Jammer,
}

impl Radio {
    pub fn model(self) -> &'static str {
        match self {
            Radio::Radar => "radar",
            Radio::Warner => "alertador",
            Radio::Infrared => "irst",
            Radio::Transponder => "transpondedor",
            Radio::Jammer => "perturbador",
        }
    }
    /// The model's name back to what it is.
    pub fn of(model: &str) -> Option<Radio> {
        [Radio::Radar, Radio::Warner, Radio::Infrared, Radio::Transponder, Radio::Jammer].into_iter().find(|r| r.model() == model)
    }
    /// It radiates when it works (a radar may be on and silent: `emitir`).
    fn radiates(self) -> bool {
        matches!(self, Radio::Radar | Radio::Jammer)
    }
}

/// A box of avionics: on or off, warm or not, radiating or silent.
///
/// A radar has range scales (`escalas`, m): the one chosen (`escala`, its index) is how far its
/// screen reaches and, with it, how much it radiates — the power that finds a square metre at
/// that range and no more (range goes with the fourth root of power, so half the range is a
/// sixteenth of the power). `cuota` is that share of its full power: what whoever listens for
/// it hears.
pub struct Device {
    what: Radio,
    ports: [PortSpec; 1],
    power: f64,
    standby: f64,
    warm_up: f64,
    scales: Vec<f64>,
    // state
    timer: f64,
    up: bool,
    share: f64,
    // commands
    c_on: SignalId,
    c_emit: SignalId,
    c_scale: SignalId,
    always: bool,
    // telemetry
    o_on: SignalId,
    o_emit: SignalId,
    o_share: SignalId,
    o_range: SignalId,
    o_p: SignalId,
}

impl Device {
    pub fn new(b: &mut Build, what: Radio) -> Result<Device, String> {
        let scales: Vec<f64> = match b.params.get("escalas") {
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("{}: escalas: {e}", b.id))?,
            None => Vec::new(),
        };
        if scales.windows(2).any(|w| w[1] <= w[0]) || scales.iter().any(|s| *s <= 0.0) {
            return Err(format!("{}: las escalas van de menor a mayor (m)", b.id));
        }
        let (power, standby) = match what {
            Radio::Radar => (4000.0, 150.0),
            Radio::Jammer => (2500.0, 60.0),
            _ => (60.0, 0.0),
        };
        Ok(Device {
            what,
            ports: [PortSpec { role: "alimentacion", medium: Medium::Electrico }],
            power: b.q("potencia", "W", power)?,
            standby: b.q("espera", "W", standby)?,
            warm_up: b.q("calentamiento", "s", 1.5)?,
            scales,
            timer: 0.0,
            up: false,
            share: 0.0,
            c_on: b.cmd("encender"),
            c_emit: b.cmd("emitir"),
            c_scale: b.cmd("escala"),
            always: b.flag("siempre", false),
            o_on: b.out("on", "")?,
            o_emit: b.out("emite", "")?,
            o_share: b.out("cuota", "%")?,
            o_range: b.out("alcance", "km")?,
            o_p: b.out("p", "W")?,
        })
    }

    /// The scale chosen (m) and the share of full power it takes.
    fn scale(&self, cx: &Cx) -> (f64, f64) {
        let (Some(&top), false) = (self.scales.last(), self.scales.is_empty()) else { return (0.0, 1.0) };
        let k = (cx.signals.get(self.c_scale).round().max(0.0) as usize).min(self.scales.len() - 1);
        (self.scales[k], (self.scales[k] / top).powi(4))
    }

    fn wanted(&self, cx: &Cx) -> bool {
        cx.working && (self.always || cx.signals.on(self.c_on))
    }
}

impl Machine for Device {
    fn kind(&self) -> &'static str {
        self.what.model()
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !self.wanted(cx) {
            return;
        }
        let emitting = self.up && self.what.radiates() && (self.what != Radio::Radar || cx.signals.on(self.c_emit));
        let (_, share) = self.scale(cx);
        io[0].demand = if emitting { self.standby + (self.power - self.standby).max(0.0) * share } else if self.what.radiates() { self.standby } else { self.power };
        io[0].priority = 120;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let fed = self.wanted(cx) && io[0].fed && io[0].share >= 0.5;
        self.timer = if fed { self.timer + cx.dt } else { 0.0 };
        self.up = fed && self.timer >= self.warm_up;
        let emitting = self.up && self.what.radiates() && (self.what != Radio::Radar || cx.signals.on(self.c_emit));
        let (range, share) = self.scale(cx);
        // (hurt, it radiates less)
        self.share = if emitting { share * f64::from(cx.health.clamp(0.0, 1.0)) } else { 0.0 };
        let s = &mut cx.signals;
        s.set(self.o_on, if self.up { 1.0 } else { 0.0 });
        s.set(self.o_emit, if emitting { 1.0 } else { 0.0 });
        s.set(self.o_share, self.share);
        s.set(self.o_range, range);
        s.set(self.o_p, io[0].got);
    }
    fn heat(&self) -> f64 {
        if self.up { self.standby.max(self.power * self.share) * 0.6 } else { 0.0 }
    }
    fn settling(&self) -> Option<bool> {
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

/// Anything that lets something fly: a cannon (as many a second as its rate while it is asked),
/// a rail or a dispenser (one each time it is asked). What flies is its data's (`municion`: a
/// shot, a guided missile or a decoy, by id): the owner of the ship reads it and makes it.
///
/// It fires while `fuego` is up, it is fed, and it has rounds; a cannon first spins up (`giro`)
/// and every round warms its barrel, which cools by itself: too hot (`t_max`) it holds its fire
/// until it is back under `t_listo`.
pub struct Launcher {
    ports: [PortSpec; 1],
    rate: f64,
    magazine: f64,
    single: bool,
    spin_up: f64,
    power: f64,
    standby: f64,
    /// Kelvin a round adds to the barrel, and the seconds it takes to lose 63 % of it.
    warms: f64,
    cools: f64,
    t_max: f64,
    t_ready: f64,
    // state
    rounds: f64,
    owed: f64,
    spin: f64,
    temp: f64,
    hot: bool,
    was_asked: bool,
    out: u32,
    firing: bool,
    // commands
    c_fire: SignalId,
    c_reload: SignalId,
    // telemetry
    o_rounds: SignalId,
    o_level: SignalId,
    o_ready: SignalId,
    o_firing: SignalId,
    o_temp: SignalId,
    o_hot: SignalId,
}

/// Where a barrel cools to (K).
const BARREL_REST: f64 = 290.0;

impl Launcher {
    pub fn new(b: &mut Build) -> Result<Launcher, String> {
        let magazine = b.f("cargador", 1.0).max(1.0);
        let single = match b.text("modo") {
            None | Some("auto") => false,
            Some("uno") => true,
            Some(m) => return Err(format!("{}: modo de arma desconocido '{m}' (auto, uno)", b.id)),
        };
        if b.text("municion").is_none() {
            return Err(format!("{}: un arma dice qué lanza ('municion')", b.id));
        }
        Ok(Launcher {
            ports: [PortSpec { role: "alimentacion", medium: Medium::Electrico }],
            rate: b.q("cadencia", "1/s", if single { 2.0 } else { 20.0 })?.max(0.01),
            magazine,
            single,
            spin_up: b.q("giro", "s", 0.0)?,
            power: b.q("potencia", "W", 400.0)?,
            standby: b.q("espera", "W", 15.0)?,
            warms: b.dq("calienta", 0.0)?,
            cools: b.q("enfria", "s", 8.0)?.max(0.1),
            t_max: b.q("t_max", "K", 900.0)?,
            t_ready: b.q("t_listo", "K", 700.0)?,
            rounds: (b.f("cargado", 1.0).clamp(0.0, 1.0) * magazine).round(),
            owed: 0.0,
            spin: 0.0,
            temp: BARREL_REST,
            hot: false,
            was_asked: false,
            out: 0,
            firing: false,
            c_fire: b.cmd("fuego"),
            c_reload: b.cmd("recargar"),
            o_rounds: b.out("municion", "")?,
            o_level: b.out("nivel", "%")?,
            o_ready: b.out("lista", "")?,
            o_firing: b.out("disparando", "")?,
            o_temp: b.out("t", "°C")?,
            o_hot: b.out("caliente", "")?,
        })
    }

    /// Rounds left in it.
    pub fn rounds(&self) -> u32 {
        self.rounds as u32
    }
}

impl Machine for Launcher {
    fn kind(&self) -> &'static str {
        "arma"
    }
    fn ports(&self) -> &[PortSpec] {
        &self.ports
    }
    fn plan(&mut self, cx: &mut Cx, io: &mut [PortIo]) {
        if !cx.working {
            return;
        }
        let asked = cx.signals.on(self.c_fire) && self.rounds >= 1.0;
        io[0].demand = if asked { self.power } else { self.standby };
        io[0].priority = 150;
    }
    fn step(&mut self, cx: &mut Cx, io: &[PortIo]) {
        let dt = cx.dt;
        let fed = cx.working && io[0].fed && io[0].share >= 0.5;
        let asked = cx.signals.on(self.c_fire);
        // (rearmed where it stands: whoever serves it says so)
        if cx.signals.on(self.c_reload) && !asked {
            self.rounds = self.magazine;
        }
        self.temp = approach(self.temp, BARREL_REST, self.cools, dt);
        if self.temp > self.t_max {
            self.hot = true;
        } else if self.temp < self.t_ready {
            self.hot = false;
        }
        let can = fed && !self.hot && self.rounds >= 1.0;
        let mut n = 0u32;
        if self.single {
            // one each time it is asked, no faster than its rate
            self.owed = (self.owed - dt).max(0.0);
            if asked && !self.was_asked && can && self.owed <= 0.0 {
                n = 1;
                self.owed = 1.0 / self.rate;
            }
        } else if asked && can {
            self.spin = (self.spin + dt).min(self.spin_up.max(dt));
            if self.spin >= self.spin_up {
                self.owed += self.rate * dt;
                n = (self.owed.floor() as u32).min(self.rounds as u32);
                self.owed -= f64::from(n);
            }
        } else {
            self.spin = (self.spin - dt * 2.0).max(0.0);
            self.owed = 0.0;
        }
        self.was_asked = asked;
        self.firing = n > 0 || (asked && can && !self.single);
        if n > 0 {
            self.rounds -= f64::from(n);
            self.temp += self.warms * f64::from(n);
            self.out += n;
        }
        let s = &mut cx.signals;
        s.set(self.o_rounds, self.rounds);
        s.set(self.o_level, self.rounds / self.magazine);
        s.set(self.o_ready, if can { 1.0 } else { 0.0 });
        s.set(self.o_firing, if self.firing { 1.0 } else { 0.0 });
        s.set(self.o_temp, self.temp);
        s.set(self.o_hot, if self.hot { 1.0 } else { 0.0 });
    }
    fn take_shots(&mut self) -> u32 {
        std::mem::take(&mut self.out)
    }
    fn heat(&self) -> f64 {
        if self.firing { self.power * 0.5 } else { 0.0 }
    }
    // (how warm it is and how far spun up follow the trigger, and with automatic fire the
    // trigger follows a solution that two copies work out a hair apart: what is left is what
    // makes a copy right or wrong)
    fn kept(&self, out: &mut Vec<f64>) {
        out.push(self.rounds);
    }
    fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.rounds, self.temp, self.spin, f64::from(u8::from(self.hot))]);
    }
    fn load(&mut self, s: &[f64]) {
        if let [r, t, sp, h, ..] = *s {
            (self.rounds, self.temp, self.spin, self.hot) = (r, t, sp, h >= 0.5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Env;
    use lunar_signals::{Store, Writer};
    use serde_json::{Map, Value, json};

    fn run(m: &mut dyn Machine, store: &mut Store, secs: f64) -> u32 {
        let (dt, mut io, mut shots) = (0.02, [PortIo::on(0, 0)], 0);
        for k in 0..(secs / dt).round() as usize {
            io[0].clear_ask();
            let mut cx = Cx { signals: store, env: Env::default(), health: 1.0, working: true, t: k as f64 * dt, dt };
            m.plan(&mut cx, &mut io);
            (io[0].fed, io[0].share, io[0].got) = (true, 1.0, io[0].demand);
            m.step(&mut cx, &io);
            shots += m.take_shots();
        }
        shots
    }

    fn params(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn a_cannon_fires_at_its_rate_and_runs_dry() {
        let mut store = Store::new();
        let (p, o) = (params(json!({ "municion": "x", "cadencia": 50.0, "cargador": 120, "giro": "0.2 s" })), Map::new());
        let mut gun = Launcher::new(&mut Build::new("canon", &p, &o, &mut store, Writer::Machine(0))).unwrap();
        let fire = store.find("canon.fuego").unwrap();
        assert_eq!(run(&mut gun, &mut store, 1.0), 0, "sin pedirlo no dispara");
        store.set(fire, 1.0);
        let n = run(&mut gun, &mut store, 1.0);
        assert!((38..=41).contains(&n), "50 por segundo tras 0,2 s de giro: {n}");
        assert_eq!(n + run(&mut gun, &mut store, 5.0), 120, "ni una más de las que lleva");
        assert_eq!(gun.rounds(), 0);
    }

    #[test]
    fn a_hot_barrel_holds_its_fire_and_comes_back() {
        let mut store = Store::new();
        let (p, o) = (params(json!({ "municion": "x", "cadencia": 50.0, "cargador": 5000, "calienta": 6.0, "enfria": "4 s", "t_max": "900 K", "t_listo": "600 K" })), Map::new());
        let mut gun = Launcher::new(&mut Build::new("canon", &p, &o, &mut store, Writer::Machine(0))).unwrap();
        store.set(store.find("canon.fuego").unwrap(), 1.0);
        let first = run(&mut gun, &mut store, 4.0);
        assert!(first < 190, "se calienta y para: {first}");
        assert!(store.on(store.find("canon.caliente").unwrap()));
        assert!(run(&mut gun, &mut store, 6.0) > 0, "enfriado, vuelve a tirar");
    }

    #[test]
    fn a_rail_lets_one_go_each_time_it_is_asked() {
        let mut store = Store::new();
        let (p, o) = (params(json!({ "municion": "x", "modo": "uno", "cargador": 2, "cadencia": 4.0 })), Map::new());
        let mut rail = Launcher::new(&mut Build::new("riel", &p, &o, &mut store, Writer::Machine(0))).unwrap();
        let fire = store.find("riel.fuego").unwrap();
        store.set(fire, 1.0);
        assert_eq!(run(&mut rail, &mut store, 2.0), 1, "mantenerlo no lanza el segundo");
        store.set(fire, 0.0);
        run(&mut rail, &mut store, 0.1);
        store.set(fire, 1.0);
        assert_eq!(run(&mut rail, &mut store, 0.1), 1);
        store.set(fire, 0.0);
        run(&mut rail, &mut store, 1.0);
        store.set(fire, 1.0);
        assert_eq!(run(&mut rail, &mut store, 1.0), 0, "vacío");
    }

    #[test]
    fn a_radar_radiates_by_its_range_scale() {
        let mut store = Store::new();
        let (p, o) = (params(json!({ "escalas": [10000.0, 40000.0, 80000.0], "potencia": "4 kW", "espera": "100 W", "calentamiento": "1 s" })), Map::new());
        let mut radar = Device::new(&mut Build::new("radar", &p, &o, &mut store, Writer::Machine(0)), Radio::Radar).unwrap();
        let get = |s: &Store, n: &str| s.get(s.find(n).unwrap());
        store.set(store.find("radar.encender").unwrap(), 1.0);
        run(&mut radar, &mut store, 2.0);
        assert_eq!(get(&store, "radar.on"), 1.0);
        assert_eq!(get(&store, "radar.emite"), 0.0, "encendido y callado hasta que se le manda emitir");
        store.set(store.find("radar.emitir").unwrap(), 1.0);
        store.set(store.find("radar.escala").unwrap(), 2.0);
        run(&mut radar, &mut store, 0.1);
        assert_eq!(get(&store, "radar.cuota"), 1.0);
        assert!((get(&store, "radar.p") - 4000.0).abs() < 1.0);
        // half the range: a sixteenth of the power
        store.set(store.find("radar.escala").unwrap(), 1.0);
        run(&mut radar, &mut store, 0.1);
        assert!((get(&store, "radar.cuota") - 1.0 / 16.0).abs() < 1e-9);
        assert_eq!(get(&store, "radar.alcance"), 40000.0);
    }
}
