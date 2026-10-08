//! What the game says over the network many times a second, as plain data: a player, a rigid
//! thing (a ship, a crate, anything loose). Each knows its compact encoding (`encode` / `decode`)
//! and how to be mixed between two moments.
mod player;
mod rigid;

pub use player::{LOCAL_UNITS, PLAYER_VEL_UNITS, PUSH_UNITS, PlayerState};
pub use rigid::{Frame, JOINT_UNITS, MAX_JOINTS, RIGID_VEL_UNITS, RigidState, SPIN_UNITS};

/// World positions travel in steps of 1/4096 m (0.24 mm, so the error is at most 0.12 mm) as
/// signed varints: 5 bytes per axis up to 4 194 km from the origin (the Moon and its low orbits),
/// 6 up to 536 000 km (past the Earth), 8 across the solar system. Never as `f32`: at the Moon's
/// radius an `f32` only tells apart positions 12 cm from each other.
pub const POS_UNITS: f64 = 4096.0;

/// The bits of `PlayerState::flags`.
pub mod flag {
    /// Feet on something.
    pub const GROUNDED: u16 = 1 << 0;
    pub const CROUCHED: u16 = 1 << 1;
    /// The manoeuvring pack is on.
    pub const PACK: u16 = 1 << 2;
    /// The pack (or the suit) is thrusting.
    pub const THRUSTING: u16 = 1 << 3;
    /// The helmet lamp is lit.
    pub const LAMP: u16 = 1 << 4;
    /// The trigger of the tool in hand is held.
    pub const TRIGGER: u16 = 1 << 5;
    /// The player looks at themself from outside.
    pub const THIRD_PERSON: u16 = 1 << 6;
    /// Seated at the flight controls of the ship in `seat` (a passenger's seat does not set it).
    pub const AT_CONTROLS: u16 = 1 << 7;
    /// The tool in hand has what it fires in (a launcher loaded: its lamp green).
    pub const LOADED: u16 = 1 << 8;
    /// The wrist computer is raised to be looked at.
    pub const WRIST: u16 = 1 << 9;
    /// The helmet's sun visor is down.
    pub const VISOR: u16 = 1 << 10;
    /// The second use of the tool in hand is on (the right button's: a view, an aim).
    pub const SECOND: u16 = 1 << 11;
    /// `ride` and `local` say where the player is beside a thing, not aboard it (floating by a
    /// ship that flies): placed by it as it is where they are drawn, however fast both go, and
    /// going on their own.
    pub const BESIDE: u16 = 1 << 12;
}

/// The shortest way from angle `a` to angle `b`, a fraction `t` of it; the result in [-π, π).
pub fn mix_angle(a: f32, b: f32, t: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (b - a + PI).rem_euclid(TAU) - PI;
    (a + d * t + PI).rem_euclid(TAU) - PI
}
