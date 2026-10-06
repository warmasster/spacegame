//! The frame as a sequence of stages run over the registered render systems (terrain, scene, sky,
//! post, overlay...). A system declares the shared stages it draws in and may record passes of its
//! own before the views (compute) or after them (Hi-Z, post, overlay); the graph opens the shared
//! passes — every shadow cascade, then the main view — and calls each system in registration order.
use crate::{
    frame::View,
    globals::{Globals, PASS_MAIN},
    overlay::UiFrame,
    shadow::{Action, Cascades},
    targets::Targets,
    timing::{GpuPass, GpuTimer},
};
use glam::{DVec3, Mat4};
use lunar_core::{frustum::Frustum, quality::Settings};

/// Draw calls and triangles a system submitted.
#[derive(Clone, Copy, Debug, Default)]
pub struct Draws {
    pub calls: u32,
    pub triangles: u64,
}

impl std::ops::AddAssign for Draws {
    fn add_assign(&mut self, o: Draws) {
        self.calls += o.calls;
        self.triangles += o.triangles;
    }
}

/// A shared pass a system can draw into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Depth into cascade `cascade`: its static casters, its moving ones, or both.
    Shadow { cascade: usize, statics: bool, moving: bool },
    /// Main view, in this order.
    Opaque,
    Sky,
    Glow,
    /// Its own pass after the main one: blended, with the scene depth read-only (and bindable).
    Transparent,
}

/// Bits for `RenderSystem::stages`.
pub const SHADOW: u8 = 1;
pub const OPAQUE: u8 = 2;
pub const SKY: u8 = 4;
pub const GLOW: u8 = 8;
pub const TRANSPARENT: u8 = 16;

impl Stage {
    fn bit(self) -> u8 {
        match self {
            Stage::Shadow { .. } => SHADOW,
            Stage::Opaque => OPAQUE,
            Stage::Sky => SKY,
            Stage::Glow => GLOW,
            Stage::Transparent => TRANSPARENT,
        }
    }
}

/// CPU preparation inputs (after the cascades are placed).
pub struct PrepareCx<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a View,
    pub prev_vp: Mat4,
    pub prev_eye: DVec3,
    pub main: &'a Frustum,
    pub cascades: &'a Cascades,
    pub settings: &'a Settings,
    pub viewport: (u32, u32),
    pub sun: DVec3,
}

/// Recording inputs.
pub struct EncodeCx<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub enc: &'a mut wgpu::CommandEncoder,
    pub timer: &'a mut GpuTimer,
    pub targets: &'a Targets,
    pub settings: &'a Settings,
    pub out: &'a wgpu::TextureView,
    pub out_size: (u32, u32),
    pub ui: Option<&'a UiFrame>,
}

pub trait RenderSystem {
    /// The shared stages this system draws in (bits above).
    fn stages(&self) -> u8 {
        0
    }
    fn prepare(&mut self, _cx: &PrepareCx) {}
    /// Own passes before the views (compute).
    fn before_views(&mut self, _cx: &mut EncodeCx) {}
    fn draw(&mut self, _pass: &mut wgpu::RenderPass, _stage: Stage) -> Draws {
        Draws::default()
    }
    /// Own passes after the main view; returns draw calls.
    fn after_views(&mut self, _cx: &mut EncodeCx) -> u32 {
        0
    }
    fn after_submit(&mut self, _device: &wgpu::Device) {}
    /// Something static changed this frame: the cached shadow maps must be redrawn.
    fn statics_changed(&self) -> bool {
        false
    }
    fn gpu_bytes(&self) -> u64 {
        0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameDraws {
    pub main: u32,
    pub shadow: u32,
    pub after: u32,
    pub triangles: u64,
}

fn draw_stage(systems: &mut [&mut dyn RenderSystem], pass: &mut wgpu::RenderPass, stage: Stage) -> Draws {
    let mut d = Draws::default();
    for s in systems.iter_mut().filter(|s| s.stages() & stage.bit() != 0) {
        d += s.draw(pass, stage);
    }
    d
}

/// Record the whole frame.
pub fn encode(systems: &mut [&mut dyn RenderSystem], cx: &mut EncodeCx, cascades: &Cascades, globals: &Globals) -> FrameDraws {
    let mut out = FrameDraws::default();
    for s in systems.iter_mut() {
        s.before_views(cx);
    }
    // shadow cascades: (cascade, into the static cache, clear, statics, moving) per pass
    let nc = cascades.count;
    let cached = cascades.cached();
    let mut steps = [(0usize, false, false, false, false); 2 * crate::shadow::MAX_CASCADES];
    let mut n = 0;
    for c in 0..nc {
        let mut push = |s| {
            steps[n] = s;
            n += 1;
        };
        match cascades.actions[c] {
            Action::Keep => {}
            Action::Full if cached => {
                push((c, true, true, true, false));
                push((c, false, false, false, true));
            }
            Action::Full => push((c, false, true, true, true)),
            Action::Dynamic => push((c, false, false, false, true)),
        }
    }
    let last = n.saturating_sub(1);
    for (k, (c, to_static, clear, statics, moving)) in steps[..n].iter().copied().enumerate() {
        if cached && !to_static {
            cascades.restore(cx.enc, c);
        }
        let ts = if k == 0 || k == last { cx.timer.pass(GpuPass::Shadows) } else { None }.map(|(q, i)| wgpu::RenderPassTimestampWrites {
            query_set: q,
            beginning_of_pass_write_index: (k == 0).then_some(i),
            end_of_pass_write_index: (k == last).then_some(i + 1),
        });
        let target = if to_static { &cascades.static_layers[c] } else { &cascades.layers[c] };
        let mut pass = cx.enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: target,
                depth_ops: Some(wgpu::Operations { load: if clear { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: ts,
            ..Default::default()
        });
        pass.set_bind_group(0, &globals.shadow, &[Globals::offset(1 + c as u32)]);
        let d = draw_stage(systems, &mut pass, Stage::Shadow { cascade: c, statics, moving });
        out.shadow += d.calls;
        out.triangles += d.triangles;
    }
    // main view
    {
        let (vw, vh) = cx.targets.viewport();
        let mut pass = cx.enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &cx.targets.hdr_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &cx.targets.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: cx.timer.render(GpuPass::Main),
            ..Default::default()
        });
        pass.set_viewport(0.0, 0.0, vw as f32, vh as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, vw, vh);
        pass.set_bind_group(0, &globals.main, &[Globals::offset(PASS_MAIN)]);
        for stage in [Stage::Opaque, Stage::Sky, Stage::Glow] {
            let d = draw_stage(systems, &mut pass, stage);
            out.main += d.calls;
            out.triangles += d.triangles;
        }
    }
    // blended things over the finished scene, depth tested but not written
    if systems.iter().any(|s| s.stages() & TRANSPARENT != 0) {
        let (vw, vh) = cx.targets.viewport();
        let mut pass = cx.enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("transparent"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &cx.targets.hdr_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &cx.targets.depth_view, depth_ops: None, stencil_ops: None }),
            ..Default::default()
        });
        pass.set_viewport(0.0, 0.0, vw as f32, vh as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, vw, vh);
        pass.set_bind_group(0, &globals.main, &[Globals::offset(PASS_MAIN)]);
        let d = draw_stage(systems, &mut pass, Stage::Transparent);
        out.main += d.calls;
        out.triangles += d.triangles;
    }
    for s in systems.iter_mut() {
        out.after += s.after_views(cx);
    }
    out
}
