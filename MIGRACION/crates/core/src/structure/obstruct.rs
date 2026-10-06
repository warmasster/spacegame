//! What stands in the way of a moving part: a mechanism — a ramp coming down, a door shutting, a
//! crane's hoist, a gear leg swinging out — is moved by whoever owns its joint, part by part,
//! with nothing of the physics in between; this says how deep those parts would be in the ground
//! and in the other structures where the move takes them, so the owner can stop the joint where
//! it touches instead of going through (`lunar_ship::obstruct`).
//!
//! The move is tried and taken back, as Source does with what it pushes (`Blocked`): nothing is
//! swept. A part is measured at its corners against the ground under them and, against the parts
//! of other structures, by the least overlap of the two along their faces (the same test the
//! contacts between bodies use).
use super::{physics, set::Structures, state::Structure};
use crate::body::BodyRegistry;
use glam::{Affine3A, DVec3, Vec3};

/// What is looked at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct What<'a> {
    /// The ground under the structure, where it is now (if there is any).
    pub ground: bool,
    /// The other structures (what it holds and what holds it left out).
    pub others: bool,
    /// Parts of the structure itself that are in the way like anything else (sorted): what it
    /// carries lashed down (cargo in its clamps). The rest of the structure is not looked at:
    /// a door is in its frame, a mast in its tube, and that is how they are built.
    pub own: &'a [u32],
    /// The ground as a plane already looked at (`ground_plane`: a point of it and its way up):
    /// whoever tries a move many times over looks at the ground once.
    pub plane: Option<(DVec3, DVec3)>,
}

impl What<'_> {
    pub const ALL: What<'static> = What { ground: true, others: true, own: &[], plane: None };
}

/// The ground under the parts `parts` of structure `k` (each moved by `delta`) as a plane: a
/// point of it and its way up; None if they are high over it. Three looks at the ground for
/// the lot: what moves is a few metres across, and the ground is smooth at that size.
pub fn ground_plane(set: &Structures, k: usize, parts: &[u32], delta: Affine3A, bodies: &BodyRegistry) -> Option<(DVec3, DVec3)> {
    let s = &set.list[k];
    let (mut middle, mut n) = (Vec3::ZERO, 0.0f32);
    for &p in parts {
        let part = &s.parts[p as usize];
        if part.alive {
            middle += delta.transform_point3(part.center);
            n += 1.0;
        }
    }
    if n == 0.0 {
        return None;
    }
    middle /= n;
    let mut reach = 0.0f32;
    for &p in parts {
        let part = &s.parts[p as usize];
        if part.alive {
            reach = reach.max(delta.transform_point3(part.center).distance(middle) + part.radius);
        }
    }
    let centre = s.to_world(middle);
    let b = bodies.get(bodies.field(centre).ground?);
    let up = b.up(centre);
    let east = up.any_orthonormal_vector();
    let north = up.cross(east);
    let span = f64::from(reach.clamp(0.5, 6.0)) * 0.6;
    let on = |p: DVec3| b.center + b.up(p) * (b.radius + b.height_at(b.up(p), 0.25));
    let p0 = on(centre + east * span);
    if (centre - p0).dot(up) > f64::from(reach) * 2.0 + 1.0 {
        return None;
    }
    let (p1, p2) = (on(centre - east * span * 0.5 + north * span * 0.87), on(centre - east * span * 0.5 - north * span * 0.87));
    let normal = (p1 - p0).cross(p2 - p0).normalize_or(up);
    Some((p0, if normal.dot(up) < 0.0 { -normal } else { normal }))
}

/// How deep (m; 0: clear) the parts `parts` of structure `k` are in what is round them, each
/// moved by `delta` (in the structure's frame) from where it is now. `spheres`: more that is
/// in the way (a person: centre in the world, radius). `bones`: the bones that move with them:
/// what the structure holds on one of those (a load on a crane's hook) moves too, and must not
/// go into the ground, into anything else, or into the rest of the structure itself (the deck
/// under the load).
pub fn depth(set: &Structures, k: usize, parts: &[u32], bones: &[u16], delta: Affine3A, bodies: &BodyRegistry, what: What, spheres: &[(DVec3, f32)]) -> f32 {
    let s = &set.list[k];
    let mut deepest = own(set, k, parts, delta, bodies, what, spheres);
    if bones.is_empty() || s.loads.is_empty() {
        return deepest;
    }
    let (mut mine, mut theirs) = (physics::Solid::default(), physics::Solid::default());
    let origin = s.to_world(s.center);
    let ground = bodies.field(origin).ground.map(|g| bodies.get(g));
    for (h, o) in set.list.iter().enumerate() {
        let Some(held) = o.held.filter(|h| h.by == s.id && bones.contains(&h.bone)) else { continue };
        let _ = held;
        // the held structure's parts, in the holder's frame, moved with the joint
        let into = Affine3A::from_rotation_translation(s.rot.inverse() * o.rot, s.to_local(o.pos));
        for part in o.parts.iter().filter(|p| p.alive && p.collide) {
            let moved = delta * into * part.local;
            if what.ground
                && let Some(b) = ground
            {
                for v in part.shape.verts() {
                    let w = s.to_world(moved.transform_point3(v));
                    let ground = b.radius + b.height_at(b.up(w), 0.25);
                    deepest = deepest.max((ground - (w - b.center).length()) as f32);
                }
            }
            physics::solid_at(s, &part.shape, moved, origin, &mut mine);
            let c_local = moved.transform_point3(part.local.inverse().transform_point3(part.center));
            // the rest of the holder: what does not move with the joint
            s.index.sphere(c_local, part.radius, |qi| {
                let q = &s.parts[qi as usize];
                if !q.alive || !q.collide || parts.binary_search(&qi).is_ok() || q.center.distance_squared(c_local) > (q.radius + part.radius) * (q.radius + part.radius) {
                    return;
                }
                physics::solid_at(s, &q.shape, q.local, origin, &mut theirs);
                if let Some((_, overlap)) = physics::separating(&theirs, &mine) {
                    deepest = deepest.max(overlap);
                }
            });
            if !what.others {
                continue;
            }
            let centre = s.to_world(c_local);
            for (j, other) in set.list.iter().enumerate() {
                if j == k || j == h || other.held.is_some_and(|x| x.by == s.id) || other.to_world(other.center).distance(centre) > f64::from(other.radius + part.radius) {
                    continue;
                }
                let c = other.to_local(centre);
                other.index.sphere(c, part.radius, |qi| {
                    let q = &other.parts[qi as usize];
                    if !q.alive || !q.collide || q.center.distance_squared(c) > (q.radius + part.radius) * (q.radius + part.radius) {
                        return;
                    }
                    physics::solid_at(other, &q.shape, q.local, origin, &mut theirs);
                    if let Some((_, overlap)) = physics::separating(&theirs, &mine) {
                        deepest = deepest.max(overlap);
                    }
                });
            }
        }
    }
    deepest
}

/// The parts themselves (`depth` without what they carry).
fn own(set: &Structures, k: usize, parts: &[u32], delta: Affine3A, bodies: &BodyRegistry, what: What, spheres: &[(DVec3, f32)]) -> f32 {
    let s = &set.list[k];
    let mut deepest = 0.0f32;
    // the ground under each corner, as a plane (`ground_plane`), and only the parts that reach
    // down to it
    if what.ground
        && let Some((p0, normal)) = what.plane.or_else(|| ground_plane(set, k, parts, delta, bodies))
    {
        for &p in parts {
            let part = &s.parts[p as usize];
            if !part.alive {
                continue;
            }
            let c = s.to_world(delta.transform_point3(part.center));
            if (c - p0).dot(normal) > f64::from(part.radius) {
                continue;
            }
            for v in part.shape.verts() {
                let w = s.to_world(delta.transform_point3(part.local.transform_point3(v)));
                deepest = deepest.max(-(w - p0).dot(normal) as f32);
            }
        }
    }
    if !what.others && spheres.is_empty() && what.own.is_empty() {
        return deepest;
    }
    let origin = s.to_world(s.center);
    let (mut mine, mut theirs) = (physics::Solid::default(), physics::Solid::default());
    for &p in parts {
        let part = &s.parts[p as usize];
        if !part.alive {
            continue;
        }
        let moved = delta * part.local;
        physics::solid_at(s, &part.shape, moved, origin, &mut mine);
        let centre = s.to_world(moved.transform_point3(part.local.inverse().transform_point3(part.center)));
        for &(c, r) in spheres {
            deepest = deepest.max(sphere_depth(&mine, (c - origin).as_vec3(), r));
        }
        // what of its own is in the way (and does not move with these)
        if !what.own.is_empty() {
            let c_local = moved.transform_point3(part.local.inverse().transform_point3(part.center));
            s.index.sphere(c_local, part.radius, |qi| {
                let q = &s.parts[qi as usize];
                if !q.alive || !q.collide || what.own.binary_search(&qi).is_err() || parts.binary_search(&qi).is_ok() || q.center.distance_squared(c_local) > (q.radius + part.radius) * (q.radius + part.radius) {
                    return;
                }
                physics::solid_at(s, &q.shape, q.local, origin, &mut theirs);
                if let Some((_, overlap)) = physics::separating(&theirs, &mine) {
                    deepest = deepest.max(overlap);
                }
            });
        }
        if !what.others {
            continue;
        }
        for (j, o) in set.list.iter().enumerate() {
            // (what it carries goes with it; what carries it, it does not stop for)
            if j == k || o.held.is_some_and(|h| h.by == s.id) || s.held.is_some_and(|h| h.by == o.id) || o.to_world(o.center).distance(centre) > f64::from(o.radius + part.radius) {
                continue;
            }
            let c = o.to_local(centre);
            o.index.sphere(c, part.radius, |qi| {
                let q = &o.parts[qi as usize];
                if !q.alive || !q.collide || q.center.distance_squared(c) > (q.radius + part.radius) * (q.radius + part.radius) {
                    return;
                }
                physics::solid_at(o, &q.shape, q.local, origin, &mut theirs);
                if let Some((_, overlap)) = physics::separating(&theirs, &mine) {
                    deepest = deepest.max(overlap);
                }
            });
        }
    }
    deepest
}

/// How deep a sphere (centre relative to the solid's origin) is in a convex solid: by the face
/// it is least far behind, 0 when it is clear of it.
fn sphere_depth(solid: &physics::Solid, c: Vec3, r: f32) -> f32 {
    let mut least = f32::MAX;
    for &(n, d) in &solid.1 {
        let gap = n.dot(c) - d - r;
        if gap >= 0.0 {
            return 0.0;
        }
        least = least.min(-gap);
    }
    if solid.1.is_empty() { 0.0 } else { least }
}

/// The same for a whole structure standing where it is (nothing moved): how deep its parts
/// `parts` are now.
pub fn depth_now(set: &Structures, k: usize, parts: &[u32], bodies: &BodyRegistry, what: What) -> f32 {
    depth(set, k, parts, &[], Affine3A::IDENTITY, bodies, what, &[])
}

/// Whether anything of `s` other than `parts` matters here (a helper for owners: a structure
/// with nothing alive in `parts` has nothing to stop).
pub fn any_alive(s: &Structure, parts: &[u32]) -> bool {
    parts.iter().any(|&p| s.parts[p as usize].alive)
}
