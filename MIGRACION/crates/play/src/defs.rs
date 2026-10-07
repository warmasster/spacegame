//! What the game is made of, from `assets/defs`: the star system, the scenario, the structures'
//! catalog and blueprints, the ships, the effects, the missiles and the font their panels are
//! lettered with. The same for every machine that plays it (the server included): what only
//! draws (models' meshes, textures) is loaded by whoever draws.
use lunar_core::{
    defs::{self, DefError},
    effects::EffectDefs,
    font::Font,
    missiles::MissileDef,
    model::ModelDefs,
    palette::Palette,
    scenario::ScenarioDef,
    structure::Library,
    system::System,
};
use lunar_ship::{ShipKind, ShipLibrary};
use std::{path::Path, sync::Arc};

/// Every definition, loaded before anything is made of them.
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
    /// A few bytes of all of it (`fingerprint`): two machines play together only with the same.
    pub fingerprint: u32,
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
            fingerprint: fingerprint(dir),
        })
    }
}

/// A few bytes of every definition under `dir` (each file's path from there and what it says,
/// in the order of their paths, the ends of its lines as they are on any machine): what the
/// server and every player's game must agree on before they play (`lunar_net::ServerConfig::game`).
pub fn fingerprint(dir: &Path) -> u32 {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, base, out);
            } else if p.extension().is_some_and(|x| x == "jsonc" || x == "json") {
                let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push((rel, p));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    let mut h = 0x811c_9dc5u32;
    let mut eat = |b: u8| h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    for (rel, p) in files {
        rel.bytes().for_each(&mut eat);
        eat(0);
        if let Ok(bytes) = std::fs::read(&p) {
            bytes.into_iter().filter(|b| *b != b'\r').for_each(&mut eat);
        }
        eat(0);
    }
    h
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
