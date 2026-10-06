//! Geometry generators: the hull loft, decks, bulkheads and conduits, each piece a convex part
//! with a detailed look (smooth hull skin with chines, an inside face in the cabin colour,
//! chamfered edges; round tubes; ribbed ducts). Everything is built at rest, in the ship frame
//! (+Y up, nose toward +Z, +X to port).
use crate::def::{BulkheadDef, FloorDef, HullDef};
use glam::{Affine3A, Quat, Vec2, Vec3};
use lunar_core::{
    mesh::{Material, Mesh, panel_code},
    structure::convex::Convex,
};

/// What a generated part is, for tagging (compartment boundaries, hull, conduits...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Hull,
    Glass,
    Floor,
    Bulkhead,
    /// Trunks, outlets, ducts, and the cables and pipes that are only seen.
    Conduit,
    Component,
}

#[derive(Clone, Debug)]
pub struct GenPart {
    pub id: String,
    pub material: String,
    pub color: Option<[u8; 3]>,
    pub glow: u8,
    pub hollow: Option<f32>,
    /// In the part frame.
    pub shape: Convex,
    pub look: Option<Mesh>,
    /// Part frame → ship frame (at rest).
    pub at: Affine3A,
    pub role: Role,
    /// Compartments it bounds (one: toward vacuum; two: between them).
    pub bounds: Vec<String>,
    /// A detailed model (a look loaded from the models) to draw instead of `look`.
    pub model: Option<String>,
    /// `GHOST`, `NO_COLLIDE`, `SPARKS` (see `lunar_core::structure::catalog::PartKindDef`).
    pub flags: u8,
    /// What it lets go of when it is destroyed.
    pub burst: Option<lunar_core::structure::catalog::BurstDef>,
    /// Its own surface as its data has it (a component's piece): what a model's `tinte` wears.
    pub surface: Option<lunar_core::mesh::Material>,
    /// Its plain shape with its edges broken (a component's piece without a model): what it
    /// looks like from close by, where a razor edge reads as a toy.
    pub fine: Option<Mesh>,
    /// Its model is drawn mirrored in its own x (the copy across the centreline of a piece with
    /// a style: a wing's tip stays outboard).
    pub mirrored: bool,
}

/// Only seen: nothing strikes it or weighs it.
pub const GHOST: u8 = 1;
/// Bodies do not touch it; shots and blasts do.
pub const NO_COLLIDE: u8 = 2;
/// Badly damaged, it spits sparks.
pub const SPARKS: u8 = 4;

impl GenPart {
    /// A part whose points are given in the ship frame: its frame is centred on them.
    pub fn from_points(id: String, material: &str, role: Role, pts: &[Vec3]) -> Option<GenPart> {
        let c = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        let local: Vec<Vec3> = pts.iter().map(|p| *p - c).collect();
        let shape = Convex::hull(&local)?;
        Some(GenPart {
            id,
            material: material.to_string(),
            color: None,
            glow: 0,
            hollow: None,
            shape,
            look: None,
            at: Affine3A::from_translation(c),
            role,
            bounds: Vec::new(),
            model: None,
            flags: 0,
            burst: None,
            surface: None,
            fine: None,
            mirrored: false,
        })
    }
}

fn mat(rgb: [u8; 3], rough: u8, metal: u8, panel: u8) -> Material {
    Material { albedo: rgb, rough, metal, emissive: 0, panel, finish: 0 }
}

/// A finish by name (`lunar_core::mesh::FINISHES`), none if unknown.
pub fn fin(name: &str) -> u8 {
    lunar_core::mesh::finish(name).unwrap_or(0)
}

/// Inward offset of a closed convex outline (counter-clockwise) by `t`, mitred.
pub fn offset(outline: &[Vec2], t: f32) -> Vec<Vec2> {
    let n = outline.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = outline[(i + n - 1) % n];
        let p = outline[i];
        let next = outline[(i + 1) % n];
        // inward normals of the two edges (counter-clockwise: inward is the left)
        let e0 = (p - prev).normalize_or_zero();
        let e1 = (next - p).normalize_or_zero();
        let n0 = Vec2::new(-e0.y, e0.x);
        let n1 = Vec2::new(-e1.y, e1.x);
        let bis = (n0 + n1).normalize_or(n0);
        let cos = bis.dot(n0).max(0.3);
        out.push(p + bis * (t / cos));
    }
    out
}

/// The outline of station `s` (scaled, lifted) as 2D points.
fn ring(h: &HullDef, s: &crate::def::StationDef) -> Result<Vec<Vec2>, String> {
    let o = h.perfiles.get(&s.perfil).ok_or_else(|| format!("casco: perfil desconocido '{}'", s.perfil))?;
    Ok(o.iter().map(|p| Vec2::new(p[0] * s.escala[0], p[1] * s.escala[1] + s.y)).collect())
}

/// Area-signed check: counter-clockwise outlines have positive area.
fn ccw(o: &[Vec2]) -> bool {
    let mut a = 0.0;
    for i in 0..o.len() {
        let (p, q) = (o[i], o[(i + 1) % o.len()]);
        a += p.x * q.y - q.x * p.y;
    }
    a > 0.0
}

/// The hull loft: one part per facet per station interval (and division), caps if asked.
pub fn hull(h: &HullDef, prefix: &str) -> Result<Vec<GenPart>, String> {
    let st = &h.estaciones;
    if st.len() < 2 {
        return Err("casco: hacen falta al menos dos estaciones".into());
    }
    let rings: Vec<Vec<Vec2>> = st.iter().map(|s| ring(h, s)).collect::<Result<_, _>>()?;
    let n = rings[0].len();
    if n < 3 || rings.iter().any(|r| r.len() != n) {
        return Err("casco: todos los perfiles deben tener los mismos puntos (3 o más)".into());
    }
    for (r, s) in rings.iter().zip(st) {
        if !ccw(r) {
            return Err(format!("casco: el perfil '{}' debe ir en sentido antihorario visto desde el morro", s.perfil));
        }
    }
    let interior = h.interior.unwrap_or([196, 192, 182]);
    let plate = panel_code(h.chapa, false);
    let mut out = Vec::new();
    let t = h.espesor;
    for i in 0..st.len() - 1 {
        let divs = st[i].divisiones.max(1);
        for d in 0..divs {
            let (fa, fb) = (d as f32 / divs as f32, (d + 1) as f32 / divs as f32);
            let za = st[i].z + (st[i + 1].z - st[i].z) * fa;
            let zb = st[i].z + (st[i + 1].z - st[i].z) * fb;
            let ra: Vec<Vec2> = rings[i].iter().zip(&rings[i + 1]).map(|(a, b)| a.lerp(*b, fa)).collect();
            let rb: Vec<Vec2> = rings[i].iter().zip(&rings[i + 1]).map(|(a, b)| a.lerp(*b, fb)).collect();
            let (ia, ib) = (offset(&ra, t), offset(&rb, t));
            for k in 0..n {
                if h.huecos.iter().any(|&[s, f]| s == i && f == k) {
                    continue;
                }
                let k1 = (k + 1) % n;
                let p3 = |v: Vec2, z: f32| Vec3::new(v.x, v.y, z);
                let outer = [p3(ra[k], za), p3(ra[k1], za), p3(rb[k1], zb), p3(rb[k], zb)];
                let inner = [p3(ia[k], za), p3(ia[k1], za), p3(ib[k1], zb), p3(ib[k], zb)];
                let glass = h.ventanas.iter().any(|&[s, f]| s == i && f == k);
                let id = if divs > 1 { format!("{prefix}.{i}.{d}.{k}") } else { format!("{prefix}.{i}.{k}") };
                let pts: Vec<Vec3> = outer.iter().chain(inner.iter()).copied().collect();
                let material = if glass { "cristal" } else { h.material.as_str() };
                let Some(mut part) = GenPart::from_points(id, material, if glass { Role::Glass } else { Role::Hull }, &pts) else {
                    continue;
                };
                // the look: smooth or creased skin, the inside face, chamfered sides
                let paint = h.pintura.iter().rev().find(|p| p.caras.contains(&k) && p.tramos.is_none_or(|[a, b]| i >= a && i <= b));
                let skin = paint.map_or(mat([222, 222, 216], 100, 0, plate), |p| mat(p.color, p.rugosidad.unwrap_or(100), p.metal.unwrap_or(0), plate)).with_finish(fin("pintura"));
                let normal_at = |r: &[Vec2], kk: usize, z: f32| -> Vec3 {
                    // the outward normal of facet j of ring r (2D), as 3D (ignoring taper)
                    let fnorm = |j: usize| {
                        let e = r[(j + 1) % n] - r[j];
                        Vec3::new(e.y, -e.x, 0.0).normalize_or(Vec3::Y)
                    };
                    let _ = z;
                    if h.suaves.contains(&kk) { (fnorm((kk + n - 1) % n) + fnorm(kk)).normalize_or(Vec3::Y) } else { fnorm(k) }
                };
                let c = part.at.translation;
                let rel = |p: Vec3| p - Vec3::from(c);
                let edge = mat([88, 90, 94], 170, 120, 0).with_finish(fin("cepillado"));
                let mut m = Mesh::default();
                if glass {
                    let g = mat([120, 160, 185], 10, 0, 0);
                    quad(&mut m, [rel(outer[0]), rel(outer[1]), rel(outer[2]), rel(outer[3])], None, g);
                    quad(&mut m, [rel(inner[3]), rel(inner[2]), rel(inner[1]), rel(inner[0])], None, g);
                } else {
                    let ns = [normal_at(&ra, k, za), normal_at(&ra, k1, za), normal_at(&rb, k1, zb), normal_at(&rb, k, zb)];
                    quad(&mut m, [rel(outer[0]), rel(outer[1]), rel(outer[2]), rel(outer[3])], Some(ns), skin);
                    let inside = mat(interior, 150, 0, panel_code(0.7, false)).with_finish(fin(h.acabado_interior.as_deref().unwrap_or("pintura")));
                    quad(&mut m, [rel(inner[3]), rel(inner[2]), rel(inner[1]), rel(inner[0])], None, inside);
                }
                // edges: four sides
                for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
                    quad(&mut m, [rel(outer[b]), rel(outer[a]), rel(inner[a]), rel(inner[b])], None, edge);
                }
                part.look = Some(m);
                // from close by: a plate with its seam cut round it and its smooth chines
                // rounded; a window in its frame
                let centre = Vec3::from(c);
                part.fine = if glass {
                    Some(window_look(outer.map(|p| p - centre), inner.map(|p| p - centre), interior))
                } else {
                    let reach = |s: usize, kk: usize| {
                        if rounded(h, st.len(), n, s, kk) { FILLET } else { 0.0 }
                    };
                    let at = |kk: usize, f: f32| reach(i, kk) + (reach(i + 1, kk) - reach(i, kk)) * f;
                    let inside = mat(interior, 150, 0, panel_code(0.7, false)).with_finish(fin(h.acabado_interior.as_deref().unwrap_or("pintura")));
                    plate_look(&ra, &rb, k, [za, zb], [[at(k, fa), at(k1, fa)], [at(k, fb), at(k1, fb)]], &h.suaves, t, inner.map(|p| p - centre), centre, skin, inside, edge)
                };
                if let Some(c) = &st[i].compartimento
                    && !h.bajo_suelo.contains(&k)
                {
                    part.bounds.push(c.clone());
                }
                out.push(part);
            }
        }
    }
    // caps
    for (end, cap) in h.tapas.iter().enumerate() {
        if !cap {
            continue;
        }
        let (si, dir) = if end == 0 { (0, 1.0) } else { (st.len() - 1, -1.0) };
        let z = st[si].z;
        let r = &rings[si];
        let pts: Vec<Vec3> = r.iter().map(|p| Vec3::new(p.x, p.y, z)).chain(r.iter().map(|p| Vec3::new(p.x, p.y, z + dir * t))).collect();
        if let Some(mut part) = GenPart::from_points(format!("{prefix}.tapa{end}"), &h.material, Role::Hull, &pts) {
            let c = Vec3::from(part.at.translation);
            let skin = mat([210, 210, 204], 110, 0, plate).with_finish(fin("pintura"));
            let mut m = Mesh::default();
            let outside: Vec<Vec3> = r.iter().map(|p| Vec3::new(p.x, p.y, z) - c).collect();
            let inside: Vec<Vec3> = r.iter().map(|p| Vec3::new(p.x, p.y, z + dir * t) - c).collect();
            fan(&mut m, &outside, Vec3::new(0.0, 0.0, -dir), skin);
            fan(&mut m, &inside, Vec3::new(0.0, 0.0, dir), mat(interior, 200, 0, 0).with_finish(fin("pintura")));
            part.look = Some(m);
            if let Some(cmp) = &st[si.min(st.len() - 2)].compartimento {
                part.bounds.push(cmp.clone());
            }
            out.push(part);
        }
    }
    Ok(out)
}

/// How far along each plate a smooth chine's rounding reaches (m), and the seam cut between two
/// plates: how wide on each and how deep (m).
const FILLET: f32 = 0.24;
const SEAM: f32 = 0.007;
const SEAM_DEPTH: f32 = 0.006;
/// Steps of a rounded chine's arc (even: each plate takes half).
const ARC: usize = 6;

/// Whether the chine at vertex `k` of station `s` is rounded: a smooth one with no window and no
/// gap among the four plates that meet there (a window keeps its corners; so does what is by it).
fn rounded(h: &HullDef, stations: usize, n: usize, s: usize, k: usize) -> bool {
    if !h.suaves.contains(&k) {
        return false;
    }
    let before = (k + n - 1) % n;
    let open = |i: usize, f: usize| h.ventanas.iter().chain(&h.huecos).any(|&[a, b]| a == i && b == f);
    let around = [s.checked_sub(1), (s + 1 < stations).then_some(s)];
    !around.iter().flatten().any(|&i| open(i, before) || open(i, k))
}

/// The rounded corner at vertex `k` of outline `r` (counter-clockwise): the points of its arc and
/// their outward normals, from the edge before it to the edge after it (`ARC` + 1 of them). It
/// reaches `reach` along each edge (0: all its points at the corner itself) and cuts into the
/// corner no deeper than `sink`. None where the outline does not turn outward.
fn corner_arc(r: &[Vec2], k: usize, reach: f32, sink: f32) -> Option<Vec<(Vec2, Vec2)>> {
    let m = r.len();
    let (prev, p, next) = (r[(k + m - 1) % m], r[k], r[(k + 1) % m]);
    let (l0, l1) = ((p - prev).length(), (next - p).length());
    if l0 < 1e-5 || l1 < 1e-5 {
        return None;
    }
    let (e0, e1) = ((p - prev) / l0, (next - p) / l1);
    let theta = e0.dot(e1).clamp(-1.0, 1.0).acos();
    if e0.perp_dot(e1) <= 1e-4 || theta < 0.03 {
        return None;
    }
    let t = reach.min(0.4 * l0).min(0.4 * l1).min(sink / (theta * 0.25).tan()).max(1e-4);
    let radius = t / (theta * 0.5).tan();
    let n0 = Vec2::new(e0.y, -e0.x);
    let centre = p - e0 * t - n0 * radius;
    Some(
        (0..=ARC)
            .map(|i| {
                let n = Vec2::from_angle(theta * i as f32 / ARC as f32).rotate(n0);
                (centre + n * radius, n)
            })
            .collect(),
    )
}

/// Facet `k` of outline `r` as the fine skin has it, from vertex `k` to the next: half of each
/// smooth end's arc (`reach`: how far each of the two reaches), else the vertex itself.
fn section(r: &[Vec2], k: usize, smooth: &[usize], reach: [f32; 2], sink: f32) -> Vec<(Vec2, Vec2)> {
    let k1 = (k + 1) % r.len();
    let e = (r[k1] - r[k]).normalize_or_zero();
    let flat = Vec2::new(e.y, -e.x);
    let mut out = Vec::with_capacity(ARC + 2);
    match smooth.contains(&k).then(|| corner_arc(r, k, reach[0], sink)).flatten() {
        Some(arc) => out.extend_from_slice(&arc[ARC / 2..]),
        None => out.push((r[k], flat)),
    }
    match smooth.contains(&k1).then(|| corner_arc(r, k1, reach[1], sink)).flatten() {
        Some(arc) => out.extend_from_slice(&arc[..=ARC / 2]),
        None => out.push((r[k1], flat)),
    }
    out
}

/// A quad facing `toward` (its corners in either order round it), flat.
pub(crate) fn quad_facing(m: &mut Mesh, p: [Vec3; 4], toward: Vec3, material: Material) {
    let face = (p[1] - p[0]).cross(p[2] - p[0]) + (p[2] - p[0]).cross(p[3] - p[0]);
    quad(m, if face.dot(toward) >= 0.0 { p } else { [p[3], p[2], p[1], p[0]] }, None, material);
}

/// A hull plate from close by (facet `k` between the outlines `ra` at `z[0]` and `rb` at
/// `z[1]`, in the ship frame; the mesh about `centre`): its skin flush, rounded at its smooth
/// chines, a seam cut all round it down to its edge; its inside face; its four sides.
#[allow(clippy::too_many_arguments)]
fn plate_look(ra: &[Vec2], rb: &[Vec2], k: usize, z: [f32; 2], reach: [[f32; 2]; 2], smooth: &[usize], thickness: f32, inner: [Vec3; 4], centre: Vec3, skin: Material, inside: Material, edge: Material) -> Option<Mesh> {
    let sink = thickness * 0.5;
    let (sa, sb) = (section(ra, k, smooth, reach[0], sink), section(rb, k, smooth, reach[1], sink));
    let n = sa.len();
    if n != sb.len() || n < 2 || (z[1] - z[0]).abs() < 4.0 * SEAM {
        return None;
    }
    let p3 = |v: Vec2, z: f32| Vec3::new(v.x, v.y, z) - centre;
    let n3 = |v: Vec2| Vec3::new(v.x, v.y, 0.0);
    // the plate's edge (sunk into the seam) and its flush skin (a seam's width in from it), each
    // a row at either end
    let dir = (z[1] - z[0]).signum();
    let mut rim: [Vec<Vec3>; 2] = [Vec::with_capacity(n), Vec::with_capacity(n)];
    let mut flush: [Vec<(Vec3, Vec3)>; 2] = [Vec::with_capacity(n), Vec::with_capacity(n)];
    for (row, (s, zz)) in [(&sa, z[0]), (&sb, z[1])].into_iter().enumerate() {
        let inward = if row == 0 { dir } else { -dir } * SEAM;
        for i in 0..n {
            let (p, nrm) = s[i];
            rim[row].push(p3(p - nrm * SEAM_DEPTH, zz));
            // the ends: a seam's width along the outline toward the plate's middle
            let q = if i == 0 {
                p + (s[1].0 - p).clamp_length_max(SEAM)
            } else if i + 1 == n {
                p + (s[n - 2].0 - p).clamp_length_max(SEAM)
            } else {
                p
            };
            flush[row].push((p3(q, zz + inward), n3(nrm)));
        }
    }
    let mut m = Mesh::default();
    let out = |i: usize| (flush[0][i].1 + flush[1][i].1).normalize_or(Vec3::Y);
    for i in 0..n - 1 {
        let (a0, a1, b1, b0) = (flush[0][i], flush[0][i + 1], flush[1][i + 1], flush[1][i]);
        if (a1.0 - a0.0).length_squared() < 1e-10 && (b1.0 - b0.0).length_squared() < 1e-10 {
            continue;
        }
        quad_facing(&mut m, [a0.0, a1.0, b1.0, b0.0], out(i) + out(i + 1), skin);
        // (smooth across the plate: each corner its own normal)
        let base = m.nrm.len() - 4;
        let face = Vec3::from_array(m.nrm[base]);
        for (slot, p) in (base..base + 4).zip([a0, a1, b1, b0]) {
            // quad_facing may have turned the corners round: find each by its place
            let at = Vec3::from_array(m.pos[slot]);
            let own = [a0, a1, b1, b0].into_iter().find(|c| c.0.distance_squared(at) < 1e-12).unwrap_or(p).1;
            m.nrm[slot] = if own.dot(face) > 0.2 { own.to_array() } else { face.to_array() };
        }
        // the seam's slope at either end of the strip
        for row in 0..2 {
            quad_facing(&mut m, [rim[row][i], rim[row][i + 1], flush[row][i + 1].0, flush[row][i].0], out(i) + out(i + 1), skin);
        }
    }
    // the seam down the two chines, and the plate's sides behind them
    let along = (p3(sa[n - 1].0, z[0]) - p3(sa[0].0, z[0])).normalize_or(Vec3::X);
    for (i, side, corners) in [(0, -along, [inner[0], inner[3]]), (n - 1, along, [inner[1], inner[2]])] {
        quad_facing(&mut m, [rim[0][i], flush[0][i].0, flush[1][i].0, rim[1][i]], out(i), skin);
        quad_facing(&mut m, [rim[0][i], rim[1][i], corners[1], corners[0]], side, edge);
    }
    for (row, corners) in [(0, [inner[1], inner[0]]), (1, [inner[2], inner[3]])] {
        let facing = Vec3::Z * if row == 0 { -dir } else { dir };
        let mut poly: Vec<Vec3> = rim[row].clone();
        poly.extend(corners);
        poly.dedup_by(|a, b| a.distance_squared(*b) < 1e-10);
        if poly.len() >= 3 {
            fan(&mut m, &poly, facing, edge);
        }
    }
    quad(&mut m, [inner[3], inner[2], inner[1], inner[0]], None, inside);
    Some(m)
}

/// A window from close by: its pane set back in a frame that stands a little proud of the skin,
/// outside and inside. (Of a window's look, what is not smooth as glass is solid: its frame.)
fn window_look(outer: [Vec3; 4], inner: [Vec3; 4], interior: [u8; 3]) -> Mesh {
    const FRAME: f32 = 0.05;
    const PROUD: f32 = 0.012;
    const BACK: f32 = 0.016;
    let mut m = Mesh::default();
    let pane = mat([120, 160, 185], 10, 0, 0);
    let outside = mat([58, 60, 66], 120, 150, 0).with_finish(fin("cepillado"));
    let within = mat(interior.map(|c| (u16::from(c) * 3 / 5) as u8), 150, 0, 0).with_finish(fin("pintura"));
    let centre = (outer.iter().copied().sum::<Vec3>() + inner.iter().copied().sum::<Vec3>()) / 8.0;
    for (p, frame, out) in [(outer, outside, 1.0), (inner, within, -1.0)] {
        let mid = p.iter().copied().sum::<Vec3>() / 4.0;
        let mut n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
        if n.dot(mid - centre) < 0.0 {
            n = -n;
        }
        let _ = out;
        // each corner a frame's width in along both its edges
        let w = FRAME.min(0.22 * (p[1] - p[0]).length().min((p[3] - p[0]).length()));
        let inset: [Vec3; 4] = std::array::from_fn(|i| {
            let (a, b) = (p[(i + 1) % 4] - p[i], p[(i + 3) % 4] - p[i]);
            p[i] + a.normalize_or_zero() * w + b.normalize_or_zero() * w
        });
        for i in 0..4 {
            let j = (i + 1) % 4;
            let outward = ((p[i] + p[j]) * 0.5 - mid).normalize_or(Vec3::X);
            // its outer wall, its top, its inner wall down to the pane
            quad_facing(&mut m, [p[i], p[j], p[j] + n * PROUD, p[i] + n * PROUD], outward, frame);
            quad_facing(&mut m, [p[i] + n * PROUD, p[j] + n * PROUD, inset[j] + n * PROUD, inset[i] + n * PROUD], n, frame);
            quad_facing(&mut m, [inset[i] + n * PROUD, inset[j] + n * PROUD, inset[j] - n * BACK, inset[i] - n * BACK], -outward, frame);
        }
        quad_facing(&mut m, [inset[0] - n * BACK, inset[1] - n * BACK, inset[2] - n * BACK, inset[3] - n * BACK], n, pane);
    }
    let edge = mat([88, 90, 94], 170, 120, 0).with_finish(fin("cepillado"));
    let mid = outer.iter().copied().sum::<Vec3>() / 4.0;
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad_facing(&mut m, [outer[i], outer[j], inner[j], inner[i]], (outer[i] + outer[j]) * 0.5 - mid, edge);
    }
    m
}

/// A quad (counter-clockwise from outside) with per-corner normals or its face normal.
pub fn quad(m: &mut Mesh, p: [Vec3; 4], normals: Option<[Vec3; 4]>, material: Material) {
    let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or((p[2] - p[0]).cross(p[3] - p[0]).normalize_or(Vec3::Y));
    let base = m.pos.len() as u32;
    for (k, q) in p.iter().enumerate() {
        let n = normals.map_or(face, |ns| if ns[k].dot(face) > 0.2 { ns[k] } else { face });
        m.vertex(*q, n, material);
    }
    m.quad(base, base + 1, base + 2, base + 3);
}

/// A convex polygon as a fan facing `n`.
pub fn fan(m: &mut Mesh, p: &[Vec3], n: Vec3, material: Material) {
    let base = m.pos.len() as u32;
    for q in p {
        m.vertex(*q, n, material);
    }
    // wind toward n
    let face = (p[1] - p[0]).cross(p[2] - p[0]);
    let flip = face.dot(n) < 0.0;
    for k in 1..p.len() as u32 - 1 {
        if flip {
            m.tri(base, base + k + 1, base + k);
        } else {
            m.tri(base, base + k, base + k + 1);
        }
    }
}

/// Plates of a deck.
///
/// Its plates by the sides reach the hull (inside its skin) at every z they span, whatever the data
/// says: a deck in a tapering nose leaves no gap down its sides, nor goes through them.
pub fn floor(f: &FloorDef, hull: Option<&HullDef>) -> Vec<GenPart> {
    let mut out = Vec::new();
    let [cols, rows] = f.divisiones;
    let (x0, z0) = (f.desde[0].min(f.hasta[0]), f.desde[1].min(f.hasta[1]));
    let (x1, z1) = (f.desde[0].max(f.hasta[0]), f.desde[1].max(f.hasta[1]));
    let (dx, dz) = ((x1 - x0) / cols as f32, (z1 - z0) / rows as f32);
    let color = f.color.unwrap_or([92, 96, 100]);
    for c in 0..cols {
        for r in 0..rows {
            if f.huecos.contains(&[c, r]) {
                continue;
            }
            let lo = Vec3::new(x0 + dx * c as f32, f.y - f.espesor, z0 + dz * r as f32);
            let hi = Vec3::new(x0 + dx * (c + 1) as f32, f.y, z0 + dz * (r + 1) as f32);
            let look = mat(color, 205, 0, panel_code(0.6, false)).with_finish(fin(f.acabado.as_deref().unwrap_or("lagrimado")));
            let id = format!("{}.{c}.{r}", f.id);
            // the side plates out to the hull at both ends
            let span = |z: f32| hull.and_then(|h| span_at(h, f.y - f.espesor * 0.5, z));
            let fit = |z: f32, x_lo: f32, x_hi: f32| -> (f32, f32) {
                match span(z) {
                    Some((l, h)) => (if c == 0 { l + 0.004 } else { x_lo.max(l + 0.004) }, if c + 1 == cols { h - 0.004 } else { x_hi.min(h - 0.004) }),
                    None => (x_lo, x_hi),
                }
            };
            let ((a0, a1), (b0, b1)) = (fit(lo.z, lo.x, hi.x), fit(hi.z, lo.x, hi.x));
            let fitted = (a0 - lo.x).abs() + (a1 - hi.x).abs() + (b0 - lo.x).abs() + (b1 - hi.x).abs() > 1e-3;
            let mut part = if fitted && a1 > a0 && b1 > b0 {
                let pts: Vec<Vec3> = [(a0, lo.z), (a1, lo.z), (b1, hi.z), (b0, hi.z)].iter().flat_map(|&(x, z)| [Vec3::new(x, lo.y, z), Vec3::new(x, hi.y, z)]).collect();
                match GenPart::from_points(id.clone(), &f.material, Role::Floor, &pts) {
                    Some(mut p) => {
                        let mut m = Mesh::default();
                        for face in &p.shape.faces {
                            let n = face.plane.n;
                            fan(&mut m, &face.verts, n, look);
                        }
                        p.look = Some(m);
                        p.color = Some(look.albedo);
                        p
                    }
                    None => boxed(id, &f.material, Role::Floor, lo, hi, look),
                }
            } else {
                boxed(id, &f.material, Role::Floor, lo, hi, look)
            };
            if let Some(cmp) = &f.compartimento {
                part.bounds.push(cmp.clone());
            }
            out.push(part);
        }
    }
    out
}

/// A box part (ship-frame corners) with a look in one material (with chamfer-free faces).
pub fn boxed(id: String, material: &str, role: Role, lo: Vec3, hi: Vec3, look: Material) -> GenPart {
    let c = (lo + hi) * 0.5;
    let h = (hi - lo) * 0.5;
    let shape = Convex::cuboid(h);
    let mut m = Mesh::default();
    for f in &shape.faces {
        let p = [f.verts[0], f.verts[1], f.verts[2], f.verts[3]];
        quad(&mut m, p, None, look);
    }
    GenPart {
        id,
        material: material.to_string(),
        color: Some(look.albedo),
        glow: 0,
        hollow: None,
        shape,
        look: Some(m),
        at: Affine3A::from_translation(c),
        role,
        bounds: Vec::new(),
        model: None,
        flags: 0,
        burst: None,
        surface: None,
        fine: None,
        mirrored: false,
    }
}

/// Clip a convex polygon to the half-plane where `keep(p) ≥ 0` (linear in p).
fn clip(poly: &[Vec2], f: impl Fn(Vec2) -> f32) -> Vec<Vec2> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            out.push(a + (b - a) * (fa / (fa - fb)));
        }
    }
    out
}

/// Where the inside of the hull is at height `y` and `z`: its left and right x.
pub fn span_at(h: &HullDef, y: f32, z: f32) -> Option<(f32, f32)> {
    let o = outline_at(h, z).ok()?;
    let mut xs = Vec::new();
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        if (a.y - y) * (b.y - y) <= 0.0 && (a.y - b.y).abs() > 1e-6 {
            xs.push(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y));
        }
    }
    let lo = xs.iter().copied().fold(f32::MAX, f32::min);
    let hi = xs.iter().copied().fold(f32::MIN, f32::max);
    (xs.len() >= 2 && hi > lo).then_some((lo, hi))
}

/// The outline at `z` (between stations), inside the skin.
pub fn outline_at(h: &HullDef, z: f32) -> Result<Vec<Vec2>, String> {
    let st = &h.estaciones;
    let k = st.windows(2).position(|w| z >= w[0].z.min(w[1].z) && z <= w[0].z.max(w[1].z)).ok_or_else(|| format!("z = {z} fuera del casco"))?;
    let (a, b) = (ring(h, &st[k])?, ring(h, &st[k + 1])?);
    let f = (z - st[k].z) / (st[k + 1].z - st[k].z);
    let r: Vec<Vec2> = a.iter().zip(&b).map(|(p, q)| p.lerp(*q, f)).collect();
    Ok(offset(&r, h.espesor))
}

/// A bulkhead: the inside outline at its z, above `desde_y`, less its doorway, in convex pieces.
pub fn bulkhead(h: &HullDef, b: &BulkheadDef) -> Result<Vec<GenPart>, String> {
    let o = clip(&outline_at(h, b.z)?, |p| p.y - b.desde_y);
    let mut pieces: Vec<(String, Vec<Vec2>)> = Vec::new();
    match b.puerta {
        None => pieces.push((b.id.clone(), o)),
        Some([x0, x1, top]) => {
            pieces.push((format!("{}.izq", b.id), clip(&o, |p| p.x - x1)));
            pieces.push((format!("{}.der", b.id), clip(&o, |p| x0 - p.x)));
            let mid = clip(&clip(&o, |p| x1 - p.x), |p| p.x - x0);
            pieces.push((format!("{}.din", b.id), clip(&mid, |p| p.y - top)));
            // under the floor, below the doorway
            if b.desde_y < 0.0 {
                pieces.push((format!("{}.bajo", b.id), clip(&mid, |p| -p.y)));
            }
        }
    }
    let color = b.color.unwrap_or([170, 168, 160]);
    let t = b.espesor * 0.5;
    let mut out = Vec::new();
    for (id, poly) in pieces {
        if poly.len() < 3 {
            continue;
        }
        let pts: Vec<Vec3> = poly.iter().flat_map(|p| [Vec3::new(p.x, p.y, b.z - t), Vec3::new(p.x, p.y, b.z + t)]).collect();
        let Some(mut part) = GenPart::from_points(id, &b.material, Role::Bulkhead, &pts) else {
            continue;
        };
        let c = Vec3::from(part.at.translation);
        let mut m = Mesh::default();
        let front: Vec<Vec3> = poly.iter().map(|p| Vec3::new(p.x, p.y, b.z + t) - c).collect();
        let back: Vec<Vec3> = poly.iter().map(|p| Vec3::new(p.x, p.y, b.z - t) - c).collect();
        let look = mat(color, 150, 0, panel_code(0.8, false)).with_finish(fin("pintura"));
        fan(&mut m, &front, Vec3::Z, look);
        fan(&mut m, &back, Vec3::NEG_Z, look);
        for i in 0..poly.len() {
            let j = (i + 1) % poly.len();
            quad(&mut m, [back[i], back[j], front[j], front[i]], None, mat([110, 112, 116], 170, 90, 0).with_finish(fin("cepillado")));
        }
        part.look = Some(m);
        if let Some([a, bb]) = &b.entre {
            part.bounds = vec![a.clone(), bb.clone()];
        }
        out.push(part);
    }
    Ok(out)
}

/// What a conduit looks like.
#[derive(Clone, Copy, Debug)]
pub struct ConduitLook {
    /// Round (radius) or a rectangular duct (width, height).
    pub radius: f32,
    pub duct: Option<[f32; 2]>,
    pub color: [u8; 3],
    pub metal: u8,
    pub material: &'static str,
    pub finish: u8,
}

/// The look of a conduit kind by name (data names them per network).
pub fn conduit_look(kind: &str, color: Option<[u8; 3]>) -> ConduitLook {
    let (radius, duct, c, metal, material) = match kind {
        "cable" => (0.012, None, [40, 40, 44], 0, "cableado"),
        "cable_grueso" => (0.022, None, [28, 28, 30], 0, "cableado"),
        "mazo" => (0.03, None, [52, 54, 58], 0, "cableado"),
        "datos" => (0.008, None, [30, 60, 120], 0, "cableado"),
        "tubo" => (0.016, None, [150, 150, 154], 200, "acero"),
        "tubo_alta" => (0.014, None, [170, 120, 60], 220, "acero"),
        "tubo_grueso" => (0.035, None, [160, 160, 166], 200, "acero"),
        "conducto" => (0.0, Some([0.22, 0.14]), [176, 178, 180], 160, "aluminio"),
        "conducto_grande" => (0.0, Some([0.32, 0.18]), [176, 178, 180], 160, "aluminio"),
        _ => (0.015, None, [60, 60, 64], 0, "cableado"),
    };
    let finish = fin(match (duct.is_some(), material) {
        (true, _) => "corrugado",
        (_, "cableado") => "trenzado",
        _ => "cepillado",
    });
    ConduitLook { radius, duct, color: color.unwrap_or(c), metal, material, finish }
}

/// One straight conduit segment from `a` to `b` (ship frame).
pub fn conduit(id: String, a: Vec3, b: Vec3, look: ConduitLook) -> Option<GenPart> {
    let d = b - a;
    let len = d.length();
    if len < 0.02 {
        return None;
    }
    let dir = d / len;
    // part frame: +Y along the conduit, centred
    let rot = Quat::from_rotation_arc(Vec3::Y, dir);
    let at = Affine3A::from_rotation_translation(rot, (a + b) * 0.5);
    let mut m = Mesh::default();
    let material = mat(look.color, if look.metal > 100 { 90 } else { 160 }, look.metal, 0).with_finish(look.finish);
    let shape = if let Some([w, h]) = look.duct {
        let half = Vec3::new(w * 0.5, len * 0.5 + h * 0.25, h * 0.5);
        let shape = Convex::cuboid(half);
        for f in &shape.faces {
            quad(&mut m, [f.verts[0], f.verts[1], f.verts[2], f.verts[3]], None, mat(look.color, 120, look.metal, 0).with_finish(look.finish));
        }
        shape
    } else {
        let r = look.radius;
        // a smooth tube, a little long so runs overlap at their bends
        let seg = 10;
        let hy = len * 0.5 + r;
        for i in 0..seg {
            let a0 = std::f32::consts::TAU * i as f32 / seg as f32;
            let a1 = std::f32::consts::TAU * (i + 1) as f32 / seg as f32;
            let n0 = Vec3::new(a0.cos(), 0.0, a0.sin());
            let n1 = Vec3::new(a1.cos(), 0.0, a1.sin());
            let p = [n0 * r - Vec3::Y * hy, n1 * r - Vec3::Y * hy, n1 * r + Vec3::Y * hy, n0 * r + Vec3::Y * hy];
            quad(&mut m, [p[1], p[0], p[3], p[2]], Some([n1, n0, n0, n1]), material);
        }
        Convex::prism(r, hy, 6, 1.0)
    };
    Some(GenPart {
        id,
        material: look.material.to_string(),
        color: Some(look.color),
        glow: 0,
        hollow: None,
        shape,
        look: Some(m),
        at,
        role: Role::Conduit,
        bounds: Vec::new(),
        model: None,
        flags: 0,
        burst: None,
        surface: None,
        fine: None,
        mirrored: false,
    })
}
