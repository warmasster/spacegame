//! The culling dispatch: per-view planes and LOD scale into a uniform, the indirect commands reset
//! from their template, one thread per instance; command counts read back (late) for statistics.
use super::{VIEWS, draw::Buffers, hiz::Hiz};
use crate::{frame::View, shader, shadow::Cascades};
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4};
use lunar_core::{frustum::Frustum, quality::Settings};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewGpu {
    planes: [[f32; 4]; 6],
    shell: [f32; 4],
    flags: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CullU {
    views: [ViewGpu; VIEWS],
    main_planes: [[f32; 4]; 6],
    prev_vp: [[f32; 4]; 4],
    origin: [f32; 4],
    prev_origin: [f32; 4],
    shadow_dir: [f32; 4],
    lod: [f32; 4],
    counts: [u32; 4],
    hiz: [f32; 4],
    proj: [f32; 4],
}

const RING: usize = 3;

pub struct Cull {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    pub group: Option<wgpu::BindGroup>,
    dummy_hiz: wgpu::TextureView,
    ring: Vec<(Option<wgpu::Buffer>, Arc<AtomicU8>)>,
    slot: usize,
    copied: bool,
    counts: Vec<u32>,
}

fn planes(f: &Frustum) -> [[f32; 4]; 6] {
    f.planes.map(|p| p.to_array())
}

impl Cull {
    pub fn new(device: &wgpu::Device) -> Cull {
        let cs = wgpu::ShaderStages::COMPUTE;
        let buf = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: cs, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cull"),
            entries: &[
                buf(0, wgpu::BufferBindingType::Uniform),
                buf(1, wgpu::BufferBindingType::Storage { read_only: true }),
                buf(2, wgpu::BufferBindingType::Storage { read_only: true }),
                buf(3, wgpu::BufferBindingType::Storage { read_only: false }),
                buf(4, wgpu::BufferBindingType::Storage { read_only: false }),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: cs,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("cull"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let module = shader(device, "cull", &[include_str!("../shaders/cull.wgsl")]);
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cull"),
            layout: Some(&pl),
            module: &module,
            entry_point: Some("cull"),
            compilation_options: Default::default(),
            cache: None,
        });
        let dummy_hiz = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("no hiz"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Cull {
            pipeline,
            layout,
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cull"),
                size: std::mem::size_of::<CullU>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            group: None,
            dummy_hiz,
            ring: (0..RING).map(|_| (None, Arc::new(AtomicU8::new(0)))).collect(),
            slot: 0,
            copied: false,
            counts: Vec::new(),
        }
    }

    pub fn invalidate(&mut self) {
        self.group = None;
        for r in &mut self.ring {
            if r.1.load(Ordering::Acquire) == 0 {
                r.0 = None;
            }
        }
    }

    fn ensure(&mut self, device: &wgpu::Device, gpu: &Buffers, hiz: &Hiz) {
        for r in &mut self.ring {
            if r.1.load(Ordering::Acquire) == 0 && r.0.as_ref().is_none_or(|b| b.size() != gpu.cmds.size()) {
                r.0 = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("cull counts"),
                    size: gpu.cmds.size(),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }));
            }
        }
        if self.group.is_some() {
            return;
        }
        let hv = hiz.view().unwrap_or(&self.dummy_hiz);
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cull"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: gpu.instances.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: gpu.models.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: gpu.cmds.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: gpu.list.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(hv) },
            ],
        }));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        gpu: &Buffers,
        view: &View,
        prev_vp: Mat4,
        origin: DVec3,
        prev_eye: DVec3,
        main: &Frustum,
        cascades: &Cascades,
        s: &Settings,
        aspect: f32,
        viewport_h: f32,
        hiz: &Hiz,
        count: u32,
        views: u32,
        sun: DVec3,
    ) {
        self.ensure(device, gpu, hiz);
        let empty = ViewGpu { planes: [[0.0, 0.0, 0.0, 1.0]; 6], shell: [0.0; 4], flags: [0; 4] };
        let mut vs = [empty; VIEWS];
        let occlusion = s.occlusion_culling && hiz.built.0 > 0;
        vs[0] = ViewGpu { planes: planes(main), shell: [0.0; 4], flags: [super::FLAG_STATIC | super::FLAG_MOVING, 0, 0, u32::from(occlusion)] };
        let shift = if s.shadow_lod_bias > 0.0 { 1 } else { 0 };
        for c in 0..cascades.count {
            let near = if c == 0 { 0.0 } else { cascades.far[c - 1] };
            let v = ViewGpu { planes: planes(&cascades.frusta[c]), shell: [near, cascades.far[c], 0.0, 0.0], flags: [super::FLAG_STATIC, shift, 1, 0] };
            vs[1 + 2 * c] = v;
            vs[2 + 2 * c] = ViewGpu { flags: [super::FLAG_MOVING, shift, 1, 0], ..v };
        }
        let tan = (view.fov_y * 0.5).tan();
        let px = viewport_h / (2.0 * tan) * s.lod_bias;
        let p = glam::camera::rh::proj::directx::perspective_infinite_reverse(view.fov_y, aspect, view.near);
        let away = (-sun).normalize().as_vec3();
        let u = CullU {
            views: vs,
            main_planes: planes(main),
            prev_vp: prev_vp.to_cols_array_2d(),
            origin: [(origin - view.eye).x as f32, (origin - view.eye).y as f32, (origin - view.eye).z as f32, 0.0],
            prev_origin: {
                let o = (origin - prev_eye).as_vec3();
                [o.x, o.y, o.z, 0.0]
            },
            // a flying caster's shadow lands far from it: swept as far as the cascades take casters
            shadow_dir: [away.x, away.y, away.z, crate::shadow::REACH as f32],
            lod: [px, s.draw_distance, 4.0, view.near],
            counts: [count, gpu.layout.cmds.len() as u32, views, gpu.layout.stride],
            hiz: [hiz.built.0 as f32, hiz.built.1 as f32, hiz.mips() as f32, f32::from(u8::from(occlusion))],
            proj: [p.x_axis.x, p.y_axis.y, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }

    pub fn record(&mut self, encoder: &mut wgpu::CommandEncoder, gpu: &Buffers, count: u32, ts: Option<wgpu::ComputePassTimestampWrites>) {
        let Some(group) = &self.group else { return };
        encoder.copy_buffer_to_buffer(&gpu.template, 0, &gpu.cmds, 0, gpu.template.size());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("cull"), timestamp_writes: ts });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(count.div_ceil(64).max(1), 1, 1);
        }
        // statistics: copy the counts to a free readback slot
        self.copied = false;
        let (buf, state) = &self.ring[self.slot];
        if let Some(b) = buf
            && state.load(Ordering::Acquire) == 0
            && b.size() == gpu.cmds.size()
        {
            encoder.copy_buffer_to_buffer(&gpu.cmds, 0, b, 0, gpu.cmds.size());
            self.copied = true;
        }
    }

    /// Start mapping this frame's copy; return the newest finished counts (instance count per
    /// view × command).
    pub fn readback(&mut self, device: &wgpu::Device) -> Option<&[u32]> {
        if self.copied {
            if let (Some(b), state) = (&self.ring[self.slot].0, &self.ring[self.slot].1) {
                let st = state.clone();
                st.store(1, Ordering::Release);
                b.slice(..).map_async(wgpu::MapMode::Read, move |r| st.store(if r.is_ok() { 2 } else { 0 }, Ordering::Release));
            }
        }
        self.copied = false;
        self.slot = (self.slot + 1) % RING;
        let _ = device.poll(wgpu::PollType::Poll);
        let mut got = false;
        for (buf, state) in &mut self.ring {
            if state.load(Ordering::Acquire) != 2 {
                continue;
            }
            if let Some(b) = buf {
                if let Ok(data) = b.slice(..).get_mapped_range() {
                    let cmds: &[super::models::DrawCmd] = bytemuck::cast_slice(&data);
                    self.counts.clear();
                    self.counts.extend(cmds.iter().map(|c| c.instance_count));
                    got = true;
                }
                b.unmap();
            }
            state.store(0, Ordering::Release);
        }
        got.then_some(&self.counts[..])
    }
}
