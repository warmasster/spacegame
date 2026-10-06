//! egui on top of the final image (diagnostics and settings; looks are not the point).
use crate::{
    graph::{EncodeCx, RenderSystem},
    timing::GpuPass,
};

pub struct UiFrame {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures: egui::TexturesDelta,
    pub pixels_per_point: f32,
}

pub struct Overlay {
    renderer: egui_wgpu::Renderer,
}

impl Overlay {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Overlay {
        Overlay { renderer: egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions { msaa_samples: 1, depth_stencil_format: None, dithering: true, predictable_texture_filtering: false }) }
    }

    fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        ui: &UiFrame,
        timestamps: Option<wgpu::RenderPassTimestampWrites>,
    ) {
        for (id, deltas) in &ui.textures.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [size.0, size.1], pixels_per_point: ui.pixels_per_point };
        let extra = self.renderer.update_buffers(device, queue, encoder, &ui.primitives, &screen);
        if !extra.is_empty() {
            queue.submit(extra);
        }
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            ..Default::default()
        });
        let mut pass = pass.forget_lifetime();
        self.renderer.render(&mut pass, &ui.primitives, &screen);
        drop(pass);
        for id in &ui.textures.free {
            self.renderer.free_texture(id);
        }
    }
}

impl RenderSystem for Overlay {
    fn after_views(&mut self, cx: &mut EncodeCx) -> u32 {
        if let Some(ui) = cx.ui {
            self.record(cx.device, cx.queue, cx.enc, cx.out, cx.out_size, ui, cx.timer.render(GpuPass::Overlay));
        }
        0
    }
}
