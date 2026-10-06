//! The bezel of a multi-function display (`mfd`): a row of buttons along each side of its glass,
//! one control. Pressing button `k` shows the page with that legend (it writes the page signal);
//! buttons with no page do nothing. Electronic: dead without power.
//! Sub-elements (`Press { elem }`): left side top to bottom, right side, bottom left to right, top
//! (`mfd::slot`). Pose e0: button held (index + 1, 0 none).
use super::{Mechanism, gate_block, pos_label};
use crate::{
    def::ControlDef,
    intent::{ControlState, Event, F_PRESSED, Gate, Intent, Outcome, Pose},
};

pub struct Bezel {
    /// The legend of each page, in button order.
    pub labels: Vec<String>,
    pub per_side: u32,
}

impl Bezel {
    pub fn new(d: &ControlDef) -> Result<Bezel, String> {
        let per_side = d.botones.unwrap_or(crate::mfd::PER_SIDE).max(1);
        if d.posiciones.len() as u32 > per_side * 4 {
            return Err(format!("{}: {} páginas y solo {} botones", d.id, d.posiciones.len(), per_side * 4));
        }
        Ok(Bezel { labels: d.posiciones.clone(), per_side })
    }
}

impl Mechanism for Bezel {
    fn kind(&self) -> &'static str {
        "bisel"
    }

    fn init(&self) -> ControlState {
        ControlState::default()
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        match *i {
            Intent::Press { elem } => {
                if let Some(b) = gate_block(g, true) {
                    return Outcome::blocked(b);
                }
                st.set(F_PRESSED, true);
                st.v = f64::from(elem) + 1.0;
                if (elem as usize) < self.labels.len() {
                    let changed = st.x != f64::from(elem);
                    st.x = f64::from(elem);
                    return Outcome { changed, event: Some(Event::Click) };
                }
                Outcome { changed: false, event: Some(Event::Click) }
            }
            Intent::Release => {
                st.set(F_PRESSED, false);
                st.v = 0.0;
                Outcome::default()
            }
            Intent::Set { value } => Outcome::click(self.set(st, value)),
            _ => Outcome::default(),
        }
    }

    fn advance(&self, _: &mut ControlState, _: f32, _: f32) -> Outcome {
        Outcome::default()
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = st.v as f32;
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        out.push_str("página ");
        pos_label(&self.labels, st.x as usize, out);
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let x = value.round().clamp(0.0, (self.labels.len().max(1) - 1) as f64);
        let changed = x != st.x;
        st.x = x;
        changed
    }
}
