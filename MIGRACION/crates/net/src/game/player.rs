//! A player as the others need to draw them: where the eye is, where it looks, how it moves, what
//! the pack pushes with, what the tool in hand works on, and a few flags. The body is animated by
//! each client from this, so this is all that travels.
use super::{POS_UNITS, mix_angle};
use crate::wire::{Reader, Wire, WireError, Writer};
use glam::{DVec3, Vec3};

/// Positions in a thing's frame: steps per metre (0.49 mm). 3 bytes per axis up to 512 m.
pub const LOCAL_UNITS: f64 = 2048.0;
/// A player's velocity: steps per m/s (3.9 mm/s). 2 bytes per axis up to 32 m/s.
pub const PLAYER_VEL_UNITS: f64 = 256.0;
/// The pack's push: steps per unit (1: all it has). A byte per axis.
pub const PUSH_UNITS: f32 = 100.0;

// What is present after the first number (a varint: one byte unless something of the last ones
// is): what is absent has its default and costs nothing.
const RIDE: u16 = 1;
const SEAT: u16 = 2;
const TOOL: u16 = 4;
const HEAD: u16 = 8;
const VEL: u16 = 16;
const BODY: u16 = 32;
const PUSH: u16 = 64;
const WORK: u16 = 128;
const GESTURE: u16 = 256;
const ALL: u16 = 511;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    /// The celestial body the player is at.
    pub body: u8,
    /// The eye, in world coordinates.
    pub pos: DVec3,
    /// The thing (network id) the player rides, if any.
    pub ride: Option<u64>,
    /// The eye in that thing's frame. While riding, receivers place the player from this (and their
    /// own copy of the thing), so the player does not shake against a ship that moves.
    pub local: Vec3,
    /// Where the body faces, radians.
    pub yaw: f32,
    pub pitch: f32,
    /// Free look of the head over the body: yaw and pitch, radians.
    pub head: [f32; 2],
    /// World velocity, m/s.
    pub vel: Vec3,
    /// Bits of `flag`.
    pub flags: u16,
    /// The seat taken: thing (network id) and seat index.
    pub seat: Option<(u64, u8)>,
    /// What is in hand; 0: bare hands.
    pub tool: u8,
    /// Height of the eye over the feet, metres (it travels in centimetres, up to 2.55 m).
    pub eye_h: f32,
    /// What the pack pushes with, world axes, as a share of all it has (each axis -1.27..1.27):
    /// the others draw its jets against it.
    pub push: Vec3,
    /// What the tool in hand is at work on: the thing (network id) and the point of it (its frame).
    pub work: Option<(u64, Vec3)>,
    /// What the body is doing with itself on purpose (a wave, a pointing arm: a number the game
    /// gives a meaning to); 0: nothing.
    pub gesture: u8,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState { body: 0, pos: DVec3::ZERO, ride: None, local: Vec3::ZERO, yaw: 0.0, pitch: 0.0, head: [0.0; 2], vel: Vec3::ZERO, flags: 0, seat: None, tool: 0, eye_h: 1.6, push: Vec3::ZERO, work: None, gesture: 0 }
    }
}

fn push_steps(v: Vec3) -> [i8; 3] {
    v.to_array().map(|x| (x * PUSH_UNITS).round().clamp(-127.0, 127.0) as i8)
}

impl PlayerState {
    /// 26 bytes for someone walking on the Moon; 22 standing still; 37 seated at the controls of a ship.
    pub fn encode(&self, w: &mut Writer) {
        let vel = self.vel.to_array().map(|v| (v as f64 * PLAYER_VEL_UNITS).round() as i64);
        let push = push_steps(self.push);
        let mut bits = 0;
        for (on, bit) in [(self.ride.is_some(), RIDE), (self.seat.is_some(), SEAT), (self.tool != 0, TOOL), (self.head != [0.0; 2], HEAD), (vel != [0; 3], VEL), (self.body != 0, BODY), (push != [0; 3], PUSH), (self.work.is_some(), WORK), (self.gesture != 0, GESTURE)] {
            if on {
                bits |= bit;
            }
        }
        w.var(u64::from(bits));
        if self.body != 0 {
            w.u8(self.body);
        }
        w.var(self.flags as u64);
        w.fixed3(self.pos, POS_UNITS);
        if let Some(ride) = self.ride {
            w.var(ride);
            w.fixed3(self.local.as_dvec3(), LOCAL_UNITS);
        }
        w.angle(self.yaw);
        w.angle(self.pitch);
        if bits & HEAD != 0 {
            w.angle(self.head[0]);
            w.angle(self.head[1]);
        }
        if bits & VEL != 0 {
            vel.iter().for_each(|v| w.zig(*v));
        }
        if let Some((thing, seat)) = self.seat {
            w.var(thing);
            w.u8(seat);
        }
        if self.tool != 0 {
            w.u8(self.tool);
        }
        w.u8((self.eye_h * 100.0).round().clamp(0.0, 255.0) as u8);
        if bits & PUSH != 0 {
            push.iter().for_each(|p| w.u8(*p as u8));
        }
        if let Some((thing, at)) = self.work {
            w.var(thing);
            w.fixed3(at.as_dvec3(), LOCAL_UNITS);
        }
        if self.gesture != 0 {
            w.u8(self.gesture);
        }
    }

    pub fn decode(r: &mut Reader) -> Wire<PlayerState> {
        let bits = r.var16()?;
        if bits & !ALL != 0 {
            return Err(WireError::Value);
        }
        let mut s = PlayerState { body: if bits & BODY != 0 { r.u8()? } else { 0 }, flags: r.var16()?, pos: r.fixed3(POS_UNITS)?, ..PlayerState::default() };
        if bits & RIDE != 0 {
            s.ride = Some(r.var()?);
            s.local = r.fixed3(LOCAL_UNITS)?.as_vec3();
        }
        s.yaw = r.angle()?;
        s.pitch = r.angle()?;
        if bits & HEAD != 0 {
            s.head = [r.angle()?, r.angle()?];
        }
        if bits & VEL != 0 {
            s.vel = r.fixed3(PLAYER_VEL_UNITS)?.as_vec3();
        }
        if bits & SEAT != 0 {
            s.seat = Some((r.var()?, r.u8()?));
        }
        if bits & TOOL != 0 {
            s.tool = r.u8()?;
            // (a tool is never 0 on the wire: that is what its bit says)
            if s.tool == 0 {
                return Err(WireError::Value);
            }
        }
        s.eye_h = r.u8()? as f32 / 100.0;
        if bits & PUSH != 0 {
            s.push = Vec3::new(r.u8()? as i8 as f32, r.u8()? as i8 as f32, r.u8()? as i8 as f32) / PUSH_UNITS;
        }
        if bits & WORK != 0 {
            s.work = Some((r.var()?, r.fixed3(LOCAL_UNITS)?.as_vec3()));
        }
        if bits & GESTURE != 0 {
            s.gesture = r.u8()?;
            if s.gesture == 0 {
                return Err(WireError::Value);
            }
        }
        Ok(s)
    }

    /// The state a fraction `t` (0..1) of the way from `a` to `b`: positions in a straight line,
    /// angles the short way round; what cannot be mixed (flags, seat, tool, ship) from the nearer.
    pub fn mix(a: &PlayerState, b: &PlayerState, t: f32) -> PlayerState {
        let t = t.clamp(0.0, 1.0);
        let near = if t < 0.5 { a } else { b };
        PlayerState {
            body: near.body,
            pos: if a.body == b.body { a.pos.lerp(b.pos, t as f64) } else { near.pos },
            ride: near.ride,
            // Two ships, two frames: there is no half way between them.
            local: if a.ride == b.ride { a.local.lerp(b.local, t) } else { near.local },
            yaw: mix_angle(a.yaw, b.yaw, t),
            pitch: mix_angle(a.pitch, b.pitch, t),
            head: [mix_angle(a.head[0], b.head[0], t), mix_angle(a.head[1], b.head[1], t)],
            vel: a.vel.lerp(b.vel, t),
            flags: near.flags,
            seat: near.seat,
            tool: near.tool,
            eye_h: a.eye_h + (b.eye_h - a.eye_h) * t,
            push: a.push.lerp(b.push, t),
            work: near.work,
            gesture: near.gesture,
        }
    }

    /// The state `dt` seconds later if nothing changes: the eye goes on at its velocity.
    pub fn carry(&mut self, dt: f32) {
        self.pos += (self.vel * dt).as_dvec3();
    }
}
