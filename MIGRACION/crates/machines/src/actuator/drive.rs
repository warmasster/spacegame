//! What moves an actuator, and what it uses to do it. Each drive turns a command `u` (−1..1:
//! which way and how hard) and what its networks gave it into a push on the joint: a force, or a
//! velocity it holds up to a force (hydraulics, self-locking screws). A new kind of drive is a
//! variant here: everything else (joints, locks, sensors, networks) is shared.
use crate::net::{Medium, PortIo};
use lunar_signals::Q;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum DriveDef {
    /// A DC motor: no-load speed and stall torque at its voltage, a current limit.
    Electrico {
        #[serde(default)]
        tension: Option<Q>,
        velocidad_vacio: Q,
        par_bloqueo: Q,
        #[serde(default)]
        corriente_max: Option<Q>,
    },
    /// A hydraulic cylinder behind a solenoid valve: force = pressure × area, speed = flow / area.
    Hidraulico {
        diametro: Q,
        #[serde(default)]
        vastago: Option<Q>,
        /// Rated flow of its valve.
        caudal_valvula: Q,
        /// Trapped fluid holds the load up to this pressure (relief valve).
        #[serde(default)]
        presion_alivio: Option<Q>,
    },
    /// A pneumatic cylinder: force = pressure × area, cushioned, it gives a little under load.
    Neumatico {
        diametro: Q,
        #[serde(default)]
        caudal_valvula: Option<Q>,
    },
    /// A hand crank (the player turns it): its own coordinate per turn.
    Manual {
        /// Drive coordinate per crank turn (rad of the motor-side shaft, or m).
        avance: Q,
        #[serde(default)]
        fuerza: Option<Q>,
    },
    /// A spring or gas strut: preload plus stiffness along the drive, once its latch lets go.
    Muelle {
        precarga: Q,
        #[serde(default)]
        rigidez: Option<Q>,
        /// Drive coordinate at which the force is the preload.
        #[serde(default)]
        en: Option<f64>,
    },
    /// Explosive bolts or a gas generator: a force for a moment, once per charge.
    Pirotecnico {
        fuerza: Q,
        duracion: Q,
        #[serde(default = "one_u")]
        cargas: u32,
    },
    /// No drive: the load falls under gravity once released, braked by a damper.
    Gravedad { amortiguador: Q },
    /// A solenoid: pulls while powered, a spring returns it.
    Solenoide { fuerza: Q, potencia: Q },
}

fn one_u() -> u32 {
    1
}

/// What a drive does to its joint this step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Push {
    /// A generalized force on the joint (N or N·m).
    Force(f64),
    /// It drives the joint at speed `v` while that takes no more than `max` (generalized force);
    /// past that it stops and holds the joint still up to `hold` (a self-locking screw, trapped
    /// hydraulic fluid); past `hold` it gives way.
    Velocity { v: f64, max: f64, hold: f64 },
}

#[derive(Clone, Copy, Debug)]
pub enum Drive {
    Electric { volts: f64, w0: f64, stall: f64, i_max: f64, r: f64, ke: f64 },
    Hydraulic { area: f64, area_r: f64, flow: f64, relief: f64 },
    Pneumatic { area: f64, flow: f64 },
    Manual { per_turn: f64, force: f64 },
    Spring { preload: f64, k: f64, at: f64 },
    Pyro { force: f64, time: f64, charges: u32 },
    Gravity { damping: f64 },
    Solenoid { force: f64, power: f64 },
}

impl Drive {
    pub fn new(d: &DriveDef) -> Result<Drive, String> {
        let si = |q: &Q, u: &str| q.si_as(u).map_err(|e| e.0);
        let opt = |q: &Option<Q>, u: &str, or: f64| q.as_ref().map_or(Ok(or), |q| si(q, u));
        Ok(match d {
            DriveDef::Electrico { tension, velocidad_vacio, par_bloqueo, corriente_max } => {
                let volts = opt(tension, "V", 28.0)?;
                let w0 = si(velocidad_vacio, "Hz")?;
                let stall = si(par_bloqueo, "N*m")?;
                let ke = volts / w0.max(1e-3);
                let i_stall = stall / ke.max(1e-9);
                Drive::Electric { volts, w0, stall, i_max: opt(corriente_max, "A", i_stall)?, r: volts / i_stall.max(1e-9), ke }
            }
            DriveDef::Hidraulico { diametro, vastago, caudal_valvula, presion_alivio } => {
                let d = si(diametro, "m")?;
                let rod = opt(vastago, "m", d * 0.5)?;
                let area = std::f64::consts::PI * d * d / 4.0;
                Drive::Hydraulic { area, area_r: area - std::f64::consts::PI * rod * rod / 4.0, flow: si(caudal_valvula, "m3/s")?, relief: opt(presion_alivio, "Pa", 25e6)? }
            }
            DriveDef::Neumatico { diametro, caudal_valvula } => {
                let d = si(diametro, "m")?;
                Drive::Pneumatic { area: std::f64::consts::PI * d * d / 4.0, flow: opt(caudal_valvula, "kg/s", 0.02)? }
            }
            DriveDef::Manual { avance, fuerza } => Drive::Manual { per_turn: avance.si().map_err(|e| e.0)?, force: opt(fuerza, "N", 250.0)? },
            DriveDef::Muelle { precarga, rigidez, en } => Drive::Spring { preload: si(precarga, "N")?, k: opt(rigidez, "N/m", 0.0)?, at: en.unwrap_or(0.0) },
            DriveDef::Pirotecnico { fuerza, duracion, cargas } => Drive::Pyro { force: si(fuerza, "N")?, time: si(duracion, "s")?, charges: *cargas },
            DriveDef::Gravedad { amortiguador } => Drive::Gravity { damping: si(amortiguador, "N*s/m")? },
            DriveDef::Solenoide { fuerza, potencia } => Drive::Solenoid { force: si(fuerza, "N")?, power: si(potencia, "W")? },
        })
    }

    /// The networks it draws on: (role, medium).
    pub fn ports(&self) -> &'static [(&'static str, Medium)] {
        match self {
            Drive::Electric { .. } => &[("motor", Medium::Electrico)],
            Drive::Hydraulic { .. } => &[("fluido", Medium::Hidraulico), ("valvula", Medium::Electrico)],
            Drive::Pneumatic { .. } => &[("gas", Medium::Neumatico), ("valvula", Medium::Electrico)],
            Drive::Pyro { .. } => &[("disparo", Medium::Electrico)],
            Drive::Solenoid { .. } => &[("bobina", Medium::Electrico)],
            Drive::Manual { .. } | Drive::Spring { .. } | Drive::Gravity { .. } => &[],
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Drive::Electric { .. } => "eléctrico",
            Drive::Hydraulic { .. } => "hidráulico",
            Drive::Pneumatic { .. } => "neumático",
            Drive::Manual { .. } => "manual",
            Drive::Spring { .. } => "muelle",
            Drive::Pyro { .. } => "pirotécnico",
            Drive::Gravity { .. } => "gravedad",
            Drive::Solenoid { .. } => "solenoide",
        }
    }
}

/// What a drive asks of its networks for the coming step, given the command and the motion.
/// `io` are its ports in `Drive::ports` order. `sd` is the drive's coordinate speed.
pub fn plan(d: &Drive, u: f64, sd: f64, io: &mut [PortIo]) {
    match *d {
        Drive::Electric { volts, stall, ke, .. } => {
            if u != 0.0 {
                // what the motor will draw: the torque it works against, plus its spin
                let i = (stall / ke) * (0.15 + 0.85 * u.abs());
                io[0].demand = volts * i.min(stall / ke);
                io[0].priority = 100;
            }
            let _ = sd;
        }
        Drive::Hydraulic { flow, .. } => {
            if u != 0.0 {
                // the valve meters flow by its opening
                io[0].demand = flow * u.abs();
                io[0].priority = 100;
                io[1].demand = 18.0;
                io[1].priority = 160;
            }
        }
        Drive::Pneumatic { area, flow } => {
            if u != 0.0 {
                // gas to fill the cylinder as it strokes, at its pressure (roughly 1 kg/m³ per bar)
                io[0].demand = (area * sd.abs() * 8.0).max(flow * 0.1).min(flow);
                io[0].priority = 100;
                io[1].demand = 12.0;
                io[1].priority = 160;
            }
        }
        Drive::Pyro { .. } => {
            io[0].demand = 2.0;
            io[0].priority = 250;
        }
        Drive::Solenoid { power, .. } => {
            if u > 0.0 {
                io[0].demand = power;
                io[0].priority = 120;
            }
        }
        Drive::Manual { .. } | Drive::Spring { .. } | Drive::Gravity { .. } => {}
    }
}

/// The state a drive keeps between steps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DriveState {
    /// Motor current (A), cylinder pressure (Pa)... what its gauge shows.
    pub reading: f64,
    /// Winding temperature rise (K) of an electric motor.
    pub heat: f64,
    /// Pyro: time left of a firing, charges used. Spring: released.
    pub timer: f64,
    pub used: f64,
    pub released: bool,
}

/// The push on the joint. `s` the drive coordinate and `n` = ds/dq, `eff` the transmission's
/// efficiency, `locking` whether it does not back-drive, `qd` the joint speed, `crank` the crank
/// speed (turns/s) for manual drives.
#[allow(clippy::too_many_arguments)]
pub fn push(d: &Drive, st: &mut DriveState, u: f64, s: f64, n: f64, eff: f64, locking: bool, qd: f64, crank: f64, io: &[PortIo], dt: f64) -> Push {
    let n_abs = n.abs().max(1e-9);
    match *d {
        Drive::Electric { volts, w0, stall, i_max, r, ke } => {
            let supply = if io[0].fed && io[0].share >= 0.5 { (io[0].level / volts).clamp(0.0, 1.2) * io[0].share.min(1.0) } else { 0.0 };
            let v = u * volts * supply;
            let w = qd * n;
            let i = ((v - ke * w) / r).clamp(-i_max, i_max);
            st.reading = i;
            // I²R warms the winding; it cools with its own time constant
            st.heat += (i * i * r * 0.02 - st.heat * 0.05) * dt;
            if locking {
                // a self-locking drive sets the speed; it stalls when the load beats its torque
                // and the screw holds whatever the structure holds
                Push::Velocity { v: u * w0 * supply / n, max: stall * supply * n_abs * eff, hold: stall * 60.0 * n_abs }
            } else {
                Push::Force(ke * i * n * eff)
            }
        }
        Drive::Hydraulic { area, area_r, flow, relief } => {
            let line = io[0].fed;
            let p = if line { io[0].level.max(0.0) } else { 0.0 };
            let valve = io[1].fed && io[1].share >= 0.5;
            st.reading = p;
            if !line {
                // the line is cut: nothing holds the fluid, the load sinks against the orifice
                return Push::Force(-qd * n * n * 4.0e5 * area);
            }
            if u == 0.0 || !valve || p < 1e5 {
                // valve shut: trapped fluid holds up to the relief pressure
                return Push::Velocity { v: 0.0, max: relief * area * n_abs, hold: relief * area * n_abs };
            }
            let a = if u > 0.0 { area } else { area_r };
            let q = (flow * u.abs()).min(if io[0].demand > 0.0 { io[0].got.max(io[0].demand * io[0].share) } else { flow * 0.2 });
            let speed = u.signum() * q / a / n_abs * n.signum();
            Push::Velocity { v: speed, max: p * a * n_abs * eff, hold: relief * a * n_abs }
        }
        Drive::Pneumatic { area, .. } => {
            let p = if io[0].fed { io[0].level.max(0.0) } else { 0.0 };
            let valve = io[1].fed && io[1].share >= 0.5;
            st.reading = p;
            let f = if valve { u * p * area } else { 0.0 };
            // the gas cushions: some damping always
            Push::Force(f * n * eff - qd * n * n * 2.0e3)
        }
        Drive::Manual { per_turn, force } => {
            Push::Velocity { v: crank * per_turn / n_abs * n.signum(), max: force * 20.0 * n_abs, hold: if locking { force * 2000.0 * n_abs } else { 0.0 } }
        }
        Drive::Spring { preload, k, at } => {
            if !st.released && u > 0.0 {
                st.released = true;
            }
            // it pushes out along its coordinate, less as it extends past `at`
            if st.released { Push::Force((preload - k * (s - at)).max(0.0) * n) } else { Push::Force(0.0) }
        }
        Drive::Pyro { force, time, charges } => {
            let armed = io[0].fed && io[0].share >= 0.5;
            if u > 0.0 && armed && st.timer <= 0.0 && st.used < f64::from(charges) {
                st.timer = time;
                st.used += 1.0;
            }
            if st.timer > 0.0 {
                st.timer -= dt;
                Push::Force(force * n)
            } else {
                Push::Force(0.0)
            }
        }
        Drive::Gravity { damping } => Push::Force(-damping * qd * n * n),
        Drive::Solenoid { force, .. } => {
            let on = u > 0.0 && io[0].fed && io[0].share >= 0.5;
            Push::Force(if on { force * n } else { -force * 0.3 * n })
        }
    }
}
