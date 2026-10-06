//! The terrain's grid, bind group and pipelines (colour and depth-only).
use crate::shader;
use std::ops::Range;
use wgpu::util::DeviceExt;

/// Pipelines, bind group layout and grid shared by every body's terrain.
pub struct Draw {
    main: wgpu::RenderPipeline,
    depth: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    index: wgpu::Buffer,
    full: u32,
    grid_only: u32,
}

/// The per-body resources the terrain shader reads.
pub struct Bindings<'a> {
    pub nodes: &'a wgpu::Buffer,
    pub visible: &'a wgpu::Buffer,
    pub heights: &'a wgpu::TextureView,
    pub normals: &'a wgpu::TextureView,
    pub detail: &'a wgpu::TextureView,
}

impl Draw {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, grid: u32) -> Draw {
        let (indices, grid_only) = grid_indices(grid);
        let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain grid"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let vf = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let tex = |binding, filterable, dim| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vf,
            // binding 2: heights, raw bits (f32 height, f16 sun visibility + f16 parent delta)
            ty: wgpu::BindingType::Texture {
                sample_type: if binding == 2 { wgpu::TextureSampleType::Uint } else { wgpu::TextureSampleType::Float { filterable } },
                view_dimension: dim,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &[
                storage(0),
                storage(1),
                tex(2, false, wgpu::TextureViewDimension::D2Array),
                tex(3, true, wgpu::TextureViewDimension::D2Array),
                tex(4, true, wgpu::TextureViewDimension::D2),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(globals), Some(&layout)],
            immediate_size: 0,
        });
        let module = shader(
            device,
            "terrain",
            &[include_str!("../shaders/common.wgsl"), include_str!("../shaders/terrain_common.wgsl"), include_str!("../shaders/terrain.wgsl")],
        );
        let main = crate::pipes::scene(device, &pl, &module, "terrain_vs", Some("terrain_fs"), &[], None);
        let depth = crate::pipes::shadow(device, &pl, &module, "terrain_depth_vs", &[], None);
        Draw { main, depth, layout, index, full: indices.len() as u32, grid_only }
    }

    pub fn bind_group(&self, device: &wgpu::Device, b: &Bindings) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: b.nodes.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: b.visible.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(b.heights) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(b.normals) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(b.detail) },
            ],
        })
    }

    /// Pipeline and grid for a run of terrain draws.
    pub fn begin(&self, pass: &mut wgpu::RenderPass, depth_only: bool) {
        pass.set_pipeline(if depth_only { &self.depth } else { &self.main });
        pass.set_index_buffer(self.index.slice(..), wgpu::IndexFormat::Uint32);
    }

    /// One body's visible nodes (after `begin`).
    pub fn draw(&self, pass: &mut wgpu::RenderPass, group: &wgpu::BindGroup, instances: Range<u32>, depth_only: bool) {
        pass.set_bind_group(1, group, &[]);
        // skirts never cast: they would draw the node's border into the shadow maps
        pass.draw_indexed(0..if depth_only { self.grid_only } else { self.full }, 0, instances);
    }

    pub fn triangles(&self, depth_only: bool) -> u32 {
        (if depth_only { self.grid_only } else { self.full }) / 3
    }
}

/// Grid quads, then the four skirts (vertex ids past the grid: side * (n+1) + t).
fn grid_indices(n: u32) -> (Vec<u32>, u32) {
    let row = n + 1;
    let mut idx = Vec::with_capacity((n * n * 6 + 4 * n * 6) as usize);
    for j in 0..n {
        for i in 0..n {
            let v = j * row + i;
            idx.extend_from_slice(&[v, v + 1, v + row, v + 1, v + row + 1, v + row]);
        }
    }
    let grid_only = idx.len() as u32;
    let border = |side: u32, t: u32| -> u32 {
        let (i, j) = match side {
            0 => (t, 0),
            1 => (n, t),
            2 => (n - t, n),
            _ => (0, n - t),
        };
        j * row + i
    };
    for side in 0..4 {
        for t in 0..n {
            let (a, b) = (border(side, t), border(side, t + 1));
            let (sa, sb) = (row * row + side * row + t, row * row + side * row + t + 1);
            idx.extend_from_slice(&[a, sa, b, b, sa, sb]);
        }
    }
    (idx, grid_only)
}
