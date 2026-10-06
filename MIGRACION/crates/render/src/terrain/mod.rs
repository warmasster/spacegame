//! Whole-body CDLOD terrain: six quadtrees per body, nodes generated on the GPU into texture arrays
//! (an LRU cache of slots), one shared grid drawn instanced — one draw per body for the view and
//! for each cascade. `TerrainSet` (set.rs) keeps one `Terrain` per body with a procedural surface.
mod draw;
pub mod generator;
mod set;

pub use set::TerrainSet;

use crate::frame::View;
use generator::{Generator, GeneratorDesc, Kernel, NodeDesc};
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyId},
    deform::{Crater, MAX_NODE_CRATERS},
    frustum::Frustum,
    quadtree::{QuadConfig, Quadtree, skirt},
    terrain_gen::NodeKey,
};

pub const HEIGHT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Uint;
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Views: main, then up to four cascades.
pub const VIEWS: usize = 5;
/// Nodes a terrain's cache holds at least (the six roots and a generation batch).
const MIN_CAPACITY: u32 = 96;
const MAX_JOBS: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainConfig {
    pub grid: u32,
    pub capacity: u32,
    pub finest_cell: f64,
    pub baked_shadows: bool,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Visible {
    center: [f32; 3],
    slot: u32,
    /// Morph start and end (m from the camera): set every frame from the split in use.
    morph: [f32; 2],
    pad: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerrainStats {
    pub drawn: [u32; VIEWS],
    pub cached: u32,
    pub capacity: u32,
    pub generated: u32,
    pub pending: u32,
    /// Nodes waiting to be regenerated (ground changed).
    pub stale: u32,
    /// Share of the asked LOD distance the node cache holds.
    pub split_scale: f32,
    pub triangles_per_node: u32,
    pub max_level: u32,
    pub deepest: u32,
    pub gpu_bytes: u64,
}

impl TerrainStats {
    /// Sum over bodies (levels: the deepest).
    fn add(&mut self, o: &TerrainStats) {
        for (a, b) in self.drawn.iter_mut().zip(o.drawn) {
            *a += b;
        }
        self.cached += o.cached;
        self.capacity += o.capacity;
        self.generated += o.generated;
        self.pending += o.pending;
        self.stale += o.stale;
        self.split_scale = if self.split_scale == 0.0 { o.split_scale } else { self.split_scale.min(o.split_scale) };
        self.triangles_per_node = o.triangles_per_node;
        self.max_level = self.max_level.max(o.max_level);
        self.deepest = self.deepest.max(o.deepest);
        self.gpu_bytes += o.gpu_bytes;
    }
}

pub struct Frusta<'a> {
    pub main: &'a Frustum,
    pub cascades: &'a [Frustum],
}

/// Shared GPU pieces every body's terrain is built with.
pub struct Shared<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub draw: &'a draw::Draw,
    pub kernel: &'a Kernel,
    pub detail: &'a wgpu::TextureView,
    pub sun: DVec3,
}

/// A body's terrain on the GPU: the quadtree (core) decides, this generates and draws.
pub struct Terrain {
    pub body: BodyId,
    center: DVec3,
    radius: f64,
    capacity: u32,
    grid: u32,
    quad: Quadtree,
    /// Deform version the nodes were generated with.
    seen: u64,
    craters: [Crater; MAX_NODE_CRATERS],
    generator: Generator,
    node_buf: wgpu::Buffer,
    visible_buf: wgpu::Buffer,
    visible: [Vec<Visible>; VIEWS],
    group: wgpu::BindGroup,
    pub stats: TerrainStats,
}

impl Terrain {
    /// None when the body has no surface with a GPU generator.
    pub fn new(sh: &Shared, cfg: TerrainConfig, id: BodyId, body: &Body) -> Option<Terrain> {
        let surface = body.surface.clone()?;
        let procedural = surface.procedural()?;
        let device = sh.device;
        let capacity = ((f64::from(cfg.capacity) * body.terrain_cache) as u32).max(MIN_CAPACITY);
        let side = cfg.grid + 1;
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: capacity },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let heights = texture("terrain heights", HEIGHT_FORMAT);
        let normals = texture("terrain normals", NORMAL_FORMAT);
        let array = wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() };
        let (hv, nv) = (heights.create_view(&array), normals.create_view(&array));
        let border = if cfg.baked_shadows { 12 } else { 1 };
        let generator = Generator::new(
            device,
            sh.kernel,
            procedural,
            &GeneratorDesc { sun: sh.sun, grid: cfg.grid, border, baked: cfg.baked_shadows, max_jobs: MAX_JOBS, heights: &hv, normals: &nv },
        );
        let buffer = |label, size: usize| {
            device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
        };
        let node_buf = buffer("terrain nodes", std::mem::size_of::<generator::NodeGpu>() * capacity as usize);
        let visible_buf = buffer("terrain visible", std::mem::size_of::<Visible>() * capacity as usize * VIEWS);
        let group = sh.draw.bind_group(device, &draw::Bindings { nodes: &node_buf, visible: &visible_buf, heights: &hv, normals: &nv, detail: sh.detail });
        let quad = Quadtree::new(body, surface.clone(), QuadConfig { grid: cfg.grid, capacity, finest_cell: cfg.finest_cell });
        let texel_bytes = 8 + 8;
        let gpu_bytes = u64::from(side * side * capacity) * texel_bytes + node_buf.size() + visible_buf.size();
        let max_level = quad.max_level();
        let mut t = Terrain {
            body: id,
            center: body.center,
            radius: body.radius,
            capacity,
            grid: cfg.grid,
            seen: body.deform().version(),
            craters: [Crater { dir: DVec3::Y, radius: 1.0, depth: 0.0, rim: 0.0, seed: 0.0, ground: 0.0 }; MAX_NODE_CRATERS],
            quad,
            generator,
            node_buf,
            visible_buf,
            visible: std::array::from_fn(|_| Vec::with_capacity(capacity as usize)),
            group,
            stats: TerrainStats { capacity, max_level, triangles_per_node: cfg.grid * cfg.grid * 2, gpu_bytes, ..Default::default() },
        };
        for i in 0..t.quad.created.len() {
            let (key, slot) = t.quad.created[i];
            t.generate(sh.queue, body, key, slot);
        }
        t.quad.created.clear();
        Some(t)
    }

    /// Queue node `key` for generation into `slot`, with the craters on its ground.
    fn generate(&mut self, queue: &wgpu::Queue, body: &Body, key: NodeKey, slot: u32) {
        let arc = key.arc(self.radius);
        let desc = NodeDesc { frame: key.frame(), layer: slot, level: u32::from(key.level), skirt: skirt(arc, self.grid) as f32 };
        let at = u64::from(slot) * std::mem::size_of::<generator::NodeGpu>() as u64;
        queue.write_buffer(&self.node_buf, at, bytemuck::bytes_of(&generator::node_gpu(&desc, self.grid, self.radius)));
        let n = body.deform().touching(desc.frame.dir, arc * 0.95, self.radius, &mut self.craters);
        // its bounding sphere holds the ground the craters moved (a node deep in one would be
        // culled where it really is): every crater on it at its deepest or highest, and what
        // levelling a slope under it may move (a fifth of its radius), added up
        let moved: f64 = self.craters[..n].iter().map(|c| c.depth * c.rim.max(1.0) + c.radius * 0.2).sum();
        self.quad.set_moved(key, moved);
        if let Some(p) = body.surface.as_ref().and_then(|s| s.procedural()) {
            self.generator.push(p, &desc, arc / f64::from(self.grid), &self.craters[..n]);
        }
    }

    /// Choose the nodes for this frame, generate a batch (new nodes first, then nodes whose
    /// ground changed), upload the per-view lists.
    pub fn update(&mut self, queue: &wgpu::Queue, body: &Body, view: &View, frusta: &Frusta, split: f64, budget: u32) {
        // blast craters and other ground changes since the last frame
        let version = body.deform().version();
        if version != self.seen {
            let d = body.deform();
            match d.changes_since(self.seen) {
                Some(changes) => changes.for_each(|(dir, reach)| self.quad.invalidate(dir, reach)),
                None => self.quad.invalidate_all(),
            }
            drop(d);
            self.seen = version;
        }
        let cam = view.eye;
        let main = frusta.main;
        let batch = (budget as usize).min(MAX_JOBS as usize);
        self.quad.update(cam, split, batch, |rel, r| main.sphere(rel.as_vec3(), r as f32));
        let mut generated = 0;
        for i in 0..self.quad.created.len() {
            let (key, slot) = self.quad.created[i];
            self.generate(queue, body, key, slot);
            generated += 1;
        }
        while generated < MAX_JOBS as usize {
            let Some((key, slot)) = self.quad.next_stale() else { break };
            self.generate(queue, body, key, slot);
            generated += 1;
        }
        for v in &mut self.visible {
            v.clear();
        }
        // morph: a node turns into its parent as it nears where the parent stops splitting
        let split = self.quad.split(split);
        for s in &self.quad.selected {
            let rel32 = (s.center - cam).as_vec3();
            let arc = s.key.arc(self.radius);
            let parent_range = split * arc * 2.0;
            let morph = if s.key.level == 0 { [1e12, 2e12] } else { [(parent_range * 0.62) as f32, (parent_range * 0.96) as f32] };
            // the datum-sphere centre, camera-relative: what the vertex shader offsets from
            let sphere = (self.center + s.key.frame().dir * self.radius - cam).as_vec3();
            let entry = Visible { center: sphere.to_array(), slot: s.slot, morph, pad: [0.0; 2] };
            if main.sphere(rel32, s.radius as f32) {
                self.visible[0].push(entry);
            }
            for (c, f) in frusta.cascades.iter().enumerate() {
                if f.sphere(rel32, s.radius as f32) {
                    self.visible[c + 1].push(entry);
                }
            }
        }
        for (v, list) in self.visible.iter().enumerate() {
            if !list.is_empty() {
                let at = (v * self.capacity as usize * std::mem::size_of::<Visible>()) as u64;
                queue.write_buffer(&self.visible_buf, at, bytemuck::cast_slice(list));
            }
        }
        self.stats.drawn = std::array::from_fn(|i| self.visible[i].len() as u32);
        self.stats.cached = self.quad.cached() as u32;
        self.stats.generated = generated as u32;
        self.stats.pending = self.quad.pending as u32;
        self.stats.stale = self.quad.stale_len() as u32;
        self.stats.deepest = self.quad.deepest;
        self.stats.split_scale = self.quad.split(1.0) as f32;
    }

    pub fn has_work(&self) -> bool {
        self.generator.queued() > 0
    }

    /// Draw view `v` (0 main, 1.. cascades) after `Draw::begin`; returns draw calls and triangles.
    fn draw(&self, d: &draw::Draw, pass: &mut wgpu::RenderPass, v: usize, depth_only: bool) -> (u32, u64) {
        let count = self.visible[v].len() as u32;
        if count == 0 {
            return (0, 0);
        }
        let first = v as u32 * self.capacity;
        d.draw(pass, &self.group, first..first + count, depth_only);
        (1, u64::from(count) * u64::from(d.triangles(depth_only)))
    }
}
