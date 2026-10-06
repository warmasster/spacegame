//! The frame uniform: camera, cascades, the sun and backdrop of the star system, and the body
//! under the camera (its vertical and the altitude over it).
use crate::{
    frame::{View, m4, v4},
    globals::{FrameU, MAX_LIGHTS},
    shadow::Cascades,
    targets::Targets,
};
use glam::{DVec3, Mat4};
use lunar_core::{body::Body, quality::Settings, system::{Backdrop, Sun}};

pub struct FrameInputs<'a> {
    pub view: &'a View,
    pub vp: Mat4,
    pub prev_vp: Mat4,
    pub prev_eye: DVec3,
    /// Where the scene's instances are stored relative to.
    pub origin: DVec3,
    pub time: f64,
    pub frame: u64,
    pub cascades: &'a Cascades,
    pub targets: &'a Targets,
    pub settings: &'a Settings,
    pub sun: &'a Sun,
    pub backdrop: Option<&'a Backdrop>,
    /// The body under the camera.
    pub body: &'a Body,
    /// Flashes lighting the scene.
    pub lights: &'a [Light],
}

/// A light: world position, colour x intensity, range (m); a spot when `cone` > 0 (the cosine
/// of its half-angle) along `dir`; `inside` lamps (in a structure) do not light the ground.
#[derive(Clone, Copy, Debug, Default)]
pub struct Light {
    pub pos: DVec3,
    pub color: [f32; 3],
    pub range: f32,
    pub dir: [f32; 3],
    pub cone: f32,
    pub inside: bool,
    /// Carried about (a helmet lamp): it lights what is inside a hull and what is outside alike.
    pub everywhere: bool,
}

pub fn frame_uniforms(i: &FrameInputs) -> FrameU {
    let (view, s, c) = (i.view, i.settings, i.cascades);
    let eye = view.eye;
    let (vw, vh) = i.targets.viewport();
    let t = i.targets;
    let up = i.body.up(eye);
    let alt = i.body.datum_altitude(eye);
    let nc = c.count;
    let mut cascades = [[[0f32; 4]; 4]; 4];
    let mut far = [0f32; 4];
    for k in 0..nc {
        cascades[k] = m4(c.mats[k]);
        far[k] = c.far[k];
    }
    let texel0 = if nc > 0 { (2.0 * c.half[0] / f64::from(c.res)) as f32 } else { 0.1 };
    let mut lights = [[0f32; 4]; 3 * MAX_LIGHTS];
    let nl = i.lights.len().min(MAX_LIGHTS);
    for (k, l) in i.lights[..nl].iter().enumerate() {
        lights[k * 3] = v4((l.pos - eye).as_vec3(), l.range);
        lights[k * 3 + 1] = [l.color[0], l.color[1], l.color[2], if l.everywhere { 2.0 } else if l.inside { 1.0 } else { 0.0 }];
        lights[k * 3 + 2] = [l.dir[0], l.dir[1], l.dir[2], l.cone];
    }
    let backdrop = i.backdrop.map_or([0.0, 1.0, 0.0, 0.0], |b| v4(b.direction().as_vec3(), (b.angular_radius as f32).to_radians()));
    FrameU {
        view_proj: m4(i.vp),
        inv_view_proj: m4(i.vp.inverse()),
        prev_view_proj: m4(i.prev_vp),
        cascades,
        cascade_far: far,
        cam_forward: v4(view.forward.as_vec3(), (view.fov_y * 0.5).tan()),
        origin: v4((i.origin - eye).as_vec3(), (i.time % 100_000.0) as f32),
        prev_origin: v4((i.origin - i.prev_eye).as_vec3(), 0.0),
        sun_dir: v4(i.sun.direction().as_vec3(), i.sun.intensity),
        sun_color: [i.sun.color[0], i.sun.color[1], i.sun.color[2], i.sun.shadow_sweep],
        backdrop,
        up: v4(up.as_vec3(), alt as f32),
        body_center: v4((i.body.center - eye).as_vec3(), i.body.radius as f32),
        viewport: [vw as f32, vh as f32, 1.0 / vw as f32, 1.0 / vh as f32],
        target: [t.width as f32, t.height as f32, vw as f32 / t.width as f32, vh as f32 / t.height as f32],
        params: [s.exposure, s.shadow_filter as f32, nc as f32, texel0],
        params2: [i.frame as f32, s.lod_bias, if s.bloom { 1.0 } else { 0.0 }, s.terrain_detail as f32],
        shadow: [s.shadow_softness, s.shadow_blend, nl as f32, 0.0],
        lights,
    }
}
