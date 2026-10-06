//! GPU buffers of the scene, the mesh pipelines (one per family, colour and depth) and the baked
//! animation texture.
use super::{
    InstanceGpu,
    models::{DrawCmd, Family, Geometry, Layout, Model, RigidVertex, SkinnedVertex, model_gpu},
};
use crate::shader;
use wgpu::util::DeviceExt;

pub struct Buffers {
    pub rigid_vb: wgpu::Buffer,
    pub rigid_ib: wgpu::Buffer,
    pub skinned_vb: wgpu::Buffer,
    pub skinned_ib: wgpu::Buffer,
    pub quad_ib: wgpu::Buffer,
    pub instances: wgpu::Buffer,
    pub models: wgpu::Buffer,
    pub cmds: wgpu::Buffer,
    pub template: wgpu::Buffer,
    pub list: wgpu::Buffer,
    pub glows: wgpu::Buffer,
    pub layout: Layout,
    pub bytes: u64,
}

fn init(device: &wgpu::Device, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    // never zero-sized: wgpu rejects empty bindings
    let mut v = data.to_vec();
    if v.is_empty() {
        v.resize(64, 0);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: &v, usage })
}

impl Buffers {
    pub fn new(device: &wgpu::Device, g: &Geometry, models: &[Model], instances: &[InstanceGpu], glows: &[[f32; 4]], layout: Layout, views: usize) -> Buffers {
        use wgpu::BufferUsages as U;
        let mut template: Vec<DrawCmd> = Vec::with_capacity(layout.cmds.len() * views);
        for v in 0..views {
            for c in &layout.cmds {
                template.push(DrawCmd { first_instance: c.first_instance + v as u32 * layout.stride, ..*c });
            }
        }
        let gm: Vec<_> = models.iter().map(model_gpu).collect();
        let list_bytes = u64::from(layout.stride) * views as u64 * 4;
        let b = Buffers {
            rigid_vb: init(device, "rigid vertices", bytemuck::cast_slice(&g.rigid_v), U::VERTEX),
            rigid_ib: init(device, "rigid indices", bytemuck::cast_slice(&g.rigid_i), U::INDEX),
            skinned_vb: init(device, "skinned vertices", bytemuck::cast_slice(&g.skinned_v), U::VERTEX),
            skinned_ib: init(device, "skinned indices", bytemuck::cast_slice(&g.skinned_i), U::INDEX),
            quad_ib: init(device, "impostor quad", bytemuck::cast_slice(&[0u32, 1, 2, 2, 1, 3]), U::INDEX),
            instances: init(device, "instances", bytemuck::cast_slice(instances), U::STORAGE | U::COPY_DST),
            models: init(device, "models", bytemuck::cast_slice(&gm), U::STORAGE),
            cmds: init(device, "indirect", bytemuck::cast_slice(&template), U::STORAGE | U::INDIRECT | U::COPY_DST | U::COPY_SRC),
            template: init(device, "indirect template", bytemuck::cast_slice(&template), U::COPY_SRC),
            list: device.create_buffer(&wgpu::BufferDescriptor { label: Some("visible lists"), size: list_bytes.max(64), usage: U::STORAGE, mapped_at_creation: false }),
            glows: init(device, "glows", bytemuck::cast_slice(glows), U::STORAGE),
            bytes: 0,
            layout,
        };
        let bytes = [&b.rigid_vb, &b.rigid_ib, &b.skinned_vb, &b.skinned_ib, &b.instances, &b.models, &b.cmds, &b.template, &b.list].iter().map(|x| x.size()).sum();
        Buffers { bytes, ..b }
    }
}

pub struct Animation {
    pub view: wgpu::TextureView,
    pub clips: wgpu::Buffer,
    pub bytes: u64,
}

impl Animation {
    pub fn empty(device: &wgpu::Device, queue: &wgpu::Queue) -> Animation {
        Animation::new(device, queue, 1, &[[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]], &[(0, 1)])
    }

    /// `frames`: for each frame, for each joint, 3 rows of the 3x4 skinning matrix.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, joints: u32, frames: &[[f32; 4]], clips: &[(u32, u32)]) -> Animation {
        let width = joints * 3;
        let height = (frames.len() as u32 / width).max(1);
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("animation"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            tex.as_image_copy(),
            bytemuck::cast_slice(frames),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 16), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let clip_data: Vec<[f32; 4]> = clips.iter().map(|(f, n)| [*f as f32, *n as f32, joints as f32, 1.0]).collect();
        Animation {
            view: tex.create_view(&Default::default()),
            clips: init(device, "clips", bytemuck::cast_slice(&clip_data), wgpu::BufferUsages::STORAGE),
            bytes: u64::from(width * height) * 16,
        }
    }
}

pub struct Pipes {
    layout: wgpu::BindGroupLayout,
    rigid: wgpu::RenderPipeline,
    rigid_depth: wgpu::RenderPipeline,
    skinned: wgpu::RenderPipeline,
    skinned_depth: wgpu::RenderPipeline,
    impostor: wgpu::RenderPipeline,
    glow: wgpu::RenderPipeline,
    pub group: Option<wgpu::BindGroup>,
    pub atlas: Option<wgpu::TextureView>,
    dummy_atlas: wgpu::TextureView,
}

impl Pipes {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout) -> Pipes {
        let vf = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vf,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let tex = |binding, filterable, dim| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vf,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable }, view_dimension: dim, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meshes"),
            entries: &[
                storage(0),
                storage(1),
                tex(2, false, wgpu::TextureViewDimension::D2),
                storage(3),
                tex(4, true, wgpu::TextureViewDimension::D2Array),
                storage(5),
                storage(6),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("meshes"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        let module = shader(device, "meshes", &[include_str!("../shaders/common.wgsl"), include_str!("../shaders/mesh.wgsl")]);
        const RIGID: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Unorm8x4, 3 => Unorm8x4];
        const SKINNED: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Uint8x4, 5 => Unorm8x4];
        let rigid_vb = [Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<RigidVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &RIGID })];
        let skinned_vb = [Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<SkinnedVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &SKINNED })];
        let back = Some(wgpu::Face::Back);
        let dummy_atlas = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("no atlas"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 2 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        Pipes {
            rigid: crate::pipes::scene(device, &pl, &module, "rigid_vs", Some("mesh_fs"), &rigid_vb, back),
            rigid_depth: crate::pipes::shadow(device, &pl, &module, "rigid_depth_vs", &rigid_vb, None),
            skinned: crate::pipes::scene(device, &pl, &module, "skinned_vs", Some("mesh_fs"), &skinned_vb, back),
            skinned_depth: crate::pipes::shadow(device, &pl, &module, "skinned_depth_vs", &skinned_vb, None),
            impostor: crate::pipes::scene(device, &pl, &module, "impostor_vs", Some("impostor_fs"), &[], None),
            glow: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("glows"),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("glow_vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::context::DEPTH,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("glow_fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::context::HDR,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                            alpha: wgpu::BlendComponent::OVER,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            }),
            layout,
            group: None,
            atlas: None,
            dummy_atlas,
        }
    }

    pub fn ensure(&mut self, device: &wgpu::Device, gpu: &Buffers, anim: &Animation) {
        if self.group.is_some() {
            return;
        }
        let atlas = self.atlas.as_ref().unwrap_or(&self.dummy_atlas);
        self.group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("meshes"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: gpu.instances.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: gpu.list.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&anim.view) },
                wgpu::BindGroupEntry { binding: 3, resource: anim.clips.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(atlas) },
                wgpu::BindGroupEntry { binding: 5, resource: gpu.models.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: gpu.glows.as_entire_binding() },
            ],
        }));
    }

    pub fn bind_glows(&self, pass: &mut wgpu::RenderPass) -> bool {
        let Some(group) = &self.group else { return false };
        pass.set_bind_group(1, group, &[]);
        pass.set_pipeline(&self.glow);
        true
    }

    /// Pipeline and geometry of a family; false when there is nothing bound yet.
    pub fn bind(&self, pass: &mut wgpu::RenderPass, gpu: &Buffers, fam: Family, depth: bool) -> bool {
        let Some(group) = &self.group else { return false };
        pass.set_bind_group(1, group, &[]);
        match fam {
            Family::Rigid => {
                pass.set_pipeline(if depth { &self.rigid_depth } else { &self.rigid });
                pass.set_vertex_buffer(0, gpu.rigid_vb.slice(..));
                pass.set_index_buffer(gpu.rigid_ib.slice(..), wgpu::IndexFormat::Uint32);
            }
            Family::Skinned => {
                pass.set_pipeline(if depth { &self.skinned_depth } else { &self.skinned });
                pass.set_vertex_buffer(0, gpu.skinned_vb.slice(..));
                pass.set_index_buffer(gpu.skinned_ib.slice(..), wgpu::IndexFormat::Uint32);
            }
            Family::Impostor => {
                pass.set_pipeline(&self.impostor);
                pass.set_index_buffer(gpu.quad_ib.slice(..), wgpu::IndexFormat::Uint32);
            }
        }
        true
    }
}
