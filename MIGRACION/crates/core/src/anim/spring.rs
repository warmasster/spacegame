//! Springs that follow what they are after and settle without swinging past it (critically
//! damped), stepped exactly whatever the frame's length: what a hand does with the weight of a
//! tool, what the knees do with a landing. Told by their half-life: the time in which half of
//! what is left to go is gone (D. Holden, "Spring-It-On", 2021).
use glam::{Quat, Vec3};

/// The damping (1/s) of a spring that halves what is left in `halflife` seconds.
fn damping(halflife: f32) -> f32 {
    4.0 * std::f32::consts::LN_2 / halflife.max(1e-5)
}

/// A value after its goal: where it is and how fast it goes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub x: f32,
    pub v: f32,
}

impl Spring {
    pub fn at(x: f32) -> Spring {
        Spring { x, v: 0.0 }
    }

    /// `dt` seconds toward `goal`.
    pub fn step(&mut self, goal: f32, halflife: f32, dt: f32) -> f32 {
        let y = damping(halflife) / 2.0;
        let j0 = self.x - goal;
        let j1 = self.v + j0 * y;
        let e = (-y * dt).exp();
        self.x = e * (j0 + j1 * dt) + goal;
        self.v = e * (self.v - j1 * y * dt);
        self.x
    }

    /// A kick: its speed changed at once (a recoil, a blow).
    pub fn kick(&mut self, dv: f32) {
        self.v += dv;
    }
}

/// The same for a point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring3 {
    pub x: Vec3,
    pub v: Vec3,
}

impl Spring3 {
    pub fn at(x: Vec3) -> Spring3 {
        Spring3 { x, v: Vec3::ZERO }
    }

    pub fn step(&mut self, goal: Vec3, halflife: f32, dt: f32) -> Vec3 {
        let y = damping(halflife) / 2.0;
        let j0 = self.x - goal;
        let j1 = self.v + j0 * y;
        let e = (-y * dt).exp();
        self.x = e * (j0 + j1 * dt) + goal;
        self.v = e * (self.v - j1 * y * dt);
        self.x
    }

    pub fn kick(&mut self, dv: Vec3) {
        self.v += dv;
    }
}

/// A point on a spring that swings past its goal and settles: what something with weight does
/// when what carries it moves or stops. Told by how many times a second it swings (`hz`) and how
/// soon the swing dies (`ratio`: 1 it never goes past, 0.5 once and back, 0.3 lively), stepped
/// exactly whatever the frame's length.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway3 {
    pub x: Vec3,
    pub v: Vec3,
}

impl Sway3 {
    pub fn at(x: Vec3) -> Sway3 {
        Sway3 { x, v: Vec3::ZERO }
    }

    pub fn step(&mut self, goal: Vec3, hz: f32, ratio: f32, dt: f32) -> Vec3 {
        let w = std::f32::consts::TAU * hz.max(1e-3);
        let z = ratio.clamp(0.05, 1.0);
        let j0 = self.x - goal;
        let e = (-z * w * dt).exp();
        if z > 0.999 {
            // (no swing: the same as `Spring3`)
            let j1 = self.v + j0 * w;
            self.x = e * (j0 + j1 * dt) + goal;
            self.v = e * (self.v - j1 * w * dt);
            return self.x;
        }
        let wd = w * (1.0 - z * z).sqrt();
        let (s, c) = (wd * dt).sin_cos();
        let b = (self.v + j0 * (z * w)) / wd;
        let at = j0 * c + b * s;
        self.x = goal + at * e;
        self.v = (at * (-z * w) + (b * c - j0 * s) * wd) * e;
        self.x
    }

    pub fn kick(&mut self, dv: Vec3) {
        self.v += dv;
    }
}

/// How many times a second a spring that settles with `halflife` swings (`Sway3`'s `hz` for the
/// same stiffness as a `Spring3` of that half-life).
pub fn hz_of(halflife: f32) -> f32 {
    damping(halflife) / 2.0 / std::f32::consts::TAU
}

/// The same for a turn (its speed as a turn vector, rad/s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringQ {
    pub x: Quat,
    pub v: Vec3,
}

impl Default for SpringQ {
    fn default() -> SpringQ {
        SpringQ { x: Quat::IDENTITY, v: Vec3::ZERO }
    }
}

impl SpringQ {
    pub fn step(&mut self, goal: Quat, halflife: f32, dt: f32) -> Quat {
        let y = damping(halflife) / 2.0;
        // what is left to turn, as a vector (the short way round)
        let mut d = self.x * goal.inverse();
        if d.w < 0.0 {
            d = -d;
        }
        let j0 = d.to_scaled_axis();
        let j1 = self.v + j0 * y;
        let e = (-y * dt).exp();
        self.x = (Quat::from_scaled_axis(e * (j0 + j1 * dt)) * goal).normalize();
        self.v = e * (self.v - j1 * y * dt);
        self.x
    }

    pub fn kick(&mut self, dv: Vec3) {
        self.v += dv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_halves_what_is_left_in_about_its_half_life_and_never_overshoots() {
        let mut s = Spring::at(1.0);
        let (mut t, mut least) = (0.0, f32::MAX);
        let mut at_half = None;
        while t < 2.0 {
            s.step(0.0, 0.2, 1.0 / 120.0);
            t += 1.0 / 120.0;
            least = least.min(s.x);
            if at_half.is_none() && s.x <= 0.5 {
                at_half = Some(t);
            }
        }
        assert!(least >= -1e-6, "it swings past its goal: {least}");
        assert!(s.x.abs() < 1e-3);
        // (from rest it first has to get going: a little over its half-life)
        assert!(at_half.is_some_and(|t| t > 0.2 && t < 0.4), "{at_half:?}");
    }

    #[test]
    fn the_step_does_not_matter() {
        let (mut a, mut b) = (Spring3::at(Vec3::X), Spring3::at(Vec3::X));
        a.kick(Vec3::new(0.0, 3.0, 0.0));
        b.kick(Vec3::new(0.0, 3.0, 0.0));
        for _ in 0..30 {
            a.step(Vec3::ZERO, 0.15, 1.0 / 30.0);
        }
        for _ in 0..240 {
            b.step(Vec3::ZERO, 0.15, 1.0 / 240.0);
        }
        assert!((a.x - b.x).length() < 1e-4 && (a.v - b.v).length() < 1e-3, "{:?} vs {:?}", a.x, b.x);
    }

    #[test]
    fn a_sway_goes_past_its_goal_once_and_settles_the_same_at_any_step() {
        let run = |fps: f32, ratio: f32| {
            let mut s = Sway3::at(Vec3::X);
            let (mut least, mut t) = (f32::MAX, 0.0);
            while t < 2.0 {
                s.step(Vec3::ZERO, 3.0, ratio, 1.0 / fps);
                t += 1.0 / fps;
                least = least.min(s.x.x);
            }
            (least, s.x.x, s.v.x)
        };
        // half damped: past its goal by a sixth of the way, and still in the end
        let (past, end, speed) = run(120.0, 0.5);
        assert!(past < -0.1 && past > -0.2, "past by {past}");
        assert!(end.abs() < 1e-3 && speed.abs() < 1e-2);
        // fully damped: never past it
        assert!(run(120.0, 1.0).0 >= -1e-6);
        // the same swing at 30 frames a second as at 240
        let (a, b) = (run(30.0, 0.4), run(240.0, 0.4));
        assert!((a.0 - b.0).abs() < 0.02, "{} vs {}", a.0, b.0);
        // as stiff as the plain spring of that half-life
        assert!((hz_of(0.08) - 2.76).abs() < 0.05, "{}", hz_of(0.08));
    }

    #[test]
    fn a_turn_settles_on_its_goal_the_short_way() {
        let mut s = SpringQ::default();
        let goal = Quat::from_rotation_y(3.0);
        let mut most = 0.0f32;
        for _ in 0..600 {
            s.step(goal, 0.1, 1.0 / 120.0);
            most = most.max(s.x.angle_between(Quat::IDENTITY));
        }
        assert!(s.x.angle_between(goal) < 1e-3);
        assert!(most < 3.05, "it went the long way: {most}");
    }
}
