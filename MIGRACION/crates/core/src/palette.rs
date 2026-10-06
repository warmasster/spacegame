//! Named materials (`palette.jsonc`): content refers to materials by name, never by value.
use crate::{
    defs::DefError,
    mesh::{Material, panel_code},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialDef {
    /// sRGB albedo.
    pub rgb: [u8; 3],
    pub rough: u8,
    #[serde(default)]
    pub metal: u8,
    #[serde(default)]
    pub emissive: u8,
    /// Hull plating drawn by the shader: plate size (m), 0 = smooth.
    #[serde(default)]
    pub panel: f32,
    /// The plates of an emissive material are windows, some lit, some dark.
    #[serde(default)]
    pub windows: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Palette {
    materials: HashMap<String, Material>,
}

impl Palette {
    pub fn new(defs: HashMap<String, MaterialDef>) -> Palette {
        let materials = defs.into_iter().map(|(k, d)| (k, Material { albedo: d.rgb, rough: d.rough, metal: d.metal, emissive: d.emissive, panel: panel_code(d.panel, d.windows), finish: 0 })).collect();
        Palette { materials }
    }

    pub fn parse(name: &str, text: &str) -> Result<Palette, DefError> {
        crate::defs::parse(name, text).map(Palette::new)
    }

    pub fn get(&self, name: &str) -> Result<Material, String> {
        self.materials.get(name).copied().ok_or_else(|| format!("unknown material '{name}'"))
    }
}
