//! Bodies: meshes that bend with a skeleton whose pose comes from the CPU every frame — the
//! player's own body (its hands on a tool, its feet on the ground), whoever stands near enough
//! to be worth posing bone by bone. (A crowd is drawn by `scene`, from poses baked once.)
//!
//! A mesh is given once (`add_mesh`: its vertices follow up to four bones each); each frame the
//! owner gives the bodies to draw and every bone of each (`lunar_core::anim::BodyScene`). A body
//! may be one mesh to the eye and another to the sun (the player sees no helmet round the
//! camera; the shadow on the ground has its head). A bone's part of a body may be faded
//! (`BodyScene::fades`: one's own arm in front of what one aims at): drawn as a screen of
//! dots so that what is behind shows through, with nothing to sort; its shadow stays whole.
use crate::{
    graph::{self, Draws, PrepareCx, RenderSystem, Stage},
    shader,
    shadow::MAX_CASCADES,
};
use bytemuck::{Pod, Zeroable};
use lunar_core::{
    anim::{BodyDraw, BodyScene},
    mesh::Mesh,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VertexGpu {
    pos: [f32; 3],
    nrm: [f32; 3],
    /// sRGB.
    color: [u8; 4],
    /// Roughness, metalness, glow.
    mat: [u8; 4],
    joints: [u8; 4],
    weights: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct BodyGpu {
    pos: [f32; 4],
    rot: [f32; 4],
    params: [f32; 4],
}

pub struct BodiesGpu {
    main: wgpu::RenderPipeline,
    depth: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    group: Option<wgpu::BindGroup>,
    vertices: Option<(wgpu::Buffer, wgpu::Buffer)>,
    verts: Vec<VertexGpu>,
    idx: Vec<u32>,
    /// Per mesh: (first index, index count, base vertex).
    ranges: Vec<(u32, u32, i32)>,
    bodies: wgpu::Buffer,
    body_cap: usize,
    bones: wgpu::Buffer,
    bone_cap: usize,
    fades: wgpu::Buffer,
    draws: Vec<BodyDraw>,
    staged: Vec<BodyGpu>,
    staged_bones: Vec<[f32; 12]>,
    /// One per bone (as many as `staged_bones`), 0 where the scene says nothing.
    staged_fades: Vec<f32>,
    /// The fades last written are all 0 (none to write again).
    clear: bool,
}

fn storage(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes.max(256) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// Four weights as bytes that add up to 255 (so a vertex is never left short of its bones).
fn weights(w: [f32; 4]) -> [u8; 4] {
    let sum: f32 = w.iter().sum();
    if sum <= 1e-6 {
        return [255, 0, 0, 0];
    }
    let mut out = w.map(|x| (x / sum * 255.0).round().clamp(0.0, 255.0) as u8);
    let total: i32 = out.iter().map(|&x| i32::from(x)).sum();
    let big = (0..4).max_by_key(|&k| out[k]).unwrap_or(0);
    out[big] = (i32::from(out[big]) + 255 - total).clamp(0, 255) as u8;
    out
}

impl BodiesGpu {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout) -> BodiesGpu {
        let module = shader(device, "bodies", &[include_str!("shaders/common.wgsl"), include_str!("shaders/bodies.wgsl")]);
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("bodies"), entries: &[entry(0), entry(1), entry(2)] });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("bodies"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        const ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Uint8x4, 5 => Unorm8x4];
        let vb = [Some(wgpu::VertexBufferLayout { array_stride: 40, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS })];
        BodiesGpu {
            main: crate::pipes::scene(device, &pl, &module, "body_vs", Some("body_fs"), &vb, Some(wgpu::Face::Back)),
            depth: crate::pipes::shadow(device, &pl, &module, "body_depth_vs", &vb, None),
            layout,
            group: None,
            vertices: None,
            verts: Vec::new(),
            idx: Vec::new(),
            ranges: Vec::new(),
            bodies: storage(device, "bodies", 48 * 8),
            body_cap: 8,
            bones: storage(device, "body bones", 48 * 256),
            bone_cap: 256,
            fades: storage(device, "body fades", 4 * 256),
            draws: Vec::new(),
            staged: Vec::new(),
            staged_bones: Vec::new(),
            staged_fades: Vec::new(),
            clear: false,
        }
    }

    /// One more mesh a body may be drawn as: its number (`BodyDraw::mesh`). Its vertices must
    /// say which bones they follow (`Mesh::joints`, `Mesh::weights`); None if they do not.
    pub fn add_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh) -> Option<u16> {
        use wgpu::util::DeviceExt;
        if mesh.joints.len() != mesh.pos.len() || mesh.weights.len() != mesh.pos.len() {
            return None;
        }
        let id = u16::try_from(self.ranges.len()).ok()?;
        self.ranges.push((self.idx.len() as u32, mesh.idx.len() as u32, self.verts.len() as i32));
        for i in 0..mesh.pos.len() {
            let m = mesh.mat[i];
            self.verts.push(VertexGpu { pos: mesh.pos[i], nrm: mesh.nrm[i], color: [m.albedo[0], m.albedo[1], m.albedo[2], 255], mat: [m.rough, m.metal, m.emissive, 0], joints: mesh.joints[i], weights: weights(mesh.weights[i]) });
        }
        self.idx.extend_from_slice(&mesh.idx);
        self.vertices = Some((
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("body meshes"), contents: bytemuck::cast_slice(&self.verts), usage: wgpu::BufferUsages::VERTEX }),
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("body indices"), contents: bytemuck::cast_slice(&self.idx), usage: wgpu::BufferUsages::INDEX }),
        ));
        Some(id)
    }

    /// This frame's bodies (copied: the caller keeps its scene).
    pub fn set(&mut self, s: &BodyScene) {
        self.draws.clone_from(&s.bodies);
        self.staged_bones.clone_from(&s.bones);
        self.staged_fades.clear();
        self.staged_fades.extend(s.fades.iter().take(s.bones.len()));
        self.staged_fades.resize(s.bones.len(), 0.0);
    }
}

impl RenderSystem for BodiesGpu {
    fn stages(&self) -> u8 {
        if self.draws.is_empty() || self.vertices.is_none() { 0 } else { graph::OPAQUE | graph::SHADOW }
    }

    fn prepare(&mut self, cx: &PrepareCx) {
        let eye = cx.view.eye;
        self.staged.clear();
        for b in &self.draws {
            let rel = (b.pos - eye).as_vec3();
            self.staged.push(BodyGpu { pos: [rel.x, rel.y, rel.z, b.scale], rot: b.rot.normalize().to_array(), params: [f32::from(u8::from(b.inside)), b.first as f32, 0.0, 0.0] });
        }
        let mut rebind = self.group.is_none();
        if self.staged.len() > self.body_cap {
            self.body_cap = self.staged.len().next_power_of_two();
            self.bodies = storage(cx.device, "bodies", 48 * self.body_cap);
            rebind = true;
        }
        if self.staged_bones.len() > self.bone_cap {
            self.bone_cap = self.staged_bones.len().next_power_of_two();
            self.bones = storage(cx.device, "body bones", 48 * self.bone_cap);
            self.fades = storage(cx.device, "body fades", 4 * self.bone_cap);
            (rebind, self.clear) = (true, false);
        }
        if rebind {
            self.group = Some(cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bodies"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.bodies.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.bones.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: self.fades.as_entire_binding() },
                ],
            }));
        }
        if !self.staged.is_empty() {
            cx.queue.write_buffer(&self.bodies, 0, bytemuck::cast_slice(&self.staged));
        }
        if !self.staged_bones.is_empty() {
            cx.queue.write_buffer(&self.bones, 0, bytemuck::cast_slice(&self.staged_bones));
            // (nothing faded, frame after frame: written once)
            let clear = self.staged_fades.iter().all(|&f| f == 0.0);
            if !(clear && self.clear) {
                cx.queue.write_buffer(&self.fades, 0, bytemuck::cast_slice(&self.staged_fades));
            }
            self.clear = clear;
        }
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
        let mut d = Draws::default();
        let (Some(group), Some((vertices, indices))) = (&self.group, &self.vertices) else { return d };
        let (pipe, shadow) = match stage {
            Stage::Opaque => (&self.main, false),
            // they move: with the moving casters, every frame
            Stage::Shadow { cascade, moving: true, .. } if cascade < MAX_CASCADES => (&self.depth, true),
            _ => return d,
        };
        let mut bound = false;
        for (i, b) in self.draws.iter().enumerate() {
            let Some(&(first, count, base)) = (if shadow { b.shadow } else { b.mesh }).and_then(|m| self.ranges.get(usize::from(m))) else { continue };
            if !bound {
                pass.set_pipeline(pipe);
                pass.set_bind_group(1, group, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                bound = true;
            }
            pass.draw_indexed(first..first + count, base, i as u32..i as u32 + 1);
            d.calls += 1;
            d.triangles += u64::from(count / 3);
        }
        d
    }

    fn gpu_bytes(&self) -> u64 {
        self.bodies.size() + self.bones.size() + self.fades.size() + self.vertices.as_ref().map_or(0, |(v, i)| v.size() + i.size())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn weights_always_add_up() {
        for w in [[0.5, 0.5, 0.0, 0.0], [0.333, 0.333, 0.334, 0.0], [1.0, 0.0, 0.0, 0.0], [0.1, 0.2, 0.3, 0.4], [0.0; 4], [0.004, 0.996, 0.0, 0.0]] {
            let b = super::weights(w);
            assert_eq!(b.iter().map(|&x| u32::from(x)).sum::<u32>(), 255, "{w:?} -> {b:?}");
        }
    }
}
