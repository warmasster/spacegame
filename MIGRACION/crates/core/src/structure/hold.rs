//! Holds: a structure held by another — cargo on a clamp, a load under a magnet, a small ship in
//! a cradle. The held one is not a body of its own while it is held: it goes where its holder
//! goes (its place in the holder's frame is fixed), its mass is the holder's to carry (`Load`),
//! and whatever strikes it strikes something that does not give. Let go, it is a free body again
//! with the speed that place of the holder had.
//!
//! Nothing here knows what holds or why: clamps, magnets and cradles are their owners' business
//! (`lunar_ship::cargo`); they find what is loose where they are (`loose_in`), take it (`hold`,
//! `hold_seated`) and drop it (`let_go`).
use super::{set::Structures, state::Structure};
use glam::{DVec3, Mat3, Quat, Vec3};

/// Held by structure `by`, at a fixed place of one of its bones (0: the structure itself; a
/// crane's hook is a bone that moves).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub by: u64,
    pub bone: u16,
    /// The held structure's frame in the bone's: its origin and its turn.
    pub pos: Vec3,
    pub rot: Quat,
}

impl Held {
    /// The held structure's frame in the holder's, as the bone is posed now.
    pub fn place(&self, holder: &Structure) -> (Vec3, Quat) {
        match holder.bones.get(usize::from(self.bone)).filter(|_| self.bone > 0) {
            Some(m) => (m.transform_point3(self.pos), (m.to_scale_rotation_translation().1 * self.rot).normalize()),
            None => (self.pos, self.rot),
        }
    }
}

/// What a structure carries that it holds: a mass at a point of its frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub id: u64,
    pub mass: f32,
    pub at: Vec3,
}

/// Higher than this over the plane it would be set down on (m), a thing stands on something
/// else: it is held where it is.
const STACKED: f32 = 0.2;

/// Mass, first moment and inertia about the origin that loads add (for `Structure::mass_props`).
pub(crate) fn weigh(loads: &[Load]) -> (f32, Vec3) {
    loads.iter().fold((0.0, Vec3::ZERO), |(m, mc), l| (m + l.mass, mc + l.at * l.mass))
}

/// A point mass `m` at `r` from the centre of mass, as inertia.
pub(crate) fn point_inertia(m: f32, r: Vec3) -> Mat3 {
    (Mat3::IDENTITY * r.length_squared() - Mat3::from_cols(r * r.x, r * r.y, r * r.z)) * m
}

impl Structure {
    /// The velocity (world) of the point of it at world `p`.
    pub fn velocity_at(&self, p: DVec3) -> DVec3 {
        self.vel + self.spin.cross((p - self.to_world(self.com)).as_vec3()).as_dvec3()
    }

    /// How the point of it at world `p` speeds up (world, m/s²): as the whole does, and toward
    /// its axis as it turns (how its turning itself changes is left out).
    pub fn acc_at(&self, p: DVec3) -> DVec3 {
        let r = (p - self.to_world(self.com)).as_vec3();
        self.acc + self.spin.cross(self.spin.cross(r)).as_dvec3()
    }

    /// A name for what it is, for whoever looks at it: its own if something owns it (a ship),
    /// else what its heaviest part is called (a crate come off a ship is a crate).
    pub fn label(&self, cat: &super::catalog::Catalog) -> String {
        if self.owner.is_some() {
            return self.name.to_string();
        }
        match self.parts.iter().filter(|p| p.alive && !p.ghost).max_by(|a, b| a.mass.total_cmp(&b.mass)) {
            Some(p) => cat.parts[usize::from(p.kind)].def.name.replace('_', " "),
            None => self.name.to_string(),
        }
    }
}

impl Structures {
    /// Where structure `id` is in the list (ids grow with the list; a scan if ever they do not).
    pub fn index_of(&self, id: u64) -> Option<usize> {
        match self.list.binary_search_by_key(&id, |s| s.id) {
            Ok(k) => Some(k),
            Err(_) => self.list.iter().position(|s| s.id == id),
        }
    }

    /// Whether `a` is held, at any remove, by `b`.
    fn held_by(&self, a: u64, b: u64) -> bool {
        let mut at = a;
        for _ in 0..4 {
            match self.index_of(at).and_then(|k| self.list[k].held) {
                Some(h) if h.by == b => return true,
                Some(h) => at = h.by,
                None => return false,
            }
        }
        false
    }

    /// Structure `id` held by bone `bone` of `by` where it is now. False (and nothing done) if
    /// either is not there, `id` is held or anchored, or `by` is held by `id`.
    pub fn hold(&mut self, id: u64, by: u64, bone: u16) -> bool {
        let (Some(a), Some(b)) = (self.index_of(id), self.index_of(by)) else {
            return false;
        };
        let (s, h) = (&self.list[a], &self.list[b]);
        self.hold_at(id, by, bone, h.to_local(s.pos), (h.rot.inverse() * s.rot).normalize())
    }

    /// Structure `id` held by `by` set down on the plane `y` of the holder's frame: upright (its
    /// axis nearest the holder's up turned onto it) with its lowest point on the plane. `over`:
    /// its middle over that point of the plane too, and its heading squared to the holder's
    /// axes (a cradle); else it stays where it is over the plane (a clamp takes it as it lies).
    pub fn hold_seated(&mut self, id: u64, by: u64, bone: u16, y: f32, over: Option<Vec3>) -> bool {
        let (Some(a), Some(b)) = (self.index_of(id), self.index_of(by)) else {
            return false;
        };
        let (s, h) = (&self.list[a], &self.list[b]);
        let rel = (h.rot.inverse() * s.rot).normalize();
        // what stands on something else over the plane (a crate on crates) is taken as it is
        if over.is_none() {
            let base = h.to_local(s.pos).y;
            let lowest = s.parts.iter().filter(|p| p.alive && !p.ghost).flat_map(|p| p.shape.verts().map(|v| (rel * p.local.transform_point3(v)).y)).fold(f32::MAX, f32::min) + base;
            if lowest > y + STACKED {
                return self.hold(id, by, bone);
            }
        }
        let axis = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z].into_iter().max_by(|p, q| (rel * *p).y.total_cmp(&(rel * *q).y)).unwrap_or(Vec3::Y);
        let mut rot = (Quat::from_rotation_arc((rel * axis).normalize(), Vec3::Y) * rel).normalize();
        if over.is_some() {
            // some axis of it that lies level now, turned to the nearest of the holder's
            let level = rot * axis.any_orthonormal_vector();
            let yaw = level.x.atan2(level.z);
            let square = (yaw / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2;
            rot = (Quat::from_rotation_y(square - yaw) * rot).normalize();
        }
        let low = s.parts.iter().filter(|p| p.alive && !p.ghost).flat_map(|p| p.shape.verts().map(|v| (rot * p.local.transform_point3(v)).y)).fold(f32::MAX, f32::min);
        if low == f32::MAX {
            return false;
        }
        let now = h.to_local(s.to_world(s.center));
        let to = over.unwrap_or(now);
        let middle = rot * s.center;
        self.hold_at(id, by, bone, Vec3::new(to.x - middle.x, y - low, to.z - middle.z), rot)
    }

    /// Structure `id` held by `by` hanging under the plane `y` of the holder's frame: turned as it
    /// is, raised (or lowered) until its highest point touches the plane — a magnet takes its
    /// load to its face.
    pub fn hold_under(&mut self, id: u64, by: u64, bone: u16, y: f32) -> bool {
        let (Some(a), Some(b)) = (self.index_of(id), self.index_of(by)) else {
            return false;
        };
        let (s, h) = (&self.list[a], &self.list[b]);
        let rel = (h.rot.inverse() * s.rot).normalize();
        let at = h.to_local(s.pos);
        let high = s.parts.iter().filter(|p| p.alive && !p.ghost).flat_map(|p| p.shape.verts().map(|v| (rel * p.local.transform_point3(v)).y)).fold(f32::MIN, f32::max);
        if high == f32::MIN {
            return false;
        }
        self.hold_at(id, by, bone, Vec3::new(at.x, y - high, at.z), rel)
    }

    /// The highest point of structure `id` as it lies now, in the frame of `by`: its y there (what
    /// a magnet's face comes down on).
    pub fn top_in(&self, id: u64, by: u64) -> Option<f32> {
        let (s, h) = (&self.list[self.index_of(id)?], &self.list[self.index_of(by)?]);
        let rel = (h.rot.inverse() * s.rot).normalize();
        let high = s.parts.iter().filter(|p| p.alive && !p.ghost).flat_map(|p| p.shape.verts().map(|v| (rel * p.local.transform_point3(v)).y)).fold(f32::MIN, f32::max);
        (high > f32::MIN).then(|| h.to_local(s.pos).y + high)
    }

    /// Structure `id` held by bone `bone` of `by`, with its frame at `pos`, turned `rot`, in the
    /// holder's frame as it is posed now.
    pub fn hold_at(&mut self, id: u64, by: u64, bone: u16, pos: Vec3, rot: Quat) -> bool {
        let (Some(a), Some(b)) = (self.index_of(id), self.index_of(by)) else {
            return false;
        };
        if a == b || self.list[a].held.is_some() || self.list[a].anchored || self.held_by(by, id) {
            return false;
        }
        // in the bone's frame, so that it goes with it
        let held = match self.list[b].bones.get(usize::from(bone)).filter(|_| bone > 0) {
            Some(m) => {
                let inv = m.inverse();
                Held { by, bone, pos: inv.transform_point3(pos), rot: (inv.to_scale_rotation_translation().1 * rot).normalize() }
            }
            None => Held { by, bone: 0, pos, rot },
        };
        let s = &mut self.list[a];
        s.held = Some(held);
        (s.force, s.torque) = (Vec3::ZERO, Vec3::ZERO);
        let load = Load { id, mass: s.mass, at: pos + rot * s.com };
        let h = &mut self.list[b];
        h.loads.push(load);
        h.resting = false;
        h.still = 0.0;
        h.refresh();
        self.follow();
        true
    }

    /// Structure `id` held exactly as `held` says (its frame in the bone's of what holds it),
    /// whatever held it before; none: let go. What another game says of it (a server: every game
    /// has what is held held by the same, at the same place). False if it could not be.
    pub fn hold_as(&mut self, id: u64, held: Option<Held>) -> bool {
        let Some(a) = self.index_of(id) else { return false };
        if self.list[a].held == held {
            return true;
        }
        if self.list[a].held.is_some() {
            self.let_go(id);
        }
        let Some(h) = held else { return true };
        let Some(b) = self.index_of(h.by).filter(|&b| b != a) else { return false };
        let (pos, rot) = h.place(&self.list[b]);
        let s = &mut self.list[a];
        s.held = Some(h);
        (s.force, s.torque) = (Vec3::ZERO, Vec3::ZERO);
        let load = Load { id, mass: s.mass, at: pos + rot * s.com };
        let holder = &mut self.list[b];
        holder.loads.retain(|l| l.id != id);
        holder.loads.push(load);
        holder.resting = false;
        holder.still = 0.0;
        holder.refresh();
        self.follow();
        true
    }

    /// Structure `id` let go by whatever holds it: free, with the speed its place had. False if
    /// nothing held it.
    pub fn let_go(&mut self, id: u64) -> bool {
        let Some(a) = self.index_of(id) else {
            return false;
        };
        let Some(held) = self.list[a].held.take() else {
            return false;
        };
        let at = self.list[a].to_world(self.list[a].com);
        let (mut vel, mut spin) = (self.list[a].vel, self.list[a].spin);
        if let Some(b) = self.index_of(held.by) {
            let h = &mut self.list[b];
            (vel, spin) = (h.velocity_at(at), h.spin);
            h.loads.retain(|l| l.id != id);
            h.resting = false;
            h.still = 0.0;
            h.refresh();
        }
        let s = &mut self.list[a];
        (s.vel, s.spin) = (vel, spin);
        s.resting = false;
        s.still = 0.0;
        s.clock = f64::NAN;
        true
    }

    /// The loose structures (not anchored, not held, no heavier than `max`, not `by` nor what
    /// holds it) whose middle is in a box of `by`'s frame (its centre and half extents), the
    /// nearest to its centre first.
    pub fn loose_in(&self, by: u64, centre: Vec3, half: Vec3, max: f32, out: &mut Vec<u64>) {
        out.clear();
        let Some(b) = self.index_of(by) else { return };
        let h = &self.list[b];
        let reach = f64::from(half.length()) + 1.0;
        let mid = h.to_world(centre);
        let mut found: Vec<(f32, u64)> = Vec::new();
        for s in &self.list {
            if s.id == by || s.anchored || s.held.is_some() || s.mass > max || s.mass <= 0.0 || s.to_world(s.center).distance(mid) > reach + f64::from(s.radius) {
                continue;
            }
            let c = h.to_local(s.to_world(s.center));
            if (c - centre).abs().cmple(half).all() && !self.held_by(by, s.id) {
                found.push((c.distance(centre), s.id));
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.extend(found.into_iter().map(|f| f.1));
    }

    /// Held structures go where their holders went; what lost its holder is free; loads that are
    /// gone weigh no more. Called after the bodies moved.
    pub fn follow(&mut self) {
        if !self.any_held && !self.list.iter().any(|s| s.held.is_some() || !s.loads.is_empty()) {
            return;
        }
        self.any_held = false;
        // twice: what is held may hold something itself
        for pass in 0..2 {
            for k in 0..self.list.len() {
                let Some(held) = self.list[k].held else {
                    continue;
                };
                self.any_held = true;
                match self.index_of(held.by) {
                    Some(b) => {
                        let h = &self.list[b];
                        let (at, turn) = held.place(h);
                        let (pos, rot) = (h.to_world(at), (h.rot * turn).normalize());
                        let (spin, resting) = (h.spin, h.resting);
                        let s = &mut self.list[k];
                        (s.pos, s.rot) = (pos, rot);
                        let at = self.list[k].to_world(self.list[k].com);
                        let (vel, acc) = (self.list[b].velocity_at(at), self.list[b].acc_at(at));
                        let s = &mut self.list[k];
                        (s.vel, s.spin, s.resting, s.acc) = (vel, spin, resting, acc);
                    }
                    None if pass == 0 => {
                        let s = &mut self.list[k];
                        s.held = None;
                        s.resting = false;
                        s.clock = f64::NAN;
                    }
                    None => {}
                }
            }
        }
        // a load that is gone, or held by another now, is no longer carried
        for k in 0..self.list.len() {
            if self.list[k].loads.is_empty() {
                continue;
            }
            let me = self.list[k].id;
            let mut loads = std::mem::take(&mut self.list[k].loads);
            let before = loads.len();
            loads.retain(|l| self.index_of(l.id).is_some_and(|j| self.list[j].held.is_some_and(|h| h.by == me)));
            // (and what it weighs now if it lost parts, and where if its bone moved it far)
            let mut changed = loads.len() != before;
            for l in &mut loads {
                let Some(j) = self.index_of(l.id) else {
                    continue;
                };
                let (s, Some(h)) = (&self.list[j], self.list[j].held) else {
                    continue;
                };
                let (at, turn) = h.place(&self.list[k]);
                let now = Load { id: l.id, mass: s.mass, at: at + turn * s.com };
                if (now.mass - l.mass).abs() > 0.5 || now.at.distance(l.at) > 0.2 {
                    changed = true;
                    *l = now;
                }
            }
            self.list[k].loads = loads;
            if changed {
                self.list[k].refresh();
            }
        }
    }
}
