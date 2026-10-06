//! Hi-Z pyramid built from the frame's depth, read by the next frame's culling.
use crate::{shader, targets::Targets};

pub struct Hiz {
    copy: wgpu::ComputePipeline,
    down: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    res: Option<Res>,
    /// Viewport the pyramid was built from (0 = not built yet).
    pub built: (u32, u32),
}

struct Res {
    size: (u32, u32),
    pub view: wgpu::TextureView,
    groups: Vec<wgpu::BindGroup>,
    dims: Vec<(u32, u32)>,
}

impl Hiz {
    pub fn new(device: &wgpu::Device) -> Hiz {
        let cs = wgpu::ShaderStages::COMPUTE;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hiz"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: cs,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: cs,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: cs,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: cs,
                    ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::R32Float, view_dimension: wgpu::TextureViewDimension::D2 },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("hiz"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let module = shader(device, "hiz", &[include_str!("../shaders/hiz.wgsl")]);
        let pipe = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Hiz {
            copy: pipe("copy_depth"),
            down: pipe("downsample"),
            uniform: device.create_buffer(&wgpu::BufferDescriptor { label: Some("hiz"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
            layout,
            res: None,
            built: (0, 0),
        }
    }

    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.res.as_ref().map(|r| &r.view)
    }

    pub fn mips(&self) -> u32 {
        self.res.as_ref().map_or(1, |r| r.dims.len() as u32)
    }

    /// Recreated with the targets; returns true when the texture (and bindings using it) changed.
    pub fn ensure(&mut self, device: &wgpu::Device, targets: &Targets) -> bool {
        let size = (targets.width, targets.height);
        if self.res.as_ref().is_some_and(|r| r.size == size) {
            return false;
        }
        let mips = 32 - size.0.max(size.1).leading_zeros();
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hiz"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mip_view = |m: u32| tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: m, mip_level_count: Some(1), ..Default::default() });
        let mut groups = Vec::new();
        let mut dims = Vec::new();
        for m in 0..mips {
            let src = mip_view(m.saturating_sub(1));
            let dst = mip_view(m);
            groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hiz"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&targets.depth_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&if m == 0 { mip_view(1.min(mips - 1)) } else { src }) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&dst) },
                ],
            }));
            dims.push(((size.0 >> m).max(1), (size.1 >> m).max(1)));
        }
        self.res = Some(Res { size, view: tex.create_view(&Default::default()), groups, dims });
        self.built = (0, 0);
        true
    }

    pub fn record(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, viewport: (u32, u32), ts: Option<wgpu::ComputePassTimestampWrites>) -> u32 {
        let Some(r) = &self.res else { return 0 };
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[viewport.0, viewport.1, 0, 0]));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("hiz"), timestamp_writes: ts });
        for (m, g) in r.groups.iter().enumerate() {
            let (w, h) = r.dims[m];
            pass.set_pipeline(if m == 0 { &self.copy } else { &self.down });
            pass.set_bind_group(0, g, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        self.built = viewport;
        r.groups.len() as u32
    }
}
