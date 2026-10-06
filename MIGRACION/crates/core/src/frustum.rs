//! View frusta for camera-relative, reversed-Z, infinite-far projections, and the swept-sphere test
//! the shadow cascades use (a caster matters only where its shadow can land).
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    /// Left, right, bottom, top, near (the far plane is at infinity); plus two optional planes.
    pub planes: [Vec4; 6],
}

impl Frustum {
    /// From a clip matrix with z in [0, w] (reversed: near maps to w).
    pub fn from_matrix(m: Mat4) -> Frustum {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let n = |p: Vec4| {
            let l = p.truncate().length();
            if l < 1e-9 { Vec4::W } else { p / l }
        };
        Frustum { planes: [n(r3 + r0), n(r3 - r0), n(r3 + r1), n(r3 - r1), n(r3 - r2), n(r2)] }
    }
    /// A frustum slice between two view distances along `forward` (camera at the origin).
    pub fn slice(mut self, forward: Vec3, near: f32, far: f32) -> Frustum {
        self.planes[4] = forward.extend(-near);
        self.planes[5] = (-forward).extend(far);
        self
    }
    pub fn sphere(&self, c: Vec3, r: f32) -> bool {
        self.planes.iter().all(|p| p.truncate().dot(c) + p.w >= -r)
    }
    /// The sphere swept `len` along `dir` (a capsule) touches the frustum.
    pub fn capsule(&self, c: Vec3, r: f32, dir: Vec3, len: f32) -> bool {
        let e = c + dir * len;
        self.planes.iter().all(|p| {
            let n = p.truncate();
            (n.dot(c) + p.w).max(n.dot(e) + p.w) >= -r
        })
    }
}

/// Reversed-Z, infinite-far perspective (right-handed view space).
pub fn perspective(fov_y: f32, aspect: f32, near: f32) -> Mat4 {
    glam::camera::rh::proj::directx::perspective_infinite_reverse(fov_y, aspect, near)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capsule_and_slices() {
        let f = Frustum::from_matrix(perspective(1.2, 1.5, 0.1));
        assert!(f.sphere(Vec3::new(0., 0., -10.), 0.5));
        assert!(!f.sphere(Vec3::new(0., 0., 10.), 0.5));
        let near = f.slice(Vec3::NEG_Z, 0.1, 20.);
        let far = f.slice(Vec3::NEG_Z, 60., 200.);
        let sun = Vec3::new(0.2, -0.3, -1.0).normalize();
        // behind the camera but its shadow falls forward into the far slice only when long enough
        let c = Vec3::new(0., 0., 5.);
        assert!(!near.sphere(c, 1.) && near.capsule(c, 1., sun, 30.));
        assert!(!far.capsule(c, 1., sun, 30.) && far.capsule(c, 1., sun, 100.));
    }
}
