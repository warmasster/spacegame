//! Marks on loose ground on the GPU: the marks near the eye as one instance buffer, the table of
//! their kinds once, one draw in the transparent stage before the plumes and the dust
//! (shaders/prints.wgsl). A mark's place is kept from an anchor (a point near the eye, f64 on the
//! CPU), so the buffer is written only when a mark is left or the eye has gone far enough for
//! other marks to be the near ones: a frame in which nothing is left costs the 16 bytes of where
//! the anchor is from the camera; with no mark near, nothing at all (no upload, no stage, no draw).
//! A mark's age is worked out by the shader from when it was left: nothing is written for it to fade.
use crate::{
    graph::{self, Draws, EncodeCx, PrepareCx, RenderSystem, Stage},
    renderer::Renderer,
    shader,
};
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec3};

/// Marks drawn at once: the nearest the eye (the app keeps many more: `app::footprints`).
pub const CAPACITY: usize = 4096;
/// Kinds of mark the table holds.
pub const MAX_KINDS: usize = 16;
/// The clock marks are dated by goes round at this (s): the frame's (`uniforms::frame_uniforms`).
pub const CLOCK: f64 = 100_000.0;

/// The frame's clock at `time` (s): what a mark left then is dated with.
pub fn clock(time: f64) -> f32 {
    (time % CLOCK) as f32
}

/// How a mark is shaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A boot's sole with its tread, a left one or a right one.
    Boot = 0,
    /// Round: a landing pad, a drum stood on end.
    Disc = 1,
    /// Square with its corners a little round: a crate, a pallet.
    Box = 2,
    /// Nothing pressed: the dust swept from the middle outward (a jet).
    Swept = 3,
}

/// A kind of mark as the shader reads it (`PrintKind`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug, PartialEq)]
pub struct Kind {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
}

/// What a kind of mark looks like, in plain numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub shape: Shape,
    /// The light of its pressed floor and of the rim thrown up round it (x the ground's).
    pub floor: f32,
    pub rim: f32,
    /// How deep it is pressed and how high its rim stands (m); how wide the rim is (share of
    /// its half-width).
    pub depth: f32,
    pub rim_height: f32,
    pub rim_width: f32,
    /// Its tread: bars along it, and how deep they are (share of its depth).
    pub bars: f32,
    pub bars_depth: f32,
    /// How long it lasts (s; 0: for ever) and the share of that it fades over.
    pub life: f32,
    pub fade: f32,
    /// Seen whole within this far and gone at that (m).
    pub seen: [f32; 2],
    /// Its slopes drawn this much steeper than they are, and how ragged its edge is (0..1).
    pub relief: f32,
    pub grain: f32,
}

impl Kind {
    pub fn new(l: &Look) -> Kind {
        Kind { a: [l.shape as u32 as f32, l.floor, l.rim, l.depth], b: [l.rim_height, l.rim_width, l.bars, l.bars_depth], c: [l.life, l.fade, l.seen[0], l.seen[1]], d: [l.relief, l.grain, 0.0, 0.0] }
    }
}

/// A mark as the shader reads it (`Print`): where it lies from the anchor, the ground's normal
/// there, the way it points, its half sizes, its kind, how hard it was pressed, when.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug, PartialEq)]
pub struct Print {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
}

impl Print {
    /// `rel`: its middle from the anchor (m); `normal` and `ahead`: unit, at right angles;
    /// `half`: half its width and length (m); `born`: the frame's clock when it was left
    /// (`clock`); `side`: 1 a left one, -1 a right one.
    #[allow(clippy::too_many_arguments)]
    pub fn new(rel: Vec3, normal: Vec3, ahead: Vec3, half: [f32; 2], kind: u8, hard: f32, born: f32, seed: f32, side: f32) -> Print {
        Print { a: [rel.x, rel.y, rel.z, half[0]], b: [normal.x, normal.y, normal.z, half[1]], c: [ahead.x, ahead.y, ahead.z, born], d: [f32::from(kind), hard, seed, side] }
    }

    /// Its sixteen numbers, in the order the shader reads them (tools and tests).
    pub fn floats(&self) -> [f32; 16] {
        bytemuck::cast(*self)
    }
}

pub struct PrintsGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    instances: wgpu::Buffer,
    kinds: wgpu::Buffer,
    uniform: wgpu::Buffer,
    /// Bindings with the depth of the targets they were made for (`Targets::id`).
    group: Option<(u64, wgpu::BindGroup)>,
    /// What the marks' places are kept from.
    anchor: DVec3,
    count: u32,
    /// Bytes written to the instance buffer since the start (tools and tests: what leaving
    /// marks costs).
    pub written: u64,
}

impl PrintsGpu {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout) -> PrintsGpu {
        let module = shader(device, "prints", &[include_str!("shaders/common.wgsl"), include_str!("shaders/prints.wgsl")]);
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let buffer = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: both, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("prints"),
            entries: &[
                buffer(0, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(1, wgpu::BufferBindingType::Uniform),
                buffer(2, wgpu::BufferBindingType::Uniform),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: both, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("prints"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        // what the terrain drew, times what the mark says: darker pressed, lighter on its rim
        let times = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Dst, dst_factor: wgpu::BlendFactor::Zero, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("prints"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("print_vs"), compilation_options: Default::default(), buffers: &[] },
            // the far faces of each box, where they are behind the scene: every pixel of the
            // ground in it once, from outside it and from inside
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: Some(wgpu::Face::Front), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: crate::context::DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("print_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: crate::context::HDR, blend: Some(times), write_mask: wgpu::ColorWrites::COLOR })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let make = |label, size: usize, usage| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        PrintsGpu {
            pipeline,
            layout,
            instances: make("prints", std::mem::size_of::<Print>() * CAPACITY, wgpu::BufferUsages::STORAGE),
            kinds: make("print kinds", std::mem::size_of::<Kind>() * MAX_KINDS, wgpu::BufferUsages::UNIFORM),
            uniform: make("prints anchor", 16, wgpu::BufferUsages::UNIFORM),
            group: None,
            anchor: DVec3::ZERO,
            count: 0,
            written: 0,
        }
    }

    pub fn set_kinds(&self, queue: &wgpu::Queue, kinds: &[Kind]) {
        let mut table = [Kind::default(); MAX_KINDS];
        let n = kinds.len().min(MAX_KINDS);
        table[..n].copy_from_slice(&kinds[..n]);
        queue.write_buffer(&self.kinds, 0, bytemuck::cast_slice(&table));
    }

    /// The marks near the eye, all of them, their places from `anchor`: the buffer written whole
    /// (none: nothing is written, nothing is drawn).
    pub fn set(&mut self, queue: &wgpu::Queue, anchor: DVec3, prints: &[Print]) {
        let n = prints.len().min(CAPACITY);
        self.anchor = anchor;
        self.count = n as u32;
        if n > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&prints[..n]));
            self.written += (n * std::mem::size_of::<Print>()) as u64;
        }
    }

    /// One mark written: a new one (`index` the count: one more is drawn) or one that changed.
    /// False: there is no such place.
    pub fn put(&mut self, queue: &wgpu::Queue, index: u32, print: &Print) -> bool {
        if index > self.count || index as usize >= CAPACITY {
            return false;
        }
        queue.write_buffer(&self.instances, u64::from(index) * std::mem::size_of::<Print>() as u64, bytemuck::bytes_of(print));
        self.written += std::mem::size_of::<Print>() as u64;
        self.count = self.count.max(index + 1);
        true
    }

    /// Marks drawn this frame.
    pub fn count(&self) -> u32 {
        self.count
    }
}

impl RenderSystem for PrintsGpu {
    fn stages(&self) -> u8 {
        if self.count > 0 { graph::TRANSPARENT } else { 0 }
    }

    fn prepare(&mut self, cx: &PrepareCx) {
        if self.count > 0 {
            let rel = (self.anchor - cx.view.eye).as_vec3();
            cx.queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&[rel.x, rel.y, rel.z, 0.0]));
        }
    }

    fn before_views(&mut self, cx: &mut EncodeCx) {
        if self.count == 0 || self.group.as_ref().is_some_and(|g| g.0 == cx.targets.id) {
            return;
        }
        let group = cx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("prints"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.instances.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.kinds.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&cx.targets.depth_view) },
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
        // a box each: its six far faces
        pass.draw(0..36, 0..self.count);
        Draws { calls: 1, triangles: u64::from(self.count) * 12 }
    }

    fn gpu_bytes(&self) -> u64 {
        self.instances.size() + self.kinds.size() + self.uniform.size()
    }
}

impl Renderer {
    /// How the kinds of mark look (index = a mark's kind).
    pub fn set_print_kinds(&mut self, kinds: &[Kind]) {
        self.prints.set_kinds(&self.gpu.queue, kinds);
    }

    /// The marks near the eye, their places from `anchor` (a point near it): all of them, written
    /// whole. Call it when the near ones are others; a mark more or one changed is `put_print`.
    pub fn set_prints(&mut self, anchor: DVec3, prints: &[Print]) {
        self.prints.set(&self.gpu.queue, anchor, prints);
    }

    /// One mark of the near ones written (`index`: its place among them; the count: one more).
    pub fn put_print(&mut self, index: u32, print: &Print) -> bool {
        self.prints.put(&self.gpu.queue, index, print)
    }

    /// Marks the GPU holds, and the bytes of them written since the start.
    pub fn prints(&self) -> (u32, u64) {
        (self.prints.count(), self.prints.written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shader_parses_and_a_mark_is_what_it_reads() {
        let src = [include_str!("shaders/common.wgsl"), include_str!("shaders/prints.wgsl")].concat();
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        v.validate(&module).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        // four vectors each, as the shader's structs
        assert_eq!(std::mem::size_of::<Print>(), 64);
        assert_eq!(std::mem::size_of::<Kind>(), 64);
        let p = Print::new(Vec3::new(1.0, 2.0, 3.0), Vec3::Y, Vec3::Z, [0.075, 0.165], 3, 1.25, 77.5, 0.4, -1.0);
        assert_eq!(bytemuck::cast::<Print, [f32; 16]>(p), [1.0, 2.0, 3.0, 0.075, 0.0, 1.0, 0.0, 0.165, 0.0, 0.0, 1.0, 77.5, 3.0, 1.25, 0.4, -1.0]);
        let look = Look { shape: Shape::Box, floor: 0.9, rim: 1.05, depth: 0.02, rim_height: 0.005, rim_width: 0.1, bars: 9.0, bars_depth: 0.4, life: 600.0, fade: 0.2, seen: [40.0, 60.0], relief: 1.5, grain: 0.5 };
        assert_eq!(bytemuck::cast::<Kind, [f32; 16]>(Kind::new(&look)), [2.0, 0.9, 1.05, 0.02, 0.005, 0.1, 9.0, 0.4, 600.0, 0.2, 40.0, 60.0, 1.5, 0.5, 0.0, 0.0]);
        // the clock goes round with the frame's
        assert_eq!(clock(12.5), 12.5);
        assert!((clock(CLOCK + 3.0) - 3.0).abs() < 1e-3);
    }
}
