//! Which parts of a structure are near a point, a ray or another structure, without looking at
//! all of them: a tree of boxes over the parts' bounding spheres (median splits, a few parts per
//! leaf, everything in two flat vectors). Built when the structure changes (a part gone, a pose),
//! which costs a few tens of microseconds for hundreds of parts; asked many times in between by
//! rays (shots, sights, the air's leaks), blasts and contacts.
use glam::Vec3;

const LEAF: usize = 4;
const STACK: usize = 64;

#[derive(Clone, Copy, Debug, Default)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Inner: its first child (the second follows it). Leaf: its first item.
    first: u32,
    /// Items of a leaf; 0 for an inner node.
    count: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Bvh {
    nodes: Vec<Node>,
    items: Vec<u32>,
    /// Scratch of the build: (item, centre, radius).
    work: Vec<(u32, Vec3, f32)>,
}

impl Bvh {
    #[cfg(test)]
    pub(crate) fn reserved(&self) -> [usize; 6] {
        [self.nodes.as_ptr() as usize, self.nodes.capacity(), self.items.as_ptr() as usize, self.items.capacity(), self.work.as_ptr() as usize, self.work.capacity()]
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Over the spheres given: (item, centre, radius).
    pub fn build(&mut self, spheres: impl Iterator<Item = (u32, Vec3, f32)>) {
        self.nodes.clear();
        self.items.clear();
        let mut work = std::mem::take(&mut self.work);
        work.clear();
        work.extend(spheres);
        if !work.is_empty() {
            self.nodes.push(Node::default());
            self.split(0, &mut work, 0);
        }
        self.work = work;
    }

    fn bounds(items: &[(u32, Vec3, f32)]) -> (Vec3, Vec3) {
        items.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &(_, c, r)| (lo.min(c - r), hi.max(c + r)))
    }

    /// Node `n` over `items` (a slice of the work list starting at `base` of the final order).
    fn split(&mut self, n: usize, items: &mut [(u32, Vec3, f32)], depth: usize) {
        let (lo, hi) = Bvh::bounds(items);
        if items.len() <= LEAF || depth >= STACK - 2 {
            self.nodes[n] = Node { lo, hi, first: self.items.len() as u32, count: items.len() as u32 };
            self.items.extend(items.iter().map(|i| i.0));
            return;
        }
        // along the axis the centres spread most on, half and half
        let (clo, chi) = items.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), i| (lo.min(i.1), hi.max(i.1)));
        let e = chi - clo;
        let axis = if e.x >= e.y && e.x >= e.z {
            0
        } else if e.y >= e.z {
            1
        } else {
            2
        };
        let mid = items.len() / 2;
        items.select_nth_unstable_by(mid, |a, b| a.1[axis].total_cmp(&b.1[axis]));
        let first = self.nodes.len();
        self.nodes.push(Node::default());
        self.nodes.push(Node::default());
        self.nodes[n] = Node { lo, hi, first: first as u32, count: 0 };
        let (left, right) = items.split_at_mut(mid);
        self.split(first, left, depth + 1);
        self.split(first + 1, right, depth + 1);
    }

    /// Every item whose box the sphere (`c`, `r`) touches.
    pub fn sphere(&self, c: Vec3, r: f32, mut f: impl FnMut(u32)) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = [0u32; STACK];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let n = &self.nodes[stack[top] as usize];
            let d = (n.lo - c).max(c - n.hi).max(Vec3::ZERO);
            if d.length_squared() > r * r {
                continue;
            }
            if n.count > 0 {
                for &i in &self.items[n.first as usize..(n.first + n.count) as usize] {
                    f(i);
                }
            } else {
                stack[top] = n.first;
                stack[top + 1] = n.first + 1;
                top += 2;
            }
        }
    }

    /// Every item whose box the ray from `o` along `dir` (unit) crosses within its reach, nearest
    /// boxes first. `f` gets the item and the reach now, and gives the reach from then on (shorter
    /// once it has hit something: what lies beyond is not looked at).
    pub fn ray(&self, o: Vec3, dir: Vec3, max: f32, mut f: impl FnMut(u32, f32) -> f32) {
        if self.nodes.is_empty() {
            return;
        }
        // where the ray enters a box, if it does before `reach`
        let enter = |n: &Node, reach: f32| -> Option<f32> {
            let (mut near, mut far) = (0.0f32, reach);
            for a in 0..3 {
                let (lo, hi, from, d) = (n.lo[a], n.hi[a], o[a], dir[a]);
                if d.abs() < 1e-12 {
                    if from < lo || from > hi {
                        return None;
                    }
                } else {
                    let (t0, t1) = ((lo - from) / d, (hi - from) / d);
                    near = near.max(t0.min(t1));
                    far = far.min(t0.max(t1));
                }
            }
            (near <= far).then_some(near)
        };
        let mut reach = max;
        let mut stack = [(0u32, 0.0f32); STACK];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let (k, t) = stack[top];
            if t > reach {
                continue;
            }
            let n = &self.nodes[k as usize];
            if n.count > 0 {
                for &i in &self.items[n.first as usize..(n.first + n.count) as usize] {
                    reach = f(i, reach);
                }
                continue;
            }
            let (l, r) = (n.first, n.first + 1);
            let (tl, tr) = (enter(&self.nodes[l as usize], reach), enter(&self.nodes[r as usize], reach));
            // the nearer child last onto the stack: looked at first
            match (tl, tr) {
                (Some(a), Some(b)) => {
                    let ((near, tn), (far, tf)) = if a <= b { ((l, a), (r, b)) } else { ((r, b), (l, a)) };
                    stack[top] = (far, tf);
                    stack[top + 1] = (near, tn);
                    top += 2;
                }
                (Some(a), None) => {
                    stack[top] = (l, a);
                    top += 1;
                }
                (None, Some(b)) => {
                    stack[top] = (r, b);
                    top += 1;
                }
                (None, None) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scatter(n: u32) -> Vec<(u32, Vec3, f32)> {
        let mut x = 12345u32;
        let mut rnd = move || {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 8) as f32 / 16_777_216.0
        };
        (0..n).map(|i| (i, Vec3::new(rnd() * 20.0 - 10.0, rnd() * 6.0, rnd() * 30.0 - 15.0), 0.05 + rnd() * 0.8)).collect()
    }

    #[test]
    fn a_sphere_finds_exactly_what_a_full_search_finds() {
        let s = scatter(700);
        let mut b = Bvh::default();
        b.build(s.iter().copied());
        assert_eq!(b.len(), 700);
        for &(c, r) in &[(Vec3::new(0.0, 3.0, 0.0), 2.0), (Vec3::new(9.0, 1.0, -14.0), 0.5), (Vec3::new(-30.0, 0.0, 0.0), 3.0), (Vec3::ZERO, 50.0)] {
            let mut got = Vec::new();
            b.sphere(c, r, |i| {
                let (_, pc, pr) = s[i as usize];
                if pc.distance(c) <= pr + r {
                    got.push(i);
                }
            });
            got.sort_unstable();
            let want: Vec<u32> = s.iter().filter(|(_, pc, pr)| pc.distance(c) <= pr + r).map(|i| i.0).collect();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn a_ray_finds_the_nearest_sphere_and_stops_looking_beyond_it() {
        let s = scatter(700);
        let mut b = Bvh::default();
        b.build(s.iter().copied());
        let hit = |o: Vec3, d: Vec3, c: Vec3, r: f32| -> Option<f32> {
            let oc = o - c;
            let (bq, cq) = (oc.dot(d), oc.length_squared() - r * r);
            let disc = bq * bq - cq;
            (disc >= 0.0 && -bq - disc.sqrt() >= 0.0).then(|| -bq - disc.sqrt())
        };
        for (o, d) in [(Vec3::new(-20.0, 3.0, 0.0), Vec3::X), (Vec3::new(0.0, 20.0, 3.0), Vec3::NEG_Y), (Vec3::new(-12.0, 1.0, -16.0), Vec3::new(1.0, 0.1, 1.2).normalize())] {
            let mut looked = 0;
            let mut best: Option<(u32, f32)> = None;
            b.ray(o, d, 100.0, |i, reach| {
                looked += 1;
                let (_, c, r) = s[i as usize];
                match hit(o, d, c, r) {
                    Some(t) if t < reach => {
                        best = Some((i, t));
                        t
                    }
                    _ => reach,
                }
            });
            let want = s.iter().filter_map(|&(i, c, r)| hit(o, d, c, r).map(|t| (i, t))).min_by(|a, b| a.1.total_cmp(&b.1));
            assert_eq!(best.map(|b| b.0), want.map(|w| w.0));
            assert!(looked < 200, "it looked at {looked} of 700");
        }
    }

    #[test]
    fn nothing_in_it_answers_nothing() {
        let b = Bvh::default();
        b.sphere(Vec3::ZERO, 10.0, |_| panic!());
        b.ray(Vec3::ZERO, Vec3::X, 10.0, |_, _| panic!());
    }
}
