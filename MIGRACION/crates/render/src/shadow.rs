//! Cascaded sun shadows. Cascades are spheres round the camera (selected by distance), snapped in
//! world light space: their placement only changes when the camera crosses a snap step, so the
//! static casters' depth is cached per cascade and only the moving casters are redrawn on top.
//! Far cascades refresh every few frames.
use crate::context::DEPTH;
use glam::{DMat3, DVec3, Mat4, Vec3};
use lunar_core::frustum::Frustum;

pub const MAX_CASCADES: usize = 4;
/// How far toward the sun (m) casters are taken, past the cascade's own sphere: far enough for a
/// crater rim's shadow at a low sun (the baked far horizon takes the ground past the cascades).
pub const REACH: f64 = 4000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Keep last frame's map.
    Keep,
    /// Redraw the moving casters over the cached static ones.
    Dynamic,
    /// Redraw everything (placement moved or the cache is off).
    Full,
}

pub struct Cascades {
    pub count: usize,
    pub res: u32,
    pub view: wgpu::TextureView,
    texture: wgpu::Texture,
    statics: Option<wgpu::Texture>,
    pub layers: Vec<wgpu::TextureView>,
    pub static_layers: Vec<wgpu::TextureView>,
    /// World placement (snapped centre) of each cascade and its half size (m).
    centers: [DVec3; MAX_CASCADES],
    pub half: [f64; MAX_CASCADES],
    pub far: [f32; MAX_CASCADES],
    static_valid: [bool; MAX_CASCADES],
    pub mats: [Mat4; MAX_CASCADES],
    pub frusta: [Frustum; MAX_CASCADES],
    pub actions: [Action; MAX_CASCADES],
    basis: DMat3,
}

impl Cascades {
    pub fn new(device: &wgpu::Device, count: usize, res: u32, cache: bool) -> Cascades {
        let count = count.min(MAX_CASCADES);
        let layers = count.max(1) as u32;
        let make = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: res, height: res, depth_or_array_layers: layers },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let texture = make("shadow cascades");
        let statics = (cache && count > 0).then(|| make("shadow static cache"));
        let layer_views = |t: &wgpu::Texture| {
            (0..count as u32)
                .map(|l| {
                    t.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: l,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>()
        };
        Cascades {
            count,
            res,
            view: texture.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() }),
            layers: layer_views(&texture),
            static_layers: statics.as_ref().map(layer_views).unwrap_or_default(),
            texture,
            statics,
            centers: [DVec3::splat(f64::NAN); MAX_CASCADES],
            half: [0.0; MAX_CASCADES],
            far: [0.0; MAX_CASCADES],
            static_valid: [false; MAX_CASCADES],
            mats: [Mat4::IDENTITY; MAX_CASCADES],
            frusta: [Frustum { planes: [glam::Vec4::W; 6] }; MAX_CASCADES],
            actions: [Action::Keep; MAX_CASCADES],
            basis: DMat3::IDENTITY,
        }
    }

    pub fn bytes(&self) -> u64 {
        let one = u64::from(self.res) * u64::from(self.res) * 4 * self.count.max(1) as u64;
        one * if self.statics.is_some() { 2 } else { 1 }
    }

    pub fn cached(&self) -> bool {
        self.statics.is_some()
    }

    /// Something static changed (terrain arrived, rocks moved): redraw the static layers.
    pub fn invalidate(&mut self) {
        self.static_valid = [false; MAX_CASCADES];
    }

    /// Place the cascades for this frame; decide what each one redraws.
    /// `split`: 0 uniform, 1 logarithmic cascade distances.
    pub fn update(&mut self, eye: DVec3, sun: DVec3, distance: f64, split: f64, far_every: u64, frame: u64) {
        let n = self.count;
        if n == 0 {
            return;
        }
        // light basis: z toward the sun
        let z = sun.normalize();
        let x = z.cross(DVec3::Y).try_normalize().unwrap_or(DVec3::X);
        let y = x.cross(z);
        self.basis = DMat3::from_cols(x, y, z);
        let near = 1.0f64;
        for c in 0..n {
            let t = (c + 1) as f64 / n as f64;
            let log = near * (distance / near).powf(t);
            let uni = near + (distance - near) * t;
            let far = split * log + (1.0 - split) * uni;
            self.far[c] = far as f32;
            let texel = far * 2.3 / f64::from(self.res);
            let step = ((far * 0.1) / texel).round().max(1.0) * texel;
            let half = far * 1.15 + step;
            let lx = (eye.dot(x) / step).round() * step;
            let ly = (eye.dot(y) / step).round() * step;
            let center = x * lx + y * ly + z * eye.dot(z);
            let moved = center.distance(self.centers[c]) > step * 0.5 || half != self.half[c];
            let due = c < 2 || (frame + c as u64) % far_every.max(1) == 0;
            self.actions[c] = if moved || !self.cached() || !self.static_valid[c] {
                Action::Full
            } else if due {
                Action::Dynamic
            } else {
                Action::Keep
            };
            if self.actions[c] == Action::Full {
                self.centers[c] = center;
                self.half[c] = half;
                self.static_valid[c] = self.cached();
            }
        }
    }

    /// Camera-relative matrices (recomputed every frame from the fixed world placement).
    pub fn matrices(&mut self, eye: DVec3) {
        for c in 0..self.count {
            let rel = (self.centers[c] - eye).as_vec3();
            let (x, y, z) = (self.basis.x_axis.as_vec3(), self.basis.y_axis.as_vec3(), self.basis.z_axis.as_vec3());
            let h = self.half[c] as f32;
            let view = Mat4::from_cols(x.extend(0.), y.extend(0.), z.extend(0.), Vec3::ZERO.extend(1.)).transpose()
                * Mat4::from_translation(-rel);
            let depth = h + REACH as f32;
            // looking down -z (away from the sun): near plane toward the sun
            let proj = glam::camera::rh::proj::directx::orthographic(-h, h, -h, h, -depth, h);
            self.mats[c] = proj * view;
            self.frusta[c] = Frustum::from_matrix(self.mats[c]);
        }
    }

    /// Copy the cached static depth of cascade `c` into the live map.
    pub fn restore(&self, encoder: &mut wgpu::CommandEncoder, c: usize) {
        if let Some(s) = &self.statics {
            let origin = wgpu::Origin3d { x: 0, y: 0, z: c as u32 };
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo { texture: s, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
                wgpu::Extent3d { width: self.res, height: self.res, depth_or_array_layers: 1 },
            );
        }
    }
}
