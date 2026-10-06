//! Props: what structures show besides their mesh, rebuilt every frame by whoever owns them —
//! panel controls posed as they stand, gauge needles, lamps, screens, actuator rods, the tool in
//! the hands — as instances of a few meshes (four primitives, and any model given with
//! `add_mesh`: it wears its own colours), and text (silkscreen, displays, screens) as quads of a
//! signed-distance font atlas. Everything comes in the frame of its owner (a structure's origin and
//! turn, f64) and goes to the GPU camera-relative, so it stays still anywhere on any body.
use crate::{
    graph::{self, Draws, PrepareCx, RenderSystem, Stage},
    shader,
};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3};
use lunar_core::mesh::{Material, Mesh, cone, cuboid, sphere};

pub use lunar_core::props::{BOX, CONE, CYLINDER, DecalQuad, GlyphQuad, Lamp, PRIMITIVES, Prop, PropFrame, PropScene, SPHERE};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct PropGpu {
    pos: [f32; 4],
    rot: [f32; 4],
    scale: [f32; 4],
    mat: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct GlyphGpu {
    center: [f32; 4],
    u: [f32; 4],
    v: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VertexGpu {
    pos: [f32; 3],
    nrm: [f32; 3],
    /// sRGB; a model's own colour (a primitive's is white: its instance's shows).
    color: [u8; 4],
    /// Roughness, metalness, glow; w: 255 they are the surface's own (a model), 0 the instance's.
    mat: [u8; 4],
}

fn lin(c: u8) -> f32 {
    (f32::from(c) / 255.0).powf(2.2)
}

pub struct PropsGpu {
    prop_pipe: wgpu::RenderPipeline,
    glyph_pipe: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    group: Option<wgpu::BindGroup>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// Every mesh's vertices and indices (kept: a mesh added later makes the buffers anew).
    verts: Vec<VertexGpu>,
    idx: Vec<u32>,
    /// Per mesh: (first index, index count, base vertex).
    ranges: Vec<(u32, u32, i32)>,
    props: wgpu::Buffer,
    prop_cap: usize,
    glyphs: wgpu::Buffer,
    glyph_cap: usize,
    atlas: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Posters, signs, logos (RGBA, mipmapped); a blank one until set.
    decals: (wgpu::Texture, wgpu::TextureView),
    sampler: wgpu::Sampler,
    scene: PropScene,
    staged: Vec<PropGpu>,
    staged_glyphs: Vec<GlyphGpu>,
    /// Instances of each mesh this frame (start, count) in `staged`.
    batches: Vec<(u32, u32)>,
    counts: Vec<usize>,
}

fn mesh_buffers(device: &wgpu::Device, verts: &[VertexGpu], idx: &[u32]) -> (wgpu::Buffer, wgpu::Buffer) {
    use wgpu::util::DeviceExt;
    (
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("prop meshes"), contents: bytemuck::cast_slice(verts), usage: wgpu::BufferUsages::VERTEX }),
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("prop indices"), contents: bytemuck::cast_slice(idx), usage: wgpu::BufferUsages::INDEX }),
    )
}

/// An RGBA8 sRGB texture of `size`² with its mip chain (each level the 2×2 average of the one
/// above).
fn rgba_texture(device: &wgpu::Device, queue: &wgpu::Queue, size: u32, data: &[u8]) -> (wgpu::Texture, wgpu::TextureView) {
    let mips = 32 - size.max(1).leading_zeros();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("decals"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level = data.to_vec();
    let mut s = size;
    for mip in 0..mips {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: mip, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &level,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * 4), rows_per_image: Some(s) },
            wgpu::Extent3d { width: s, height: s, depth_or_array_layers: 1 },
        );
        if s == 1 {
            break;
        }
        let h = s / 2;
        let mut next = vec![0u8; (h * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..h as usize {
                // average in linear light, weighted by coverage, so edges keep their colour
                let mut acc = [0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = ((2 * y + dy) * s as usize + 2 * x + dx) * 4;
                    let a = f32::from(level[i + 3]) / 255.0;
                    for c in 0..3 {
                        acc[c] += (f32::from(level[i + c]) / 255.0).powf(2.2) * a;
                    }
                    acc[3] += a;
                }
                let o = (y * h as usize + x) * 4;
                for c in 0..3 {
                    next[o + c] = if acc[3] > 0.0 { ((acc[c] / acc[3]).powf(1.0 / 2.2) * 255.0).round() as u8 } else { 0 };
                }
                next[o + 3] = (acc[3] / 4.0 * 255.0).round() as u8;
            }
        }
        level = next;
        s = h;
    }
    let view = tex.create_view(&Default::default());
    (tex, view)
}

fn storage(device: &wgpu::Device, label: &str, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes.max(256) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

impl PropsGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals: &wgpu::BindGroupLayout) -> PropsGpu {
        let module = shader(device, "props", &[include_str!("shaders/common.wgsl"), include_str!("shaders/props.wgsl")]);
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty, count: None };
        let buf = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("props"),
            entries: &[
                entry(0, buf),
                entry(1, buf),
                entry(2, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                entry(4, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("props"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Unorm8x4];
        let vb = [Some(wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS })];
        let prop_pipe = crate::pipes::scene(device, &pl, &module, "prop_vs", Some("prop_fs"), &vb, Some(wgpu::Face::Back));
        let glyph_pipe = crate::pipes::blended(device, &pl, &module, "glyph_vs", "glyph_fs", &[], None);
        // the primitive meshes, one buffer
        let m = Material::new([255; 3], 128, 0);
        let meshes: [Mesh; PRIMITIVES as usize] = [cuboid(Vec3::ONE, 0.0, m), cone(0.5, 0.5, 1.0, 20, true, m), sphere(0.5, 14, false, m), cone(0.5, 0.0, 1.0, 16, true, m)];
        let mut verts: Vec<VertexGpu> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut ranges = Vec::new();
        for mesh in &meshes {
            ranges.push((idx.len() as u32, mesh.idx.len() as u32, verts.len() as i32));
            verts.extend(mesh.pos.iter().zip(&mesh.nrm).map(|(p, n)| VertexGpu { pos: *p, nrm: *n, color: [255; 4], mat: [0; 4] }));
            idx.extend_from_slice(&mesh.idx);
        }
        let (vertices, indices) = mesh_buffers(device, &verts, &idx);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, ..Default::default() });
        let decals = rgba_texture(device, queue, 1, &[0, 0, 0, 0]);
        PropsGpu {
            prop_pipe,
            glyph_pipe,
            layout,
            group: None,
            vertices,
            indices,
            verts,
            idx,
            batches: vec![(0, 0); ranges.len()],
            counts: vec![0; ranges.len()],
            ranges,
            props: storage(device, "props", 80 * 256),
            prop_cap: 256,
            glyphs: storage(device, "glyphs", 80 * 256),
            glyph_cap: 256,
            atlas: None,
            decals,
            sampler,
            scene: PropScene::default(),
            staged: Vec::new(),
            staged_glyphs: Vec::new(),
        }
    }

    /// One more mesh a prop may be an instance of (a model: a tool, a control): its number
    /// (`Prop::mesh`). It is drawn as it is — its vertices' own colours, roughness, metalness
    /// and glow; the instance's colour tints it (white: as it is) — and at the instance's size
    /// (1: its own). None when there is no room for more (`Prop::mesh` is a byte).
    pub fn add_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh) -> Option<u8> {
        let id = u8::try_from(self.ranges.len()).ok()?;
        self.ranges.push((self.idx.len() as u32, mesh.idx.len() as u32, self.verts.len() as i32));
        self.verts.extend(mesh.pos.iter().zip(&mesh.nrm).zip(&mesh.mat).map(|((p, n), m)| VertexGpu { pos: *p, nrm: *n, color: [m.albedo[0], m.albedo[1], m.albedo[2], 255], mat: [m.rough, m.metal, m.emissive, 255] }));
        self.idx.extend_from_slice(&mesh.idx);
        (self.vertices, self.indices) = mesh_buffers(device, &self.verts, &self.idx);
        self.batches.push((0, 0));
        self.counts.push(0);
        Some(id)
    }

    /// The font atlas (R8 signed distances).
    pub fn set_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, data: &[u8]) {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("font atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
            size,
        );
        let view = tex.create_view(&Default::default());
        self.atlas = Some((tex, view));
        self.group = None;
    }

    /// The decal atlas: `size`² RGBA8 (sRGB).
    pub fn set_decals(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: u32, data: &[u8]) {
        self.decals = rgba_texture(device, queue, size, data);
        self.group = None;
    }

    /// This frame's props (copied: the caller keeps its scene).
    pub fn set(&mut self, s: &PropScene) {
        self.scene.frames.clone_from(&s.frames);
        self.scene.props.clone_from(&s.props);
        self.scene.glyphs.clone_from(&s.glyphs);
        self.scene.decals.clone_from(&s.decals);
    }

    fn bind(&mut self, device: &wgpu::Device) {
        let Some((_, view)) = &self.atlas else { return };
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("props"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.props.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.glyphs.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&self.decals.1) },
            ],
        }));
    }
}

impl RenderSystem for PropsGpu {
    fn stages(&self) -> u8 {
        if self.atlas.is_none() || (self.scene.props.is_empty() && self.scene.glyphs.is_empty() && self.scene.decals.is_empty()) { 0 } else { graph::OPAQUE | graph::TRANSPARENT }
    }

    fn prepare(&mut self, cx: &PrepareCx) {
        let eye = cx.view.eye;
        let s = &self.scene;
        // camera-relative, sorted by mesh (counting sort)
        let meshes = self.ranges.len();
        self.counts.fill(0);
        for p in &s.props {
            self.counts[usize::from(p.mesh).min(meshes - 1)] += 1;
        }
        let mut start = vec![0usize; meshes];
        let mut acc = 0;
        for k in 0..meshes {
            start[k] = acc;
            self.batches[k] = (acc as u32, self.counts[k] as u32);
            acc += self.counts[k];
        }
        self.staged.clear();
        self.staged.resize(acc, PropGpu::default());
        let frames = &s.frames;
        let place = |frame: u16, p: Vec3| -> (Vec3, Quat) {
            let f = frames.get(usize::from(frame)).copied().unwrap_or_default();
            ((f.pos - eye).as_vec3() + f.rot * p, f.rot)
        };
        let inside = |frame: u16| f32::from(u8::from(frames.get(usize::from(frame)).is_some_and(|f| f.inside)));
        for p in &s.props {
            let k = usize::from(p.mesh).min(meshes - 1);
            let (pos, frot) = place(p.frame, p.pos);
            let rot = (frot * p.rot).normalize();
            self.staged[start[k]] = PropGpu {
                pos: [pos.x, pos.y, pos.z, p.emissive],
                rot: rot.to_array(),
                scale: [p.size.x, p.size.y, p.size.z, 0.0],
                mat: [lin(p.color[0]), lin(p.color[1]), lin(p.color[2]), f32::from(p.rough) / 255.0],
                params: [f32::from(p.metal) / 255.0, inside(p.frame), 0.0, 0.0],
            };
            start[k] += 1;
        }
        self.staged_glyphs.clear();
        for g in &s.glyphs {
            let (c, frot) = place(g.frame, g.center);
            let u = frot * g.u;
            let v = frot * g.v;
            self.staged_glyphs.push(GlyphGpu {
                center: [c.x, c.y, c.z, g.emissive],
                u: [u.x, u.y, u.z, 0.0],
                v: [v.x, v.y, v.z, inside(g.frame)],
                uv: g.uv,
                color: [lin(g.color[0]), lin(g.color[1]), lin(g.color[2]), g.relief],
            });
        }
        // decals ride with the glyphs, told apart by u.w
        for d in &s.decals {
            let (c, frot) = place(d.frame, d.center);
            let u = frot * d.u;
            let v = frot * d.v;
            self.staged_glyphs.push(GlyphGpu {
                center: [c.x, c.y, c.z, d.emissive],
                u: [u.x, u.y, u.z, 1.0],
                v: [v.x, v.y, v.z, inside(d.frame)],
                uv: d.uv,
                color: [lin(d.tint[0]), lin(d.tint[1]), lin(d.tint[2]), 0.0],
            });
        }
        let mut rebind = self.group.is_none();
        if self.staged.len() > self.prop_cap {
            self.prop_cap = self.staged.len().next_power_of_two();
            self.props = storage(cx.device, "props", 80 * self.prop_cap);
            rebind = true;
        }
        if self.staged_glyphs.len() > self.glyph_cap {
            self.glyph_cap = self.staged_glyphs.len().next_power_of_two();
            self.glyphs = storage(cx.device, "glyphs", 80 * self.glyph_cap);
            rebind = true;
        }
        if rebind {
            self.bind(cx.device);
        }
        if !self.staged.is_empty() {
            cx.queue.write_buffer(&self.props, 0, bytemuck::cast_slice(&self.staged));
        }
        if !self.staged_glyphs.is_empty() {
            cx.queue.write_buffer(&self.glyphs, 0, bytemuck::cast_slice(&self.staged_glyphs));
        }
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
        let mut d = Draws::default();
        let Some(group) = &self.group else { return d };
        match stage {
            Stage::Opaque if !self.staged.is_empty() => {
                pass.set_pipeline(&self.prop_pipe);
                pass.set_bind_group(1, group, &[]);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
                for k in 0..self.ranges.len() {
                    let (first, count) = self.batches[k];
                    if count == 0 {
                        continue;
                    }
                    let (i0, n, base) = self.ranges[k];
                    pass.draw_indexed(i0..i0 + n, base, first..first + count);
                    d.calls += 1;
                    d.triangles += u64::from(n / 3) * u64::from(count);
                }
            }
            Stage::Transparent if !self.staged_glyphs.is_empty() => {
                pass.set_pipeline(&self.glyph_pipe);
                pass.set_bind_group(1, group, &[]);
                let n = self.staged_glyphs.len() as u32;
                pass.draw(0..6, 0..n);
                d.calls += 1;
                d.triangles += u64::from(n) * 2;
            }
            _ => {}
        }
        d
    }

    fn gpu_bytes(&self) -> u64 {
        self.props.size() + self.glyphs.size() + self.vertices.size() + self.indices.size()
    }
}
