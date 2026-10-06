//! CPU half of the GPU terrain generator: quadtree node keys, the node's precise local frame, and
//! the per-node noise table. Everything that needs f64 happens here; the GPU only ever sees small
//! numbers (offsets from the node's centre, noise phases split into integer cell + fraction).
use crate::cube_sphere::{FACES, cube_arc};
use crate::noise::smoothstep;
use crate::surface::{MAX_MOUNTAIN_OCTAVES, MAX_RELIEF_OCTAVES, Procedural};
use glam::DVec3;
use std::f64::consts::FRAC_PI_4;

/// Noise calls per sample, in the order the WGSL generator evaluates them.
pub const NOISE_CALLS: usize = 7 + MAX_MOUNTAIN_OCTAVES as usize + MAX_RELIEF_OCTAVES as usize;
pub use crate::surface::MAX_CRATER_LAYERS;

/// A quadtree node: face, level and integer cell at that level.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeKey {
    pub face: u8,
    pub level: u8,
    pub i: u32,
    pub j: u32,
}

impl NodeKey {
    pub const fn root(face: u8) -> NodeKey {
        NodeKey { face, level: 0, i: 0, j: 0 }
    }
    /// Extent in face parameters (the whole face is 2).
    pub fn size(&self) -> f64 {
        2.0 / (1_u64 << self.level) as f64
    }
    pub fn center_params(&self) -> (f64, f64) {
        let s = self.size();
        (-1.0 + (f64::from(self.i) + 0.5) * s, -1.0 + (f64::from(self.j) + 0.5) * s)
    }
    pub fn child(&self, k: u32) -> NodeKey {
        NodeKey { face: self.face, level: self.level + 1, i: self.i * 2 + (k & 1), j: self.j * 2 + (k >> 1) }
    }
    pub fn parent(&self) -> Option<NodeKey> {
        (self.level > 0).then(|| NodeKey { face: self.face, level: self.level - 1, i: self.i / 2, j: self.j / 2 })
    }
    /// Width over the surface (m).
    pub fn arc(&self, radius: f64) -> f64 {
        cube_arc(self.size(), radius)
    }
    pub fn frame(&self) -> NodeFrame {
        let (a, b) = self.center_params();
        NodeFrame::new(self.face as usize, a, b, self.size() * 0.5)
    }
}

/// A node's centre and its tangent-warp derivatives: enough to rebuild any point of the node
/// relative to its centre in f32 without cancellation (see `delta`, mirrored in `terrain_common.wgsl`).
#[derive(Clone, Copy, Debug)]
pub struct NodeFrame {
    pub face: usize,
    pub a: f64,
    pub b: f64,
    pub half: f64,
    pub alpha: [f64; 2],
    pub tan: [f64; 2],
    pub cos: [f64; 2],
    pub sin: [f64; 2],
    /// Unit direction of the centre and s = |n + A u + B v|.
    pub dir: DVec3,
    pub s: f64,
}

impl NodeFrame {
    pub fn new(face: usize, a: f64, b: f64, half: f64) -> NodeFrame {
        let alpha = [a * FRAC_PI_4, b * FRAC_PI_4];
        let tan = alpha.map(f64::tan);
        let f = &FACES[face];
        let v = DVec3::from_array(f.n) + DVec3::from_array(f.u) * tan[0] + DVec3::from_array(f.v) * tan[1];
        NodeFrame { face, a, b, half, alpha, tan, cos: alpha.map(f64::cos), sin: alpha.map(f64::sin), dir: v.normalize(), s: v.length() }
    }
    /// Unit direction at parameter offsets (da, db) from the centre, minus the centre's direction.
    /// The f64 twin of the WGSL `node_delta`.
    pub fn delta(&self, da: f64, db: f64) -> DVec3 {
        let f = &FACES[self.face];
        let (qa, qb) = (da * FRAC_PI_4, db * FRAC_PI_4);
        let d_a = qa.sin() / ((self.alpha[0] + qa).cos() * self.cos[0]);
        let d_b = qb.sin() / ((self.alpha[1] + qb).cos() * self.cos[1]);
        let (aa, bb) = (self.tan[0] + d_a, self.tan[1] + d_b);
        let s = (1.0 + aa * aa + bb * bb).sqrt();
        let ds = (2.0 * self.tan[0] * d_a + d_a * d_a + 2.0 * self.tan[1] * d_b + d_b * d_b) / (s + self.s);
        (DVec3::from_array(f.u) * d_a + DVec3::from_array(f.v) * d_b - self.dir * ds) / s
    }
}

/// One noise call's phase at the node's centre: integer cell (mod 256) and fraction, plus its scale.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoiseCall {
    pub cell: [i32; 3],
    pub frac: [f32; 3],
    pub scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct NodeGen {
    pub calls: [NoiseCall; NOISE_CALLS],
    pub relief_octaves: u32,
    pub mountain_octaves: u32,
    pub crater_layers: u32,
    pub min_feature: f32,
    pub broad_fade: f32,
    pub detail_fade: f32,
}

/// (scale, added phase, table) for every call, in WGSL order. Tables: 0 relief, 1 broad, 2 mare.
pub fn noise_calls(s: &Procedural) -> [(f64, [f64; 3], u32); NOISE_CALLS] {
    let def = s.def();
    let mut out = [(0.0, [0.0; 3], 0); NOISE_CALLS];
    let fh = 1. / def.highland_wavelength;
    out[0] = (fh, [0.; 3], 1);
    out[1] = (fh * 2.3, [11., 0., -7.], 1);
    let fm = 1. / def.mare_wavelength;
    out[2] = (fm, [0.; 3], 2);
    out[3] = (fm * 2.1, [-5., 3., 0.], 2);
    let f0 = 1. / def.mountains.wavelength;
    out[4] = (f0 * 0.28, [31., -13., 0.], 1);
    let mut f = f0;
    for i in 0..MAX_MOUNTAIN_OCTAVES as usize {
        out[5 + i] = (f, [43. + i as f64 * 7., -19., 61.], 0);
        f *= def.mountains.lacunarity;
    }
    let mut f = 1. / def.relief.wavelength;
    let r0 = 5 + MAX_MOUNTAIN_OCTAVES as usize;
    for i in 0..MAX_RELIEF_OCTAVES as usize {
        let k = i as f64;
        out[r0 + i] = (f, [k * 17.3, -k * 9.1, k * 3.7], 0);
        f *= def.relief.lacunarity;
    }
    out[NOISE_CALLS - 2] = (1. / 3100., [0.; 3], 0);
    out[NOISE_CALLS - 1] = (1. / 57., [0.; 3], 0);
    out
}

/// The node's generation table: `c` is the centre's point on the datum sphere, relative to the
/// body's centre (dir · radius); `min_feature` the node's cell (m).
pub fn node_gen(s: &Procedural, c: DVec3, min_feature: f64) -> NodeGen {
    let def = s.def();
    let mut calls = [NoiseCall::default(); NOISE_CALLS];
    for (k, (scale, add, _)) in noise_calls(s).iter().enumerate() {
        let o = c * *scale + DVec3::from_array(*add);
        let fl = o.floor();
        calls[k] = NoiseCall { cell: [0, 1, 2].map(|i| (fl[i] as i64 & 255) as i32), frac: [0, 1, 2].map(|i| (o[i] - fl[i]) as f32), scale: *scale as f32 };
    }
    let mut mountain_octaves = 0;
    let mut f = 1. / def.mountains.wavelength;
    for i in 0..def.mountains.count {
        if i > 0 && 1. / f < min_feature * 2. {
            break;
        }
        mountain_octaves += 1;
        f *= def.mountains.lacunarity;
    }
    let mut relief_octaves = 0;
    let mut f = 1. / def.relief.wavelength;
    for _ in 0..def.relief.count {
        if 1. / f < min_feature * 2. {
            break;
        }
        relief_octaves += 1;
        f *= def.relief.lacunarity;
    }
    let crater_layers = def.craters.iter().take_while(|l| l.cell * l.r_max * 2. >= min_feature * 1.5).count() as u32;
    NodeGen { calls, relief_octaves, mountain_octaves, crater_layers, min_feature: min_feature as f32, broad_fade: (1. - smoothstep(775., 1550., min_feature)) as f32, detail_fade: (1. - smoothstep(14., 28., min_feature)) as f32 }
}

/// Split an f64 into two f32 whose sum carries ~48 bits.
pub fn split(x: f64) -> [f32; 2] {
    let hi = x as f32;
    [hi, (x - f64::from(hi)) as f32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_sphere::cube_dir;
    #[test]
    fn local_frame_matches_absolute_directions() {
        for (face, a, b, half) in [(0, 0.3, -0.2, 0.25), (2, -0.99, 0.97, 2f64.powi(-17)), (5, 0.0, 0.0, 1.0)] {
            let f = NodeFrame::new(face, a, b, half);
            for (da, db) in [(half, -half), (-half * 0.3, half * 0.7), (0.0, 0.0)] {
                let d = DVec3::from_array(cube_dir(face, a + da, b + db));
                let err = (f.dir + f.delta(da, db) - d).length();
                assert!(err < 1e-14, "face {face} err {err}");
            }
        }
    }
}
