//! The keys of a seat (`SeatDef::mandos`): what each does to which control, and what a key held
//! down sends that control as time goes by. One place for the game (`app::aboard`) and for the
//! tests that work a seat as a pilot does, so that both press the same keys the same way.
//!
//! A key held on a control that turns (a throttle: `subir`, `bajar`) turns it at `HELD_RATE`
//! notches a second, however many frames that second is cut into: what it sends in a frame is
//! that rate times the frame's length (`held`). The control is what makes that a steady motion
//! (`lunar_controls`: a detent catches a lever coming into it, never one leaving it).
use crate::ship::Ship;
use lunar_controls::{Intent, Mods};

/// Notches a second a key held turns its control by.
pub const HELD_RATE: f32 = 25.0;

/// What a key does to its control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Does {
    /// While held, axis `axis` of a sprung control stands at `value` (the keys of one axis add).
    Axis {
        axis: u8,
        value: f64,
    },
    /// While held, the control turns up or down.
    Up,
    Down,
    /// Pressed, the control goes to its least.
    Zero,
    /// Pressed and let go, as a finger does.
    Press,
}

/// A key of a seat, as its data names it ("W", "Mayús"), the control it works (index among the
/// ship's controls) and what it does to it.
#[derive(Clone, Debug)]
pub struct SeatKey {
    pub key: String,
    pub control: usize,
    pub does: Does,
}

/// The keys of seat `seat` of `ship`, and what of its data says nothing that can be done (a
/// control that is not there, an action that is not one).
pub fn keys(ship: &Ship, seat: usize) -> (Vec<SeatKey>, Vec<String>) {
    let (mut out, mut bad) = (Vec::new(), Vec::new());
    let Some(s) = ship.kind.seats.get(seat) else { return (out, bad) };
    for b in &s.def.mandos {
        let Some(control) = ship.panels.controls.iter().position(|c| c.id == b.mando) else {
            bad.push(format!("asiento {}: tecla '{}': no hay mando '{}'", s.def.id, b.tecla, b.mando));
            continue;
        };
        let does = match (b.accion.as_deref(), b.eje) {
            (Some("subir"), _) => Does::Up,
            (Some("bajar"), _) => Does::Down,
            (Some("cero"), _) => Does::Zero,
            (Some("pulsar"), _) => Does::Press,
            (Some(a), _) => {
                bad.push(format!("asiento {}: tecla '{}': acción '{a}' (subir, bajar, cero o pulsar)", s.def.id, b.tecla));
                continue;
            }
            (None, axis) => Does::Axis { axis: axis.unwrap_or(0), value: b.valor.unwrap_or(1.0) },
        };
        out.push(SeatKey { key: b.tecla.clone(), control, does });
    }
    (out, bad)
}

/// What a key held turns its control by over a frame of `dt` s (`sign`: up 1, down −1). (A
/// steady turn: none of the speeding up a wheel spun fast by hand gets.)
pub fn held(sign: f32, dt: f32) -> Intent {
    Intent::Turn { notches: sign * HELD_RATE * dt, rate: 0.0, m: Mods::default() }
}
