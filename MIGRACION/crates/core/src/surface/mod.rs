//! The ground of a body: a small trait any kind of surface implements, and the surface kinds that
//! can be declared in data (`"surface": { "kind": "procedural", ... }`).
mod procedural;

pub use procedural::{CraterLayer, MAX_CRATER_LAYERS, MAX_MOUNTAIN_OCTAVES, MAX_RELIEF_OCTAVES, Octaves, Procedural, ProceduralDef};

use crate::cube_sphere::V3;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// Over the body's datum sphere (m).
    pub height: f64,
    pub albedo: f64,
}

pub trait Surface: Send + Sync {
    /// Height and albedo along the unit direction `dir`; `min_feature` (m) drops smaller detail.
    fn sample(&self, dir: V3, min_feature: f64) -> Sample;
    /// The collision query: full-detail height.
    fn height(&self, dir: V3) -> f64 {
        self.sample(dir, 0.0).height
    }
    /// The GPU terrain generator's parameters, for surfaces that have a GPU twin.
    fn procedural(&self) -> Option<&Procedural> {
        None
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceDef {
    Procedural(ProceduralDef),
}

impl SurfaceDef {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            SurfaceDef::Procedural(d) => d.validate(),
        }
    }

    /// `level`: direction where the ground is moved to height 0.
    pub fn build(&self, radius: f64, seed: u32, level: Option<V3>) -> Arc<dyn Surface> {
        match self {
            SurfaceDef::Procedural(d) => Arc::new(Procedural::new(d.clone(), radius, seed, level)),
        }
    }
}
