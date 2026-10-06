//! A skeleton and its poses. Bones are a tree, parents before their children; each has a place
//! at rest in its parent's frame. A pose is every bone's place in its parent's frame now; from
//! it, every bone's place in the model, and the matrix that takes what was skinned to the bone
//! at rest to where the bone has it.
use glam::{Affine3A, Mat3, Quat, Vec3};

/// A rigid placement: a turn, then a move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xf {
    pub pos: Vec3,
    pub rot: Quat,
}

impl Default for Xf {
    fn default() -> Xf {
        Xf::IDENTITY
    }
}

impl Xf {
    pub const IDENTITY: Xf = Xf { pos: Vec3::ZERO, rot: Quat::IDENTITY };

    pub fn new(pos: Vec3, rot: Quat) -> Xf {
        Xf { pos, rot }
    }

    /// `o` placed in this frame.
    pub fn then(self, o: Xf) -> Xf {
        Xf { pos: self.pos + self.rot * o.pos, rot: (self.rot * o.rot).normalize() }
    }

    pub fn point(self, p: Vec3) -> Vec3 {
        self.pos + self.rot * p
    }

    pub fn inverse(self) -> Xf {
        let r = self.rot.inverse();
        Xf { pos: r * -self.pos, rot: r }
    }

    /// Between this and `o` (the short way round).
    pub fn mix(self, o: Xf, t: f32) -> Xf {
        Xf { pos: self.pos.lerp(o.pos, t), rot: self.rot.slerp(o.rot, t) }
    }

    pub fn affine(self) -> Affine3A {
        Affine3A::from_rotation_translation(self.rot, self.pos)
    }
}

/// The turn that takes the frame (`u0`, `n0`) to (`u1`, `n1`): `u` along something, `n` a side
/// of it (made square to `u`). A limb's bone and the plane it bends in, before and after.
pub fn frame_turn(u0: Vec3, n0: Vec3, u1: Vec3, n1: Vec3) -> Quat {
    let basis = |u: Vec3, n: Vec3| {
        let u = u.normalize_or(Vec3::Y);
        let n = (n - u * n.dot(u)).normalize_or(u.any_orthonormal_vector());
        Mat3::from_cols(u, n, u.cross(n))
    };
    Quat::from_mat3(&(basis(u1, n1) * basis(u0, n0).transpose())).normalize()
}

#[derive(Clone, Debug, Default)]
pub struct Skeleton {
    pub names: Vec<String>,
    /// Parents come before their children.
    pub parent: Vec<Option<u16>>,
    /// Each bone at rest, in its parent's frame.
    pub rest: Vec<Xf>,
    /// Each bone at rest, in the model's.
    pub bind: Vec<Xf>,
    /// What takes a vertex of the model at rest into each bone's frame.
    pub inverse_bind: Vec<Affine3A>,
}

impl Skeleton {
    /// From bones in any order (name, parent, place at rest in its parent's frame, inverse bind
    /// matrix): sorted so that parents come first. The numbers the mesh's vertices know the
    /// bones by are the order given: `order[k]` is where bone `k` of the mesh went.
    pub fn new(bones: Vec<(String, Option<usize>, Xf, Affine3A)>) -> Result<(Skeleton, Vec<u16>), String> {
        let n = bones.len();
        if n == 0 || n > 255 {
            return Err(format!("un esqueleto de {n} huesos (de 1 a 255)"));
        }
        let mut depth = vec![0usize; n];
        for k in 0..n {
            let (mut at, mut d) = (bones[k].1, 0);
            while let Some(p) = at {
                d += 1;
                if d > n || p >= n {
                    return Err(format!("el hueso '{}' no cuelga de ninguna raíz", bones[k].0));
                }
                at = bones[p].1;
            }
            depth[k] = d;
        }
        let mut by_depth: Vec<usize> = (0..n).collect();
        by_depth.sort_by_key(|&k| (depth[k], k));
        let mut order = vec![0u16; n];
        for (new, &old) in by_depth.iter().enumerate() {
            order[old] = new as u16;
        }
        let mut sk = Skeleton::default();
        for &old in &by_depth {
            let (name, parent, rest, ibm) = &bones[old];
            sk.names.push(name.clone());
            sk.parent.push(parent.map(|p| order[p]));
            sk.rest.push(*rest);
            sk.inverse_bind.push(*ibm);
        }
        sk.bind = Pose::rest(&sk).model;
        Ok((sk, order))
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// The same, or why not (for whoever reads names from data).
    pub fn find(&self, name: &str) -> Result<usize, String> {
        self.bone(name).ok_or_else(|| format!("el esqueleto no tiene hueso '{name}'"))
    }

    /// Whether bone `b` is `of` or hangs from it.
    pub fn under(&self, b: usize, of: usize) -> bool {
        let mut at = Some(b);
        while let Some(k) = at {
            if k == of {
                return true;
            }
            at = self.parent[k].map(usize::from);
        }
        false
    }
}

#[derive(Clone, Debug, Default)]
pub struct Pose {
    /// Each bone in its parent's frame.
    pub local: Vec<Xf>,
    /// Each bone in the model's (`update` brings it up to date with `local`).
    pub model: Vec<Xf>,
}

impl Pose {
    pub fn rest(sk: &Skeleton) -> Pose {
        let mut p = Pose { local: sk.rest.clone(), model: vec![Xf::IDENTITY; sk.len()] };
        p.update(sk);
        p
    }

    /// Back to rest.
    pub fn reset(&mut self, sk: &Skeleton) {
        self.local.clone_from(&sk.rest);
        self.update(sk);
    }

    /// Every bone's place in the model from its place in its parent.
    pub fn update(&mut self, sk: &Skeleton) {
        self.update_from(sk, 0);
    }

    /// The same for bone `from` and the ones after it (what hangs from a bone comes after it).
    pub fn update_from(&mut self, sk: &Skeleton, from: usize) {
        for k in from..sk.len() {
            self.model[k] = match sk.parent[k] {
                Some(p) => self.model[usize::from(p)].then(self.local[k]),
                None => self.local[k],
            };
        }
    }

    fn parent_rot(&self, sk: &Skeleton, b: usize) -> Quat {
        sk.parent[b].map_or(Quat::IDENTITY, |p| self.model[usize::from(p)].rot)
    }

    /// Bone `b` turned by `q` (a turn in the model's axes) about its own head; what hangs from
    /// it goes with it.
    pub fn turn(&mut self, sk: &Skeleton, b: usize, q: Quat) {
        let rot = q * self.model[b].rot;
        self.aim(sk, b, rot);
    }

    /// Bone `b` turned to `rot` (in the model's axes); what hangs from it goes with it.
    pub fn aim(&mut self, sk: &Skeleton, b: usize, rot: Quat) {
        let rot = (self.parent_rot(sk, b).inverse() * rot).normalize();
        self.local[b].rot = rot;
        self.update_from(sk, b);
    }

    /// Bone `b` moved by `d` (model axes); what hangs from it goes with it.
    pub fn shift(&mut self, sk: &Skeleton, b: usize, d: Vec3) {
        let d = self.parent_rot(sk, b).inverse() * d;
        self.local[b].pos += d;
        self.update_from(sk, b);
    }

    /// Part of the way from this pose to `o`, bone by bone (`update` after).
    pub fn mix(&mut self, o: &Pose, t: f32) {
        for (a, b) in self.local.iter_mut().zip(&o.local) {
            *a = a.mix(*b, t);
        }
    }

    /// The matrices a mesh skinned to this skeleton is bent with, added to `out` (three rows of
    /// four each).
    pub fn palette(&self, sk: &Skeleton, out: &mut Vec<[f32; 12]>) {
        for k in 0..sk.len() {
            let m = glam::Mat4::from(self.model[k].affine() * sk.inverse_bind[k]).transpose();
            let (a, b, c) = (m.x_axis.to_array(), m.y_axis.to_array(), m.z_axis.to_array());
            out.push([a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], c[0], c[1], c[2], c[3]]);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A leg hanging from a hip: hip at 1 m, knee at 0.55 (a little ahead), ankle at 0.1, toe.
    pub fn leg() -> Skeleton {
        let b = |name: &str, parent: Option<usize>, pos: Vec3| (name.to_string(), parent, Xf::new(pos, Quat::IDENTITY), Affine3A::IDENTITY);
        let (mut sk, _) = Skeleton::new(vec![b("hip", None, Vec3::new(0.0, 1.0, 0.0)), b("knee", Some(0), Vec3::new(0.0, -0.45, 0.03)), b("ankle", Some(1), Vec3::new(0.0, -0.45, -0.03)), b("toe", Some(2), Vec3::new(0.0, -0.08, 0.15))]).unwrap();
        sk.inverse_bind = sk.bind.iter().map(|x| x.inverse().affine()).collect();
        sk
    }

    #[test]
    fn parents_come_first_whatever_the_order_given() {
        let b = |name: &str, parent: Option<usize>| (name.to_string(), parent, Xf::new(Vec3::Y, Quat::IDENTITY), Affine3A::IDENTITY);
        let (sk, order) = Skeleton::new(vec![b("hand", Some(2)), b("root", None), b("arm", Some(1))]).unwrap();
        assert_eq!(sk.names, ["root", "arm", "hand"]);
        assert_eq!(order, [2, 0, 1]);
        assert_eq!(sk.parent, [None, Some(0), Some(1)]);
        assert!((sk.bind[2].pos - Vec3::new(0.0, 3.0, 0.0)).length() < 1e-6);
        assert!(sk.under(2, 0) && !sk.under(0, 2));
        assert!(Skeleton::new(vec![b("a", Some(1)), b("b", Some(0))]).is_err(), "a loop is no tree");
    }

    #[test]
    fn at_rest_nothing_moves_and_a_turn_carries_what_hangs() {
        let sk = leg();
        let mut pose = Pose::rest(&sk);
        let mut pal = Vec::new();
        pose.palette(&sk, &mut pal);
        for m in &pal {
            let id = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
            assert!(m.iter().zip(&id).all(|(a, b)| (a - b).abs() < 1e-5), "{m:?}");
        }
        // the hip turned a quarter forward (about x): the ankle comes up level with the hip, ahead
        let (knee, ankle) = (sk.bone("knee").unwrap(), sk.bone("ankle").unwrap());
        pose.turn(&sk, 0, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        assert!((pose.model[0].pos - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-6, "it turns about its own head");
        assert!((pose.model[ankle].pos.y - 1.0).abs() < 0.07 && pose.model[ankle].pos.z > 0.85, "{:?}", pose.model[ankle].pos);
        // and a vertex skinned to the knee at rest goes with it
        pal.clear();
        pose.palette(&sk, &mut pal);
        let m = pal[knee];
        let p = sk.bind[knee].pos;
        let moved = Vec3::new(m[0] * p.x + m[1] * p.y + m[2] * p.z + m[3], m[4] * p.x + m[5] * p.y + m[6] * p.z + m[7], m[8] * p.x + m[9] * p.y + m[10] * p.z + m[11]);
        assert!((moved - pose.model[knee].pos).length() < 1e-5);
    }

    #[test]
    fn a_frame_turn_takes_one_frame_to_the_other() {
        let q = frame_turn(Vec3::Y, Vec3::X, Vec3::Z, Vec3::Y);
        assert!((q * Vec3::Y - Vec3::Z).length() < 1e-5 && (q * Vec3::X - Vec3::Y).length() < 1e-5);
    }
}
