//! The GPU's side: a device that spends little, the window's surface, egui painted on it and,
//! for `--prueba`, the same frame painted on a texture of its own that is read back as a picture.
use crate::system;
use std::{error::Error, sync::Arc, time::Duration};
use winit::window::Window;

/// What egui made of a frame, ready to paint.
pub struct Frame<'a> {
    pub primitives: &'a [egui::ClippedPrimitive],
    pub textures: &'a egui::TexturesDelta,
    pub pixels_per_point: f32,
}

/// The format of the picture kept of a frame.
const PICTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Where every frame is painted as well when a picture is wanted: a texture that can be read
/// back, with an egui renderer of its own (its format need not be the window's).
struct Picture {
    texture: wgpu::Texture,
    renderer: egui_wgpu::Renderer,
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
    picture: Option<Picture>,
    /// The graphics cards the API told of, the one in use marked.
    cards: Vec<system::Gpu>,
    /// How long each part of making it took (milliseconds).
    took: Vec<(&'static str, u32)>,
}

/// The graphics APIs the launcher tries, in order: the system's own first (it is the quickest to
/// answer), the other if that one has no card that can draw on the window.
const APIS: [wgpu::Backends; 2] = [wgpu::Backends::DX12, wgpu::Backends::VULKAN];

/// How a card is told of.
fn card(info: &wgpu::AdapterInfo, used: bool) -> system::Gpu {
    let api = match info.backend {
        wgpu::Backend::Dx12 => "DirectX 12".to_string(),
        wgpu::Backend::Vulkan => "Vulkan".to_string(),
        other => format!("{other:?}"),
    };
    let kind = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => "dedicada",
        wgpu::DeviceType::IntegratedGpu => "integrada",
        wgpu::DeviceType::VirtualGpu => "virtual",
        wgpu::DeviceType::Cpu => "por software",
        wgpu::DeviceType::Other => "otra",
    };
    system::Gpu { name: info.name.trim().to_string(), api, kind: kind.to_string(), driver: info.driver_info.trim().to_string(), used }
}

/// Which card a launcher draws with, given the choice: the one that spends least (it needs no
/// more). Lower is better.
fn thrift(kind: wgpu::DeviceType) -> u8 {
    match kind {
        wgpu::DeviceType::IntegratedGpu => 0,
        wgpu::DeviceType::DiscreteGpu => 1,
        wgpu::DeviceType::Other => 2,
        wgpu::DeviceType::VirtualGpu => 3,
        wgpu::DeviceType::Cpu => 4,
    }
}

/// Initialization only: a tiny executor for wgpu's adapter and device futures.
fn wait_for<T>(future: impl Future<Output = T>) -> T {
    use std::task::{Context, Poll, Wake, Waker};
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

fn renderer(device: &wgpu::Device, format: wgpu::TextureFormat) -> egui_wgpu::Renderer {
    egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions { msaa_samples: 1, depth_stencil_format: None, dithering: true, predictable_texture_filtering: false })
}

fn picture_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("picture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: PICTURE,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// The frame's new and changed textures (the type's atlas), into a renderer.
fn upload(renderer: &mut egui_wgpu::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame) {
    for (id, deltas) in &frame.textures.set {
        for delta in deltas {
            renderer.update_texture(device, queue, *id, delta);
        }
    }
}

/// The frame painted over black on `target`.
fn paint(renderer: &mut egui_wgpu::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, size: [u32; 2], frame: &Frame) {
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: frame.pixels_per_point };
    let extra = renderer.update_buffers(device, queue, encoder, frame.primitives, &screen);
    if !extra.is_empty() {
        queue.submit(extra);
    }
    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("launcher"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
        depth_stencil_attachment: None,
        ..Default::default()
    });
    let mut pass = pass.forget_lifetime();
    renderer.render(&mut pass, frame.primitives, &screen);
}

fn release(renderer: &mut egui_wgpu::Renderer, frame: &Frame) {
    for id in &frame.textures.free {
        renderer.free_texture(id);
    }
}

impl Gpu {
    /// The device for a window: the adapter that spends least (a launcher needs no more), the
    /// surface in step with the screen. With `picture`, every frame is kept to be read back.
    pub fn new(window: Arc<Window>, picture: bool, vulkan_first: bool) -> Result<Gpu, Box<dyn Error>> {
        let size = window.inner_size();
        let mut clock = std::time::Instant::now();
        let mut took = Vec::new();
        let mut mark = |what: &'static str| {
            took.push((what, clock.elapsed().as_millis() as u32));
            clock = std::time::Instant::now();
        };
        let mut found = None;
        let apis = if vulkan_first { [APIS[1], APIS[0]] } else { APIS };
        for api in apis {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = api;
            let instance = wgpu::Instance::new(desc);
            let surface = instance.create_surface(window.clone())?;
            mark("grafica_api");
            // (the cards are asked for once: the same list picks the one to draw with and tells of them all)
            let adapters = wait_for(instance.enumerate_adapters(api));
            mark("grafica_tarjetas");
            let Some(best) = adapters.iter().enumerate().filter(|(_, a)| a.is_surface_supported(&surface)).min_by_key(|(_, a)| thrift(a.get_info().device_type)).map(|(n, _)| n) else { continue };
            let cards = adapters.iter().enumerate().map(|(n, a)| card(&a.get_info(), n == best)).collect::<Vec<_>>();
            found = Some((surface, adapters.into_iter().nth(best), cards));
            break;
        }
        let Some((surface, Some(adapter), cards)) = found else { return Err("no hay ninguna tarjeta gráfica que pueda dibujar la ventana (ni con DirectX 12 ni con Vulkan)".into()) };
        let (device, queue) = wait_for(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("launcher"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))?;
        mark("grafica_dispositivo");
        let (width, height) = (size.width.max(1), size.height.max(1));
        let mut config = surface.get_default_config(&adapter, width, height).ok_or("la tarjeta gráfica no puede dibujar en la ventana")?;
        // egui's colours are blended as they are written (not linear): a plain format if there is
        // one, which is also what the picture is kept in
        if let Some(format) = surface.get_capabilities(&adapter).formats.iter().find(|f| !f.is_srgb()) {
            config.format = *format;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        mark("grafica_superficie");
        let picture = picture.then(|| Picture { texture: picture_texture(&device, width, height), renderer: renderer(&device, PICTURE) });
        let renderer = renderer(&device, config.format);
        mark("grafica_pintor");
        Ok(Gpu { device, queue, surface, config, renderer, picture, cards, took })
    }

    /// How long each part of making it took (milliseconds).
    pub fn took(&self) -> &[(&'static str, u32)] {
        &self.took
    }

    /// The graphics cards there are, the one the launcher draws with marked.
    pub fn cards(&self) -> &[system::Gpu] {
        &self.cards
    }

    /// The side of the largest texture the device takes (egui's atlas must fit).
    pub fn max_texture_side(&self) -> usize {
        self.device.limits().max_texture_dimension_2d as usize
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width, height) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        if let Some(p) = &mut self.picture {
            p.texture = picture_texture(&self.device, width, height);
        }
    }

    /// Paint a frame on the window (when it can be painted on: not while it is hidden or being
    /// resized) and on the picture, if one is kept.
    pub fn draw(&mut self, frame: &Frame) -> Result<(), Box<dyn Error>> {
        let size = [self.config.width, self.config.height];
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("launcher") });
        if let Some(p) = &mut self.picture {
            upload(&mut p.renderer, &self.device, &self.queue, frame);
            paint(&mut p.renderer, &self.device, &self.queue, &mut encoder, &p.texture.create_view(&Default::default()), size, frame);
            release(&mut p.renderer, frame);
        }
        let target = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => return Err("la ventana no se puede dibujar (error de validación de la superficie)".into()),
        };
        upload(&mut self.renderer, &self.device, &self.queue, frame);
        if let Some(t) = &target {
            paint(&mut self.renderer, &self.device, &self.queue, &mut encoder, &t.texture.create_view(&Default::default()), size, frame);
        }
        release(&mut self.renderer, frame);
        self.queue.submit([encoder.finish()]);
        if let Some(t) = target {
            self.queue.present(t);
        }
        Ok(())
    }

    /// The last frame painted, as it was kept: its width, its height and its rows of RGB.
    pub fn picture(&self) -> Result<(u32, u32, Vec<u8>), Box<dyn Error>> {
        let texture = &self.picture.as_ref().ok_or("no se guarda imagen de los fotogramas")?.texture;
        let (width, height) = (texture.width(), texture.height());
        let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("picture"), size: u64::from(stride) * u64::from(height), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) } },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(10)) })?;
        rx.recv_timeout(Duration::from_secs(10))??;
        let data = buffer.slice(..).get_mapped_range()?;
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for row in data.chunks(stride as usize) {
            for pixel in row[..width as usize * 4].chunks(4) {
                rgb.extend_from_slice(&pixel[..3]);
            }
        }
        drop(data);
        buffer.unmap();
        Ok((width, height, rgb))
    }
}
