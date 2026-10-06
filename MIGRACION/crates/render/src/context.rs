//! Device, queue and swapchain; which optional GPU features this adapter gives us.
use std::{error::Error, sync::Arc};
use winit::window::Window;

#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub timestamps: bool,
    pub indirect_first_instance: bool,
    pub integrated: bool,
    pub max_layers: u32,
    pub max_anisotropy: u16,
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub caps: Caps,
    pub adapter_name: String,
    present_modes: Vec<wgpu::PresentMode>,
}

/// A window's surface and the instance it is of. Made on the thread the window is of (a window
/// hands its handle only there); the device and everything else can then be made on any thread.
pub struct Screen {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    size: (u32, u32),
}

impl Screen {
    pub fn new(window: Arc<Window>, backend: Option<wgpu::Backends>) -> Result<Screen, Box<dyn Error>> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backend.unwrap_or(wgpu::Backends::DX12 | wgpu::Backends::VULKAN);
        let instance = wgpu::Instance::new(desc);
        let size = window.inner_size();
        let surface = instance.create_surface(window)?;
        Ok(Screen { instance, surface, size: (size.width, size.height) })
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

pub const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

impl Gpu {
    pub async fn new(screen: Screen, vsync: bool) -> Result<Gpu, Box<dyn Error>> {
        let Screen { instance, surface, size } = screen;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let info = adapter.get_info();
        let adapter_name = format!("{} ({:?})", info.name, info.backend);
        let available = adapter.features();
        let wanted = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::INDIRECT_FIRST_INSTANCE;
        let features = available & wanted;
        let limits = adapter.limits();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("lunar"),
                required_features: features,
                required_limits: limits.clone(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await?;
        let caps = Caps {
            timestamps: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            indirect_first_instance: features.contains(wgpu::Features::INDIRECT_FIRST_INSTANCE),
            integrated: info.device_type == wgpu::DeviceType::IntegratedGpu,
            max_layers: limits.max_texture_array_layers,
            max_anisotropy: 16,
        };
        let surface_caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, size.0.max(1), size.1.max(1))
            .ok_or("no surface configuration")?;
        if let Some(f) = surface_caps.formats.iter().find(|f| f.is_srgb()) {
            config.format = *f;
        }
        // screenshots copy the swapchain image
        if surface_caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let mut gpu = Gpu { device, queue, surface, config, caps, adapter_name, present_modes: surface_caps.present_modes };
        gpu.set_vsync(vsync);
        Ok(gpu)
    }

    /// Fifo when synced; otherwise Immediate (nothing waits for the screen: what a frame costs is
    /// what is measured), else Mailbox.
    pub fn set_vsync(&mut self, vsync: bool) {
        use wgpu::PresentMode::*;
        let has = |m| self.present_modes.contains(&m);
        self.config.present_mode = if vsync {
            Fifo
        } else if has(Immediate) {
            Immediate
        } else if has(Mailbox) {
            Mailbox
        } else {
            Fifo
        };
        self.config.desired_maximum_frame_latency = 2;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.config.present_mode
    }
}
