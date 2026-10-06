//! Quantised values on top of `wire`: what the game sends many times a second, in fewer bytes than
//! its floats and with a known error.
//! - angle: 16 bits a turn, error up to π/65536 rad (0.0027°);
//! - fixed point: `value × per_unit` rounded, as a zig-zag varint, error up to half a step; small
//!   values take one byte, and the range only ends where `i64` does;
//! - unit quaternion: "smallest three" in 48 bits (2 for which component was dropped, 15 for each
//!   of the others), error under 0.01° of rotation. 32 bits (10 per component) would err up to
//!   0.14°, which is 6 cm at the far end of a 50 m ship: too much for a state that is applied to
//!   a simulation, and the 2 bytes saved are 40 bytes a second per ship.
use crate::wire::{Reader, Wire, Writer};
use glam::{DVec3, Quat};
use std::f32::consts::FRAC_1_SQRT_2;

/// One step of a quantised angle, radians.
pub const ANGLE_STEP: f32 = std::f32::consts::TAU / 65536.0;
/// Steps of a quaternion component from 0 to 1/√2 (and as many to -1/√2): zero is one of them, so
/// the identity and the right angles travel exactly.
const QUAT_STEPS: f32 = 16383.0;

impl Writer<'_> {
    /// An angle in radians, wrapped to one turn, in 2 bytes.
    pub fn angle(&mut self, radians: f32) {
        let steps = (radians as f64 * (65536.0 / std::f64::consts::TAU)).round() as i64;
        self.u16(steps as u16);
    }
    /// `v` in steps of `1 / per_unit`, as a signed varint.
    pub fn fixed(&mut self, v: f64, per_unit: f64) {
        // `as` saturates and turns NaN into 0: no value can make this panic.
        self.zig((v * per_unit).round() as i64);
    }
    /// The three axes of a vector, each as `fixed`.
    pub fn fixed3(&mut self, v: DVec3, per_unit: f64) {
        self.fixed(v.x, per_unit);
        self.fixed(v.y, per_unit);
        self.fixed(v.z, per_unit);
    }
    /// A rotation in 6 bytes. Anything that is no rotation (zero, NaN) goes as the identity.
    pub fn quat(&mut self, q: Quat) {
        let mut c = q.to_array();
        let len = c.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !(len.is_finite() && len > 1e-6) {
            c = [0.0, 0.0, 0.0, 1.0];
        } else {
            c = c.map(|x| x / len);
        }
        let mut big = 0;
        for i in 1..4 {
            if c[i].abs() > c[big].abs() {
                big = i;
            }
        }
        // q and -q are the same rotation: send the one whose dropped component is positive.
        let sign = if c[big] < 0.0 { -1.0 } else { 1.0 };
        let mut bits = big as u64;
        for (i, x) in c.iter().enumerate() {
            if i != big {
                let n = ((x * sign / FRAC_1_SQRT_2 * QUAT_STEPS).round().clamp(-QUAT_STEPS, QUAT_STEPS) + QUAT_STEPS) as u64;
                bits = (bits << 15) | n;
            }
        }
        self.bytes(&bits.to_le_bytes()[..6]);
    }
}

impl Reader<'_> {
    /// An angle in [-π, π).
    pub fn angle(&mut self) -> Wire<f32> {
        Ok(self.u16()? as i16 as f32 * ANGLE_STEP)
    }
    pub fn fixed(&mut self, per_unit: f64) -> Wire<f64> {
        Ok(self.zig()? as f64 / per_unit)
    }
    pub fn fixed3(&mut self, per_unit: f64) -> Wire<DVec3> {
        Ok(DVec3::new(self.fixed(per_unit)?, self.fixed(per_unit)?, self.fixed(per_unit)?))
    }
    /// Any 6 bytes are a rotation: this only fails on short input.
    pub fn quat(&mut self) -> Wire<Quat> {
        let mut raw = [0u8; 8];
        raw[..6].copy_from_slice(self.bytes(6)?);
        let bits = u64::from_le_bytes(raw);
        let big = ((bits >> 45) & 3) as usize;
        let mut c = [0.0f32; 4];
        let mut sum = 0.0;
        let mut shift = 30;
        for (i, x) in c.iter_mut().enumerate() {
            if i != big {
                *x = (((bits >> shift) & 0x7fff) as f32 - QUAT_STEPS) / QUAT_STEPS * FRAC_1_SQRT_2;
                sum += *x * *x;
                shift -= 15;
            }
        }
        c[big] = (1.0 - sum).max(0.0).sqrt();
        Ok(Quat::from_array(c).normalize())
    }
}
