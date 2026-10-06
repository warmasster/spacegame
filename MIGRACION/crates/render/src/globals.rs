//! Group 0 of every scene pipeline: the frame uniform, per-pass uniforms (dynamic offset), the
//! shadow map and the shared samplers. One layout, two bind groups: the shadow passes bind a
//! dummy shadow map (a texture cannot be sampled while it is being drawn into).
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct FrameU {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub prev_view_proj: [[f32; 4]; 4],
    pub cascades: [[[f32; 4]; 4]; 4],
    pub cascade_far: [f32; 4],
    pub cam_forward: [f32; 4],
    pub origin: [f32; 4],
    pub prev_origin: [f32; 4],
    pub sun_dir: [f32; 4],
    pub sun_color: [f32; 4],
    pub backdrop: [f32; 4],
    pub up: [f32; 4],
    pub body_center: [f32; 4],
    pub viewport: [f32; 4],
    pub target: [f32; 4],
    pub params: [f32; 4],
    pub params2: [f32; 4],
    pub shadow: [f32; 4],
    pub lights: [[f32; 4]; 3 * MAX_LIGHTS],
}

/// Lights the frame carries: explosion flashes and the lamps of structures nearby.
pub const MAX_LIGHTS: usize = 24;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PassU {
    pub view_proj: [[f32; 4]; 4],
    pub info: [u32; 4],
    pub light: [f32; 4],
}

pub const PASS_STRIDE: u64 = 256;
pub const PASS_SLOTS: u64 = 16;
/// Pass slots: main view, then one per cascade.
pub const PASS_MAIN: u32 = 0;

pub struct Globals {
    pub frame: wgpu::Buffer,
    pub pass: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
    pub main: wgpu::BindGroup,
    pub shadow: wgpu::BindGroup,
    shadow_cmp: wgpu::Sampler,
    pub linear_repeat: wgpu::Sampler,
    pub linear_clamp: wgpu::Sampler,
    dummy: wgpu::TextureView,
    pass_staging: Vec<u8>,
}

impl Globals {
    pub fn new(device: &wgpu::Device, shadow_view: &wgpu::TextureView, anisotropy: u16) -> Globals {
        let frame = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: std::mem::size_of::<FrameU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pass = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("passes"),
            size: PASS_STRIDE * PASS_SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let all = wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE;
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: all, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<PassU>() as u64),
                }),
                entry(2, wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                }),
                entry(3, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)),
                entry(4, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                entry(5, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let shadow_cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let (linear_repeat, linear_clamp) = samplers(device, anisotropy);
        let dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("no shadow"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::context::DEPTH,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let mut g = Globals {
            main: dummy_group(device),
            shadow: dummy_group(device),
            frame,
            pass,
            layout,
            shadow_cmp,
            linear_repeat,
            linear_clamp,
            dummy,
            pass_staging: vec![0; (PASS_STRIDE * PASS_SLOTS) as usize],
        };
        g.rebind(device, shadow_view);
        g
    }

    /// After the shadow map or the samplers change.
    pub fn rebind(&mut self, device: &wgpu::Device, shadow_view: &wgpu::TextureView) {
        let group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.frame.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.pass,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<PassU>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.shadow_cmp) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.linear_repeat) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&self.linear_clamp) },
                ],
            })
        };
        self.main = group(shadow_view);
        self.shadow = group(&self.dummy);
    }

    pub fn set_anisotropy(&mut self, device: &wgpu::Device, anisotropy: u16, shadow_view: &wgpu::TextureView) {
        (self.linear_repeat, self.linear_clamp) = samplers(device, anisotropy);
        self.rebind(device, shadow_view);
    }

    pub fn set_pass(&mut self, slot: u32, pass: &PassU) {
        let at = (u64::from(slot) * PASS_STRIDE) as usize;
        self.pass_staging[at..at + std::mem::size_of::<PassU>()].copy_from_slice(bytemuck::bytes_of(pass));
    }

    pub fn upload(&self, queue: &wgpu::Queue, frame: &FrameU) {
        queue.write_buffer(&self.frame, 0, bytemuck::bytes_of(frame));
        queue.write_buffer(&self.pass, 0, &self.pass_staging);
    }

    pub fn offset(slot: u32) -> u32 {
        (u64::from(slot) * PASS_STRIDE) as u32
    }
}

fn samplers(device: &wgpu::Device, anisotropy: u16) -> (wgpu::Sampler, wgpu::Sampler) {
    let make = |mode| {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: mode,
            address_mode_v: mode,
            address_mode_w: mode,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: anisotropy.clamp(1, 16),
            ..Default::default()
        })
    };
    (make(wgpu::AddressMode::Repeat), make(wgpu::AddressMode::ClampToEdge))
}

fn dummy_group(device: &wgpu::Device) -> wgpu::BindGroup {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[] });
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &[] })
}
