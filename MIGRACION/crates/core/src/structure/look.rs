//! How a structure looks: a `Mesher` turns its parts into triangles (structure frame, every joint
//! at rest) for any renderer. The default draws every part — live or not — in its detailed look if
//! its kind has one (a generated hull panel, a model), else its plain shape flat-shaded in its
//! material, and says per vertex which part it is and which bone moves it. What changes while it
//! lives is not in the triangles: the renderer keeps a word per part (`part_state`: there or
//! gone, how damaged, lit) and the shader reads it, so a hit, a part destroyed, a ship broken in
//! two or a lamp going out never rebuild a look, and every structure of a blueprint shares one.
use super::{catalog::Catalog, state::Structure};
use glam::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    /// sRGB.
    pub albedo: [u8; 3],
    /// 0 sound .. 255 wrecked.
    pub damage: u8,
    pub rough: u8,
    pub metal: u8,
    pub glow: u8,
    /// A broken piece (bare material at its cut faces).
    pub fragment: u8,
    /// Plating the shader draws (`mesh::Material::panel`).
    pub panel: u8,
    /// The joint that moves it (0: none).
    pub bone: u8,
    /// See-through (glass).
    pub glass: u8,
    /// Its finish (`mesh::FINISHES`).
    pub finish: u8,
    /// Middle extent of its part (cm, 255 for 2.55 m and more): how big it looks from afar.
    pub size: u8,
    /// What its part is from afar: `INNER` inside a hull (not seen from outside), `WIRING` a cable,
    /// pipe or duct.
    pub inner: u8,
    /// Which part it is: its place in the blueprint for a structure that keeps its blueprint's look
    /// (`Structure::model`), else its index. `NO_PART` in simplified looks (parts merged).
    pub part: u16,
}

/// `Vertex::part` of a vertex that belongs to no one part.
pub const NO_PART: u16 = u16::MAX;

/// The word the renderer keeps per part: bits 0-7 its damage (0 sound .. 255 wrecked), `ALIVE`
/// while it is there, `LIT` while it works and its networks feed it (its glow shows).
pub const ALIVE: u32 = 1 << 8;
pub const LIT: u32 = 1 << 9;
/// Not there and not missing (cargo that left, `Part::left`): never shown, not even as gone.
pub const AWAY: u32 = 1 << 10;

/// The state word of every part of `s`, by `Vertex::part`: `out[k]` for the part whose place in
/// the look is `k` (`n` places: the blueprint's parts, or its own). Places it has no part for are
/// gone (0).
pub fn part_states(s: &Structure, n: usize, out: &mut Vec<u32>) {
    out.clear();
    out.resize(n, 0);
    for (i, p) in s.parts.iter().enumerate() {
        let k = if s.model { p.origin as usize } else { i };
        if k >= n {
            continue;
        }
        if !p.alive {
            if p.left {
                out[k] = AWAY;
            }
            continue;
        }
        let lit = p.working && p.supplied >= 0.5;
        out[k] = u32::from((p.damage() * 255.0) as u8) | ALIVE | if lit { LIT } else { 0 };
    }
}

pub trait Mesher: Send + Sync {
    /// Triangles (three vertices each) of `s` into `out` (cleared first).
    fn mesh(&self, s: &Structure, cat: &Catalog, out: &mut Vec<Vertex>);

    /// The same without its models (`catalog::Basic`): what it looks like from a little way
    /// off, and what its coarser levels are made from.
    fn basic(&self, s: &Structure, cat: &Catalog, out: &mut Vec<Vertex>) {
        self.mesh(s, cat, out);
    }
}

/// Whether `s` has any part whose look from afar is not its look up close (a model).
pub fn detailed(s: &Structure, cat: &Catalog) -> bool {
    s.parts.iter().any(|p| cat.parts[usize::from(p.kind)].basic != super::catalog::Basic::Same)
}

/// Detailed looks where a part has one, else flat faces in each part's material. Glass comes
/// last (the renderer draws that tail blended). Every part is in it, live or not, as if sound and
/// lit: what it is like now is the renderer's word per part (`part_states`).
pub struct FlatMesher;

impl Mesher for FlatMesher {
    fn mesh(&self, s: &Structure, cat: &Catalog, out: &mut Vec<Vertex>) {
        out.clear();
        for glass_pass in [false, true] {
            mesh_parts(s, cat, glass_pass, false, out);
        }
    }

    fn basic(&self, s: &Structure, cat: &Catalog, out: &mut Vec<Vertex>) {
        out.clear();
        for glass_pass in [false, true] {
            mesh_parts(s, cat, glass_pass, true, out);
        }
    }
}

/// Number of vertices before the glass tail of a look built by `FlatMesher`.
pub fn opaque_len(v: &[Vertex]) -> usize {
    v.iter().position(|x| x.glass != 0).unwrap_or(v.len())
}

/// Of the look of a part of glass, what is not smooth as glass is solid — its frame, its gasket,
/// its ground edge —: drawn with everything else, not blended.
pub const GLASS_ROUGH: u8 = 64;

fn mesh_parts(s: &Structure, cat: &Catalog, glass_pass: bool, basic: bool, out: &mut Vec<Vertex>) {
    {
        for (index, p) in s.parts.iter().enumerate() {
            let kind = &cat.parts[usize::from(p.kind)];
            let m = cat.material(p.kind);
            let glass = u8::from(m.glass);
            let albedo = kind.def.color.unwrap_or(m.color);
            let (damage, lit) = (0, true);
            let glow = kind.def.glow;
            let part = (if s.model { p.origin as usize } else { index }).min(usize::from(NO_PART) - 1) as u16;
            let bone = p.bone.min(255) as u8;
            let at = if p.bone > 0 { p.rest } else { p.local };
            let size = (kind.size * 100.0).round().clamp(1.0, 255.0) as u8;
            let inner = if kind.def.interior { INNER } else { 0 } | if kind.def.wiring { WIRING } else { 0 };
            // a whole part in its detailed look (from afar, in the one it has for that); a chipped
            // one or a broken piece in its shape
            let look = match (basic, kind.basic) {
                (true, super::catalog::Basic::Shape) => None,
                (true, super::catalog::Basic::Look(l)) => Some(l),
                _ => kind.look,
            };
            if let Some(look) = look.filter(|_| !p.fragment && std::sync::Arc::ptr_eq(&p.shape, &kind.shape)) {
                let mesh = &cat.looks[look as usize].1;
                for t in mesh.idx.chunks_exact(3) {
                    // (a triangle is of what its first corner is)
                    let clear = m.glass && mesh.mat[t[0] as usize].rough < GLASS_ROUGH;
                    if clear != glass_pass {
                        continue;
                    }
                    let glass = u8::from(clear);
                    for &i in t {
                        let i = i as usize;
                        let mat = mesh.mat[i];
                        out.push(Vertex {
                            pos: at.transform_point3(Vec3::from_array(mesh.pos[i])).to_array(),
                            nrm: at.transform_vector3(Vec3::from_array(mesh.nrm[i])).normalize_or_zero().to_array(),
                            albedo: mat.albedo,
                            damage,
                            rough: mat.rough,
                            metal: mat.metal,
                            glow: if lit { mat.emissive.max(glow) } else { 0 },
                            fragment: 0,
                            panel: mat.panel,
                            bone,
                            glass,
                            finish: mat.finish,
                            size,
                            inner,
                            part,
                        });
                    }
                }
                continue;
            }
            if m.glass != glass_pass {
                continue;
            }
            for f in &p.shape.faces {
                let n = at.transform_vector3(f.plane.n).normalize().to_array();
                let v = |q: Vec3| Vertex { pos: at.transform_point3(q).to_array(), nrm: n, albedo, damage, rough: m.rough, metal: m.metal, glow, fragment: u8::from(p.fragment), panel: 0, bone, glass, finish: m.finish, size, inner, part };
                for k in 1..f.verts.len() - 1 {
                    out.extend([v(f.verts[0]), v(f.verts[k]), v(f.verts[k + 1])]);
                }
            }
        }
    }
}

/// Coarser looks of a structure for distance: built from the full one (off the frame's thread by
/// the renderer). With `distances` the level is the one its distance asks (a structure looks as it
/// is up close and simplifies in steps far away); without, the finest whose triangles stay under
/// `density` per pixel of the disc it covers on screen.
pub trait LodBuilder: Send + Sync {
    /// Triangles per screen pixel the levels aim for.
    fn density(&self) -> f32;
    /// Levels 1.. from the full look (cleared first); `basic`, the look without its models if
    /// it has any (`Mesher::basic`): the first level, and what the rest are made from; `far`, the
    /// structure's far shape if its blueprint has one (`Catalog::far`).
    fn build(&self, full: &[Vertex], basic: Option<&[Vertex]>, far: Option<&crate::mesh::Mesh>, out: &mut Vec<Vec<Vertex>>);
    /// From how far (m, from its centre) each level 1.. may be drawn, for a structure of radius
    /// `radius` (none: by density alone); `models`: it has a level without its models.
    fn distances(&self, _radius: f32, _models: bool) -> Vec<f32> {
        Vec::new()
    }
}

/// `Vertex::inner` bits.
pub const INNER: u8 = 1;
pub const WIRING: u8 = 2;

/// One coarser look: what it keeps of the full one and how simplified.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    /// Parts thinner than this (middle extent, cm) are left out: bolts, cables, then pipes...
    pub min_size: u8,
    /// Parts inside the hull are kept only this big or more (cm; 255: none).
    pub inner_size: u8,
    /// Cables, pipes and ducts kept.
    pub wiring: bool,
    /// Share of the triangles kept by simplifying (1: all).
    pub keep: f32,
    /// Not drawn nearer than this many radii of the structure from its centre.
    pub from: f32,
}

/// The levels of detail of a structure, from its full look down to its far shape:
/// 0. (a structure with models) the same with every part in its plain look instead of its
///    model, from a few tens of metres: models are for standing by, and a hundred ships a
///    stone's throw away must cost what they did without them;
/// 1. everything but its cables, pipes and ducts and the small things inside (its outside
///    whole, and what an open door shows of the inside);
/// 2. its outside only (no inside part, no cable anywhere), windows dark;
/// 3. and 4. that simplified ever more (only big parts);
/// 5. its far shape (`Catalog::far`: a loft and boxes in its own colours, taken from its real
///    look), or its box in its average colour when it has none.
///
/// The levels that only leave parts out still tell them apart (a part gone is gone in them too);
/// the simplified ones and the far shape do not: the renderer shows a structure that has lost
/// parts no coarser than the last level that does. Each simplified level simplifies every bone on
/// its own so moving parts keep moving.
pub struct DetailLods {
    pub levels: Vec<Level>,
    /// Models are drawn up to this many radii from the centre, and never less than
    /// `models_least` m: past it, the look without them.
    pub models_to: f32,
    pub models_least: f32,
    /// The far shape from this many radii.
    pub far_from: f32,
    /// Radii are never counted smaller than this (m): small things keep their detail longer.
    pub min_radius: f32,
    pub density: f32,
}

/// The old name: every structure gets the same levels.
pub type DecimatedLods = DetailLods;

impl Default for DetailLods {
    fn default() -> Self {
        let l = |min_size, inner_size, keep, from| Level { min_size, inner_size, wiring: false, keep, from };
        // a ship of 10.6 m (the Alcotán): 200, 600, 1200, 2400 m, its far shape from 4.8 km
        DetailLods { levels: vec![l(2, 10, 1.0, 18.9), l(8, 255, 1.0, 56.6), l(20, 255, 0.4, 113.2), l(50, 255, 0.15, 226.4)], models_to: 3.8, models_least: 40.0, far_from: 452.8, min_radius: 5.0, density: 0.25 }
    }
}

fn to_mesh(v: &[Vertex]) -> crate::mesh::Mesh {
    let mut m = crate::mesh::Mesh::default();
    for (i, x) in v.iter().enumerate() {
        let dark = 1.0 - 0.5 * f32::from(x.damage) / 255.0;
        let albedo = x.albedo.map(|c| (f32::from(c) * dark) as u8);
        let mat = crate::mesh::Material { albedo, rough: x.rough, metal: x.metal, emissive: x.glow, panel: x.panel, finish: x.finish };
        m.vertex(Vec3::from_array(x.pos), Vec3::from_array(x.nrm), mat);
        if i % 3 == 2 {
            m.tri(i as u32 - 2, i as u32 - 1, i as u32);
        }
    }
    m
}

fn from_mesh(m: &crate::mesh::Mesh, bone: u8, out: &mut Vec<Vertex>) {
    for &i in &m.idx {
        let i = i as usize;
        let mat = m.mat[i];
        out.push(Vertex { pos: m.pos[i], nrm: m.nrm[i], albedo: mat.albedo, damage: 0, rough: mat.rough, metal: mat.metal, glow: mat.emissive, fragment: 0, panel: mat.panel, bone, glass: 0, finish: mat.finish, size: 255, inner: 0, part: NO_PART });
    }
}

/// What level `l` keeps of the full look: its triangles (windows made opaque and dark: there is
/// nothing to see through them any more).
pub fn filtered(full: &[Vertex], l: &Level) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(full.len());
    for t in full.chunks_exact(3) {
        let x = &t[0];
        if x.size < l.min_size || (x.inner & INNER != 0 && x.size < l.inner_size) || (x.inner & WIRING != 0 && !l.wiring) {
            continue;
        }
        for v in t {
            let mut v = *v;
            if v.glass != 0 {
                v.glass = 0;
                v.albedo = [18, 24, 30];
                v.rough = 18;
                v.metal = 0;
                v.glow = 0;
            }
            out.push(v);
        }
    }
    out
}

/// The bounding box of `v` in its area-weighted average material.
pub fn box_look(v: &[Vertex], out: &mut Vec<Vertex>) {
    out.clear();
    if v.len() < 3 {
        return;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let (mut sum, mut weight) = (Vec3::ZERO, 0.0_f32);
    for t in v.chunks_exact(3) {
        let p = [0, 1, 2].map(|i| Vec3::from_array(t[i].pos));
        let a = (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
        let dark = 1.0 - 0.5 * f32::from(t[0].damage) / 255.0;
        sum += Vec3::from_array(t[0].albedo.map(f32::from)) * dark * a;
        weight += a;
        for q in p {
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    let c = (sum / weight.max(1e-6)).to_array().map(|x| x as u8);
    let cube = super::convex::Convex::cuboid((hi - lo) * 0.5);
    let shift = (hi + lo) * 0.5;
    for f in &cube.faces {
        let n = f.plane.n.to_array();
        let at = |q: Vec3| Vertex { pos: (q + shift).to_array(), nrm: n, albedo: c, damage: 0, rough: 200, metal: 0, glow: 0, fragment: 0, panel: 0, bone: 0, glass: 0, finish: 0, size: 255, inner: 0, part: NO_PART };
        for k in 1..f.verts.len() - 1 {
            out.extend([at(f.verts[0]), at(f.verts[k]), at(f.verts[k + 1])]);
        }
    }
}

/// Boxes an automatic far shape may have at most.
const AUTO_BOXES: usize = 24;

/// A far shape for any structure that brings none (a building, a station, a piece of wreck): its
/// look voxelized coarsely, its hollows filled, and the solid merged into a few boxes (greedy:
/// runs along x, grown along y, then z). Coarser grids until it takes `AUTO_BOXES` or fewer.
///
/// It never has more than `max_tris` triangles (the level before it): coarser grids until then.
pub fn auto_far(src: &[Vertex], max_tris: usize) -> crate::mesh::Mesh {
    let mut m = crate::mesh::Mesh::default();
    if src.len() < 3 {
        return m;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in src {
        lo = lo.min(Vec3::from_array(v.pos));
        hi = hi.max(Vec3::from_array(v.pos));
    }
    let size = (hi - lo).max(Vec3::splat(1e-3));
    for cells in [20u32, 14, 10, 7, 5, 3] {
        let h = size.max_element() / cells as f32;
        let n = (size / h).ceil().as_uvec3().max(glam::UVec3::ONE);
        let (nx, ny, nz) = (n.x as usize, n.y as usize, n.z as usize);
        let idx = |x: usize, y: usize, z: usize| x + nx * (y + ny * z);
        let mut solid = vec![false; nx * ny * nz];
        // the surface: points over each triangle no farther apart than half a cell
        for t in src.chunks_exact(3) {
            let p = [0, 1, 2].map(|i| Vec3::from_array(t[i].pos));
            let k = ((p[0].distance(p[1]).max(p[1].distance(p[2])).max(p[2].distance(p[0]))) / (h * 0.5)).ceil().max(1.0) as usize;
            for a in 0..=k {
                for b in 0..=k - a {
                    let q = p[0] + (p[1] - p[0]) * (a as f32 / k as f32) + (p[2] - p[0]) * (b as f32 / k as f32);
                    let c = ((q - lo) / h).floor().as_uvec3().min(n - 1);
                    solid[idx(c.x as usize, c.y as usize, c.z as usize)] = true;
                }
            }
        }
        // the outside: everything the edges reach; the rest is solid (hollows filled)
        let mut out = vec![false; solid.len()];
        let mut q = std::collections::VecDeque::new();
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    if (x == 0 || y == 0 || z == 0 || x == nx - 1 || y == ny - 1 || z == nz - 1) && !solid[idx(x, y, z)] {
                        out[idx(x, y, z)] = true;
                        q.push_back((x, y, z));
                    }
                }
            }
        }
        while let Some((x, y, z)) = q.pop_front() {
            let nb = [(x.wrapping_sub(1), y, z), (x + 1, y, z), (x, y.wrapping_sub(1), z), (x, y + 1, z), (x, y, z.wrapping_sub(1)), (x, y, z + 1)];
            for (a, b, c) in nb {
                if a < nx && b < ny && c < nz && !solid[idx(a, b, c)] && !out[idx(a, b, c)] {
                    out[idx(a, b, c)] = true;
                    q.push_back((a, b, c));
                }
            }
        }
        let mut fill: Vec<bool> = out.iter().map(|o| !o).collect();
        // greedy boxes
        let mut boxes = Vec::new();
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    if !fill[idx(x, y, z)] {
                        continue;
                    }
                    let mut x1 = x;
                    while x1 + 1 < nx && fill[idx(x1 + 1, y, z)] {
                        x1 += 1;
                    }
                    let mut y1 = y;
                    while y1 + 1 < ny && (x..=x1).all(|i| fill[idx(i, y1 + 1, z)]) {
                        y1 += 1;
                    }
                    let mut z1 = z;
                    while z1 + 1 < nz && (y..=y1).all(|j| (x..=x1).all(|i| fill[idx(i, j, z1 + 1)])) {
                        z1 += 1;
                    }
                    for k in z..=z1 {
                        for j in y..=y1 {
                            for i in x..=x1 {
                                fill[idx(i, j, k)] = false;
                            }
                        }
                    }
                    boxes.push((Vec3::new(x as f32, y as f32, z as f32), Vec3::new(x1 as f32 + 1.0, y1 as f32 + 1.0, z1 as f32 + 1.0)));
                }
            }
        }
        if (boxes.len() > AUTO_BOXES || boxes.len() * 12 > max_tris) && cells > 3 {
            continue;
        }
        if boxes.len() * 12 > max_tris {
            return m;
        }
        let mat = crate::mesh::Material { albedo: [150, 150, 150], rough: 180, metal: 20, emissive: 0, panel: 0, finish: 0 };
        for (a, b) in boxes {
            // shrunk to what the look takes of the cells (no shell bigger than the thing)
            let (a, b) = ((lo + a * h).max(lo), (lo + b * h).min(hi));
            let piece = crate::mesh::cuboid(b - a, 0.0, mat);
            m.append(&piece, glam::Affine3A::from_translation((a + b) * 0.5));
        }
        break;
    }
    m
}

/// How far (m) from a far shape's face the real look it takes its colour from may be.
const PAINT_REACH: f32 = 0.45;

/// The far shape `far` in the colours of the look `src` (what of `src` lies on each of its faces,
/// facing the same way, area-weighted; the shape's own colour where nothing does). Flat faces.
pub fn paint_far(far: &crate::mesh::Mesh, src: &[Vertex], out: &mut Vec<Vertex>) {
    out.clear();
    // the look's triangles by the cell (1 m) of their centre
    let mut grid: std::collections::HashMap<(i32, i32, i32), Vec<u32>> = std::collections::HashMap::new();
    let cell = |p: Vec3| (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let tris: Vec<(Vec3, Vec3, f32, [f32; 5])> = src
        .chunks_exact(3)
        .map(|t| {
            let p = [0, 1, 2].map(|i| Vec3::from_array(t[i].pos));
            let c = (p[0] + p[1] + p[2]) / 3.0;
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            let a = n.length() * 0.5;
            let dark = 1.0 - 0.5 * f32::from(t[0].damage) / 255.0;
            let al = t[0].albedo.map(|x| f32::from(x) * dark);
            (c, n.normalize_or_zero(), a, [al[0], al[1], al[2], f32::from(t[0].rough), f32::from(t[0].metal)])
        })
        .collect();
    for (k, t) in tris.iter().enumerate() {
        grid.entry(cell(t.0)).or_default().push(k as u32);
    }
    for f in far.idx.chunks_exact(3) {
        let p = [0, 1, 2].map(|i| Vec3::from_array(far.pos[f[i] as usize]));
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n == Vec3::ZERO {
            continue;
        }
        let (lo, hi) = (p[0].min(p[1]).min(p[2]) - PAINT_REACH, p[0].max(p[1]).max(p[2]) + PAINT_REACH);
        let (c0, c1) = (cell(lo), cell(hi));
        let (mut sum, mut w) = ([0.0f32; 5], 0.0f32);
        for x in c0.0..=c1.0 {
            for y in c0.1..=c1.1 {
                for z in c0.2..=c1.2 {
                    let Some(list) = grid.get(&(x, y, z)) else {
                        continue;
                    };
                    for &k in list {
                        let (c, tn, a, m) = tris[k as usize];
                        if tn.dot(n) < 0.3 || (c - p[0]).dot(n).abs() > PAINT_REACH || !inside_prism(c, &p, n, 0.15) {
                            continue;
                        }
                        for i in 0..5 {
                            sum[i] += m[i] * a;
                        }
                        w += a;
                    }
                }
            }
        }
        let own = far.mat[f[0] as usize];
        let (albedo, rough, metal) = if w > 1e-6 { ([sum[0] / w, sum[1] / w, sum[2] / w].map(|x| x.round().clamp(0.0, 255.0) as u8), (sum[3] / w) as u8, (sum[4] / w) as u8) } else { (own.albedo, own.rough, own.metal) };
        for q in p {
            out.push(Vertex { pos: q.to_array(), nrm: n.to_array(), albedo, damage: 0, rough, metal, glow: 0, fragment: 0, panel: 0, bone: 0, glass: 0, finish: 0, size: 255, inner: 0, part: NO_PART });
        }
    }
}

/// Whether `c` projects inside triangle `p` (normal `n`), with a margin (share of its size).
fn inside_prism(c: Vec3, p: &[Vec3; 3], n: Vec3, margin: f32) -> bool {
    let q = c - n * (c - p[0]).dot(n);
    let (v0, v1, v2) = (p[1] - p[0], p[2] - p[0], q - p[0]);
    let (d00, d01, d11, d20, d21) = (v0.dot(v0), v0.dot(v1), v1.dot(v1), v2.dot(v0), v2.dot(v1));
    let den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-12 {
        return false;
    }
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    v >= -margin && w >= -margin && v + w <= 1.0 + margin
}

impl LodBuilder for DetailLods {
    fn density(&self) -> f32 {
        self.density
    }

    fn build(&self, full: &[Vertex], basic: Option<&[Vertex]>, far: Option<&crate::mesh::Mesh>, out: &mut Vec<Vec<Vertex>>) {
        out.clear();
        // (with models: the look without them first, as it is — glass and all —, and the rest
        // made from it)
        if let Some(b) = basic {
            out.push(b.to_vec());
        }
        let full = basic.unwrap_or(full);
        let mut outside = Vec::new();
        for l in &self.levels {
            let kept = filtered(full, l);
            if l.inner_size == 255 && outside.is_empty() {
                outside = kept.clone();
            }
            if l.keep >= 1.0 {
                out.push(kept);
                continue;
            }
            // the triangles of each bone apart, simplified
            let mut bones: Vec<u8> = kept.chunks_exact(3).map(|t| t[0].bone).collect();
            bones.sort_unstable();
            bones.dedup();
            let mut v = Vec::new();
            for &b in &bones {
                let tris: Vec<Vertex> = kept.chunks_exact(3).filter(|t| t[0].bone == b).flatten().copied().collect();
                let n = tris.len() / 3;
                from_mesh(&crate::simplify::decimate(&to_mesh(&tris), ((n as f32 * l.keep) as usize).max(12)), b, &mut v);
            }
            out.push(v);
        }
        // its far shape: the one its blueprint brings, else one made from its look; painted
        let src = if outside.is_empty() { full } else { &outside };
        let mut b = Vec::new();
        match far {
            Some(f) => paint_far(f, src, &mut b),
            None => {
                let before = out.last().map_or(usize::MAX, |l: &Vec<Vertex>| l.len() / 3);
                let shape = auto_far(src, before);
                if shape.idx.is_empty() { box_look(full, &mut b) } else { paint_far(&shape, src, &mut b) }
            }
        }
        out.push(b);
    }

    fn distances(&self, radius: f32, models: bool) -> Vec<f32> {
        let r = radius.max(self.min_radius);
        let near = models.then(|| (self.models_to * r).max(self.models_least));
        near.into_iter().chain(self.levels.iter().map(|l| l.from * r)).chain(std::iter::once(self.far_from * r)).collect()
    }
}
