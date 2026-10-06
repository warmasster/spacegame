//! Convex polyhedra: the shape of every structure part and of every piece one breaks into. Faces
//! are convex polygons wound counter-clockwise seen from outside; cutting by a plane keeps that
//! (each face clipped, the cut capped with the crossing points), so pieces cut again freely.
use crate::mesh::{Material, Mesh};
use glam::{Affine3A, Vec3};

/// Points closer than this to a plane are on it (m).
const EPS: f32 = 1e-5;

/// The plane n·p = d (unit n); "front" is where n·p > d.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub n: Vec3,
    pub d: f32,
}

impl Plane {
    pub fn through(p: Vec3, n: Vec3) -> Plane {
        let n = n.normalize();
        Plane { n, d: n.dot(p) }
    }
    pub fn dist(&self, p: Vec3) -> f32 {
        self.n.dot(p) - self.d
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub plane: Plane,
    /// Counter-clockwise seen from outside.
    pub verts: Vec<Vec3>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Convex {
    pub faces: Vec<Face>,
}

/// Newell normal of a polygon (any winding; length ~ twice the area).
fn newell(v: &[Vec3]) -> Vec3 {
    let mut n = Vec3::ZERO;
    for (i, a) in v.iter().enumerate() {
        let b = v[(i + 1) % v.len()];
        n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    n
}

impl Convex {
    /// From vertices and faces (indices, any winding): each face turned to face outward.
    pub fn from_faces(verts: &[Vec3], faces: &[&[usize]]) -> Convex {
        let c = verts.iter().copied().sum::<Vec3>() / verts.len() as f32;
        let faces = faces
            .iter()
            .map(|f| {
                let mut v: Vec<Vec3> = f.iter().map(|&i| verts[i]).collect();
                let mut n = newell(&v).normalize();
                if n.dot(v[0] - c) < 0.0 {
                    v.reverse();
                    n = -n;
                }
                Face { plane: Plane { n, d: n.dot(v[0]) }, verts: v }
            })
            .collect();
        Convex { faces }
    }

    /// A box of half extents `h`.
    pub fn cuboid(h: Vec3) -> Convex {
        let v: Vec<Vec3> = (0..8).map(|i| Vec3::new(if i & 1 == 0 { -h.x } else { h.x }, if i & 2 == 0 { -h.y } else { h.y }, if i & 4 == 0 { -h.z } else { h.z })).collect();
        Convex::from_faces(&v, &[&[0, 2, 6, 4], &[1, 3, 7, 5], &[0, 1, 5, 4], &[2, 3, 7, 6], &[0, 1, 3, 2], &[4, 5, 7, 6]])
    }

    /// A prism of `seg` sides round the Y axis: radius `r`, height `2 hy`, radii scaled by `taper`
    /// at the top (1 a cylinder, 0.01 nearly a cone).
    pub fn prism(r: f32, hy: f32, seg: usize, taper: f32) -> Convex {
        let seg = seg.max(3);
        let mut v = Vec::with_capacity(seg * 2);
        for i in 0..seg {
            let a = i as f32 / seg as f32 * std::f32::consts::TAU;
            v.push(Vec3::new(a.cos() * r, -hy, a.sin() * r));
            v.push(Vec3::new(a.cos() * r * taper.max(0.01), hy, a.sin() * r * taper.max(0.01)));
        }
        let bottom: Vec<usize> = (0..seg).map(|i| i * 2).collect();
        let top: Vec<usize> = (0..seg).map(|i| i * 2 + 1).collect();
        let sides: Vec<[usize; 4]> = (0..seg).map(|i| [i * 2, (i + 1) % seg * 2, (i + 1) % seg * 2 + 1, i * 2 + 1]).collect();
        let mut faces: Vec<&[usize]> = vec![&bottom, &top];
        faces.extend(sides.iter().map(|s| s.as_slice()));
        Convex::from_faces(&v, &faces)
    }

    /// A ramp: the box of half extents `h` with its top edge at -z only (slope toward +z).
    pub fn wedge(h: Vec3) -> Convex {
        let v = [Vec3::new(-h.x, -h.y, -h.z), Vec3::new(h.x, -h.y, -h.z), Vec3::new(-h.x, -h.y, h.z), Vec3::new(h.x, -h.y, h.z), Vec3::new(-h.x, h.y, -h.z), Vec3::new(h.x, h.y, -h.z)];
        Convex::from_faces(&v, &[&[0, 1, 3, 2], &[0, 1, 5, 4], &[2, 3, 5, 4], &[0, 2, 4], &[1, 3, 5]])
    }

    /// The convex hull of a small point set (generators, models: up to a few dozen points; it is
    /// built once when content loads). Every plane through three points with all the others behind
    /// it is a face; the points on it, ordered round its normal, its polygon. None when the points
    /// span no volume.
    pub fn hull(points: &[Vec3]) -> Option<Convex> {
        // drop duplicates
        let mut pts: Vec<Vec3> = Vec::with_capacity(points.len());
        for &p in points {
            if !pts.iter().any(|q| q.distance_squared(p) < 1e-10) {
                pts.push(p);
            }
        }
        let n = pts.len();
        if n < 4 {
            return None;
        }
        let size = pts.iter().fold(0.0f32, |m, p| m.max(p.abs().max_element())).max(1e-3);
        let eps = size * 1e-5;
        let c = pts.iter().copied().sum::<Vec3>() / n as f32;
        let mut planes: Vec<Plane> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    let nrm = (pts[j] - pts[i]).cross(pts[k] - pts[i]);
                    if nrm.length_squared() < eps * eps * 1e-2 {
                        continue;
                    }
                    let mut pl = Plane::through(pts[i], nrm);
                    if pl.dist(c) > 0.0 {
                        pl = Plane { n: -pl.n, d: -pl.d };
                    }
                    if pts.iter().all(|&p| pl.dist(p) <= eps) && !planes.iter().any(|q| q.n.dot(pl.n) > 1.0 - 1e-5 && (q.d - pl.d).abs() < eps * 4.0) {
                        planes.push(pl);
                    }
                }
            }
        }
        let mut faces = Vec::with_capacity(planes.len());
        for pl in planes {
            let on: Vec<Vec3> = pts.iter().copied().filter(|&p| pl.dist(p).abs() <= eps * 4.0).collect();
            if on.len() < 3 {
                continue;
            }
            // order counter-clockwise round the normal (seen from outside)
            let fc = on.iter().copied().sum::<Vec3>() / on.len() as f32;
            let u = (on[0] - fc).normalize_or(pl.n.any_orthonormal_vector());
            let v = pl.n.cross(u);
            let mut ring: Vec<(f32, Vec3)> = on.iter().map(|&p| ((p - fc).dot(v).atan2((p - fc).dot(u)), p)).collect();
            ring.sort_by(|a, b| a.0.total_cmp(&b.0));
            faces.push(Face { plane: Plane { n: pl.n, d: pl.n.dot(fc) }, verts: ring.into_iter().map(|(_, p)| p).collect() });
        }
        let hull = Convex { faces };
        (hull.faces.len() >= 4 && hull.volume() > 1e-9).then_some(hull)
    }

    pub fn verts(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.faces.iter().flat_map(|f| f.verts.iter().copied())
    }

    /// Volume (m³) and centroid.
    pub fn mass_props(&self) -> (f32, Vec3) {
        let n: usize = self.faces.iter().map(|f| f.verts.len()).sum();
        let c0 = self.verts().sum::<Vec3>() / n.max(1) as f32;
        let (mut vol, mut moment) = (0.0, Vec3::ZERO);
        for f in &self.faces {
            let a = f.verts[0] - c0;
            for w in f.verts[1..].windows(2) {
                let (b, c) = (w[0] - c0, w[1] - c0);
                let v = a.dot(b.cross(c)) / 6.0;
                vol += v;
                moment += v * (a + b + c) / 4.0;
            }
        }
        (vol, if vol > 0.0 { c0 + moment / vol } else { c0 })
    }

    pub fn volume(&self) -> f32 {
        self.mass_props().0
    }

    /// Surface area (m²).
    pub fn area(&self) -> f32 {
        self.faces.iter().map(|f| newell(&f.verts).length() * 0.5).sum()
    }

    /// Area seen along `dir` (unit): half the sum of every face's area times |n·dir|.
    pub fn projected_area(&self, dir: Vec3) -> f32 {
        self.faces.iter().map(|f| newell(&f.verts).length() * 0.5 * f.plane.n.dot(dir).abs()).sum::<f32>() * 0.5
    }

    /// Bounding sphere (centre, radius) round the centroid.
    pub fn sphere(&self) -> (Vec3, f32) {
        let c = self.mass_props().1;
        (c, self.verts().map(|v| v.distance(c)).fold(0.0, f32::max))
    }

    /// Farthest extent along `dir`.
    pub fn support(&self, dir: Vec3) -> f32 {
        self.verts().map(|v| v.dot(dir)).fold(f32::MIN, f32::max)
    }

    /// Signed distance-like value: ≤ 0 inside (exact on faces, an underestimate past edges).
    pub fn distance(&self, p: Vec3) -> f32 {
        self.faces.iter().map(|f| f.plane.dist(p)).fold(f32::MIN, f32::max)
    }

    /// Exact signed distance to the surface and the way out: outside, the distance to the nearest
    /// point (on the faces that see `p`) and the unit direction from it; inside, minus the depth
    /// under the nearest face and that face's normal.
    pub fn closest(&self, p: Vec3) -> (f32, Vec3) {
        let (mut deep, mut out_n) = (f32::MIN, Vec3::Y);
        let mut best = (f32::MAX, Vec3::Y);
        for f in &self.faces {
            let h = f.plane.dist(p);
            if h > deep {
                deep = h;
                out_n = f.plane.n;
            }
            if h <= 0.0 {
                continue;
            }
            // nearest point of this face: its plane if the foot falls inside, else an edge
            let foot = p - f.plane.n * h;
            let n = f.verts.len();
            let mut inside = true;
            for i in 0..n {
                let (a, b) = (f.verts[i], f.verts[(i + 1) % n]);
                if (b - a).cross(foot - a).dot(f.plane.n) < 0.0 {
                    inside = false;
                    break;
                }
            }
            let q = if inside {
                foot
            } else {
                let mut q = f.verts[0];
                let mut d2 = f32::MAX;
                for i in 0..n {
                    let (a, b) = (f.verts[i], f.verts[(i + 1) % n]);
                    let ab = b - a;
                    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                    let c = a + ab * t;
                    let e = c.distance_squared(p);
                    if e < d2 {
                        d2 = e;
                        q = c;
                    }
                }
                q
            };
            let d = q.distance(p);
            if d < best.0 {
                best = (d, if d > 1e-6 { (p - q) / d } else { f.plane.n });
            }
        }
        if deep <= 0.0 { (deep, out_n) } else { best }
    }

    /// First hit of the ray `o + t dir` with t in [0, max]: t and the face normal (t = 0 inside).
    pub fn raycast(&self, o: Vec3, dir: Vec3, max: f32) -> Option<(f32, Vec3)> {
        let (mut enter, mut exit, mut normal) = (f32::MIN, max, Vec3::ZERO);
        for f in &self.faces {
            let denom = f.plane.n.dot(dir);
            let dist = f.plane.d - f.plane.n.dot(o);
            if denom.abs() < 1e-12 {
                if dist < 0.0 {
                    return None;
                }
                continue;
            }
            let t = dist / denom;
            if denom < 0.0 {
                if t > enter {
                    enter = t;
                    normal = f.plane.n;
                }
            } else {
                exit = exit.min(t);
            }
            if enter > exit {
                return None;
            }
        }
        (exit >= 0.0).then(|| (enter.max(0.0), normal))
    }

    /// The pieces behind (n·p < d) and in front of a plane; None for a side with nothing.
    pub fn split(&self, pl: Plane) -> (Option<Convex>, Option<Convex>) {
        let mut back = Vec::with_capacity(self.faces.len() + 1);
        let mut front = Vec::with_capacity(self.faces.len() + 1);
        let mut cut: Vec<Vec3> = Vec::new();
        for f in &self.faces {
            let mut b = Vec::with_capacity(f.verts.len() + 2);
            let mut fr = Vec::with_capacity(f.verts.len() + 2);
            for (i, &p) in f.verts.iter().enumerate() {
                let q = f.verts[(i + 1) % f.verts.len()];
                let (dp, dq) = (pl.dist(p), pl.dist(q));
                if dp <= EPS {
                    b.push(p);
                }
                if dp >= -EPS {
                    fr.push(p);
                }
                if dp.abs() <= EPS {
                    cut.push(p);
                }
                if (dp < -EPS && dq > EPS) || (dp > EPS && dq < -EPS) {
                    let x = p + (q - p) * (dp / (dp - dq));
                    b.push(x);
                    fr.push(x);
                    cut.push(x);
                }
            }
            if b.len() >= 3 && newell(&b).length() > 1e-9 {
                back.push(Face { plane: f.plane, verts: b });
            }
            if fr.len() >= 3 && newell(&fr).length() > 1e-9 {
                front.push(Face { plane: f.plane, verts: fr });
            }
        }
        // the cap: the crossing points round their centre, counter-clockwise about n
        cut.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)).then(a.z.total_cmp(&b.z)));
        cut.dedup_by(|a, b| a.distance_squared(*b) < EPS * EPS * 4.0);
        let mut uniq: Vec<Vec3> = Vec::with_capacity(cut.len());
        for p in cut {
            if !uniq.iter().any(|q| q.distance_squared(p) < EPS * EPS * 4.0) {
                uniq.push(p);
            }
        }
        if uniq.len() >= 3 {
            let c = uniq.iter().copied().sum::<Vec3>() / uniq.len() as f32;
            let u = pl.n.any_orthonormal_vector();
            let v = pl.n.cross(u);
            uniq.sort_by(|a, b| {
                let (pa, pb) = (*a - c, *b - c);
                pa.dot(v).atan2(pa.dot(u)).total_cmp(&pb.dot(v).atan2(pb.dot(u)))
            });
            if newell(&uniq).length() > 1e-9 {
                if !back.is_empty() {
                    back.push(Face { plane: pl, verts: uniq.clone() });
                }
                if !front.is_empty() {
                    uniq.reverse();
                    front.push(Face { plane: Plane { n: -pl.n, d: -pl.d }, verts: uniq });
                }
            }
        }
        let keep = |faces: Vec<Face>| {
            let c = Convex { faces };
            (c.faces.len() >= 4 && c.volume() > 1e-9).then_some(c)
        };
        (keep(back), keep(front))
    }

    /// The same shape moved, turned and uniformly scaled.
    pub fn transformed(&self, t: Affine3A) -> Convex {
        let faces = self
            .faces
            .iter()
            .map(|f| {
                let verts: Vec<Vec3> = f.verts.iter().map(|&p| t.transform_point3(p)).collect();
                let n = t.transform_vector3(f.plane.n).normalize();
                Face { plane: Plane { n, d: n.dot(verts[0]) }, verts }
            })
            .collect();
        Convex { faces }
    }

    /// Flat-shaded triangles in material `m`, transformed by `t`.
    pub fn mesh_into(&self, mesh: &mut Mesh, t: Affine3A, m: Material) {
        for f in &self.faces {
            let n = t.transform_vector3(f.plane.n);
            let first = mesh.pos.len() as u32;
            for &p in &f.verts {
                mesh.vertex(t.transform_point3(p), n, m);
            }
            for k in 1..f.verts.len() as u32 - 1 {
                mesh.tri(first, first + k, first + k + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_have_their_volumes() {
        let b = Convex::cuboid(Vec3::new(1.0, 2.0, 0.5));
        assert!((b.volume() - 8.0).abs() < 1e-4);
        let (_, c) = b.mass_props();
        assert!(c.length() < 1e-5);
        let w = Convex::wedge(Vec3::ONE);
        assert!((w.volume() - 4.0).abs() < 1e-4);
        let p = Convex::prism(1.0, 1.0, 64, 1.0);
        assert!((p.volume() - std::f32::consts::TAU).abs() < 0.02);
        for f in &b.faces {
            for v in &f.verts {
                assert!(f.plane.dist(*v).abs() < 1e-5);
            }
            assert!(f.plane.n.dot(newell(&f.verts)) > 0.0, "a face winds inward");
        }
    }

    #[test]
    fn cuts_keep_the_volume_and_cut_again() {
        let b = Convex::cuboid(Vec3::ONE);
        let pl = Plane::through(Vec3::new(0.2, 0.1, 0.0), Vec3::new(1.0, 0.7, 0.3));
        let (back, front) = b.split(pl);
        let (back, front) = (back.unwrap(), front.unwrap());
        assert!((back.volume() + front.volume() - 8.0).abs() < 1e-3);
        assert!(back.verts().all(|v| pl.dist(v) < 1e-4));
        // a piece of a piece
        let (a, c) = front.split(Plane::through(front.mass_props().1, Vec3::Z));
        assert!((a.unwrap().volume() + c.unwrap().volume() - front.volume()).abs() < 1e-3);
        // a plane that misses leaves one side empty
        let (none, all) = b.split(Plane::through(Vec3::new(-5.0, 0.0, 0.0), Vec3::X));
        assert!(none.is_none() && (all.unwrap().volume() - 8.0).abs() < 1e-4);
    }

    #[test]
    fn rays_hit_the_near_face() {
        let b = Convex::cuboid(Vec3::ONE);
        let (t, n) = b.raycast(Vec3::new(-5.0, 0.2, 0.1), Vec3::X, 100.0).unwrap();
        assert!((t - 4.0).abs() < 1e-5 && n == Vec3::NEG_X);
        assert!(b.raycast(Vec3::new(-5.0, 3.0, 0.0), Vec3::X, 100.0).is_none());
        assert!(b.raycast(Vec3::new(-5.0, 0.0, 0.0), Vec3::X, 2.0).is_none());
        assert_eq!(b.raycast(Vec3::ZERO, Vec3::Y, 1.0).unwrap().0, 0.0);
        assert!(b.distance(Vec3::ZERO) < 0.0 && b.distance(Vec3::new(2.0, 0.0, 0.0)) > 0.0);
    }
}
