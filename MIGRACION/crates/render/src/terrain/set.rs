//! One terrain per body with a GPU-capable surface, sharing the grid, pipelines and generator
//! kernel; registered in the frame as a render system (compute generation, opaque, shadows).
use super::{Frusta, Shared, Terrain, TerrainConfig, TerrainStats, draw::Draw, generator::Kernel};
use crate::{
    graph::{self, Draws, EncodeCx, PrepareCx, RenderSystem, Stage},
    timing::GpuPass,
};
use glam::DVec3;
use lunar_core::body::BodyRegistry;
use std::sync::Arc;

pub struct TerrainSet {
    pub cfg: TerrainConfig,
    bodies: Arc<BodyRegistry>,
    draw: Draw,
    kernel: Kernel,
    terrains: Vec<Terrain>,
    pub stats: TerrainStats,
}

impl TerrainSet {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals: &wgpu::BindGroupLayout,
        detail: &wgpu::TextureView,
        cfg: TerrainConfig,
        sun: DVec3,
        bodies: Arc<BodyRegistry>,
    ) -> TerrainSet {
        let draw = Draw::new(device, globals, cfg.grid);
        let kernel = Kernel::new(device);
        let terrains = {
            let sh = Shared { device, queue, draw: &draw, kernel: &kernel, detail, sun };
            bodies.iter().filter_map(|(id, b)| Terrain::new(&sh, cfg, id, b)).collect()
        };
        TerrainSet { cfg, bodies, draw, kernel, terrains, stats: TerrainStats::default() }
    }

    /// New settings or sun (baked shadows): regenerate every body's terrain.
    pub fn rebuild(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, globals: &wgpu::BindGroupLayout, detail: &wgpu::TextureView, cfg: TerrainConfig, sun: DVec3) {
        *self = TerrainSet::new(device, queue, globals, detail, cfg, sun, self.bodies.clone());
    }

    pub fn len(&self) -> usize {
        self.terrains.len()
    }

    pub fn is_empty(&self) -> bool {
        self.terrains.is_empty()
    }
}

impl RenderSystem for TerrainSet {
    fn stages(&self) -> u8 {
        graph::OPAQUE | graph::SHADOW
    }

    fn prepare(&mut self, cx: &PrepareCx) {
        let s = cx.settings;
        let frusta = Frusta { main: cx.main, cascades: &cx.cascades.frusta[..cx.cascades.count] };
        let mut stats = TerrainStats::default();
        for t in &mut self.terrains {
            t.update(cx.queue, self.bodies.get(t.body), cx.view, &frusta, f64::from(s.terrain_split), s.terrain_gen_per_frame);
            stats.add(&t.stats);
        }
        self.stats = stats;
    }

    fn before_views(&mut self, cx: &mut EncodeCx) {
        let mut any = false;
        for t in &self.terrains {
            any |= t.generator.upload(cx.queue);
        }
        if !any {
            return;
        }
        let mut pass = cx.enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("terrain gen"), timestamp_writes: cx.timer.compute(GpuPass::TerrainGen) });
        for t in &mut self.terrains {
            t.generator.dispatch(&self.kernel, &mut pass);
        }
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
        let (view, depth) = match stage {
            Stage::Opaque => (0, false),
            // terrain only casts into the static layers
            Stage::Shadow { cascade, statics: true, .. } => (cascade + 1, true),
            _ => return Draws::default(),
        };
        if self.terrains.iter().all(|t| t.visible[view].is_empty()) {
            return Draws::default();
        }
        self.draw.begin(pass, depth);
        let mut d = Draws::default();
        for t in &self.terrains {
            let (calls, tris) = t.draw(&self.draw, pass, view, depth);
            d += Draws { calls, triangles: tris };
        }
        d
    }

    fn statics_changed(&self) -> bool {
        self.terrains.iter().any(Terrain::has_work)
    }

    fn gpu_bytes(&self) -> u64 {
        self.stats.gpu_bytes
    }
}
