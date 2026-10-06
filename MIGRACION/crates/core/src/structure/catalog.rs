//! What structures are built from, as data under `structures/`: materials (`materials.jsonc`),
//! joint kinds (`joints.jsonc`) and part kinds (`parts/*.jsonc`, each file a map of ids). A part
//! kind is a convex shape in a material, the networks it plugs into and, optionally, the machine
//! it is. Hit points and mass come from the material and the volume, never typed by hand.
use super::convex::Convex;
use crate::defs::{self, DefError};
use glam::Vec3;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// Networks a catalog can name (one bit each).
pub const MAX_NETWORKS: usize = 32;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialDef {
    pub name: String,
    /// kg/m³.
    pub density: f32,
    /// Energy that wrecks a cubic metre (J/m³): a part's hit points are this x its volume.
    pub toughness: f32,
    /// 0 ductile (dents, tears into few big pieces) .. 1 brittle (shatters, chips at the edges).
    pub brittleness: f32,
    /// sRGB.
    pub color: [u8; 3],
    #[serde(default = "rough")]
    pub rough: u8,
    #[serde(default)]
    pub metal: u8,
    /// How its parts break: a fracture model by name (default "planos").
    #[serde(default)]
    pub fracture: Option<String>,
    /// Effect (an explosion id) where a part of it breaks or loses a chip.
    #[serde(default)]
    pub breaks: Option<String>,
    /// See-through (glass): drawn blended over the scene.
    #[serde(default)]
    pub glass: bool,
    /// The texture its parts wear by default (`mesh::FINISHES`).
    #[serde(default)]
    pub acabado: Option<String>,
    /// `acabado` resolved (set when the catalog is made).
    #[serde(skip)]
    pub finish: u8,
}

fn rough() -> u8 {
    170
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapeDef {
    /// Size (m) along x, y, z.
    Box { size: [f32; 3] },
    /// Round the Y axis; `taper` scales the top (1 a cylinder, toward 0 a cone).
    Cylinder {
        radius: f32,
        height: f32,
        #[serde(default = "sides")]
        sides: u32,
        #[serde(default = "one")]
        taper: f32,
    },
    /// A box sloping from its top at -z down to its bottom at +z.
    Wedge { size: [f32; 3] },
    /// The convex hull of points (generators, models exported with their collision hulls).
    Hull { points: Vec<[f32; 3]> },
}

fn sides() -> u32 {
    12
}
fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn third() -> f32 {
    0.3
}

impl ShapeDef {
    pub fn convex(&self) -> Convex {
        match *self {
            ShapeDef::Box { size } => Convex::cuboid(Vec3::from_array(size) * 0.5),
            ShapeDef::Cylinder { radius, height, sides, taper } => Convex::prism(radius, height * 0.5, sides as usize, taper),
            ShapeDef::Wedge { size } => Convex::wedge(Vec3::from_array(size) * 0.5),
            ShapeDef::Hull { ref points } => {
                let p: Vec<Vec3> = points.iter().map(|&q| Vec3::from_array(q)).collect();
                Convex::hull(&p).unwrap_or_else(|| Convex::cuboid(Vec3::splat(0.01)))
            }
        }
    }

    /// Its volume (m³) and its surface (m²) as it is defined: a cylinder is round here (its
    /// `convex` is a prism of a few sides, which holds a little less).
    pub fn measure(&self) -> (f32, f32) {
        use std::f32::consts::PI;
        match *self {
            ShapeDef::Box { size: [x, y, z] } => (x * y * z, 2.0 * (x * y + y * z + x * z)),
            ShapeDef::Cylinder { radius, height, taper, .. } => {
                let (a, b) = (radius, radius * taper);
                let slant = (height * height + (a - b) * (a - b)).sqrt();
                (PI * height * (a * a + a * b + b * b) / 3.0, PI * (a * a + b * b) + PI * (a + b) * slant)
            }
            _ => {
                let c = self.convex();
                (c.volume(), c.area())
            }
        }
    }
}

/// The machine a part is: a system module kind and its parameters (read by that module).
#[derive(Clone, Debug, Deserialize)]
pub struct ModuleDef {
    pub kind: String,
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// What a part lets go of when it is destroyed (a drum of propellant, a bottle under pressure, a
/// charge): a blast of `energy` J reaching `radius` m where it was, shown as explosion `effect`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BurstDef {
    pub energy: f32,
    pub radius: f32,
    pub effect: String,
}

/// A line of text on a part, in the part's frame (`labels`): where its anchor is, the letters'
/// across and up (each a letter's height long), where the anchor is along it (0 its left end,
/// 0.5 its middle, 1 its right end).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub text: String,
    pub at: [f32; 3],
    pub u: [f32; 3],
    pub v: [f32; 3],
    #[serde(default = "half")]
    pub align: f32,
    /// sRGB.
    #[serde(default)]
    pub color: [u8; 3],
    /// 0 painted; more, raised (cast, embossed); less, cut in (engraved, stamped).
    #[serde(default)]
    pub relief: f32,
    /// Lit letters (a sign) glow this much.
    #[serde(default)]
    pub emissive: f32,
    /// It goes round a cylinder of this radius about its up (0: it lies flat).
    #[serde(default)]
    pub radius: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartKindDef {
    pub name: String,
    pub shape: ShapeDef,
    pub material: String,
    /// Networks it plugs into ("energia", "propelente", "aire"...).
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub module: Option<ModuleDef>,
    /// Under this share of its hit points it stops working: ports and module off.
    #[serde(default = "half")]
    pub function: f32,
    /// Instead of the material's colour (sRGB).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Light it gives off (0-255), e.g. windows and lamps.
    #[serde(default)]
    pub glow: u8,
    /// A shell with walls this thick (m): mass and hit points from the walls, not the volume.
    #[serde(default)]
    pub hollow: Option<f32>,
    /// A detailed look (a mesh of the catalog's `looks`) drawn instead of its plain shape.
    #[serde(default)]
    pub look: Option<String>,
    /// The look it has from a little way off, when `look` is a model too fine to draw far away
    /// (one of the catalog's `looks`); without one, `look` at any distance.
    #[serde(default)]
    pub basic: Option<String>,
    /// How much of a hit it takes (1 normal; thin things like cables and lamps more).
    #[serde(default = "unit")]
    pub fragility: f32,
    /// What it lets go of when it is destroyed (`breakup::Event::Burst`).
    #[serde(default)]
    pub burst: Option<BurstDef>,
    /// Not of the structure it is on: cargo. Come off it whole, nothing is missing from it
    /// (nothing to put back, nothing to show as gone).
    #[serde(default)]
    pub carried: bool,
    /// What it holds as built, as a scanner reads it ("170 L de agua"), if it is a container:
    /// said of every part of a thing that holds something. What is left in it now is told by
    /// `Structure::told`.
    #[serde(default)]
    pub contents: Option<String>,
    /// It is a container: what it holds, how much at most and as built (`contents`). What it
    /// holds weighs, and can be taken out and put in.
    #[serde(default)]
    pub holds: Option<super::contents::ContainerDef>,
    /// Inside a hull (a ship's cabin, a station's rooms): from outside it is not seen, and the
    /// looks from afar leave it out (`look::DetailLods`).
    #[serde(default)]
    pub interior: bool,
    /// A cable, pipe or duct: the first thing the looks from afar leave out.
    #[serde(default)]
    pub wiring: bool,
    /// Only seen: nothing strikes it, damages it or weighs it (a cable from an outlet to its
    /// device, a fixed pipe). It goes with the structure it is on.
    #[serde(default)]
    pub ghost: bool,
    /// Bodies do not rest on it nor bump into it (thin things: trunks, outlets); shots and
    /// blasts still hurt it.
    #[serde(default)]
    pub no_collide: bool,
    /// Badly damaged (and fed), it spits sparks (outlets, junction boxes).
    #[serde(default)]
    pub sparks: bool,
    /// What is written on it (`labels`): it goes with the part wherever the part goes.
    #[serde(default)]
    pub labels: Vec<Label>,
}

fn unit() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointKindDef {
    pub name: String,
    /// Energy that breaks it (J).
    pub strength: f32,
    /// Share of the damage the parts it holds take that reaches it.
    #[serde(default = "third")]
    pub transfer: f32,
}

/// A part kind ready to build with.
#[derive(Clone, Debug)]
pub struct PartKind {
    pub id: String,
    pub def: PartKindDef,
    pub shape: Arc<Convex>,
    pub material: u16,
    /// Network bits.
    pub ports: u32,
    pub volume: f32,
    /// Share of the volume that is material (1 solid; walls x area / volume for a shell).
    pub fill: f32,
    /// Its detailed look (index into `Catalog::looks`).
    pub look: Option<u32>,
    /// What it looks like from a little way off.
    pub basic: Basic,
    /// Its middle extent (m): how big it looks from afar (a cable is thin however long, a plate
    /// is wide however thin).
    pub size: f32,
    /// What it holds, if it is a container (`PartKindDef::holds`, resolved).
    pub container: Option<super::contents::Container>,
}

/// The look of a part kind beyond the reach of its model (`look::DetailLods`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Basic {
    /// The same as up close: it has no model, or none worth swapping.
    #[default]
    Same,
    /// Its plain shape in its material.
    Shape,
    /// Another of the catalog's looks (what it had before it had a model).
    Look(u32),
}

/// The name of the mesh of a model file that is the look of a thing of one piece.
pub const BODY: &str = "cuerpo";

/// The file of the substances registry, beside the structures' folder (`contents`).
pub const SUBSTANCES: &str = "sustancias";

#[derive(Clone)]
pub struct Catalog {
    pub materials: Vec<(String, MaterialDef)>,
    pub parts: Vec<PartKind>,
    pub joints: Vec<(String, JointKindDef)>,
    /// Network names, by bit.
    pub networks: Vec<String>,
    /// Detailed looks of parts (in the part's frame), by name: generated hull panels, models.
    pub looks: Vec<(String, crate::mesh::Mesh)>,
    /// The models there are (`assets/models`): looks made elsewhere, taken by name.
    pub models: super::models::Models,
    /// The shape of a whole structure seen from far away, by blueprint name (structure frame,
    /// joints at rest): a few boxes and a loft, coloured by the renderer from its real look
    /// (`look::DetailLods`). Without one the farthest look is its box.
    pub far: Vec<(String, Arc<crate::mesh::Mesh>)>,
    /// The substances things hold (`contents`, `sustancias.jsonc`), by id.
    pub substances: Vec<(String, super::contents::SubstanceDef)>,
}

/// The middle of a shape's three extents (m), in its own frame.
pub fn middle_extent(shape: &Convex) -> f32 {
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for v in shape.verts() {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let mut e = (hi - lo).max(glam::Vec3::ZERO).to_array();
    e.sort_by(f32::total_cmp);
    e[1]
}

impl Catalog {
    /// `materials.jsonc`, `joints.jsonc` and `parts/*.jsonc` under `dir`; and the substances
    /// things hold, which are of everything and live beside it (`dir/../sustancias.jsonc`: none
    /// without the file).
    pub fn load(dir: &Path) -> Result<Catalog, DefError> {
        let materials: BTreeMap<String, MaterialDef> = defs::load(&defs::file(dir, "materials"))?;
        let joints: BTreeMap<String, JointKindDef> = defs::load(&defs::file(dir, "joints"))?;
        let files: Vec<(String, BTreeMap<String, PartKindDef>)> = defs::load_dir(&dir.join("parts"))?;
        let parts = files.into_iter().flat_map(|(_, m)| m).collect();
        // (the models live beside the definitions: `assets/models` for `assets/defs/structures`)
        let models = super::models::Models::load(&dir.join("../../models")).map_err(|e| DefError::new("models", e))?;
        let registry = dir.join(format!("../{SUBSTANCES}.{}", defs::EXTENSION));
        let substances: BTreeMap<String, super::contents::SubstanceDef> = if registry.exists() { defs::load(&registry)? } else { BTreeMap::new() };
        Catalog::with(materials.into_iter().collect(), parts, joints.into_iter().collect(), models, substances.into_iter().collect()).map_err(|e| DefError::new("structures", e))
    }

    pub fn new(materials: Vec<(String, MaterialDef)>, parts: Vec<(String, PartKindDef)>, joints: Vec<(String, JointKindDef)>) -> Result<Catalog, String> {
        Catalog::with_models(materials, parts, joints, super::models::Models::default())
    }

    pub fn with_models(materials: Vec<(String, MaterialDef)>, parts: Vec<(String, PartKindDef)>, joints: Vec<(String, JointKindDef)>, models: super::models::Models) -> Result<Catalog, String> {
        Catalog::with(materials, parts, joints, models, Vec::new())
    }

    /// The same with the substances its parts may hold.
    pub fn with(materials: Vec<(String, MaterialDef)>, parts: Vec<(String, PartKindDef)>, joints: Vec<(String, JointKindDef)>, models: super::models::Models, substances: Vec<(String, super::contents::SubstanceDef)>) -> Result<Catalog, String> {
        if let Some((m, _)) = materials.iter().find(|(_, m)| m.density <= 0.0 || m.toughness <= 0.0) {
            return Err(format!("material {m}: density and toughness must be > 0"));
        }
        let mut materials = materials;
        for (id, m) in &mut materials {
            m.finish = match &m.acabado {
                Some(f) => crate::mesh::finish(f).ok_or_else(|| format!("material {id}: acabado desconocido '{f}' (hay: {})", crate::mesh::FINISHES.join(", ")))?,
                None => 0,
            };
        }
        if let Some((s, _)) = substances.iter().find(|(_, s)| !(s.densidad > 0.0)) {
            return Err(format!("substance {s}: density must be > 0"));
        }
        let mut cat = Catalog { materials, parts: Vec::with_capacity(parts.len()), joints, networks: Vec::new(), looks: Vec::new(), models, far: Vec::new(), substances };
        for (id, def) in parts {
            cat.add_part(id, def, None)?;
        }
        Ok(cat)
    }

    /// One more part kind (generators add theirs before the library is shared). `shape` overrides
    /// the definition's (a generated convex).
    pub fn add_part(&mut self, id: String, def: PartKindDef, shape: Option<Convex>) -> Result<u16, String> {
        if self.parts.len() >= usize::from(u16::MAX) {
            return Err("too many part kinds".into());
        }
        let material = self.materials.iter().position(|(m, _)| *m == def.material).ok_or_else(|| format!("part {id}: unknown material '{}'", def.material))? as u16;
        let mut ports = 0;
        for p in &def.ports {
            let bit = match self.networks.iter().position(|n| n == p) {
                Some(b) => b,
                None => {
                    self.networks.push(p.clone());
                    self.networks.len() - 1
                }
            };
            if bit >= MAX_NETWORKS {
                return Err(format!("more than {MAX_NETWORKS} networks"));
            }
            ports |= 1 << bit;
        }
        let shape = shape.unwrap_or_else(|| def.shape.convex());
        let volume = shape.volume();
        if volume <= 0.0 {
            return Err(format!("part {id}: empty shape"));
        }
        let fill = def.hollow.map_or(1.0, |t| (shape.area() * t / volume).min(1.0));
        let named = |looks: &[(String, crate::mesh::Mesh)], l: &str| looks.iter().position(|(n, _)| n == l).map(|k| k as u32).ok_or_else(|| format!("part {id}: unknown look '{l}'"));
        let (look, basic) = match &def.look {
            Some(l) => (
                Some(named(&self.looks, l)?),
                match &def.basic {
                    Some(b) => Basic::Look(named(&self.looks, b)?),
                    None => Basic::Same,
                },
            ),
            // a part kind with a model of its own name wears it, in its own colours
            None => match self.models.get(&format!("parte_{id}/{BODY}")) {
                Some(model) => {
                    let m = &self.materials[usize::from(material)].1;
                    let own = crate::mesh::Material { albedo: def.color.unwrap_or(m.color), rough: m.rough, metal: m.metal, emissive: def.glow, panel: 0, finish: m.finish };
                    let mesh = model.tinted(own);
                    (Some(self.add_look(&format!("{id}/{BODY}"), mesh)), Basic::Shape)
                }
                None => (None, Basic::Same),
            },
        };
        let size = middle_extent(&shape);
        let container = match &def.holds {
            Some(h) => Some(super::contents::Container::new(h, &shape, def.hollow.unwrap_or(0.0), fill, &self.substances).map_err(|e| format!("part {id}: {e}"))?),
            None => None,
        };
        self.parts.push(PartKind { id, def, shape: Arc::new(shape), material, ports, volume, fill, look, basic, size, container });
        Ok((self.parts.len() - 1) as u16)
    }

    /// The far shape of blueprint `name` (replaced if it has one).
    pub fn add_far(&mut self, name: &str, mesh: crate::mesh::Mesh) {
        let mesh = Arc::new(mesh);
        match self.far.iter_mut().find(|(n, _)| n == name) {
            Some(f) => f.1 = mesh,
            None => self.far.push((name.to_string(), mesh)),
        }
    }

    /// The far shape of blueprint `name`, if it has one.
    pub fn far_of(&self, name: &str) -> Option<Arc<crate::mesh::Mesh>> {
        self.far.iter().find(|(n, _)| n == name).map(|(_, m)| m.clone())
    }

    /// A detailed look by name (replaced if it exists): its index.
    pub fn add_look(&mut self, name: &str, mesh: crate::mesh::Mesh) -> u32 {
        if let Some(i) = self.looks.iter().position(|(n, _)| n == name) {
            self.looks[i].1 = mesh;
            return i as u32;
        }
        self.looks.push((name.to_string(), mesh));
        (self.looks.len() - 1) as u32
    }

    pub fn part(&self, id: &str) -> Option<u16> {
        self.parts.iter().position(|p| p.id == id).map(|i| i as u16)
    }

    pub fn joint(&self, id: &str) -> Option<u16> {
        self.joints.iter().position(|(j, _)| j == id).map(|i| i as u16)
    }

    pub fn material(&self, part: u16) -> &MaterialDef {
        &self.materials[usize::from(self.parts[usize::from(part)].material)].1
    }

    /// A substance of the registry by id: its index.
    pub fn substance(&self, id: &str) -> Option<u16> {
        self.substances.iter().position(|(s, _)| s == id).map(|i| i as u16)
    }

    pub fn network(&self, name: &str) -> Option<u32> {
        self.networks.iter().position(|n| n == name).map(|b| 1 << b)
    }
}
