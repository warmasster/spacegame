//! glTF/GLB models declared in data (`"kind": "gltf"`): one mesh (materials folded into vertices),
//! simplified far LODs, and — for skinned humanoids — procedural walk/idle clips baked into joint
//! matrices for the GPU (files like astronaut.glb carry no animations).
use glam::{Affine3A, Mat4, Quat, Vec3};
use lunar_core::{
    mesh::{Material, Mesh},
    model::{Animation, BuiltModel, GltfDef, ShapeBuilder},
    simplify,
};
use std::{
    collections::HashMap,
    error::Error,
    f32::consts::TAU,
    path::{Path, PathBuf},
};

pub const WALK_FRAMES: u32 = 32;
pub const IDLE_FRAMES: u32 = 32;
/// The procedural clip set `bake` knows (walk, idle) for a humanoid rig.
pub const WALKER: &str = "caminante";

/// A glTF model definition with its file resolved.
pub struct GltfModel<'a> {
    pub def: &'a GltfDef,
    pub path: PathBuf,
}

impl ShapeBuilder for GltfModel<'_> {
    fn build(&self) -> Result<BuiltModel, String> {
        load(&self.path, self.def).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

struct Skeleton {
    parent: Vec<Option<usize>>,
    local: Vec<Affine3A>,
    order: Vec<usize>,
    joint_nodes: Vec<usize>,
    ibm: Vec<Mat4>,
    names: Vec<String>,
}

impl Skeleton {
    fn world_bind(&self) -> Vec<Affine3A> {
        let mut w = vec![Affine3A::IDENTITY; self.local.len()];
        for &n in &self.order {
            w[n] = self.parent[n].map_or(Affine3A::IDENTITY, |p| w[p]) * self.local[n];
        }
        w
    }
    fn node(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
}

fn material(m: &gltf::Material, tints: &HashMap<String, [u8; 3]>) -> Material {
    let pbr = m.pbr_metallic_roughness();
    let c = pbr.base_color_factor();
    let srgb = |v: f32| (v.max(0.0).powf(1.0 / 2.2) * 255.0).round().min(255.0) as u8;
    let e = m.emissive_factor();
    let strength = m.emissive_strength().unwrap_or(1.0);
    let emissive = (e[0].max(e[1]).max(e[2]) * strength / 8.0 * 255.0).min(255.0) as u8;
    let mut albedo = [srgb(c[0]), srgb(c[1]), srgb(c[2])];
    if let Some(t) = m.name().and_then(|n| tints.get(n)) {
        albedo = *t;
    }
    Material { albedo, rough: (pbr.roughness_factor() * 255.0) as u8, metal: (pbr.metallic_factor() * 255.0) as u8, emissive, panel: 0, finish: 0 }
}

fn load(path: &Path, def: &GltfDef) -> Result<BuiltModel, Box<dyn Error>> {
    let bytes = std::fs::read(path)?;
    let gltf = gltf::Gltf::from_slice_without_validation(&bytes)?;
    let blob = gltf.blob.as_deref().ok_or("GLB without binary chunk")?;
    let doc = &gltf.document;
    // node graph
    let count = doc.nodes().count();
    let mut parent = vec![None; count];
    let mut local = vec![Affine3A::IDENTITY; count];
    let mut names = vec![String::new(); count];
    for n in doc.nodes() {
        let (t, r, s) = n.transform().decomposed();
        local[n.index()] = Affine3A::from_scale_rotation_translation(Vec3::from(s), Quat::from_array(r), Vec3::from(t));
        names[n.index()] = n.name().unwrap_or("").to_string();
        for c in n.children() {
            parent[c.index()] = Some(n.index());
        }
    }
    let mut order = Vec::with_capacity(count);
    let mut stack: Vec<usize> = (0..count).filter(|n| parent[*n].is_none()).collect();
    while let Some(n) = stack.pop() {
        order.push(n);
        for c in doc.nodes().nth(n).expect("node").children() {
            stack.push(c.index());
        }
    }
    let skin = doc.skins().next().ok_or("no skin")?;
    let joint_nodes: Vec<usize> = skin.joints().map(|j| j.index()).collect();
    let ibm: Vec<Mat4> = skin
        .reader(|_| Some(blob))
        .read_inverse_bind_matrices()
        .ok_or("no inverse bind matrices")?
        .map(|m| Mat4::from_cols_array_2d(&m))
        .collect();
    let skel = Skeleton { parent, local, order, joint_nodes, ibm, names };
    // one mesh: every primitive of every skinned node
    let mut mesh = Mesh::default();
    let mut interior: Vec<std::ops::Range<usize>> = Vec::new();
    for node in doc.nodes() {
        let Some(m) = node.mesh() else { continue };
        for prim in m.primitives() {
            let r = prim.reader(|_| Some(blob));
            let base = mesh.pos.len() as u32;
            let mat = material(&prim.material(), &def.tints);
            let inner = prim.material().name().is_some_and(|n| def.hidden_far.iter().any(|h| h == n));
            let idx_start = mesh.idx.len();
            let pos: Vec<[f32; 3]> = r.read_positions().ok_or("no positions")?.collect();
            let nrm: Vec<[f32; 3]> = r.read_normals().ok_or("no normals")?.collect();
            let colors: Vec<[f32; 4]> = r.read_colors(0).map(|c| c.into_rgba_f32().collect()).unwrap_or_else(|| vec![[1.0; 4]; pos.len()]);
            let joints: Vec<[u16; 4]> = r.read_joints(0).ok_or("no joints")?.into_u16().collect();
            let weights: Vec<[f32; 4]> = r.read_weights(0).ok_or("no weights")?.into_f32().collect();
            for i in 0..pos.len() {
                let mut m = mat;
                for k in 0..3 {
                    m.albedo[k] = (f32::from(m.albedo[k]) * colors[i][k].powf(1.0 / 2.2)).round().min(255.0) as u8;
                }
                mesh.vertex(Vec3::from(pos[i]), Vec3::from(nrm[i]), m);
                mesh.joints.push(joints[i].map(|j| j as u8));
                mesh.weights.push(weights[i]);
            }
            match r.read_indices() {
                Some(ix) => mesh.idx.extend(ix.into_u32().map(|i| i + base)),
                None => mesh.idx.extend(base..base + pos.len() as u32),
            }
            if inner {
                interior.push(idx_start..mesh.idx.len());
            }
        }
    }
    // LODs by clustering, to budgets
    // far LODs never show the hidden parts (insides): clustering would push them through the shell
    let mut outer = mesh.clone();
    outer.idx = mesh.idx.chunks(3).enumerate().filter(|(t, _)| !interior.iter().any(|r| r.contains(&(t * 3)))).flat_map(|(_, t)| t.to_vec()).collect();
    let lods = def
        .lods
        .iter()
        .map(|l| match l.budget {
            None => (mesh.clone(), l.min_px),
            Some(budget) => (simplify::decimate(&outer, budget), l.min_px),
        })
        .collect();
    let animation = match def.animation.as_deref() {
        None => None,
        Some(WALKER) => {
            let (frames, clips) = bake(&skel);
            Some(Animation { joints: skel.joint_nodes.len() as u32, frames, clips })
        }
        Some(other) => return Err(format!("unknown animation '{other}' (known: {WALKER})").into()),
    };
    Ok(BuiltModel { lods, skinned: true, glows: Vec::new(), animation })
}

/// World-space rotations (in the bind frame) per joint name, and a pelvis offset, for one pose.
fn pose(walk: bool, t: f32, side: Vec3, fwd: Vec3, up: Vec3) -> (Vec<(&'static str, Quat)>, Vec3) {
    let a = t * TAU;
    let rs = |deg: f32| Quat::from_axis_angle(side, -deg.to_radians());
    let tw = |deg: f32| Quat::from_axis_angle(up, deg.to_radians());
    let roll = |deg: f32| Quat::from_axis_angle(fwd, deg.to_radians());
    if walk {
        let s = a.sin();
        let knee = |ph: f32| 6.0 + 42.0 * (ph.sin().max(0.0)).powf(1.5);
        (
            vec![
                ("thigh.L", rs(26.0 * s)),
                ("thigh.R", rs(-26.0 * s)),
                ("shin.L", rs(-knee(a - 0.9))),
                ("shin.R", rs(-knee(a + std::f32::consts::PI - 0.9))),
                ("foot.L", rs(10.0 * (a - 0.4).sin())),
                ("foot.R", rs(-10.0 * (a - 0.4).sin())),
                ("upperarm.L", rs(-18.0 * s)),
                ("upperarm.R", rs(18.0 * s)),
                ("forearm.L", rs(22.0 + 10.0 * s)),
                ("forearm.R", rs(22.0 - 10.0 * s)),
                ("spine", tw(5.0 * s) * rs(4.0)),
                ("chest", tw(-3.0 * s)),
                ("pelvis", roll(3.0 * s)),
                ("head", tw(-2.0 * s)),
            ],
            up * (-0.035 * (2.0 * a).cos() - 0.02),
        )
    } else {
        let b = a.sin();
        (
            vec![
                ("chest", rs(-1.5 * b)),
                ("head", tw(12.0 * (a * 0.5).sin()) * rs(3.0 * b)),
                ("upperarm.L", rs(3.0 * b) * roll(-4.0)),
                ("upperarm.R", rs(-3.0 * b) * roll(4.0)),
                ("forearm.L", rs(14.0)),
                ("forearm.R", rs(14.0)),
                ("shin.L", rs(-4.0)),
                ("shin.R", rs(-4.0)),
            ],
            up * (0.004 * b),
        )
    }
}

fn bake(sk: &Skeleton) -> (Vec<[f32; 4]>, Vec<(u32, u32)>) {
    let bind = sk.world_bind();
    let at = |name: &str| sk.node(name).map(|n| Vec3::from(bind[n].translation)).unwrap_or_default();
    // the suit's own frame: up the spine, forward where the toes point
    let up = Vec3::Y;
    let toe = (at("toe.L") + at("toe.R")) * 0.5 - (at("foot.L") + at("foot.R")) * 0.5;
    let fwd = (toe - up * toe.dot(up)).normalize_or(Vec3::Z);
    let side = up.cross(fwd);
    // turn the whole suit to face +Z (the instances' forward)
    let face = Quat::from_rotation_arc(fwd, Vec3::Z);
    let face = Mat4::from_quat(face);
    let pelvis = sk.node("pelvis");
    let mut frames = Vec::new();
    let mut clips = Vec::new();
    for (walk, n) in [(true, WALK_FRAMES), (false, IDLE_FRAMES)] {
        clips.push(((frames.len() / (sk.joint_nodes.len() * 3)) as u32, n));
        for f in 0..n {
            let (rots, lift) = pose(walk, f as f32 / n as f32, side, fwd, up);
            let mut world = vec![Affine3A::IDENTITY; sk.local.len()];
            for &node in &sk.order {
                let mut local = sk.local[node];
                if let Some((_, q)) = rots.iter().find(|(name, _)| sk.names[node] == *name) {
                    // conjugate the bind-frame rotation into the joint's local frame
                    let (_, rw, _) = bind[node].to_scale_rotation_translation();
                    let q_local = rw.inverse() * *q * rw;
                    local = local * Affine3A::from_quat(q_local);
                }
                if Some(node) == pelvis {
                    let p = sk.parent[node].map_or(Affine3A::IDENTITY, |p| bind[p]);
                    local.translation += glam::Vec3A::from(p.inverse().transform_vector3(lift));
                }
                world[node] = sk.parent[node].map_or(Affine3A::IDENTITY, |p| world[p]) * local;
            }
            for (j, &node) in sk.joint_nodes.iter().enumerate() {
                let m = face * Mat4::from(world[node]) * sk.ibm[j];
                let r = m.transpose();
                frames.push(r.x_axis.to_array());
                frames.push(r.y_axis.to_array());
                frames.push(r.z_axis.to_array());
            }
        }
    }
    (frames, clips)
}

#[cfg(test)]
mod tests {
    use lunar_core::{defs, model::{ModelDef, ModelSource}};

    /// Diagnostic: per LOD, the share of surface that is dark, metallic or wound backwards.
    #[test]
    fn far_lods_keep_the_suit_colours() {
        let root = crate::root();
        let def: ModelDef = defs::load(&defs::file(&root.join("assets/defs/models"), "astronauta")).unwrap();
        let ModelSource::Gltf(g) = &def.source else { panic!() };
        let path = crate::content::find_asset(&root.join("assets"), &g.path);
        let m = super::load(&path, g).unwrap();
        let mut base = None;
        for (l, (mesh, _)) in m.lods.iter().enumerate() {
            let (mut area, mut dark, mut metal, mut flipped) = (0f32, 0f32, 0f32, 0f32);
            for t in mesh.idx.chunks(3) {
                let p = [0, 1, 2].map(|k| glam::Vec3::from_array(mesh.pos[t[k] as usize]));
                let face = (p[1] - p[0]).cross(p[2] - p[0]);
                let a = face.length() * 0.5;
                let n: glam::Vec3 = t.iter().map(|&i| glam::Vec3::from_array(mesh.nrm[i as usize])).sum();
                let mat = mesh.mat[t[0] as usize];
                let lum = mat.albedo.iter().map(|&c| f32::from(c)).sum::<f32>() / 3.0;
                area += a;
                if lum < 70.0 {
                    dark += a;
                }
                if mat.metal > 128 {
                    metal += a;
                }
                if face.dot(n) < 0.0 {
                    flipped += a;
                }
            }
            let r = [dark / area, metal / area, flipped / area];
            println!("LOD {l}: {} tris  dark {:.3}  metal {:.3}  flipped {:.3}", mesh.tris(), r[0], r[1], r[2]);
            let b = *base.get_or_insert(r);
            // colours must hold at every LOD; winding must stay clean on the LODs seen as more than
            // a couple of pixels (the last ones are a pixel or two tall)
            let wind = if mesh.tris() >= 2000 { 0.02 } else { 0.08 };
            if l > 0 {
                assert!(r[0] < b[0] + 0.04 && r[1] < b[1] + 0.04 && r[2] < wind, "LOD {l} off from LOD 0: {r:?} vs {b:?}");
            }
        }
    }
}
