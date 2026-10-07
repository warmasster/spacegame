//! Content from `assets/defs`: the star system, the palette, every model and the scenario. Models
//! are built by the `ShapeBuilder` their source kind names (in parallel) and registered in the
//! renderer, which only ever sees meshes, LODs and glows.
use crate::gltf_model::GltfModel;
use lunar_core::{
    defs::{self, DefError},
    effects::EffectDefs,
    font::Font,
    missiles::MissileDef,
    structure::Library,
    model::{BuiltModel, ModelDefs, ModelSource, ShapeBuilder},
    palette::Palette,
    scenario::ScenarioDef,
    system::System,
};
use lunar_render::{Renderer, scene::Family};
use lunar_ship::{ShipKind, ShipLibrary};
use rayon::prelude::*;
use std::{
    sync::Arc,
    collections::HashMap,
    error::Error,
    path::{Path, PathBuf},
};

/// Every definition, loaded before the renderer exists (it needs the system's bodies).
pub struct Defs {
    pub system: System,
    pub scenario: ScenarioDef,
    pub palette: Palette,
    pub models: ModelDefs,
    /// Particle styles and explosions.
    pub effects: EffectDefs,
    /// Structure catalog and blueprints.
    pub structures: Arc<Library>,
    /// Long-range missiles (`missiles.jsonc`).
    pub missiles: Vec<(String, MissileDef)>,
    /// Ship kinds (`ships/`, `components/`, `panels/`): their parts are in the structure catalog
    /// and their blueprints among the structures'.
    pub ships: Vec<Arc<ShipKind>>,
    /// The silkscreen font (`assets/fonts/serigrafia`): metrics and its R8 atlas.
    pub font: (Font, Vec<u8>),
    /// The structures' finishes (`assets/textures/acabados`): tile size, layers, RGBA8 data.
    pub finishes: (u32, u32, Vec<u8>),
    /// The decal atlas (`assets/textures/calcas`): its names and its RGBA8 pixels.
    pub decals: (lunar_core::props::DecalAtlas, Vec<u8>),
}

impl Defs {
    pub fn load(dir: &Path) -> Result<Defs, DefError> {
        let mut structures = Library::load(&dir.join("structures"))?;
        // a ship being edited (the MCP server's photos): LUNA_NAVE_EDITADA=id=file.jsonc
        let mut instead = Vec::new();
        if let Ok(v) = std::env::var("LUNA_NAVE_EDITADA")
            && let Some((id, path)) = v.split_once('=')
        {
            instead.push((id.to_string(), defs::load(Path::new(path))?));
        }
        let (ships, blueprints) = ShipLibrary::load_with(dir, &mut structures.catalog, &instead)?;
        structures.blueprints.extend(blueprints);
        Ok(Defs {
            system: System::load(dir)?,
            scenario: defs::load(&defs::file(dir, "scenario"))?,
            palette: defs::load(&defs::file(dir, "palette")).map(Palette::new)?,
            models: ModelDefs::load(&dir.join("models"))?,
            effects: EffectDefs::load(dir)?,
            structures: Arc::new(structures),
            missiles: defs::load::<std::collections::BTreeMap<String, MissileDef>>(&defs::file(dir, "missiles"))?.into_iter().collect(),
            ships: ships.kinds,
            font: load_font(&dir.join("../fonts/serigrafia"))?,
            finishes: load_finishes(&dir.join("../textures/acabados"))?,
            decals: load_decals(&dir.join("../textures/calcas"))?,
        })
    }
}

/// A signed-distance font: `<path>.json` metrics and `<path>.bin` atlas (R8, width × height).
fn load_font(path: &Path) -> Result<(Font, Vec<u8>), DefError> {
    let json = path.with_extension("json");
    let fail = |e: String| DefError::new(json.display().to_string(), e);
    let font = Font::parse(&std::fs::read_to_string(&json).map_err(|e| fail(e.to_string()))?).map_err(fail)?;
    let atlas = std::fs::read(path.with_extension("bin")).map_err(|e| fail(e.to_string()))?;
    if atlas.len() != (font.width * font.height) as usize {
        return Err(fail(format!("atlas de {} bytes, se esperaban {}×{}", atlas.len(), font.width, font.height)));
    }
    Ok((font, atlas))
}

/// The finishes' layers (`tools/texturas/acabados.py`): `<path>.json` says their size and names,
/// which must be `mesh::FINISHES` from the second on; `<path>.bin` holds them.
fn load_finishes(path: &Path) -> Result<(u32, u32, Vec<u8>), DefError> {
    #[derive(serde::Deserialize)]
    struct Meta {
        size: u32,
        capas: Vec<String>,
    }
    let json = path.with_extension("json");
    let fail = |e: String| DefError::new(json.display().to_string(), e);
    let meta: Meta = serde_json::from_str(&std::fs::read_to_string(&json).map_err(|e| fail(e.to_string()))?).map_err(|e| fail(e.to_string()))?;
    let want = &lunar_core::mesh::FINISHES[1..];
    if meta.capas != want {
        return Err(fail(format!("las capas no son las de mesh::FINISHES ({}): regenera con tools/texturas/acabados.py", want.join(", "))));
    }
    let data = std::fs::read(path.with_extension("bin")).map_err(|e| fail(e.to_string()))?;
    let n = meta.capas.len() as u32;
    if data.len() != (meta.size * meta.size * 4 * n) as usize {
        return Err(fail(format!("{} bytes, se esperaban {} capas de {}²", data.len(), n, meta.size)));
    }
    Ok((meta.size, n, data))
}

/// The decal atlas: `<path>.json` (names, rectangles) and `<path>.bin` (RGBA8, size²).
fn load_decals(path: &Path) -> Result<(lunar_core::props::DecalAtlas, Vec<u8>), DefError> {
    let json = path.with_extension("json");
    let fail = |e: String| DefError::new(json.display().to_string(), e);
    let atlas = lunar_core::props::DecalAtlas::parse(&std::fs::read_to_string(&json).map_err(|e| fail(e.to_string()))?).map_err(fail)?;
    let data = std::fs::read(path.with_extension("bin")).map_err(|e| fail(e.to_string()))?;
    if data.len() != (atlas.size * atlas.size * 4) as usize {
        return Err(fail(format!("{} bytes, se esperaban {}² RGBA", data.len(), atlas.size)));
    }
    Ok((atlas, data))
}

/// Renderer model ids by definition id.
pub struct Models {
    ids: HashMap<String, u32>,
}

impl Models {
    /// Build and register every model definition.
    pub fn register(r: &mut Renderer, defs: &Defs, assets: &Path) -> Result<Models, Box<dyn Error>> {
        let built: Vec<(&str, &str, BuiltModel)> = defs
            .models
            .defs
            .par_iter()
            .map(|(id, def)| {
                let fail = |e: String| format!("model {id}: {e}");
                let builder: Box<dyn ShapeBuilder> = match &def.source {
                    ModelSource::Hull(h) => Box::new(h.resolve(&defs.palette).map_err(fail)?),
                    ModelSource::Gltf(g) => Box::new(GltfModel { def: g, path: find_asset(assets, &g.path) }),
                };
                Ok((id.as_str(), def.name.as_str(), builder.build().map_err(fail)?))
            })
            .collect::<Result<_, String>>()?;
        let mut ids = HashMap::new();
        for (id, name, m) in built {
            let family = if m.skinned { Family::Skinned } else { Family::Rigid };
            let lods: Vec<_> = m.lods.iter().map(|(mesh, px)| (mesh, family, *px)).collect();
            let scene = r.scene();
            let model = scene.add_model(name, &lods);
            if !m.glows.is_empty() {
                scene.add_glows(model, &m.glows);
            }
            if let Some(a) = &m.animation {
                r.set_animation(a);
            }
            ids.insert(id.to_string(), model);
        }
        Ok(Models { ids })
    }

    pub fn get(&self, id: &str) -> Result<u32, String> {
        self.ids.get(id).copied().ok_or_else(|| format!("unknown model '{id}'"))
    }
}

/// An asset path relative to `assets/`, or to the reference sources the migration came from (a
/// build's folder carries only `assets/`: whatever it needs is copied there, as `astronaut.glb`).
pub fn find_asset(assets: &Path, name: &str) -> PathBuf {
    let local = assets.join(name);
    if local.exists() { local } else { assets.join("../reference/sources/public/assets").join(name) }
}
