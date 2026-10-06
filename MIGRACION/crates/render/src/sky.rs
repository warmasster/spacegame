//! Stars (one instanced draw), Milky Way, Earth and sun (one full-screen draw), behind everything.
use crate::{
    context::{DEPTH, HDR},
    graph::{self, Draws, RenderSystem, Stage},
    shader,
};
use glam::Vec3;
use lunar_core::noise::Random;
use wgpu::util::DeviceExt;

pub struct Sky {
    sky: wgpu::RenderPipeline,
    stars: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    pub count: u32,
}

fn star_data(count: u32) -> Vec<[f32; 4]> {
    let mut rng = Random(0x57a25);
    let pole = Vec3::new(0.46, 0.31, 0.83).normalize();
    let mut out = Vec::with_capacity(count as usize * 2);
    for i in 0..count {
        let mut r = || rng.next_f64() as f32;
        let mut d = Vec3::new(r() * 2. - 1., r() * 2. - 1., r() * 2. - 1.);
        while d.length_squared() > 1.0 || d.length_squared() < 1e-4 {
            d = Vec3::new(r() * 2. - 1., r() * 2. - 1., r() * 2. - 1.);
        }
        let mut d = d.normalize();
        // a third crowd the galactic band
        if i % 3 == 0 {
            d = (d - pole * d.dot(pole) * (0.85 + 0.15 * r())).normalize();
        }
        // magnitudes: many faint, a few bright
        let m = -1.0 + 7.5 * r().powf(0.55);
        let flux = 10f32.powf(-0.4 * m);
        let temp = r();
        let color = if temp < 0.15 {
            Vec3::new(0.7, 0.8, 1.0)
        } else if temp < 0.7 {
            Vec3::new(1.0, 0.98, 0.95)
        } else if temp < 0.9 {
            Vec3::new(1.0, 0.9, 0.7)
        } else {
            Vec3::new(1.0, 0.75, 0.55)
        };
        let size = 0.9 + flux.sqrt() * 0.9;
        out.push([d.x, d.y, d.z, size]);
        out.push([color.x, color.y, color.z, flux * 0.35]);
    }
    out
}

impl Sky {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, count: u32) -> Sky {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stars"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[Some(globals), Some(&layout)],
            immediate_size: 0,
        });
        let module = shader(device, "sky", &[include_str!("shaders/common.wgsl"), include_str!("shaders/sky.wgsl")]);
        let make = |vs: &str, fs: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: HDR, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent::OVER,
        };
        let sky = make("sky_vs", "sky_fs", None);
        let stars = make("star_vs", "star_fs", Some(add));
        let mut s = Sky { sky, stars, group: empty(device, &layout), layout, count: 0 };
        s.set_count(device, count);
        s
    }

    pub fn set_count(&mut self, device: &wgpu::Device, count: u32) {
        let data = star_data(count.max(1));
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("stars"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE,
        });
        self.group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stars"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        self.count = count;
    }

    /// After the opaque geometry: only pixels still at the far plane are touched. Returns draws.
    fn draw_sky(&self, pass: &mut wgpu::RenderPass) -> u32 {
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.stars);
        pass.draw(0..6, 0..self.count);
        2
    }
}

fn empty(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::BindGroup {
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 32, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
    })
}

impl RenderSystem for Sky {
    fn stages(&self) -> u8 {
        graph::SKY
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, _stage: Stage) -> Draws {
        Draws { calls: self.draw_sky(pass), triangles: 0 }
    }
}
