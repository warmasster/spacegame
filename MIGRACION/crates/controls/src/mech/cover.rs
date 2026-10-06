//! Guard covers (protect other controls, maybe sealed), key switches (need a key or a role) and
//! circuit breakers (pushed in to close, pulled to open, tripped by I²t, reset by hand once cool).
use super::{Mechanism, gate_block, pos_label};
use crate::{
    def::ControlDef,
    intent::{Blocked, ControlState, Event, F_PRESSED, F_SEAL, F_TRIPPED, Gate, Intent, Outcome, Pose},
};

/// How far a guard cover swings open (rad): back over the top of what it guards, near flat on
/// the panel above it, out of the hand's way (`layout` keeps that room free).
pub const COVER_OPEN: f32 = 2.9;
/// Height of a guard cover over its panel (mm, nominal): over the short lever of a guarded toggle.
pub const GUARD_H: f32 = 19.0;

/// How far above a cover's top edge (mm, at nominal size) it reaches open: its lid of length
/// `h` laid back at `COVER_OPEN`.
pub fn cover_reach(h: f32) -> f32 {
    h * (-COVER_OPEN.cos()) + 1.0
}

/// A guard cover. Value 1 open. Pose e0: hinge angle (rad), e1: 1 with the seal broken.
pub struct Cover {
    pub protects: Vec<String>,
    sealed: bool,
}

impl Cover {
    pub fn new(d: &ControlDef) -> Result<Cover, String> {
        if d.protege.is_empty() {
            return Err(format!("{}: una tapa protege algún mando ('protege')", d.id));
        }
        Ok(Cover { protects: d.protege.clone(), sealed: d.precinto })
    }
}

impl Mechanism for Cover {
    fn kind(&self) -> &'static str {
        "tapa"
    }

    fn init(&self) -> ControlState {
        ControlState::default()
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        match *i {
            Intent::Press { .. } => {
                if let Some(b) = gate_block(g, false) {
                    return Outcome::blocked(b);
                }
                st.x = if st.x >= 0.5 { 0.0 } else { 1.0 };
                if st.x >= 0.5 && self.sealed && !st.has(F_SEAL) {
                    st.set(F_SEAL, true);
                    return Outcome { changed: true, event: Some(Event::Seal) };
                }
                Outcome::click(true)
            }
            Intent::Set { value } => Outcome::click(self.set(st, value)),
            _ => Outcome::default(),
        }
    }

    fn advance(&self, st: &mut ControlState, dt: f32, _: f32) -> Outcome {
        // the lid swings (visual only): v follows x
        let target = st.x;
        st.v += (target - st.v) * (1.0 - (-f64::from(dt) * 14.0).exp());
        Outcome::default()
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = st.v as f32 * COVER_OPEN;
        out.e[1] = if st.has(F_SEAL) || !self.sealed { 1.0 } else { 0.0 };
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        out.push_str(if st.x >= 0.5 { "TAPA ABIERTA" } else { "TAPA CERRADA" });
        if self.sealed && !st.has(F_SEAL) {
            out.push_str(" (precintada)");
        }
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let x = if value >= 0.5 { 1.0 } else { 0.0 };
        let changed = x != st.x;
        st.x = x;
        changed
    }
}

/// A key switch: turning needs the key (or the role). Pose e0: angle.
pub struct KeySwitch {
    labels: Vec<String>,
    pub key: Option<String>,
}

impl KeySwitch {
    pub fn new(d: &ControlDef) -> Result<KeySwitch, String> {
        let labels = if d.posiciones.is_empty() { vec!["BLOQUEADO".into(), "LIBRE".into()] } else { d.posiciones.clone() };
        if !(2..=3).contains(&labels.len()) {
            return Err(format!("{}: una llave tiene 2 o 3 posiciones", d.id));
        }
        Ok(KeySwitch { labels, key: d.llave.clone() })
    }
}

impl Mechanism for KeySwitch {
    fn kind(&self) -> &'static str {
        "llave"
    }

    fn init(&self) -> ControlState {
        ControlState::default()
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        let dir = match *i {
            Intent::Turn { notches, .. } => {
                if notches > 0.0 {
                    1
                } else {
                    -1
                }
            }
            Intent::Press { .. } => 1,
            Intent::Set { value } => return Outcome::click(self.set(st, value)),
            _ => return Outcome::default(),
        };
        if let Some(b) = gate_block(g, false) {
            return Outcome::blocked(b);
        }
        if !g.has_key {
            return Outcome::blocked(Blocked::NoKey);
        }
        let n = self.labels.len() as i32;
        let mut to = st.x as i32 + dir;
        if matches!(i, Intent::Press { .. }) {
            to = to.rem_euclid(n);
        }
        if to < 0 || to >= n {
            return Outcome { changed: false, event: Some(Event::Stop) };
        }
        st.x = f64::from(to);
        Outcome::click(true)
    }

    fn advance(&self, _: &mut ControlState, _: f32, _: f32) -> Outcome {
        Outcome::default()
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = st.x as f32 * 0.8;
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        pos_label(&self.labels, st.x as usize, out);
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let x = value.round().clamp(0.0, (self.labels.len() - 1) as f64);
        let changed = x != st.x;
        st.x = x;
        changed
    }
}

/// A circuit breaker: value 1 closed. Trips when the current through it (the `sense` its owner
/// gives, as a share of the rating) heats it past its curve, at once on a short circuit. A
/// tripped breaker pops out and can be pushed back only once it cooled. Pose e0: how far out (m),
/// e1: 1 tripped (the white collar shows).
pub struct Breaker {
    /// Seconds to trip at twice the rating.
    tau: f64,
    /// Rated current (A), for the HUD.
    pub rating: f64,
}

impl Breaker {
    pub fn new(d: &ControlDef) -> Result<Breaker, String> {
        let tau = match d.curva_disparo.as_deref() {
            Some("rapida") | Some("rápida") => 0.6,
            None | Some("normal") => 2.0,
            Some("lenta") => 6.0,
            Some(c) => return Err(format!("{}: curva de disparo '{c}' (rapida, normal, lenta)", d.id)),
        };
        let rating = match &d.nominal {
            Some(q) => q.si_as("A").map_err(|e| e.0)?,
            None => 10.0,
        };
        Ok(Breaker { tau, rating })
    }
}

impl Mechanism for Breaker {
    fn kind(&self) -> &'static str {
        "disyuntor"
    }

    fn init(&self) -> ControlState {
        ControlState { x: 1.0, ..Default::default() }
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        match *i {
            Intent::Press { .. } => {
                if let Some(b) = gate_block(g, false) {
                    return Outcome::blocked(b);
                }
                st.set(F_PRESSED, true);
                if st.x >= 0.5 {
                    // pulled open by hand
                    st.x = 0.0;
                    return Outcome::click(true);
                }
                if st.has(F_TRIPPED) && st.t > 0.3 {
                    return Outcome::blocked(Blocked::Hot);
                }
                st.set(F_TRIPPED, false);
                st.x = 1.0;
                Outcome::click(true)
            }
            Intent::Release => {
                st.set(F_PRESSED, false);
                Outcome::default()
            }
            Intent::Set { value } => Outcome::click(self.set(st, value)),
            _ => Outcome::default(),
        }
    }

    fn advance(&self, st: &mut ControlState, dt: f32, sense: f32) -> Outcome {
        let dt = f64::from(dt);
        let r = f64::from(sense);
        // heat: grows past the rating as (I/In)² - 1, cools otherwise
        let over = r * r - 1.0;
        if st.x >= 0.5 && over > 0.0 {
            st.t += over * dt / (3.0 * self.tau);
        } else {
            st.t = (st.t - dt / (4.0 * self.tau)).max(0.0);
        }
        if st.x >= 0.5 && (st.t >= 1.0 || r > 8.0) {
            st.x = 0.0;
            st.set(F_TRIPPED, true);
            return Outcome { changed: true, event: Some(Event::Trip) };
        }
        Outcome::default()
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = if st.x >= 0.5 { 0.0 } else { 0.007 };
        out.e[1] = if st.has(F_TRIPPED) { 1.0 } else { 0.0 };
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        out.push_str(if st.has(F_TRIPPED) {
            "SALTADO"
        } else if st.x >= 0.5 {
            "CERRADO"
        } else {
            "ABIERTO"
        });
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let x = if value >= 0.5 { 1.0 } else { 0.0 };
        let changed = x != st.x;
        st.x = x;
        if x >= 0.5 {
            st.set(F_TRIPPED, false);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaker_trips_on_overload_and_resets_cool() {
        let d: ControlDef = serde_json::from_str(r#"{ "id": "brk", "kind": "disyuntor", "nominal": "40 A" }"#).unwrap();
        let b = Breaker::new(&d).unwrap();
        let mut st = b.init();
        // at the rating it holds forever
        for _ in 0..1000 {
            assert!(!b.advance(&mut st, 0.05, 1.0).changed);
        }
        // at twice the rating it trips in about its time constant
        let mut t = 0.0;
        while !b.advance(&mut st, 0.05, 2.0).changed {
            t += 0.05;
            assert!(t < 10.0);
        }
        assert!((1.0..3.0).contains(&t), "{t}");
        assert_eq!(b.value(&st), 0.0);
        // too hot to reset at once
        let g = Gate::default();
        assert_eq!(b.intent(&mut st, &Intent::Press { elem: 0 }, &g).event, Some(Event::Blocked(Blocked::Hot)));
        for _ in 0..200 {
            b.advance(&mut st, 0.05, 0.0);
        }
        assert!(b.intent(&mut st, &Intent::Press { elem: 0 }, &g).changed);
        assert_eq!(b.value(&st), 1.0);
        // a short circuit trips it at once
        assert!(b.advance(&mut st, 0.05, 20.0).changed);
    }
}
