//! Actuators: what moves the moving parts of a structure (ramps, doors, landing gear, tilting
//! nacelles, radiators, hatches, antennas, valves, shutters...). One actuator is
//!
//! - a **drive** (`drive.rs`): what it uses and how it pushes — electric motor, hydraulic or
//!   pneumatic cylinder, hand crank, spring, explosive charge, gravity, solenoid;
//! - a **linkage** (`linkage.rs`): how the drive's own motion becomes the joint's — direct, a
//!   gearbox, a lead screw, a rod between two anchors (with its real geometry: the force it can
//!   give changes along the travel), a winch;
//! - a **control law**: open/close, position or speed, from a signal;
//! - **locks** at the ends (up-locks, down-locks), each released by a solenoid, by pressure or
//!   by hand before it can move;
//! - **sensors**: position, end switches, locks, force, current or pressure, stalls;
//! - **wear** and **provenance**: cycles, a crack that grows with each loaded cycle, a serial.
//!
//! Several actuators may move one joint (two rams on a ramp): their pushes add up. The joint
//! itself (its inertia, the gravity on what it carries, its stops) belongs to the owner, which
//! calls `joint::integrate`.
pub mod drive;
pub mod joint;
pub mod linkage;

use crate::{
    machine::{Cx, PortSpec},
    net::{Medium, PortIo},
    wear::{Provenance, Wear, hash},
};
use drive::{Drive, DriveDef, DriveState, Push};
use linkage::{JointView, Linkage, LinkageDef};
use lunar_signals::{Q, SignalId, Store, Writer};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LawDef {
    /// "abrir_cerrar" (the signal ≥ 0.5 goes to the end), "posicion" (0..1 of the travel) or
    /// "velocidad" (−1..1).
    #[serde(default = "open_close")]
    pub modo: String,
    /// The command signal (default `<id>.orden`).
    #[serde(default)]
    pub orden: Option<String>,
    /// Swap the ends.
    #[serde(default)]
    pub invertir: bool,
    /// Share of full speed it asks (0..1).
    #[serde(default = "one")]
    pub velocidad: f64,
    /// Share of the travel over which it slows into its target.
    #[serde(default = "zone")]
    pub frenado: f64,
}

fn open_close() -> String {
    "abrir_cerrar".into()
}
fn one() -> f64 {
    1.0
}
fn zone() -> f64 {
    0.08
}

impl Default for LawDef {
    fn default() -> Self {
        LawDef { modo: open_close(), orden: None, invertir: false, velocidad: 1.0, frenado: zone() }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockDef {
    /// "inicio", "fin" or a joint position.
    pub en: serde_json::Value,
    /// "solenoide" (needs power on its `cerrojo` port), "hidraulico" (needs line pressure),
    /// "manual" (a signal `<id>.liberar`), or "ninguna" (a detent that any push overcomes).
    #[serde(default = "solenoid")]
    pub liberacion: String,
    #[serde(default)]
    pub tiempo: Option<Q>,
    #[serde(default)]
    pub nombre: Option<String>,
}

fn solenoid() -> String {
    "solenoide".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActuatorDef {
    /// The joint it moves.
    pub articulacion: String,
    pub accionamiento: DriveDef,
    #[serde(default)]
    pub transmision: Option<LinkageDef>,
    #[serde(default)]
    pub control: LawDef,
    #[serde(default)]
    pub bloqueos: Vec<LockDef>,
    /// Networks of its ports by role ("fluido": "hid_a", "valvula": "bus_ess"...).
    #[serde(default)]
    pub redes: std::collections::BTreeMap<String, String>,
    /// The part that is its body (its health is the actuator's).
    #[serde(default)]
    pub pieza: Option<String>,
    #[serde(default)]
    pub nombre: Option<String>,
    #[serde(default)]
    pub fabricante: Option<String>,
    #[serde(default)]
    pub modelo: Option<String>,
    /// Cycles at full load it was designed for.
    #[serde(default)]
    pub vida: Option<f64>,
    /// Wired with conduits of its own (default): false, it is connected without any (its wiring
    /// hidden: nothing to see nor to cut). Chosen where it is placed, not by its kind.
    #[serde(default)]
    pub cableado: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    OpenClose,
    Position,
    Speed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Release {
    Solenoid,
    Pressure,
    Hand,
    Detent,
}

#[derive(Clone, Copy, Debug)]
struct Lock {
    /// Joint position (resolved from inicio/fin when the joint is known).
    at: f64,
    end: i8,
    release: Release,
    time: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LockState {
    pub engaged: bool,
    /// Seconds of release applied so far.
    pub releasing: f64,
}

/// Telemetry of an actuator.
#[derive(Clone, Copy, Debug)]
struct Out {
    pos: SignalId,
    speed: SignalId,
    force: SignalId,
    reading: SignalId,
    at_start: SignalId,
    at_end: SignalId,
    locked: SignalId,
    stalled: SignalId,
    state: SignalId,
    wear: SignalId,
    cycles: SignalId,
    temp: SignalId,
}

pub struct Actuator {
    pub id: String,
    pub name: String,
    pub joint_name: String,
    /// Joint index (set by the owner).
    pub joint: usize,
    pub part: Option<String>,
    pub drive: Drive,
    pub link: Linkage,
    mode: Mode,
    invert: bool,
    speed: f64,
    brake: f64,
    locks: Vec<Lock>,
    lock_defs: Vec<LockDef>,
    pub lock_state: Vec<LockState>,
    ports: Vec<PortSpec>,
    /// Port index of the lock release (solenoid), if any.
    lock_port: Option<usize>,
    /// Networks by role, as the data named them.
    pub nets: Vec<(String, String)>,
    pub st: DriveState,
    pub wear: Wear,
    pub provenance: Provenance,
    // motion bookkeeping
    u: f64,
    stall_t: f64,
    last_end: i8,
    peak: f64,
    peak_max: f64,
    applied: f64,
    cmd: SignalId,
    crank: SignalId,
    hand_release: SignalId,
    out: Out,
}

impl Actuator {
    pub fn new(id: &str, d: &ActuatorDef, store: &mut Store, writer: Writer, seed: u64) -> Result<Actuator, String> {
        let err = |e: String| format!("actuador {id}: {e}");
        let drive = Drive::new(&d.accionamiento).map_err(err)?;
        let link = match &d.transmision {
            Some(l) => Linkage::new(l).map_err(err)?,
            None => Linkage::Ratio { n: 1.0, locking: false, eff: 0.9 },
        };
        let mode = match d.control.modo.as_str() {
            "abrir_cerrar" => Mode::OpenClose,
            "posicion" | "posición" => Mode::Position,
            "velocidad" => Mode::Speed,
            m => return Err(err(format!("modo de control '{m}' (abrir_cerrar, posicion, velocidad)"))),
        };
        let mut ports: Vec<PortSpec> = drive.ports().iter().map(|&(role, medium)| PortSpec { role, medium }).collect();
        let mut locks = Vec::new();
        let mut lock_port = None;
        for l in &d.bloqueos {
            let release = match l.liberacion.as_str() {
                "solenoide" => Release::Solenoid,
                "hidraulico" | "hidráulico" => Release::Pressure,
                "manual" => Release::Hand,
                "ninguna" => Release::Detent,
                r => return Err(err(format!("liberación '{r}' (solenoide, hidraulico, manual, ninguna)"))),
            };
            if release == Release::Solenoid && lock_port.is_none() {
                ports.push(PortSpec { role: "cerrojo", medium: Medium::Electrico });
                lock_port = Some(ports.len() - 1);
            }
            let (at, end) = match &l.en {
                serde_json::Value::String(s) if s == "inicio" => (0.0, -1),
                serde_json::Value::String(s) if s == "fin" => (0.0, 1),
                serde_json::Value::Number(n) => (n.as_f64().unwrap_or(0.0), 0),
                v => return Err(err(format!("bloqueo en {v} (inicio, fin o una posición)"))),
            };
            let time = match &l.tiempo {
                Some(q) => q.si_as("s").map_err(|e| err(e.0))?,
                None => 0.3,
            };
            locks.push(Lock { at, end, release, time });
        }
        for role in d.redes.keys() {
            if !ports.iter().any(|p| p.role == role) {
                return Err(err(format!("no tiene puerto '{role}' (tiene: {})", ports.iter().map(|p| p.role).collect::<Vec<_>>().join(", "))));
            }
        }
        let w = |field: &str, unit: &str, store: &mut Store| -> Result<SignalId, String> {
            let s = store.define_unit(&format!("{id}.{field}"), unit, 0.0).map_err(|e| err(e.0))?;
            store.claim(s, writer.clone()).map_err(err)?;
            Ok(s)
        };
        let reading_unit = match drive {
            Drive::Electric { .. } => "A",
            Drive::Hydraulic { .. } | Drive::Pneumatic { .. } => "MPa",
            _ => "",
        };
        let out = Out {
            pos: w("pos", "%", store)?,
            speed: w("vel", "", store)?,
            force: w("fuerza", "kN", store)?,
            reading: w("lectura", reading_unit, store)?,
            at_start: w("en_inicio", "", store)?,
            at_end: w("en_fin", "", store)?,
            locked: w("bloqueado", "", store)?,
            stalled: w("atasco", "", store)?,
            state: w("estado", "", store)?,
            wear: w("desgaste", "%", store)?,
            cycles: w("ciclos", "", store)?,
            temp: w("t_motor", "K", store)?,
        };
        let cmd = store.define(d.control.orden.as_deref().unwrap_or(&format!("{id}.orden")));
        let maker = d.fabricante.clone().unwrap_or_else(|| "Genérico".into());
        let model = d.modelo.clone().unwrap_or_else(|| format!("{} {}", drive.name(), id));
        let provenance = Provenance::new(&maker, &model, hash(id, seed));
        // a bad lot leaves bigger flaws
        let flaw = 1.0 + (provenance.fingerprint[0].abs() * 0.5);
        let wear = Wear::new(0.1e-3, 3e-3, d.vida.unwrap_or(20_000.0)).flawed(flaw);
        Ok(Actuator {
            id: id.to_string(),
            name: d.nombre.clone().unwrap_or_else(|| id.to_string()),
            joint_name: d.articulacion.clone(),
            joint: usize::MAX,
            part: d.pieza.clone(),
            drive,
            link,
            mode,
            invert: d.control.invertir,
            speed: d.control.velocidad.clamp(0.0, 1.0),
            brake: d.control.frenado.max(1e-3),
            lock_state: vec![LockState::default(); locks.len()],
            locks,
            lock_defs: d.bloqueos.clone(),
            ports,
            lock_port,
            nets: d.redes.iter().map(|(a, b)| (a.clone(), b.clone())).collect(),
            st: DriveState::default(),
            wear,
            provenance,
            u: 0.0,
            stall_t: 0.0,
            last_end: 0,
            peak: 0.0,
            peak_max: 0.0,
            applied: 0.0,
            cmd,
            crank: store.define(&format!("{id}.manivela")),
            hand_release: store.define(&format!("{id}.liberar")),
            out,
        })
    }

    pub fn ports(&self) -> &[PortSpec] {
        &self.ports
    }

    /// Resolve end locks against the joint's travel (called once the joint is known).
    pub fn bind(&mut self, joint: usize, j: &JointView) {
        self.joint = joint;
        for l in &mut self.locks {
            match l.end {
                -1 => l.at = j.lo,
                1 => l.at = j.hi,
                _ => {}
            }
        }
        // engage the locks it starts in
        for (l, s) in self.locks.iter().zip(&mut self.lock_state) {
            s.engaged = (j.q - l.at).abs() <= (j.hi - j.lo).abs() * 0.015;
        }
    }

    /// Where the command wants the joint, and the drive command u (−1..1).
    fn law(&self, cx: &Cx, j: &JointView) -> (f64, f64) {
        let c = cx.signals.get(self.cmd);
        let range = (j.hi - j.lo).abs().max(1e-6);
        let (lo, hi) = if self.invert { (j.hi, j.lo) } else { (j.lo, j.hi) };
        match self.mode {
            Mode::OpenClose => {
                let target = if c >= 0.5 { hi } else { lo };
                let e = (target - j.q) / range;
                let u = if e.abs() < 0.002 { 0.0 } else { (e / self.brake).clamp(-1.0, 1.0) };
                (target, u.signum() * u.abs().max(0.12).min(1.0) * self.speed * f64::from(u8::from(u != 0.0)))
            }
            Mode::Position => {
                let target = lo + (hi - lo) * c.clamp(0.0, 1.0);
                let e = (target - j.q) / range;
                let u = if e.abs() < 0.002 { 0.0 } else { (e / self.brake).clamp(-1.0, 1.0) * self.speed };
                (target, u)
            }
            Mode::Speed => (j.q, (c.clamp(-1.0, 1.0) * if self.invert { -1.0 } else { 1.0 }) * self.speed),
        }
    }

    /// Before the networks are solved: what the drive and its lock releases ask.
    pub fn plan(&mut self, cx: &mut Cx, j: &JointView, io: &mut [PortIo]) {
        if !cx.working {
            self.u = 0.0;
            return;
        }
        let (_, u) = self.law(cx, j);
        // the drive works in its own coordinate: it turns the joint direction through ds/dq (a
        // winch pays out at its speed: the load follows by its weight)
        let n = self.link.ratio(j, j.q);
        self.u = u;
        let sd = j.qd * n;
        drive::plan(&self.drive, u * n.signum(), sd, io);
        if let Some(k) = self.lock_port
            && self.lock_state.iter().any(|l| l.engaged)
            && u != 0.0
        {
            io[k].demand = 30.0;
            io[k].priority = 200;
        }
    }

    /// The push on its joint this step, or None while a lock holds it.
    pub fn push(&mut self, cx: &mut Cx, j: &JointView, io: &[PortIo], dt: f64) -> Option<Push> {
        let u = if cx.working { self.u } else { 0.0 };
        // locks: released by their means while the command wants to leave
        let mut held = false;
        for (k, l) in self.locks.iter().enumerate() {
            let s = &mut self.lock_state[k];
            if !s.engaged {
                continue;
            }
            // the command wants away from the lock
            let leaving = if (l.at - j.lo).abs() < 1e-9 {
                u > 0.0
            } else if (l.at - j.hi).abs() < 1e-9 {
                u < 0.0
            } else {
                u != 0.0
            };
            let can = match l.release {
                Release::Solenoid => self.lock_port.is_some_and(|p| io[p].fed && io[p].share >= 0.5),
                Release::Pressure => io.first().is_some_and(|p| p.fed && p.level > 5e6),
                Release::Hand => cx.signals.on(self.hand_release),
                Release::Detent => true,
            };
            if leaving && can {
                s.releasing += dt;
                if s.releasing >= l.time {
                    s.engaged = false;
                    s.releasing = 0.0;
                }
            } else if !leaving {
                s.releasing = 0.0;
            }
            if s.engaged {
                held = true;
            }
        }
        if held {
            return None;
        }
        let s = self.link.s(j, j.q);
        let n = self.link.ratio(j, j.q);
        let crank = cx.signals.get(self.crank);
        let crank_u = if matches!(self.drive, drive::Drive::Manual { .. }) { crank } else { 0.0 };
        let p = drive::push(&self.drive, &mut self.st, u * n.signum(), s, n, self.link.efficiency(), self.link.self_locking(), j.qd, crank_u, io, dt);
        Some(match p {
            // a cable only pulls
            Push::Force(f) if self.link.pulls_only() && f * n > 0.0 => Push::Force(0.0),
            p => p,
        })
    }

    /// After the joint moved: locks catch at their ends, sensors, wear, telemetry.
    pub fn after(&mut self, cx: &mut Cx, j: &JointView, applied: f64, max: f64) {
        self.applied = applied;
        let range = (j.hi - j.lo).abs().max(1e-6);
        let u = self.u;
        for (l, s) in self.locks.iter().zip(&mut self.lock_state) {
            // it catches the joint arriving (or resting) at it, not one being driven away
            let leaving = if (l.at - j.lo).abs() < 1e-9 {
                u > 0.0
            } else if (l.at - j.hi).abs() < 1e-9 {
                u < 0.0
            } else {
                u != 0.0
            };
            if !s.engaged && !leaving && (j.q - l.at).abs() <= range * 0.012 && j.qd.abs() < range * 0.5 {
                s.engaged = true;
            }
        }
        // a commanded drive that does not move is stalled (an obstruction, a load too big)
        let (target, _) = self.law(cx, j);
        let off = (target - j.q).abs() / range;
        if self.u != 0.0 && j.qd.abs() < range * 0.01 && off > 0.02 && !self.lock_state.iter().any(|l| l.engaged) {
            self.stall_t += cx.dt;
        } else {
            self.stall_t = 0.0;
        }
        // cycles: from one end to the other, at the peak load share of the travel
        self.peak = self.peak.max(applied.abs());
        self.peak_max = self.peak_max.max(max.abs());
        let end = if (j.q - j.lo).abs() <= range * 0.015 {
            -1
        } else if (j.q - j.hi).abs() <= range * 0.015 {
            1
        } else {
            0
        };
        if end != 0 && end != self.last_end {
            if self.last_end != 0 {
                let share = if self.peak_max > 0.0 { self.peak / self.peak_max } else { 0.5 };
                self.wear.cycle(share.clamp(0.0, 1.5));
            }
            self.last_end = end;
            self.peak = 0.0;
            self.peak_max = 0.0;
        }
        if j.qd.abs() > 1e-4 {
            self.wear.hours += cx.dt / 3600.0;
        }
        let s = &mut cx.signals;
        let o = self.out;
        s.set(o.pos, (j.q - j.lo) / (j.hi - j.lo));
        s.set(o.speed, j.qd);
        let n = self.link.ratio(j, j.q);
        s.set(o.force, if n.abs() > 1e-9 { applied / n } else { 0.0 });
        s.set(o.reading, self.st.reading);
        s.set(o.at_start, if end == -1 { 1.0 } else { 0.0 });
        s.set(o.at_end, if end == 1 { 1.0 } else { 0.0 });
        s.set(o.locked, if self.lock_state.iter().any(|l| l.engaged) { 1.0 } else { 0.0 });
        s.set(o.stalled, if self.stall_t > 0.6 { 1.0 } else { 0.0 });
        let state = if !cx.working {
            4.0
        } else if self.stall_t > 0.6 {
            2.0
        } else if self.lock_state.iter().any(|l| l.engaged) {
            3.0
        } else if j.qd.abs() > range * 0.01 {
            1.0
        } else {
            0.0
        };
        s.set(o.state, state);
        s.set(o.wear, self.wear.share());
        s.set(o.cycles, self.wear.cycles);
        s.set(o.temp, 293.15 + self.st.heat);
    }

    /// The lock definitions (names for the HUD).
    pub fn lock_names(&self) -> impl Iterator<Item = &str> {
        self.lock_defs.iter().map(|l| l.nombre.as_deref().unwrap_or("bloqueo"))
    }

    pub fn save(&self, out: &mut Vec<f64>) {
        out.extend([self.st.reading, self.st.heat, self.st.timer, self.st.used, f64::from(u8::from(self.st.released)), self.wear.cycles, self.wear.hours, self.wear.u, f64::from(self.last_end)]);
        for l in &self.lock_state {
            out.extend([f64::from(u8::from(l.engaged)), l.releasing]);
        }
    }

    pub fn load(&mut self, s: &[f64]) {
        if s.len() < 9 {
            return;
        }
        (self.st.reading, self.st.heat, self.st.timer, self.st.used) = (s[0], s[1], s[2], s[3]);
        self.st.released = s[4] > 0.5;
        (self.wear.cycles, self.wear.hours, self.wear.u) = (s[5], s[6], s[7]);
        self.last_end = s[8] as i8;
        for (k, l) in self.lock_state.iter_mut().enumerate() {
            if let (Some(&e), Some(&r)) = (s.get(9 + 2 * k), s.get(10 + 2 * k)) {
                l.engaged = e > 0.5;
                l.releasing = r;
            }
        }
    }
}
