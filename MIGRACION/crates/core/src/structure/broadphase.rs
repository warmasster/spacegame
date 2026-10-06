//! Which structures a path might touch, without looking at all of them: a hash grid of their
//! bounding spheres. Rebuilt when asked (structures move); queried with segments.
use super::state::Structure;
use glam::DVec3;
use std::collections::HashMap;

pub struct Grid {
    /// Cell size (m).
    pub cell: f64,
    cells: HashMap<[i64; 3], Vec<u32>>,
    seen: Vec<u32>,
    stamp: u32,
}

impl Grid {
    pub fn new(cell: f64) -> Grid {
        Grid { cell, cells: HashMap::new(), seen: Vec::new(), stamp: 0 }
    }

    fn key(&self, p: DVec3) -> [i64; 3] {
        let q = (p / self.cell).floor();
        [q.x as i64, q.y as i64, q.z as i64]
    }

    /// Every structure into the cells its bounding sphere overlaps.
    pub fn build(&mut self, list: &[Structure]) {
        for v in self.cells.values_mut() {
            v.clear();
        }
        for (i, s) in list.iter().enumerate() {
            let c = s.to_world(s.center);
            let r = f64::from(s.radius);
            let (lo, hi) = (self.key(c - DVec3::splat(r)), self.key(c + DVec3::splat(r)));
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        self.cells.entry([x, y, z]).or_default().push(i as u32);
                    }
                }
            }
        }
        self.cells.retain(|_, v| !v.is_empty());
        self.seen.clear();
        self.seen.resize(list.len(), 0);
    }

    /// Structures whose cells the segment a→b (widened by `pad` m) crosses, each once, into `out`.
    pub fn along(&mut self, a: DVec3, b: DVec3, pad: f64, out: &mut Vec<u32>) {
        out.clear();
        self.stamp = self.stamp.wrapping_add(1).max(1);
        let len = a.distance(b);
        let steps = (len / (self.cell * 0.5)).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let p = a + (b - a) * (k as f64 / steps as f64);
            let (lo, hi) = (self.key(p - DVec3::splat(pad)), self.key(p + DVec3::splat(pad)));
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        let Some(v) = self.cells.get(&[x, y, z]) else {
                            continue;
                        };
                        for &i in v {
                            if self.seen[i as usize] != self.stamp {
                                self.seen[i as usize] = self.stamp;
                                out.push(i);
                            }
                        }
                    }
                }
            }
        }
    }
}
