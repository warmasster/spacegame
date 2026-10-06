//! A structure as data (`structures/builds/<id>.jsonc`): parts of the catalog placed in its own
//! frame (+Y up, metres), repeated in rows and rings, and the joints between them, listed or
//! found where parts touch. Resolved into a `Blueprint` the runtime builds from.
use super::catalog::Catalog;
use crate::defs::{self, DefError};
use glam::{Affine3A, EulerRot, Quat, Vec3};
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatDef {
    pub count: u32,
    /// Move per copy (m), in the structure's frame.
    #[serde(default)]
    pub step: [f32; 3],
    /// Turn per copy round the structure's origin (deg, XYZ): rings.
    #[serde(default)]
    pub turn: [f32; 3],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedDef {
    pub id: String,
    /// Part kind of the catalog.
    pub part: String,
    #[serde(default)]
    pub at: [f32; 3],
    /// Euler XYZ (deg).
    #[serde(default)]
    pub rot: [f32; 3],
    /// Copies (each nests in the previous: rows of rows, rings of columns); copy k of a part is
    /// named `id#k` (`id#i.j` with two).
    #[serde(default)]
    pub repeat: Vec<RepeatDef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointDef {
    pub a: String,
    pub b: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureDef {
    pub name: String,
    /// Fixed to the ground (bases, buildings): what stays attached to the anchor never falls.
    #[serde(default)]
    pub anchored: bool,
    pub parts: Vec<PlacedDef>,
    #[serde(default)]
    pub joints: Vec<JointDef>,
    /// Joint kind for every pair of parts that touch (none: only the listed joints).
    #[serde(default)]
    pub auto_joint: Option<String>,
    /// How close counts as touching (m).
    #[serde(default = "touch")]
    pub touch: f32,
}

fn touch() -> f32 {
    0.02
}

#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub kind: u16,
    /// Part frame → structure frame (at rest).
    pub local: Affine3A,
    /// The joint that moves it (0: none).
    pub bone: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Joined {
    pub a: u32,
    pub b: u32,
    pub kind: u16,
    /// Where (structure frame).
    pub at: Vec3,
    /// Networks it carries: the ports both parts share.
    pub networks: u32,
}

#[derive(Clone)]
pub struct Blueprint {
    pub name: String,
    pub anchored: bool,
    pub ids: Vec<String>,
    pub parts: Vec<Placed>,
    pub joints: Vec<Joined>,
}

/// Every placement a `PlacedDef` makes: (name, transform).
fn expand(p: &PlacedDef) -> Vec<(String, Affine3A)> {
    let deg = |r: [f32; 3]| Quat::from_euler(EulerRot::XYZ, r[0].to_radians(), r[1].to_radians(), r[2].to_radians());
    let base = Affine3A::from_rotation_translation(deg(p.rot), Vec3::from_array(p.at));
    let mut out = vec![(p.id.clone(), base)];
    for (level, r) in p.repeat.iter().enumerate() {
        let mut next = Vec::with_capacity(out.len() * r.count as usize);
        for (name, t) in &out {
            for k in 0..r.count {
                let kf = k as f32;
                let turn = Affine3A::from_quat(deg([r.turn[0] * kf, r.turn[1] * kf, r.turn[2] * kf]));
                let step = Affine3A::from_translation(Vec3::from_array(r.step) * kf);
                let n = if level == 0 { format!("{name}#{k}") } else { format!("{name}.{k}") };
                next.push((n, turn * step * *t));
            }
        }
        out = next;
    }
    out
}

impl Blueprint {
    pub fn load(dir: &Path, catalog: &Catalog) -> Result<Vec<(String, Blueprint)>, DefError> {
        let defs: Vec<(String, StructureDef)> = defs::load_dir(dir)?;
        defs.into_iter().map(|(id, d)| Blueprint::new(&d, catalog).map(|b| (id.clone(), b)).map_err(|e| DefError::new(format!("structure {id}"), e))).collect()
    }

    pub fn new(d: &StructureDef, cat: &Catalog) -> Result<Blueprint, String> {
        let mut ids = Vec::new();
        let mut parts = Vec::new();
        for p in &d.parts {
            let kind = cat.part(&p.part).ok_or_else(|| format!("part {}: unknown kind '{}'", p.id, p.part))?;
            for (name, local) in expand(p) {
                if ids.contains(&name) {
                    return Err(format!("two parts named '{name}'"));
                }
                ids.push(name);
                parts.push(Placed { kind, local, bone: 0 });
            }
        }
        let shapes: Vec<_> = parts.iter().map(|p| cat.parts[usize::from(p.kind)].shape.transformed(p.local)).collect();
        let spheres: Vec<_> = shapes.iter().map(|s| s.sphere()).collect();
        let shared = |a: usize, b: usize| cat.parts[usize::from(parts[a].kind)].ports & cat.parts[usize::from(parts[b].kind)].ports;
        let mut joints = Vec::new();
        for j in &d.joints {
            let find = |n: &str| ids.iter().position(|i| i == n).ok_or_else(|| format!("joint {}-{}: no part '{n}'", j.a, j.b));
            let (a, b) = (find(&j.a)?, find(&j.b)?);
            let kind = cat.joint(&j.kind).ok_or_else(|| format!("joint {}-{}: unknown kind '{}'", j.a, j.b, j.kind))?;
            let at = (spheres[a].0 + spheres[b].0) * 0.5;
            joints.push(Joined { a: a as u32, b: b as u32, kind, at, networks: shared(a, b) });
        }
        if let Some(k) = &d.auto_joint {
            let kind = cat.joint(k).ok_or_else(|| format!("auto_joint: unknown kind '{k}'"))?;
            for a in 0..parts.len() {
                for b in a + 1..parts.len() {
                    if spheres[a].0.distance(spheres[b].0) > spheres[a].1 + spheres[b].1 + d.touch {
                        continue;
                    }
                    if joints.iter().any(|j: &Joined| (j.a.min(j.b), j.a.max(j.b)) == (a as u32, b as u32)) {
                        continue;
                    }
                    // where they touch: the corners of each within `touch` of the other
                    let mut sum = Vec3::ZERO;
                    let mut n = 0;
                    for v in shapes[a].verts().filter(|&v| shapes[b].distance(v) <= d.touch).chain(shapes[b].verts().filter(|&v| shapes[a].distance(v) <= d.touch)) {
                        sum += v;
                        n += 1;
                    }
                    if n > 0 {
                        joints.push(Joined { a: a as u32, b: b as u32, kind, at: sum / n as f32, networks: shared(a, b) });
                    }
                }
            }
        }
        if parts.is_empty() {
            return Err("no parts".into());
        }
        Ok(Blueprint { name: d.name.clone(), anchored: d.anchored, ids, parts, joints })
    }
}
