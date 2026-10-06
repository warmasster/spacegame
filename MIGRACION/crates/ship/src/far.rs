//! How a ship looks from far away (`lunar_core::structure::look::DetailLods`, its last level): its
//! hull lofted through a few of its stations (the nose closing to its tip), and a box for every
//! big part outside the hull (wings, nacelles, legs, fins). A couple of hundred flat triangles;
//! the renderer paints each face with what of the real look lies on it (the black top of a
//! nacelle, the orange stripe, the dark windows), and the colours given here stand in where
//! nothing does.
//!
//! Which parts are inside (`interior`): every part but the pressure boundary (hull, glass, decks,
//! bulkheads) whose centre is inside a compartment or the hull's outline: from outside nothing of
//! it shows, and the looks from afar leave it out first.
use crate::{
    def::HullDef,
    geom::{GenPart, Role},
};
use glam::{Vec2, Vec3};
use lunar_core::mesh::{Material, Mesh};

/// Parts thinner than this (middle extent, m) get no box of their own.
const MIN_BOX: f32 = 0.25;
/// At most this many boxes (the biggest).
const MAX_BOXES: usize = 16;
/// A station is kept when its outline is this much smaller or bigger than the last one kept.
const KEEP_SCALE: f32 = 0.12;

/// Whether `p` (ship frame) is inside the hull's outline at its z.
pub fn in_hull(h: &HullDef, p: Vec3) -> bool {
    let st = &h.estaciones;
    let (z0, z1) = st.iter().fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s.z), b.max(s.z)));
    if p.z < z0 || p.z > z1 {
        return false;
    }
    match crate::geom::outline_at(h, p.z) {
        Ok(o) => inside(&o, Vec2::new(p.x, p.y)),
        Err(_) => false,
    }
}

/// Point in polygon (any winding).
fn inside(o: &[Vec2], p: Vec2) -> bool {
    let mut c = false;
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

/// Whether part `p` is inside: not the pressure boundary, its centre in a compartment's boxes or
/// the hull's outline.
pub fn interior(p: &GenPart, hull: Option<&HullDef>, rooms: &[[Vec3; 2]]) -> bool {
    if matches!(p.role, Role::Hull | Role::Glass | Role::Floor | Role::Bulkhead) {
        return false;
    }
    let c = p.at.transform_point3(p.shape.sphere().0);
    rooms.iter().any(|[lo, hi]| c.cmpge(*lo).all() && c.cmple(*hi).all()) || hull.is_some_and(|h| in_hull(h, c))
}

fn tri(m: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, color: [u8; 3], outward: Vec3) {
    let mut n = (b - a).cross(c - a);
    if n.length_squared() < 1e-12 {
        return;
    }
    let (b, c) = if n.dot(outward) < 0.0 {
        n = -n;
        (c, b)
    } else {
        (b, c)
    };
    let n = n.normalize();
    let mat = Material { albedo: color, rough: 160, metal: 30, emissive: 0, panel: 0, finish: 0 };
    let base = m.pos.len() as u32;
    for p in [a, b, c] {
        m.vertex(p, n, mat);
    }
    m.tri(base, base + 1, base + 2);
}

/// The far shape of a ship of hull `hull` (if it has one) and parts `parts` (`interior` says which
/// are inside), at rest, in the ship frame.
pub fn far_shape(hull: Option<&HullDef>, parts: &[GenPart], interior: &[bool]) -> Mesh {
    let mut m = Mesh::default();
    if let Some(h) = hull {
        loft(h, &mut m);
    }
    // the big parts outside it, biggest first
    let mut boxes: Vec<(f32, usize)> = parts
        .iter()
        .enumerate()
        .filter(|(i, p)| !interior[*i] && !matches!(p.role, Role::Hull | Role::Glass | Role::Floor | Role::Bulkhead | Role::Conduit) || (hull.is_none() && matches!(p.role, Role::Hull)))
        .filter_map(|(i, p)| {
            let size = lunar_core::structure::catalog::middle_extent(&p.shape);
            let c = p.at.transform_point3(p.shape.sphere().0);
            (size >= MIN_BOX && !hull.is_some_and(|h| in_hull(h, c))).then(|| (p.shape.volume(), i))
        })
        .collect();
    boxes.sort_by(|a, b| b.0.total_cmp(&a.0));
    for &(_, i) in boxes.iter().take(MAX_BOXES) {
        let p = &parts[i];
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in p.shape.verts() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let color = p.color.unwrap_or([150, 152, 156]);
        let corner = |k: usize| p.at.transform_point3(Vec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z }));
        let center = p.at.transform_point3((lo + hi) * 0.5);
        for face in [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]] {
            let q = face.map(corner);
            let mid = (q[0] + q[1] + q[2] + q[3]) * 0.25;
            tri(&mut m, q[0], q[1], q[2], color, mid - center);
            tri(&mut m, q[0], q[2], q[3], color, mid - center);
        }
    }
    m
}

/// The hull through a few stations: the ends, and wherever its size changes.
fn loft(h: &HullDef, m: &mut Mesh) {
    let st = &h.estaciones;
    let ring = |i: usize| -> Option<Vec<Vec3>> {
        let s = &st[i];
        let o = h.perfiles.get(&s.perfil)?;
        Some(o.iter().map(|p| Vec3::new(p[0] * s.escala[0], p[1] * s.escala[1] + s.y, s.z)).collect())
    };
    let size = |i: usize| st[i].escala[0].max(st[i].escala[1]);
    let mut keep = vec![0];
    for i in 1..st.len() {
        let last = *keep.last().unwrap_or(&0);
        if i + 1 == st.len() || (size(i) - size(last)).abs() > KEEP_SCALE * size(last).max(0.05) || (i + 1 < st.len() && (size(i + 1) - size(last)).abs() > KEEP_SCALE * 2.0 * size(last).max(0.05)) {
            keep.push(i);
        }
    }
    let rings: Vec<Vec<Vec3>> = keep.iter().filter_map(|&i| ring(i)).collect();
    let color = [216, 216, 210];
    let axis = |r: &[Vec3]| r.iter().copied().sum::<Vec3>() / r.len() as f32;
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let n = a.len().min(b.len());
        let mid = (axis(a) + axis(b)) * 0.5;
        for k in 0..n {
            let (p0, p1, q0, q1) = (a[k], a[(k + 1) % n], b[k], b[(k + 1) % n]);
            let c = (p0 + p1 + q0 + q1) * 0.25;
            let out = Vec3::new(c.x - mid.x, c.y - mid.y, 0.0);
            tri(m, p0, p1, q1, color, out);
            tri(m, p0, q1, q0, color, out);
        }
    }
    // the ends shut
    for (r, dir) in [(rings.first(), -1.0f32), (rings.last(), 1.0)] {
        let Some(r) = r else { continue };
        let c = axis(r);
        for k in 0..r.len() {
            tri(m, c, r[k], r[(k + 1) % r.len()], color, Vec3::Z * dir);
        }
    }
}
