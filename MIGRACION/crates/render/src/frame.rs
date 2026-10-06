//! The camera as the app hands it over (f64 world), and its camera-relative matrices.
use glam::{DVec3, Mat4, Vec3};
use lunar_core::frustum::{Frustum, perspective};

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: DVec3,
    pub forward: DVec3,
    pub up: DVec3,
    pub fov_y: f32,
    pub near: f32,
}

impl View {
    /// Rotation-only view (the camera stays at the origin: camera-relative rendering).
    pub fn rotation(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(Vec3::ZERO, self.forward.as_vec3(), self.up.as_vec3())
    }
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        perspective(self.fov_y, aspect, self.near) * self.rotation()
    }
    pub fn frustum(&self, aspect: f32) -> Frustum {
        Frustum::from_matrix(self.view_proj(aspect))
    }
}

pub fn m4(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

pub fn v4(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}
