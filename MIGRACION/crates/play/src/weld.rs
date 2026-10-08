//! A welder at work, as a step of the game: the part looked at within its reach is mended at the
//! welder's rate while the trigger is held, and a part that is gone, looked at where it would be,
//! is put back after as long as the welder takes. The same in the server, which decides it, and in
//! each player's game, which predicts its own: what the hands hold and the trigger go in the
//! command (`Cmd::tool`, `Cmd::trigger`), where it is aimed is the body's look. How it is drawn and
//! what its screen reads is the window's (`app/src/gear.rs`).
use crate::{game::Player, gear::Gear, ships::Ships};
use glam::DVec3;
use lunar_core::{
    structure::{schedule::LINGER, set::Structures},
    view::View,
};

/// Of its hit points, what a part put back has.
pub const REBUILT: f32 = 0.2;

/// What a welder aimed along `view` works on, within `reach` m: the part looked at, or a part that
/// is gone where it would be, if that is nearer (the one looked at, as the eye sees them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub structure: u64,
    pub part: u32,
    pub gone: bool,
    /// Where it is looked at (world).
    pub at: DVec3,
}

/// What `view` meets within `reach` that a welder works on.
pub fn target(view: &View, reach: f64, set: &Structures) -> Option<Target> {
    let mut best = set.raycast(view.eye, view.forward, reach).map(|(k, h, p)| (k, h.part, f64::from(h.t), p, false));
    for (k, s) in set.list.iter().enumerate() {
        if s.to_world(s.center).distance(view.eye) > f64::from(s.radius) + reach {
            continue;
        }
        let limit = best.map_or(reach, |b| b.2) as f32;
        if let Some(h) = s.raycast_gone(s.to_local(view.eye), s.dir_to_local(view.forward), limit) {
            best = Some((k, h.part, f64::from(h.t), view.eye + view.forward * f64::from(h.t), true));
        }
    }
    best.map(|(k, part, _, at, gone)| Target { structure: set.list[k].id, part, gone, at })
}

/// What a player's welder did at a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Done {
    /// Mended so many hit points of it.
    Mended(Target, f32),
    /// Putting it back, this share of the way.
    PuttingBack(Target, f32),
    /// Put back.
    PutBack(Target),
}

/// `dt` s of player `p`'s welder, if what they hold is one and the trigger is held: what it did.
pub fn step(p: &mut Player, gear: &Gear, set: &mut Structures, ships: &mut Ships, dt: f64) -> Option<Done> {
    let Some((reach, rate, takes)) = gear.welder(p.tool).filter(|_| p.trigger && !p.away) else {
        p.putting_back = None;
        return None;
    };
    let view = p.acting_view(set);
    let Some(t) = target(&view, reach, set) else {
        p.putting_back = None;
        return None;
    };
    let now = set.now;
    let lib = set.lib.clone();
    let k = set.index_of(t.structure)?;
    let s = &mut set.list[k];
    s.awake_until = s.awake_until.max(now + LINGER);
    let done = if t.gone {
        // (putting it back takes a moment: the same part looked at all along)
        let share = match p.putting_back {
            Some((id, part, share)) if (id, part) == (t.structure, t.part) => share,
            _ => 0.0,
        } + dt as f32 / takes.max(0.05);
        if share >= 1.0 {
            p.putting_back = None;
            s.rebuild(&lib.catalog, t.part as usize, REBUILT).then_some(Done::PutBack(t))?
        } else {
            p.putting_back = Some((t.structure, t.part, share));
            Done::PuttingBack(t, share)
        }
    } else {
        p.putting_back = None;
        let pt = s.parts.get(t.part as usize)?;
        let hp = (pt.max_hp * rate * dt as f32).min(pt.max_hp - pt.hp);
        if hp <= 0.0 {
            return None;
        }
        s.mend(&lib.catalog, t.part as usize, hp)?;
        Done::Mended(t, hp)
    };
    if let Some(n) = ships.by_structure(t.structure) {
        ships.list[n].touch();
    }
    Some(done)
}
