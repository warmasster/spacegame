//! Render targets at the settings' scale; dynamic resolution only moves the viewport inside them
//! (no reallocation, no black frame).
use crate::context::{DEPTH, HDR};

pub struct Targets {
    /// Unique per creation: bindings made with these views know when to be remade.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub hdr: wgpu::Texture,
    pub hdr_view: wgpu::TextureView,
    pub depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    /// Current dynamic fraction of the target that is rendered (0.4..1).
    pub dynamic: f32,
}

impl Targets {
    pub fn new(device: &wgpu::Device, window: (u32, u32), scale: f32) -> Targets {
        let width = ((window.0 as f32 * scale) as u32).max(8);
        let height = ((window.1 as f32 * scale) as u32).max(8);
        let tex = |label, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let hdr = tex("hdr", HDR, rt | wgpu::TextureUsages::COPY_SRC);
        let depth = tex("depth", DEPTH, rt);
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Targets {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            width,
            height,
            hdr_view: hdr.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            hdr,
            depth,
            dynamic: 1.0,
        }
    }

    /// The rendered area (pixels) this frame.
    pub fn viewport(&self) -> (u32, u32) {
        (((self.width as f32 * self.dynamic) as u32).max(8), ((self.height as f32 * self.dynamic) as u32).max(8))
    }

    pub fn bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * (8 + 4)
    }
}
