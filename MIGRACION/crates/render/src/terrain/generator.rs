//! GPU node generation: a batch of jobs per frame, compute dispatches (heights; with baked shadows
//! the far horizon toward the sun; then normals and baked sun visibility) writing straight into the
//! node texture arrays. The pipelines (`Kernel`) are
//! shared; each body has its own `Generator` built from its procedural surface.
use crate::shader;
use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use lunar_core::{
    deform::{Crater, MAX_NODE_CRATERS},
    surface::Procedural,
    terrain_gen::{MAX_CRATER_LAYERS, NOISE_CALLS, NodeFrame, NodeGen, node_gen, split},
};
use std::f64::consts::FRAC_PI_4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct NodeGpu {
    pub frame0: [f32; 4],
    pub frame1: [f32; 4],
    pub dir: [f32; 4],
    pub info: [f32; 4],
    pub tex: [f32; 4],
    pub ids: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct JobGpu {
    calls: [[f32; 4]; NOISE_CALLS],
    cells: [[i32; 4]; NOISE_CALLS],
    node: NodeGpu,
    split: [f32; 4],
    counts: [u32; 4],
    fades: [f32; 4],
    /// Blast craters on the node, oldest first: (direction - node direction, radius), (depth, rim,
    /// ground level it wipes to, seed + 2 if kept by the parent); count in `craters_n.x`; `craters_n.y`: the far horizon's octaves and layers.
    craters: [[f32; 4]; 2 * MAX_NODE_CRATERS],
    craters_n: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SurfaceU {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
    e: [u32; 4],
    sun: [f32; 4],
    layers: [[f32; 4]; MAX_CRATER_LAYERS * 2],
}

/// Far horizon samples per node (9 x 9, twin of HOR in terrain_gen.wgsl).
const HORIZON: u64 = 81;
/// The far horizon's relief: this many times the node's cell (octaves and crater layers finer are
/// left out of the long march toward the sun).
const HORIZON_COARSE: f64 = 8.0;

/// Detail texture period (m): every tile size used by the terrain shader divides it.
const TEX_PERIOD: f64 = 2048.0;

pub struct NodeDesc {
    pub frame: NodeFrame,
    pub layer: u32,
    pub level: u32,
    pub skirt: f32,
}

pub fn node_gpu(d: &NodeDesc, grid: u32, radius: f64) -> NodeGpu {
    let f = &d.frame;
    let m_per_param = radius * FRAC_PI_4;
    let tex = |c: f64| ((c - f.half) * m_per_param).rem_euclid(TEX_PERIOD) as f32;
    NodeGpu {
        frame0: [f.alpha[0] as f32, f.alpha[1] as f32, f.tan[0] as f32, f.tan[1] as f32],
        frame1: [f.cos[0] as f32, f.cos[1] as f32, f.sin[0] as f32, f.sin[1] as f32],
        dir: [f.dir.x as f32, f.dir.y as f32, f.dir.z as f32, f.s as f32],
        info: [f.half as f32, radius as f32, 0.0, 0.0],
        tex: [tex(f.a), tex(f.b), m_per_param as f32, d.skirt],
        ids: [f.face as u32, d.layer, d.level, grid],
    }
}

/// The generator's pipelines, shared by every body's terrain.
pub struct Kernel {
    layout: wgpu::BindGroupLayout,
    heights: wgpu::ComputePipeline,
    horizon: wgpu::ComputePipeline,
    finish: wgpu::ComputePipeline,
}

impl Kernel {
    pub fn new(device: &wgpu::Device) -> Kernel {
        let module = shader(device, "terrain gen", &[include_str!("../shaders/terrain_common.wgsl"), include_str!("../shaders/terrain_gen.wgsl")]);
        let cs = wgpu::ShaderStages::COMPUTE;
        let buffer = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: cs,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let storage_tex = |binding, format| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: cs,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2Array,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain gen"),
            entries: &[
                buffer(0, wgpu::BufferBindingType::Uniform),
                buffer(1, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(2, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer(3, wgpu::BufferBindingType::Storage { read_only: false }),
                buffer(4, wgpu::BufferBindingType::Storage { read_only: false }),
                storage_tex(5, super::HEIGHT_FORMAT),
                storage_tex(6, super::NORMAL_FORMAT),
                buffer(7, wgpu::BufferBindingType::Storage { read_only: false }),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain gen"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Kernel { heights: pipeline("gen_heights"), horizon: pipeline("gen_horizon"), finish: pipeline("gen_finish"), layout }
    }
}

/// One body's generator: its surface uniform, noise tables, job queue and scratch.
pub struct Generator {
    group: wgpu::BindGroup,
    jobs: wgpu::Buffer,
    staging: Vec<JobGpu>,
    pub max_jobs: u32,
    grid: u32,
    border: u32,
    baked: bool,
    radius: f64,
}

pub struct GeneratorDesc<'a> {
    pub sun: DVec3,
    pub grid: u32,
    pub border: u32,
    pub baked: bool,
    pub max_jobs: u32,
    pub heights: &'a wgpu::TextureView,
    pub normals: &'a wgpu::TextureView,
}

impl Generator {
    pub fn new(device: &wgpu::Device, kernel: &Kernel, surface: &Procedural, d: &GeneratorDesc) -> Generator {
        use wgpu::util::DeviceExt;
        let def = surface.def();
        let mut layers = [[0f32; 4]; MAX_CRATER_LAYERS * 2];
        let quarter = std::f64::consts::FRAC_PI_2 * surface.radius();
        for (i, l) in def.craters.iter().enumerate() {
            let n = f64::from(surface.crater_cells()[i]);
            layers[i * 2] = [n as f32, (quarter / n) as f32, (2. / n) as f32, l.p as f32];
            layers[i * 2 + 1] = [l.mare as f32, l.r_min as f32, l.r_max as f32, l.depth as f32];
        }
        let su = SurfaceU {
            a: [surface.radius() as f32, (def.datum + surface.offset()) as f32, def.highland_amp as f32, def.mare_depth as f32],
            b: [def.mare_from as f32, def.mare_to as f32, def.mountains.amp as f32, def.mountains.gain as f32],
            c: [def.relief.amp as f32, def.relief.gain as f32, def.mare_smooth as f32, def.complex_from as f32],
            d: [def.albedo_high as f32, def.albedo_mare as f32, def.crater_detail as f32, def.crater_odds_fade as f32],
            e: [surface.seed().wrapping_mul(31).wrapping_mul(0x9e3779b9), d.grid, d.border, u32::from(d.baked)],
            sun: [d.sun.x as f32, d.sun.y as f32, d.sun.z as f32, 0.],
            layers,
        };
        let perm: Vec<u32> = surface.noises().iter().flat_map(|n| n.perm().iter().map(|v| *v as u32)).collect();
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("surface"),
            contents: bytemuck::bytes_of(&su),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let perm = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("perm"),
            contents: bytemuck::cast_slice(&perm),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let jobs = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain jobs"),
            size: (std::mem::size_of::<JobGpu>() as u64) * u64::from(d.max_jobs),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let w = u64::from(d.grid + 1 + 2 * d.border);
        let texels = w * w * u64::from(d.max_jobs);
        let scratch = |label, bytes| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: texels * bytes, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let scratch_pos = scratch("terrain scratch pos", 16);
        let scratch_alb = scratch("terrain scratch albedo", 8);
        let scratch_hor = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain scratch horizon"),
            size: HORIZON * 4 * u64::from(d.max_jobs),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain gen"),
            layout: &kernel.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: perm.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: jobs.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: scratch_pos.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: scratch_alb.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(d.heights) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(d.normals) },
                wgpu::BindGroupEntry { binding: 7, resource: scratch_hor.as_entire_binding() },
            ],
        });
        Generator { group, jobs, staging: Vec::with_capacity(d.max_jobs as usize), max_jobs: d.max_jobs, grid: d.grid, border: d.border, baked: d.baked, radius: surface.radius() }
    }

    pub fn queued(&self) -> usize {
        self.staging.len()
    }

    /// Queue a node.
    /// Queue a node; `craters` are the blast craters on its ground (at most `MAX_NODE_CRATERS`).
    pub fn push(&mut self, surface: &Procedural, desc: &NodeDesc, min_feature: f64, craters: &[Crater]) {
        let f = &desc.frame;
        let c = f.dir * surface.radius();
        let g: NodeGen = node_gen(surface, c, min_feature);
        // what the parent (twice the cell) keeps: the rest is faded out as the node morphs into it
        let p = node_gen(surface, c, min_feature * 2.);
        let parent = p.relief_octaves | p.mountain_octaves << 8 | p.crater_layers << 16;
        let h = node_gen(surface, c, min_feature * HORIZON_COARSE);
        let horizon = h.relief_octaves | h.mountain_octaves << 8 | h.crater_layers << 16;
        let [ah, al] = split(f.a);
        let [bh, bl] = split(f.b);
        self.staging.push(JobGpu {
            calls: g.calls.map(|k| [k.frac[0], k.frac[1], k.frac[2], k.scale]),
            cells: g.calls.map(|k| [k.cell[0], k.cell[1], k.cell[2], 0]),
            node: node_gpu(desc, self.grid, self.radius),
            split: [ah, al, bh, bl],
            counts: [g.relief_octaves, g.mountain_octaves, g.crater_layers, parent],
            fades: [g.min_feature, g.broad_fade, g.detail_fade, 0.],
            craters: [[0.0; 4]; 2 * MAX_NODE_CRATERS],
            craters_n: [0, horizon, 0, 0],
        });
        let job = self.staging.last_mut().expect("just pushed");
        let mut n = 0;
        for c in craters.iter().take(MAX_NODE_CRATERS) {
            // too small for this node's cell: left out (the finer nodes draw it)
            if c.radius * 2.0 < min_feature * 1.5 {
                continue;
            }
            let kept = c.radius * 2.0 >= min_feature * 3.0;
            let d = (c.dir - f.dir).as_vec3();
            job.craters[n * 2] = [d.x, d.y, d.z, c.radius as f32];
            // seed (0..1) plus 2 when the parent draws it too
            job.craters[n * 2 + 1] = [c.depth as f32, c.rim as f32, c.ground as f32, c.seed as f32 + if kept { 2.0 } else { 0.0 }];
            n += 1;
        }
        job.craters_n[0] = n as u32;
    }

    /// Upload the queued jobs; true when there is something to dispatch.
    pub fn upload(&self, queue: &wgpu::Queue) -> bool {
        if !self.staging.is_empty() {
            queue.write_buffer(&self.jobs, 0, bytemuck::cast_slice(&self.staging));
        }
        !self.staging.is_empty()
    }

    /// Dispatch the uploaded jobs into `pass` and clear the queue.
    pub fn dispatch(&mut self, kernel: &Kernel, pass: &mut wgpu::ComputePass) {
        let n = self.staging.len() as u32;
        if n == 0 {
            return;
        }
        let w = self.grid + 1 + 2 * self.border;
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(&kernel.heights);
        pass.dispatch_workgroups(w.div_ceil(8), w.div_ceil(8), n);
        if self.baked {
            pass.set_pipeline(&kernel.horizon);
            pass.dispatch_workgroups(1, 1, n);
        }
        pass.set_pipeline(&kernel.finish);
        pass.dispatch_workgroups((self.grid + 1).div_ceil(8), (self.grid + 1).div_ceil(8), n);
        self.staging.clear();
    }
}
