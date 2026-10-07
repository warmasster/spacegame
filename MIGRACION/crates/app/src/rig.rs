//! Rigged models: a GLB whose mesh follows a skeleton (`tools/modelos/astronauta`), read into
//! `lunar_core::anim::Skeleton` and a mesh whose vertices know their bones; and what the data
//! says of a rig (`assets/defs/rigs/*.jsonc`): which of its bones are what (a pelvis, the legs,
//! the arms, each finger), where its eyes are, how it walks, the poses of its hands. Whoever
//! animates a body works with those names and never with a model's own.
use glam::{Affine3A, Mat4, Quat, Vec3};
use lunar_core::{
    anim::{Skeleton, Xf, gait::GaitDef, ik::Limb},
    mesh::{Material, Mesh},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

/// A rigged model as read.
pub struct Rigged {
    pub skeleton: Skeleton,
    /// All of it: one mesh, its vertices' bones as the skeleton numbers them.
    pub mesh: Mesh,
    /// Its materials' names, and each triangle's.
    pub materials: Vec<String>,
    pub triangle_material: Vec<u16>,
}

fn material(m: &gltf::Material) -> Material {
    let pbr = m.pbr_metallic_roughness();
    let c = pbr.base_color_factor();
    let srgb = |v: f32| (v.max(0.0).powf(1.0 / 2.2) * 255.0).round().min(255.0) as u8;
    let e = m.emissive_factor();
    let strength = m.emissive_strength().unwrap_or(1.0);
    let emissive = (e[0].max(e[1]).max(e[2]) * strength / 8.0 * 255.0).min(255.0) as u8;
    Material { albedo: [srgb(c[0]), srgb(c[1]), srgb(c[2])], rough: (pbr.roughness_factor() * 255.0) as u8, metal: (pbr.metallic_factor() * 255.0) as u8, emissive, panel: 0, finish: 0 }
}

impl Rigged {
    pub fn load(path: &Path) -> Result<Rigged, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Rigged::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn read(bytes: &[u8]) -> Result<Rigged, String> {
        let gltf = gltf::Gltf::from_slice_without_validation(bytes).map_err(|e| e.to_string())?;
        let blob = gltf.blob.as_deref().ok_or("un GLB sin su parte binaria")?;
        let doc = &gltf.document;
        // every node in the scene's space
        let count = doc.nodes().count();
        let mut parent = vec![None; count];
        let mut local = vec![Affine3A::IDENTITY; count];
        for n in doc.nodes() {
            let (t, r, s) = n.transform().decomposed();
            local[n.index()] = Affine3A::from_scale_rotation_translation(Vec3::from(s), Quat::from_array(r), Vec3::from(t));
            for c in n.children() {
                parent[c.index()] = Some(n.index());
            }
        }
        let world = |mut n: usize| {
            let mut m = local[n];
            while let Some(p) = parent[n] {
                m = local[p] * m;
                n = p;
            }
            m
        };
        let skin = doc.skins().next().ok_or("sin esqueleto (skin)")?;
        let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
        let ibm: Vec<Mat4> = skin.reader(|_| Some(blob)).read_inverse_bind_matrices().ok_or("sin matrices de reposo")?.map(|m| Mat4::from_cols_array_2d(&m)).collect();
        if ibm.len() != joints.len() {
            return Err("tantas matrices de reposo como huesos".into());
        }
        // a bone's parent is the nearest joint over it; its place at rest, in that one's frame
        let mut bones = Vec::with_capacity(joints.len());
        for (k, &node) in joints.iter().enumerate() {
            let mut up = parent[node];
            while let Some(p) = up.filter(|p| !joints.contains(p)) {
                up = parent[p];
            }
            let over = up.and_then(|p| joints.iter().position(|&j| j == p));
            let rel = match up {
                Some(p) => world(p).inverse() * world(node),
                None => world(node),
            };
            let (s, r, t) = rel.to_scale_rotation_translation();
            if (s - Vec3::ONE).abs().max_element() > 1e-3 {
                return Err(format!("el hueso '{}' tiene escala {s:?}: los huesos no se estiran", doc.nodes().nth(node).and_then(|n| n.name().map(str::to_string)).unwrap_or_default()));
            }
            let name = doc.nodes().nth(node).and_then(|n| n.name().map(str::to_string)).unwrap_or_else(|| format!("hueso_{k}"));
            bones.push((name, over, Xf::new(t, r.normalize()), Affine3A::from_mat4(ibm[k])));
        }
        let (skeleton, order) = Skeleton::new(bones)?;
        // one mesh: every primitive of every skinned node
        let mut mesh = Mesh::default();
        let mut materials: Vec<String> = Vec::new();
        let mut triangle_material = Vec::new();
        for node in doc.nodes() {
            let (Some(m), Some(_)) = (node.mesh(), node.skin()) else { continue };
            for prim in m.primitives() {
                let r = prim.reader(|_| Some(blob));
                let base = mesh.pos.len() as u32;
                let mat = material(&prim.material());
                let name = prim.material().name().unwrap_or("").to_string();
                let mi = materials.iter().position(|n| *n == name).unwrap_or_else(|| {
                    materials.push(name.clone());
                    materials.len() - 1
                }) as u16;
                let pos: Vec<[f32; 3]> = r.read_positions().ok_or("sin posiciones")?.collect();
                let nrm: Vec<[f32; 3]> = r.read_normals().ok_or("sin normales")?.collect();
                let colors: Vec<[f32; 4]> = r.read_colors(0).map(|c| c.into_rgba_f32().collect()).unwrap_or_else(|| vec![[1.0; 4]; pos.len()]);
                let js: Vec<[u16; 4]> = r.read_joints(0).ok_or_else(|| format!("la malla '{}' no sigue a ningún hueso", m.name().unwrap_or("")))?.into_u16().collect();
                let ws: Vec<[f32; 4]> = r.read_weights(0).ok_or("sin pesos")?.into_f32().collect();
                for i in 0..pos.len() {
                    let mut v = mat;
                    // (the baked shade is in the vertices' colours)
                    for k in 0..3 {
                        v.albedo[k] = (f32::from(v.albedo[k]) * colors[i][k].powf(1.0 / 2.2)).round().min(255.0) as u8;
                    }
                    mesh.vertex(Vec3::from(pos[i]), Vec3::from(nrm[i]), v);
                    let mut j = [0u8; 4];
                    for k in 0..4 {
                        j[k] = if ws[i][k] > 0.0 { *order.get(usize::from(js[i][k])).ok_or("un vértice sigue a un hueso que no hay")? as u8 } else { 0 };
                    }
                    mesh.joints.push(j);
                    mesh.weights.push(ws[i]);
                }
                let before = mesh.idx.len();
                match r.read_indices() {
                    Some(ix) => mesh.idx.extend(ix.into_u32().map(|i| i + base)),
                    None => mesh.idx.extend(base..base + pos.len() as u32),
                }
                triangle_material.extend(std::iter::repeat_n(mi, (mesh.idx.len() - before) / 3));
            }
        }
        if mesh.pos.is_empty() {
            return Err("sin malla que siga al esqueleto".into());
        }
        Ok(Rigged { skeleton, mesh, materials, triangle_material })
    }

    /// The mesh without the triangles of the materials `hidden` (what whoever wears it does not
    /// see: the helmet round the eyes).
    pub fn without(&self, hidden: &[String]) -> Mesh {
        let mut m = self.mesh.clone();
        m.idx = self.mesh.idx.chunks(3).zip(&self.triangle_material).filter(|(_, mi)| !hidden.contains(&self.materials[usize::from(**mi)])).flat_map(|(t, _)| t.iter().copied()).collect();
        m
    }
}

/// A leg, an arm: its bones by name.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegDef {
    pub muslo: String,
    pub espinilla: String,
    pub pie: String,
    #[serde(default)]
    pub punta: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmDef {
    #[serde(default)]
    pub clavicula: Option<String>,
    pub brazo: String,
    pub antebrazo: String,
    pub mano: String,
    /// Thumb first, then index to little finger: each its bones from the palm out.
    #[serde(default)]
    pub dedos: Vec<Vec<String>>,
}

/// How far each joint of a finger bends from rest (degrees): to a fist, and to a flat hand.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FingerDef {
    pub cerrar: [f32; 3],
    pub abrir: [f32; 3],
}

/// A rig as the data has it (`assets/defs/rigs/<id>.jsonc`). Sides: left first, then right.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigDef {
    /// `assets/models/<modelo>.glb`.
    pub modelo: String,
    /// Model axes: x to its left, y up, z ahead; `escala`: metres of the world per unit of it.
    pub escala: f32,
    /// Where its eyes are at rest (model axes).
    pub ojos: [f32; 3],
    /// Materials whoever wears it does not see (they are round the eyes).
    #[serde(default)]
    pub oculto: Vec<String>,
    pub pelvis: String,
    /// From the pelvis up to under the neck.
    pub columna: Vec<String>,
    pub piernas: [LegDef; 2],
    pub brazos: [ArmDef; 2],
    /// The thumb's and the other fingers' travel.
    pub pulgar: FingerDef,
    pub dedo: FingerDef,
    /// How thick the handles its hands close on are (m, model units): a grip's point is the
    /// middle of one.
    #[serde(default = "handle")]
    pub mango: f32,
    /// How far each finger's tip is past the head of its last bone, thumb first (model units):
    /// where the glove ends, for whoever puts a fingertip on something (`reach`).
    #[serde(default = "tips")]
    pub yemas: [f32; 5],
    /// Hand poses by name: how closed each finger is, thumb first (-1 flat, 0 at rest, 1 a fist).
    #[serde(default)]
    pub manos: BTreeMap<String, [f32; 5]>,
    /// Places on it by name (model axes at rest) and the bone each goes with: where a hand goes
    /// for something carried on the suit.
    #[serde(default)]
    pub puntos: BTreeMap<String, PointDef>,
    #[serde(default)]
    pub marcha: GaitDef,
    #[serde(default)]
    pub sentado: SeatedDef,
    /// How far its wrists go.
    #[serde(default)]
    pub muneca: WristRange,
}

/// How far a wrist goes from the line of its forearm (degrees): bent toward the palm and toward
/// the back of the hand, leant toward the thumb and toward the little finger, and turned about
/// the forearm from how the arm hangs — palm back (`pronacion`) and palm ahead (`supinacion`).
/// A hand asked past any of these is held to it (`body`): the elbow goes round first to ease it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WristRange {
    pub flexion: f32,
    pub extension: f32,
    pub radial: f32,
    pub cubital: f32,
    pub pronacion: f32,
    pub supinacion: f32,
}

impl Default for WristRange {
    fn default() -> WristRange {
        WristRange { flexion: 60.0, extension: 55.0, radial: 20.0, cubital: 32.0, pronacion: 85.0, supinacion: 85.0 }
    }
}

/// How it sits: how far over what it sits on its hip joints are (m of the world: the thigh's own
/// thickness under them), and where its hands rest with nothing to hold (places of `puntos`,
/// left and right).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SeatedDef {
    #[serde(default = "seat_over")]
    pub asiento: f32,
    #[serde(default)]
    pub manos: [Option<String>; 2],
}

impl Default for SeatedDef {
    fn default() -> SeatedDef {
        SeatedDef { asiento: seat_over(), manos: [None, None] }
    }
}

fn seat_over() -> f32 {
    0.11
}

/// A place a hand may be at: the middle of the palm, the way the palm faces and the way from
/// the index finger to the little one, in the axes of what it is on.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PointDef {
    pub en: [f32; 3],
    #[serde(default = "palm_down")]
    pub palma: [f32; 3],
    #[serde(default = "across")]
    pub traves: [f32; 3],
    /// Body points only: the bone it goes with.
    #[serde(default)]
    pub hueso: Option<String>,
}

fn handle() -> f32 {
    0.044
}

fn tips() -> [f32; 5] {
    [0.03, 0.024, 0.027, 0.026, 0.021]
}

fn palm_down() -> [f32; 3] {
    [0.0, -1.0, 0.0]
}

fn across() -> [f32; 3] {
    [0.0, 0.0, -1.0]
}

/// A finger of a rig: its bones, the axis it curls about (model space, at rest: closing is a
/// turn about it by the right hand's rule) and its travel.
#[derive(Clone, Debug)]
pub struct Finger {
    pub bones: Vec<usize>,
    pub axis: Vec3,
    pub travel: FingerDef,
    /// How far its tip is past the head of its last bone, along that bone (model units).
    pub tip: f32,
}

/// A hand of a rig at rest (model space): the middle of its palm, the way the palm faces, the
/// way its fingers point and the way from its index finger to its little one.
#[derive(Clone, Copy, Debug)]
pub struct Palm {
    pub at: Vec3,
    pub normal: Vec3,
    #[cfg_attr(not(test), allow(dead_code))]
    pub fingers: Vec3,
    pub across: Vec3,
}

/// A wrist at rest (model space): the line of its forearm toward the hand, the axis the hand
/// bends about toward its palm, and the one it leans about toward its little finger (turns by
/// the right hand's rule).
#[derive(Clone, Copy, Debug)]
pub struct WristAxes {
    pub along: Vec3,
    pub flex: Vec3,
    pub lean: Vec3,
}

/// A rig ready to pose: its skeleton, what is what in it and what of it takes up room.
#[derive(Clone)]
pub struct Rig {
    pub def: RigDef,
    pub skeleton: Skeleton,
    pub pelvis: usize,
    pub spine: Vec<usize>,
    pub legs: [Limb; 2],
    pub toes: [Option<usize>; 2],
    pub arms: [Limb; 2],
    pub clavicles: [Option<usize>; 2],
    pub fingers: [Vec<Finger>; 2],
    pub palms: [Palm; 2],
    pub wrists: [WristAxes; 2],
    /// Body points: the bone each goes with and its place in that bone's frame.
    pub points: BTreeMap<String, (usize, Xf)>,
    /// Where each hand rests seated (a body point), if anywhere.
    pub seated_hands: [Option<(usize, Xf)>; 2],
    /// Its trunk and its arms' thickness, measured from its mesh (`bulk`).
    pub bulk: crate::bulk::Bulk,
}

/// The turn that takes a hand at rest (`palm`) to a place a hand may be at: palm facing `normal`,
/// index-to-little along `across` (both unit, in the same axes as the result).
pub fn palm_turn(palm: &Palm, normal: Vec3, across: Vec3) -> Quat {
    lunar_core::anim::skeleton::frame_turn(palm.normal, palm.across, normal, across)
}

impl Rig {
    /// The rig of `model` as `def` says.
    pub fn new(def: RigDef, model: &Rigged) -> Result<Rig, String> {
        let skeleton = model.skeleton.clone();
        let sk = &skeleton;
        let pelvis = sk.find(&def.pelvis)?;
        let spine = def.columna.iter().map(|n| sk.find(n)).collect::<Result<Vec<_>, _>>()?;
        let leg = |l: &LegDef| Ok::<_, String>(Limb { upper: sk.find(&l.muslo)?, lower: sk.find(&l.espinilla)?, end: sk.find(&l.pie)? });
        let legs = [leg(&def.piernas[0])?, leg(&def.piernas[1])?];
        let toe = |l: &LegDef| l.punta.as_deref().map(|n| sk.find(n)).transpose();
        let toes = [toe(&def.piernas[0])?, toe(&def.piernas[1])?];
        let arm = |a: &ArmDef| Ok::<_, String>(Limb { upper: sk.find(&a.brazo)?, lower: sk.find(&a.antebrazo)?, end: sk.find(&a.mano)? });
        let arms = [arm(&def.brazos[0])?, arm(&def.brazos[1])?];
        let clav = |a: &ArmDef| a.clavicula.as_deref().map(|n| sk.find(n)).transpose();
        let clavicles = [clav(&def.brazos[0])?, clav(&def.brazos[1])?];
        let mut fingers: [Vec<Finger>; 2] = [Vec::new(), Vec::new()];
        let mut palms = [Palm { at: Vec3::ZERO, normal: Vec3::NEG_Y, fingers: Vec3::Z, across: Vec3::X }; 2];
        for side in 0..2 {
            let a = &def.brazos[side];
            let hand = arms[side].end;
            for (k, names) in a.dedos.iter().enumerate() {
                // (a model without fingers: its hands are as they were made)
                let Ok(bones) = names.iter().map(|n| sk.find(n)).collect::<Result<Vec<_>, _>>() else { continue };
                if bones.len() < 2 {
                    continue;
                }
                // the plane it curls in, by its own bend at rest; closing is toward the palm
                let p: Vec<Vec3> = bones.iter().map(|&b| sk.bind[b].pos).collect();
                let mut axis = Vec3::ZERO;
                for w in p.windows(3) {
                    axis += (w[1] - w[0]).cross(w[2] - w[1]);
                }
                fingers[side].push(Finger { bones, axis: axis.normalize_or(Vec3::X), travel: if k == 0 { def.pulgar } else { def.dedo }, tip: def.yemas[k.min(4)] });
            }
            // the palm: between the wrist and the knuckles of the four fingers
            let wrist = sk.bind[hand].pos;
            let knuckles: Vec<Vec3> = fingers[side].iter().skip(1).map(|f| sk.bind[f.bones[0]].pos).collect();
            if knuckles.len() >= 2 {
                let mid = knuckles.iter().sum::<Vec3>() / knuckles.len() as f32;
                let along = (mid - wrist).normalize_or(Vec3::NEG_Y);
                let across = knuckles[knuckles.len() - 1] - knuckles[0];
                let across = (across - along * across.dot(along)).normalize_or(along.any_orthonormal_vector());
                // the fingers curl toward the palm: about their axis the tips go the palm's way
                let curl = fingers[side].iter().skip(1).map(|f| f.axis.cross(along)).sum::<Vec3>();
                let n = along.cross(across);
                let normal = if n.dot(curl) >= 0.0 { n } else { -n };
                // where the middle of a handle is in a closed hand: across the roots of the
                // fingers, off the palm by its own thickness and the handle's half
                palms[side] = Palm { at: mid - along * 0.012 + normal * (0.0205 + def.mango * 0.5), normal, fingers: along, across };
            } else {
                // no fingers to tell by: the hand's own bone, the palm facing the body
                let tip = sk.parent.iter().position(|p| *p == Some(hand as u16)).map_or(wrist + Vec3::NEG_Y * 0.1, |c| sk.bind[c].pos);
                let along = (tip - wrist).normalize_or(Vec3::NEG_Y);
                let inward = Vec3::new(-wrist.x.signum(), 0.0, 0.0);
                let normal = (inward - along * inward.dot(along)).normalize_or(Vec3::X);
                palms[side] = Palm { at: wrist + along * 0.085 + normal * 0.03, normal, fingers: along, across: along.cross(normal) * if side == 0 { 1.0 } else { -1.0 } };
            }
        }
        // each wrist: the line of its forearm, and what its hand bends and leans about
        let wrists = std::array::from_fn(|side| {
            let (arm, palm) = (arms[side], palms[side]);
            let along = (sk.bind[arm.end].pos - sk.bind[arm.lower].pos).normalize_or(palm.fingers);
            let square = |v: Vec3| (v - along * v.dot(along)).normalize_or(along.any_orthonormal_vector());
            WristAxes { along, flex: along.cross(square(palm.normal)), lean: along.cross(square(palm.across)) }
        });
        let mut points = BTreeMap::new();
        for (name, p) in &def.puntos {
            let bone = sk.find(p.hueso.as_deref().ok_or_else(|| format!("punto '{name}': falta su hueso"))?)?;
            let normal = Vec3::from(p.palma).normalize_or(Vec3::NEG_Y);
            let across = Vec3::from(p.traves).normalize_or(Vec3::X);
            // (kept as a frame: x across, y the palm's way, in the bone's own axes)
            let rot = lunar_core::anim::skeleton::frame_turn(Vec3::Y, Vec3::X, normal, across);
            points.insert(name.clone(), (bone, sk.bind[bone].inverse().then(Xf::new(Vec3::from(p.en), rot))));
        }
        let mut seated_hands = [None, None];
        for (i, name) in def.sentado.manos.iter().enumerate() {
            if let Some(name) = name {
                seated_hands[i] = Some(*points.get(name).ok_or_else(|| format!("sentado: no hay punto '{name}'"))?);
            }
        }
        let bulk = crate::bulk::Bulk::measure(sk, &model.mesh, pelvis, &spine, arms.map(|a| [a.upper, a.lower, a.end]));
        Ok(Rig { def, skeleton, pelvis, spine, legs, toes, arms, clavicles, fingers, palms, wrists, points, seated_hands, bulk })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rig of the game's own astronaut, if its model is there.
    pub fn astronaut() -> Option<(Rig, Rigged)> {
        let root = crate::root();
        let def: RigDef = lunar_core::defs::load(&root.join("assets/defs/rigs/astronauta.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let path = crate::content::find_asset(&root.join("assets"), &format!("models/{}.glb", def.modelo));
        let model = Rigged::load(&path).ok()?;
        let rig = Rig::new(def, &model).unwrap_or_else(|e| panic!("{e}"));
        Some((rig, model))
    }

    #[test]
    fn the_astronaut_is_read_whole_and_its_parts_are_where_a_body_has_them() {
        let Some((rig, model)) = astronaut() else {
            eprintln!("(sin assets/models/astronauta.glb: nada que comprobar)");
            return;
        };
        let sk = &rig.skeleton;
        eprintln!("{} huesos, {} vértices, {} triángulos, materiales {:?}", sk.len(), model.mesh.pos.len(), model.mesh.tris(), model.materials);
        assert!(model.mesh.joints.len() == model.mesh.pos.len() && model.triangle_material.len() == model.mesh.tris());
        // every vertex follows bones there are, with weights that add up
        for (j, w) in model.mesh.joints.iter().zip(&model.mesh.weights) {
            assert!(j.iter().all(|&b| usize::from(b) < sk.len()));
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 0.02, "pesos {w:?}");
        }
        // at rest the palette moves nothing
        let mut pal = Vec::new();
        lunar_core::anim::Pose::rest(sk).palette(sk, &mut pal);
        for (k, m) in pal.iter().enumerate() {
            let id = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
            assert!(m.iter().zip(&id).all(|(a, b)| (a - b).abs() < 2e-3), "en reposo el hueso {} mueve su malla: {m:?}", sk.names[k]);
        }
        // a body: left at +x, feet down, head up, knees over ankles, hands at the ends of arms
        let at = |b: usize| sk.bind[b].pos;
        for side in 0..2 {
            let s = if side == 0 { 1.0 } else { -1.0 };
            let (leg, arm) = (rig.legs[side], rig.arms[side]);
            assert!(at(leg.upper).x * s > 0.05 && at(leg.end).y < 0.2 && at(leg.lower).y > at(leg.end).y + 0.3 && at(leg.upper).y > at(leg.lower).y + 0.3, "pierna {side}");
            assert!(at(arm.upper).x * s > 0.15 && at(arm.upper).y > 1.3 && at(arm.end).y < at(arm.lower).y, "brazo {side}");
            let p = rig.palms[side];
            eprintln!("palma {side}: en {:.3?}, mira a {:.2?}, dedos a {:.2?}, de índice a meñique {:.2?}; {} dedos", p.at, p.normal, p.fingers, p.across, rig.fingers[side].len());
            assert!((p.normal.length() - 1.0).abs() < 1e-3 && p.normal.dot(p.fingers).abs() < 0.05 && p.normal.dot(p.across).abs() < 0.05);
            // (arms down at rest: the palms face the thighs)
            assert!(p.normal.x * s < -0.3, "la palma {side} no mira al cuerpo: {:?}", p.normal);
        }
        assert!(at(rig.pelvis).y > 0.8 && at(rig.pelvis).y < 1.1);
        // the eyes are in the head, over everything else that is not hidden
        let eyes = Vec3::from(rig.def.ojos);
        let seen = model.without(&rig.def.oculto);
        assert!(seen.tris() < model.mesh.tris() && seen.tris() > model.mesh.tris() / 2, "{} de {} triángulos a la vista", seen.tris(), model.mesh.tris());
        let mut near = f32::MAX;
        for t in seen.idx.chunks(3) {
            for &i in t {
                near = near.min(Vec3::from(seen.pos[i as usize]).distance(eyes));
            }
        }
        eprintln!("lo más cerca de los ojos que queda a la vista: {near:.3} m");
        assert!(near > 0.07, "algo de lo que se ve está a {near:.3} m de los ojos");
    }
}
