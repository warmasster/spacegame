//! Push buttons: momentary (1 while pressed), latching (push on, push off), mushroom (push to stop,
//! turn to release) and pulse (one tick on each press). Pose: e0 = how far in (m), e1 = the
//! mushroom's turn (rad).
use super::{Mechanism, gate_block};
use crate::{
    def::ControlDef,
    intent::{ControlState, Event, F_PRESSED, F_PULSE, Gate, Intent, Outcome, Pose},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Momentary,
    Latching,
    Mushroom,
    Pulse,
}

pub struct Button {
    mode: Mode,
    labels: Vec<String>,
    travel: f32,
}

impl Button {
    pub fn new(d: &ControlDef) -> Result<Button, String> {
        let mode = match d.modo.as_deref() {
            None | Some("momentaneo") => {
                if d.pulso {
                    Mode::Pulse
                } else {
                    Mode::Momentary
                }
            }
            Some("enclavado") => Mode::Latching,
            Some("seta") => Mode::Mushroom,
            Some("pulso") => Mode::Pulse,
            Some(m) => return Err(format!("{}: modo de pulsador desconocido '{m}'", d.id)),
        };
        let travel = if mode == Mode::Mushroom { 0.008 } else { 0.003 };
        Ok(Button { mode, labels: d.posiciones.clone(), travel })
    }
}

impl Mechanism for Button {
    fn kind(&self) -> &'static str {
        "pulsador"
    }

    fn init(&self) -> ControlState {
        ControlState::default()
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        if let Some(b) = gate_block(g, false) {
            if matches!(i, Intent::Press { .. }) {
                return Outcome::blocked(b);
            }
            if !matches!(i, Intent::Release) {
                return Outcome::default();
            }
        }
        let before = self.value(st);
        match (*i, self.mode) {
            (Intent::Press { .. }, Mode::Momentary) => {
                st.set(F_PRESSED, true);
                st.x = 1.0;
            }
            (Intent::Press { .. }, Mode::Pulse) => {
                st.set(F_PRESSED, true);
                st.set(F_PULSE, true);
                st.x = 1.0;
            }
            (Intent::Press { .. }, Mode::Latching) => {
                st.set(F_PRESSED, true);
                st.x = if st.x >= 0.5 { 0.0 } else { 1.0 };
            }
            (Intent::Press { .. }, Mode::Mushroom) => {
                st.set(F_PRESSED, true);
                st.x = 1.0;
            }
            (Intent::Turn { notches, .. }, Mode::Mushroom) if notches.abs() > 0.0 && st.x >= 0.5 => {
                // twist to release
                st.x = 0.0;
                st.t = 0.35;
                return Outcome { changed: true, event: Some(Event::Spring) };
            }
            (Intent::Release, m) => {
                st.set(F_PRESSED, false);
                if m == Mode::Momentary {
                    st.x = 0.0;
                }
            }
            (Intent::Set { value }, _) => {
                st.x = if value >= 0.5 { 1.0 } else { 0.0 };
            }
            _ => return Outcome::default(),
        }
        Outcome::click(self.value(st) != before || matches!(i, Intent::Press { .. }))
    }

    fn advance(&self, st: &mut ControlState, dt: f32, _: f32) -> Outcome {
        // a pulse lasts one tick: the owner writes values before it advances its controls
        if self.mode == Mode::Pulse && st.x > 0.0 {
            st.x = 0.0;
            st.set(F_PULSE, false);
            return Outcome { changed: true, event: None };
        }
        if st.t > 0.0 {
            st.t = (st.t - f64::from(dt)).max(0.0);
        }
        Outcome::default()
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        let pressed = st.has(F_PRESSED) || (self.mode != Mode::Pulse && self.mode != Mode::Momentary && st.x >= 0.5);
        out.e[0] = if pressed { self.travel } else { 0.0 };
        out.e[1] = (st.t as f32) * 2.0;
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        let i = usize::from(st.x >= 0.5);
        if self.labels.is_empty() {
            out.push_str(if i == 1 { "PULSADO" } else { "SUELTO" });
        } else {
            super::pos_label(&self.labels, i.min(self.labels.len() - 1), out);
        }
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let x = if value >= 0.5 { 1.0 } else { 0.0 };
        let changed = st.x != x;
        st.x = x;
        changed
    }
}
