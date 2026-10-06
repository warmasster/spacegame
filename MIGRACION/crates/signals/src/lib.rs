//! Signals with physical units: the nervous system of every structure. Controls write commands,
//! machines read them and write telemetry, derived expressions combine them, indicators show them.
//! - `units`: SI inside, quantities with units in the data (`"45 kN"`), shown in any unit;
//! - `store`: one store per structure, every signal by index, one writer each;
//! - `expr`: a tiny expression language compiled to bytecode (with `lag`, `latch`, `blink`...);
//! - `derived`: derived signals evaluated in dependency order, only what changed.
pub mod derived;
pub mod expr;
pub mod qty;
pub mod store;
pub mod units;

pub use derived::DerivedSet;
pub use qty::Q;
pub use expr::{Eval, Program, compile, compile_in};
pub use store::{Quality, SignalId, Store, Writer};
pub use units::{Unit, parse, si};
