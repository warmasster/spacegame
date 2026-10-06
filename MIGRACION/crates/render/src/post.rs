//! Bloom, tone mapping (with the dynamic-resolution upscale) and FXAA into the swapchain image.
use crate::{
    context::HDR,
    graph::{EncodeCx, RenderSystem},
    shader,
    targets::Targets,
    timing::GpuPass,
};
use bytemuck::{Pod, Zeroable};

const BLOOM_MIPS: u32 = 5;
const SLOT: u64 = 256;
const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostU {
    src: [f32; 4],
    params: [f32; 4],
    /// The helmet's visor (`Visor`): its filter and the breath on it; its sun visor and how far
    /// down it is; the glare left under it and the picture's width over its height.
    visor: [[f32; 4]; 3],
}

const POST_U: u64 = std::mem::size_of::<PostU>() as u64;

/// The visor the picture is seen through, from the player's own eyes: a few numbers of the tone
/// mapping pass, no pass of its own. `Visor::default()` is no visor at all (seen from outside).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visor {
    /// What all of it lets through (linear rgb; 1: clear): the filter that darkens for a welding arc.
    pub filter: [f32; 3],
    /// What its sun visor lets through, how far down it is (0 up .. 1 down) and the share of
    /// the glare (bloom) it leaves.
    pub sun: [f32; 3],
    pub lowered: f32,
    pub glare: f32,
    /// Breath on it (0 none .. 1): mist from its rim inward.
    pub mist: f32,
    /// How hard its filter, and its sun visor where it is down, press down what is bright (0:
    /// not at all): light through them is divided by 1 + this times how bright it is, so what
    /// is dim is hardly dimmed and what would burn the picture white is brought into it.
    pub knee: f32,
    pub sun_knee: f32,
}

impl Default for Visor {
    fn default() -> Visor {
        Visor { filter: [1.0; 3], sun: [1.0; 3], lowered: 0.0, glare: 1.0, mist: 0.0, knee: 0.0, sun_knee: 0.0 }
    }
}

impl crate::renderer::Renderer {
    /// The visor this frame's picture is seen through.
    pub fn set_visor(&mut self, visor: Visor) {
        self.post.visor = visor;
    }
}

pub struct Post {
    pub visor: Visor,
    /// The picture's width over its height (the breath on the visor is laid out by it).
    aspect: f32,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
    first: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    direct: wgpu::RenderPipeline,
    ldr_pipe: wgpu::RenderPipeline,
    fxaa: wgpu::RenderPipeline,
    res: Option<Resources>,
    staging: Vec<u8>,
}

struct Resources {
    size: (u32, u32),
    out: (u32, u32),
    mips: Vec<wgpu::TextureView>,
    mip_sizes: Vec<(u32, u32)>,
    ldr: wgpu::TextureView,
    g_first: wgpu::BindGroup,
    g_down: Vec<wgpu::BindGroup>,
    g_up: Vec<wgpu::BindGroup>,
    g_tone: wgpu::BindGroup,
    g_fxaa: wgpu::BindGroup,
}

pub struct PostSettings {
    pub exposure: f32,
    pub bloom: bool,
    pub bloom_strength: f32,
    pub fxaa: bool,
}

impl Post {
    pub fn new(device: &wgpu::Device, out_format: wgpu::TextureFormat) -> Post {
        let fs = wgpu::ShaderStages::FRAGMENT;
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: fs,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: fs,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(POST_U) },
                    count: None,
                },
                tex(1),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: fs, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                tex(3),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("post"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let module = shader(device, "post", &[include_str!("shaders/post.wgsl")]);
        let make = |fs: &str, format, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("fullscreen"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        Post {
            first: make("bloom_first", HDR, None),
            down: make("bloom_down", HDR, None),
            up: make("bloom_up", HDR, Some(add)),
            direct: make("tonemap_direct", out_format, None),
            ldr_pipe: make("tonemap_ldr", LDR, None),
            fxaa: make("fxaa", out_format, None),
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("post"),
                size: SLOT * 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("post"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
            res: None,
            staging: vec![0; (SLOT * 16) as usize],
            visor: Visor::default(),
            aspect: 1.0,
        }
    }

    fn ensure(&mut self, device: &wgpu::Device, targets: &Targets, out: (u32, u32)) {
        let size = (targets.width, targets.height);
        if self.res.as_ref().is_some_and(|r| r.size == size && r.out == out) {
            return;
        }
        let (bw, bh) = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        let bloom = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bloom"),
            size: wgpu::Extent3d { width: bw, height: bh, depth_or_array_layers: 1 },
            mip_level_count: BLOOM_MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mips: Vec<_> = (0..BLOOM_MIPS)
            .map(|m| bloom.create_view(&wgpu::TextureViewDescriptor { base_mip_level: m, mip_level_count: Some(1), ..Default::default() }))
            .collect();
        let mip_sizes = (0..BLOOM_MIPS).map(|m| ((bw >> m).max(1), (bh >> m).max(1))).collect();
        let ldr = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("ldr"),
                size: wgpu::Extent3d { width: out.0, height: out.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: LDR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let group = |src: &wgpu::TextureView, bloom: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.uniform, offset: 0, size: wgpu::BufferSize::new(POST_U) }),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(bloom) },
                ],
            })
        };
        let g_first = group(&targets.hdr_view, &mips[1]);
        let g_down = (1..BLOOM_MIPS as usize).map(|m| group(&mips[m - 1], &mips[0])).collect();
        let g_up = (0..BLOOM_MIPS as usize - 1).map(|m| group(&mips[m + 1], &mips[BLOOM_MIPS as usize - 1])).collect();
        let g_tone = group(&targets.hdr_view, &mips[0]);
        let g_fxaa = group(&ldr, &mips[0]);
        self.res = Some(Resources { size, out, mips, mip_sizes, ldr, g_first, g_down, g_up, g_tone, g_fxaa });
    }

    fn slot(&mut self, i: usize, src: [f32; 4], params: [f32; 4]) -> u32 {
        let at = i * SLOT as usize;
        let v = &self.visor;
        let visor = [[v.filter[0], v.filter[1], v.filter[2], v.mist], [v.sun[0], v.sun[1], v.sun[2], v.lowered], [v.glare, self.aspect, v.knee, v.sun_knee]];
        self.staging[at..at + POST_U as usize].copy_from_slice(bytemuck::bytes_of(&PostU { src, params, visor }));
        (i as u64 * SLOT) as u32
    }

    /// Record the post chain from `targets.hdr` into `out`. Returns the number of passes (draws).
    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets,
        out: &wgpu::TextureView,
        out_size: (u32, u32),
        s: &PostSettings,
        timestamps: Option<(&wgpu::QuerySet, u32)>,
    ) -> u32 {
        self.ensure(device, targets, out_size);
        let (vw, vh) = targets.viewport();
        let scale = [vw as f32 / targets.width as f32, vh as f32 / targets.height as f32];
        let hdr_texel = [1.0 / targets.width as f32, 1.0 / targets.height as f32];
        self.aspect = out_size.0 as f32 / out_size.1.max(1) as f32;
        let params = [s.exposure, s.bloom_strength, if s.bloom { 1.0 } else { 0.0 }, 0.0];
        let res = self.res.take().expect("post resources");
        let mut draws = 0;
        // uniforms: 0 first, 1..4 down, 5..8 up, 9 tone, 10 fxaa
        let o_first = self.slot(0, [scale[0], scale[1], hdr_texel[0], hdr_texel[1]], params);
        let mut o_down = [0u32; BLOOM_MIPS as usize];
        for m in 1..BLOOM_MIPS as usize {
            let (w, h) = res.mip_sizes[m - 1];
            o_down[m] = self.slot(m, [1.0, 1.0, 1.0 / w as f32, 1.0 / h as f32], params);
        }
        let mut o_up = [0u32; BLOOM_MIPS as usize];
        for m in 0..BLOOM_MIPS as usize - 1 {
            let (w, h) = res.mip_sizes[m + 1];
            o_up[m] = self.slot(5 + m, [1.0, 1.0, 1.0 / w as f32, 1.0 / h as f32], params);
        }
        let o_tone = self.slot(9, [scale[0], scale[1], hdr_texel[0], hdr_texel[1]], params);
        let o_fxaa = self.slot(10, [1.0, 1.0, 1.0 / out_size.0 as f32, 1.0 / out_size.1 as f32], params);
        queue.write_buffer(&self.uniform, 0, &self.staging[..(SLOT * 11) as usize]);
        // (pipeline, target, group, offset, additive): a fixed array, nothing allocated per frame
        let mut passes = [(&self.direct, out, &res.g_tone, o_tone, false); 2 * BLOOM_MIPS as usize + 2];
        let mut n = 0;
        let mut push = |p| {
            passes[n] = p;
            n += 1;
        };
        if s.bloom {
            push((&self.first, &res.mips[0], &res.g_first, o_first, false));
            for m in 1..BLOOM_MIPS as usize {
                push((&self.down, &res.mips[m], &res.g_down[m - 1], o_down[m], false));
            }
            for m in (0..BLOOM_MIPS as usize - 1).rev() {
                push((&self.up, &res.mips[m], &res.g_up[m], o_up[m], true));
            }
        }
        if s.fxaa {
            push((&self.ldr_pipe, &res.ldr, &res.g_tone, o_tone, false));
            push((&self.fxaa, out, &res.g_fxaa, o_fxaa, false));
        } else {
            push((&self.direct, out, &res.g_tone, o_tone, false));
        }
        let passes = &passes[..n];
        let last = passes.len() - 1;
        for (i, (pipe, target, group, offset, load)) in passes.iter().enumerate() {
            let ts = timestamps.and_then(|(q, idx)| {
                (i == 0 || i == last).then_some(wgpu::RenderPassTimestampWrites {
                    query_set: q,
                    beginning_of_pass_write_index: (i == 0).then_some(idx),
                    end_of_pass_write_index: (i == last).then_some(idx + 1),
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: if *load { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) }, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: ts,
                ..Default::default()
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, *group, &[*offset]);
            pass.draw(0..3, 0..1);
        }
        draws += passes.len() as u32;
        self.res = Some(res);
        draws
    }
}

impl RenderSystem for Post {
    fn after_views(&mut self, cx: &mut EncodeCx) -> u32 {
        let s = cx.settings;
        let ps = PostSettings { exposure: s.exposure, bloom: s.bloom, bloom_strength: s.bloom_strength, fxaa: s.fxaa };
        let ts = cx.timer.pass(GpuPass::Post);
        self.record(cx.device, cx.queue, cx.enc, cx.targets, cx.out, cx.out_size, &ps, ts)
    }
}
