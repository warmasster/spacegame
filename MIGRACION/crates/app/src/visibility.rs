//! What structures can be seen from the eye, by rays: one is hidden when every ray to it (its
//! centre, top, bottom and sides) passes under the ground, or, from inside a ship, through its
//! hull (not its windows, nor an open door). Hidden ones are not drawn (only their shadows), get
//! no props and light nothing inside. A few are checked each frame within a time budget, the
//! nearest first; a structure is hidden after two checks in a row see nothing of it, shown again
//! at the first that sees it, and anything near is always shown.
use glam::DVec3;
use lunar_core::{
    body::BodyRegistry,
    structure::{catalog::Catalog, set::Structures, state::Structure},
};
use std::{collections::HashMap, time::Instant};

/// Time per frame the checks may take (ms), and how often each structure is checked (s).
const BUDGET_MS: f64 = 0.25;
const EVERY: f64 = 0.2;
/// Nearer than this (m, from its bounding sphere) a structure is always shown.
const NEAR: f64 = 12.0;
/// Rays this far below the ground (m) are blocked by it.
const UNDER: f64 = 0.4;

#[derive(Clone, Copy, Default)]
struct Seen {
    hidden: bool,
    misses: u8,
    /// When it is checked again (s of play).
    next: f64,
}

#[derive(Default)]
pub struct Visibility {
    seen: HashMap<u64, Seen>,
    cursor: usize,
    /// Hidden this frame (ids).
    pub hidden: Vec<u64>,
    /// Checks made last frame, and how long they took (ms).
    pub checks: usize,
    pub ms: f64,
}

/// Whether the ground hides `p` from `eye` (samples along the way, the ends spared).
fn under_ground(bodies: &BodyRegistry, eye: DVec3, p: DVec3) -> bool {
    let len = eye.distance(p);
    let n = (len / 8.0).clamp(6.0, 40.0) as usize;
    let b = bodies.get(bodies.dominant(eye));
    (1..n).any(|i| {
        let t = i as f64 / n as f64;
        (0.02..0.97).contains(&t) && b.altitude(eye.lerp(p, t)) < -UNDER
    })
}

/// Whether an opaque part of `s` (the hull the eye is in) lies between `eye` and `p`.
fn through_hull(s: &Structure, cat: &Catalog, eye: DVec3, p: DVec3) -> bool {
    let (o, q) = (s.to_local(eye), s.to_local(p));
    let len = o.distance(q);
    if len < 1e-3 {
        return false;
    }
    let dir = (q - o) / len;
    // only within the structure: beyond its sphere nothing of it is in the way
    let max = len.min(s.center.distance(o) + s.radius);
    for part in s.parts.iter().filter(|x| x.alive && !cat.material(x.kind).glass) {
        let oc = o - part.center;
        let b = oc.dot(dir);
        let c = oc.length_squared() - part.radius * part.radius;
        if c > 0.0 && (b > 0.0 || b * b < c) {
            continue;
        }
        let inv = part.local.inverse();
        if part.shape.raycast(inv.transform_point3(o), inv.transform_vector3(dir), max).is_some() {
            return true;
        }
    }
    false
}

impl Visibility {
    /// Check what the budget allows. `inside`: the structure the eye is inside of (a closed
    /// hull round it), if any.
    pub fn update(&mut self, bodies: &BodyRegistry, set: &Structures, cat: &Catalog, eye: DVec3, inside: Option<u64>, now: f64) {
        let t0 = Instant::now();
        self.seen.retain(|id, _| set.list.iter().any(|s| s.id == *id));
        self.checks = 0;
        let n = set.list.len();
        let shell = inside.and_then(|id| set.get(id));
        for step in 0..n {
            if t0.elapsed().as_secs_f64() * 1e3 > BUDGET_MS && step > 0 {
                break;
            }
            let s = &set.list[(self.cursor + step) % n];
            let c = s.to_world(s.center);
            let r = f64::from(s.radius);
            let seen = self.seen.entry(s.id).or_default();
            if Some(s.id) == inside || c.distance(eye) - r < NEAR {
                *seen = Seen::default();
                continue;
            }
            if now < seen.next {
                continue;
            }
            seen.next = now + EVERY * (1.0 + (s.id % 7) as f64 * 0.05);
            self.checks += 1;
            let up = bodies.get(bodies.dominant(c)).up(c);
            let to = (c - eye).normalize_or(DVec3::X);
            let side = to.cross(up).normalize_or(DVec3::X);
            let k = r * 0.7;
            let points = [c, c + up * k, c - up * k * 0.5, c + side * k, c - side * k];
            let visible = points.iter().any(|&p| {
                let b = bodies.get(bodies.dominant(p));
                b.altitude(p) > -UNDER && !under_ground(bodies, eye, p) && !shell.is_some_and(|h| through_hull(h, cat, eye, p))
            });
            if visible {
                *seen = Seen { next: seen.next, ..Seen::default() };
            } else {
                seen.misses = seen.misses.saturating_add(1);
                seen.hidden = seen.misses >= 2;
            }
        }
        if n > 0 {
            self.cursor = (self.cursor + self.checks.max(1)) % n;
        }
        self.hidden.clear();
        self.hidden.extend(self.seen.iter().filter(|(_, s)| s.hidden).map(|(id, _)| *id));
        self.ms = t0.elapsed().as_secs_f64() * 1e3;
    }

    /// Whether structure `id` is hidden.
    pub fn is_hidden(&self, id: u64) -> bool {
        self.seen.get(&id).is_some_and(|s| s.hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;
    use lunar_core::structure::Library;

    fn alcotan() -> (Library, Structure) {
        let defs = crate::root().join("assets/defs");
        let mut lib = Library::load(&defs.join("structures")).unwrap();
        let (ships, bps) = lunar_ship::ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        lib.blueprints.extend(bps);
        let kind = ships.get("alcotan").unwrap().clone();
        let bp = lib.blueprint(&kind.blueprint).unwrap();
        let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
        (lib, s)
    }

    #[test]
    fn the_hull_hides_what_is_outside_but_not_through_a_window() {
        let (lib, s) = alcotan();
        let eye = DVec3::new(0.0, 1.6, 0.5);
        // straight out through the cabin's side under its windows, and its roof: hull
        let low = DVec3::new(0.0, 0.5, 0.5);
        assert!(through_hull(&s, &lib.catalog, low, DVec3::new(40.0, 0.5, 0.5)));
        assert!(through_hull(&s, &lib.catalog, eye, DVec3::new(0.0, 40.0, 0.5)));
        // out through the cabin's side window, and some window of the bridge: seen
        assert!(!through_hull(&s, &lib.catalog, eye, DVec3::new(40.0, 1.6, 0.5)));
        let eye = DVec3::new(0.0, 1.5, 3.6);
        let through_glass = s.parts.iter().filter(|p| lib.catalog.material(p.kind).glass).any(|p| {
            let c = p.center.as_dvec3();
            !through_hull(&s, &lib.catalog, eye, eye + (c - eye) * 30.0)
        });
        assert!(through_glass, "some window of the bridge shows the outside");
    }
}
