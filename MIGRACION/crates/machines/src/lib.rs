//! Machines and the networks that feed them, for any structure. Physical models (batteries,
//! generators, reactors, rockets, pumps, tanks, radiators, fans, scrubbers...), actuators that
//! move joints (electric, hydraulic, pneumatic, manual, spring, explosive, gravity, solenoid;
//! through gears, screws, rods or winches; with locks and sensors), and one solver for every
//! medium (power, hydraulics, gas, propellant, air, coolant, heat, data). Plus where each piece
//! comes from and how it wears.
//!
//! Everything reads and writes signals (`lunar-signals`); nothing here knows a structure, a
//! renderer or an input device. The owner wires ports to networks, parts to health, joints to
//! geometry.
pub mod actuator;
pub mod gas;
pub mod machine;
pub mod models;
pub mod net;
pub mod wear;

pub use machine::{Build, Cx, Env, Machine, PortSpec, Treat};
pub use net::{Edge, EdgeKind, Medium, Net, PortIo, Solver};
