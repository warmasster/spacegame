//! The structures in the world: built from blueprints, set on the ground of a body, found by id
//! and by ray. Damage, fracture and motion work on this list.
use super::{
    Library,
    graph::Groups,
    networks::{Machines, Solver},
    state::{RayHit, Structure},
};
use crate::{
    body::{BodyId, BodyRegistry},
    scene::basis,
};
use glam::{DVec3, Vec3};
use std::sync::Arc;

/// A sphere against a structure part (`Structures::sphere_contacts`).
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub structure: usize,
    pub part: u32,
    /// World, out of the part.
    pub normal: DVec3,
    pub depth: f32,
}

pub struct Structures {
    pub list: Vec<Structure>,
    pub lib: Arc<Library>,
    next: u64,
    pub(crate) groups: Groups,
    /// Simulation time (s), as of the last `simulate`.
    pub now: f64,
    /// Toward the sun (world), for machines that care.
    pub sun: DVec3,
    pub(crate) machines: Machines,
    pub(crate) solver: Solver,
    pub(crate) physics: super::physics::Physics,
    /// Scratch of the scheduler: structures to step together, and alone with their own time.
    pub(crate) due: (Vec<usize>, Vec<(usize, f32)>),
    /// (and where the world is watched from this step: the watchers given, and whatever lives
    /// among the structures)
    pub(crate) watch: Vec<DVec3>,
    /// Something is held (`hold`): held structures follow their holders after every step.
    pub(crate) any_held: bool,
    /// Hits decided here on what is simulated elsewhere or done in one order by every game
    /// (`Structure::remote`, `Structure::shared`), each in its structure's own frame: not done
    /// here, kept for whoever tells them (`multi` takes them from here and every game, this one
    /// too, does them when they come back). Nothing is ever put in it with no such structure.
    pub told: Vec<(u64, super::damage::Hit)>,
    /// Where each structure was put back to while it is drawn between steps (`present`): its id,
    /// where it really is and its turn, for `restore`.
    shown: Vec<(u64, DVec3, glam::Quat)>,
}

impl Structures {
    pub fn new(lib: Arc<Library>) -> Structures {
        // checked when the library loaded
        let machines = Machines::new(&lib.catalog).unwrap_or_else(|e| panic!("{e}"));
        Structures { list: Vec::new(), lib, next: 1, groups: Groups::default(), now: 0.0, sun: DVec3::Y, machines, solver: Solver::default(), physics: Default::default(), due: Default::default(), watch: Vec::new(), any_held: false, told: Vec::new(), shown: Vec::new() }
    }

    /// A step begins: where each structure is now is where it was, for whoever draws between
    /// this step and the next (`present`).
    pub fn begin_step(&mut self) {
        for s in &mut self.list {
            s.before = Some((s.pos, s.rot));
        }
    }

    /// Every structure where it was a share `alpha` (0..1) of the way through the last step,
    /// between where it was as it began and where it is: what is drawn between steps, all of the
    /// same instant. Nothing is simulated until `restore` puts them back.
    pub fn present(&mut self, alpha: f64) {
        self.shown.clear();
        for s in &mut self.list {
            self.shown.push((s.id, s.pos, s.rot));
            if let Some((pos, rot)) = s.before {
                s.pos = pos.lerp(s.pos, alpha);
                let (a, b) = (rot.as_dquat(), s.rot.as_dquat());
                let b = if a.dot(b) < 0.0 { -b } else { b };
                s.rot = a.slerp(b, alpha).normalize().as_quat();
            }
        }
    }

    /// Where structure `id` really is while it is drawn between steps (`present`): its place and
    /// turn at the step. None outside `present`, or if there is no such structure.
    pub fn true_pose(&self, id: u64) -> Option<(DVec3, glam::Quat)> {
        self.shown.iter().find(|s| s.0 == id).map(|s| (s.1, s.2))
    }

    /// Every structure back where it is (`present` undone).
    pub fn restore(&mut self) {
        for (k, &(id, pos, rot)) in self.shown.iter().enumerate() {
            let s = match self.list.get_mut(k) {
                Some(s) if s.id == id => s,
                _ => match self.list.iter_mut().find(|s| s.id == id) {
                    Some(s) => s,
                    None => continue,
                },
            };
            (s.pos, s.rot) = (pos, rot);
        }
        self.shown.clear();
    }

    /// The id the next structure made will have (none is taken).
    pub fn next_free(&self) -> u64 {
        self.next
    }

    pub fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next - 1
    }

    /// The library grew (part kinds appended: a ship rebuilt by the editor): what is kept per part
    /// kind follows it.
    pub fn library_changed(&mut self) -> Result<(), String> {
        self.machines = Machines::new(&self.lib.catalog)?;
        Ok(())
    }

    /// Structure `id` gone (a ship rebuilt by the editor, say).
    pub fn remove(&mut self, id: u64) {
        self.list.retain(|s| s.id != id);
    }

    pub fn get(&self, id: u64) -> Option<&Structure> {
        self.list.iter().find(|s| s.id == id)
    }

    /// Blueprint `id` standing on the ground of `body` at unit direction `dir`, turned `yaw` (rad,
    /// from north toward east) round the vertical, `lift` m over the lowest ground under it.
    pub fn place(&mut self, id: &str, bodies: &BodyRegistry, body: BodyId, dir: DVec3, yaw: f64, lift: f64) -> Result<u64, String> {
        let lib = self.lib.clone();
        let bp = lib.blueprint(id).ok_or_else(|| format!("unknown structure '{id}'"))?;
        let b = bodies.get(body);
        let up = dir.normalize();
        let north = b.turn_from(up);
        let east = north.cross(up);
        let fwd = north * yaw.cos() + east * yaw.sin();
        let rot = basis(up, fwd);
        let new = self.next_id();
        let mut s = Structure::new(new, bp, &lib.catalog, DVec3::ZERO, rot);
        // rest the lowest point on the lowest ground under the footprint
        let (lo, hi) = s.parts.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(p.center - p.radius), hi.max(p.center + p.radius)));
        let bottom = s.parts.iter().flat_map(|p| p.shape.verts().map(|v| p.local.transform_point3(v).y)).fold(f32::MAX, f32::min);
        let mut ground = f64::MAX;
        for (x, z) in [(0.0, 0.0), (lo.x, lo.z), (lo.x, hi.z), (hi.x, lo.z), (hi.x, hi.z)] {
            let at = (b.center + up * b.radius + (rot * Vec3::new(x, 0.0, z)).as_dvec3() - b.center).normalize();
            ground = ground.min(b.height(at));
        }
        s.pos = b.center + up * (b.radius + ground + lift - f64::from(bottom));
        // what stands on the ground holds an anchored structure there
        for p in &mut s.parts {
            p.anchor = p.shape.verts().any(|v| p.local.transform_point3(v).y < bottom + 0.05);
        }
        s.resting = s.anchored;
        self.list.push(s);
        Ok(new)
    }

    /// Set structure `id` down on the ground under it as it stands now (its joints posed): turned
    /// to the slope under its lowest corners (at most `MAX_LEAN`), then raised so the corner over
    /// the highest ground touches. Nothing starts buried, nothing drops far. Where there is no
    /// ground under it (past every body's reach) it stays as it is.
    pub fn rest_on_ground(&mut self, id: u64, bodies: &BodyRegistry) {
        const MAX_LEAN: f64 = 0.35;
        let Some(s) = self.list.iter_mut().find(|s| s.id == id) else {
            return;
        };
        let Some(ground) = bodies.field(s.to_world(s.center)).ground else {
            return;
        };
        let b = bodies.get(ground);
        let ground = |p: DVec3| {
            let dir = (p - b.center).normalize();
            b.center + dir * (b.radius + b.height(dir))
        };
        let (mut low, mut pts) = (Vec::new(), Vec::new());
        let up = b.up(s.to_world(s.center));
        super::physics::low_corners(s, up, &mut low, &mut pts);
        // the slope: the plane through the ground under the corners that span the most
        let feet: Vec<DVec3> = pts.iter().filter(|p| (**p - pts[0]).dot(up) < 0.6).map(|&p| ground(p)).collect();
        let mut best = (0.0, up);
        for i in 0..feet.len() {
            for j in i + 1..feet.len() {
                for k in j + 1..feet.len() {
                    let n = (feet[j] - feet[i]).cross(feet[k] - feet[i]);
                    if n.length() > best.0 {
                        best = (n.length(), if n.dot(up) < 0.0 { -n.normalize() } else { n.normalize() });
                    }
                }
            }
        }
        let slope = best.1;
        let lean = slope.angle_between(up);
        let target = if lean > MAX_LEAN { up.lerp(slope, MAX_LEAN / lean).normalize() } else { slope };
        let now = (s.rot * Vec3::Y).as_dvec3();
        let turn = glam::DQuat::from_rotation_arc(now, target);
        let pivot = s.to_world(s.center);
        s.rot = (turn.as_quat() * s.rot).normalize();
        s.pos = pivot + turn * (s.pos - pivot);
        // raised (or lowered) until the most buried corner just touches: of its lowest ones, and
        // of the ones nearest the slope (leaning less than it, its uphill side is the one in it)
        super::physics::low_corners(s, up, &mut low, &mut pts);
        let mut lift = pts.iter().map(|&p| (ground(p) - p).dot(up)).fold(f64::MIN, f64::max);
        super::physics::low_corners(s, slope, &mut low, &mut pts);
        lift = pts.iter().map(|&p| (ground(p) - p).dot(up)).fold(lift, f64::max);
        if lift.is_finite() {
            s.pos += up * lift;
        }
        s.vel = DVec3::ZERO;
        s.spin = Vec3::ZERO;
    }

    /// Blueprint `id` free at `pos` turned `rot` (nothing holds it: it falls, or floats in space).
    /// What pulls it and what ground it may meet is whatever holds where it is, at every step.
    pub fn spawn(&mut self, id: &str, pos: DVec3, rot: glam::Quat) -> Result<u64, String> {
        let lib = self.lib.clone();
        let bp = lib.blueprint(id).ok_or_else(|| format!("unknown structure '{id}'"))?;
        let new = self.next_id();
        let mut s = Structure::new(new, bp, &lib.catalog, pos, rot);
        s.anchored = false;
        s.resting = false;
        for p in &mut s.parts {
            p.anchor = false;
        }
        self.list.push(s);
        Ok(new)
    }

    /// The structure whose rooms world point `p` is in, if any (`Structure::rooms`).
    pub fn rooms_at(&self, p: DVec3) -> Option<u64> {
        self.list.iter().find(|s| !s.rooms.is_empty() && s.to_world(s.center).distance_squared(p) < f64::from(s.radius * s.radius) && s.in_rooms(s.to_local(p))).map(|s| s.id)
    }

    /// Parts of every structure a world sphere overlaps (bodies walking among them): structure
    /// index, part, world normal out of the part, depth (m). Parts with a bounding radius under
    /// `min` m are left out.
    pub fn sphere_contacts(&self, c: DVec3, r: f32, min: f32, out: &mut Vec<Contact>) {
        let mut scratch = Vec::new();
        for (i, s) in self.list.iter().enumerate() {
            if s.to_world(s.center).distance(c) > f64::from(s.radius + r) {
                continue;
            }
            scratch.clear();
            s.sphere_contacts(s.to_local(c), r, min, &mut scratch);
            for &(part, n, depth) in &scratch {
                out.push(Contact { structure: i, part, normal: (s.rot * n).as_dvec3(), depth });
            }
        }
    }

    /// What a body would stand on or bump into along a world ray within `max` m (the parts
    /// `sphere_contacts` takes): structure index, how far (m), the world normal there.
    pub fn raycast_solid(&self, from: DVec3, dir: DVec3, max: f64, min: f32) -> Option<(usize, f64, DVec3)> {
        let mut best: Option<(usize, f64, DVec3)> = None;
        for (i, s) in self.list.iter().enumerate() {
            let c = s.to_world(s.center);
            let along = (c - from).dot(dir);
            let off = (c - from - dir * along).length();
            if off > f64::from(s.radius) || along < -f64::from(s.radius) || along > max + f64::from(s.radius) {
                continue;
            }
            let limit = best.map_or(max, |b| b.1);
            if let Some(h) = s.raycast_solid(s.to_local(from), s.dir_to_local(dir), limit as f32, min) {
                best = Some((i, f64::from(h.t), (s.rot * h.normal).as_dvec3()));
            }
        }
        best
    }

    /// First structure part along a world ray within `max` m: structure index, hit, world point.
    pub fn raycast(&self, from: DVec3, dir: DVec3, max: f64) -> Option<(usize, RayHit, DVec3)> {
        self.raycast_in(0..self.list.len(), from, dir, max)
    }

    /// The same among the structures `idx` only (from a broadphase).
    pub fn raycast_among(&self, idx: &[u32], from: DVec3, dir: DVec3, max: f64) -> Option<(usize, RayHit, DVec3)> {
        self.raycast_in(idx.iter().map(|&i| i as usize), from, dir, max)
    }

    fn raycast_in(&self, idx: impl Iterator<Item = usize>, from: DVec3, dir: DVec3, max: f64) -> Option<(usize, RayHit, DVec3)> {
        let mut best: Option<(usize, RayHit, DVec3)> = None;
        for i in idx {
            let s = &self.list[i];
            let c = s.to_world(s.center);
            let along = (c - from).dot(dir);
            let off = (c - from - dir * along).length();
            if off > f64::from(s.radius) || along < -f64::from(s.radius) || along > max + f64::from(s.radius) {
                continue;
            }
            let limit = best.as_ref().map_or(max, |b| from.distance(b.2));
            if let Some(h) = s.raycast(s.to_local(from), s.dir_to_local(dir), limit as f32) {
                let p = from + dir * f64::from(h.t);
                best = Some((i, h, p));
            }
        }
        best
    }
}
