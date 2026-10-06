use super::{bvh::Bvh, set::Structures, state::Structure};
use glam::{DQuat, DVec3, Vec3};

const BOUNDS_ROUNDING: f64 = 8.0 * f32::EPSILON as f64;
const SEGMENT_EPSILON: f64 = 1e-10;

#[derive(Clone, Copy, Debug, Default)]
pub struct Motion {
    pub at: DVec3,
    pub vel: DVec3,
    pub spin: DVec3,
}

impl Motion {
    pub fn velocity_at(self, point: DVec3) -> DVec3 {
        self.vel + self.spin.cross(point - self.at)
    }
}

#[derive(Clone, Copy)]
struct Pose {
    at: DVec3,
    rot: DQuat,
}

impl Pose {
    fn of(structure: &Structure) -> Self {
        Self { at: structure.pos, rot: structure.rot.as_dquat().normalize() }
    }

    fn local(self, point: DVec3) -> DVec3 {
        self.rot.conjugate() * (point - self.at)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceHit {
    pub id: u64,
    pub point: Vec3,
    pub dir: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct Crossing {
    pub surface: SurfaceHit,
    pub fraction: f64,
    pub at: DVec3,
}

#[derive(Default)]
pub struct Sweep {
    before: Vec<Pose>,
    tree: Bvh,
    origin: DVec3,
}

impl Sweep {
    pub fn begin(&mut self, set: &Structures) {
        self.before.clear();
        self.before.extend(set.list.iter().map(Pose::of));
    }

    pub fn end(&mut self, set: &Structures) {
        self.origin = set.list.first().map_or(DVec3::ZERO, |structure| structure.pos);
        let origin = self.origin;
        self.tree.build(set.list.iter().enumerate().map(|(index, structure)| {
            let radius = f64::from(structure.radius + structure.center.length());
            let start = self.before[index].at;
            let center = (start - origin) + (structure.pos - start) * 0.5;
            let radius = radius + start.distance(structure.pos) * 0.5;
            (index as u32, center.as_vec3(), (radius + BOUNDS_ROUNDING * (center.length() + radius)) as f32)
        }));
    }

    pub fn forecast(&mut self, set: &Structures, dt: f64) {
        self.origin = set.list.first().map_or(DVec3::ZERO, |structure| structure.pos);
        let origin = self.origin;
        self.tree.build(set.list.iter().enumerate().map(|(index, structure)| {
            let travel = structure.vel * dt;
            let center = structure.to_world(structure.com) - origin + travel * 0.5;
            let radius = f64::from(structure.radius + (structure.center - structure.com).length()) + travel.length() * 0.5;
            (index as u32, center.as_vec3(), (radius + BOUNDS_ROUNDING * (center.length() + radius)) as f32)
        }));
    }

    pub fn candidates(&self, from: DVec3, to: DVec3, mut visit: impl FnMut(usize)) {
        let travel = to - from;
        let length = travel.length();
        if length <= SEGMENT_EPSILON {
            self.tree.sphere((from - self.origin).as_vec3(), SEGMENT_EPSILON as f32, |index| visit(index as usize));
        } else {
            self.tree.ray((from - self.origin).as_vec3(), (travel / length).as_vec3(), length as f32, |index, reach| {
                visit(index as usize);
                reach
            });
        }
    }

    pub fn hit(&self, set: &Structures, from: DVec3, to: DVec3) -> Option<Crossing> {
        let mut best: Option<Crossing> = None;
        self.candidates(from, to, |index| {
            let structure = &set.list[index];
            let start = self.before[index].local(from);
            let finish = Pose::of(structure).local(to);
            let travel = finish - start;
            let length = travel.length();
            if length <= SEGMENT_EPSILON {
                return;
            }
            let dir = (travel / length).as_vec3();
            let reach = length * best.map_or(1.0, |hit| hit.fraction);
            if let Some(hit) = structure.raycast(start.as_vec3(), dir, reach as f32) {
                let fraction = (f64::from(hit.t) / length).clamp(0.0, 1.0);
                let point = (start + travel * fraction).as_vec3();
                best = Some(Crossing { surface: SurfaceHit { id: structure.id, point, dir }, fraction, at: from.lerp(to, fraction) });
            }
        });
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::Library;
    use std::{path::Path, sync::Arc};

    #[test]
    fn swept_bounds_keep_their_storage_once_warm() {
        let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
        let name = lib.blueprints[0].0.clone();
        let mut set = Structures::new(lib);
        for index in 0..300 {
            set.spawn(&name, DVec3::new(4e6 + f64::from(index) * 100.0, 3e6, 2e6), glam::Quat::IDENTITY).unwrap();
        }
        let mut sweep = Sweep::default();
        sweep.begin(&set);
        sweep.end(&set);
        let stored = sweep.tree.reserved();
        let poses = (sweep.before.as_ptr(), sweep.before.capacity());
        for frame in 0..100 {
            sweep.forecast(&set, 0.1);
            sweep.begin(&set);
            for structure in &mut set.list {
                structure.pos += DVec3::X * f64::from(frame);
            }
            sweep.end(&set);
            assert_eq!(sweep.tree.reserved(), stored);
            assert_eq!((sweep.before.as_ptr(), sweep.before.capacity()), poses);
        }
    }
}
