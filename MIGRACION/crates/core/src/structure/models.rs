//! Looks made elsewhere: the models of parts (`assets/models/*.glb`, each written by a Blender
//! recipe of `tools/modelos`). A part is still its convex shape for everything that is physics
//! — what strikes it, what it weighs, how it breaks —; a model is only what it looks like from
//! close by, drawn instead of that shape.
//!
//! A file is a thing (a kind of component, a kind of part, a tool); each mesh in it is one of
//! its pieces, named after it, in that piece's own frame (y up, z forward, metres). The model
//! `<file>/<mesh>` is the look of piece `<mesh>` of `<file>`; whoever builds parts asks for it by
//! that name and, if it is there, uses it — a new model is a file dropped in, nothing else.
//!
//! What a surface is comes from its material in the file: its colour (times the vertex colour:
//! shade baked into the corners), roughness, metalness and glow; `#<finish>` at the end of its
//! name is the finish it wears (`mesh::FINISHES`). A material whose name starts with `tinte` is
//! not a colour of the model's own: it is the part's surface as its data has it (its paint, its
//! finish), so one model serves a blue drum and a red one.
//!
//! The reader is a small one of its own for what the recipes write (a `.glb` with one buffer,
//! triangles, float positions and normals): no images, no skins, no node transforms.
use crate::mesh::{Material, Mesh};
use serde::Deserialize;
use std::path::Path;

/// One model: its triangles in the piece's frame, and per vertex whether it is the part's own
/// surface (`tint` > 0: how much of the part's colour shows, 255 all of it — the shade baked
/// there) or a surface of the model's own (0).
#[derive(Clone, Debug, Default)]
pub struct PartModel {
    pub mesh: Mesh,
    pub tint: Vec<u8>,
}

impl PartModel {
    /// The model as the look of a part whose own surface is `own`.
    pub fn tinted(&self, own: Material) -> Mesh {
        let mut m = self.mesh.clone();
        for (mat, &t) in m.mat.iter_mut().zip(&self.tint) {
            if t > 0 {
                let shade = |c: u8| (u16::from(c) * u16::from(t) / 255) as u8;
                *mat = Material { albedo: own.albedo.map(shade), emissive: mat.emissive.max(own.emissive), ..own };
            }
        }
        m
    }

    /// The same reflected in its own x (triangles turned so they still face out): the model of a
    /// piece's copy across a centreline.
    pub fn mirrored(&self) -> PartModel {
        let mut m = self.clone();
        for p in &mut m.mesh.pos {
            p[0] = -p[0];
        }
        for n in &mut m.mesh.nrm {
            n[0] = -n[0];
        }
        for t in m.mesh.idx.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
        m
    }

    /// Its box (min, max).
    pub fn bounds(&self) -> (glam::Vec3, glam::Vec3) {
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for p in &self.mesh.pos {
            lo = lo.min(glam::Vec3::from_array(*p));
            hi = hi.max(glam::Vec3::from_array(*p));
        }
        (lo, hi)
    }
}

/// Every model, by name (`<file>/<mesh>`), sorted.
#[derive(Clone, Debug, Default)]
pub struct Models {
    list: Vec<(String, PartModel)>,
}

impl Models {
    /// Every `.glb` under `dir` (none if there is no such folder).
    pub fn load(dir: &Path) -> Result<Models, String> {
        let mut list = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(Models::default());
        };
        let mut paths: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "glb")).collect();
        paths.sort();
        for p in paths {
            let file = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            for (mesh, model) in read(&bytes).map_err(|e| format!("{}: {e}", p.display()))? {
                list.push((format!("{file}/{mesh}"), model));
            }
        }
        list.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Models { list })
    }

    pub fn get(&self, name: &str) -> Option<&PartModel> {
        self.list.binary_search_by(|(n, _)| n.as_str().cmp(name)).ok().map(|k| &self.list[k].1)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.list.iter().map(|(n, _)| n.as_str())
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// One more (tests, tools): replaced if it is there.
    pub fn insert(&mut self, name: &str, model: PartModel) {
        match self.list.binary_search_by(|(n, _)| n.as_str().cmp(name)) {
            Ok(k) => self.list[k].1 = model,
            Err(k) => self.list.insert(k, (name.to_string(), model)),
        }
    }
}

// ---------------------------------------------------------------- the file

#[derive(Deserialize)]
struct Gltf {
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    meshes: Vec<GMesh>,
    #[serde(default)]
    materials: Vec<GMaterial>,
    #[serde(default)]
    accessors: Vec<Accessor>,
    #[serde(default, rename = "bufferViews")]
    views: Vec<View>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(default)]
    name: String,
    mesh: Option<usize>,
    translation: Option<[f32; 3]>,
    rotation: Option<[f32; 4]>,
    scale: Option<[f32; 3]>,
    matrix: Option<[f32; 16]>,
}

#[derive(Deserialize)]
struct GMesh {
    primitives: Vec<Primitive>,
}

#[derive(Deserialize)]
struct Primitive {
    attributes: std::collections::BTreeMap<String, usize>,
    indices: Option<usize>,
    material: Option<usize>,
    #[serde(default = "triangles")]
    mode: u32,
}

fn triangles() -> u32 {
    4
}

#[derive(Deserialize, Default)]
struct GMaterial {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "pbrMetallicRoughness")]
    pbr: Pbr,
    #[serde(default, rename = "emissiveFactor")]
    emissive: [f32; 3],
    #[serde(default)]
    extensions: serde_json::Value,
}

#[derive(Deserialize)]
struct Pbr {
    #[serde(default = "white", rename = "baseColorFactor")]
    color: [f32; 4],
    #[serde(default = "one", rename = "metallicFactor")]
    metal: f32,
    #[serde(default = "one", rename = "roughnessFactor")]
    rough: f32,
}

impl Default for Pbr {
    fn default() -> Pbr {
        Pbr { color: white(), metal: 1.0, rough: 1.0 }
    }
}

fn white() -> [f32; 4] {
    [1.0; 4]
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize)]
struct Accessor {
    #[serde(rename = "bufferView")]
    view: Option<usize>,
    #[serde(default, rename = "byteOffset")]
    offset: usize,
    #[serde(rename = "componentType")]
    component: u32,
    #[serde(default)]
    normalized: bool,
    count: usize,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct View {
    #[serde(default, rename = "byteOffset")]
    offset: usize,
    #[serde(rename = "byteLength")]
    length: usize,
    #[serde(rename = "byteStride")]
    stride: Option<usize>,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Linear light to an sRGB byte.
fn srgb(x: f32) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let s = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// The meshes of a `.glb`, by the name of their node up to its first `.` (several nodes of one
/// name are one model).
pub fn read(bytes: &[u8]) -> Result<Vec<(String, PartModel)>, String> {
    if bytes.get(0..4) != Some(b"glTF".as_slice()) || u32_at(bytes, 4) != Some(2) {
        return Err("no es un .glb de glTF 2".into());
    }
    // its two chunks: the JSON, the buffer
    let json_len = u32_at(bytes, 12).ok_or("truncado")? as usize;
    if u32_at(bytes, 16) != Some(0x4E4F_534A) {
        return Err("sin trozo JSON".into());
    }
    let json = bytes.get(20..20 + json_len).ok_or("truncado")?;
    let rest = 20 + json_len;
    let bin: &[u8] = match (u32_at(bytes, rest), u32_at(bytes, rest + 4)) {
        (Some(len), Some(0x004E_4942)) => bytes.get(rest + 8..rest + 8 + len as usize).ok_or("truncado")?,
        _ => &[],
    };
    let g: Gltf = serde_json::from_slice(json).map_err(|e| format!("JSON: {e}"))?;
    // an accessor's values as floats, `width` to a value
    let floats = |a: usize, width: usize| -> Result<Vec<f32>, String> {
        let acc = g.accessors.get(a).ok_or("accesor que no existe")?;
        let per = match acc.kind.as_str() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            k => return Err(format!("accesor de tipo {k}")),
        };
        let size = match acc.component {
            5121 => 1,
            5123 => 2,
            5125 | 5126 => 4,
            c => return Err(format!("componente {c}")),
        };
        let view = g.views.get(acc.view.ok_or("accesor sin vista")?).ok_or("vista que no existe")?;
        let stride = view.stride.unwrap_or(per * size);
        let start = view.offset + acc.offset;
        if acc.count > 0 && (start + (acc.count - 1) * stride + per * size > bin.len() || acc.offset + (acc.count - 1) * stride + per * size > view.length) {
            return Err("accesor fuera del buffer".into());
        }
        let mut out = Vec::with_capacity(acc.count * width);
        for i in 0..acc.count {
            for c in 0..width {
                if c >= per {
                    out.push(1.0);
                    continue;
                }
                let at = start + i * stride + c * size;
                out.push(match acc.component {
                    5121 => f32::from(bin[at]) / if acc.normalized { 255.0 } else { 1.0 },
                    5123 => f32::from(u16::from_le_bytes([bin[at], bin[at + 1]])) / if acc.normalized { 65535.0 } else { 1.0 },
                    5125 => u32::from_le_bytes([bin[at], bin[at + 1], bin[at + 2], bin[at + 3]]) as f32,
                    _ => f32::from_le_bytes([bin[at], bin[at + 1], bin[at + 2], bin[at + 3]]),
                });
            }
        }
        Ok(out)
    };
    let mut out: Vec<(String, PartModel)> = Vec::new();
    for node in &g.nodes {
        let Some(mesh) = node.mesh else { continue };
        // (a recipe bakes its transforms into its vertices: a node that still has one is a mistake)
        let still = node.matrix.is_none() && node.translation.is_none_or(|t| t.iter().all(|x| x.abs() < 1e-5)) && node.rotation.is_none_or(|r| r[..3].iter().all(|x| x.abs() < 1e-5)) && node.scale.is_none_or(|s| s.iter().all(|x| (x - 1.0).abs() < 1e-5));
        if !still {
            return Err(format!("malla '{}': su nodo tiene una transformación sin aplicar", node.name));
        }
        let name = node.name.split('.').next().unwrap_or_default().to_string();
        let k = match out.iter().position(|(n, _)| *n == name) {
            Some(k) => k,
            None => {
                out.push((name, PartModel::default()));
                out.len() - 1
            }
        };
        let model = &mut out[k].1;
        for prim in &g.meshes.get(mesh).ok_or("malla que no existe")?.primitives {
            if prim.mode != 4 {
                return Err(format!("malla '{}': no son triángulos", node.name));
            }
            let pos = floats(*prim.attributes.get("POSITION").ok_or("sin POSITION")?, 3)?;
            let nrm = floats(*prim.attributes.get("NORMAL").ok_or_else(|| format!("malla '{}': sin NORMAL", node.name))?, 3)?;
            let shade = match prim.attributes.get("COLOR_0") {
                Some(&a) => Some(floats(a, 4)?),
                None => None,
            };
            let default = GMaterial::default();
            let gm = prim.material.and_then(|m| g.materials.get(m)).unwrap_or(&default);
            let (base, finish) = match gm.name.split_once('#') {
                Some((base, f)) => (base, crate::mesh::finish(f).ok_or_else(|| format!("material '{}': acabado desconocido '{f}'", gm.name))?),
                None => (gm.name.as_str(), 0),
            };
            let tinted = base.starts_with("tinte");
            let strength = gm.extensions.get("KHR_materials_emissive_strength").and_then(|e| e.get("emissiveStrength")).and_then(serde_json::Value::as_f64).unwrap_or(1.0) as f32;
            let emissive = (gm.emissive.iter().copied().fold(0.0, f32::max) * strength / 8.0 * 255.0).clamp(0.0, 255.0) as u8;
            let first = model.mesh.pos.len() as u32;
            for i in 0..pos.len() / 3 {
                let s = shade.as_ref().map_or([1.0; 3], |c| [c[i * 4], c[i * 4 + 1], c[i * 4 + 2]]);
                let albedo = [srgb(gm.pbr.color[0] * s[0]), srgb(gm.pbr.color[1] * s[1]), srgb(gm.pbr.color[2] * s[2])];
                model.mesh.pos.push([pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]]);
                model.mesh.nrm.push([nrm[i * 3], nrm[i * 3 + 1], nrm[i * 3 + 2]]);
                model.mesh.mat.push(Material { albedo, rough: (gm.pbr.rough * 255.0) as u8, metal: (gm.pbr.metal * 255.0) as u8, emissive, panel: 0, finish });
                // (the part's own surface: only the shade of the corner is kept)
                model.tint.push(if tinted { srgb((s[0] + s[1] + s[2]) / 3.0).max(1) } else { 0 });
            }
            match prim.indices {
                Some(a) => model.mesh.idx.extend(floats(a, 1)?.into_iter().map(|i| first + i as u32)),
                None => model.mesh.idx.extend(first..model.mesh.pos.len() as u32),
            }
            if model.mesh.idx.len() % 3 != 0 || model.mesh.idx.iter().any(|&i| i as usize >= model.mesh.pos.len()) {
                return Err(format!("malla '{}': índices rotos", node.name));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.glb` of one triangle, as a recipe would write it.
    fn triangle(material: &str, node: &str, moved: bool) -> Vec<u8> {
        let mut bin: Vec<u8> = Vec::new();
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            bin.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        }
        for _ in 0..3 {
            bin.extend([0.0f32, 0.0, 1.0].iter().flat_map(|x| x.to_le_bytes()));
        }
        for c in [[65535u16; 4], [32768, 32768, 32768, 65535], [65535; 4]] {
            bin.extend(c.iter().flat_map(|x| x.to_le_bytes()));
        }
        bin.extend([0u16, 1, 2].iter().flat_map(|x| x.to_le_bytes()));
        bin.extend([0u8; 2]);
        let node = if moved { format!(r#"{{"name":"{node}","mesh":0,"translation":[0,1,0]}}"#) } else { format!(r#"{{"name":"{node}","mesh":0}}"#) };
        let mut json = format!(
            r#"{{"asset":{{"version":"2.0"}},"nodes":[{node}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"COLOR_0":2}},"indices":3,"material":0}}]}}],
            "materials":[{{"name":"{material}","pbrMetallicRoughness":{{"baseColorFactor":[1,0.5,0.25,1],"metallicFactor":0.2,"roughnessFactor":0.6}}}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},
            {{"bufferView":2,"componentType":5123,"normalized":true,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5123,"count":3,"type":"SCALAR"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},{{"buffer":0,"byteOffset":96,"byteLength":6}}],
            "buffers":[{{"byteLength":104}}]}}"#
        )
        .into_bytes();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2u32.to_le_bytes());
        out.extend(((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(0x4E4F_534Au32.to_le_bytes());
        out.extend(&json);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(0x004E_4942u32.to_le_bytes());
        out.extend(&bin);
        out
    }

    #[test]
    fn a_model_is_read_with_its_material_its_shade_and_its_finish() {
        let models = read(&triangle("goma_negra#goma", "cojin.001", false)).unwrap();
        assert_eq!(models.len(), 1);
        let (name, m) = &models[0];
        assert_eq!(name, "cojin", "its name is its node's, up to the first dot");
        assert_eq!(m.mesh.tris(), 1);
        assert_eq!(m.mesh.pos[1], [1.0, 0.0, 0.0]);
        assert_eq!(m.mesh.nrm[0], [0.0, 0.0, 1.0]);
        // its colour: the material's, darker where the corner is shaded; linear light to sRGB
        assert_eq!(m.mesh.mat[0].albedo, [255, 188, 137]);
        assert!(m.mesh.mat[1].albedo[0] < 200 && m.mesh.mat[1].albedo[0] > 170, "{:?}", m.mesh.mat[1].albedo);
        assert_eq!((m.mesh.mat[0].rough, m.mesh.mat[0].metal), (153, 51));
        assert_eq!(m.mesh.mat[0].finish, crate::mesh::finish("goma").unwrap());
        assert!(m.tint.iter().all(|t| *t == 0), "a colour of its own");
    }

    #[test]
    fn a_tint_material_is_the_parts_own_surface() {
        let models = read(&triangle("tinte", "cuerpo", false)).unwrap();
        let m = &models[0].1;
        assert!(m.tint[0] == 255 && m.tint[1] < 200 && m.tint[1] > 170, "{:?}", m.tint);
        let own = Material { albedo: [200, 40, 30], rough: 90, metal: 10, emissive: 0, panel: 6, finish: 1 };
        let look = m.tinted(own);
        assert_eq!(look.mat[0], own, "where nothing shades it, the part's surface as it is");
        assert!(look.mat[1].albedo[0] < 150 && look.mat[1].albedo[0] > 130 && look.mat[1].finish == 1 && look.mat[1].panel == 6, "{:?}", look.mat[1]);
    }

    #[test]
    fn what_is_not_a_model_says_so() {
        assert!(read(b"nada").is_err());
        let e = read(&triangle("x#terciopelo", "a", false)).unwrap_err();
        assert!(e.contains("terciopelo"), "{e}");
        let e = read(&triangle("x", "a", true)).unwrap_err();
        assert!(e.contains("transformación"), "{e}");
        let mut cut = triangle("x", "a", false);
        cut.truncate(cut.len() - 40);
        assert!(read(&cut).is_err());
    }

    #[test]
    fn models_are_found_by_name() {
        let mut m = Models::default();
        for n in ["bidon/cuerpo", "asiento/cojin", "asiento/respaldo"] {
            m.insert(n, PartModel::default());
        }
        assert_eq!(m.names().collect::<Vec<_>>(), ["asiento/cojin", "asiento/respaldo", "bidon/cuerpo"]);
        assert!(m.get("asiento/respaldo").is_some() && m.get("asiento").is_none());
        assert!(Models::load(Path::new("no/such/folder")).unwrap().is_empty());
    }
}
