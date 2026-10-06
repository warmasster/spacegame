//! Looking round without turning (Alt held): the mouse turns the head — from one's own eyes — or
//! the camera — from outside — and not the body, which goes on facing, aiming and working where
//! it was. Let go, the look comes back to where the body faces.
use lunar_render::View;

/// How far the head turns from where the body faces: to a side and up or down (rad). From
/// outside the camera goes all the way round.
const HEAD: (f64, f64) = (1.95, 1.2);
/// The steepest the look ever is (rad from level).
const STEEP: f64 = 1.5;
/// Let go, the half-life with which it comes back (s).
const BACK: f64 = 0.07;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FreeLook {
    /// To the right and up of where the body looks (rad).
    pub yaw: f64,
    pub pitch: f64,
    /// Held this frame.
    pub held: bool,
}

impl FreeLook {
    /// The mouse, while it is held: `outside`, the camera's (all the way round); else the head's.
    pub fn turn(&mut self, right: f64, down: f64, outside: bool) {
        self.yaw += right;
        self.pitch -= down;
        if outside {
            self.yaw = (self.yaw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
            self.pitch = self.pitch.clamp(-STEEP, STEEP);
        } else {
            self.yaw = self.yaw.clamp(-HEAD.0, HEAD.0);
            self.pitch = self.pitch.clamp(-HEAD.1, HEAD.1);
        }
    }

    /// A frame gone by: held or let go (then it comes back).
    pub fn update(&mut self, dt: f64, held: bool) {
        self.held = held;
        if !held {
            let k = (-dt * std::f64::consts::LN_2 / BACK).exp();
            self.yaw *= k;
            self.pitch *= k;
            if self.yaw.abs() < 1e-4 && self.pitch.abs() < 1e-4 {
                (self.yaw, self.pitch) = (0.0, 0.0);
            }
        }
    }

    /// Whether the look is away from where the body faces.
    pub fn active(&self) -> bool {
        self.held || self.yaw != 0.0 || self.pitch != 0.0
    }

    /// `view` (the body's own look) turned by it: the eye where it is, the same way up.
    pub fn apply(&self, view: &View) -> View {
        if self.yaw == 0.0 && self.pitch == 0.0 {
            return *view;
        }
        let up = view.up;
        let rise = view.forward.dot(up).clamp(-1.0, 1.0);
        let level = (view.forward - up * rise).normalize_or(up.any_orthonormal_vector());
        let right = level.cross(up);
        let turned = level * self.yaw.cos() + right * self.yaw.sin();
        let pitch = (rise.asin() + self.pitch).clamp(-STEEP, STEEP);
        View { forward: (turned * pitch.cos() + up * pitch.sin()).normalize_or(view.forward), ..*view }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    /// The way from `a` to `b` round `up`, to the right positive (rad).
    fn right_of(a: DVec3, b: DVec3, up: DVec3) -> f64 {
        let (a, b) = ((a - up * a.dot(up)).normalize(), (b - up * b.dot(up)).normalize());
        a.cross(up).dot(b).atan2(a.dot(b))
    }

    fn view() -> View {
        let up = DVec3::new(0.2, 0.9, 0.1).normalize();
        let ahead = up.any_orthonormal_vector();
        View { eye: DVec3::new(10.0, 20.0, 30.0), forward: (ahead * 0.95 + up * 0.2).normalize(), up, fov_y: 1.2, near: 0.05 }
    }

    #[test]
    fn the_head_turns_as_far_as_a_head_does_and_the_eye_stays_where_it_is() {
        let v = view();
        let mut f = FreeLook::default();
        assert!(!f.active() && f.apply(&v).forward == v.forward);
        f.update(0.016, true);
        f.turn(0.6, 0.0, false);
        let w = f.apply(&v);
        assert!(w.eye == v.eye && w.up == v.up);
        assert!((right_of(v.forward, w.forward, v.up) - 0.6).abs() < 1e-9, "turned {}", right_of(v.forward, w.forward, v.up));
        // (as high as it was: only round)
        assert!((w.forward.dot(v.up) - v.forward.dot(v.up)).abs() < 1e-9);
        // up, and no further than a head goes; never over the top
        f.turn(9.0, -9.0, false);
        assert!((f.yaw - HEAD.0).abs() < 1e-12 && (f.pitch - HEAD.1).abs() < 1e-12);
        assert!(f.apply(&v).forward.dot(v.up) < STEEP.sin() + 1e-9);
    }

    #[test]
    fn from_outside_the_camera_goes_all_the_way_round() {
        let v = view();
        let mut f = FreeLook::default();
        f.update(0.016, true);
        for _ in 0..100 {
            f.turn(0.1, 0.0, true);
        }
        // ten radians round: the same as 10 − 4π... whatever it is, a whole way round and more
        assert!((f.yaw - (10.0 - std::f64::consts::TAU * 2.0)).abs() < 1e-6 || (f.yaw - (10.0 - std::f64::consts::TAU)).abs() < 1e-6, "{}", f.yaw);
        let back = FreeLook { yaw: std::f64::consts::PI, pitch: 0.0, held: true }.apply(&v);
        assert!(right_of(v.forward, back.forward, v.up).abs() > 3.1);
    }

    #[test]
    fn let_go_it_comes_back_to_where_the_body_faces() {
        let v = view();
        let mut f = FreeLook { yaw: 1.2, pitch: -0.5, held: true };
        for _ in 0..60 {
            f.update(1.0 / 60.0, false);
        }
        assert!(!f.active() && f.apply(&v).forward == v.forward, "{f:?}");
        // held, it stays where it is left
        let mut f = FreeLook { yaw: 1.2, pitch: -0.5, held: true };
        f.update(1.0, true);
        assert!(f.active() && f.yaw == 1.2);
    }
}
