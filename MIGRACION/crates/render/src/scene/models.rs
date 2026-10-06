//! Model registry: every LOD of every model lives in one vertex/index mega-buffer per family
//! (rigid, skinned); a model is a row of LODs with screen-size thresholds. Adding content is
//! registering a model — the culling and drawing never change.
use bytemuck::{Pod, Zeroable};
use lunar_core::mesh::Mesh;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Rigid = 0,
    Skinned = 1,
    Impostor = 2,
}
pub const FAMILIES: usize = 3;
pub const MAX_LODS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct RigidVertex {
    pub pos: [f32; 3],
    pub nrm: [i8; 4],
    pub color: [u8; 4],
    pub params: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub pos: [f32; 3],
    pub nrm: [i8; 4],
    pub color: [u8; 4],
    pub params: [u8; 4],
    pub joints: [u8; 4],
    pub weights: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct DrawCmd {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct ModelGpu {
    pub bounds: [f32; 4],
    pub thresholds: [f32; MAX_LODS],
    pub cmds: [u32; MAX_LODS],
    pub regions: [u32; MAX_LODS],
    /// lod count, impostor lod (or 99), impostor atlas layer, -
    pub info: [u32; 4],
}

#[derive(Clone, Debug)]
pub struct Lod {
    pub family: Family,
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    /// Smallest on-screen radius (px) this LOD is used at.
    pub min_px: f32,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub lods: Vec<Lod>,
    pub center: [f32; 3],
    pub radius: f32,
    pub impostor_layer: Option<u32>,
    /// Filled by `layout`.
    pub cmds: [u32; MAX_LODS],
    pub regions: [u32; MAX_LODS],
    pub capacity: u32,
    /// Glow sprites: first, count.
    pub glows: (u32, u32),
}

fn snorm(v: f32) -> i8 {
    (v.clamp(-1.0, 1.0) * 127.0).round() as i8
}

#[derive(Default)]
pub struct Geometry {
    pub rigid_v: Vec<RigidVertex>,
    pub rigid_i: Vec<u32>,
    pub skinned_v: Vec<SkinnedVertex>,
    pub skinned_i: Vec<u32>,
}

impl Geometry {
    pub fn add(&mut self, mesh: &Mesh, family: Family) -> (u32, u32, i32) {
        let vtx = |i: usize| {
            let n = mesh.nrm[i];
            let m = mesh.mat[i];
            (
                mesh.pos[i],
                [snorm(n[0]), snorm(n[1]), snorm(n[2]), 0],
                [m.albedo[0], m.albedo[1], m.albedo[2], 255],
                [m.rough, m.metal, m.emissive, m.panel],
            )
        };
        match family {
            Family::Rigid | Family::Impostor => {
                let base = self.rigid_v.len() as i32;
                let first = self.rigid_i.len() as u32;
                for i in 0..mesh.pos.len() {
                    let (pos, nrm, color, params) = vtx(i);
                    self.rigid_v.push(RigidVertex { pos, nrm, color, params });
                }
                self.rigid_i.extend_from_slice(&mesh.idx);
                (first, mesh.idx.len() as u32, base)
            }
            Family::Skinned => {
                let base = self.skinned_v.len() as i32;
                let first = self.skinned_i.len() as u32;
                for i in 0..mesh.pos.len() {
                    let (pos, nrm, color, params) = vtx(i);
                    let w = mesh.weights[i];
                    let sum: f32 = w.iter().sum::<f32>().max(1e-6);
                    self.skinned_v.push(SkinnedVertex {
                        pos,
                        nrm,
                        color,
                        params,
                        joints: mesh.joints[i],
                        weights: w.map(|x| ((x / sum) * 255.0).round() as u8),
                    });
                }
                self.skinned_i.extend_from_slice(&mesh.idx);
                (first, mesh.idx.len() as u32, base)
            }
        }
    }
}

/// Commands grouped by family (one multi-draw each); list regions sized by instance counts.
pub struct Layout {
    pub cmds: Vec<DrawCmd>,
    pub family_ranges: [(u32, u32); FAMILIES],
    /// Visible-list entries per view.
    pub stride: u32,
}

pub fn layout(models: &mut [Model], counts: &[u32]) -> Layout {
    let mut cmds = Vec::new();
    let mut family_ranges = [(0u32, 0u32); FAMILIES];
    for fam in [Family::Rigid, Family::Skinned, Family::Impostor] {
        let start = cmds.len() as u32;
        for m in models.iter_mut() {
            for (l, lod) in m.lods.iter().enumerate() {
                if lod.family == fam {
                    m.cmds[l] = cmds.len() as u32;
                    cmds.push(DrawCmd { index_count: lod.index_count, instance_count: 0, first_index: lod.first_index, base_vertex: lod.base_vertex, first_instance: 0 });
                }
            }
        }
        family_ranges[fam as usize] = (start, cmds.len() as u32 - start);
    }
    let mut region = 0u32;
    for (k, m) in models.iter_mut().enumerate() {
        m.capacity = counts.get(k).copied().unwrap_or(0);
        for l in 0..m.lods.len() {
            m.regions[l] = region;
            cmds[m.cmds[l] as usize].first_instance = region;
            region += m.capacity;
        }
    }
    Layout { cmds, family_ranges, stride: region.max(1) }
}

pub fn model_gpu(m: &Model) -> ModelGpu {
    let mut thresholds = [0f32; MAX_LODS];
    for (l, lod) in m.lods.iter().enumerate() {
        thresholds[l] = lod.min_px;
    }
    let impostor = m.lods.iter().position(|l| l.family == Family::Impostor).map_or(99, |p| p as u32);
    ModelGpu {
        bounds: [m.center[0], m.center[1], m.center[2], m.radius],
        thresholds,
        cmds: m.cmds,
        regions: m.regions,
        info: [m.lods.len() as u32, impostor, m.impostor_layer.unwrap_or(0), (m.glows.0 << 4) | m.glows.1.min(15)],
    }
}
