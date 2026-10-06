//! Far LODs by quadric edge collapse (Garland-Heckbert, half-edge variant): the cheapest edge folds
//! one endpoint into the other until the triangle budget is met. Topology is welded by position,
//! so split vertices (hard edges, material seams) move together; material borders and open borders
//! carry extra constraint planes, and a collapse that would flip or crush a triangle is refused.
//! Kept vertices keep their own attributes (material, skinning; the normal unless it no longer
//! matches the simplified faces): no colour ever bleeds from a hidden or metallic part onto the suit.
use crate::mesh::{Material, Mesh};
use glam::DVec3;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

/// Symmetric 4x4 error quadric.
#[derive(Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn plane(n: DVec3, d: f64, w: f64) -> Quadric {
        let (a, b, c) = (n.x, n.y, n.z);
        Quadric([a * a, a * b, a * c, a * d, b * b, b * c, b * d, c * c, c * d, d * d].map(|v| v * w))
    }
    fn add(&mut self, o: &Quadric) {
        for (a, b) in self.0.iter_mut().zip(o.0) {
            *a += b;
        }
    }
    fn eval(&self, p: DVec3) -> f64 {
        let q = &self.0;
        let (x, y, z) = (p.x, p.y, p.z);
        q[0] * x * x + 2.0 * q[1] * x * y + 2.0 * q[2] * x * z + 2.0 * q[3] * x + q[4] * y * y + 2.0 * q[5] * y * z + 2.0 * q[6] * y + q[7] * z * z + 2.0 * q[8] * z + q[9]
    }
}

struct Candidate {
    cost: f64,
    from: u32,
    to: u32,
    stamps: (u32, u32),
}

impl PartialEq for Candidate {
    fn eq(&self, o: &Self) -> bool {
        self.cost == o.cost
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Candidate {
    // min-heap on cost
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost.total_cmp(&self.cost)
    }
}

/// Weight of border and material-seam constraint planes relative to the surface's own.
const BORDER_WEIGHT: f64 = 20.0;

fn same_look(a: &Material, b: &Material) -> bool {
    a == b
}

/// Simplify `mesh` to at most `max_tris` triangles (fewer if nothing more can fold safely).
pub fn decimate(mesh: &Mesh, max_tris: usize) -> Mesh {
    let n = mesh.pos.len();
    let skinned = !mesh.joints.is_empty();
    // weld by position: w[v] is the welded vertex of original vertex v
    let mut weld: HashMap<[i64; 3], u32> = HashMap::with_capacity(n);
    let mut w = vec![0u32; n];
    let mut copies: Vec<Vec<u32>> = Vec::new();
    let mut pos: Vec<DVec3> = Vec::new();
    for (v, p) in mesh.pos.iter().enumerate() {
        let key = p.map(|c| (f64::from(c) * 1e5).round() as i64);
        let id = *weld.entry(key).or_insert_with(|| {
            copies.push(Vec::new());
            pos.push(DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])));
            (copies.len() - 1) as u32
        });
        w[v] = id;
        copies[id as usize].push(v as u32);
    }
    let nw = copies.len();
    let mut tris: Vec<[u32; 3]> = mesh.idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).filter(|t| w[t[0] as usize] != w[t[1] as usize] && w[t[1] as usize] != w[t[2] as usize] && w[t[0] as usize] != w[t[2] as usize]).collect();
    let mut alive = vec![true; tris.len()];
    let mut live = tris.len();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); nw];
    for (t, tri) in tris.iter().enumerate() {
        for c in tri {
            adj[w[*c as usize] as usize].push(t as u32);
        }
    }
    let face = |tri: &[u32; 3], pos: &[DVec3], w: &[u32]| -> DVec3 {
        let p = tri.map(|c| pos[w[c as usize] as usize]);
        (p[1] - p[0]).cross(p[2] - p[0])
    };
    // quadrics: every face's plane, plus planes across open borders and material seams
    let mut quad = vec![Quadric::default(); nw];
    let mut edges: HashMap<(u32, u32), (u32, u32)> = HashMap::with_capacity(tris.len() * 2);
    for (t, tri) in tris.iter().enumerate() {
        let f = face(tri, &pos, &w);
        let area = f.length() * 0.5;
        if area <= 0.0 {
            continue;
        }
        let nrm = f / (2.0 * area);
        let q = Quadric::plane(nrm, -nrm.dot(pos[w[tri[0] as usize] as usize]), area);
        for c in tri {
            quad[w[*c as usize] as usize].add(&q);
        }
        for k in 0..3 {
            let (a, b) = (w[tri[k] as usize], w[tri[(k + 1) % 3] as usize]);
            let e = edges.entry((a.min(b), a.max(b))).or_insert((t as u32, u32::MAX));
            if e.0 != t as u32 {
                e.1 = t as u32;
            }
        }
    }
    for (&(a, b), &(t0, t1)) in &edges {
        let seam = t1 == u32::MAX || !same_look(&mesh.mat[tris[t0 as usize][0] as usize], &mesh.mat[tris[t1 as usize][0] as usize]);
        if !seam {
            continue;
        }
        let f = face(&tris[t0 as usize], &pos, &w).normalize_or_zero();
        let e = pos[b as usize] - pos[a as usize];
        let side = e.cross(f).normalize_or_zero();
        if side == DVec3::ZERO {
            continue;
        }
        let q = Quadric::plane(side, -side.dot(pos[a as usize]), e.length_squared() * BORDER_WEIGHT);
        quad[a as usize].add(&q);
        quad[b as usize].add(&q);
    }
    let mut stamp = vec![0u32; nw];
    let mut dead = vec![false; nw];
    let mut heap = BinaryHeap::with_capacity(edges.len() * 2);
    let push = |heap: &mut BinaryHeap<Candidate>, a: u32, b: u32, quad: &[Quadric], pos: &[DVec3], stamp: &[u32]| {
        let mut q = quad[a as usize];
        q.add(&quad[b as usize]);
        // fold the endpoint whose removal costs less
        let (ca, cb) = (q.eval(pos[a as usize]), q.eval(pos[b as usize]));
        let (from, to, cost) = if cb <= ca { (a, b, cb) } else { (b, a, ca) };
        heap.push(Candidate { cost: cost.max(0.0), from, to, stamps: (stamp[from as usize], stamp[to as usize]) });
    };
    for &(a, b) in edges.keys() {
        push(&mut heap, a, b, &quad, &pos, &stamp);
    }
    drop(edges);
    let mut ring_a: Vec<u32> = Vec::new();
    let mut ring_b: Vec<u32> = Vec::new();
    while live > max_tris {
        let Some(c) = heap.pop() else { break };
        let (a, b) = (c.from as usize, c.to as usize);
        if dead[a] || dead[b] || stamp[a] != c.stamps.0 || stamp[b] != c.stamps.1 {
            continue;
        }
        adj[a].retain(|t| alive[*t as usize]);
        adj[b].retain(|t| alive[*t as usize]);
        // link condition: the edge's two ends share only the edge's own triangles' third vertices
        ring_a.clear();
        ring_b.clear();
        for (ring, v) in [(&mut ring_a, a), (&mut ring_b, b)] {
            for t in &adj[v] {
                ring.extend(tris[*t as usize].iter().map(|c| w[*c as usize]).filter(|x| *x as usize != v));
            }
            ring.sort_unstable();
            ring.dedup();
        }
        let shared = ring_a.iter().filter(|x| ring_b.binary_search(x).is_ok()).count();
        let edge_tris = adj[a].iter().filter(|t| tris[**t as usize].iter().any(|c| w[*c as usize] as usize == b)).count();
        if edge_tris == 0 || shared > edge_tris {
            continue;
        }
        // no flipped or crushed triangles round `a`
        let ok = adj[a].iter().all(|t| {
            let tri = &tris[*t as usize];
            if tri.iter().any(|c| w[*c as usize] as usize == b) {
                return true;
            }
            let before = face(tri, &pos, &w);
            let p = tri.map(|c| if w[c as usize] as usize == a { pos[b] } else { pos[w[c as usize] as usize] });
            let after = (p[1] - p[0]).cross(p[2] - p[0]);
            after.dot(before) > 0.2 * before.length() * after.length() && after.length() > 1e-12
        });
        if !ok {
            continue;
        }
        // fold a into b
        let moved: Vec<u32> = std::mem::take(&mut adj[a]);
        for t in moved {
            let ti = t as usize;
            if tris[ti].iter().any(|c| w[*c as usize] as usize == b) {
                alive[ti] = false;
                live -= 1;
                continue;
            }
            for k in 0..3 {
                let corner = tris[ti][k] as usize;
                if w[corner] as usize != a {
                    continue;
                }
                // the copy of b that looks like this corner (same material, nearest normal)
                let mat = mesh.mat[corner];
                let nrm = DVec3::from_array(mesh.nrm[corner].map(f64::from));
                let score = |v: &u32| {
                    let v = *v as usize;
                    let same = if same_look(&mesh.mat[v], &mat) { 2.0 } else { 0.0 };
                    same + DVec3::from_array(mesh.nrm[v].map(f64::from)).dot(nrm)
                };
                let best = copies[b].iter().max_by(|x, y| score(x).total_cmp(&score(y))).copied().unwrap_or(corner as u32);
                tris[ti][k] = best;
            }
            adj[b].push(t);
        }
        let qa = quad[a];
        quad[b].add(&qa);
        dead[a] = true;
        stamp[b] += 1;
        // new candidates round b
        ring_b.clear();
        for t in &adj[b] {
            if alive[*t as usize] {
                ring_b.extend(tris[*t as usize].iter().map(|c| w[*c as usize]).filter(|x| *x as usize != b));
            }
        }
        ring_b.sort_unstable();
        ring_b.dedup();
        for &v in &ring_b {
            push(&mut heap, b as u32, v, &quad, &pos, &stamp);
        }
    }
    // compact
    let mut out = Mesh::default();
    let mut remap = vec![u32::MAX; n];
    for (t, tri) in tris.iter().enumerate() {
        if !alive[t] {
            continue;
        }
        for &c in tri {
            let c = c as usize;
            if remap[c] == u32::MAX {
                remap[c] = out.pos.len() as u32;
                out.pos.push(mesh.pos[c]);
                out.nrm.push(mesh.nrm[c]);
                out.mat.push(mesh.mat[c]);
                if skinned {
                    out.joints.push(mesh.joints[c]);
                    out.weights.push(mesh.weights[c]);
                }
            }
            out.idx.push(remap[c]);
        }
    }
    // normals that no longer match the simplified faces round them are rebuilt from those faces
    let mut sums = vec![glam::Vec3::ZERO; out.pos.len()];
    for t in out.idx.chunks_exact(3) {
        let p = [0, 1, 2].map(|k| glam::Vec3::from_array(out.pos[t[k] as usize]));
        let f = (p[1] - p[0]).cross(p[2] - p[0]);
        for &c in t {
            sums[c as usize] += f;
        }
    }
    for (nrm, sum) in out.nrm.iter_mut().zip(sums) {
        let rebuilt = sum.normalize_or_zero();
        if rebuilt != glam::Vec3::ZERO && glam::Vec3::from_array(*nrm).dot(rebuilt) < 0.7 {
            *nrm = rebuilt.to_array();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::mesh::{Material, sphere};

    #[test]
    fn sphere_to_budget_keeps_shape_and_winding() {
        let m = Material::new([200, 200, 200], 100, 0);
        let s = sphere(1.0, 64, false, m);
        let d = super::decimate(&s, s.tris() / 8);
        assert!(d.tris() <= s.tris() / 8 && d.tris() > s.tris() / 16, "{} of {}", d.tris(), s.tris());
        for t in d.idx.chunks(3) {
            let p = [0, 1, 2].map(|k| glam::Vec3::from_array(d.pos[t[k] as usize]));
            let f = (p[1] - p[0]).cross(p[2] - p[0]);
            let c = (p[0] + p[1] + p[2]) / 3.0;
            assert!(f.dot(c) >= 0.0, "a triangle faces inward");
            assert!((c.length() - 1.0).abs() < 0.15);
        }
    }
}
