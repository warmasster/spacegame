//! A limb of two bones reaching for a point: where its middle joint (an elbow, a knee) must be
//! for its end (a wrist, an ankle) to be at the target, bending in the plane a pole chooses (the
//! way the elbow or the knee points). Solved by the law of cosines, exactly: no iterations.
use super::skeleton::{Pose, Skeleton, frame_turn};
use glam::{Quat, Vec3};

/// A limb: the bone at its root (an upper arm, a thigh), the one after its middle joint (a
/// forearm, a shin) and the one at its end (a hand, a foot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limb {
    pub upper: usize,
    pub lower: usize,
    pub end: usize,
}

/// A limb never goes quite straight: it stops this short of its full length (a knee locked
/// straight snaps as it bends again).
const STRAIGHT: f32 = 0.999;

impl Limb {
    /// Its two lengths at rest.
    pub fn lengths(&self, sk: &Skeleton) -> (f32, f32) {
        ((sk.bind[self.lower].pos - sk.bind[self.upper].pos).length(), (sk.bind[self.end].pos - sk.bind[self.lower].pos).length())
    }

    /// Its full reach.
    pub fn reach(&self, sk: &Skeleton) -> f32 {
        let (a, b) = self.lengths(sk);
        (a + b) * STRAIGHT
    }

    /// Bends it so that its end is at `target` (model space), its middle joint toward `pole` (a
    /// direction in model space, not a point), and — if given — its end turned to `end` (model
    /// space). The pose's model places must be up to date for the limb's root (`Pose::update`);
    /// they are for the limb after. How far short of the target the end is left (0 if it
    /// reaches).
    pub fn solve(&self, sk: &Skeleton, pose: &mut Pose, target: Vec3, pole: Vec3, end: Option<Quat>) -> f32 {
        let a = pose.model[self.upper].pos;
        let b = pose.model[self.lower].pos;
        let c = pose.model[self.end].pos;
        let (l1, l2) = ((b - a).length(), (c - b).length());
        let to = target - a;
        let far = to.length();
        // as far as it reaches, and no nearer than it folds
        let d = far.clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * STRAIGHT);
        let dir = if far > 1e-6 { to / far } else { (c - a).normalize_or(Vec3::NEG_Y) };
        // the side its middle goes to
        let side = (pole - dir * pole.dot(dir)).normalize_or(dir.any_orthonormal_vector());
        // the triangle: its middle is `along` the line to the target and `off` it to the side
        let along = (l1 * l1 + d * d - l2 * l2) / (2.0 * d);
        let off = (l1 * l1 - along * along).max(0.0).sqrt();
        let mid = a + dir * along + side * off;
        let tip = a + dir * d;
        // the plane it bends in, as it is and as it will be (by its normal)
        let normal = side.cross(dir);
        let was = (b - a).cross(c - b);
        let was = if was.length_squared() > 1e-10 { was } else { normal };
        pose.turn(sk, self.upper, frame_turn(b - a, was, mid - a, normal));
        let (b, c) = (pose.model[self.lower].pos, pose.model[self.end].pos);
        pose.turn(sk, self.lower, frame_turn(c - b, normal, tip - b, normal));
        if let Some(rot) = end {
            pose.aim(sk, self.end, rot);
        }
        (far - d).max(0.0)
    }
}

/// The turn that brings `from` to point at `to` and no more than that (the short way).
pub fn swing(from: Vec3, to: Vec3) -> Quat {
    let (f, t) = (from.normalize_or_zero(), to.normalize_or_zero());
    if f == Vec3::ZERO || t == Vec3::ZERO { Quat::IDENTITY } else { Quat::from_rotation_arc(f, t) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::skeleton::tests::leg;

    fn limb(sk: &Skeleton) -> Limb {
        Limb { upper: sk.bone("hip").unwrap(), lower: sk.bone("knee").unwrap(), end: sk.bone("ankle").unwrap() }
    }

    #[test]
    fn the_foot_goes_where_it_is_sent_and_the_knee_where_the_pole_says() {
        let sk = leg();
        let l = limb(&sk);
        let (l1, l2) = l.lengths(&sk);
        for (target, pole) in [(Vec3::new(0.0, 0.3, 0.2), Vec3::Z), (Vec3::new(0.2, 0.25, -0.3), Vec3::Z), (Vec3::new(-0.1, 0.5, 0.5), Vec3::new(0.3, 0.0, 1.0)), (Vec3::new(0.0, 0.12, 0.0), Vec3::Z), (Vec3::new(0.0, 0.7, 0.05), Vec3::Z)] {
            let mut pose = Pose::rest(&sk);
            let short = l.solve(&sk, &mut pose, target, pole, None);
            let (hip, knee, ankle) = (pose.model[l.upper].pos, pose.model[l.lower].pos, pose.model[l.end].pos);
            assert!(short < 1e-4 && (ankle - target).length() < 2e-3, "to {target:?}: ankle at {ankle:?}");
            // nothing stretched
            assert!(((knee - hip).length() - l1).abs() < 1e-4 && ((ankle - knee).length() - l2).abs() < 1e-4);
            // the knee on the pole's side of the line from hip to ankle
            let line = (ankle - hip).normalize();
            let out = (knee - hip) - line * (knee - hip).dot(line);
            let side = pole - line * pole.dot(line);
            assert!(out.length() < 1e-3 || out.normalize().dot(side.normalize()) > 0.99, "knee off the pole: {out:?} vs {side:?}");
        }
    }

    #[test]
    fn out_of_reach_it_stretches_toward_it_and_says_how_short_it_is() {
        let sk = leg();
        let l = limb(&sk);
        let mut pose = Pose::rest(&sk);
        let target = Vec3::new(0.0, 1.0, 2.0);
        let short = l.solve(&sk, &mut pose, target, Vec3::Y, None);
        let ankle = pose.model[l.end].pos;
        assert!((short - (2.0 - l.reach(&sk))).abs() < 1e-3, "short by {short}");
        assert!(ankle.z > 0.89 && (ankle.y - 1.0).abs() < 0.01 && ankle.x.abs() < 1e-3, "{ankle:?}");
    }

    #[test]
    fn the_end_is_turned_as_asked_and_what_hangs_from_it_follows() {
        let sk = leg();
        let l = limb(&sk);
        let toe = sk.bone("toe").unwrap();
        let mut pose = Pose::rest(&sk);
        // the foot flat, pointing along +x
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        l.solve(&sk, &mut pose, Vec3::new(0.1, 0.2, 0.1), Vec3::Z, Some(rot));
        let d = pose.model[toe].pos - pose.model[l.end].pos;
        assert!((d - rot * sk.rest[toe].pos).length() < 1e-4, "{d:?}");
    }

    #[test]
    fn solving_again_from_where_it_is_changes_nothing() {
        // (no drift frame after frame: the same target gives the same pose)
        let sk = leg();
        let l = limb(&sk);
        let mut pose = Pose::rest(&sk);
        let target = Vec3::new(0.15, 0.3, 0.25);
        l.solve(&sk, &mut pose, target, Vec3::Z, None);
        let first = pose.clone();
        for _ in 0..50 {
            l.solve(&sk, &mut pose, target, Vec3::Z, None);
        }
        for k in 0..sk.len() {
            assert!((pose.model[k].pos - first.model[k].pos).length() < 1e-4 && pose.model[k].rot.dot(first.model[k].rot).abs() > 0.9999);
        }
    }
}
