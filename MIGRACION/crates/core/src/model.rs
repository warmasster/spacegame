//! Models as data (`models/<id>.jsonc`): a name, and a source some `ShapeBuilder` turns into meshes
//! per LOD with the on-screen size each LOD is drawn at. The renderer only ever sees the result.
use crate::{
    defs::{self, DefError},
    hull::HullDef,
    mesh::{Glow, Mesh},
};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

/// Baked skeletal animation: joint matrices (3 rows of 3x4 per joint per frame) and clips as
/// (first frame, frames).
pub struct Animation {
    pub joints: u32,
    pub frames: Vec<[f32; 4]>,
    pub clips: Vec<(u32, u32)>,
}

pub struct BuiltModel {
    /// Finest first, each with the smallest on-screen radius (px) it is drawn at.
    pub lods: Vec<(Mesh, f32)>,
    pub skinned: bool,
    pub glows: Vec<Glow>,
    pub animation: Option<Animation>,
}

/// Turns one kind of model source into meshes.
pub trait ShapeBuilder: Send + Sync {
    fn build(&self) -> Result<BuiltModel, String>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelDef {
    pub name: String,
    #[serde(flatten)]
    pub source: ModelSource,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelSource {
    /// Primitive parts plus greebles (`hull.rs`).
    Hull(HullDef),
    /// A glTF/GLB file, simplified into far LODs (built by the app, which reads glTF).
    Gltf(GltfDef),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GltfLod {
    pub min_px: f32,
    /// Triangle budget of a simplified LOD (none: the file's mesh as is).
    #[serde(default)]
    pub budget: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GltfDef {
    /// Relative to the assets folder.
    pub path: String,
    pub lods: Vec<GltfLod>,
    /// Materials left out of the simplified LODs (insides nobody sees from afar).
    #[serde(default)]
    pub hidden_far: Vec<String>,
    /// Albedo overrides by material name (sRGB).
    #[serde(default)]
    pub tints: HashMap<String, [u8; 3]>,
    /// Name of a procedural animation set the app knows how to bake.
    #[serde(default)]
    pub animation: Option<String>,
}

/// Every model definition by id.
pub struct ModelDefs {
    pub defs: Vec<(String, ModelDef)>,
}

impl ModelDefs {
    pub fn load(dir: &Path) -> Result<ModelDefs, DefError> {
        Ok(ModelDefs { defs: defs::load_dir(dir)? })
    }

    pub fn get(&self, id: &str) -> Option<&ModelDef> {
        self.defs.iter().find(|(k, _)| k == id).map(|(_, d)| d)
    }
}
