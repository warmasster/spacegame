//! Instanced objects of any kind (ships, NPCs, rocks...): models registered once, instances as
//! plain data near a render origin, GPU culling into indirect multi-draws — one call per family
//! per view whatever the number of objects.
mod cull;
mod draw;
mod hiz;
pub mod models;

use crate::{
    context::Caps,
    graph::{self, Draws, EncodeCx, PrepareCx, RenderSystem, Stage},
    timing::GpuPass,
};
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Quat};
use lunar_core::mesh::{Glow, Mesh};
pub use models::Family;
use models::{Geometry, Lod, Model};
use rayon::prelude::*;
use std::ops::Range;

pub const FLAG_STATIC: u32 = 1;
pub const FLAG_MOVING: u32 = 2;
/// Culling views: main, then (static, moving) per cascade.
pub const VIEWS: usize = 9;
const REBASE_M: f64 = 4000.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct InstanceGpu {
    pub pos: [f32; 3],
    pub model: u32,
    pub rot: [f32; 4],
    /// Animation phase (cycles), clip, scale.
    pub extra: [f32; 3],
    pub flags: u32,
}

#[derive(Clone, Debug, Default)]
pub struct SceneStats {
    pub instances: u32,
    pub models: Vec<String>,
    /// Visible per model per LOD in the main view (a few frames late).
    pub visible: Vec<[u32; models::MAX_LODS]>,
    pub shadow_visible: u32,
    pub triangles_main: u64,
    pub triangles_shadow: u64,
    pub multi_draws: u32,
    pub sub_draws: u32,
}

pub struct SceneGpu {
    caps: Caps,
    geometry: Geometry,
    models: Vec<Model>,
    world: Vec<DVec3>,
    instances: Vec<InstanceGpu>,
    dirty: Option<Range<usize>>,
    origin: DVec3,
    gpu: Option<draw::Buffers>,
    cull: cull::Cull,
    hiz: hiz::Hiz,
    draw: draw::Pipes,
    anim: draw::Animation,
    pub stats: SceneStats,
    glows: Vec<[f32; 4]>,
    active_views: u32,
    cascades: usize,
}

impl SceneGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals: &wgpu::BindGroupLayout, caps: Caps) -> SceneGpu {
        SceneGpu {
            caps,
            geometry: Geometry::default(),
            models: Vec::new(),
            world: Vec::new(),
            instances: Vec::new(),
            dirty: None,
            origin: DVec3::ZERO,
            gpu: None,
            cull: cull::Cull::new(device),
            hiz: hiz::Hiz::new(device),
            draw: draw::Pipes::new(device, globals),
            anim: draw::Animation::empty(device, queue),
            stats: SceneStats::default(),
            glows: Vec::new(),
            active_views: 1,
            cascades: 0,
        }
    }

    /// Register a model: LODs finest first, each with the smallest on-screen radius (px) it is used at.
    pub fn add_model(&mut self, name: &str, lods: &[(&Mesh, Family, f32)]) -> u32 {
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for p in &lods[0].0.pos {
            lo = lo.min(glam::Vec3::from_array(*p));
            hi = hi.max(glam::Vec3::from_array(*p));
        }
        let center = (lo + hi) * 0.5;
        let radius = lods[0].0.pos.iter().map(|p| glam::Vec3::from_array(*p).distance(center)).fold(0.0, f32::max);
        let lods = lods
            .iter()
            .map(|(mesh, fam, px)| {
                let (first_index, index_count, base_vertex) = if *fam == Family::Impostor { (0, 6, 0) } else { self.geometry.add(mesh, *fam) };
                Lod { family: *fam, first_index, index_count, base_vertex, min_px: *px }
            })
            .collect();
        self.models.push(Model { name: name.into(), lods, center: center.to_array(), radius, impostor_layer: None, cmds: [0; models::MAX_LODS], regions: [0; models::MAX_LODS], capacity: 0, glows: (0, 0) });
        (self.models.len() - 1) as u32
    }

    /// Far-visible glows of a model (before `set_instances`); at most 15 per model.
    pub fn add_glows(&mut self, model: u32, glows: &[Glow]) {
        let first = (self.glows.len() / 2) as u32;
        for g in glows.iter().take(15) {
            self.glows.push([g.pos[0], g.pos[1], g.pos[2], g.size]);
            self.glows.push([g.color[0], g.color[1], g.color[2], g.intensity]);
        }
        self.models[model as usize].glows = (first, glows.len().min(15) as u32);
    }

    pub fn model_radius(&self, model: u32) -> f32 {
        self.models[model as usize].radius
    }

    pub fn set_impostor_layer(&mut self, model: u32, layer: u32) {
        self.models[model as usize].impostor_layer = Some(layer);
    }

    /// Joint matrices (3 rows of 3x4 per joint) for every frame, and clips as (first frame, frames).
    pub fn set_animation(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, joints: u32, frames: &[[f32; 4]], clips: &[(u32, u32)]) {
        self.anim = draw::Animation::new(device, queue, joints, frames, clips);
        self.draw.group = None;
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    pub fn impostor_atlas(&mut self, view: wgpu::TextureView) {
        self.draw.atlas = Some(view);
        self.draw.group = None;
    }

    /// Replace every instance. `desc`: model, world position, rotation, scale, flags.
    pub fn set_instances(&mut self, device: &wgpu::Device, desc: &[(u32, DVec3, Quat, f32, u32)]) {
        self.origin = desc.first().map_or(DVec3::ZERO, |d| d.1);
        self.world = desc.iter().map(|d| d.1).collect();
        self.instances = desc
            .iter()
            .map(|(m, p, r, s, f)| InstanceGpu { pos: (*p - self.origin).as_vec3().to_array(), model: *m, rot: r.to_array(), extra: [0.0, 0.0, *s], flags: *f })
            .collect();
        let mut counts = vec![0u32; self.models.len()];
        for d in desc {
            counts[d.0 as usize] += 1;
        }
        let layout = models::layout(&mut self.models, &counts);
        self.gpu = Some(draw::Buffers::new(device, &self.geometry, &self.models, &self.instances, &self.glows, layout, VIEWS));
        self.dirty = None;
        self.stats.instances = self.instances.len() as u32;
        self.stats.models = self.models.iter().map(|m| m.name.clone()).collect();
        self.stats.visible = vec![[0; models::MAX_LODS]; self.models.len()];
        self.cull.invalidate();
        self.draw.group = None;
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Mutable poses for `range`: world positions, and the GPU records (rot, extra).
    pub fn poses(&mut self, range: Range<usize>) -> (&mut [DVec3], &mut [InstanceGpu]) {
        self.dirty = Some(match self.dirty.take() {
            Some(d) => d.start.min(range.start)..d.end.max(range.end),
            None => range.clone(),
        });
        (&mut self.world[range.clone()], &mut self.instances[range])
    }

    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.gpu.as_ref().map_or(0, |g| g.bytes) + self.anim.bytes
    }

    pub fn compute_dispatches(&self) -> u32 {
        u32::from(self.gpu.is_some()) + self.hiz.mips()
    }

    fn prepare_views(&mut self, cx: &PrepareCx) {
        let Some(gpu) = &self.gpu else { return };
        self.draw.ensure(cx.device, gpu, &self.anim);
        // keep f32 exact: re-centre when the camera wanders off
        if cx.view.eye.distance(self.origin) > REBASE_M {
            self.origin = cx.view.eye;
            self.dirty = Some(0..self.instances.len());
        }
        if let Some(r) = self.dirty.take() {
            let origin = self.origin;
            self.instances[r.clone()].par_iter_mut().zip(self.world[r.clone()].par_iter()).with_min_len(256).for_each(|(i, w)| {
                i.pos = (*w - origin).as_vec3().to_array();
            });
            cx.queue.write_buffer(&gpu.instances, (r.start * std::mem::size_of::<InstanceGpu>()) as u64, bytemuck::cast_slice(&self.instances[r]));
        }
        self.cascades = cx.cascades.count;
        self.active_views = 1 + 2 * cx.cascades.count as u32;
        let aspect = cx.viewport.0 as f32 / cx.viewport.1 as f32;
        self.cull.prepare(
            cx.device,
            cx.queue,
            gpu,
            cx.view,
            cx.prev_vp,
            self.origin,
            cx.prev_eye,
            cx.main,
            cx.cascades,
            cx.settings,
            aspect,
            cx.viewport.1 as f32,
            &self.hiz,
            self.instances.len() as u32,
            self.active_views,
            cx.sun,
        );
    }

    fn cull(&mut self, encoder: &mut wgpu::CommandEncoder, ts: Option<wgpu::ComputePassTimestampWrites>) {
        let Some(gpu) = &self.gpu else { return };
        self.cull.record(encoder, gpu, self.instances.len() as u32, ts);
    }

    fn draw_view(&mut self, pass: &mut wgpu::RenderPass, view: usize, depth: bool) -> (u32, u64) {
        let Some(gpu) = &self.gpu else { return (0, 0) };
        if !self.caps.indirect_first_instance {
            return (0, 0);
        }
        let mut draws = 0;
        for fam in [Family::Rigid, Family::Skinned, Family::Impostor] {
            let (start, count) = gpu.layout.family_ranges[fam as usize];
            if count == 0 || (depth && fam == Family::Impostor) {
                continue;
            }
            if !self.draw.bind(pass, gpu, fam, depth) {
                continue;
            }
            let offset = ((view * gpu.layout.cmds.len() + start as usize) * std::mem::size_of::<models::DrawCmd>()) as u64;
            pass.multi_draw_indexed_indirect(&gpu.cmds, offset, count);
            draws += 1;
            self.stats.sub_draws += count;
        }
        self.stats.multi_draws += draws;
        (draws, 0)
    }

    fn draw_main(&mut self, pass: &mut wgpu::RenderPass) -> (u32, u64) {
        self.stats.multi_draws = 0;
        self.stats.sub_draws = 0;
        let r = self.draw_view(pass, 0, false);
        (r.0, self.stats.triangles_main)
    }

    /// Glow sprites of every instance (engines, beacons): one draw, after the sky.
    fn draw_glows(&mut self, pass: &mut wgpu::RenderPass) -> u32 {
        if self.gpu.is_none() || self.glows.is_empty() || !self.draw.bind_glows(pass) {
            return 0;
        }
        pass.draw(0..6, 0..self.instances.len() as u32 * 16);
        1
    }

    fn draw_shadow(&mut self, pass: &mut wgpu::RenderPass, c: usize, statics: bool, moving: bool) -> (u32, u64) {
        let mut d = 0;
        if statics {
            d += self.draw_view(pass, 1 + 2 * c, true).0;
        }
        if moving {
            d += self.draw_view(pass, 2 + 2 * c, true).0;
        }
        (d, 0)
    }

    fn harvest(&mut self, device: &wgpu::Device) {
        let Some(gpu) = &self.gpu else { return };
        if let Some(counts) = self.cull.readback(device) {
            let n = gpu.layout.cmds.len();
            let mut tris_main = 0u64;
            let mut tris_shadow = 0u64;
            let mut shadow_vis = 0u32;
            for (k, m) in self.models.iter().enumerate() {
                let mut vis = [0u32; models::MAX_LODS];
                for l in 0..m.lods.len() {
                    let c = m.cmds[l] as usize;
                    vis[l] = counts[c];
                    tris_main += u64::from(counts[c]) * u64::from(m.lods[l].index_count / 3);
                    for v in 1..self.active_views as usize {
                        let cc = counts[v * n + c];
                        shadow_vis += cc;
                        tris_shadow += u64::from(cc) * u64::from(m.lods[l].index_count / 3);
                    }
                }
                self.stats.visible[k] = vis;
            }
            self.stats.triangles_main = tris_main;
            self.stats.triangles_shadow = tris_shadow;
            self.stats.shadow_visible = shadow_vis;
        }
    }
}

impl RenderSystem for SceneGpu {
    fn stages(&self) -> u8 {
        graph::OPAQUE | graph::SHADOW | graph::GLOW
    }

    fn prepare(&mut self, cx: &PrepareCx) {
        self.prepare_views(cx);
    }

    fn before_views(&mut self, cx: &mut EncodeCx) {
        self.cull(cx.enc, cx.timer.compute(GpuPass::Cull));
    }

    fn draw(&mut self, pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
        let (calls, triangles) = match stage {
            Stage::Opaque => self.draw_main(pass),
            Stage::Glow => (self.draw_glows(pass), 0),
            Stage::Shadow { cascade, statics, moving } => self.draw_shadow(pass, cascade, statics, moving),
            Stage::Sky | Stage::Transparent => (0, 0),
        };
        Draws { calls, triangles }
    }

    /// Hi-Z from this frame's depth, for next frame's occlusion culling.
    fn after_views(&mut self, cx: &mut EncodeCx) -> u32 {
        if self.hiz.ensure(cx.device, cx.targets) {
            self.cull.invalidate();
        }
        if cx.settings.occlusion_culling && self.gpu.is_some() {
            self.hiz.record(cx.queue, cx.enc, cx.targets.viewport(), cx.timer.compute(GpuPass::HiZ));
        } else {
            self.hiz.built = (0, 0);
        }
        self.compute_dispatches()
    }

    fn after_submit(&mut self, device: &wgpu::Device) {
        self.harvest(device);
    }

    fn gpu_bytes(&self) -> u64 {
        SceneGpu::gpu_bytes(self)
    }
}
