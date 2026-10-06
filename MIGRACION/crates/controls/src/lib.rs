//! Controls: the mechanisms a hand works (buttons, switches, selectors, continuous wheels, levers,
//! covers, keys, keypads, breakers), the indicators that show signals (lamps, needles, bars,
//! displays, counters, screens, annunciators) and the panels that hold them, laid out from data.
//!
//! This crate knows no ship and no pixel: it takes `Intent`s and gives values (SI), poses for the
//! models and events for sound and the HUD. The owner (a structure's systems) resolves signals,
//! power and covers, and draws.
pub mod def;
pub mod indicator;
pub mod intent;
pub mod layout;
pub mod mech;
pub mod mfd;

pub use def::ControlDef;
pub use indicator::{IndState, Indicator};
pub use intent::{Blocked, ControlState, Event, Gate, Intent, Mods, Outcome, Pose, Spin};
pub use layout::{PanelDef, PanelLayout, layout};
pub use mech::Mechanism;
