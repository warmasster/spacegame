//! One step of a one-degree-of-freedom joint under the pushes of its actuators, gravity on what it
//! carries and its own damping, between hard stops. Force pushes add up; velocity pushes (a
//! hydraulic ram, a self-locking screw) drive it at their speed while they have the force, else
//! stop and hold it, else give way.
use super::{drive::Push, linkage::JointView};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Step {
    /// Generalized force the drives actually gave.
    pub applied: f64,
    /// The most they could have given (for load shares).
    pub max: f64,
    /// It hit a stop this step, and how fast (rad/s or m/s).
    pub hit: bool,
    pub impact: f64,
}

/// Advance `j` by `dt` with `pushes` (None: a lock holds it), viscous `damping` and an extra
/// external generalized force `ext` (contacts, someone standing on it).
pub fn integrate(j: &mut JointView, pushes: &[Option<Push>], damping: f64, ext: f64, dt: f64) -> Step {
    let mut st = Step::default();
    if pushes.iter().any(Option::is_none) {
        j.qd = 0.0;
        return st;
    }
    let inertia = j.inertia.max(1e-6);
    let mut force = 0.0;
    let (mut v_sum, mut w_sum, mut max, mut hold) = (0.0, 0.0, 0.0, 0.0);
    for p in pushes.iter().flatten() {
        match *p {
            Push::Force(f) => force += f,
            Push::Velocity { v, max: m, hold: h } => {
                v_sum += v * m.max(1e-9);
                w_sum += m.max(1e-9);
                max += m;
                hold += h;
            }
        }
    }
    let f_ext = force + j.gravity + ext - damping * j.qd;
    st.max = max + force.abs();
    if w_sum > 0.0 {
        let v = v_sum / w_sum;
        let need = inertia * (v - j.qd) / dt - f_ext;
        if need.abs() <= max {
            j.qd = v;
            st.applied = need + force;
        } else {
            let f = need.clamp(-max, max);
            let qd = j.qd + (f_ext + f) / inertia * dt;
            if v == 0.0 || qd * v.signum() < 0.0 {
                // it cannot drive it: it stops and holds, if it can
                let still = inertia * (0.0 - j.qd) / dt - f_ext;
                if still.abs() <= hold {
                    j.qd = 0.0;
                    st.applied = still + force;
                } else {
                    let g = still.clamp(-hold, hold);
                    j.qd += (f_ext + g) / inertia * dt;
                    st.applied = g + force;
                }
            } else {
                j.qd = qd;
                st.applied = f + force;
            }
        }
    } else {
        j.qd += f_ext / inertia * dt;
        st.applied = force;
    }
    j.q += j.qd * dt;
    if j.q < j.lo {
        j.q = j.lo;
        if j.qd < 0.0 {
            st.hit = true;
            st.impact = -j.qd;
            j.qd = 0.0;
        }
    } else if j.q > j.hi {
        j.q = j.hi;
        if j.qd > 0.0 {
            st.hit = true;
            st.impact = j.qd;
            j.qd = 0.0;
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actuator::{
            Actuator, ActuatorDef,
            linkage::JointView,
        },
        machine::{Cx, Env},
        net::{Medium, PortIo},
    };
    use lunar_signals::{Store, Writer};

    fn ramp() -> JointView {
        // a 900 kg ramp, 3 m long, hinged at its top: gravity pulls it open (down)
        JointView { q: 0.0, qd: 0.0, lo: 0.0, hi: 1.75, hinge: true, axis: [1.0, 0.0, 0.0], pivot: [0.0, 0.0, 0.0], inertia: 900.0 * 3.0, gravity: 0.0 }
    }

    fn gravity(q: f64) -> f64 {
        // the ramp's weight (900 kg at 1.4 m) opens it while upright... it falls toward horizontal
        900.0 * 1.62 * 1.4 * q.cos() * -1.0 * -1.0
    }

    #[test]
    fn hydraulic_rams_open_hold_and_drop_when_cut() {
        let mut store = Store::new();
        let def: ActuatorDef = serde_json::from_str(
            r#"{ "articulacion": "rampa",
                 "accionamiento": { "tipo": "hidraulico", "diametro": "70 mm", "caudal_valvula": "30 L/min" },
                 "transmision": { "tipo": "biela", "anclaje_fijo": [0.0, -0.3, -1.5], "anclaje_movil": [0.0, -1.2, 0.0] },
                 "bloqueos": [ { "en": "inicio", "liberacion": "hidraulico" } ] }"#,
        )
        .unwrap();
        let mut a = Actuator::new("rampa_izq", &def, &mut store, Writer::Actuator(0), 1).unwrap();
        let mut j = ramp();
        a.bind(0, &j);
        let order = store.find("rampa_izq.orden").unwrap();
        store.set(order, 1.0);
        // ports: fluid (21 MPa, plenty of flow), valve power
        let mut io = vec![PortIo::on(0, 0), PortIo::on(1, 0)];
        let mut opened = false;
        for k in 0..2000 {
            for p in &mut io {
                p.clear_ask();
            }
            let mut cx = Cx { signals: &mut store, env: Env::default(), health: 1.0, working: true, t: k as f64 * 0.02, dt: 0.02 };
            a.plan(&mut cx, &j, &mut io);
            io[0].fed = true;
            io[0].level = 21e6;
            io[0].got = io[0].demand;
            io[0].share = 1.0;
            io[1].fed = true;
            io[1].share = 1.0;
            j.gravity = gravity(j.q);
            let p = a.push(&mut cx, &j, &io, 0.02);
            let s = integrate(&mut j, &[p], 50.0, 0.0, 0.02);
            a.after(&mut cx, &j, s.applied, s.max);

            if j.q >= j.hi - 0.01 {
                opened = true;
                break;
            }
        }
        assert!(opened, "the ram opens the ramp ({})", j.q);
        // the line is cut: nothing holds it, it sinks (down: toward hi here, so command closed)
        store.set(order, 0.0);
        let q0 = j.q;
        for k in 0..200 {
            let mut cx = Cx { signals: &mut store, env: Env::default(), health: 1.0, working: true, t: k as f64 * 0.02, dt: 0.02 };
            for p in &mut io {
                p.clear_ask();
            }
            a.plan(&mut cx, &j, &mut io);
            io[0].fed = false;
            io[0].level = 0.0;
            j.gravity = 2000.0;
            let p = a.push(&mut cx, &j, &io, 0.02);
            integrate(&mut j, &[p], 50.0, 0.0, 0.02);
        }
        assert!(j.q >= q0 - 1e-9, "with the line cut it does not close");
        let _ = Medium::Hidraulico;
    }

    #[test]
    fn a_self_locking_screw_holds_without_power() {
        let mut j = JointView { q: 0.5, qd: 0.0, lo: 0.0, hi: 1.0, hinge: true, axis: [1.0, 0.0, 0.0], inertia: 50.0, gravity: -3000.0, ..Default::default() };
        let p = Push::Velocity { v: 0.0, max: 0.0, hold: 1e5 };
        for _ in 0..100 {
            integrate(&mut j, &[Some(p)], 0.0, 0.0, 0.02);
        }
        assert!((j.q - 0.5).abs() < 1e-9);
        // a free joint falls to its stop
        for _ in 0..200 {
            integrate(&mut j, &[Some(Push::Force(0.0))], 0.0, 0.0, 0.02);
        }
        assert_eq!(j.q, 0.0);
    }
}
