//! A thing as a rigid body — a ship, a crate, a drum, a piece of wreck — and the positions of its
//! joints if it has any (gear, doors, ramps): what whoever simulates it sends so the others'
//! copies of it follow. Its place may be told in the frame of another thing (the ship it lies in,
//! the ship it flies beside): then it is small numbers, and it holds however fast both go.
use super::POS_UNITS;
use crate::wire::{Reader, Wire, WireError, Writer};
use glam::{DVec3, Quat, Vec3};

/// A thing's velocity: steps per m/s (about 1 mm/s). 2 bytes per axis up to 8 m/s, 4 at orbital speed.
pub const RIGID_VEL_UNITS: f64 = 1024.0;
/// Spin: steps per rad/s. 2 bytes per axis up to 2 rad/s.
pub const SPIN_UNITS: f64 = 4096.0;
/// Joint positions (metres, radians or fractions, as the ship has them): steps per unit, so the
/// error is at most 0.25 mm or 0.014°. 1 byte for a joint at 0, 2 bytes up to ±4, 3 up to ±512.
pub const JOINT_UNITS: f64 = 2048.0;
/// The most joints a state carries: more are not sent (and more on the wire is refused).
pub const MAX_JOINTS: usize = 64;

const MOVING: u8 = 1;
const BODY: u8 = 2;
const JOINTS: u8 = 4;
const FRAME: u8 = 8;
const TURNS: u8 = 16;

/// The frame a state is told in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Frame {
    /// The world's: position from its origin, velocity against it.
    #[default]
    World,
    /// Beside thing `0`: position from that thing's origin and velocity against its own, both in
    /// the world's axes (two ships flying together).
    Beside(u64),
    /// Aboard thing `0`: position, turn, velocity and spin in that thing's own frame, turning with
    /// it (cargo in a hold, something carried along a deck).
    Aboard(u64),
}

impl Frame {
    /// The thing it hangs from, if any.
    pub fn of(&self) -> Option<u64> {
        match *self {
            Frame::World => None,
            Frame::Beside(id) | Frame::Aboard(id) => Some(id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RigidState {
    /// The thing's network id.
    pub id: u64,
    /// The celestial body it is at.
    pub body: u8,
    pub frame: Frame,
    pub pos: DVec3,
    pub rot: Quat,
    /// Velocity, m/s.
    pub vel: Vec3,
    /// Angular velocity, rad/s.
    pub spin: Vec3,
    pub joints: Vec<f32>,
}

impl RigidState {
    /// The whole state, its id first: 75 bytes for a ship flying near the Moon with 20 joints; 23 parked with none.
    pub fn encode(&self, w: &mut Writer) {
        let joints = &self.joints[..self.joints.len().min(MAX_JOINTS)];
        w.var(self.id);
        self.write_rigid(w, if joints.is_empty() { 0 } else { JOINTS });
        if !joints.is_empty() {
            w.u8(joints.len() as u8);
            joints.iter().for_each(|j| w.fixed(*j as f64, JOINT_UNITS));
        }
    }

    /// The state without its id nor its joints (33 bytes flying near the Moon, 22 parked, 13 lying
    /// in a ship's hold): what goes every tick while it moves. The joints go apart
    /// (`encode_joints`), only when they change: in a ship that flies they are more than half the
    /// bytes, and they hardly ever move.
    pub fn encode_rigid(&self, w: &mut Writer) {
        self.write_rigid(w, 0);
    }

    fn write_rigid(&self, w: &mut Writer, joints: u8) {
        let vel = self.vel.to_array().map(|v| (v as f64 * RIGID_VEL_UNITS).round() as i64);
        let spin = self.spin.to_array().map(|v| (v as f64 * SPIN_UNITS).round() as i64);
        let moving = vel != [0; 3] || spin != [0; 3];
        let frame = match self.frame {
            Frame::World => 0,
            Frame::Beside(_) => FRAME,
            Frame::Aboard(_) => FRAME | TURNS,
        };
        w.u8(if moving { MOVING } else { 0 } | if self.body != 0 { BODY } else { 0 } | joints | frame);
        if self.body != 0 {
            w.u8(self.body);
        }
        if let Some(of) = self.frame.of() {
            w.var(of);
        }
        w.fixed3(self.pos, POS_UNITS);
        w.quat(self.rot);
        if moving {
            vel.iter().chain(&spin).for_each(|v| w.zig(*v));
        }
    }

    /// The joints alone.
    pub fn encode_joints(&self, w: &mut Writer) {
        let joints = &self.joints[..self.joints.len().min(MAX_JOINTS)];
        w.u8(joints.len() as u8);
        joints.iter().for_each(|j| w.fixed(*j as f64, JOINT_UNITS));
    }

    /// Reads what `encode_joints` wrote into `out` (in place of what it had).
    pub fn decode_joints(r: &mut Reader, out: &mut Vec<f32>) -> Wire<()> {
        let n = r.u8()? as usize;
        if n > MAX_JOINTS {
            return Err(WireError::Long);
        }
        out.clear();
        for _ in 0..n {
            out.push(r.fixed(JOINT_UNITS)? as f32);
        }
        Ok(())
    }

    /// Reads what `encode` wrote.
    pub fn decode(r: &mut Reader) -> Wire<RigidState> {
        let mut s = RigidState { id: r.var()?, ..RigidState::default() };
        s.decode_rigid(r)?;
        Ok(s)
    }

    /// Reads what `encode_rigid` wrote (or the rest of an `encode`) over this state, keeping its id
    /// and the room its joints already have (nothing is allocated once it has been as long). A
    /// state that brings no joints leaves this one with none.
    pub fn decode_rigid(&mut self, r: &mut Reader) -> Wire<()> {
        let bits = r.u8()?;
        if bits & !(MOVING | BODY | JOINTS | FRAME | TURNS) != 0 || bits & (FRAME | TURNS) == TURNS {
            return Err(WireError::Value);
        }
        self.body = if bits & BODY != 0 { r.u8()? } else { 0 };
        self.frame = match (bits & FRAME != 0, bits & TURNS != 0) {
            (false, _) => Frame::World,
            (true, false) => Frame::Beside(r.var()?),
            (true, true) => Frame::Aboard(r.var()?),
        };
        self.pos = r.fixed3(POS_UNITS)?;
        self.rot = r.quat()?;
        (self.vel, self.spin) = if bits & MOVING != 0 { (r.fixed3(RIGID_VEL_UNITS)?.as_vec3(), r.fixed3(SPIN_UNITS)?.as_vec3()) } else { (Vec3::ZERO, Vec3::ZERO) };
        self.joints.clear();
        if bits & JOINTS != 0 {
            RigidState::decode_joints(r, &mut self.joints)?;
        }
        Ok(())
    }

    /// The state a fraction `t` (0..1) of the way from `a` to `b`.
    pub fn mix(a: &RigidState, b: &RigidState, t: f32) -> RigidState {
        let mut out = RigidState::default();
        RigidState::mix_into(a, b, t, &mut out);
        out
    }

    /// `mix` into a state that already exists (its joints keep their room): positions in a straight
    /// line, the rotation along the shortest arc, joints one by one. Two states told in different
    /// frames have no half way between them: the nearer one is taken.
    pub fn mix_into(a: &RigidState, b: &RigidState, t: f32, out: &mut RigidState) {
        let t = t.clamp(0.0, 1.0);
        let near = if t < 0.5 { a } else { b };
        out.id = near.id;
        out.body = near.body;
        out.frame = near.frame;
        if a.body == b.body && a.frame == b.frame {
            out.pos = a.pos.lerp(b.pos, t as f64);
            out.rot = a.rot.slerp(b.rot, t);
            out.vel = a.vel.lerp(b.vel, t);
            out.spin = a.spin.lerp(b.spin, t);
        } else {
            (out.pos, out.rot, out.vel, out.spin) = (near.pos, near.rot, near.vel, near.spin);
        }
        out.joints.clear();
        if a.joints.len() == b.joints.len() {
            out.joints.extend(a.joints.iter().zip(&b.joints).map(|(x, y)| x + (y - x) * t));
        } else {
            out.joints.extend_from_slice(&near.joints);
        }
    }

    /// Copies `from` over this state without giving up the room of the joints.
    pub fn set(&mut self, from: &RigidState) {
        self.set_with(from, &from.joints);
    }

    /// Copies the rigid part of `from` and takes `joints` for its joints.
    pub fn set_with(&mut self, from: &RigidState, joints: &[f32]) {
        (self.id, self.body, self.frame, self.pos, self.rot, self.vel, self.spin) = (from.id, from.body, from.frame, from.pos, from.rot, from.vel, from.spin);
        self.joints.clear();
        self.joints.extend_from_slice(joints);
    }

    /// The state `dt` seconds later if nothing pushes it: it goes on at its velocity and its spin
    /// (in whatever frame it is told in).
    pub fn carry(&mut self, dt: f32) {
        self.pos += (self.vel * dt).as_dvec3();
        self.rot = (Quat::from_scaled_axis(self.spin * dt) * self.rot).normalize();
    }
}
