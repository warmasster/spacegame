//! Modular structures: ships, stations, bases, buildings, vehicles, all one thing. Parts of a
//! catalog (convex shapes in materials, with network ports and maybe a machine) held together by
//! joints; parts and joints take damage, parts crack, lose chips and shatter into pieces with their
//! own shape and mass, and whatever no longer holds to the rest flies off as a structure of its own.
//!
//! Each piece is behind a contract and can be swapped without touching the others:
//! - data: `catalog` (materials, part and joint kinds) and `blueprint` (a structure), all `.jsonc`;
//! - `convex`: the geometry every part and piece is;
//! - `graph`: what still holds together;
//! - `state`: the one place structure state lives;
//! - `look::Mesher`: state → triangles for any renderer;
//! - `models`: looks made elsewhere (Blender), a file dropped in per thing;
//! - `labels`: what is written on parts, laid out for whoever draws text;
//! - `damage::DamageModel`: who takes how much of a hit;
//! - `fracture::Fracture`: what a part breaks into (per material);
//! - `breakup`: hits applied: cracks, chips, shards, joints giving, pieces flying off;
//! - `physics`: loose structures as rigid bodies against the ground and each other;
//! - `obstruct`: what stands in the way of a moving part (so a mechanism stops where it touches);
//! - `hold`: a structure held by another (a clamp, a magnet, a cradle): it follows, it weighs;
//! - `contents`: what parts hold (water, propellant, regolith...), from a registry of substances:
//!   it weighs where it lies, and it can be taken out and put in;
//! - `weight`: what something carried by a structure (or by nothing) weighs, and which way;
//! - `schedule::LodPolicy`: how much simulation each one gets (active, coarse, dormant);
//! - `broadphase::Grid`: which structures a path may touch;
//! - `networks`: energy, propellant... flowing over the joints (`Machine` per module kind);
//! - `set::Structures`: the structures in the world, built and placed from data.
pub mod blueprint;
pub mod breakup;
pub mod broadphase;
pub mod bvh;
pub mod catalog;
pub mod contents;
pub mod convex;
pub mod damage;
pub mod fracture;
pub mod graph;
pub mod hold;
pub mod labels;
pub mod look;
pub mod models;
pub mod networks;
pub mod obstruct;
pub mod physics;
pub mod schedule;
pub mod set;
pub mod state;
pub mod weight;

use crate::defs::DefError;
use blueprint::Blueprint;
use catalog::Catalog;
use std::path::Path;

/// Everything under `structures/`: the catalog and every blueprint (`builds/*.jsonc`).
#[derive(Clone)]
pub struct Library {
    pub catalog: Catalog,
    pub blueprints: Vec<(String, Blueprint)>,
}

impl Library {
    pub fn load(dir: &Path) -> Result<Library, DefError> {
        let catalog = Catalog::load(dir)?;
        networks::Machines::new(&catalog).map_err(|e| DefError::new("structures", e))?;
        let blueprints = Blueprint::load(&dir.join("builds"), &catalog)?;
        Ok(Library { catalog, blueprints })
    }

    pub fn blueprint(&self, id: &str) -> Option<&Blueprint> {
        self.blueprints.iter().find(|(b, _)| b == id).map(|(_, b)| b)
    }

    /// The blueprint structures named `name` are built from (`Structure::name`).
    pub fn blueprint_named(&self, name: &str) -> Option<&Blueprint> {
        self.blueprints.iter().find(|(_, b)| b.name == name).map(|(_, b)| b)
    }
}
