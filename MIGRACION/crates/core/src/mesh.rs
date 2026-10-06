//! CPU meshes (rigid or skinned) and the primitive shapes everything procedural is built from.
//! Materials travel per vertex, so any number of parts merges into one draw.
use glam::{Affine3A, Vec3};
use std::f32::consts::TAU;

/// Per-vertex surface: sRGB albedo, roughness, metalness and emission (all 0-255), the plating
/// the shader draws on it (`PANEL_*`: plate size in dm, windows bit; 0 = smooth) and its finish
/// (`FINISHES`: the texture it wears; 0 = none).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Material {
    pub albedo: [u8; 3],
    pub rough: u8,
    pub metal: u8,
    pub emissive: u8,
    pub panel: u8,
    pub finish: u8,
}

/// Surface finishes: tileable textures (relief, light and dark, sheen) parts wear, by name in the
/// data. The renderer's layers come in this order from `tools/texturas/acabados.py`
/// (`assets/textures/acabados`); 0 is none.
pub const FINISHES: [&str; 17] = ["liso", "pintura", "cepillado", "fundicion", "goma", "plastico", "tela", "lagrimado", "perforado", "rejilla", "acolchado", "carbono", "hormigon", "trenzado", "corrugado", "madera", "aislante"];

/// A finish by name (`FINISHES`).
pub fn finish(name: &str) -> Option<u8> {
    FINISHES.iter().position(|f| *f == name).map(|k| k as u8)
}

/// `Material::panel`: the low bits are the plate size in decimetres (up to 12.7 m)...
pub const PANEL_SIZE: u8 = 0x7f;
/// ...and this bit turns the plates of an emissive material into lit and dark windows.
pub const PANEL_WINDOWS: u8 = 0x80;

/// The `panel` code of plates `size` m wide, as windows or not.
pub fn panel_code(size: f32, windows: bool) -> u8 {
    let dm = (size * 10.0).round().clamp(0.0, f32::from(PANEL_SIZE)) as u8;
    if dm == 0 { 0 } else { dm | if windows { PANEL_WINDOWS } else { 0 } }
}

impl Material {
    pub const fn new(rgb: [u8; 3], rough: u8, metal: u8) -> Material {
        Material { albedo: rgb, rough, metal, emissive: 0, panel: 0, finish: 0 }
    }
    pub const fn glow(rgb: [u8; 3], emissive: u8) -> Material {
        Material { albedo: rgb, rough: 200, metal: 0, emissive, panel: 0, finish: 0 }
    }
    /// The same wearing finish `f`.
    pub const fn with_finish(self, f: u8) -> Material {
        Material { finish: f, ..self }
    }
}

/// A glowing point of a model (engine, beacon): drawn as a sprite that never shrinks below a
/// few pixels, so it stays visible long after the geometry is gone.
#[derive(Clone, Copy, Debug)]
pub struct Glow {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    pub mat: Vec<Material>,
    pub idx: Vec<u32>,
    /// Skinned meshes only (same length as `pos`), else empty.
    pub joints: Vec<[u8; 4]>,
    pub weights: Vec<[f32; 4]>,
}

impl Mesh {
    pub fn tris(&self) -> usize {
        self.idx.len() / 3
    }
    pub fn vertex(&mut self, p: Vec3, n: Vec3, m: Material) -> u32 {
        self.pos.push(p.to_array());
        self.nrm.push(n.normalize_or_zero().to_array());
        self.mat.push(m);
        (self.pos.len() - 1) as u32
    }
    pub fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.idx.extend_from_slice(&[a, b, c]);
    }
    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.idx.extend_from_slice(&[a, b, c, a, c, d]);
    }
    /// Append `other` transformed by `t` (normals by its rotation part; uniform scales assumed).
    pub fn append(&mut self, other: &Mesh, t: Affine3A) {
        let base = self.pos.len() as u32;
        for (i, p) in other.pos.iter().enumerate() {
            self.pos.push(t.transform_point3(Vec3::from_array(*p)).to_array());
            self.nrm.push(t.transform_vector3(Vec3::from_array(other.nrm[i])).normalize_or_zero().to_array());
            self.mat.push(other.mat[i]);
        }
        self.idx.extend(other.idx.iter().map(|i| i + base));
        if !other.joints.is_empty() {
            self.joints.extend_from_slice(&other.joints);
            self.weights.extend_from_slice(&other.weights);
        }
    }
    /// Bounding sphere around the AABB centre.
    pub fn bounds(&self) -> (Vec3, f32) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &self.pos {
            lo = lo.min(Vec3::from_array(*p));
            hi = hi.max(Vec3::from_array(*p));
        }
        let c = (lo + hi) * 0.5;
        let r = self.pos.iter().map(|p| Vec3::from_array(*p).distance(c)).fold(0.0, f32::max);
        (c, r)
    }
}

// ---- shapes (centred on the origin; cylinders and cones along +Y) ----

/// Box, optionally chamfered (`bevel` > 0 adds edge strips and corner triangles).
pub fn cuboid(size: Vec3, bevel: f32, m: Material) -> Mesh {
    let mut o = Mesh::default();
    let h = size * 0.5;
    let b = bevel.min(h.min_element() * 0.45).max(0.0);
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    for (k, n) in axes.iter().enumerate() {
        for s in [1.0f32, -1.0] {
            let n = *n * s;
            let u = axes[(k + 1) % 3] * s;
            let v = axes[(k + 2) % 3];
            let (hu, hv) = (h[(k + 1) % 3] - b, h[(k + 2) % 3] - b);
            let c = n * h[k];
            let q = [c - u * hu - v * hv, c + u * hu - v * hv, c + u * hu + v * hv, c - u * hu + v * hv];
            let i = q.map(|p| o.vertex(p, n, m));
            o.quad(i[0], i[1], i[2], i[3]);
        }
    }
    if b > 0.0 {
        // 12 edge strips joining the inset faces, 8 corner triangles
        for (k, ax) in axes.iter().enumerate() {
            let (a1, a2) = (axes[(k + 1) % 3], axes[(k + 2) % 3]);
            for s1 in [1.0f32, -1.0] {
                for s2 in [1.0f32, -1.0] {
                    let e = a1 * s1 * h[(k + 1) % 3] + a2 * s2 * h[(k + 2) % 3];
                    let n = (a1 * s1 + a2 * s2).normalize();
                    let l = *ax * (h[k] - b);
                    let p1 = e - a1 * s1 * b;
                    let p2 = e - a2 * s2 * b;
                    let i = [p1 - l, p2 - l, p2 + l, p1 + l].map(|p| o.vertex(p, n, m));
                    if s1 * s2 > 0.0 { o.quad(i[0], i[3], i[2], i[1]) } else { o.quad(i[0], i[1], i[2], i[3]) }
                }
            }
        }
        for sx in [1.0f32, -1.0] {
            for sy in [1.0f32, -1.0] {
                for sz in [1.0f32, -1.0] {
                    let c = h * Vec3::new(sx, sy, sz);
                    let n = Vec3::new(sx, sy, sz).normalize();
                    let p = [c - Vec3::X * sx * b, c - Vec3::Y * sy * b, c - Vec3::Z * sz * b];
                    let i = p.map(|p| o.vertex(p, n, m));
                    if sx * sy * sz > 0.0 { o.tri(i[0], i[1], i[2]) } else { o.tri(i[0], i[2], i[1]) }
                }
            }
        }
    }
    o
}

/// Truncated cone (r0 at -len/2, r1 at +len/2); a cylinder when r0 == r1.
pub fn cone(r0: f32, r1: f32, len: f32, seg: u32, caps: bool, m: Material) -> Mesh {
    let mut o = Mesh::default();
    let seg = seg.max(3);
    let (y0, y1) = (-len * 0.5, len * 0.5);
    let slope = (r0 - r1) / len.max(1e-6);
    for s in 0..=seg {
        let a = s as f32 / seg as f32 * TAU;
        let (sn, cs) = a.sin_cos();
        let n = Vec3::new(cs, slope, sn);
        o.vertex(Vec3::new(cs * r0, y0, sn * r0), n, m);
        o.vertex(Vec3::new(cs * r1, y1, sn * r1), n, m);
    }
    for s in 0..seg {
        let i = s * 2;
        o.quad(i, i + 1, i + 3, i + 2);
    }
    if caps {
        for (y, r, n) in [(y0, r0, -Vec3::Y), (y1, r1, Vec3::Y)] {
            if r <= 0.0 {
                continue;
            }
            let c = o.vertex(Vec3::new(0., y, 0.), n, m);
            let first = o.pos.len() as u32;
            for s in 0..seg {
                let (sn, cs) = (s as f32 / seg as f32 * TAU).sin_cos();
                o.vertex(Vec3::new(cs * r, y, sn * r), n, m);
            }
            for s in 0..seg {
                let (a, b) = (first + s, first + (s + 1) % seg);
                if n.y > 0.0 { o.tri(c, b, a) } else { o.tri(c, a, b) }
            }
        }
    }
    o
}

/// Triangular prism: a box whose top slopes down to zero height at +Z.
pub fn wedge(size: Vec3, m: Material) -> Mesh {
    let mut o = Mesh::default();
    let h = size * 0.5;
    let p = |x: f32, y: f32, z: f32| Vec3::new(x * h.x, y * h.y, z * h.z);
    let (a, b, c, d) = (p(-1., -1., -1.), p(1., -1., -1.), p(1., -1., 1.), p(-1., -1., 1.));
    let (e, f) = (p(-1., 1., -1.), p(1., 1., -1.));
    let mut face = |pts: &[Vec3]| {
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize();
        let i: Vec<u32> = pts.iter().map(|q| o.vertex(*q, n, m)).collect();
        if i.len() == 4 { o.quad(i[0], i[1], i[2], i[3]) } else { o.tri(i[0], i[1], i[2]) }
    };
    face(&[a, b, c, d]);
    face(&[a, e, f, b]);
    face(&[e, d, c, f]);
    face(&[a, d, e]);
    face(&[b, f, c]);
    o
}

/// UV sphere (or its upper half when `half`).
pub fn sphere(r: f32, seg: u32, half: bool, m: Material) -> Mesh {
    let mut o = Mesh::default();
    let (seg, rings) = (seg.max(4), (seg / 2).max(2));
    let top = if half { rings / 2 } else { rings };
    for j in 0..=top {
        let v = j as f32 / rings as f32 * std::f32::consts::PI;
        for i in 0..=seg {
            let u = i as f32 / seg as f32 * TAU;
            let n = Vec3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
            o.vertex(n * r, n, m);
        }
    }
    let w = seg + 1;
    for j in 0..top {
        for i in 0..seg {
            let a = j * w + i;
            o.quad(a, a + 1, a + w + 1, a + w);
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shapes_face_outwards() {
        let m = Material::default();
        for (k, mesh) in [cuboid(Vec3::ONE, 0.1, m), cone(1., 0.5, 2., 12, true, m), wedge(Vec3::ONE, m), sphere(1., 12, false, m)].iter().enumerate() {
            for t in mesh.idx.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from_array(mesh.pos[t[k] as usize]));
                let face = (b - a).cross(c - a);
                if face.length() < 1e-5 {
                    continue;
                }
                let n = Vec3::from_array(mesh.nrm[t[0] as usize]);
                assert!(face.dot(n) > 0.0, "inward triangle in shape {k}: {t:?}");
            }
        }
    }
}
