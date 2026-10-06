//! Rigid bodies for loose structures (anchored and sleeping ones stand still for the others). Each
//! moves as one body with its mass, centre and inertia; it touches the ground at its lowest
//! corners (along the slope of the terrain there) and other structures where its corners are
//! inside their parts, or theirs inside it. Contacts are solved by sequential impulses with
//! accumulated clamping (friction in two directions, restitution for hard blows), then pushed
//! apart; a body that stays still sleeps until something strikes it.
//!
//! Nothing here depends on how fast everything goes. A body's speed is kept whole (as exact as
//! positions are) and a step works on what it adds to it, so the smallest push counts the same on
//! the ground and at orbital speed. And what a body touches stops it against the body that thing
//! is part of: what a ship holds is the ship, not something standing still in the world.
//!
//! Nor on where a body was made or has been. What pulls it and whose ground is under it is asked
//! every step of where it is now (`BodyRegistry::field`): carried from one moon to another it is
//! the other's from then on, and past every body's reach nothing pulls it and there is no ground.
//!
//! Finding the contacts is most of the work when many bodies meet (a fleet falling on another),
//! and each body's are its own: they are found on every core at once (rayon), each body against
//! the ground under it (a patch of it kept while it stays over it) and against the bodies round
//! it through their part indices (`bvh`), never part by part over the whole of both.
use super::{broadphase::Grid, schedule::Among, set::Structures, state::Structure};
use crate::body::{BodyId, BodyRegistry, Field};
use glam::{DVec3, Mat3, Quat, Vec3};
use rayon::prelude::*;

const ITERATIONS: usize = 10;
/// Penetration left alone (m), and the share of the rest pushed out per step.
const SLOP: f32 = 0.01;
const PUSH: f32 = 0.5;
const FRICTION: f32 = 0.6;
const RESTITUTION: f32 = 0.2;
/// Corners tried against the ground per body, and contacts kept per pair of bodies.
const CORNERS: usize = 8;
const PAIR_CONTACTS: usize = 8;
/// Part against part tests per pair of bodies and step at most (two hulls run into each other
/// touch in a few places; past this the deepest found so far hold them).
const PAIR_TESTS: usize = 256;
/// Slices per call at most: a slow frame takes longer steps instead of more of them (more work
/// per frame would make the next one slower still).
const MAX_SLICES: usize = 3;
/// Speeds under which a body counts as still (m/s, rad/s), and how long until it sleeps.
const STILL: (f32, f32) = (0.15, 0.3);
const SLEEP_AFTER: f32 = 0.6;
/// A body struck this hard (m/s) wakes.
const WAKE_SPEED: f32 = 0.6;
/// A body higher than this over the ground under it (m, plus a tenth of its size) is not looked
/// at corner by corner; one smaller than `SMALL` (m) rests on the plane of the ground under it
/// rather than on the ground under each of its corners.
const CLEAR: f64 = 0.6;
const SMALL: f32 = 1.5;
/// From this many bodies their contacts are found on every core.
const PARALLEL_FROM: usize = 6;

/// The ground under a body: whose it is, a point of it and its slope there, taken where the body
/// was.
#[derive(Clone, Copy)]
struct Patch {
    body: BodyId,
    at: DVec3,
    point: DVec3,
    n: DVec3,
}

#[derive(Clone, Copy, Default)]
struct Body {
    /// How fast it went as the step began (world, exact), and what the step has added to that:
    /// pushes add to the small number, so none is lost however fast it goes.
    v0: DVec3,
    v: Vec3,
    w: Vec3,
    inv_m: f32,
    inv_i: Mat3,
    com: DVec3,
}

#[derive(Clone, Copy)]
struct Contact {
    a: usize,
    /// The other body (index into the bodies), or None for the ground and still structures.
    b: Option<usize>,
    /// Structure the other side belongs to (to wake it), if any.
    other: Option<usize>,
    n: Vec3,
    depth: f32,
    ra: Vec3,
    rb: Vec3,
    /// How far from touching it is yet (m; 0 once it touches): it may close that much this step
    /// and no more (a speculative contact: what would go through a thin plate in one step is
    /// stopped at its face).
    gap: f32,
    t: [Vec3; 2],
    mn: f32,
    mt: [f32; 2],
    bounce: f32,
    jn: f32,
    jt: [f32; 2],
    /// A sprung leg's: which of its body's (1 on; 0: none, a hard contact), what the spring
    /// pushes with as it is pressed in now (N), how much more per metre (N/m) and its damper
    /// going in and coming out (N·s/m). Solved as a soft constraint (`gamma`: its compliance).
    leg: u16,
    spring: [f32; 2],
    damp: [f32; 2],
    gamma: f32,
    grip: f32,
}

/// World corners of every live part of `s`.
fn corners(s: &Structure, out: &mut Vec<DVec3>) {
    out.clear();
    for p in s.parts.iter().filter(|p| p.alive && p.collide && !s.on_leg(p.bone) && !s.gives(p.bone)) {
        out.extend(p.shape.verts().map(|v| s.to_world(p.local.transform_point3(v))));
    }
}

/// The corners of `s` that may touch the ground under it (world), lowest first and spread over
/// its footprint: the eight lowest of one foot would rock it from foot to foot (a ship on its gear
/// stands on all its feet). With many parts, only the parts reaching near its bottom are looked at
/// (their bounding spheres first, then their corners).
pub(crate) fn low_corners(s: &Structure, up: DVec3, low: &mut Vec<(f32, u32)>, pts: &mut Vec<DVec3>) {
    if s.parts.len() <= CORNERS * 4 {
        corners(s, pts);
    } else {
        let mut local = Vec::new();
        near_bottom(s, s.rot.inverse() * up.as_vec3(), low, &mut local);
        pts.clear();
        pts.extend(local.iter().map(|&v| s.to_world(v)));
    }
    let kept = lowest_spread(pts, up, f64::from(s.radius) * 0.12, CORNERS);
    pts.truncate(kept);
}

/// The corners (in `s`'s frame) of the parts of `s` reaching near its bottom, `up` being up in
/// its frame: their bounding spheres first, then their corners.
fn near_bottom(s: &Structure, up: Vec3, low: &mut Vec<(f32, u32)>, out: &mut Vec<Vec3>) {
    low.clear();
    for (i, p) in s.parts.iter().enumerate().filter(|(_, p)| p.alive && p.collide && !s.on_leg(p.bone) && !s.gives(p.bone)) {
        low.push((p.center.dot(up) - p.radius, i as u32));
    }
    low.sort_unstable_by(|x, y| x.0.total_cmp(&y.0));
    let height = |i: u32| {
        let p = &s.parts[i as usize];
        p.shape.verts().map(|v| p.local.transform_point3(v).dot(up)).fold(f32::MAX, f32::min)
    };
    // its true bottom: parts in order of how low they might reach, until none can be lower
    let mut bottom = f32::MAX;
    for &(b, i) in low.iter() {
        if b > bottom {
            break;
        }
        bottom = bottom.min(height(i));
    }
    // and every part reaching within the ground's roughness of it
    let band = (s.radius * 0.15).max(0.5);
    out.clear();
    for &(b, i) in low.iter() {
        if b > bottom + band {
            break;
        }
        let p = &s.parts[i as usize];
        out.extend(p.shape.verts().map(|v| p.local.transform_point3(v)));
    }
}

/// Put first in `pts` its lowest points (along `up`) no nearer each other than `spread` seen
/// from above, `most` of them at most; how many.
fn lowest_spread(pts: &mut [DVec3], up: DVec3, spread: f64, most: usize) -> usize {
    pts.sort_unstable_by(|x, y| x.dot(up).total_cmp(&y.dot(up)));
    let mut kept = 0;
    for i in 0..pts.len() {
        if kept == most {
            break;
        }
        let p = pts[i];
        let flat = |q: DVec3| {
            let d = q - p;
            (d - up * d.dot(up)).length()
        };
        if pts[..kept].iter().all(|&q| flat(q) > spread) {
            pts[kept] = p;
            kept += 1;
        }
    }
    kept
}

/// What of a body may touch the ground under it, kept from step to step (finding it means going
/// through its parts: most of what a body on the ground costs): the lowest of its corners, in its
/// own frame, for the way up it was found with. Found again when it has turned more than
/// `LOW_TURN` from that, moved a part or lost one.
#[derive(Clone, Debug, Default)]
struct Low {
    version: u64,
    pose: u64,
    up: Vec3,
    pts: Vec<Vec3>,
    /// The ground under each corner: where the corner was when it was asked for, and the ground
    /// there (world).
    under: Vec<(DVec3, DVec3)>,
    /// The same under the foot of each of its sprung legs.
    feet: Vec<(DVec3, DVec3)>,
}

/// Corners kept per body (several times the ones that end up its contacts: each is measured
/// against the ground), and the cosine of the turn they are good for (two degrees and a half).
const LOW_KEPT: usize = 32;
const LOW_TURN: f32 = 0.99905;
/// A corner moved this far over the ground (m) asks for the ground under it again.
const ASK_AGAIN: f64 = 0.03;
/// The most a body is pushed out of the ground in a step (m): one found deep in it comes out
/// over a few steps, not thrown clear.
const MAX_PUSH: f32 = 0.06;

impl Low {
    /// The corners for `s` as it stands (`up` in its frame), found again if these are stale.
    fn refresh(&mut self, s: &Structure, up: Vec3) {
        if !self.pts.is_empty() && self.version == s.version && self.pose == s.stance && self.up.dot(up) > LOW_TURN {
            return;
        }
        let (mut low, mut local) = (Vec::new(), Vec::new());
        if s.parts.len() <= CORNERS * 4 {
            for p in s.parts.iter().filter(|p| p.alive && p.collide && !s.on_leg(p.bone) && !s.gives(p.bone)) {
                local.extend(p.shape.verts().map(|v| p.local.transform_point3(v)));
            }
        } else {
            near_bottom(s, up, &mut low, &mut local);
        }
        // the lowest, half as far apart as the ones tried will be
        let mut pts: Vec<DVec3> = local.iter().map(|v| v.as_dvec3()).collect();
        let kept = lowest_spread(&mut pts, up.as_dvec3(), f64::from(s.radius) * 0.06, LOW_KEPT);
        self.pts.clear();
        self.pts.extend(pts[..kept].iter().map(|p| p.as_vec3()));
        self.under.clear();
        self.under.resize(kept, (DVec3::splat(f64::MAX), DVec3::ZERO));
        (self.version, self.pose, self.up) = (s.version, s.stance, up);
    }
}

#[derive(Default)]
pub struct Physics {
    grid: Option<Grid>,
    bodies: Vec<Body>,
    /// Body of each structure, and structure of each body.
    of: Vec<Option<usize>>,
    owner: Vec<usize>,
    /// The body each structure is part of to whatever touches it: its own, or that of what holds
    /// it (at any remove). None: it stands still.
    part_of: Vec<Option<usize>>,
    contacts: Vec<Contact>,
    near: Vec<u32>,
    push: Vec<(Vec3, f32)>,
    /// Bodies with a contact this step.
    touching: Vec<bool>,
    riding: Vec<bool>,
    /// The ground under each body (by structure id), probed again when it moves off it.
    patches: std::collections::HashMap<u64, Patch>,
    /// What of each body may touch it (by structure id).
    lows: std::collections::HashMap<u64, Low>,
    /// Bodies with a contact against the ground this step.
    grounded: Vec<bool>,
    /// What holds where each body is now (what pulls it, whose ground is under it), and whether
    /// its systems push it hard enough to move it there.
    fields: Vec<Field>,
    shoved: Vec<bool>,
}

impl Structures {
    /// Every loose structure, `dt` on, all together (tests and tools; the scheduler steps by level).
    pub fn step(&mut self, dt: f32, bodies: &BodyRegistry) {
        let all: Vec<usize> = (0..self.list.len()).collect();
        self.physics(&all, dt, bodies);
        self.follow();
    }

    /// The structures `idx` (the ones awake among them) `dt` on, together, in slices of at most a
    /// sixtieth of a second.
    pub fn physics(&mut self, idx: &[usize], dt: f32, bodies: &BodyRegistry) {
        self.physics_among(idx, dt, bodies, &mut []);
    }

    /// The same, and after each slice what lives among them takes it too (`Among`): what is held
    /// is first put where its holder went, so everything it meets is of that instant.
    pub fn physics_among(&mut self, idx: &[usize], dt: f32, bodies: &BodyRegistry, among: &mut [&mut dyn Among]) {
        let n = slices(dt);
        let mut ph = std::mem::take(&mut self.physics);
        for _ in 0..n {
            for item in among.iter_mut() {
                item.before(self);
            }
            ph.slice(&mut self.list, idx, dt / n as f32, bodies);
            if dt > 0.0 {
                self.follow();
                for a in among.iter_mut() {
                    a.slice(self, bodies, f64::from(dt / n as f32));
                }
            }
        }
        self.physics = ph;
    }
}

/// How many slices `dt` s of the world are taken in: of a sixtieth of a second at most, and no
/// more than `MAX_SLICES` of them.
pub fn slices(dt: f32) -> usize {
    ((dt * 60.0).ceil() as usize).clamp(1, MAX_SLICES)
}

/// Share of its weight (and of its weight at arm's length, its radius) under which what a
/// structure's systems push it with cannot move it off where it rests (it neither lifts, slides
/// nor tips): a wheel trimming, a fan, an engine idling. Under it a body at rest sleeps on; where
/// nothing weighs, anything counts.
const PUSH_FROM: f32 = 0.2;

/// Whether `s`'s systems push it hard enough to move it (`PUSH_FROM`), where it is now.
fn pushed(s: &Structure, bodies: &BodyRegistry) -> bool {
    if s.force == Vec3::ZERO && s.torque == Vec3::ZERO {
        return false;
    }
    shoved(s, bodies.field(s.pos).g() as f32)
}

/// The same where what pulls it there is known (`g`, m/s²).
fn shoved(s: &Structure, g: f32) -> bool {
    let least = PUSH_FROM * s.mass * g;
    (s.force != Vec3::ZERO || s.torque != Vec3::ZERO) && (s.force.length_squared() > least * least || s.torque.length_squared() > (least * s.radius) * (least * s.radius))
}

impl Physics {
    fn slice(&mut self, list: &mut [Structure], idx: &[usize], dt: f32, bodies: &BodyRegistry) {
        // whatever its systems push wakes
        for &k in idx {
            let s = &mut list[k];
            if !s.anchored && s.held.is_none() && pushed(s, bodies) {
                s.resting = false;
                s.still = 0.0;
            }
            // (what stands still speeds up not at all)
            if s.anchored || s.resting {
                s.acc = DVec3::ZERO;
            }
        }
        // the bodies that move
        self.of.clear();
        self.of.resize(list.len(), None);
        self.bodies.clear();
        self.owner.clear();
        for &k in idx {
            let s = &list[k];
            if s.anchored || s.resting || s.held.is_some() || dt <= 0.0 {
                continue;
            }
            let r = Mat3::from_quat(s.rot);
            self.of[k] = Some(self.bodies.len());
            self.owner.push(k);
            self.bodies.push(Body { v0: s.vel, v: Vec3::ZERO, w: s.spin, inv_m: 1.0 / s.mass.max(0.01), inv_i: r * s.inv_inertia * r.transpose(), com: s.to_world(s.com) });
        }
        if self.bodies.is_empty() {
            return;
        }
        // what is held is part of what holds it (twice: what is held may hold something itself)
        self.part_of.clear();
        self.part_of.extend_from_slice(&self.of);
        for _ in 0..2 {
            for k in 0..list.len() {
                if let Some(h) = list[k].held
                    && self.part_of[k].is_none()
                {
                    let by = list.binary_search_by_key(&h.by, |s| s.id).ok().or_else(|| list.iter().position(|s| s.id == h.by));
                    self.part_of[k] = by.and_then(|j| self.part_of[j]);
                }
            }
        }
        let mut grid = self.grid.take().unwrap_or_else(|| Grid::new(16.0));
        grid.build(list);
        // what holds where each is now: asked once per body and step
        self.fields.clear();
        self.shoved.clear();
        for b in 0..self.bodies.len() {
            let f = bodies.field(self.bodies[b].com);
            self.shoved.push(shoved(&list[self.owner[b]], f.g() as f32));
            self.fields.push(f);
        }
        // what pulls it there, and what its systems push with
        for (k, s) in list.iter().enumerate() {
            if let Some(b) = self.of[k] {
                let pull = self.fields[b].pull;
                let body = &mut self.bodies[b];
                body.v += (pull * f64::from(dt)).as_vec3();
                body.v += s.force * body.inv_m * dt;
                body.w += body.inv_i * s.torque * dt;
            }
        }
        // contacts: what each body may touch first (the ground under it, the bodies round it),
        // then each body's contacts on its own
        self.contacts.clear();
        if self.patches.len() > 4 * self.bodies.len() + 64 {
            self.patches.clear();
            self.lows.clear();
        }
        let mut tasks: Vec<(usize, usize, Option<Patch>, Vec<u32>, Low)> = Vec::with_capacity(self.bodies.len());
        for (k, s) in list.iter().enumerate() {
            let Some(a) = self.of[k] else { continue };
            let com = self.bodies[a].com;
            // the ground under it, if there is any where it is now: probed where it is, and
            // again once it has moved off that or it is another body's ground it is over
            let patch = self.fields[a].ground.map(|ground| {
                let bd = bodies.get(ground);
                let up = bd.up(com);
                let reach = (f64::from(s.radius) * 0.5).max(1.0);
                let flat = |d: DVec3| (d - up * d.dot(up)).length();
                match self.patches.get(&s.id) {
                    Some(p) if p.body == ground && flat(p.at - com) < reach => *p,
                    _ => {
                        // the plane of the ground at the body's own scale (a ship does not lean
                        // with the pebble under its middle): across it both ways, half its size out
                        let probe = |p: DVec3| bd.center + bd.up(p) * (bd.radius + bd.height_at(bd.up(p), 0.25));
                        let side = up.any_orthonormal_vector() * (f64::from(s.radius) * 0.5).max(0.5);
                        let fwd = up.cross(side);
                        let n = (probe(com + side) - probe(com - side)).cross(probe(com + fwd) - probe(com - fwd)).normalize_or(up);
                        let p = Patch { body: ground, at: com, point: probe(com), n: if n.dot(up) < 0.0 { -n } else { n } };
                        self.patches.insert(s.id, p);
                        p
                    }
                }
            });
            let c = s.to_world(s.center);
            grid.along(c, c, f64::from(s.radius), &mut self.near);
            let near: Vec<u32> = self
                .near
                .iter()
                .copied()
                .filter(|&j| {
                    let j = j as usize;
                    let o = &list[j];
                    // (what it holds is part of it: it does not strike it)
                    j != k && !(self.of[j].is_some() && j < k) && self.part_of[j] != Some(a) && o.to_world(o.center).distance(c) <= f64::from(s.radius + o.radius)
                })
                .collect();
            tasks.push((k, a, patch, near, self.lows.remove(&s.id).unwrap_or_default()));
        }
        let (of, moving) = (&self.part_of, &self.bodies);
        let list_ref: &[Structure] = list;
        let find = |(k, a, patch, near, low): &mut (usize, usize, Option<Patch>, Vec<u32>, Low)| -> (Vec<Contact>, Vec<f32>) {
            let (mut out, mut pressed) = (Vec::new(), Vec::new());
            if let Some(patch) = patch {
                ground(&list_ref[*k], *a, patch, bodies, moving, low, &mut out);
            }
            legs(list_ref, *k, *a, patch.as_ref(), near, of, bodies, moving, low, dt, &mut out, &mut pressed);
            for &j in near.iter() {
                pair(list_ref, *k, *a, j as usize, of[j as usize], moving, &mut out);
            }
            (out, pressed)
        };
        let found: Vec<(Vec<Contact>, Vec<f32>)> = if tasks.len() >= PARALLEL_FROM { tasks.par_iter_mut().map(find).collect() } else { tasks.iter_mut().map(find).collect() };
        let mut pressed: Vec<(usize, Vec<f32>)> = Vec::new();
        for ((k, ..), (f, p)) in tasks.iter().zip(found) {
            self.contacts.extend(f);
            if !p.is_empty() {
                pressed.push((*k, p));
            }
        }
        for (k, _, _, _, low) in tasks {
            if !low.pts.is_empty() {
                self.lows.insert(list[k].id, low);
            }
        }
        self.grid = Some(grid);
        // prepare
        for c in &mut self.contacts {
            let a = &self.bodies[c.a];
            let (ia, ib) = (a.inv_i, c.b.map(|b| self.bodies[b].inv_i));
            let k = |r: Vec3, rb: Vec3, d: Vec3| {
                let x = r.cross(d);
                let mut k = a.inv_m + (ia * x).dot(x);
                if let (Some(b), Some(ib)) = (c.b, ib) {
                    let y = rb.cross(d);
                    k += self.bodies[b].inv_m + (ib * y).dot(y);
                }
                1.0 / k.max(1e-9)
            };
            c.t[0] = c.n.any_orthonormal_vector();
            c.t[1] = c.n.cross(c.t[0]);
            c.mn = k(c.ra, c.rb, c.n);
            c.mt = [k(c.ra, c.rb, c.t[0]), k(c.ra, c.rb, c.t[1])];
            let vn = Physics::relative(&self.bodies, c).dot(c.n);
            if c.leg > 0 {
                // a spring and a damper as a soft constraint (Catto, "Soft Constraints", GDC
                // 2011): what it gives over the step is worked out with the speed at its end,
                // so it neither rings nor blows up however stiff the leg or long the step
                let [f, rate] = c.spring;
                // (oil through an orifice: the faster it is driven, the harder it resists)
                let damp = c.damp[usize::from(vn > 0.0)] * (1.0 + vn.abs() / ORIFICE);
                let stiff = damp + dt * rate;
                c.gamma = 1.0 / (dt * stiff).max(1e-9);
                c.bounce = f / stiff.max(1e-9);
                c.mn = 1.0 / (1.0 / c.mn + c.gamma);
                continue;
            }
            c.bounce = if c.gap > 0.0 {
                -c.gap / dt
            } else if vn < -1.0 {
                -RESTITUTION * vn
            } else {
                0.0
            };
        }
        // solve
        for _ in 0..ITERATIONS {
            for i in 0..self.contacts.len() {
                let mut c = self.contacts[i];
                let vn = Physics::relative(&self.bodies, &c).dot(c.n);
                let jn = (c.jn + c.mn * (c.bounce - vn - c.gamma * c.jn)).max(0.0);
                Physics::apply(&mut self.bodies, &c, c.n * (jn - c.jn));
                c.jn = jn;
                for t in 0..2 {
                    let vt = Physics::relative(&self.bodies, &c).dot(c.t[t]);
                    let limit = c.grip * c.jn;
                    let jt = (c.jt[t] - c.mt[t] * vt).clamp(-limit, limit);
                    Physics::apply(&mut self.bodies, &c, c.t[t] * (jt - c.jt[t]));
                    c.jt[t] = jt;
                }
                self.contacts[i] = c;
            }
        }
        // the legs: how far in each was found, what each carries
        for (k, p) in pressed {
            for (sp, x) in list[k].springs.iter_mut().zip(p) {
                sp.x = x;
                sp.load = 0.0;
            }
        }
        for c in self.contacts.iter().filter(|c| c.leg > 0) {
            if let Some(sp) = list[self.owner[c.a]].springs.get_mut(usize::from(c.leg) - 1) {
                sp.load = c.jn / dt;
            }
        }
        // move, push apart, sleep
        for (k, s) in list.iter_mut().enumerate() {
            let Some(b) = self.of[k] else { continue };
            let bd = self.bodies[b];
            s.vel = bd.v0 + bd.v.as_dvec3();
            // (how its speed changed this step: what it carries is not pulled that much)
            s.acc = bd.v.as_dvec3() / f64::from(dt);
            s.spin = bd.w;
            s.rot = (Quat::from_scaled_axis(bd.w * dt) * s.rot).normalize();
            let com = bd.com + s.vel * f64::from(dt);
            s.pos = com - (s.rot * s.com).as_dvec3();
        }
        // out of the ground and of each other: once per body, by its deepest contact (pushing at
        // every corner would push it out several times over)
        self.push.clear();
        self.push.resize(self.bodies.len(), (Vec3::ZERO, 0.0));
        for c in &self.contacts {
            // (out of the ground, little by little; out of another body, at once: it is thin)
            let most = if c.b.is_none() && c.other.is_none() { MAX_PUSH } else { f32::MAX };
            let push = ((c.depth - SLOP).max(0.0) * PUSH).min(most);
            let ia = self.bodies[c.a].inv_m;
            let ib = c.b.map_or(0.0, |b| self.bodies[b].inv_m);
            let (pa, pb) = (push * ia / (ia + ib), push * ib / (ia + ib));
            if pa > self.push[c.a].1 {
                self.push[c.a] = (c.n, pa);
            }
            if let Some(b) = c.b
                && pb > self.push[b].1
            {
                self.push[b] = (-c.n, pb);
            }
        }
        for (b, &(n, p)) in self.push.iter().enumerate() {
            if p > 0.0 {
                list[self.owner[b]].pos += (n * p).as_dvec3();
            }
        }
        for c in &self.contacts {
            // struck hard, or touched by a body its systems push (a ship lifting off under its
            // loose cargo, or against another): a sleeper wakes
            if let Some(o) = c.other
                && c.b.is_none()
                && !list[o].anchored
                && (c.jn * self.bodies[c.a].inv_m > WAKE_SPEED || self.shoved[c.a])
            {
                list[o].resting = false;
                list[o].still = 0.0;
            }
        }
        self.touching.clear();
        self.touching.resize(self.bodies.len(), false);
        // (and what rides on a pushed body does not go to sleep on it: asleep it would stand
        // still in the world and hold it)
        self.riding.clear();
        self.riding.resize(self.bodies.len(), false);
        self.grounded.clear();
        self.grounded.resize(self.bodies.len(), false);
        for c in &self.contacts {
            self.touching[c.a] = true;
            if c.b.is_none() && c.other.is_none() {
                self.grounded[c.a] = true;
            }
            if let Some(b) = c.b {
                self.touching[b] = true;
                if self.shoved[c.a] || self.shoved[b] {
                    self.riding[c.a] = true;
                    self.riding[b] = true;
                }
            }
        }
        for (k, s) in list.iter_mut().enumerate() {
            let Some(b) = self.of[k] else { continue };
            let touching = self.touching[b];
            s.grounded = self.grounded[b];
            let (v, w) = (s.vel.length() as f32, s.spin.length());
            if touching && v < STILL.0 && w < STILL.1 {
                s.still += dt;
            } else if touching && v < STILL.0 * 4.0 && w < STILL.1 * 2.0 {
                s.still += dt * 0.25;
            } else {
                s.still = (s.still - dt).max(0.0);
            }
            if self.riding[b] {
                s.still = 0.0;
            }
            if s.still > SLEEP_AFTER && !self.shoved[b] {
                s.resting = true;
                s.vel = DVec3::ZERO;
                s.spin = Vec3::ZERO;
                s.acc = DVec3::ZERO;
            }
        }
    }

    /// Velocity of a's contact point relative to b's.
    fn relative(bodies: &[Body], c: &Contact) -> Vec3 {
        let a = &bodies[c.a];
        match c.b {
            // (their difference first: two that go fast together differ by little)
            Some(b) => {
                let b = &bodies[b];
                (a.v0 - b.v0).as_vec3() + (a.v - b.v) + a.w.cross(c.ra) - b.w.cross(c.rb)
            }
            None => a.v0.as_vec3() + a.v + a.w.cross(c.ra),
        }
    }

    fn apply(bodies: &mut [Body], c: &Contact, j: Vec3) {
        let a = &mut bodies[c.a];
        a.v += j * a.inv_m;
        a.w += a.inv_i * c.ra.cross(j);
        if let Some(b) = c.b {
            let b = &mut bodies[b];
            b.v -= j * b.inv_m;
            b.w -= b.inv_i * c.rb.cross(j);
        }
    }
}

fn contact(bodies: &[Body], a: usize, b: Option<usize>, other: Option<usize>, p: DVec3, n: Vec3, depth: f32) -> Contact {
    let ra = (p - bodies[a].com).as_vec3();
    let rb = b.map_or(Vec3::ZERO, |b| (p - bodies[b].com).as_vec3());
    // (under 0: not touching yet, that far off)
    Contact { a, b, other, n, depth: depth.max(0.0), gap: (-depth).max(0.0), ra, rb, t: [Vec3::ZERO; 2], mn: 0.0, mt: [0.0; 2], bounce: 0.0, jn: 0.0, jt: [0.0; 2], leg: 0, spring: [0.0; 2], damp: [0.0; 2], gamma: 0.0, grip: FRICTION }
}

/// The speed (m/s) at which a leg's damper resists twice what its figure says: its oil goes
/// through an orifice, so what it resists with grows with how fast it is driven.
const ORIFICE: f32 = 1.5;
/// How fast a leg with nothing under it comes out again (m/s).
const LEG_OUT: f32 = 1.2;
/// The least of a leg's axis along what it is pressed against that counts (a leg lying nearly
/// flat on the ground is not pressed in by it).
const LEG_LEAN: f32 = 0.35;

/// The sprung legs of `s` (body `a`): each pressed in by whatever is under its foot — the
/// ground, or the nearest of the structures round it (`near`) — as far as that reaches up its
/// stroke; a soft contact where it stands, and a hard one past the end of its stroke. `out`
/// gets the contacts, `pressed` how far in each leg is found.
#[allow(clippy::too_many_arguments)]
fn legs(list: &[Structure], k: usize, a: usize, patch: Option<&Patch>, near: &[u32], of: &[Option<usize>], registry: &BodyRegistry, bodies: &[Body], low: &mut Low, dt: f32, out: &mut Vec<Contact>, pressed: &mut Vec<f32>) {
    let s = &list[k];
    pressed.clear();
    if s.springs.is_empty() {
        return;
    }
    low.feet.resize(s.springs.len(), (DVec3::splat(f64::MAX), DVec3::ZERO));
    let centre = s.to_world(s.center);
    // (no ground where it is, or high over it: only what stands under each foot is looked at)
    let ground = patch.filter(|p| (centre - p.point).dot(p.n) - f64::from(s.radius) <= CLEAR + f64::from(s.radius) * 0.1).map(|p| (p, registry.get(p.body)));
    for (i, sp) in s.springs.iter().enumerate() {
        if !sp.active {
            pressed.push(0.0);
            continue;
        }
        let foot = s.to_world(sp.foot);
        let axis = s.rot * sp.axis;
        // the ground under the foot (asked for again only once the foot has moved)
        let mut best: Option<(f32, Vec3, Option<usize>, Option<usize>)> = None;
        if let Some((patch, bd)) = ground {
            let up = bd.up(bodies[a].com);
            let (at, under) = &mut low.feet[i];
            let d = foot - *at;
            if !((d - up * d.dot(up)).length_squared() < ASK_AGAIN * ASK_AGAIN) {
                (*at, *under) = (foot, bd.center + bd.up(foot) * (bd.radius + bd.height_at(bd.up(foot), 0.25)));
            }
            let n = patch.n.as_vec3();
            let lean = axis.dot(n);
            if lean > LEG_LEAN {
                let x = (*under - foot).dot(patch.n) as f32 / lean;
                if x > -0.02 {
                    best = Some((x, n, None, None));
                }
            }
        }
        // and what stands under it: the first thing down the leg's own line
        for &j in near {
            let o = &list[j as usize];
            let top = o.to_local(foot + (axis * sp.stroke).as_dvec3());
            let dir = o.rot.inverse() * -axis;
            if let Some(hit) = o.raycast(top, dir, sp.stroke + 0.02) {
                let x = sp.stroke - hit.t;
                let n = o.rot * hit.normal;
                if axis.dot(n) > LEG_LEAN && best.is_none_or(|b| x > b.0) {
                    best = Some((x, n, of[j as usize], Some(j as usize)));
                }
            }
        }
        let Some((x, n, b, other)) = best.filter(|b| b.0 > 0.0) else {
            // nothing under it: it comes out at its own pace
            pressed.push((sp.x - LEG_OUT * dt).max(0.0));
            continue;
        };
        let x_in = x.min(sp.stroke);
        pressed.push(x_in);
        let at = foot + (axis * x_in).as_dvec3();
        let (f, rate) = sp.force(x_in);
        let mut c = contact(bodies, a, b, other, at, n, 0.0);
        c.leg = i as u16 + 1;
        c.spring = [f, rate];
        c.damp = sp.damp;
        c.grip = sp.grip;
        out.push(c);
        // past the end of its stroke it is a hard contact: metal on metal
        if x > sp.stroke {
            out.push(contact(bodies, a, b, other, at, n, (x - sp.stroke) * axis.dot(n)));
        }
    }
}

/// The corners of `s` (body `a`) deepest in the ground under it (`patch`), along its slope.
///
/// Of the corners kept for the way it stands (`Low`), each is measured against the ground under
/// it (a wide body on rough ground rests where the ground is under each corner: the lowest of
/// its corners are not the ones that touch; a small one, on the plane of the ground under it),
/// and the deepest ones, spread over its footprint, are its contacts. The ground under a corner
/// is asked for again only once the corner has moved: a body that hardly moves asks nothing.
fn ground(s: &Structure, a: usize, patch: &Patch, registry: &BodyRegistry, bodies: &[Body], low: &mut Low, out: &mut Vec<Contact>) {
    // high over it: nothing to look at
    let centre = s.to_world(s.center);
    if (centre - patch.point).dot(patch.n) - f64::from(s.radius) > CLEAR + f64::from(s.radius) * 0.1 {
        return;
    }
    let bd = registry.get(patch.body);
    let up = bd.up(bodies[a].com);
    let probe = |p: DVec3| bd.center + bd.up(p) * (bd.radius + bd.height_at(bd.up(p), 0.25));
    low.refresh(s, s.rot.inverse() * patch.n.as_vec3());
    let small = s.radius < SMALL;
    let n = low.pts.len().min(LOW_KEPT);
    let mut found = [(0.0f32, DVec3::ZERO); LOW_KEPT];
    for i in 0..n {
        let p = s.to_world(low.pts[i]);
        let depth = if small {
            (patch.point - p).dot(patch.n)
        } else {
            let (at, under) = &mut low.under[i];
            let d = p - *at;
            if !((d - up * d.dot(up)).length_squared() < ASK_AGAIN * ASK_AGAIN) {
                (*at, *under) = (p, probe(p));
            }
            (*under - p).dot(patch.n)
        };
        found[i] = (depth as f32, p);
    }
    let found = &mut found[..n];
    found.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));
    let spread = f64::from(s.radius) * 0.12;
    let mut kept = 0;
    for i in 0..n {
        let (depth, p) = found[i];
        if kept == CORNERS || depth <= -SLOP {
            break;
        }
        let flat = |q: DVec3| {
            let d = q - p;
            (d - patch.n * d.dot(patch.n)).length()
        };
        if found[..kept].iter().all(|f| flat(f.1) > spread) {
            found[kept] = (depth, p);
            kept += 1;
            out.push(contact(bodies, a, None, None, p, patch.n.as_vec3(), depth.max(0.0)));
        }
    }
}

/// Contacts between structures `k` (body `a`) and `j` (body `b`, if it moves), part against
/// part: the separating axis of least overlap among the faces of both is the normal (so a block
/// on a block rests flat), and the corners of each lying in or against the other are the points.
/// Only the parts of `k` inside `j`'s bounds are looked at, each against the parts of `j` round
/// it (their indices).
fn pair(list: &[Structure], k: usize, a: usize, j: usize, b: Option<usize>, bodies: &[Body], out: &mut Vec<Contact>) {
    let mut pair: Vec<Contact> = Vec::new();
    let (mut solid_p, mut solid_q) = (Solid::default(), Solid::default());
    let (sk, sj) = (&list[k], &list[j]);
    let origin = sk.to_world(sk.center);
    // k's frame into j's
    let turn = sj.rot.inverse() * sk.rot;
    let shift = sj.rot.inverse() * (sk.pos - sj.pos).as_vec3();
    let into_j = |p: Vec3| turn * p + shift;
    let mut tests = 0;
    sk.index.sphere(sk.to_local(sj.to_world(sj.center)), sj.radius, |qi| {
        let q = &sk.parts[qi as usize];
        if !q.alive || !q.collide || tests >= PAIR_TESTS || sk.on_leg(q.bone) {
            return;
        }
        let cq = into_j(q.center);
        sj.index.sphere(cq, q.radius, |pi| {
            let p = &sj.parts[pi as usize];
            if !p.alive || !p.collide || tests >= PAIR_TESTS || sj.on_leg(p.bone) || p.center.distance_squared(cq) > (p.radius + q.radius) * (p.radius + q.radius) {
                return;
            }
            tests += 1;
            solid(sj, p, origin, &mut solid_p);
            solid(sk, q, origin, &mut solid_q);
            let Some((n, overlap)) = separating(&solid_p, &solid_q) else {
                return;
            };
            // the corners of q in (or flush against) p, and of p in q
            let top = solid_p.0.iter().map(|v| v.dot(n)).fold(f32::MIN, f32::max);
            let bottom = solid_q.0.iter().map(|v| v.dot(n)).fold(f32::MAX, f32::min);
            // (a corner gone past the far face of a thin part is still in it: the face it came
            // in by holds it, as a half-space, as deep as the two overlap; one not yet in is a
            // contact that may close its gap and no more)
            for &v in &solid_q.0 {
                if inside(&solid_p.1, v, n) {
                    let depth = (top - v.dot(n)).clamp(-TOUCH, overlap);
                    pair.push(contact(bodies, a, b, Some(j), origin + v.as_dvec3(), n, depth));
                }
            }
            for &v in &solid_p.0 {
                if inside(&solid_q.1, v, -n) {
                    let depth = (v.dot(n) - bottom).clamp(-TOUCH, overlap);
                    pair.push(contact(bodies, a, b, Some(j), origin + v.as_dvec3(), n, depth));
                }
            }
        });
    });
    pair.sort_unstable_by(|x, y| y.depth.total_cmp(&x.depth));
    pair.truncate(PAIR_CONTACTS);
    out.extend_from_slice(&pair);
}

/// How close to a solid counts as touching it (m).
const TOUCH: f32 = 0.02;

/// A part as corners and face planes (normal, offset), relative to `origin` (world).
pub(crate) type Solid = (Vec<Vec3>, Vec<(Vec3, f32)>);

fn solid(s: &Structure, p: &super::state::Part, origin: DVec3, out: &mut Solid) {
    solid_at(s, &p.shape, p.local, origin, out);
}

/// The same for a shape of `s` placed by `local` (its frame): a part where it is, or where a
/// move would take it.
pub(crate) fn solid_at(s: &Structure, shape: &super::convex::Convex, local: glam::Affine3A, origin: DVec3, out: &mut Solid) {
    out.0.clear();
    out.1.clear();
    let shift = (s.pos - origin).as_vec3();
    let place = |v: Vec3| s.rot * local.transform_point3(v) + shift;
    out.0.extend(shape.verts().map(place));
    for f in &shape.faces {
        let n = (s.rot * local.transform_vector3(f.plane.n)).normalize();
        out.1.push((n, n.dot(place(f.verts[0]))));
    }
}

/// Whether `v` is in the solid of these face planes or within `TOUCH` of it, entered along
/// `-out` (`out`: from the solid toward what the point is of): the faces looking away from where
/// it came are not asked, so a thin plate holds what went past its back (as far as `DEEP`).
fn inside(planes: &[(Vec3, f32)], v: Vec3, out: Vec3) -> bool {
    planes.iter().all(|&(n, d)| {
        let s = n.dot(v) - d;
        if n.dot(out) < -0.7 { s <= DEEP } else { s <= TOUCH }
    })
}

/// How far past the far face of a part a corner still counts as in it (m).
const DEEP: f32 = 0.5;

/// The face normal of either solid along which they overlap least (pointing from `p` toward
/// `q`) and by how much; None when one separates them.
pub(crate) fn separating(p: &Solid, q: &Solid) -> Option<(Vec3, f32)> {
    let span = |v: &[Vec3], n: Vec3| v.iter().fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x.dot(n)), hi.max(x.dot(n))));
    let mut best = (f32::MAX, Vec3::Y);
    for &(n, _) in p.1.iter().chain(q.1.iter()) {
        let (a0, a1) = span(&p.0, n);
        let (b0, b1) = span(&q.0, n);
        let overlap = a1.min(b1) - a0.max(b0);
        if overlap < -TOUCH {
            return None;
        }
        if overlap < best.0 {
            best = (overlap, n);
        }
    }
    let cp = p.0.iter().copied().sum::<Vec3>() / p.0.len() as f32;
    let cq = q.0.iter().copied().sum::<Vec3>() / q.0.len() as f32;
    let n = if (cq - cp).dot(best.1) < 0.0 { -best.1 } else { best.1 };
    Some((n, best.0.max(0.0)))
}
