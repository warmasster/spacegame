//! Particles on the GPU: the simulation's list (core::particles) sorted far to near and uploaded as
//! one instance buffer each frame, the style table once, drawn in the transparent stage with the
//! scene depth bound for soft edges (shaders/particles.wgsl).
use crate::{
    graph::{self, Draws, EncodeCx, RenderSystem, Stage},
    shader,
};
use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use lunar_core::particles::{MAX_STYLES, Particle, Particles, StyleDef};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct PartGpu {
    pos: [f32; 4],
    vel: [f32; 4],
    misc: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct StyleGpu {
    albedo0: [f32; 4],
    albedo1: [f32; 4],
    emission0: [f32; 4],
    emission1: [f32; 4],
    curve: [f32; 4],
    shape: [f32; 4],
}

impl StyleGpu {
    fn new(s: &StyleDef) -> StyleGpu {
        let c = |v: [f32; 3]| [v[0], v[1], v[2], 0.0];
        StyleGpu {
            albedo0: c(s.albedo[0]),
            albedo1: c(s.albedo[1]),
            emission0: c(s.emission[0]),
            emission1: c(s.emission[1]),
            curve: [s.opacity[0], s.opacity[1], s.fade_in, s.fade_out],
            shape: [s.stretch, s.puff, f32::from(u8::from(s.glow)), s.glow_reach],
        }
    }
}

pub struct ParticlesGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    parts: wgpu::Buffer,
    styles: wgpu::Buffer,
    capacity: usize,
    /// Bindings with the depth of the targets they were made for (`Targets::id`).
    group: Option<(u64, wgpu::BindGroup)>,
    order: Vec<(u32, u32)>,
    staged: Vec<PartGpu>,
    count: u32,
}

fn parts_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particles"),
        size: (std::mem::size_of::<PartGpu>() * capacity.max(1)) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ParticlesGpu {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, capacity: usize) -> ParticlesGpu {
        let module = shader(device, "particles", &[include_str!("shaders/common.wgsl"), include_str!("shaders/particles.wgsl")]);
        let fs = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: fs,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: fs,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("particles"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        let premultiplied = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("particle_vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::context::DEPTH,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("particle_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: crate::context::HDR,
                    blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let styles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle styles"),
            size: (std::mem::size_of::<StyleGpu>() * MAX_STYLES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        ParticlesGpu {
            pipeline,
            layout,
            parts: parts_buffer(device, capacity),
            styles,
            capacity,
            group: None,
            order: Vec::with_capacity(capacity),
            staged: Vec::with_capacity(capacity),
            count: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// A new particle budget (settings): new buffer, bindings remade on the next frame.
    pub fn resize(&mut self, device: &wgpu::Device, capacity: usize) {
        if capacity != self.capacity {
            self.parts = parts_buffer(device, capacity);
            self.capacity = capacity;
            self.group = None;
            self.order.reserve(capacity.saturating_sub(self.order.len()));
            self.staged.reserve(capacity.saturating_sub(self.staged.len()));
            self.count = 0;
        }
    }

    pub fn set_styles(&self, queue: &wgpu::Queue, styles: &[StyleDef]) {
        let mut table = [StyleGpu::default(); MAX_STYLES];
        for (t, s) in table.iter_mut().zip(styles) {
            *t = StyleGpu::new(s);
        }
        queue.write_buffer(&self.styles, 0, bytemuck::cast_slice(&table));
    }

    /// This frame's particles and `extra` ones drawn the same way (rounds in flight...),
    /// camera-relative, far to near: one buffer, one draw.
    pub fn upload(&mut self, queue: &wgpu::Queue, particles: &Particles, extra: &[Particle], eye: DVec3) {
        let list = &particles.list;
        let n = list.len().min(self.capacity);
        let m = extra.len().min(self.capacity - n);
        let key = |p: &Particle| ((p.pos - eye).length_squared() as f32).to_bits();
        self.order.clear();
        self.order.extend(list[..n].iter().enumerate().map(|(i, p)| (key(p), i as u32)));
        self.order.extend(extra[..m].iter().enumerate().map(|(i, p)| (key(p), (n + i) as u32)));
        self.order.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        self.staged.clear();
        for &(_, i) in &self.order {
            let i = i as usize;
            let p = if i < n { &list[i] } else { &extra[i - n] };
            let s = &particles.styles[usize::from(p.style)];
            let rel = (p.pos - eye).as_vec3();
            self.staged.push(PartGpu {
                pos: [rel.x, rel.y, rel.z, p.radius(s)],
                vel: [p.vel.x, p.vel.y, p.vel.z, p.t()],
                misc: [p.seed, f32::from(p.style), p.height, 0.0],
            });
        }
        let n = n + m;
        self.count = n as u32;
        if n > 0 {
            queue.write_buffer(&self.parts, 0, bytemuck::cast_slice(&self.staged));
        }
    }
}

impl RenderSystem for ParticlesGpu {
    fn stages(&self) -> u8 {
        if self.count > 0 { graph::TRANSPARENT } else { 0 }
    }

    fn before_views(&mut self, cx: &mut EncodeCx) {
        if self.count == 0 || self.group.as_ref().is_some_and(|g| g.0 == cx.targets.id) {
            return;
        }
        let group = cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.parts.as_entire_binding() },
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
        pass.draw(0..4, 0..self.count);
        Draws { calls: 1, triangles: u64::from(self.count) * 2 }
    }

    fn gpu_bytes(&self) -> u64 {
        self.parts.size() + self.styles.size()
    }
}
