//! Graphics settings: eight presets from HORRIBLE to ESPLÉNDIDOS, every knob editable in advanced mode.
//! Pure data; the renderer reads it every frame and rebuilds only what a change needs (see `Rebuild`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Horrible,
    MuyBaja,
    Baja,
    Media,
    Alta,
    MuyAlta,
    Ultra,
    Esplendidos,
}

impl Preset {
    pub const ALL: [Preset; 8] = [Preset::Horrible, Preset::MuyBaja, Preset::Baja, Preset::Media, Preset::Alta, Preset::MuyAlta, Preset::Ultra, Preset::Esplendidos];
    pub fn name(self) -> &'static str {
        match self {
            Preset::Horrible => "HORRIBLE",
            Preset::MuyBaja => "MUY BAJA",
            Preset::Baja => "BAJA",
            Preset::Media => "MEDIA",
            Preset::Alta => "ALTA",
            Preset::MuyAlta => "MUY ALTA",
            Preset::Ultra => "ULTRA",
            Preset::Esplendidos => "ESPLÉNDIDOS",
        }
    }
    pub fn parse(s: &str) -> Option<Preset> {
        let s = s.to_ascii_lowercase().replace(['-', '_', ' '], "");
        Some(match s.as_str() {
            "horrible" | "0" => Preset::Horrible,
            "muybaja" | "verylow" | "1" => Preset::MuyBaja,
            "baja" | "low" | "2" => Preset::Baja,
            "media" | "medium" | "3" => Preset::Media,
            "alta" | "high" | "4" => Preset::Alta,
            "muyalta" | "veryhigh" | "5" => Preset::MuyAlta,
            "ultra" | "6" => Preset::Ultra,
            "esplendidos" | "espléndidos" | "splendid" | "7" => Preset::Esplendidos,
            _ => return None,
        })
    }
    /// Integrated GPUs start low; dedicated ones high (docs/RENDIMIENTO.md rules).
    pub fn for_adapter(name: &str, integrated: bool) -> Preset {
        let n = name.to_ascii_lowercase();
        let weak = integrated || ["intel(r) uhd", "iris", "intel(r) graphics", "radeon(tm) graphics", "vega", "llvmpipe", "swiftshader", "microsoft basic"].iter().any(|k| n.contains(k));
        if weak { Preset::Baja } else { Preset::Alta }
    }
}

/// Every knob of the renderer. `Settings::preset(p)` fills them; advanced mode edits them one by one.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // image
    pub render_scale: f32,
    pub dynamic_resolution: bool,
    pub target_fps: f32,
    pub fxaa: bool,
    pub bloom: bool,
    pub bloom_strength: f32,
    pub exposure: f32,
    pub anisotropy: u16,
    pub vsync: bool,
    // shadows
    pub shadow_cascades: u32,
    pub shadow_resolution: u32,
    pub shadow_distance: f32,
    pub shadow_filter: u32,
    /// Filter radius factor (lower: sharper edges).
    pub shadow_softness: f32,
    /// Cascade split: 0 uniform, 1 logarithmic (more resolution near the camera).
    pub shadow_split: f32,
    /// Share of each cascade cross-faded into the next one (no visible seams).
    pub shadow_blend: f32,
    pub shadow_cache: bool,
    pub far_cascade_every: u32,
    pub baked_terrain_shadows: bool,
    // terrain
    pub terrain_grid: u32,
    pub terrain_split: f32,
    pub terrain_finest_cell: f32,
    pub terrain_gen_per_frame: u32,
    pub terrain_cache_nodes: u32,
    pub terrain_detail: u32,
    // objects
    pub lod_bias: f32,
    pub draw_distance: f32,
    pub impostors: bool,
    pub occlusion_culling: bool,
    pub shadow_lod_bias: f32,
    pub rocks_density: f32,
    pub rocks_radius: f32,
    // sky and effects
    pub stars: u32,
    pub particles: f32,
}

impl Settings {
    pub fn preset(p: Preset) -> Settings {
        // one row per preset; columns read left to right as the struct
        let i = p as usize;
        let pick = |v: [f32; 8]| v[i];
        let on = |from: usize| i >= from;
        Settings {
            render_scale: pick([0.5, 0.6, 0.75, 0.85, 1.0, 1.0, 1.25, 1.5]),
            dynamic_resolution: i < 6,
            target_fps: 60.0,
            fxaa: on(1),
            bloom: on(3),
            bloom_strength: 0.06,
            exposure: 1.0,
            anisotropy: [1, 1, 2, 4, 8, 16, 16, 16][i],
            vsync: false,
            shadow_cascades: [0, 1, 2, 3, 3, 4, 4, 4][i],
            shadow_resolution: [512, 1024, 1024, 2048, 2048, 4096, 4096, 4096][i],
            shadow_distance: pick([60., 90., 120., 200., 320., 500., 800., 1400.]),
            shadow_filter: [0, 0, 1, 1, 2, 2, 3, 3][i],
            shadow_softness: pick([1.6, 1.4, 1.2, 1.0, 0.85, 0.7, 0.55, 0.45]),
            shadow_split: pick([0.8, 0.8, 0.8, 0.85, 0.88, 0.9, 0.92, 0.94]),
            shadow_blend: 0.15,
            shadow_cache: true,
            far_cascade_every: [4, 4, 3, 2, 2, 2, 1, 1][i],
            baked_terrain_shadows: on(2),
            terrain_grid: [16, 24, 32, 48, 64, 64, 64, 96][i],
            terrain_split: pick([1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0]),
            terrain_finest_cell: pick([1.6, 1.2, 0.8, 0.6, 0.4, 0.35, 0.3, 0.2]),
            terrain_gen_per_frame: [4, 4, 6, 8, 12, 16, 24, 32][i],
            terrain_cache_nodes: [384, 448, 512, 768, 1024, 1280, 1536, 2048][i],
            terrain_detail: [0, 0, 1, 1, 2, 2, 2, 2][i],
            lod_bias: pick([0.35, 0.5, 0.6, 0.85, 1.1, 1.5, 2.2, 3.0]),
            draw_distance: pick([1500., 2500., 4000., 6000., 9000., 12000., 16000., 25000.]),
            impostors: true,
            occlusion_culling: on(2),
            shadow_lod_bias: 0.5,
            rocks_density: pick([0.0, 0.25, 0.5, 0.75, 1.0, 1.3, 1.6, 2.0]),
            rocks_radius: pick([60., 90., 120., 180., 250., 320., 400., 500.]),
            stars: [800, 1500, 3000, 5000, 8000, 12000, 16000, 24000][i],
            particles: pick([0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0]),
        }
    }

    /// Particles alive at once (the `particles` factor: 0 a few, 1 normal, 3 a storm).
    pub fn particle_budget(&self) -> usize {
        (2000.0 + self.particles.max(0.0) * 14000.0) as usize
    }

    /// What must be rebuilt when going from `self` to `next` (the rest is read live each frame).
    pub fn rebuild(&self, next: &Settings) -> Rebuild {
        Rebuild {
            terrain: self.terrain_grid != next.terrain_grid || self.baked_terrain_shadows != next.baked_terrain_shadows || self.terrain_cache_nodes != next.terrain_cache_nodes || self.terrain_finest_cell != next.terrain_finest_cell,
            shadows: self.shadow_cascades != next.shadow_cascades || self.shadow_resolution != next.shadow_resolution,
            targets: self.render_scale != next.render_scale,
            samplers: self.anisotropy != next.anisotropy,
            stars: self.stars != next.stars,
            rocks: self.rocks_density != next.rocks_density || self.rocks_radius != next.rocks_radius,
            present: self.vsync != next.vsync,
            particles: self.particle_budget() != next.particle_budget(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rebuild {
    pub terrain: bool,
    pub shadows: bool,
    pub targets: bool,
    pub samplers: bool,
    pub stars: bool,
    pub rocks: bool,
    pub present: bool,
    pub particles: bool,
}

impl Rebuild {
    pub fn any(&self) -> bool {
        self.terrain || self.shadows || self.targets || self.samplers || self.stars || self.rocks || self.present || self.particles
    }
}
