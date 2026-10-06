//! What something weighs and which way it falls: one rule for whatever is not a structure of its
//! own — someone on foot, a crew member, anything set down in a hold — from where it is now and
//! what carries it now, and nothing else.
//!
//! - Carried by nothing, it is pulled as the world pulls there (`BodyRegistry::field`): past
//!   every body's reach, not at all.
//! - In the rooms of a structure that makes gravity of its own (`Structure::gravity`,
//!   `Structure::rooms`), it weighs that, toward that structure's decks, however the structure
//!   flies and wherever it is.
//! - Carried by a structure otherwise (on one that makes none, or outside its rooms), it weighs
//!   what is left of the world's pull once what carries it has gone its own way: all of it on
//!   something standing on the ground, none on something that falls or orbits, the push of its
//!   engines the other way on one under thrust. The same goes for what it weighs on anything
//!   it touches: it is pressed against that only as far as that does not fall away under it.
//!
//! Nothing here asks which body, how fast, or what happened before.
use super::state::Structure;
use glam::{DVec3, Vec3};

/// How something at `p` (world) speeds up for what weighs on it (world, m/s²). `pull`: what
/// pulls at `p` (`BodyRegistry::field`). `by`: the structure it is aboard (it stands on it, or
/// is in its rooms), if any; `carried`: its speeds are counted in that structure (it goes with
/// it), not in the world; `inside`: it is in that structure's rooms — said by whoever knows
/// where the two are at one and the same instant (what is not carried is where it is in the
/// structure only once both have moved: at orbital speed, a slice apart is tens of metres).
pub fn felt(pull: DVec3, p: DVec3, by: Option<&Structure>, carried: bool, inside: bool) -> DVec3 {
    let Some(s) = by else {
        return pull;
    };
    let left = if carried { pull - s.acc_at(p) } else { pull };
    let own = if s.gravity.g > 0.0 && inside { f64::from(s.gravity.on.clamp(0.0, 1.0)) } else { 0.0 };
    if own <= 0.0 {
        return left;
    }
    // (its own gravity coming on or going: one gives way to the other, never at a stroke)
    let deck = (s.rot * Vec3::NEG_Y).as_dvec3() * f64::from(s.gravity.g);
    left + (deck - left) * own
}
