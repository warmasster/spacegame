//! Exhaust plumes on the GPU: this frame's list (core::plumes) as one small instance buffer, the
//! style table once, one draw in the transparent stage before the particles
//! (shaders/plumes.wgsl). With no plume to draw it takes no part in the frame: no upload, no
//! stage, no draw.
use crate::{
    graph::{self, Draws, EncodeCx, RenderSystem, Stage},
    shader,
};
use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use lunar_core::plumes::{MAX_STYLES, Plume, Styles};

/// Plumes drawn at once: the nearest ones if there are more (they come in no order, and a
/// battle of a hundred ships firing everything has fewer).
pub const CAPACITY: usize = 2048;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct PlumeGpu {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct StyleGpu {
    core: [f32; 4],
    plume: [f32; 4],
    shape: [f32; 4],
    extra: [f32; 4],
}

pub struct PlumesGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    instances: wgpu::Buffer,
    styles: wgpu::Buffer,
    /// Bindings with the depth of the targets they were made for (`Targets::id`).
    group: Option<(u64, wgpu::BindGroup)>,
    staged: Vec<PlumeGpu>,
    count: u32,
}

impl PlumesGpu {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout) -> PlumesGpu {
        let module = shader(device, "plumes", &[include_str!("shaders/common.wgsl"), include_str!("shaders/plumes.wgsl")]);
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("plumes"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: both, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: both, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: both, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("plumes"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        // (light only: the fragment's alpha is 0, so this adds)
        let premultiplied = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("plumes"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("plume_vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: crate::context::DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Greater), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("plume_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: crate::context::HDR, blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor { label: Some("plumes"), size: (std::mem::size_of::<PlumeGpu>() * CAPACITY) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let styles = device.create_buffer(&wgpu::BufferDescriptor { label: Some("plume styles"), size: (std::mem::size_of::<StyleGpu>() * MAX_STYLES) as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        PlumesGpu { pipeline, layout, instances, styles, group: None, staged: Vec::with_capacity(CAPACITY), count: 0 }
    }

    pub fn set_styles(&self, queue: &wgpu::Queue, styles: &Styles) {
        let mut table = [StyleGpu::default(); MAX_STYLES];
        for (t, s) in table.iter_mut().zip(&styles.list) {
            let d = &s.def;
            *t = StyleGpu { core: [d.nucleo[0], d.nucleo[1], d.nucleo[2], d.nucleo_largo], plume: [d.pluma[0], d.pluma[1], d.pluma[2], d.caida], shape: [d.nucleo_ancho, d.parpadeo, d.corriente, 0.0], extra: [0.0; 4] };
        }
        queue.write_buffer(&self.styles, 0, bytemuck::cast_slice(&table));
    }

    /// This frame's plumes, camera-relative: one buffer, one draw. None: nothing is sent.
    pub fn upload(&mut self, queue: &wgpu::Queue, plumes: &[Plume], eye: DVec3) {
        self.staged.clear();
        for p in plumes.iter().take(CAPACITY) {
            let rel = (p.pos - eye).as_vec3();
            self.staged.push(PlumeGpu { a: [rel.x, rel.y, rel.z, p.radius], b: [p.dir.x, p.dir.y, p.dir.z, p.length], c: [p.spread, p.gain, f32::from(p.style), p.seed] });
        }
        self.count = self.staged.len() as u32;
        if self.count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.staged));
        }
    }

    /// Plumes drawn this frame.
    pub fn count(&self) -> u32 {
        self.count
    }
}

impl RenderSystem for PlumesGpu {
    fn stages(&self) -> u8 {
        if self.count > 0 { graph::TRANSPARENT } else { 0 }
    }

    fn before_views(&mut self, cx: &mut EncodeCx) {
        if self.count == 0 || self.group.as_ref().is_some_and(|g| g.0 == cx.targets.id) {
            return;
        }
        let group = cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("plumes"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.instances.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.styles.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&cx.targets.depth_view) },
            ],
        });
        self.group = Some((cx.targets.id, group));
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
        let Some((_, group)) = &self.group else { return Draws::default() };
        if stage != Stage::Transparent || self.count == 0 {
            return Draws::default();
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, group, &[]);
        // two quads each: the cone from its side and the glow at its mouth
        pass.draw(0..12, 0..self.count);
        Draws { calls: 1, triangles: u64::from(self.count) * 4 }
    }

    fn gpu_bytes(&self) -> u64 {
        self.instances.size() + self.styles.size()
    }
}
