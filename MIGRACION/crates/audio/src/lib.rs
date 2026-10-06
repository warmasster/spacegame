//! Sound made, not played back.
//!
//! - [`def`]: a sound as data — a few layers, each an oscillator or a noise through a filter with
//!   an envelope; a blow (it lasts so long) or a loop (it is held at a level).
//! - [`synth`]: those recipes compiled for a sample rate, and a voice that renders one.
//! - [`mixer`]: a fixed pool of voices fed by a wait-free queue; nothing is allocated nor locked
//!   while it renders (it runs on the sound card's thread).
//! - [`medium`]: what is heard goes by what it comes through — the air round the head (nothing in
//!   a vacuum), what the body touches (dull), the suit itself.
//! - `device` (feature `device`): the sound card.
//!
//! The game gives it sounds to start (`Cmd::Play`) and levels to hold (`Cmd::Loop`); it knows
//! nothing of ships or players.
pub mod def;
#[cfg(feature = "device")]
pub mod device;
pub mod medium;
pub mod mixer;
pub mod synth;

pub use def::{SoundDef, SoundDefs, Via};
pub use mixer::{Cmd, LOOPS, Mixer, VOICES};
pub use synth::Sound;
