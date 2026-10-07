//! A view of the world: where an eye is and the way it looks (f64 world), and its camera-relative
//! matrices. What the game decides from where someone looks (what they aim at, what a test key
//! fires along) and what the renderer draws from are the same view.
use crate::frustum::{Frustum, perspective};
use glam::{DVec3, Mat4, Vec3};

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
