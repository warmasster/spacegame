//! Bare hands: with nothing in them, a click held on something loose takes hold of it and it
//! follows the look — carried if the hands can lift it, dragged along the deck if they cannot —
//! until the button is let go (it keeps the speed it had: a throw). The wheel brings it nearer or
//! farther.
//!
//! What is held stays a body like any other: it bumps into walls, rests on what is under it,
//! pushes what is in its way. The hands only pull at the point they took it by, as a spring and
//! a damper would, with no more than their strength (`HandsDef`). The pull is worked out for
//! where the thing will be at the end of the step, not where it is ("stable PD", Tan, Liu & Turk
//! 2011): it does not overshoot or ring whatever the frame rate, and needs nothing of the solver.
//! Speeds are taken relative to the ship the player stands on, so carrying works aboard a ship
//! under way.
use crate::{builds::Builds, ships::Ships};
use glam::{DVec3, Mat3, Vec3};
use lunar_core::{scenario::HandsDef, structure::set::Structures};
use lunar_render::View;

/// How near and how far what is held is kept (m), and how much a wheel notch moves it.
const NEAR: f64 = 0.9;
const NOTCH: f64 = 0.15;
/// Let go by itself when what is held is this far from where it should be (m): it is stuck.
const LOST: f64 = 2.2;
/// How hard its turning is damped (1/s).
const SPIN_DAMP: f32 = 9.0;

#[derive(Clone, Copy, Debug)]
struct Grab {
    id: u64,
    /// The point it was taken by (its frame), and how far from the eye it is held.
    at: Vec3,
    dist: f64,
}

/// What the hands could take, for the HUD.
#[derive(Clone, Debug, PartialEq)]
pub struct Reach {
    pub id: u64,
    pub name: String,
    pub mass: f32,
    /// Why not, if it cannot be taken.
    pub no: Option<&'static str>,
}

#[derive(Default)]
pub struct Hands {
    def: Option<HandsDef>,
    held: Option<Grab>,
}

impl Hands {
    pub fn new(def: Option<HandsDef>) -> Hands {
        Hands { def, held: None }
    }

    /// The structure in the hands.
    pub fn holding(&self) -> Option<u64> {
        self.held.map(|g| g.id)
    }

    /// What is under the look within reach that hands might take: loose things, not ships, not
    /// what the player stands on.
    pub fn reach(&self, set: &Structures, ships: &Ships, view: &View, standing_on: Option<u64>) -> Option<(Reach, Vec3, f64)> {
        let def = self.def?;
        let (k, hit, at) = set.raycast(view.eye, view.forward, def.alcance)?;
        let s = &set.list[k];
        if s.anchored || ships.by_structure(s.id).is_some() {
            return None;
        }
        let no = if s.held.is_some() {
            Some("está anclada: abre su anclaje")
        } else if standing_on == Some(s.id) {
            Some("estás encima")
        } else if s.mass > def.masa {
            Some("pesa demasiado")
        } else {
            None
        };
        let _ = hit;
        Some((Reach { id: s.id, name: s.label(&set.lib.catalog), mass: s.mass, no }, s.to_local(at), at.distance(view.eye)))
    }

    /// The button went down with nothing else under it: take what is there. What to tell the
    /// player if it cannot be taken; true if something was.
    pub fn grab(&mut self, set: &Structures, ships: &Ships, view: &View, standing_on: Option<u64>) -> Result<bool, &'static str> {
        let Some((r, at, dist)) = self.reach(set, ships, view, standing_on) else { return Ok(false) };
        if let Some(why) = r.no {
            return Err(why);
        }
        self.held = Some(Grab { id: r.id, at, dist: dist.max(NEAR) });
        Ok(true)
    }

    /// The button went up: let go (it keeps going as it was).
    pub fn release(&mut self, builds: &mut Builds) {
        if let Some(g) = self.held.take()
            && let Some(k) = builds.set.index_of(g.id)
        {
            let s = &mut builds.set.list[k];
            (s.force, s.torque) = (Vec3::ZERO, Vec3::ZERO);
        }
    }

    /// Wheel notches: nearer or farther. True if the hands took them.
    pub fn wheel(&mut self, notches: f64) -> bool {
        let (Some(g), Some(def)) = (&mut self.held, self.def) else { return false };
        g.dist = (g.dist + notches * NOTCH).clamp(NEAR, def.alcance);
        true
    }

    /// Pull what is held toward where the look puts it. `aboard`: the structure the player
    /// stands on or rides (speeds are relative to it).
    pub fn update(&mut self, dt: f64, builds: &mut Builds, view: &View, aboard: Option<u64>) {
        let (Some(g), Some(def)) = (self.held, self.def) else { return };
        let set = &mut builds.set;
        let Some(k) = set.index_of(g.id).filter(|&k| set.list[k].held.is_none() && set.list[k].alive()) else {
            self.held = None;
            return;
        };
        let (at, target) = (set.list[k].to_world(g.at), view.eye + view.forward * g.dist);
        if at.distance(target) > LOST {
            self.release(builds);
            return;
        }
        // speeds relative to what the player is on
        let frame = aboard.filter(|id| *id != g.id).and_then(|id| set.get(id)).map(|h| (h.velocity_at(at), h.spin));
        let (v_ref, w_ref) = frame.unwrap_or((DVec3::ZERO, Vec3::ZERO));
        let s = &mut set.list[k];
        let force = pull(s.mass, (target - at).as_vec3(), (s.velocity_at(at) - v_ref).as_vec3(), def.frecuencia, def.fuerza, dt as f32);
        let arm = (at - s.to_world(s.com)).as_vec3();
        // its turning damped (in the world's axes): it hangs from the hand, it does not spin
        let r = Mat3::from_quat(s.rot);
        let spin = r * s.inertia * r.transpose() * (s.spin - w_ref);
        s.force = force;
        s.torque = arm.cross(force) - spin * (SPIN_DAMP / (1.0 + SPIN_DAMP * dt as f32));
        s.resting = false;
        s.still = 0.0;
    }
}

/// The pull (N) of a spring and damper of `hz` on a mass `m` that is `off` from where it should be
/// and moving at `v` relative to it, capped at `max`: worked out at the end of the step `dt`, so
/// it is stable at any stiffness.
pub fn pull(m: f32, off: Vec3, v: Vec3, hz: f32, max: f32, dt: f32) -> Vec3 {
    let w = std::f32::consts::TAU * hz;
    let (kp, kd) = (m * w * w, 2.0 * m * w);
    let a = (off * kp - v * (kd + kp * dt)) / (m + kd * dt + kp * dt * dt);
    (a * m).clamp_length_max(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pull_brings_a_mass_to_the_hand_without_ringing_at_any_frame_rate() {
        // a 40 kg crate half a metre from the hand, at 144, 60 and 20 frames a second: it gets
        // there in about a second, never past it by more than a few centimetres, and stays
        for fps in [144.0f32, 60.0, 20.0] {
            let dt = 1.0 / fps;
            let (m, mut x, mut v) = (40.0, Vec3::new(0.5, 0.0, 0.0), Vec3::ZERO);
            let mut farthest = 0.0f32;
            for _ in 0..(2.0 * fps) as usize {
                let f = pull(m, -x, v, 3.0, 1200.0, dt);
                v += f / m * dt;
                x += v * dt;
                farthest = farthest.max(-x.x);
            }
            assert!(x.length() < 0.01 && v.length() < 0.05, "a {fps} fps queda a {:.3} m", x.length());
            assert!(farthest < 0.05, "a {fps} fps se pasa {farthest:.3} m");
        }
        // what the hands cannot lift they do not: the pull is capped
        let f = pull(2000.0, Vec3::Y * 2.0, Vec3::ZERO, 3.0, 1200.0, 1.0 / 60.0);
        assert!((f.length() - 1200.0).abs() < 1e-3);
    }
}
