//! The view from outside one's own body (V). On foot: a camera behind the right shoulder that
//! looks where the player looks, comes in when something is in its way and stays over the
//! ground. Seated: a camera round the ship, as far out as the ship is big. What the player aims
//! at stays what is under the middle of the picture (`aim`): hands, tools and the body are the
//! player's own, wherever the picture is taken from.
use glam::DVec3;
use lunar_core::{body::BodyRegistry, structure::set::Structures};
use lunar_render::View;

/// How far behind the head the camera sits (m): the nearest, the one it starts at, the farthest.
const REACH: [f64; 3] = [1.2, 3.2, 14.0];
/// What a notch of the wheel does to it (times).
const NOTCH: f64 = 1.15;
/// To the right of the head and over it (m; the side comes in as the camera does).
const SHOULDER: f64 = 0.42;
const OVER: f64 = 0.22;
/// How clear it stays of what is behind it and of the ground (m), and the least it comes in to.
const CLEAR: f64 = 0.25;
const NEAREST: f64 = 0.2;
/// Parts smaller than this are not in its way (bounding radius, m).
const SMALL: f32 = 0.1;
/// How fast it goes back out once nothing is in its way (1/s).
const OUT: f64 = 5.0;
/// Seated: how far out in radii of the ship at the nearest and per metre of reach, how high over
/// the line of sight (share of the distance), and how clear of the ground (m).
const SHIP_NEAR: f64 = 1.25;
const SHIP_REACH: f64 = 0.38;
const SHIP_OVER: f64 = 0.16;
const SHIP_CLEAR: f64 = 1.2;
/// How far what is aimed at is looked for (m).
const AIM: f64 = 200.0;

pub struct Chase {
    pub on: bool,
    /// How far out it is asked to be (m, on foot).
    reach: f64,
    /// How far out it is.
    at: f64,
    /// Whether it was round a ship last frame (it jumps from one to the other, it does not ease).
    ship: bool,
}

impl Default for Chase {
    fn default() -> Chase {
        Chase { on: false, reach: REACH[1], at: REACH[1], ship: false }
    }
}

impl Chase {
    /// On or off; whether it is on now.
    pub fn toggle(&mut self) -> bool {
        self.set(!self.on);
        self.on
    }

    pub fn set(&mut self, on: bool) {
        self.on = on;
        self.at = NEAREST;
    }

    /// The wheel: nearer or farther. True when it took the notches.
    pub fn wheel(&mut self, notches: f64) -> bool {
        if self.on {
            self.reach = (self.reach * NOTCH.powf(-notches)).clamp(REACH[0], REACH[2]);
        }
        self.on
    }

    /// How far out it is asked to be (m): a script's.
    pub fn reach(&mut self, m: f64) {
        self.reach = m.clamp(REACH[0], REACH[2]);
    }

    /// The picture's view, from the player's own (`own`: their eyes, the way they look). `ship`:
    /// the middle and radius of the ship they sit in, if they sit.
    pub fn view(&mut self, dt: f64, own: &View, ship: Option<(DVec3, f64)>, set: &Structures, bodies: &BodyRegistry) -> View {
        let (f, up) = (own.forward, own.up);
        let right = f.cross(up).normalize_or(up.any_orthonormal_vector());
        if self.ship != ship.is_some() {
            self.ship = ship.is_some();
            self.at = if self.ship { f64::MAX } else { NEAREST };
        }
        let (pivot, to, clear) = match ship {
            // round the ship: behind the way one looks and a little over it
            Some((centre, radius)) => {
                let d = radius * (SHIP_NEAR + SHIP_REACH * (self.reach - REACH[0]));
                (centre, -f * d + up * (d * SHIP_OVER), SHIP_CLEAR)
            }
            // behind the shoulder
            None => (own.eye + up * OVER, -f * self.reach + right * (SHOULDER * (self.reach / 2.0).min(1.0)), CLEAR),
        };
        let far = to.length();
        let dir = to / far;
        // what is in its way brings it in at once; it goes back out with time
        let room = match ship {
            Some(_) => far,
            None => set.raycast_solid(pivot, dir, far + CLEAR, SMALL).map_or(far, |(_, t, _)| (t - CLEAR).clamp(NEAREST, far)),
        };
        self.at = if room <= self.at { room } else { self.at + (room - self.at) * (1.0 - (-dt * OUT).exp()) };
        let mut eye = pivot + dir * self.at;
        // over the ground
        let body = bodies.get(bodies.dominant(eye));
        let low = clear - body.altitude(eye);
        if low > 0.0 {
            eye += body.up(eye) * low;
        }
        View { eye, forward: f, up, ..*own }
    }

    /// The player's own view turned to what is under the middle of the picture taken from `cam`:
    /// what they aim at from outside is what the crosshair is on.
    pub fn aim(&self, own: &View, cam: &View, set: &Structures) -> View {
        // (from past the player on: what is between the camera and them is not aimed at)
        let from = cam.eye + cam.forward * (own.eye - cam.eye).dot(cam.forward).max(0.0);
        let t = set.raycast_solid(from, cam.forward, AIM, 0.0).map_or(AIM, |(_, t, _)| t);
        let target = from + cam.forward * t.max(1.0);
        View { forward: (target - own.eye).normalize_or(own.forward), ..*own }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_core::{
        body::{Body, BodyDef},
        defs,
    };
    use std::sync::Arc;

    fn world() -> (Structures, BodyRegistry) {
        let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        let bodies = BodyRegistry::new(vec![Body::from_def("luna", &moon).unwrap()]);
        let lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
        (Structures::new(Arc::new(lib)), bodies)
    }

    fn standing(bodies: &BodyRegistry) -> View {
        let b = bodies.get(0);
        let up = DVec3::new(0.3, 0.8, 0.52).normalize();
        let eye = b.above_ground(up, 1.75);
        let ahead = up.any_orthonormal_vector();
        View { eye, forward: ahead, up, fov_y: 1.2, near: 0.05 }
    }

    #[test]
    fn on_foot_it_sits_behind_the_shoulder_looking_the_same_way() {
        let (set, bodies) = world();
        let own = standing(&bodies);
        let mut c = Chase::default();
        assert!(c.toggle());
        let mut v = own;
        for _ in 0..240 {
            v = c.view(1.0 / 60.0, &own, None, &set, &bodies);
        }
        let d = v.eye - own.eye;
        let right = own.forward.cross(own.up);
        assert!((d.dot(own.forward) + REACH[1]).abs() < 0.05, "{} m behind", -d.dot(own.forward));
        assert!((d.dot(right) - SHOULDER).abs() < 0.02 && (d.dot(own.up) - OVER).abs() < 0.3);
        assert!((v.forward - own.forward).length() < 1e-9);
        // what it aims at is what is under the middle of the picture, far away: the same way
        let a = c.aim(&own, &v, &set);
        assert!(a.eye == own.eye && a.forward.dot(own.forward) > 0.9999);
    }

    #[test]
    fn it_never_goes_under_the_ground_and_the_wheel_brings_it_in_and_out() {
        let (set, bodies) = world();
        let mut own = standing(&bodies);
        // looking up: the camera would be under the feet
        own.forward = (own.forward + own.up * 1.5).normalize();
        let mut c = Chase::default();
        c.toggle();
        for _ in 0..40 {
            c.wheel(-1.0);
        }
        let mut v = own;
        for _ in 0..600 {
            v = c.view(1.0 / 60.0, &own, None, &set, &bodies);
        }
        assert!(bodies.get(0).altitude(v.eye) >= CLEAR - 1e-6, "{} m over the ground", bodies.get(0).altitude(v.eye));
        let far = (v.eye - own.eye).length();
        for _ in 0..80 {
            c.wheel(1.0);
        }
        for _ in 0..600 {
            v = c.view(1.0 / 60.0, &own, None, &set, &bodies);
        }
        assert!((v.eye - own.eye).length() < far && (v.eye - own.eye).length() < REACH[0] + 0.6);
        // off: the wheel is not its own
        c.toggle();
        assert!(!c.wheel(1.0));
    }

    #[test]
    fn seated_it_goes_round_the_ship_as_far_out_as_the_ship_is_big() {
        let (set, bodies) = world();
        let own = standing(&bodies);
        let b = bodies.get(0);
        let centre = b.above_ground(own.up, 3.0);
        let mut c = Chase::default();
        c.toggle();
        for radius in [3.0, 14.0] {
            let mut v = own;
            for _ in 0..600 {
                v = c.view(1.0 / 60.0, &own, Some((centre, radius)), &set, &bodies);
            }
            let d = (v.eye - centre).length();
            assert!(d > radius * 1.5 && d < radius * 3.5, "radius {radius}: {d} m out");
            assert!(b.altitude(v.eye) >= SHIP_CLEAR - 1e-6);
            // the ship is in the picture: its middle within the view's cone
            assert!((centre - v.eye).normalize().dot(v.forward) > 0.9);
        }
    }
}
