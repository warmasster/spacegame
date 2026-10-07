//! The game without its picture: what decides what happens, for any number of players, and the
//! same wherever it runs — the server, which has the truth, and each player's game, which
//! predicts its own with it and draws everything. Nothing here draws, sounds or reads a key:
//! whoever plays hands it what each player asks for, and reads back what happened.
pub mod air;
pub mod blasts;
pub mod builds;
pub mod controls;
pub mod defs;
pub mod follow;
pub mod game;
pub mod hands;
pub mod host;
pub mod interest;
pub mod net;
pub mod online;
pub mod pilot;
pub mod seats;
pub mod ships;
pub mod tactics;
pub mod told;

use std::path::{Path, PathBuf};

/// The workspace's root (where `assets/` is) for the tests and tools of this crate; a game or a
/// server finds its own next to its executable.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
