//! Mechanisms: each control kind is a continuous state machine behind one contract. A kind is
//! registered by name in `build`; a new kind is one file here and one line there.
//!
//! A mechanism holds its resolved parameters (SI); its state is a `ControlState` kept by the owner,
//! so a thousand switches are a thousand small records and one object per kind of parameters.
mod button;
mod bezel;
mod cover;
mod keypad;
mod lever;
mod switch;
mod wheel;

use crate::{
    def::ControlDef,
    intent::{ControlState, Gate, Intent, Outcome, Pose},
};
use lunar_signals::Q;

pub use bezel::Bezel;
pub use button::Button;
pub use cover::{Breaker, COVER_OPEN, Cover, GUARD_H, KeySwitch, cover_reach};
pub use keypad::Keypad;
pub use lever::Lever;
pub use switch::{Selector, Switch};
pub use wheel::Wheel;

pub trait Mechanism: Send + Sync {
    /// Kind name, as in the data.
    fn kind(&self) -> &'static str;
    fn init(&self) -> ControlState;
    /// An intent, given what the world allows. Changes the state, says what happened.
    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome;
    /// Time passes: inertia, springs, timers. `sense` is what the owner measures for it (the
    /// current through a breaker as a share of its rating; 0 for the rest). Says whether the value
    /// changed and what happened (a breaker trips).
    fn advance(&self, st: &mut ControlState, dt: f32, sense: f32) -> Outcome;
    /// The SI value it writes.
    fn value(&self, st: &ControlState) -> f64;
    /// A second value (the other axis of a stick), written to `bind.senal_y`.
    fn value2(&self, _st: &ControlState) -> Option<f64> {
        None
    }
    /// Text on its own display (keypads).
    fn text(&self, _st: &ControlState, _out: &mut String) {}
    /// How its model stands.
    fn pose(&self, st: &ControlState, out: &mut Pose);
    /// Can a hand get to it (not counting covers, which the owner knows).
    fn reachable(&self, _st: &ControlState) -> bool {
        true
    }
    /// "Empuje 62,4 % (fino)": its reading for the HUD.
    fn describe(&self, st: &ControlState, out: &mut String);
    /// Set from a value (bindings, network): the nearest state that writes it.
    fn set(&self, st: &mut ControlState, value: f64) -> bool;
}

pub(crate) fn q(v: &Option<Q>, unit: &str, or: f64) -> Result<f64, String> {
    match v {
        Some(x) => x.si_as(unit).map_err(|e| e.0),
        None => Ok(or),
    }
}

/// Label of position `i` (or its number).
pub(crate) fn pos_label(labels: &[String], i: usize, out: &mut String) {
    match labels.get(i) {
        Some(l) => out.push_str(l),
        None => {
            use std::fmt::Write;
            let _ = write!(out, "{i}");
        }
    }
}

/// The mechanism of a definition, or None when the kind is an indicator.
pub fn build(d: &ControlDef) -> Result<Option<Box<dyn Mechanism>>, String> {
    let m: Box<dyn Mechanism> = match d.kind.as_str() {
        "pulsador" => Box::new(Button::new(d)?),
        "interruptor" => Box::new(Switch::new(d)?),
        "selector" => Box::new(Selector::new(d)?),
        "rueda" | "volante" => Box::new(Wheel::new(d)?),
        "palanca" => Box::new(Lever::new(d)?),
        "tapa" => Box::new(Cover::new(d)?),
        "llave" => Box::new(KeySwitch::new(d)?),
        "teclado" => Box::new(Keypad::new(d)?),
        "disyuntor" => Box::new(Breaker::new(d)?),
        "bisel" => Box::new(Bezel::new(d)?),
        _ => return Ok(None),
    };
    Ok(Some(m))
}

/// Every mechanism kind (for validation messages).
pub const KINDS: [&str; 11] = ["pulsador", "interruptor", "selector", "rueda", "volante", "palanca", "tapa", "llave", "teclado", "disyuntor", "bisel"];

/// Check the gate common to all: what blocks any change.
pub(crate) fn gate_block(g: &Gate, needs_power: bool) -> Option<crate::intent::Blocked> {
    use crate::intent::Blocked;
    if !g.working {
        return Some(Blocked::Broken);
    }
    if !g.uncovered {
        return Some(Blocked::Covered);
    }
    if needs_power && g.supply < 0.5 {
        return Some(Blocked::NoPower);
    }
    // vetoes are the owner's: it tries the change and undoes it (`Panel`)
    None
}
