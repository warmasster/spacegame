//! What of a body is where, measured from its own mesh (nothing of it written by hand, so another
//! suit or another body has its own):
//!
//! - its trunk, front and back (`Trunk`): what an arm must not go through. A hand asked across
//!   the chest (the front grip of a launcher on the other shoulder) has its elbow go round to
//!   where its arm clears it (`body::ease`);
//! - each arm, bone by bone, as a capsule (`Piece`): what the eyes' line may run into. Looked at
//!   from its own eyes, an arm in front of what one aims at fades (`Body::look_along`), so that
//!   no hand on a control hides the control next to it.
//!
//! All in the model's axes at rest (x to its left, y up, z the way it faces; model units).
use glam::Vec3;
use lunar_core::{anim::Skeleton, mesh::Mesh};

/// The trunk's front, as seen from ahead: what an arm reaching across the body goes round. On a
/// grid of `cell` over x and y, how far ahead its front is (z; NaN where there is none of it):
/// whatever is behind that is the trunk or what it carries on its back. (Its own side of it an
/// arm is not told against: by its side a sleeve rubs on it as sleeves do. `Trunk::depth`.)
#[derive(Clone, Debug, Default)]
pub struct Trunk {
    cell: f32,
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    front: Vec<f32>,
}

/// A bone of an arm as a capsule: the line it is round and how thick it is.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub bone: usize,
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
}

/// What a body takes up.
#[derive(Clone, Debug, Default)]
pub struct Bulk {
    /// The trunk each arm goes round (left, right): grown by its upper arm's thickness, and
    /// without its own side out from inside the shoulders by that much (`Trunk::grown`).
    pub trunks: [Trunk; 2],
    /// How far out to each side (x) the trunk an arm goes round is told from.
    #[cfg_attr(not(test), allow(dead_code))]
    pub side: f32,
    /// The bone its trunk goes with above the waist (the last of its spine): an arm's place is
    /// taken into that bone's frame at rest to be told against the trunk.
    pub chest: Option<usize>,
    /// Each arm (left, right), shoulder to hand.
    pub arms: [Vec<Piece>; 2],
}

/// The grid's cell (model units).
const CELL: f32 = 0.02;
/// What of a bone's vertices its capsule's thickness is measured to (the rest stand out: a
/// cuff, a pocket).
const THICK: f32 = 0.9;

/// The bone that moves each vertex most.
fn strongest(mesh: &Mesh, i: usize) -> usize {
    let w = mesh.weights[i];
    let k = (0..4).max_by(|&a, &b| w[a].total_cmp(&w[b])).unwrap_or(0);
    usize::from(mesh.joints[i][k])
}

impl Trunk {
    /// From the triangles of `mesh` whose three corners go with `bones` most.
    pub fn measure(mesh: &Mesh, bones: &[usize]) -> Trunk {
        let of = |i: u32| bones.contains(&strongest(mesh, i as usize));
        let mut pts = Vec::new();
        for t in mesh.idx.chunks_exact(3) {
            if !(of(t[0]) && of(t[1]) && of(t[2])) {
                continue;
            }
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.pos[i as usize]));
            // (points over it no further apart than half a cell: a big flat panel leaves no hole)
            let n = ((a.distance(b).max(b.distance(c)).max(c.distance(a)) / (CELL * 0.5)).ceil() as usize).clamp(1, 200);
            for i in 0..=n {
                for j in 0..=n - i {
                    let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                    pts.push(a + (b - a) * u + (c - a) * v);
                }
            }
        }
        if pts.is_empty() {
            return Trunk::default();
        }
        let cell_of = |v: f32| (v / CELL).floor() as i32;
        let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
        for p in &pts {
            let (x, y) = (cell_of(p.x), cell_of(p.y));
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x), hi.1.max(y));
        }
        let (w, h) = ((hi.0 - lo.0 + 1) as usize, (hi.1 - lo.1 + 1) as usize);
        let mut t = Trunk { cell: CELL, x0: lo.0, y0: lo.1, w, h, front: vec![f32::NAN; w * h] };
        for p in &pts {
            let k = (cell_of(p.y) - lo.1) as usize * w + (cell_of(p.x) - lo.0) as usize;
            t.front[k] = if t.front[k].is_nan() { p.z } else { t.front[k].max(p.z) };
        }
        // (a band of it that goes with other bones — the collar under the neck — leaves no gap
        // to go through: up and down each column, what is between two of its cells is filled)
        for i in 0..w {
            let mut last: Option<usize> = None;
            for j in 0..h {
                if t.front[j * w + i].is_nan() {
                    continue;
                }
                if let Some(l) = last.filter(|&l| j > l + 1) {
                    let (lo, hi) = (t.front[l * w + i], t.front[j * w + i]);
                    for m in l + 1..j {
                        t.front[m * w + i] = lo + (hi - lo) * (m - l) as f32 / (j - l) as f32;
                    }
                }
                last = Some(j);
            }
        }
        t
    }

    /// Whether it has anything.
    pub fn is_empty(&self) -> bool {
        self.front.is_empty()
    }

    /// The trunk's front at a cell, if it is there.
    fn at(&self, x: i32, y: i32) -> Option<f32> {
        let (i, j) = (x - self.x0, y - self.y0);
        if i < 0 || j < 0 || i as usize >= self.w || j as usize >= self.h {
            return None;
        }
        let k = j as usize * self.w + i as usize;
        (!self.front[k].is_nan()).then_some(self.front[k])
    }

    /// How far into the trunk a point is (0: clear of it).
    pub fn depth(&self, p: Vec3) -> f32 {
        self.at((p.x / self.cell).floor() as i32, (p.y / self.cell).floor() as i32).map_or(0.0, |front| (front - p.z).max(0.0))
    }

    /// The trunk as a ball `r` thick runs into it (its front brought out by that much, and
    /// round its edges by what of the ball reaches past them): what a point is told against
    /// for that ball. Only what of it is no further toward `own` (+1 its left, −1 its right)
    /// than `side` (x): an arm is not told against its own side.
    pub fn grown(&self, r: f32, own: f32, side: f32) -> Trunk {
        if self.is_empty() {
            return Trunk::default();
        }
        let n = (r / self.cell).ceil().max(0.0) as i32;
        let (w, h) = (self.w + 2 * n as usize, self.h + 2 * n as usize);
        let mut out = Trunk { cell: self.cell, x0: self.x0 - n, y0: self.y0 - n, w, h, front: vec![f32::NAN; w * h] };
        let mid = |x: i32, y: i32| (Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 0.0)) * self.cell;
        for j in 0..h as i32 {
            for i in 0..w as i32 {
                let (cx, cy) = (out.x0 + i, out.y0 + j);
                let here = mid(cx, cy);
                let mut most = f32::NAN;
                for dy in -n..=n {
                    for dx in -n..=n {
                        let Some(front) = self.at(cx + dx, cy + dy) else { continue };
                        let there = mid(cx + dx, cy + dy);
                        if there.x * own > side {
                            continue;
                        }
                        // (the ball's own thickness over that cell, from its middle)
                        let off = (there - here).length() - 0.5 * self.cell;
                        if off > r {
                            continue;
                        }
                        let up = if off <= 0.0 { r } else { (r * r - off * off).sqrt() };
                        most = if most.is_nan() { front + up } else { most.max(front + up) };
                    }
                }
                out.front[j as usize * w + i as usize] = most;
            }
        }
        out
    }
}

impl Bulk {
    /// Measured from the mesh: the trunk is what goes with `pelvis` and `spine` most; each arm
    /// is its `arms` bones (shoulder, elbow, wrist), each with whatever hangs from it but the
    /// next (the hand, its fingers).
    pub fn measure(sk: &Skeleton, mesh: &Mesh, pelvis: usize, spine: &[usize], arms: [[usize; 3]; 2]) -> Bulk {
        if mesh.joints.len() != mesh.pos.len() || mesh.weights.len() != mesh.pos.len() {
            return Bulk::default();
        }
        let shoulders = arms;
        let arms = arms.map(|bones| {
            let mut out = Vec::new();
            for (k, &bone) in bones.iter().enumerate() {
                let next = bones.get(k + 1).copied();
                // its vertices: of it or of what hangs from it, short of the next bone
                let mine: Vec<Vec3> = (0..mesh.pos.len())
                    .filter(|&i| {
                        let b = strongest(mesh, i);
                        sk.under(b, bone) && next.is_none_or(|n| !sk.under(b, n))
                    })
                    .map(|i| Vec3::from(mesh.pos[i]))
                    .collect();
                if mine.is_empty() {
                    continue;
                }
                let head = sk.bind[bone].pos;
                // along it: to the next bone's head, or (the hand) to the middle of its vertices
                let to = next.map_or_else(|| mine.iter().sum::<Vec3>() / mine.len() as f32, |n| sk.bind[n].pos);
                let dir = (to - head).normalize_or(Vec3::NEG_Y);
                let (mut t0, mut t1) = (f32::MAX, f32::MIN);
                let mut off: Vec<f32> = Vec::with_capacity(mine.len());
                for p in &mine {
                    let t = (*p - head).dot(dir);
                    (t0, t1) = (t0.min(t), t1.max(t));
                    off.push((*p - head - dir * t).length());
                }
                off.sort_by(f32::total_cmp);
                let r = off[((off.len() - 1) as f32 * THICK) as usize];
                // (its ends rounded: the capsule's line is short of them by its thickness)
                let (a, b) = if t1 - t0 > 2.0 * r { (t0 + r, t1 - r) } else { ((t0 + t1) * 0.5, (t0 + t1) * 0.5) };
                out.push(Piece { bone, a: head + dir * a, b: head + dir * b, r });
            }
            out
        });
        // the trunk, and its side: inside the shoulders by an upper arm's thickness (what an
        // arm by the body touches is not across it)
        let trunk_bones: Vec<usize> = std::iter::once(pelvis).chain(spine.iter().copied()).collect();
        let chest = spine.last().copied().unwrap_or(pelvis);
        let side = (0..2).map(|s| sk.bind[shoulders[s][0]].pos.x.abs() - arms[s].first().map_or(0.0, |p| p.r)).fold(f32::MAX, f32::min);
        let (side, trunk) = (side.max(0.0), Trunk::measure(mesh, &trunk_bones));
        let trunks = [0, 1].map(|s| trunk.grown(arms[s].first().map_or(0.0, |p| p.r), if s == 0 { 1.0 } else { -1.0 }, side));
        Bulk { trunks, side, chest: Some(chest), arms }
    }
}

/// Where along a ray from `o` the way `d` (unit) comes nearest to the segment `a`..`b`, and how
/// near (the ray's own length along it, the distance).
pub fn ray_to_segment(o: Vec3, d: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let w = o - a;
    let (dd, de) = (d.dot(ab), d.dot(w));
    let denom = len2 - dd * dd;
    // the segment's point nearest the ray's line, then the ray's point nearest it (both held to
    // what they are)
    let mut s = if len2 > 1e-12 && denom.abs() > 1e-9 { ((ab.dot(w) - dd * de) / denom).clamp(0.0, 1.0) } else { 0.0 };
    let mut t = (a + ab * s - o).dot(d).max(0.0);
    if len2 > 1e-12 {
        s = ((o + d * t - a).dot(ab) / len2).clamp(0.0, 1.0);
        t = (a + ab * s - o).dot(d).max(0.0);
    }
    (t, (o + d * t).distance(a + ab * s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_finds_how_near_it_passes_a_segment() {
        // across it, along it, past its end, behind the ray's start
        let (a, b) = (Vec3::new(-1.0, 0.0, 2.0), Vec3::new(1.0, 0.0, 2.0));
        let (t, d) = ray_to_segment(Vec3::new(0.0, 0.3, 0.0), Vec3::Z, a, b);
        assert!((t - 2.0).abs() < 1e-5 && (d - 0.3).abs() < 1e-5, "{t} {d}");
        let (t, d) = ray_to_segment(Vec3::new(-3.0, 0.0, 2.0), Vec3::X, a, b);
        assert!(d < 1e-5 && (1.9..=4.1).contains(&t), "{t} {d}");
        let (_, d) = ray_to_segment(Vec3::new(3.0, 0.0, 0.0), Vec3::Z, a, b);
        assert!((d - 2.0).abs() < 1e-4, "{d}");
        let (t, d) = ray_to_segment(Vec3::new(0.0, 0.0, 3.0), Vec3::Z, a, b);
        assert!(t == 0.0 && (d - 1.0).abs() < 1e-5, "{t} {d}");
    }

    #[test]
    fn a_trunk_is_told_from_its_triangles_and_a_ball_by_it_is_told_how_far_in() {
        // a box 0.4 wide, 0.6 high, from z −0.1 to 0.15, its faces as two triangles each
        let mut m = Mesh::default();
        let c = |x: f32, y: f32, z: f32| [x, y, z];
        let corners = [c(-0.2, 1.0, -0.1), c(0.2, 1.0, -0.1), c(0.2, 1.6, -0.1), c(-0.2, 1.6, -0.1), c(-0.2, 1.0, 0.15), c(0.2, 1.0, 0.15), c(0.2, 1.6, 0.15), c(-0.2, 1.6, 0.15)];
        for p in corners {
            m.pos.push(p);
            m.nrm.push([0.0, 0.0, 1.0]);
            m.mat.push(Default::default());
            m.joints.push([0; 4]);
            m.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
        for f in [[0, 1, 2, 3], [4, 5, 6, 7], [0, 1, 5, 4], [3, 2, 6, 7], [0, 3, 7, 4], [1, 2, 6, 5]] {
            m.idx.extend_from_slice(&[f[0], f[1], f[2], f[0], f[2], f[3]]);
        }
        let t = Trunk::measure(&m, &[0]);
        assert!(!t.is_empty());
        let depth = |p: Vec3, r: f32| t.grown(r, 1.0, 1.0).depth(p);
        // in front of it, clear; touching it; inside it, the deeper the further back; beside it
        // but thicker than the gap
        assert_eq!(depth(Vec3::new(0.0, 1.3, 0.3), 0.05), 0.0);
        assert!((depth(Vec3::new(0.0, 1.3, 0.2), 0.05)).abs() < 1e-3);
        assert!((depth(Vec3::new(0.0, 1.3, 0.1), 0.05) - 0.1).abs() < 1e-3);
        assert!((depth(Vec3::new(0.0, 1.3, -0.2), 0.05) - 0.4).abs() < 1e-3);
        assert!(depth(Vec3::new(0.24, 1.3, 0.0), 0.06) > 0.0 && depth(Vec3::new(0.3, 1.3, 0.0), 0.06) == 0.0);
        // over it or under it, clear; and no trunk at all is nothing in the way
        assert_eq!(depth(Vec3::new(0.0, 1.8, 0.0), 0.05), 0.0);
        assert_eq!(Trunk::default().grown(1.0, 1.0, 1.0).depth(Vec3::ZERO), 0.0);
        // a left arm is not told against the trunk's left past its side; across, it is
        let (left, right) = (t.grown(0.01, 1.0, 0.1), t.grown(0.01, -1.0, 0.1));
        assert!(left.depth(Vec3::new(0.15, 1.3, 0.1)) == 0.0 && left.depth(Vec3::new(-0.15, 1.3, 0.1)) > 0.0);
        assert!(right.depth(Vec3::new(-0.15, 1.3, 0.1)) == 0.0);
    }
}
