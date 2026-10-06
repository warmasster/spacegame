//! Tangent-warp cube sphere: face ordering and winding match the TS original.
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
pub type V3 = [f64; 3];
pub struct Face {
    pub n: V3,
    pub u: V3,
    pub v: V3,
}
pub const FACES: [Face; 6] = [
    Face { n: [1., 0., 0.], u: [0., 0., -1.], v: [0., 1., 0.] },
    Face { n: [-1., 0., 0.], u: [0., 0., 1.], v: [0., 1., 0.] },
    Face { n: [0., 1., 0.], u: [1., 0., 0.], v: [0., 0., -1.] },
    Face { n: [0., -1., 0.], u: [1., 0., 0.], v: [0., 0., 1.] },
    Face { n: [0., 0., 1.], u: [1., 0., 0.], v: [0., 1., 0.] },
    Face { n: [0., 0., -1.], u: [-1., 0., 0.], v: [0., 1., 0.] },
];
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn normalize(v: V3) -> V3 {
    let n = dot(v, v).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}
pub fn cube_dir(face: usize, a: f64, b: f64) -> V3 {
    let f = &FACES[face];
    let a = (a * FRAC_PI_4).tan();
    let b = (b * FRAC_PI_4).tan();
    normalize(std::array::from_fn(|i| f.n[i] + a * f.u[i] + b * f.v[i]))
}
pub fn params_on(face: usize, d: V3) -> Option<[f64; 2]> {
    let f = &FACES[face];
    let dn = dot(d, f.n);
    if dn <= 1e-12 {
        return None;
    }
    Some([(dot(d, f.u) / dn).atan() * (4.0 / PI), (dot(d, f.v) / dn).atan() * (4.0 / PI)])
}
pub fn face_of(d: V3) -> (usize, [f64; 2]) {
    let [x, y, z] = d;
    let [ax, ay, az] = [x.abs(), y.abs(), z.abs()];
    let face = if ax >= ay && ax >= az {
        usize::from(x < 0.)
    } else if ay >= az {
        2 + usize::from(y < 0.)
    } else {
        4 + usize::from(z < 0.)
    };
    (face, params_on(face, d).expect("nonzero direction required"))
}
pub fn cube_arc(size: f64, radius: f64) -> f64 {
    (size / 2.0) * FRAC_PI_2 * radius
}
pub fn cell_of(p: f64, level: u32) -> u32 {
    let n = 1_u32 << level;
    (((p + 1.0) / 2.0 * f64::from(n)).floor().max(0.0) as u32).min(n - 1)
}
pub fn level_for(m: f64, radius: f64, max_level: u32) -> u32 {
    (cube_arc(2.0, radius) / m.max(1e-6)).log2().floor().clamp(0.0, f64::from(max_level)) as u32
}
