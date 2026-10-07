#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! LUNA: the Rust migration prototype. Window, input, camera, loop, benchmark.
mod aboard;
mod air;
mod bench;
mod blasts;
mod boot;
mod bulk;
mod body;
mod builds;
mod chase;
mod cli;
mod cockpit;
mod content;
mod dust;
mod editor;
mod footprints;
mod freelook;
mod gear;
mod gestures;
mod gltf_model;
mod hands;
mod handwork;
mod holding;
mod hud;
mod input;
mod inspector;
mod multi;
mod nav;
mod perf;
mod pilot;
mod play;
mod plumes;
mod rangefinder;
mod reach;
mod rig;
mod script;
mod ships;
mod sound;
mod spawner;
mod splash;
mod splash_gpu;
mod start;
mod tactics;
mod ui;
mod visibility;
mod visor;
mod world;
mod wrist;

use std::path::{Path, PathBuf};

/// What the game is called, as the player reads it (the executables handed over are named after
/// it: `SeleneV<N>_<edition>.exe`).
pub const GAME: &str = "SELENE";

/// What this build is called (the executable handed over carries it in its name).
pub const BUILD: &str = "V39";

/// The multiplayer build (`--features multijugador`): the demo's game with other players in it.
/// (The debug build can join a server too, to try things out; the plain demo cannot.)
pub const MULTI: bool = cfg!(feature = "multijugador");

/// What this edition is called.
pub const EDITION: &str = if MULTI {
    "MULTIJUGADOR"
} else if DEMO {
    "DEMO"
} else {
    "DESARROLLO"
};

/// The demo build (`--features demo`): no inspector, the catalog has ships only.
pub const DEMO: bool = cfg!(feature = "demo");

/// Where assets and outputs live: next to the executable, else the workspace root.
pub fn root() -> PathBuf {
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for base in exe.iter().chain(std::iter::once(&dev)) {
        if base.join("assets/defs/system.jsonc").exists() {
            return base.clone();
        }
    }
    dev
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let dir = root().join("out");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("panic.log"), info.to_string());
        eprintln!("{info}");
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = cli::Options::parse(&args).map_err(|e| e.to_string()).and_then(|o| {
        if o.sound_test {
            // the card opened and its thread running for a moment, with nothing to play
            let sounds = sound::Sounds::new(&root(), false)?;
            std::thread::sleep(std::time::Duration::from_millis(400));
            println!("sonido: {}", sounds.status);
            return Ok(());
        }
        play::run(o).map_err(|e| e.to_string())
    });
    if let Err(e) = result {
        let log = root().join("out/error.log");
        let _ = std::fs::create_dir_all(root().join("out"));
        let _ = std::fs::write(&log, &e);
        eprintln!("{e}");
        std::process::exit(1);
    }
}
