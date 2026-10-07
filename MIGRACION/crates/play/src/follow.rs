//! How a copy of a thing simulated in another player's game follows it, so that it looks right
//! however fast both go:
//!
//! - **steered, not put** (`steer`): our copy runs on as this game simulates it (its controls
//!   are told too) and is brought to where it is told over a fraction of a second, before the
//!   world steps: no jump from one frame to the next, and whoever rides it goes with it. Only a
//!   copy far off (a ship put somewhere at a stroke) is put there at once;
//! - **a player beside what flies** (`pick`): one floating by a ship is told in that ship's
//!   frame, so that every game draws them by its copy of it, not where the world had them a
//!   tenth of a second ago (780 m back at orbital speed).
//!
//! What no frame can take away: two games tell where things are at the same moment only as
//! well as their clocks agree, and at 7.8 km/s every millisecond is 8 m. The clocks are kept
//! together to a millisecond or two and slide slowly when corrected (`lunar_net::clock`).
use glam::{DVec3, Quat, Vec3};

/// A ship nearer than this may be what a player floats by (m), and one already chosen is kept
/// until another is this much nearer.
pub const REACH: f64 = 5000.0;
const KEEP: f64 = 0.75;
/// A copy is brought to where it is told with this half-life (s); further than this (m, or
/// turned more than this, rad) it is put there at once.
pub const HALF_LIFE: f64 = 0.08;
const SNAP: f64 = 40.0;
const SNAP_TURN: f32 = 0.6;

/// Where a thing is and how it moves, in the world.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Kin {
    pub pos: DVec3,
    pub vel: DVec3,
    pub rot: Quat,
    pub spin: Vec3,
}

/// The ship something at `at` is to be told beside: the nearest within `REACH`, keeping the one
/// it was told beside while none is clearly nearer. `others`: each ship's number and where it is.
pub fn pick(at: DVec3, was: Option<u64>, others: impl Iterator<Item = (u64, DVec3)>) -> Option<u64> {
    let mut best: Option<(u64, f64)> = None;
    let mut kept: Option<f64> = None;
    for (k, p) in others {
        let d = p.distance(at);
        if Some(k) == was {
            kept = Some(d);
        }
        if d < REACH && best.is_none_or(|b| d < b.1) {
            best = Some((k, d));
        }
    }
    match (was, kept, best) {
        // (the one chosen, still near enough and none clearly nearer: kept)
        (Some(w), Some(d), best) if d < REACH / KEEP && best.is_none_or(|(_, b)| b > d * KEEP) => Some(w),
        (_, _, best) => best.map(|b| b.0),
    }
}

/// The copy `cur` brought toward `want` over `dt` s: a share of what is between them each
/// frame (the same share at any frame rate), its speed and spin brought likewise. True if it
/// was so far off that it was put there instead.
pub fn steer(cur: &mut Kin, want: &Kin, dt: f64) -> bool {
    let turn = cur.rot.angle_between(want.rot);
    if cur.pos.distance(want.pos) > SNAP || turn > SNAP_TURN {
        *cur = *want;
        return true;
    }
    let k = 1.0 - (-dt * std::f64::consts::LN_2 / HALF_LIFE).exp();
    cur.pos += (want.pos - cur.pos) * k;
    cur.vel += (want.vel - cur.vel) * k;
    cur.rot = cur.rot.slerp(want.rot, k as f32).normalize();
    cur.spin += (want.spin - cur.spin) * k as f32;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_ship_is_picked_and_kept_until_another_is_clearly_nearer() {
        let at = DVec3::new(1e6, 0.0, 0.0);
        let ships = |a: f64, b: f64| [(1u64, at + DVec3::X * a), (2u64, at + DVec3::Y * b)].into_iter();
        assert_eq!(pick(at, None, ships(100.0, 300.0)), Some(1));
        // the other a little nearer: the one chosen is kept
        assert_eq!(pick(at, Some(1), ships(100.0, 90.0)), Some(1));
        // clearly nearer: it changes
        assert_eq!(pick(at, Some(1), ships(100.0, 60.0)), Some(2));
        // none in reach: the world
        assert_eq!(pick(at, Some(1), ships(9000.0, 7000.0)), None);
        // the one chosen just past reach, none nearer: kept a little longer
        assert_eq!(pick(at, Some(1), ships(REACH * 1.1, 9e9)), Some(1));
    }

    #[test]
    fn a_copy_is_brought_where_it_is_told_the_same_at_any_frame_rate_and_without_jumps() {
        let want = Kin { pos: DVec3::new(2.0, 0.0, 0.0), vel: DVec3::new(0.0, 1.0, 0.0), rot: Quat::from_rotation_y(0.2), spin: Vec3::Y * 0.1 };
        let mut ends = Vec::new();
        for fps in [10.0f64, 30.0, 60.0, 144.0, 240.0] {
            let mut cur = Kin { rot: Quat::IDENTITY, ..Kin::default() };
            let dt = 1.0 / fps;
            let mut most = 0.0f64;
            for _ in 0..(0.5 * fps).round() as usize {
                let was = cur.pos;
                assert!(!steer(&mut cur, &want, dt));
                most = most.max(cur.pos.distance(was));
            }
            // a 2 m error is taken out over a few frames, none of them a jump
            assert!(most < 2.0 * 0.6, "{fps} fps: a step of {most:.3} m");
            ends.push(cur.pos.x);
        }
        // after half a second, nearly there, the same whatever the frame rate
        for e in &ends {
            assert!((e - ends[0]).abs() < 1e-6 && *e > 2.0 * 0.95, "{ends:?}");
        }
        // a copy put far away: there at once
        let mut cur = Kin { rot: Quat::IDENTITY, ..Kin::default() };
        assert!(steer(&mut cur, &Kin { pos: DVec3::X * 500.0, ..want }, 1.0 / 60.0) && cur.pos.x == 500.0);
    }
}
