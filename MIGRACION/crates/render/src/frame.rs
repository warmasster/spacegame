//! The camera as the app hands it over (`lunar_core::view::View`), and the matrices of it.
use glam::{Mat4, Vec3};

pub use lunar_core::view::View;

pub fn m4(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

pub fn v4(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}
