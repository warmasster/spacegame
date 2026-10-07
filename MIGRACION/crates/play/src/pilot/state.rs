//! The body's state as bytes and back, whole (`write_state` / `read_state`: what the server sends a
//! player whose game went astray, to be put as it is and stepped on from there), and in a few
//! bytes (`digest`: what a player's game says it got at each step, for the server to compare).
//!
//! The digest is of what holds where the body is: aboard, its place and speed in what carries it
//! (a copy of a ship a hair off the server's does not make the body differ); else, in the world.
//! Rounded to a tenth of a millimetre: differences in the last bits do not count, real ones do.
use super::{Hold, Pilot, Ride, Seat};
use glam::{DVec3, Quat, Vec3};

/// Why bytes could not be taken as a body's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Garbled;

struct Out<'a>(&'a mut Vec<u8>);

impl Out<'_> {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    fn dvec(&mut self, v: DVec3) {
        v.to_array().into_iter().for_each(|x| self.f64(x));
    }
    fn vec(&mut self, v: Vec3) {
        v.to_array().into_iter().for_each(|x| self.f32(x));
    }
    fn quat(&mut self, q: Quat) {
        q.to_array().into_iter().for_each(|x| self.f32(x));
    }
}

struct In<'a>(&'a [u8]);

impl In<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Garbled> {
        let (head, rest) = self.0.split_first_chunk::<N>().ok_or(Garbled)?;
        self.0 = rest;
        Ok(*head)
    }
    fn u8(&mut self) -> Result<u8, Garbled> {
        Ok(self.take::<1>()?[0])
    }
    fn u64(&mut self) -> Result<u64, Garbled> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    fn f64(&mut self) -> Result<f64, Garbled> {
        let v = f64::from_bits(self.u64()?);
        if v.is_finite() { Ok(v) } else { Err(Garbled) }
    }
    fn f32(&mut self) -> Result<f32, Garbled> {
        let v = f32::from_bits(u32::from_le_bytes(self.take()?));
        if v.is_finite() { Ok(v) } else { Err(Garbled) }
    }
    fn dvec(&mut self) -> Result<DVec3, Garbled> {
        Ok(DVec3::new(self.f64()?, self.f64()?, self.f64()?))
    }
    fn vec(&mut self) -> Result<Vec3, Garbled> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn quat(&mut self) -> Result<Quat, Garbled> {
        Ok(Quat::from_xyzw(self.f32()?, self.f32()?, self.f32()?, self.f32()?))
    }
}

impl Pilot {
    /// Everything the body is that a step changes, as it is, exactly.
    pub fn write_state(&self, out: &mut Vec<u8>) {
        let mut w = Out(out);
        w.dvec(self.position);
        w.f64(self.yaw);
        w.f64(self.pitch);
        let flags = u8::from(self.flying) | u8::from(self.grounded) << 1 | u8::from(self.free_look) << 2 | u8::from(self.lamps) << 3 | u8::from(self.pack_on) << 4 | u8::from(self.aloft) << 5 | u8::from(self.steady) << 6;
        w.u8(flags);
        w.f64(self.speed);
        w.u64(u64::from(self.body));
        w.u64(self.ground.map_or(0, |g| u64::from(g) + 1));
        w.dvec(self.up);
        w.dvec(self.fore);
        w.dvec(self.weight);
        w.f64(self.vertical_velocity);
        match self.ride {
            Some(r) => {
                w.u8(1);
                w.u64(r.id);
                w.vec(r.local);
                w.quat(r.rot);
            }
            None => w.u8(0),
        }
        match self.seat {
            Some(s) => {
                w.u8(1);
                w.u64(s.structure);
                w.u64(s.index as u64);
                w.vec(s.eyes);
                w.f32(s.heading);
                w.vec(s.exit);
            }
            None => w.u8(0),
        }
        w.dvec(self.wind);
        w.dvec(self.drift);
        w.dvec(self.vel);
        w.f64(self.eye_h);
        w.f64(self.fuel);
        w.f64(self.jet);
        w.dvec(self.push);
        w.dvec(self.walked);
        w.f64(self.air);
        w.u64(self.cabin.map_or(0, |c| c + 1));
        match self.hold {
            Hold::Still => w.u8(0),
            Hold::Free => w.u8(1),
            Hold::Speed(v) => {
                w.u8(2);
                w.dvec(v);
            }
            Hold::Beside { id, vel, gains } => {
                w.u8(3);
                w.u64(id);
                w.dvec(vel);
                w.dvec(gains);
            }
        }
        w.f64(self.sink);
        w.f64(self.dip);
        w.f64(self.dip_v);
        w.f64(self.landed);
        w.f64(self.stepped);
    }

    /// The body as `write_state` wrote it. Nothing is changed if the bytes are not a whole state.
    pub fn read_state(&mut self, data: &[u8]) -> Result<(), Garbled> {
        let mut r = In(data);
        let position = r.dvec()?;
        let (yaw, pitch) = (r.f64()?, r.f64()?);
        let flags = r.u8()?;
        let speed = r.f64()?;
        let body = u16::try_from(r.u64()?).map_err(|_| Garbled)?;
        let ground = match r.u64()? {
            0 => None,
            g => Some(u16::try_from(g - 1).map_err(|_| Garbled)?),
        };
        let (up, fore, weight) = (r.dvec()?, r.dvec()?, r.dvec()?);
        let vertical_velocity = r.f64()?;
        let ride = match r.u8()? {
            0 => None,
            1 => Some(Ride { id: r.u64()?, local: r.vec()?, rot: r.quat()? }),
            _ => return Err(Garbled),
        };
        let seat = match r.u8()? {
            0 => None,
            1 => Some(Seat { structure: r.u64()?, index: usize::try_from(r.u64()?).map_err(|_| Garbled)?, eyes: r.vec()?, heading: r.f32()?, exit: r.vec()? }),
            _ => return Err(Garbled),
        };
        let (wind, drift, vel) = (r.dvec()?, r.dvec()?, r.dvec()?);
        let (eye_h, fuel, jet) = (r.f64()?, r.f64()?, r.f64()?);
        let (push, walked) = (r.dvec()?, r.dvec()?);
        let air = r.f64()?;
        let cabin = match r.u64()? {
            0 => None,
            c => Some(c - 1),
        };
        let hold = match r.u8()? {
            0 => Hold::Still,
            1 => Hold::Free,
            2 => Hold::Speed(r.dvec()?),
            3 => Hold::Beside { id: r.u64()?, vel: r.dvec()?, gains: r.dvec()? },
            _ => return Err(Garbled),
        };
        let (sink, dip, dip_v, landed, stepped) = (r.f64()?, r.f64()?, r.f64()?, r.f64()?, r.f64()?);
        if !r.0.is_empty() || flags & 0x80 != 0 {
            return Err(Garbled);
        }
        let bit = |k: u8| flags & (1 << k) != 0;
        (self.position, self.yaw, self.pitch, self.speed, self.body, self.ground) = (position, yaw, pitch, speed, body, ground);
        (self.flying, self.grounded, self.free_look, self.lamps, self.pack_on, self.aloft, self.steady) = (bit(0), bit(1), bit(2), bit(3), bit(4), bit(5), bit(6));
        (self.up, self.fore, self.weight, self.vertical_velocity, self.ride, self.seat) = (up, fore, weight, vertical_velocity, ride, seat);
        (self.wind, self.drift, self.vel, self.eye_h, self.fuel, self.jet) = (wind, drift, vel, eye_h, fuel, jet);
        (self.push, self.walked, self.air, self.cabin, self.hold) = (push, walked, air, cabin, hold);
        (self.sink, self.dip, self.dip_v, self.landed, self.stepped) = (sink, dip, dip_v, landed, stepped);
        Ok(())
    }

    /// A few bytes of where the body is and how it goes, for two games to compare at one step:
    /// aboard, in what carries it; else in the world; to a tenth of a millimetre (and of a
    /// millimetre a second).
    pub fn digest(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |v: u64| {
            for b in v.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        let q = |x: f64| (x * 1e4).round() as i64 as u64;
        match self.ride {
            Some(r) => {
                eat(r.id + 1);
                r.local.to_array().into_iter().for_each(|x| eat(q(f64::from(x))));
            }
            None => {
                eat(0);
                self.position.to_array().into_iter().for_each(|x| eat(q(x)));
            }
        }
        self.vel.to_array().into_iter().for_each(|x| eat(q(x)));
        eat(u64::from(self.grounded) | u64::from(self.pack_on) << 1 | u64::from(self.flying) << 2 | u64::from(self.seat.is_some()) << 3);
        eat(q(self.fuel));
        h
    }
}
