//! How a drive's own coordinate `s` (a motor shaft's angle, a cylinder's length) relates to the
//! joint it moves (`q`: a hinge angle or a slide). Each transmission gives `s(q)` and `ds/dq`;
//! by the work done, a force `F` along `s` is a generalized force `F·ds/dq` on the joint.
use serde::Deserialize;

/// The joint an actuator moves, as it is now (from its owner).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointView {
    /// Position and speed (rad or m, per second).
    pub q: f64,
    pub qd: f64,
    pub lo: f64,
    pub hi: f64,
    pub hinge: bool,
    /// Axis (unit) and a point on it, in the parent frame.
    pub axis: [f64; 3],
    pub pivot: [f64; 3],
    /// Inertia about the joint (kg·m² or kg) and the generalized force of gravity on what it moves.
    pub inertia: f64,
    pub gravity: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkageDef {
    /// The drive turns (or slides) the joint itself: s = q × ratio.
    Directo {
        #[serde(default = "one")]
        reduccion: f64,
        #[serde(default)]
        autobloqueo: bool,
        #[serde(default = "eta")]
        eficiencia: f64,
    },
    /// A gearbox: the motor turns `reduccion` times per joint radian (or per metre).
    Reductor {
        reduccion: f64,
        #[serde(default)]
        autobloqueo: bool,
        #[serde(default = "eta")]
        eficiencia: f64,
    },
    /// A lead screw on a slide: `paso` m per motor turn (self-locking below a pitch).
    Husillo {
        paso: lunar_signals::Q,
        #[serde(default = "eta_screw")]
        eficiencia: f64,
    },
    /// A linear actuator between a fixed anchor (parent frame) and a moving one (the moving part's
    /// frame at q = 0): s is its length.
    Biela {
        anclaje_fijo: [f64; 3],
        anclaje_movil: [f64; 3],
        #[serde(default = "eta")]
        eficiencia: f64,
        /// It is itself a screw jack (an electric linear actuator): self-locking.
        #[serde(default)]
        autobloqueo: bool,
        /// The pitch of that screw (m of rod for each turn of its motor): what an electric
        /// drive needs to be a ram. None: the drive's own coordinate is the rod's length (a
        /// hydraulic cylinder).
        #[serde(default)]
        paso: Option<lunar_signals::Q>,
    },
    /// A winch: a drum of `radio` pulling a cable to the moving anchor (only pulls).
    Cable {
        radio: lunar_signals::Q,
        anclaje_fijo: [f64; 3],
        anclaje_movil: [f64; 3],
        /// Motor turns per drum turn.
        #[serde(default = "one")]
        reduccion: f64,
    },
}

fn one() -> f64 {
    1.0
}
fn eta() -> f64 {
    0.9
}
fn eta_screw() -> f64 {
    0.35
}

#[derive(Clone, Copy, Debug)]
pub enum Linkage {
    Ratio { n: f64, locking: bool, eff: f64 },
    Rod { a: [f64; 3], b: [f64; 3], eff: f64, locking: bool, drum: f64, pulls_only: bool },
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// `p` (moving frame at q = 0) where the joint at `q` puts it (Rodrigues for a hinge).
pub fn moved(j: &JointView, p: [f64; 3], q: f64) -> [f64; 3] {
    if j.hinge {
        let k = j.axis;
        let v = sub(p, j.pivot);
        let (s, c) = q.sin_cos();
        let r = add(add(scale(v, c), scale(cross(k, v), s)), scale(k, dot(k, v) * (1.0 - c)));
        add(r, j.pivot)
    } else {
        add(p, scale(j.axis, q))
    }
}

impl Linkage {
    pub fn new(d: &LinkageDef) -> Result<Linkage, String> {
        Ok(match d {
            LinkageDef::Directo { reduccion, autobloqueo, eficiencia } => Linkage::Ratio { n: *reduccion, locking: *autobloqueo, eff: *eficiencia },
            LinkageDef::Reductor { reduccion, autobloqueo, eficiencia } => Linkage::Ratio { n: *reduccion, locking: *autobloqueo, eff: *eficiencia },
            LinkageDef::Husillo { paso, eficiencia } => {
                let p = paso.si_as("m").map_err(|e| e.0)?;
                // a screw with a lead angle under its friction angle does not back-drive
                Linkage::Ratio { n: std::f64::consts::TAU / p.max(1e-6), locking: *eficiencia < 0.5, eff: *eficiencia }
            }
            LinkageDef::Biela { anclaje_fijo, anclaje_movil, eficiencia, autobloqueo, paso } => {
                let drum = match paso {
                    Some(p) => p.si_as("m").map_err(|e| e.0)? / std::f64::consts::TAU,
                    None => 0.0,
                };
                Linkage::Rod { a: *anclaje_fijo, b: *anclaje_movil, eff: *eficiencia, locking: *autobloqueo, drum, pulls_only: false }
            }
            LinkageDef::Cable { radio, anclaje_fijo, anclaje_movil, reduccion } => {
                let r = radio.si_as("m").map_err(|e| e.0)?;
                Linkage::Rod { a: *anclaje_fijo, b: *anclaje_movil, eff: 0.85, locking: true, drum: r / reduccion.max(1e-3), pulls_only: true }
            }
        })
    }

    /// The drive's coordinate at the joint's `q`.
    pub fn s(&self, j: &JointView, q: f64) -> f64 {
        match *self {
            Linkage::Ratio { n, .. } => q * n,
            Linkage::Rod { a, b, drum, .. } => {
                let l = len(sub(moved(j, b, q), a));
                if drum > 0.0 { l / drum } else { l }
            }
        }
    }

    /// ds/dq at `q` (central difference for rods).
    pub fn ratio(&self, j: &JointView, q: f64) -> f64 {
        match *self {
            Linkage::Ratio { n, .. } => n,
            Linkage::Rod { .. } => {
                let h = if j.hinge { 1e-4 } else { 1e-5 };
                (self.s(j, q + h) - self.s(j, q - h)) / (2.0 * h)
            }
        }
    }

    pub fn self_locking(&self) -> bool {
        match *self {
            Linkage::Ratio { locking, .. } | Linkage::Rod { locking, .. } => locking,
        }
    }

    pub fn efficiency(&self) -> f64 {
        match *self {
            Linkage::Ratio { eff, .. } | Linkage::Rod { eff, .. } => eff,
        }
    }

    /// Only pulls (a cable).
    pub fn pulls_only(&self) -> bool {
        matches!(*self, Linkage::Rod { pulls_only: true, .. })
    }

    /// The two ends of a rod or cable now (parent frame), to draw it.
    pub fn ends(&self, j: &JointView) -> Option<([f64; 3], [f64; 3])> {
        match *self {
            Linkage::Rod { a, b, .. } => Some((a, moved(j, b, j.q))),
            Linkage::Ratio { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ram_on_a_hinge() {
        // a ramp hinged on x at the origin, a ram from (0, 2, 0.2) to a point 1 m down the ramp
        let j = JointView { hinge: true, axis: [1.0, 0.0, 0.0], pivot: [0.0; 3], lo: 0.0, hi: 1.5, ..Default::default() };
        let l = Linkage::Rod { a: [0.0, 2.0, 0.2], b: [0.0, 1.0, 0.0], eff: 0.9, locking: false, drum: 0.0, pulls_only: false };
        let s0 = l.s(&j, 0.0);
        assert!((s0 - (1.0f64 + 0.04).sqrt()).abs() < 1e-9);
        // opening the ramp stretches the ram
        assert!(l.s(&j, 1.0) > s0);
        let r = l.ratio(&j, 0.5);
        let fd = (l.s(&j, 0.5001) - l.s(&j, 0.4999)) / 0.0002;
        assert!((r - fd).abs() < 1e-6);
    }
}
