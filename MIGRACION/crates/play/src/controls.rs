//! What a hand does to a ship, whoever's hand it is and wherever it was decided: a control set
//! where it was left, a door or a clamp worked. The hand's own game decides it (where the
//! crosshair is, the mouse, the keys of the seat) and tells it; every game that has the ship does
//! it here, the same (`lunar_play`'s server included, once it has checked the hand could).
use crate::ships::Ships;
use lunar_controls::Intent;
use lunar_core::structure::set::Structures;

/// A hand on something of a ship that is not a control of its panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Closure `0` (a door, a hatch, a lid) pushed open (`1`) or shut.
    Closure(u16, bool),
    /// Clamp `0` worked: it held something and lets it go (`1`), or it shuts on what is loose on it.
    Clamp(u16, bool),
}

/// The signal a hand orders closure `c` of `sh` by.
pub fn closure_order(sh: &lunar_ship::Ship, c: usize) -> Option<String> {
    let plan = sh.kind.closures.get(c)?;
    Some(plan.order.clone().unwrap_or_else(|| format!("{}.mano", plan.id)))
}

/// Control `k` of the ship on structure `structure` put at `value`, as a hand left it. Nothing of
/// it is heard or noted here. False if there is no such ship or control, or it did not move.
pub fn set(ships: &mut Ships, set: &Structures, structure: u64, k: usize, value: f64) -> bool {
    let (Some(n), Some(s)) = (ships.by_structure(structure), set.get(structure)) else { return false };
    let sh = &mut ships.list[n];
    if k >= sh.panels.controls.len() {
        return false;
    }
    let kind = sh.kind.clone();
    sh.panels.intent(k, &Intent::Set { value }, s, &kind, &sh.store).changed
}

/// What a hand did to the ship on structure `structure` that is no control (`Act`).
pub fn act(ships: &mut Ships, structure: u64, act: Act) {
    let Some(n) = ships.by_structure(structure) else { return };
    let sh = &mut ships.list[n];
    match act {
        Act::Closure(c, open) => {
            if let Some(order) = closure_order(sh, usize::from(c)) {
                sh.set_signal(&order, f64::from(u8::from(open)));
                sh.touch();
            }
        }
        Act::Clamp(c, holding) if usize::from(c) < sh.kind.clamps.len() => sh.work_clamp(usize::from(c), holding),
        Act::Clamp(..) => {}
    }
}
