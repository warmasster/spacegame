//! A structure in the world: its parts and joints as they are now (hit points, shapes that may have
//! lost chips, what still works), where it is and how it moves. Every look, every simulation level
//! and every network reads this; nothing else holds structure state.
use super::{blueprint::Blueprint, bvh::Bvh, catalog::Catalog, convex::Convex};
use glam::{Affine3A, DVec3, Mat3, Quat, Vec3};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Part {
    pub kind: u16,
    /// Part frame → structure frame, as it is now.
    pub local: Affine3A,
    /// Where it sits with every joint at rest, and the joint ("bone", 0 = none) that moves it:
    /// `local = bones[bone] × rest`. Looks are built at rest and posed by their bone.
    pub rest: Affine3A,
    pub bone: u16,
    /// In the part frame: the catalog's until it loses a chip.
    pub shape: Arc<Convex>,
    pub hp: f32,
    pub max_hp: f32,
    pub mass: f32,
    /// Bounding sphere in the structure frame.
    pub center: Vec3,
    pub radius: f32,
    pub alive: bool,
    /// A piece of a broken part: no ports, no machine.
    pub fragment: bool,
    /// Ports and machine work (enough hit points, not a fragment).
    pub working: bool,
    /// Holds an anchored structure to the ground.
    pub anchor: bool,
    /// Share of its volume that is material (shells), kept by its pieces.
    pub fill: f32,
    /// Level of its store on its network (NaN: full, as built).
    pub store: f32,
    /// Share of what it asks of its networks that it got last tick (1 when it asks nothing).
    pub supplied: f32,
    /// Only seen (`PartKindDef::ghost`), and whether bodies touch it.
    pub ghost: bool,
    pub collide: bool,
    /// Which part of its structure's blueprint it is (`NO_ORIGIN`: none, a broken piece): looks
    /// are shared by every structure of a blueprint and tell its parts apart by this.
    pub origin: u32,
    /// It came off whole and is not missed (cargo let go: `PartKindDef::carried`): nothing to put
    /// back, nothing to show as gone.
    pub left: bool,
}

pub const NO_ORIGIN: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub struct Joint {
    pub a: u32,
    pub b: u32,
    pub kind: u16,
    /// Structure frame.
    pub at: Vec3,
    pub hp: f32,
    pub max_hp: f32,
    pub networks: u32,
    pub alive: bool,
}

/// A sprung leg: a landing gear's strut. The parts on its bone slide along `axis` as the ground
/// (or whatever the foot is set down on) presses the foot in, against a gas spring and a damper;
/// the physics measures how far it is pressed in every step (`x`) and whoever owns the bones
/// poses them from it. While it is `active` its parts touch nothing by themselves: the leg does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// The bone its parts ride.
    pub bone: u16,
    /// The foot's sole with the leg full out, and the way it goes as it is pressed in (unit),
    /// in the structure's frame.
    pub foot: Vec3,
    pub axis: Vec3,
    /// How far it can be pressed in (m).
    pub stroke: f32,
    /// What it pushes with full out and pressed all the way in (N): a gas spring between the
    /// two (`gas`: its polytropic exponent), soft at first and hard at the end.
    pub preload: f32,
    pub end: f32,
    pub gas: f32,
    /// Its damper going in and coming out (N·s/m).
    pub damp: [f32; 2],
    /// Grip of its foot on what it stands on.
    pub grip: f32,
    pub active: bool,
    /// How far it is pressed in now (m) and what it carries (N).
    pub x: f32,
    pub load: f32,
}

impl Spring {
    /// The length of gas a spring from `preload` to `end` over `stroke` has (m).
    fn column(&self) -> f32 {
        let ratio = (self.preload / self.end.max(self.preload * 1.01)).clamp(1e-3, 0.99);
        self.stroke / (1.0 - ratio.powf(1.0 / self.gas.max(1.0)))
    }

    /// What it pushes with pressed in `x` (N), and how much more per metre there (N/m).
    pub fn force(&self, x: f32) -> (f32, f32) {
        let l = self.column();
        let x = x.clamp(0.0, self.stroke);
        let f = self.preload * (1.0 - x / l).powf(-self.gas);
        (f, f * self.gas / (l - x))
    }

    /// How far in it rests under `load` N (m).
    pub fn at_rest(&self, load: f32) -> f32 {
        if load <= self.preload {
            return 0.0;
        }
        let l = self.column();
        (l * (1.0 - (self.preload / load).powf(1.0 / self.gas.max(1.0)))).clamp(0.0, self.stroke)
    }
}

/// Where a ray met a structure (structure frame).
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub part: u32,
    pub t: f32,
    pub normal: Vec3,
}

/// Gravity of a structure's own making, for whatever it carries: `g` m/s² toward its own -Y
/// (its decks) when it is all there, and how much of it there is now (`on`, 0..1: it comes and
/// goes as whoever owns its systems says). `g` 0: it makes none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OwnGravity {
    pub g: f32,
    pub on: f32,
}

pub struct Structure {
    pub id: u64,
    pub name: Arc<str>,
    /// Structure frame → world: position of its origin and turn.
    pub pos: DVec3,
    pub rot: Quat,
    pub vel: DVec3,
    /// rad/s, world axes.
    pub spin: Vec3,
    /// How its speed changed over its last step (world, m/s²; nil at rest): what it carries
    /// does not weigh that much of the world's pull (`weight`).
    pub acc: DVec3,
    /// The gravity it makes for what is in its rooms (`weight`), if any.
    pub gravity: OwnGravity,
    /// The places inside it (boxes: least and greatest corner, its frame): what is in one of
    /// them is aboard it — it goes with it in the air, and its own gravity holds there. Said
    /// by whoever makes it (a ship: its compartments); none: it has no inside.
    pub rooms: Arc<[[Vec3; 2]]>,
    pub anchored: bool,
    pub parts: Vec<Part>,
    pub joints: Vec<Joint>,
    pub mass: f32,
    /// Centre of mass and bounding sphere (structure frame).
    pub com: Vec3,
    /// Inertia tensor about the centre of mass (structure frame), and its inverse.
    pub inertia: Mat3,
    pub inv_inertia: Mat3,
    pub center: Vec3,
    pub radius: f32,
    /// Bumped whenever a part or a joint changes (hit points, gone, fed or not): what is kept per
    /// part follows it.
    pub version: u64,
    /// Bumped when a shape changes (a chip came off): only then is a look rebuilt.
    pub shapes: u64,
    /// Lying still on the ground (free structures).
    pub resting: bool,
    /// Touching the ground under it as of its last step (its weight on its feet).
    pub grounded: bool,
    /// Someone let go of some of its joints (a clamp opened: `Structures::separate` then takes
    /// off it whatever no longer holds to the rest).
    pub parted: bool,
    /// How long it has been nearly still on the ground (s).
    pub still: f32,
    /// Simulation level now, simulated up to `clock` (NaN: not yet), kept active until `awake_until`.
    pub sim: super::schedule::SimLevel,
    pub clock: f64,
    pub awake_until: f64,
    /// Networks simulated up to (NaN: not yet).
    pub net_clock: f64,
    /// Pose of each bone (structure frame; 0 is the identity) and a counter that moves when any
    /// changes: looks re-pose from it without rebuilding.
    pub bones: Vec<Affine3A>,
    pub pose: u64,
    /// Bumped when a bone moves other than by a leg being pressed in: what it stands on (its
    /// lowest corners, its keel) is kept until then.
    pub stance: u64,
    /// Its sprung legs (`Spring`).
    pub springs: Vec<Spring>,
    /// The bones of what gives way when the ground pushes it (a ramp lying on the ground: its
    /// owner lifts it as the hull comes down): their parts do not hold the hull up.
    pub giving: Vec<u16>,
    /// Push its systems give it (world, N) and turn (world, N·m), applied every physics step until
    /// changed: thrust.
    pub force: Vec3,
    pub torque: Vec3,
    /// A name for what owns its systems (a ship's kind), if anything.
    pub owner: Option<Arc<str>>,
    /// Its parts are parts of the blueprint `name` (by their `origin`) in their own shapes: its
    /// look is that blueprint's, shared, with whatever is gone left out. False once a part loses
    /// a chip, and for loose pieces.
    pub model: bool,
    /// Its live parts by where they are (ghosts left out), for rays, blasts and contacts.
    pub index: Bvh,
    /// What holds it, if anything (`hold`): it is not a body of its own while it is held.
    pub held: Option<super::hold::Held>,
    /// What it holds: masses it carries at points of its frame.
    pub loads: Vec<super::hold::Load>,
    /// What its parts hold (`contents`: water in a drum, propellant in a tank), by part: it
    /// weighs with the structure, and changes through `take`, `put` and `fill`.
    pub stored: Vec<super::contents::Stored>,
    /// How many times its mass properties have been worked out: it moves only when something
    /// of what it weighs changed (a part, a pose, a load, what a part holds by a quantum).
    pub weighings: u64,
    /// Its mass, first moment and inertia about its own origin as last worked out, kept so
    /// that what a part holds can change them without going over every part (`contents`).
    pub(crate) sums: super::contents::Sums,
    /// It is simulated somewhere else (another player's game) and this is a copy that follows:
    /// nothing breaks or comes off it here by itself (`Structures::separate`, the hits of
    /// `blast_where` and `shoot_where`); what happens to it is told by whoever simulates it.
    pub remote: bool,
}

impl Part {
    /// A part of kind `kind` (the catalog's shape, full health) or a given shape.
    pub fn new(cat: &Catalog, kind: u16, local: Affine3A, shape: Option<Convex>, fragment: bool) -> Part {
        let k = &cat.parts[usize::from(kind)];
        let shape = shape.map_or_else(|| k.shape.clone(), Arc::new);
        let mut p = Part {
            kind,
            local,
            rest: local,
            bone: 0,
            shape,
            hp: 0.0,
            max_hp: 0.0,
            mass: 0.0,
            center: Vec3::ZERO,
            radius: 0.0,
            alive: true,
            fragment,
            working: !fragment,
            anchor: false,
            fill: k.fill,
            store: f32::NAN,
            supplied: 1.0,
            ghost: k.def.ghost && !fragment,
            collide: fragment || !(k.def.ghost || k.def.no_collide),
            origin: NO_ORIGIN,
            left: false,
        };
        p.reshape(cat, p.shape.clone());
        p.hp = p.max_hp;
        p
    }

    /// A new shape (a chip came off): mass, hit points (same share left) and bounds follow.
    pub fn reshape(&mut self, cat: &Catalog, shape: Arc<Convex>) {
        let m = cat.material(self.kind);
        let volume = shape.volume() * self.fill;
        let share = if self.max_hp > 0.0 { self.hp / self.max_hp } else { 1.0 };
        self.shape = shape;
        self.max_hp = m.toughness * volume;
        self.hp = self.max_hp * share;
        self.mass = m.density * volume;
        self.place();
    }

    /// Bounding sphere from the shape and the placement.
    pub fn place(&mut self) {
        let (c, r) = self.shape.sphere();
        self.center = self.local.transform_point3(c);
        self.radius = r;
    }

    pub fn damage(&self) -> f32 {
        1.0 - (self.hp / self.max_hp).clamp(0.0, 1.0)
    }
}

impl Structure {
    pub fn new(id: u64, bp: &Blueprint, cat: &Catalog, pos: DVec3, rot: Quat) -> Structure {
        let parts: Vec<Part> = bp
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut part = Part::new(cat, p.kind, p.local, None, false);
                part.bone = p.bone;
                part.origin = i as u32;
                part
            })
            .collect();
        let joints = bp
            .joints
            .iter()
            .map(|j| {
                let s = cat.joints[usize::from(j.kind)].1.strength;
                Joint { a: j.a, b: j.b, kind: j.kind, at: j.at, hp: s, max_hp: s, networks: j.networks, alive: true }
            })
            .collect();
        // (what its containers hold as built weighs with it)
        let stored = super::contents::as_built(&parts, cat);
        let mut s = Structure::build(id, bp.name.as_str().into(), pos, rot, bp.anchored, parts, joints, stored);
        s.model = true;
        s
    }

    #[allow(clippy::too_many_arguments)]
    pub fn assemble(id: u64, name: Arc<str>, pos: DVec3, rot: Quat, anchored: bool, parts: Vec<Part>, joints: Vec<Joint>) -> Structure {
        Structure::build(id, name, pos, rot, anchored, parts, joints, Vec::new())
    }

    /// The same for parts told from elsewhere (a piece that came off a structure in another
    /// player's game): those that are containers hold what they hold as built, until told
    /// otherwise (`fill`).
    #[allow(clippy::too_many_arguments)]
    pub fn assemble_holding(id: u64, name: Arc<str>, pos: DVec3, rot: Quat, anchored: bool, parts: Vec<Part>, joints: Vec<Joint>, cat: &Catalog) -> Structure {
        let stored = super::contents::as_built(&parts, cat);
        Structure::build(id, name, pos, rot, anchored, parts, joints, stored)
    }

    /// The same with what its parts hold (`contents`), weighed with it from the start.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build(id: u64, name: Arc<str>, pos: DVec3, rot: Quat, anchored: bool, parts: Vec<Part>, joints: Vec<Joint>, stored: Vec<super::contents::Stored>) -> Structure {
        let mut s = Structure {
            id,
            name,
            pos,
            rot,
            vel: DVec3::ZERO,
            spin: Vec3::ZERO,
            acc: DVec3::ZERO,
            gravity: OwnGravity::default(),
            rooms: Arc::new([]),
            anchored,
            parts,
            joints,
            mass: 0.0,
            com: Vec3::ZERO,
            inertia: Mat3::IDENTITY,
            inv_inertia: Mat3::IDENTITY,
            center: Vec3::ZERO,
            radius: 0.0,
            version: 0,
            shapes: 0,
            resting: false,
            grounded: false,
            parted: false,
            still: 0.0,
            sim: super::schedule::SimLevel::Active,
            clock: f64::NAN,
            awake_until: 0.0,
            net_clock: f64::NAN,
            bones: vec![Affine3A::IDENTITY],
            pose: 0,
            stance: 0,
            springs: Vec::new(),
            giving: Vec::new(),
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            owner: None,
            model: false,
            index: Bvh::default(),
            held: None,
            loads: Vec::new(),
            stored,
            weighings: 0,
            sums: super::contents::Sums::default(),
            remote: false,
        };
        s.refresh();
        s
    }

    /// Pose every bone (index 0 stays the identity): moving parts follow, bounds and mass
    /// properties too, the look only re-poses (the version does not move).
    pub fn set_pose(&mut self, bones: &[Affine3A]) {
        if self.bones.len() != bones.len() + 1 {
            self.bones.resize(bones.len() + 1, Affine3A::IDENTITY);
        }
        // (a leg pressed in a little moves nothing that matters to how the whole stands)
        let stance = bones.iter().enumerate().any(|(k, m)| self.bones[k + 1] != *m && !self.sprung(k as u16 + 1));
        self.bones[1..].copy_from_slice(bones);
        for p in &mut self.parts {
            if p.bone > 0 && usize::from(p.bone) < self.bones.len() {
                p.local = self.bones[usize::from(p.bone)] * p.rest;
                p.place();
            }
        }
        if stance {
            self.mass_props();
            self.stance += 1;
        } else {
            self.index.build(self.parts.iter().enumerate().filter(|(_, p)| p.alive && !p.ghost).map(|(i, p)| (i as u32, p.center, p.radius)));
        }
        self.pose += 1;
    }

    /// Whether `p` (its frame) is in one of its rooms.
    pub fn in_rooms(&self, p: Vec3) -> bool {
        self.rooms.iter().any(|[lo, hi]| p.cmpge(*lo).all() && p.cmple(*hi).all())
    }

    /// Whether `bone` is a sprung leg's.
    pub fn sprung(&self, bone: u16) -> bool {
        bone > 0 && self.springs.iter().any(|s| s.bone == bone)
    }

    /// Whether what is on `bone` gives way to the ground (`giving`).
    pub fn gives(&self, bone: u16) -> bool {
        bone > 0 && self.giving.contains(&bone)
    }

    /// Whether a leg stands for the parts on `bone` against the ground and other bodies (an
    /// active sprung leg's parts touch nothing by themselves).
    pub fn on_leg(&self, bone: u16) -> bool {
        bone > 0 && self.springs.iter().any(|s| s.bone == bone && s.active)
    }

    /// Mass, centre of mass and bounds from the live parts; the version moves on.
    pub fn refresh(&mut self) {
        self.mass_props();
        self.version += 1;
    }

    /// Mass, centre of mass, inertia and bounds from the live parts (as posed now), what they
    /// hold and what the structure holds.
    pub(crate) fn mass_props(&mut self) {
        self.weighings += 1;
        let (mut m, mut mc) = (0.0, Vec3::ZERO);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in self.parts.iter().filter(|p| p.alive && !p.ghost) {
            m += p.mass;
            mc += p.center * p.mass;
            lo = lo.min(p.center - p.radius);
            hi = hi.max(p.center + p.radius);
        }
        // what it holds is its to carry
        let (held_m, held_mc) = super::hold::weigh(&self.loads);
        m += held_m;
        mc += held_mc;
        // and what its parts hold, where it lies in them (`contents`)
        if !self.stored.is_empty() {
            let (in_m, in_mc) = super::contents::weigh(&self.stored, &self.parts);
            m += in_m;
            mc += in_mc;
        }
        self.index.build(self.parts.iter().enumerate().filter(|(_, p)| p.alive && !p.ghost).map(|(i, p)| (i as u32, p.center, p.radius)));
        self.mass = m;
        self.com = if m > 0.0 { mc / m } else { Vec3::ZERO };
        self.center = (lo + hi) * 0.5;
        self.radius = self.parts.iter().filter(|p| p.alive && !p.ghost).map(|p| p.center.distance(self.center) + p.radius).fold(0.0, f32::max);
        // each part as a box of its extents, turned and moved to the centre of mass
        let mut inertia = Mat3::ZERO;
        for p in self.parts.iter().filter(|p| p.alive && !p.ghost) {
            let e = Vec3::new(p.shape.support(Vec3::X) + p.shape.support(Vec3::NEG_X), p.shape.support(Vec3::Y) + p.shape.support(Vec3::NEG_Y), p.shape.support(Vec3::Z) + p.shape.support(Vec3::NEG_Z));
            let own = Mat3::from_diagonal(Vec3::new(e.y * e.y + e.z * e.z, e.x * e.x + e.z * e.z, e.x * e.x + e.y * e.y) * (p.mass / 12.0));
            let turn = Mat3::from_quat(p.local.to_scale_rotation_translation().1);
            let r = p.center - self.com;
            let shift = (Mat3::IDENTITY * r.length_squared() - Mat3::from_cols(r * r.x, r * r.y, r * r.z)) * p.mass;
            inertia += turn * own * turn.transpose() + shift;
        }
        for l in &self.loads {
            inertia += super::hold::point_inertia(l.mass, l.at - self.com);
        }
        if !self.stored.is_empty() {
            inertia += super::contents::inertia(&self.stored, &self.parts, self.com);
        }
        // (kept as sums about its own origin: what a part holds moves them alone, `contents`)
        self.sums = super::contents::Sums::of(m, mc, inertia, self.com);
        let floor = Mat3::IDENTITY * (self.mass.max(0.01) * 1e-4);
        self.inertia = inertia + floor;
        self.inv_inertia = self.inertia.inverse();
    }

    pub fn alive(&self) -> bool {
        self.parts.iter().any(|p| p.alive && !p.ghost)
    }

    /// World point of a structure-frame point.
    pub fn to_world(&self, p: Vec3) -> DVec3 {
        self.pos + (self.rot * p).as_dvec3()
    }

    /// Structure-frame point of a world point (near the structure: f32 is enough there).
    pub fn to_local(&self, p: DVec3) -> Vec3 {
        self.rot.inverse() * (p - self.pos).as_vec3()
    }

    pub fn dir_to_local(&self, d: DVec3) -> Vec3 {
        self.rot.inverse() * d.as_vec3()
    }

    /// Live parts a structure-frame sphere overlaps: (part, normal out of the part, depth). Parts
    /// smaller than `min` (m of bounding radius) are left out (knobs, lamps).
    pub fn sphere_contacts(&self, c: Vec3, r: f32, min: f32, out: &mut Vec<(u32, Vec3, f32)>) {
        self.index.sphere(c, r, |i| {
            let p = &self.parts[i as usize];
            if !p.alive || !p.collide || p.radius < min || p.center.distance_squared(c) > (p.radius + r) * (p.radius + r) {
                return;
            }
            let lc = p.local.inverse().transform_point3(c);
            let (d, n) = p.shape.closest(lc);
            if d < r {
                out.push((i, p.local.transform_vector3(n).normalize_or(Vec3::Y), r - d));
            }
        });
    }

    /// Part `i` mended by `hp` hit points (a welder at work), its joints with it. Its integrity
    /// after (0..1); None if it is gone (`rebuild` puts it back) or a loose piece.
    pub fn mend(&mut self, cat: &Catalog, i: usize, hp: f32) -> Option<f32> {
        let p = self.parts.get_mut(i).filter(|p| p.alive && !p.fragment && !p.ghost)?;
        p.hp = (p.hp + hp).min(p.max_hp);
        p.working = p.hp > p.max_hp * cat.parts[usize::from(p.kind)].def.function;
        let share = if p.max_hp > 0.0 { p.hp / p.max_hp } else { 1.0 };
        for j in self.joints.iter_mut().filter(|j| j.alive && (j.a as usize == i || j.b as usize == i)) {
            j.hp = j.hp.max(j.max_hp * share);
        }
        self.version += 1;
        Some(share)
    }

    /// Part `i`, gone (destroyed, or torn off), put back where it was with `share` of its hit
    /// points, and joined again to what it touched that is still there. False if it is there
    /// already or was never a part of it (a loose piece).
    pub fn rebuild(&mut self, cat: &Catalog, i: usize, share: f32) -> bool {
        let Some(p) = self.parts.get_mut(i).filter(|p| !p.alive && !p.fragment && !p.left) else {
            return false;
        };
        p.alive = true;
        p.hp = p.max_hp * share.clamp(0.01, 1.0);
        p.working = p.hp > p.max_hp * cat.parts[usize::from(p.kind)].def.function;
        // (what it held went with it: it comes back empty)
        self.spill(i as u32);
        for k in 0..self.joints.len() {
            let j = self.joints[k];
            if (j.a as usize == i || j.b as usize == i) && self.parts[j.a as usize].alive && self.parts[j.b as usize].alive {
                self.joints[k].alive = true;
                self.joints[k].hp = j.max_hp;
            }
        }
        self.refresh();
        true
    }

    /// The nearest part that is gone (and can be put back) along a structure-frame ray: where it
    /// would be.
    pub fn raycast_gone(&self, o: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        for (i, p) in self.parts.iter().enumerate().filter(|(_, p)| !p.alive && !p.fragment && !p.ghost && !p.left) {
            let oc = o - p.center;
            let b = oc.dot(dir);
            let c = oc.length_squared() - p.radius * p.radius;
            if c > 0.0 && (b > 0.0 || b * b < c) {
                continue;
            }
            let inv = p.local.inverse();
            let limit = best.map_or(max, |h| h.t);
            if let Some((t, n)) = p.shape.raycast(inv.transform_point3(o), inv.transform_vector3(dir), limit) {
                best = Some(RayHit { part: i as u32, t, normal: p.local.transform_vector3(n).normalize() });
            }
        }
        best
    }

    /// First live part along a structure-frame ray.
    pub fn raycast(&self, o: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
        self.raycast_by(o, dir, max, |_| true)
    }

    /// The same among what a body bumps into (`sphere_contacts`): parts that collide, of a
    /// bounding radius of `min` m at least.
    pub fn raycast_solid(&self, o: Vec3, dir: Vec3, max: f32, min: f32) -> Option<RayHit> {
        self.raycast_by(o, dir, max, |p| p.collide && p.radius >= min)
    }

    fn raycast_by(&self, o: Vec3, dir: Vec3, max: f32, keep: impl Fn(&Part) -> bool) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        self.index.ray(o, dir, max, |i, reach| {
            let p = &self.parts[i as usize];
            if !p.alive || !keep(p) {
                return reach;
            }
            // the bounding sphere first
            let oc = o - p.center;
            let b = oc.dot(dir);
            let c = oc.length_squared() - p.radius * p.radius;
            if c > 0.0 && (b > 0.0 || b * b < c) {
                return reach;
            }
            let inv = p.local.inverse();
            let (lo, ld) = (inv.transform_point3(o), inv.transform_vector3(dir));
            match p.shape.raycast(lo, ld, reach) {
                Some((t, n)) => {
                    best = Some(RayHit { part: i, t, normal: p.local.transform_vector3(n).normalize() });
                    t
                }
                None => reach,
            }
        });
        best
    }
}
