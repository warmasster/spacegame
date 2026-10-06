//! The renderer: settings → resources, and the frame. The frame is not written by hand: the render
//! systems (terrain, scene, sky, particles, post, overlay) are registered in order in `systems!` and the graph
//! (graph.rs) runs their stages — compute, shadow cascades, main view, after-view passes.
mod feeds;

use crate::{
    context::Gpu,
    detail,
    frame::View,
    globals::{Globals, MAX_LIGHTS, PASS_MAIN, PassU},
    governor::Governor,
    graph::{self, EncodeCx, PrepareCx, RenderSystem},
    overlay::{Overlay, UiFrame},
    particles::ParticlesGpu,
    plumes::PlumesGpu,
    post::Post,
    props::PropsGpu,
    scene::{SceneGpu, SceneStats},
    shadow::Cascades,
    sky::Sky,
    structures::StructuresGpu,
    targets::Targets,
    terrain::{TerrainConfig, TerrainSet, TerrainStats},
    timing::{GpuTimer, PASSES},
    uniforms::{FrameInputs, Light, frame_uniforms},
};
use glam::{DVec3, Mat4};
use lunar_core::{
    body::BodyRegistry,
    frustum::Frustum,
    quality::Settings,
    system::{Backdrop, Sun, System},
};
use std::{error::Error, path::Path, sync::Arc, time::Instant};

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub cpu_prepare_ms: f32,
    pub cpu_terrain_ms: f32,
    pub cpu_scene_ms: f32,
    pub cpu_encode_ms: f32,
    pub cpu_submit_ms: f32,
    pub gpu_ms: [f32; PASSES],
    pub gpu_timed: bool,
    pub draws: u32,
    pub draws_main: u32,
    pub draws_shadow: u32,
    pub triangles: u64,
    pub terrain: TerrainStats,
    pub scene: SceneStats,
    pub dynamic_res: f32,
    pub render_size: (u32, u32),
    pub gpu_bytes: u64,
    pub shadow_actions: [u8; 4],
    /// Name of the body under the camera.
    pub body: String,
}

pub struct FrameInput<'a> {
    pub view: View,
    pub time: f64,
    pub ui: Option<&'a UiFrame>,
    pub capture: Option<&'a Path>,
}

/// The registered render systems, in frame order.
macro_rules! systems {
    ($r:expr) => {
        [&mut $r.terrain as &mut dyn RenderSystem, &mut $r.scene, &mut $r.structures, &mut $r.figures, &mut $r.props, &mut $r.sky, &mut $r.prints, &mut $r.plumes, &mut $r.particles, &mut $r.post, &mut $r.overlay]
    };
}

pub struct Renderer {
    pub gpu: Gpu,
    globals: Globals,
    targets: Targets,
    cascades: Cascades,
    terrain: TerrainSet,
    scene: SceneGpu,
    structures: StructuresGpu,
    props: PropsGpu,
    /// Bodies posed bone by bone each frame (`bodies::BodiesGpu`).
    figures: crate::bodies::BodiesGpu,
    sky: Sky,
    particles: ParticlesGpu,
    /// Exhaust plumes: drawn before the particles (dust in front of a jet dims it).
    plumes: PlumesGpu,
    /// Marks on loose ground (`prints`): over the terrain, under the plumes and the dust.
    pub(crate) prints: crate::prints::PrintsGpu,
    pub(crate) post: Post,
    overlay: Overlay,
    timer: GpuTimer,
    detail: wgpu::TextureView,
    pub settings: Settings,
    frame: u64,
    prev_vp: Mat4,
    prev_eye: DVec3,
    pub stats: Stats,
    governor: Governor,
    sun: Sun,
    backdrop: Option<Backdrop>,
    bodies: Arc<BodyRegistry>,
    lights: [Light; MAX_LIGHTS],
    light_count: usize,
    /// How many of `lights` are flashes (the rest are lamps).
    flash_count: usize,
    /// Sun (azimuth, elevation) the terrain's shadows were baked with.
    baked_sun: (f32, f32),
}

fn terrain_config(s: &Settings) -> TerrainConfig {
    TerrainConfig { grid: s.terrain_grid, capacity: s.terrain_cache_nodes, finest_cell: f64::from(s.terrain_finest_cell), baked_shadows: s.baked_terrain_shadows }
}

impl Renderer {
    /// `choose` picks the settings once the adapter is known (name, integrated).
    /// The renderer for a window's `screen` (made on the window's thread; this can run on any).
    pub fn new(screen: crate::context::Screen, system: &System, choose: impl FnOnce(&str, bool) -> Settings) -> Result<Renderer, Box<dyn Error>> {
        let size = screen.size();
        let mut gpu = crate::wait_for(Gpu::new(screen, true))?;
        let mut settings = choose(&gpu.adapter_name, gpu.caps.integrated);
        gpu.set_vsync(settings.vsync);
        settings.terrain_cache_nodes = settings.terrain_cache_nodes.min(gpu.caps.max_layers);
        let device = &gpu.device;
        let sun = system.sun;
        let cascades = Cascades::new(device, settings.shadow_cascades as usize, settings.shadow_resolution, settings.shadow_cache);
        let globals = Globals::new(device, &cascades.view, settings.anisotropy);
        let detail = detail::texture(device, &gpu.queue);
        let terrain = TerrainSet::new(device, &gpu.queue, &globals.layout, &detail, terrain_config(&settings), sun.direction(), system.bodies.clone());
        let scene = SceneGpu::new(device, &gpu.queue, &globals.layout, gpu.caps);
        let sky = Sky::new(device, &globals.layout, settings.stars);
        let structures = StructuresGpu::new(device, &gpu.queue, &globals.layout);
        let props = PropsGpu::new(device, &gpu.queue, &globals.layout);
        let figures = crate::bodies::BodiesGpu::new(device, &globals.layout);
        let particles = ParticlesGpu::new(device, &globals.layout, settings.particle_budget());
        let plumes = PlumesGpu::new(device, &globals.layout);
        let prints = crate::prints::PrintsGpu::new(device, &globals.layout);
        let post = Post::new(device, gpu.config.format);
        let overlay = Overlay::new(device, gpu.config.format);
        let timer = GpuTimer::new(device, &gpu.queue, gpu.caps.timestamps);
        let targets = Targets::new(device, (size.0.max(1), size.1.max(1)), settings.render_scale);
        Ok(Renderer {
            gpu,
            globals,
            targets,
            cascades,
            terrain,
            scene,
            structures,
            props,
            figures,
            sky,
            particles,
            plumes,
            prints,
            post,
            overlay,
            timer,
            detail,
            settings,
            frame: 0,
            prev_vp: Mat4::IDENTITY,
            prev_eye: DVec3::ZERO,
            stats: Stats { dynamic_res: 1.0, ..Default::default() },
            governor: Governor::new(),
            baked_sun: (sun.azimuth, sun.elevation),
            sun,
            backdrop: system.backdrop,
            bodies: system.bodies.clone(),
            lights: [Light::default(); MAX_LIGHTS],
            light_count: 0,
            flash_count: 0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        let dynamic = self.targets.dynamic;
        self.targets = Targets::new(&self.gpu.device, (width, height), self.settings.render_scale);
        self.targets.dynamic = dynamic;
    }

    pub fn sun(&self) -> Sun {
        self.sun
    }

    /// Baked terrain shadows lag behind the sun (it moved while dragging).
    pub fn bake_pending(&self) -> bool {
        self.settings.baked_terrain_shadows && self.baked_sun != (self.sun.azimuth, self.sun.elevation)
    }

    fn rebuild_terrain(&mut self) {
        self.baked_sun = (self.sun.azimuth, self.sun.elevation);
        self.terrain.rebuild(&self.gpu.device, &self.gpu.queue, &self.globals.layout, &self.detail, terrain_config(&self.settings), self.sun.direction());
        self.cascades.invalidate();
    }

    /// Move the sun. `live`: mid-drag, the baked terrain shadows wait for the release.
    pub fn set_sun(&mut self, sun: Sun, live: bool) {
        if sun.direction() != self.sun.direction() {
            self.cascades.invalidate();
        }
        self.sun = sun;
        if !live && self.bake_pending() {
            self.rebuild_terrain();
        }
    }

    /// Apply new settings, rebuilding only what they touch.
    pub fn apply(&mut self, mut next: Settings) {
        next.terrain_cache_nodes = next.terrain_cache_nodes.min(self.gpu.caps.max_layers);
        let r = self.settings.rebuild(&next);
        let shadows = r.shadows || self.settings.shadow_cache != next.shadow_cache;
        let device = &self.gpu.device;
        if shadows {
            self.cascades = Cascades::new(device, next.shadow_cascades as usize, next.shadow_resolution, next.shadow_cache);
            self.globals.rebind(device, &self.cascades.view);
        }
        if r.samplers {
            self.globals.set_anisotropy(device, next.anisotropy, &self.cascades.view);
        }
        if r.stars {
            self.sky.set_count(device, next.stars);
        }
        if r.particles {
            self.particles.resize(device, next.particle_budget());
        }
        if r.present {
            self.gpu.set_vsync(next.vsync);
        }
        self.settings = next;
        if r.terrain {
            self.rebuild_terrain();
        }
        if r.targets {
            let (w, h) = (self.gpu.config.width, self.gpu.config.height);
            self.resize(w, h);
        }
    }

    pub fn adapter(&self) -> &str {
        &self.gpu.adapter_name
    }

    pub fn render(&mut self, input: &FrameInput) -> Result<bool, Box<dyn Error>> {
        let t0 = Instant::now();
        self.frame += 1;
        let gpu_ms = self.timer.enabled().then(|| self.timer.total());
        self.targets.dynamic = self.governor.update(self.settings.dynamic_resolution, self.settings.target_fps, gpu_ms, self.frame, self.targets.dynamic);
        let s = &self.settings;
        let view = input.view;
        let (vw, vh) = self.targets.viewport();
        let vp = view.view_proj(vw as f32 / vh as f32);
        let main_frustum = Frustum::from_matrix(vp);
        let sun = self.sun.direction();
        // shadows: placement and what each cascade redraws
        self.cascades.update(view.eye, sun, f64::from(s.shadow_distance), f64::from(s.shadow_split), u64::from(s.far_cascade_every), self.frame);
        self.cascades.matrices(view.eye);
        let t1 = Instant::now();
        let mut systems = systems!(self);
        let pcx = PrepareCx {
            device: &self.gpu.device,
            queue: &self.gpu.queue,
            view: &view,
            prev_vp: self.prev_vp,
            prev_eye: self.prev_eye,
            main: &main_frustum,
            cascades: &self.cascades,
            settings: s,
            viewport: (vw, vh),
            sun,
        };
        let mut split = [t1; 2];
        for (k, sys) in systems.iter_mut().enumerate() {
            sys.prepare(&pcx);
            if k < 2 {
                split[k] = Instant::now();
            }
        }
        // new ground can change static shadows
        if systems.iter().any(|sys| sys.statics_changed()) {
            self.cascades.invalidate();
        }
        let (t2, t3) = (split[0], split[1]);
        // frame uniforms: the body under the camera gives the vertical
        let body_id = self.bodies.dominant(view.eye);
        let fu = frame_uniforms(&FrameInputs {
            view: &view,
            vp,
            prev_vp: self.prev_vp,
            prev_eye: self.prev_eye,
            origin: self.scene.origin(),
            time: input.time,
            frame: self.frame,
            cascades: &self.cascades,
            targets: &self.targets,
            settings: s,
            sun: &self.sun,
            backdrop: self.backdrop.as_ref(),
            body: self.bodies.get(body_id),
            lights: &self.lights[..self.light_count],
        });
        let nc = self.cascades.count;
        self.globals.set_pass(PASS_MAIN, &PassU { view_proj: crate::frame::m4(vp), info: [0; 4], light: [0.0; 4] });
        for c in 0..nc {
            self.globals.set_pass(1 + c as u32, &PassU { view_proj: crate::frame::m4(self.cascades.mats[c]), info: [c as u32 + 1, 0, 0, 0], light: [0.0; 4] });
        }
        self.globals.upload(&self.gpu.queue, &fu);
        let t4 = Instant::now();

        // ---- encode ----
        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(false),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let (w, h) = (self.gpu.config.width, self.gpu.config.height);
                self.resize(w, h);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => return Err("surface validation error".into()),
        };
        let out = frame.texture.create_view(&Default::default());
        let out_size = (self.gpu.config.width, self.gpu.config.height);
        let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let mut systems = systems!(self);
        let draws = {
            let mut cx = EncodeCx {
                device: &self.gpu.device,
                queue: &self.gpu.queue,
                enc: &mut enc,
                timer: &mut self.timer,
                targets: &self.targets,
                settings: &self.settings,
                out: &out,
                out_size,
                ui: input.ui,
            };
            graph::encode(&mut systems, &mut cx, &self.cascades, &self.globals)
        };
        self.timer.resolve(&mut enc);
        let t5 = Instant::now();
        let (device, queue) = (&self.gpu.device, &self.gpu.queue);
        queue.submit([enc.finish()]);
        if let Some(path) = input.capture {
            let bgra = matches!(self.gpu.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            let rgb = crate::capture::read_texture(device, queue, &frame.texture, out_size.0, out_size.1, bgra)?;
            crate::capture::write_png(path, out_size.0, out_size.1, &rgb)?;
        }
        queue.present(frame);
        self.timer.after_submit(device);
        for sys in systems.iter_mut() {
            sys.after_submit(device);
        }
        let t6 = Instant::now();

        self.prev_vp = vp;
        self.prev_eye = view.eye;
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f32() * 1000.0;
        let st = &mut self.stats;
        st.cpu_prepare_ms = ms(t0, t1) + ms(t3, t4);
        st.cpu_terrain_ms = ms(t1, t2);
        st.cpu_scene_ms = ms(t2, t3);
        st.cpu_encode_ms = ms(t4, t5);
        st.cpu_submit_ms = ms(t5, t6);
        st.gpu_ms = self.timer.ms;
        st.gpu_timed = self.timer.enabled();
        st.draws_main = draws.main;
        st.draws_shadow = draws.shadow;
        st.draws = draws.main + draws.shadow + draws.after;
        st.triangles = draws.triangles;
        st.terrain = self.terrain.stats;
        st.scene.clone_from(&self.scene.stats);
        st.dynamic_res = self.targets.dynamic;
        st.render_size = (vw, vh);
        st.gpu_bytes = self.cascades.bytes() + self.targets.bytes() + systems!(self).iter().map(|sys| sys.gpu_bytes()).sum::<u64>();
        st.shadow_actions = std::array::from_fn(|c| if c < nc { self.cascades.actions[c] as u8 } else { 255 });
        let body = self.bodies.get(body_id);
        if st.body != body.name {
            st.body.clone_from(&body.name);
        }
        Ok(true)
    }
}
