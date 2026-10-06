//! Ships: structures with systems, all from data.
//!
//! - `def`: the ship file (`assets/defs/ships/<id>.jsonc`);
//! - `geom`: generators of hull panels, decks, bulkheads and conduit segments, each a part with a
//!   detailed look;
//! - `components`: the component catalog (`assets/defs/components`) and how components become parts;
//! - `contents`: what a component holds (`"contenido"`: a substance of the registry, how much);
//! - `mass`: what the ship weighs with it all, as signals (mass, propellant, Δv, thrust to weight);
//! - `cargo`: cargo and the clamps, cradles and magnets that hold it;
//! - `kind`: a kind assembled once (parts registered in the structure catalog, networks laid out with
//!   a stub conduit to every device, machines, actuators, joints, panels, compartments, seats);
//! - `ship`: one ship at work, tick by tick, on its structure;
//! - `panels`, `atmos`, `flight`, `blackbox`: its controls, its air, its flight computer, its log;
//! - `exhaust`: its thrusters' nozzles and what each gives now, for whoever shows their plumes;
//! - `scene`: what it shows besides its structure (controls, silkscreen, screens, lights, rams).
//!
//! The same code runs a ship on a server or a client: its state is signals, control states,
//! machine and actuator states and joint positions, all plain numbers (`Ship::snapshot`).
pub mod airworks;
pub mod atmos;
pub mod autopilot;
pub mod blackbox;
pub mod cargo;
pub mod closures;
pub mod components;
pub mod conduits;
pub mod contents;
pub mod def;
pub mod diag;
pub mod exhaust;
pub mod far;
pub mod flight;
pub mod geom;
pub mod kind;
pub mod mass;
pub mod mfd_auto;
pub mod panels;
pub mod plots;
pub mod power;
pub mod procedures;
pub mod rooms;
pub mod scene;
pub mod sections;
pub mod ship;
pub mod sync;
pub mod tactical;
pub mod transfer;

use lunar_core::{
    defs::{self, DefError},
    structure::{blueprint::Blueprint, catalog::Catalog},
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

pub use kind::ShipKind;
pub use ship::{Burst, Pace, Ship, World};

/// Every ship kind, assembled.
pub struct ShipLibrary {
    pub kinds: Vec<Arc<ShipKind>>,
}

/// What ships are made from (`assets/defs`): the component catalog, the panel templates and the
/// decal atlas' names. Kept to build (or rebuild) one ship from its definition: the editor's way.
pub struct Sources {
    pub components: components::Components,
    /// The panels' definitions (their text, with the sections their groups name already in).
    pub panels: BTreeMap<String, String>,
    /// The sections any panel can carry (`secciones.jsonc`).
    pub sections: sections::Sections,
    pub decals: lunar_core::props::DecalAtlas,
}

impl Sources {
    pub fn load(dir: &Path) -> Result<Sources, DefError> {
        let components: components::Components = load_maps(&dir.join("components"))?;
        let path = defs::file(dir, "secciones");
        let sections: sections::Sections = if path.exists() { defs::load(&path)? } else { sections::Sections::new() };
        let mut panels = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(dir.join("panels")) {
            let mut paths: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "jsonc")).collect();
            paths.sort();
            for p in paths {
                let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                let text = std::fs::read_to_string(&p).map_err(|e| DefError::new(p.display().to_string(), e.to_string()))?;
                let text = sections::expand(&id, &text, &sections).map_err(|e| DefError::new(p.display().to_string(), e))?;
                panels.insert(id, text);
            }
        }
        // the decal atlas' names (the atlas itself is the renderer's)
        let atlas_path = dir.join("../textures/calcas.json");
        let decals = match std::fs::read_to_string(&atlas_path) {
            Ok(t) => lunar_core::props::DecalAtlas::parse(&t).map_err(|e| DefError::new(atlas_path.display().to_string(), e))?,
            Err(_) => lunar_core::props::DecalAtlas::default(),
        };
        Ok(Sources { components, panels, sections, decals })
    }

    /// Ship `id` from its definition (filled in with what its compartments get, `rooms`), its
    /// parts registered in `catalog`; checked by building one.
    pub fn build(&self, id: &str, mut def: def::ShipDef, catalog: &mut Catalog) -> Result<(ShipKind, Blueprint), String> {
        let made = rooms::expand(&mut def)?;
        let mut all = self.panels.clone();
        all.extend(made);
        // (and what its data says of the air it moves on purpose: hand valves, compressors)
        all.extend(airworks::expand(&mut def, &self.components)?);
        // (the panels the ship makes for itself may carry sections too)
        for (id, text) in &mut all {
            *text = sections::expand(id, text, &self.sections)?;
        }
        let (k, bp) = kind::build(id, def, kind::Inputs { catalog, components: &self.components, panels: &all, decals: &self.decals })?;
        let k = Arc::new(k);
        Ship::new(k.clone(), 0, 1)?;
        let k = Arc::try_unwrap(k).map_err(|_| "la nave de prueba sigue viva".to_string())?;
        Ok((k, bp))
    }
}

impl ShipLibrary {
    /// Components, panels and ships under `dir` (`assets/defs`), their parts registered in
    /// `catalog`; the blueprints to add to the structure library. `instead`: definitions to use
    /// in place of the files of some ships (id, definition): an edited ship.
    pub fn load(dir: &Path, catalog: &mut Catalog) -> Result<(ShipLibrary, Vec<(String, Blueprint)>), DefError> {
        Self::load_with(dir, catalog, &[])
    }

    pub fn load_with(dir: &Path, catalog: &mut Catalog, instead: &[(String, def::ShipDef)]) -> Result<(ShipLibrary, Vec<(String, Blueprint)>), DefError> {
        let src = Sources::load(dir)?;
        let mut ships: Vec<(String, def::ShipDef)> = if dir.join("ships").exists() { defs::load_dir(&dir.join("ships"))? } else { Vec::new() };
        for (id, d) in instead {
            match ships.iter_mut().find(|(s, _)| s == id) {
                Some(s) => s.1 = d.clone(),
                None => ships.push((id.clone(), d.clone())),
            }
        }
        let mut kinds = Vec::new();
        let mut blueprints = Vec::new();
        for (id, d) in ships {
            let (k, bp) = src.build(&id, d, catalog).map_err(|e| DefError::new(format!("ships/{id}"), e))?;
            kinds.push(Arc::new(k));
            blueprints.push((id, bp));
        }
        Ok((ShipLibrary { kinds }, blueprints))
    }

    pub fn get(&self, id: &str) -> Option<&Arc<ShipKind>> {
        self.kinds.iter().find(|k| k.id == id)
    }
}

/// Every `*.jsonc` of a directory, each a map of ids, merged.
fn load_maps<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<BTreeMap<String, T>, DefError> {
    let mut out = BTreeMap::new();
    if !dir.exists() {
        return Ok(out);
    }
    for (_, m) in defs::load_dir::<BTreeMap<String, T>>(dir)? {
        out.extend(m);
    }
    Ok(out)
}
