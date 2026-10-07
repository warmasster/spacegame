//! What a body's hands do when they hold no tool: work the controls of a ship (`reach`), rest
//! on the stick and the throttle of the seat it sits in (`cockpit`), make gestures (`gestures`),
//! raise the wrist computer to be read (`wrist`). All of it is data (`assets/defs/manos.jsonc`,
//! `gestos.jsonc`, `muneca.jsonc`) and all of it works for any body made from a rig — the
//! player's own, seen from its eyes or from outside, and every other player's, from the little
//! that is told of them: the seat they are in, the gesture they make, whether their wrist is
//! up, the control they worked.
//!
//! Each of those says where it wants a hand (`Target`: the palm's place and turn in the world,
//! the fingers, the way the elbow goes); `Handwork` gives each hand to the first that wants it —
//! a tool before anything, then a control, the wrist computer, a gesture, the seat's own
//! controls — and takes the hand from one place to the next without a jump: what is left of
//! where it was is kept as an offset from where it is wanted now, in the body's own frame, and
//! dies away (so a hand on something that moves — a ship under way, a lever — is on it at
//! once, and only the change of place takes time).
//!
//! A body whose hands do none of this costs one comparison a frame.
use crate::{
    body::{Body, ELBOW_POLE, Grip, Stance},
    cockpit::{self, CockpitDef, CockpitKit, Cockpits},
    gestures::{self, GestureKit, GestureSet},
    reach::{self, Reach, ReachDef, ReachKit},
    rig::{Rig, RigDef, palm_turn},
    ships::Ships,
    wrist::{self, WristDef, WristKit},
};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    anim::{
        skeleton::frame_turn,
        spring::{Spring3, SpringQ},
    },
    body::BodyRegistry,
    structure::set::Structures,
};
use serde::Deserialize;
use std::{path::Path, sync::Arc};

// ---------------------------------------------------------------- where a hand is wanted

/// Where a hand is wanted: the middle of what its palm closes on (as `Grip::at`), its turn (the
/// palm's frame in the world: x from index to little finger, y the way the palm faces), how
/// closed each finger is, and the way its elbow goes if not down and out (model axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub at: DVec3,
    pub rot: Quat,
    pub fingers: [f32; 5],
    pub pole: Option<Vec3>,
}

impl Target {
    pub fn grip(&self) -> Grip {
        Grip { at: self.at, normal: self.rot * Vec3::Y, across: self.rot * Vec3::X, fingers: self.fingers }
    }

    pub fn of(g: &Grip) -> Target {
        Target { at: g.at, rot: frame_turn(Vec3::Y, Vec3::X, g.normal, g.across), fingers: g.fingers, pole: None }
    }
}

/// The part of a hand that goes on something taken with the fingertips.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tip {
    /// The tip of the index finger, of the thumb.
    #[default]
    Indice,
    Pulgar,
    /// Between the tips of thumb and index: what they pinch.
    Pinza,
    /// Between the tips of index and middle finger: what they lift or push.
    Yemas,
    /// Between the tips of thumb, index and middle finger: what they close round (a knob).
    Garra,
}

/// How far a hand goes: how far the way its fingertips come leans from a thing's own way out
/// toward the arm (rad), the most its wrist bends (rad), the share of its length its arm
/// stretches, and the share of it the arm is held out toward what is well out of its reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub lean: f32,
    pub wrist: f32,
    pub arm: f32,
    pub far: f32,
}

/// Past the arm's reach, a metre farther is this much of a metre less the arm is held out.
const SLACK: f32 = 0.6;

/// An arm of a body, for whoever puts its hand somewhere: its rig and side (0 left), where its
/// shoulder is now (world), the body's turn (model axes to the world's) and size.
#[derive(Clone, Copy)]
pub struct Arm<'a> {
    pub rig: &'a Rig,
    pub side: usize,
    pub shoulder: DVec3,
    pub rot: Quat,
    pub scale: f32,
}

impl Arm<'_> {
    pub fn up(&self) -> Vec3 {
        self.rot * Vec3::Y
    }

    pub fn ahead(&self) -> Vec3 {
        self.rot * Vec3::Z
    }

    pub fn left(&self) -> Vec3 {
        self.rot * Vec3::X
    }

    /// Toward the body's middle from this arm's side.
    pub fn inward(&self) -> Vec3 {
        self.rot * Vec3::X * if self.side == 0 { -1.0 } else { 1.0 }
    }

    /// Its two lengths (m of the world).
    fn lengths(&self) -> (f32, f32) {
        let (a, b) = self.rig.arms[self.side].lengths(&self.rig.skeleton);
        (a * self.scale, b * self.scale)
    }

    /// From its shoulder to its wrist at its longest (m of the world).
    pub fn reach(&self) -> f32 {
        let (a, b) = self.lengths();
        a + b
    }

    /// The turn of its forearm from rest (model axes at rest to the world's) with its wrist at
    /// `wrist` and its elbow going the usual way: what the body's solver will make of it
    /// (`body::hinge`). What a hand may be asked to do is told against this (`settle`).
    pub fn forearm(&self, wrist: DVec3) -> Option<Quat> {
        let (sk, limb) = (&self.rig.skeleton, self.rig.arms[self.side]);
        let along = (sk.bind[limb.end].pos - sk.bind[limb.lower].pos).normalize_or_zero();
        let elbow = self.elbow(wrist);
        let fore = (wrist - elbow).as_vec3().normalize_or_zero();
        let normal = (elbow - self.shoulder).as_vec3().cross(fore).normalize_or_zero();
        (along != Vec3::ZERO && fore != Vec3::ZERO && normal != Vec3::ZERO).then(|| frame_turn(along, crate::body::hinge(self.rig, self.side), fore, normal))
    }

    /// Where its elbow is with its wrist at `wrist` (world), going the usual way: what the
    /// body's own solver will make of it (`anim::ik`).
    pub fn elbow(&self, wrist: DVec3) -> DVec3 {
        let (l1, l2) = self.lengths();
        let to = (wrist - self.shoulder).as_vec3();
        let far = to.length();
        let d = far.clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * 0.999);
        let dir = if far > 1e-6 { to / far } else { -self.up() };
        let pole = self.rot * (ELBOW_POLE * Vec3::new(if self.side == 0 { 1.0 } else { -1.0 }, 1.0, 1.0));
        let side = (pole - dir * pole.dot(dir)).normalize_or(dir.any_orthonormal_vector());
        let along = (l1 * l1 + d * d - l2 * l2) / (2.0 * d);
        self.shoulder + (dir * along + side * (l1 * l1 - along * along).max(0.0).sqrt()).as_dvec3()
    }
}

/// A finger's tip with the hand at rest (model space) curled `c` (-1 flat, 0 at rest, 1 a
/// fist), and the way its last bone points: as the body bends it (`Body::update`).
pub fn fingertip(rig: &Rig, side: usize, f: usize, c: f32) -> (Vec3, Vec3) {
    let sk = &rig.skeleton;
    let Some(finger) = rig.fingers[side].get(f) else {
        // (a hand without fingers: its palm)
        let p = &rig.palms[side];
        return (p.at, p.fingers);
    };
    let n = finger.bones.len().min(3);
    let mut rot = Quat::IDENTITY;
    let mut at = sk.bind[finger.bones[0]].pos;
    let mut dir = Vec3::Y;
    for j in 0..n {
        let deg = if c >= 0.0 { c * finger.travel.cerrar[j] } else { c * finger.travel.abrir[j] };
        rot = Quat::from_axis_angle(finger.axis, deg.to_radians()) * rot;
        let head = sk.bind[finger.bones[j]].pos;
        let next = if j + 1 < n { sk.bind[finger.bones[j + 1]].pos } else { head + sk.bind[finger.bones[j]].rot * Vec3::Y * finger.tip };
        dir = rot * (next - head);
        at += dir;
    }
    (at, dir.normalize_or(Vec3::Y))
}

/// The part `tip` of a hand at rest (model space) with its fingers curled `c`, and the way it
/// points: what is put on a thing, and the way it comes at it.
pub fn hand_point(rig: &Rig, side: usize, tip: Tip, c: &[f32; 5]) -> (Vec3, Vec3) {
    let palm = &rig.palms[side];
    let of = |f: usize| fingertip(rig, side, f, c[f]);
    // (where the skin of the palm is: the palm's own point is the middle of a handle held)
    let skin = palm.at - palm.normal * (rig.def.mango * 0.5);
    let knuckle = |f: usize| rig.fingers[side].get(f).map_or(skin, |x| rig.skeleton.bind[x.bones[0]].pos);
    match tip {
        Tip::Indice => of(1),
        Tip::Pulgar => of(0),
        Tip::Pinza => {
            let at = (of(0).0 + of(1).0) * 0.5;
            (at, (at - knuckle(1)).normalize_or(palm.fingers))
        }
        Tip::Yemas => {
            let (a, b) = (of(1), of(2));
            ((a.0 + b.0) * 0.5, (a.1 + b.1).normalize_or(palm.fingers))
        }
        Tip::Garra => {
            let at = (of(0).0 + of(1).0 + of(2).0) / 3.0;
            (at, (at - skin).normalize_or(palm.normal))
        }
    }
}

/// `out` turned toward `to` by no more than `max` rad (both unit).
fn lean(out: Vec3, to: Vec3, max: f32) -> Vec3 {
    if out.angle_between(to) <= max {
        return to;
    }
    let axis = out.cross(to).normalize_or_zero();
    if axis == Vec3::ZERO { out } else { Quat::from_axis_angle(axis, max) * out }
}

/// A hand put somewhere: where it is wanted, the point of it that is on the thing (world), and
/// how far short of the thing that point is left (m; 0: on it).
#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub target: Target,
    pub point: DVec3,
    pub short: f32,
}

/// The palm's own frame at rest (model space): x from index to little finger, y the way it faces.
fn palm_rest(rig: &Rig, side: usize) -> Quat {
    let p = &rig.palms[side];
    frame_turn(Vec3::Y, Vec3::X, p.normal, p.across)
}

/// A hand asked to do this much of what its wrist can (`WristRange`) by whoever puts it on
/// something: the body holds it to all of it, so what is put there is within it with room to
/// spare (the body's shoulder moves a little between asking and posing).
const WRIST_SHARE: f32 = 0.94;

/// Brings a hand turned `r` (model axes at rest to the world's), with its point `p` (model
/// space at rest) at `point`, to what its arm can do: no further than the arm stretches (the
/// point comes back toward the shoulder), its wrist bent no more than `wrist` from the line of
/// its forearm (the hand turns toward it) and doing only what a wrist does (the rig's
/// `muneca`: bent, leant and turned about the forearm so far and no more). The point stays
/// where it is: it is the hand that turns about it. How far the point came back (m).
fn settle(arm: &Arm, lim: &Limits, wrist: f32, r: &mut Quat, point: &mut DVec3, p: Vec3) -> f32 {
    let rig = arm.rig;
    let joint = rig.skeleton.bind[rig.arms[arm.side].end].pos;
    let long = rig.palms[arm.side].fingers;
    let (ax, own) = (&rig.wrists[arm.side], if arm.side == 0 { 1.0 } else { -1.0 });
    let m = rig.def.muneca;
    let range = crate::rig::WristRange { flexion: m.flexion * WRIST_SHARE, extension: m.extension * WRIST_SHARE, radial: m.radial * WRIST_SHARE, cubital: m.cubital * WRIST_SHARE, pronacion: m.pronacion * WRIST_SHARE, supinacion: m.supinacion * WRIST_SHARE };
    let mut short = 0.0;
    for _ in 0..3 {
        let at = |r: Quat, point: DVec3| point - (r * (p - joint) * arm.scale).as_dvec3();
        let (far, most) = ((at(*r, *point) - arm.shoulder).length() as f32, arm.reach() * lim.arm);
        if far > most {
            // out of reach: the arm is not stretched for it — the farther it is, the less the
            // arm is held out toward it (down to a share of its length)
            let out = (most - (far - most) * SLACK).max(arm.reach() * lim.far.min(lim.arm));
            *point -= (*point - arm.shoulder).normalize_or_zero() * f64::from(far - out);
            short += far - out;
        }
        let w = at(*r, *point);
        let fore = (w - arm.elbow(w)).as_vec3().normalize_or(arm.ahead());
        let hand = *r * long;
        let bent = hand.angle_between(fore);
        let axis = hand.cross(fore).normalize_or_zero();
        if bent > wrist && axis != Vec3::ZERO {
            *r = (Quat::from_axis_angle(axis, bent - wrist) * *r).normalize();
        }
        // what a wrist does: its forearm as the body will have it, the hand from it
        if let Some(f) = arm.forearm(at(*r, *point)) {
            let (flex, lean, turn) = crate::body::wrist_angles(ax, f.inverse() * *r);
            let (flex, lean, turn, held) = crate::body::wrist_held(&range, own, flex, lean, turn);
            if held {
                *r = (f * crate::body::wrist_turn(ax, flex, lean, turn)).normalize();
            }
        }
    }
    short
}

/// A hand's part `tip` (its fingers curled `fingers`) on a thing at `at` whose way out is
/// `out`, `off` m off it: the fingers come at it leaning from that way toward the arm, the
/// palm the way that is easy for that arm — down, or ahead with the fingers up, in toward the
/// body a little — and turned `roll` rad about the way it comes (a knob going round).
#[allow(clippy::too_many_arguments)]
pub fn poke(arm: &Arm, lim: &Limits, tip: Tip, fingers: [f32; 5], at: DVec3, out: Vec3, off: f32, roll: f32) -> Placed {
    let rig = arm.rig;
    let palm = &rig.palms[arm.side];
    let (p, d) = hand_point(rig, arm.side, tip, &fingers);
    let to = (arm.shoulder - at).as_vec3().normalize_or(out);
    let (up, ahead) = (arm.up(), arm.ahead());
    let put = |most: f32| {
        let from = lean(out, to, most);
        let easy = -up + ahead * (-from.dot(up) * 0.9) + arm.inward() * 0.25;
        let facing = (easy + from * easy.dot(-from)).normalize_or(arm.inward());
        let mut r = Quat::from_axis_angle(from, roll) * frame_turn(d, palm.normal, -from, facing);
        let mut point = at + (from * off).as_dvec3();
        let short = settle(arm, lim, lim.wrist, &mut r, &mut point, p);
        (r, point, short)
    };
    let (mut r, mut point, mut short) = put(lim.lean);
    if short > 0.0 {
        // out of reach: it is pointed at from where the arm gets to — the farther short of
        // it, the more along the line to it (no longer as it would come on to its face)
        (r, point, short) = put(lim.lean + (short / FAR_POINT).min(1.0) * (std::f32::consts::FRAC_PI_2 - lim.lean).max(0.0));
    }
    Placed { target: Target { at: point - (r * (p - palm.at) * arm.scale).as_dvec3(), rot: (r * palm_rest(rig, arm.side)).normalize(), fingers, pole: None }, point, short }
}

/// A fingertip this far short of what it works (m) points straight at it.
const FAR_POINT: f32 = 0.25;

/// A fist round something `radius` thick whose middle is at `at` and that runs along `length`
/// (either way), `off` m off it. A hand closes on a bar with its fingers square to it and going
/// on from its forearm — over it or under it, on one side of it or the other: whichever faces
/// more the way of `face` —, so that its wrist is not bent for it; only a bar that runs along
/// the forearm itself (nothing to tell by) is taken with the palm facing `face`.
#[allow(clippy::too_many_arguments)]
pub fn fist(arm: &Arm, lim: &Limits, fingers: [f32; 5], at: DVec3, length: Vec3, face: Vec3, radius: f32, off: f32) -> Placed {
    let rig = arm.rig;
    let palm = &rig.palms[arm.side];
    let length = length.normalize_or(arm.up());
    let hint = (face - length * face.dot(length)).normalize_or(length.any_orthonormal_vector());
    let fore = (at - arm.elbow(at)).as_vec3().normalize_or(arm.ahead());
    // the fingers along what of the forearm's way is square to the bar; the palm then faces
    // across both, to the side the hand's own make gives (a left and a right differ)
    let along = fore - length * fore.dot(length);
    let hand = palm.across.cross(palm.normal).dot(palm.fingers).signum();
    let natural = along.normalize_or_zero().cross(length) * hand;
    let natural = if natural.dot(hint) >= 0.0 { natural } else { -natural };
    let told = ((along.length() - 0.15) / 0.3).clamp(0.0, 1.0);
    let facing = (hint + (natural - hint) * (told * told * (3.0 - 2.0 * told))).normalize_or(hint);
    let facing = (facing - length * facing.dot(length)).normalize_or(hint);
    let (a, b) = (palm_turn(palm, facing, length), palm_turn(palm, facing, -length));
    let mut r = if (a * palm.fingers).dot(fore) >= (b * palm.fingers).dot(fore) { a } else { b };
    // (the palm's point is the middle of a handle of the rig's own thickness: a thinner one
    // lies nearer the palm, a thicker one farther)
    let mut point = at - (facing * (radius - rig.def.mango * 0.5 * arm.scale + off)).as_dvec3();
    // (a fist bends at the wrist as far as a wrist does: what it closes on stays in its palm)
    let short = settle(arm, lim, std::f32::consts::PI, &mut r, &mut point, palm.at);
    Placed { target: Target { at: point, rot: (r * palm_rest(rig, arm.side)).normalize(), fingers, pole: None }, point, short }
}

/// A hand's palm at `at` (the middle of what it would close on) facing `palm` with its fingers
/// pointing along `along` (world; made square to `palm`): a hand held in the air.
pub fn held(arm: &Arm, at: DVec3, palm: Vec3, along: Vec3, fingers: [f32; 5], pole: Option<Vec3>) -> Target {
    let rest = &arm.rig.palms[arm.side];
    let r = frame_turn(rest.normal, rest.fingers, palm, along);
    Target { at, rot: (r * palm_rest(arm.rig, arm.side)).normalize(), fingers, pole }
}

// ---------------------------------------------------------------- the data

/// `manos.jsonc`: hands on a ship's controls, and at a seat's own.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HandsFile {
    /// Half-life of the way of a hand from one place to the next (s), and of its elbow; and
    /// how far off the straight line it goes on its way (a share of the way).
    viaje: f32,
    #[serde(default)]
    arco: f32,
    alcance: ReachDef,
    cabina: CockpitDef,
}

/// Everything the hands do, as the data has it.
#[derive(Clone, Debug)]
pub struct HandData {
    pub travel: f32,
    pub arc: f32,
    pub reach: ReachDef,
    pub cockpit: CockpitDef,
    pub gestures: GestureSet,
    pub wrist: WristDef,
}

impl HandData {
    /// From `assets/defs` (`manos.jsonc`, `gestos.jsonc`, `muneca.jsonc`).
    pub fn load(defs: &Path) -> Result<HandData, String> {
        let load = |name: &str| lunar_core::defs::file(defs, name);
        let fail = |e: lunar_core::defs::DefError| format!("{}: {}", e.file, e.message);
        let hands: HandsFile = lunar_core::defs::load(&load("manos")).map_err(fail)?;
        Ok(HandData { travel: hands.viaje, arc: hands.arco, reach: hands.alcance, cockpit: hands.cabina, gestures: lunar_core::defs::load(&load("gestos")).map_err(fail)?, wrist: lunar_core::defs::load(&load("muneca")).map_err(fail)? })
    }
}

/// The data ready for the bodies of a rig: its hand poses found, its points checked. One for
/// all of them.
#[derive(Clone, Debug)]
pub struct Kit {
    pub travel: f32,
    pub arc: f32,
    pub reach: ReachKit,
    pub cockpit: CockpitKit,
    pub gestures: GestureKit,
    pub wrist: WristKit,
}

impl Kit {
    pub fn new(data: &HandData, rig: &RigDef) -> Result<Kit, String> {
        Ok(Kit { travel: data.travel.max(1e-3), arc: data.arc.clamp(0.0, 0.5), reach: ReachKit::new(&data.reach, &rig.manos)?, cockpit: CockpitKit::new(&data.cockpit, &rig.manos)?, gestures: GestureKit::new(&data.gestures, &rig.manos)?, wrist: WristKit::new(&data.wrist, rig)? })
    }
}

// ---------------------------------------------------------------- whose each hand is

/// Who has a hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Owner {
    /// Nobody: the body's own business (swinging, or on a thigh).
    #[default]
    None,
    Tool,
    Control,
    Wrist,
    Gesture,
    Seat,
    /// Sitting, on its way back to its thigh.
    Rest,
}

/// What is told of a body's player this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Doing {
    /// The seat it is in: its ship's structure, which seat.
    pub seat: Option<(u64, usize)>,
    /// The way it looks (world, unit).
    pub look: DVec3,
    /// The control it aims at and the one it holds down (its own player's hand only).
    pub aim: Option<reach::Control>,
    pub held: Option<(u64, u16)>,
    /// Something loose is in its hands.
    pub carrying: bool,
}

/// A body's hands at work.
pub struct Handwork {
    kit: Arc<Kit>,
    pub reach: Reach,
    pub gesture: gestures::Playing,
    pub wrist: wrist::Wrist,
    owner: [Owner; 2],
    /// What is left of where each hand was (the body's frame, m; a turn), and how far that
    /// was when the hand set out (m).
    off: [Spring3; 2],
    turn: [SpringQ; 2],
    way: [f32; 2],
    /// Where each hand was asked last frame (the body's frame), and the body's frame then.
    last: [Option<(Vec3, Quat)>; 2],
    was: (DVec3, Quat),
    /// The way each elbow goes (model axes).
    pole: [Vec3; 2],
    /// Nothing of it was at work last frame.
    idle: bool,
}

/// An offset this small is gone (m, rad).
const SETTLED: (f32, f32) = (0.002, 0.02);

fn usual_pole(side: usize) -> Vec3 {
    ELBOW_POLE * Vec3::new(if side == 0 { 1.0 } else { -1.0 }, 1.0, 1.0)
}

impl Handwork {
    pub fn new(kit: Arc<Kit>) -> Handwork {
        Handwork { kit, reach: Reach::default(), gesture: gestures::Playing::default(), wrist: wrist::Wrist::default(), owner: [Owner::None; 2], off: Default::default(), turn: Default::default(), way: [0.0; 2], last: [None; 2], was: (DVec3::ZERO, Quat::IDENTITY), pole: [usual_pole(0), usual_pole(1)], idle: true }
    }

    pub fn kit(&self) -> &Kit {
        &self.kit
    }

    // ---- what the other players are told of this one, and what this body is told of its own

    /// The gesture it is making (its number in `gestos.jsonc`, from 1; 0: none).
    pub fn gesture(&self) -> u8 {
        self.gesture.id
    }

    /// Make this gesture (0: stop making any): the same number, told by its player.
    pub fn set_gesture(&mut self, id: u8) {
        self.gesture.set(&self.kit.gestures, id);
    }

    /// Its wrist computer is up.
    pub fn wrist_up(&self) -> bool {
        self.wrist.up
    }

    pub fn set_wrist(&mut self, up: bool) {
        self.wrist.up = up;
    }

    /// Its player worked control `control` of the ship on `structure` (`elem`: which button of
    /// it, if known): a hand goes to it. Not for what the seat's own keys fly (`drive` knows
    /// those: the hands stay on the stick and the throttle).
    pub fn worked(&mut self, structure: u64, control: u16, elem: Option<u8>) {
        self.reach.worked(structure, control, elem);
    }

    /// Whether anything here wants a hand, or still has one on its way back.
    fn busy(&self, d: &Doing) -> bool {
        !self.idle || self.reach.active() || self.gesture.id != 0 || self.wrist.up || d.seat.is_some()
    }

    /// A frame of the body: its hands where they are wanted — `tool`: where the tool in them
    /// has them (left, right); `cockpits`: the seats' own controls, as followed this frame —,
    /// then the body posed (`Body::update`).
    #[allow(clippy::too_many_arguments)]
    pub fn drive(&mut self, dt: f64, body: &mut Body, st: &Stance, set: &Structures, bodies: &BodyRegistry, ships: &Ships, cockpits: &Cockpits, tool: [Option<Grip>; 2], d: &Doing) {
        let frame = body.frame(st);
        if !self.busy(d) {
            // nothing of this: the body as it always was
            self.was = frame;
            body.update(dt, st, set, bodies, tool);
            return;
        }
        let dt32 = dt as f32;
        let kit = self.kit.clone();
        let scale = body.rig.def.escala;
        let arms: [Arm; 2] = [0, 1].map(|i| Arm { rig: &body.rig, side: i, shoulder: body.joint(body.rig.arms[i].upper, st), rot: frame.1, scale });
        let free = [tool[0].is_none() && !d.carrying, tool[1].is_none() && !d.carrying];
        // ---- who wants each hand
        // (a seat's own controls are flown from the stick and the throttle: no hand goes to them)
        let seat = d.seat.and_then(|(structure, seat)| cockpits.seat(structure, seat));
        let control = self.reach.update(dt32, &kit.reach, &reach::Scene { ships, set, aim: d.aim, held: d.held, flown: seat }, &arms, free);
        let wrist = self.wrist.update(dt32, &kit.wrist, &kit.reach.limits, body, st, &arms, free);
        // (the eyes: where the stance has them — sat or crouched they are not a standing body's
        // height over its feet)
        let gesture = self.gesture.update(dt32, &kit.gestures, &arms, d.look.as_vec3(), st.eye, free);
        let flown = seat.and_then(|s| {
            let (sh, at) = (&ships.list[ships.by_structure(s.structure)?], set.get(s.structure)?);
            Some(cockpit::hands(&kit.cockpit, s, sh, (at.pos, at.rot), free))
        });
        let shoulders = [arms[0].shoulder, arms[1].shoulder];
        let mut grips = tool;
        let mut live = false;
        for i in 0..2 {
            let (want, owner) = if let Some(g) = &tool[i] {
                (Some(Target::of(g)), Owner::Tool)
            } else if let Some(t) = control[i] {
                (Some(t), Owner::Control)
            } else if let Some(t) = wrist[i] {
                (Some(t), Owner::Wrist)
            } else if let Some(t) = gesture[i] {
                (Some(t), Owner::Gesture)
            } else if let Some(t) = flown.and_then(|f| f[i]) {
                (Some(t), Owner::Seat)
            } else if st.seated && self.owner[i] != Owner::None {
                // sitting, back to its thigh: the body's own rest, come to without a jump
                let rest = body.rig.def.sentado.manos[i].as_deref().and_then(|name| body.point(name, st));
                (rest.map(|(at, normal, across)| Target::of(&Grip { at, normal, across, fingers: [0.0; 5] })), Owner::Rest)
            } else {
                (None, Owner::None)
            };
            let Some(want) = want else {
                // (standing: the body lets the arm down by itself)
                (self.owner[i], self.last[i]) = (Owner::None, None);
                (self.off[i], self.turn[i]) = (Spring3::default(), SpringQ::default());
                self.pole[i] = usual_pole(i);
                body.poles[i] = None;
                continue;
            };
            // in the body's frame: what is left of where the hand was dies away
            let inv = frame.1.inverse();
            let local = (inv * (want.at - frame.0).as_vec3(), (inv * want.rot).normalize());
            if owner != self.owner[i] {
                let from = self.last[i].unwrap_or_else(|| {
                    let h = body.hands[i];
                    let was = self.was.1.inverse();
                    (was * (h.at - self.was.0).as_vec3(), (was * h.rot).normalize())
                });
                self.off[i].x = from.0 - local.0;
                self.way[i] = self.off[i].x.length();
                let mut q = from.1 * local.1.inverse();
                if q.w < 0.0 {
                    q = -q;
                }
                self.turn[i].x = q;
                self.owner[i] = owner;
            }
            // (a tool has its own way of moving in the hands: nothing is added to it but the
            // way to it from somewhere else)
            let off = self.off[i].step(Vec3::ZERO, kit.travel, dt32);
            let turn = self.turn[i].step(Quat::IDENTITY, kit.travel, dt32);
            let settled = off.length() < SETTLED.0 && turn.angle_between(Quat::IDENTITY) < SETTLED.1;
            // on its way it comes off the straight line, toward its own shoulder: off what it
            // was on and round to the next thing, not along it nor through it
            let gone = if self.way[i] > 0.02 { (1.0 - off.length() / self.way[i]).clamp(0.0, 1.0) } else { 1.0 };
            let middle = local.0 + off * 0.5;
            let shoulder = inv * (shoulders[i] - frame.0).as_vec3();
            let round = (shoulder - middle).normalize_or_zero() * (kit.arc * self.way[i].min(0.5) * 4.0 * gone * (1.0 - gone));
            let out = (local.0 + off + round, (turn * local.1).normalize());
            self.last[i] = Some(out);
            // the elbow: the way whoever has the hand says, come to as the hand is
            let pole = want.pole.unwrap_or(usual_pole(i));
            self.pole[i] = self.pole[i].lerp(pole, 1.0 - (-dt32 * std::f32::consts::LN_2 / (kit.travel * 2.0)).exp());
            let usual = (self.pole[i] - usual_pole(i)).length_squared() < 1e-4;
            body.poles[i] = (!usual).then_some(self.pole[i]);
            if owner == Owner::Rest && settled && usual {
                // on its thigh: the body's own again
                (self.owner[i], self.last[i]) = (Owner::None, None);
                continue;
            }
            live = true;
            if owner == Owner::Tool && settled {
                continue;
            }
            grips[i] = Some(Target { at: frame.0 + (frame.1 * out.0).as_dvec3(), rot: (frame.1 * out.1).normalize(), fingers: want.fingers, pole: None }.grip());
        }
        self.idle = !live;
        self.was = frame;
        body.update(dt, st, set, bodies, grips);
    }
}

// ---------------------------------------------------------------- the player's own

/// What the game tells of where its own player is, besides `Doing`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Here {
    /// Where the picture is taken from, and the ship the player is aboard.
    pub eye: DVec3,
    pub aboard: Option<u64>,
    /// The pack's gas (a share), the air round the suit (kPa), the pull there (m/s²).
    pub gas: Option<f32>,
    pub outside: f32,
    pub gravity: f32,
    /// Under a hull's lamps.
    pub inside: bool,
}

/// What the player's own game keeps of all this: its hands, the wheel it chooses gestures
/// with, its suit, the screen of its wrist computer, and the seats' props of the ships about
/// (which every body sat in one of them asks for its hands).
#[derive(Default)]
pub struct Own {
    pub hands: Option<Handwork>,
    pub wheel: gestures::Wheel,
    pub suit: wrist::Suit,
    pub cockpits: Cockpits,
    screen: wrist::Screen,
    font: Option<lunar_core::font::Font>,
    /// What it draws this frame (the props by the seats, the wrist computer), each thing in a
    /// frame of its own.
    scene: lunar_core::props::PropScene,
    /// Seconds out.
    clock: f64,
    /// A tool to see how the hands are turned: their own axes drawn on them, and each wrist's
    /// angles on the HUD (`axes`, `wrists`).
    pub axes: bool,
    /// Each wrist as the body was last posed, and whose each hand is (for that tool).
    wrists: [crate::body::WristPose; 2],
}

impl Own {
    /// From `assets/defs`, for bodies of `rig` (none: there is no body, and nothing of this).
    /// The models named are given to the renderer.
    pub fn load(defs: &Path, rig: Option<&RigDef>, models: &lunar_core::structure::models::Models, font: lunar_core::font::Font, r: &mut lunar_render::Renderer) -> Result<Own, String> {
        let Some(rig) = rig else { return Ok(Own::default()) };
        let kit = Arc::new(Kit::new(&HandData::load(defs)?, rig)?);
        let mut own = Own { hands: Some(Handwork::new(kit.clone())), font: Some(font), ..Own::default() };
        own.cockpits.models(&kit.cockpit, models, r)?;
        own.screen.model(&kit.wrist, models, r);
        Ok(own)
    }

    /// The data, for whoever makes the hands of another body (`Handwork::new`).
    pub fn kit(&self) -> Option<Arc<Kit>> {
        self.hands.as_ref().map(|h| h.kit.clone())
    }

    /// The gesture the player is making (0: none) and whether its wrist computer is up: what
    /// the other players are told.
    pub fn gesture(&self) -> u8 {
        self.hands.as_ref().map_or(0, Handwork::gesture)
    }

    pub fn wrist_up(&self) -> bool {
        self.hands.as_ref().is_some_and(Handwork::wrist_up)
    }

    /// The player worked a control (a click on it, the wheel).
    pub fn worked(&mut self, structure: u64, control: u16, elem: Option<u8>) {
        if let Some(h) = &mut self.hands {
            h.worked(structure, control, elem);
        }
    }

    /// The wrist computer's key: up, or down. How it is left (none: there is no body).
    pub fn toggle_wrist(&mut self) -> Option<bool> {
        let h = self.hands.as_mut()?;
        h.wrist.up = !h.wrist.up;
        Some(h.wrist.up)
    }

    pub fn set_wrist(&mut self, up: bool) {
        if let Some(h) = &mut self.hands {
            h.wrist.up = up;
        }
    }

    /// A click with nothing of a ship under it: with the wrist computer up, a tap on it.
    pub fn click(&mut self) -> bool {
        self.hands.as_mut().is_some_and(|h| h.wrist.tap())
    }

    /// The gestures' key, held or not: held, the wheel is open; let go, the gesture chosen is
    /// made (none chosen: whatever was being made stops).
    pub fn gesture_key(&mut self, held: bool) {
        let Some(h) = &mut self.hands else { return };
        let was = self.wheel.open;
        match self.wheel.hold(held) {
            Some(k) => h.gesture.start(&h.kit.gestures, k as u8 + 1),
            None if was && !held => h.gesture.stop(),
            None => {}
        }
    }

    /// A number key (from 0): the wheel's, if it is open.
    pub fn number(&mut self, k: usize) -> bool {
        let n = self.hands.as_ref().map_or(0, |h| h.kit.gestures.list.len());
        self.wheel.number(k, n)
    }

    /// The mouse: the wheel's, if it is open.
    pub fn mouse(&mut self, dx: f64, dy: f64) -> bool {
        let n = self.hands.as_ref().map_or(0, |h| h.kit.gestures.list.len());
        self.wheel.mouse(dx, dy, n)
    }

    /// A gesture by its id (a script's): whether there is one such.
    pub fn gesture_named(&mut self, id: &str) -> bool {
        let Some(h) = &mut self.hands else { return false };
        let n = h.kit.gestures.number(id);
        if n == 0 {
            h.gesture.stop();
        } else {
            h.gesture.start(&h.kit.gestures, n);
        }
        n != 0 || id.is_empty()
    }

    /// How far the look turns to read the wrist computer (to the right and up, rad), if it is
    /// up.
    pub fn look(&self, body: &Body, st: &Stance, view: &lunar_render::View) -> Option<(f64, f64)> {
        let h = self.hands.as_ref().filter(|h| h.wrist.up)?;
        wrist::Wrist::look(&h.kit.wrist, body, st, view)
    }

    /// A frame of the player's body (as `Handwork::drive`), of its suit and of what is drawn
    /// of all this.
    #[allow(clippy::too_many_arguments)]
    pub fn drive(&mut self, dt: f64, body: &mut Body, st: &Stance, set: &Structures, bodies: &BodyRegistry, ships: &Ships, tool: [Option<Grip>; 2], d: &Doing, here: &Here) {
        self.scene.clear();
        let Some(hands) = &mut self.hands else {
            body.update(dt, st, set, bodies, tool);
            return;
        };
        let kit = hands.kit.clone();
        self.clock += dt;
        self.suit.tick(dt as f32, &kit.wrist.def.traje, here.outside);
        self.cockpits.update(dt as f32, &kit.cockpit, ships, set, here.eye, here.aboard);
        hands.drive(dt, body, st, set, bodies, ships, &self.cockpits, tool, d);
        self.wrists = body.wrists;
        if self.axes {
            axes(body, st, here.inside, &mut self.scene);
        }
        self.cockpits.show(&kit.cockpit, ships, set, &mut self.scene);
        let Some(font) = &self.font else { return };
        let up = hands.wrist.up;
        if up {
            // the ship one is in, or the nearest
            let near = d.seat.map(|s| s.0).or(here.aboard).and_then(|id| ships.by_structure(id)).or_else(|| {
                let far = |n: usize| set.get(ships.list[n].structure).map(|s| (n, s.to_world(s.center).distance_squared(here.eye)));
                (0..ships.list.len()).filter_map(far).filter(|x| x.1 < NEAR * NEAR).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0)
            });
            let suit = wrist::Readings { oxygen: self.suit.oxygen, gas: here.gas, clock: self.clock, outside: here.outside, gravity: here.gravity };
            self.screen.read(dt as f32, &kit.wrist, hands.wrist.page, &suit, near.map(|n| &ships.list[n]));
        }
        self.screen.show(&kit.wrist, body, st, font, up, here.inside, &mut self.scene);
    }

    /// What it draws this frame, added to `out`.
    pub fn show(&self, out: &mut lunar_core::props::PropScene) {
        let base = out.frames.len() as u16;
        out.frames.extend(self.scene.frames.iter().copied());
        out.props.extend(self.scene.props.iter().map(|p| lunar_core::props::Prop { frame: p.frame + base, ..*p }));
        out.glyphs.extend(self.scene.glyphs.iter().map(|g| lunar_core::props::GlyphQuad { frame: g.frame + base, ..*g }));
    }

    /// The tool that shows how the hands are turned, on or off: how it is left.
    pub fn toggle_axes(&mut self) -> bool {
        self.axes = !self.axes;
        self.axes
    }

    /// How each wrist is, in words (left, right): for the HUD and for a script's log.
    pub fn wrists(&self) -> [String; 2] {
        let whose = self.hands.as_ref().map(|h| h.owner);
        std::array::from_fn(|i| {
            let w = &self.wrists[i];
            let who = match whose.map(|o| o[i]) {
                Some(Owner::Tool) => "herramienta",
                Some(Owner::Control) => "mando",
                Some(Owner::Wrist) => "ordenador",
                Some(Owner::Gesture) => "gesto",
                Some(Owner::Seat) => "mando de vuelo",
                Some(Owner::Rest) => "al muslo",
                _ if w.asked => "apoyada",
                _ => "suelta",
            };
            let turned = |deg: f32, plus: &str, minus: &str| if deg.abs() < 0.5 { "0°".to_string() } else { format!("{:.0}° {}", deg.abs(), if deg > 0.0 { plus } else { minus }) };
            let mut s = format!("{who} · dobla {} · ladea {} · gira {} · brazo {:.0} %", turned(w.flex, "a la palma", "al dorso"), turned(w.lean, "al meñique", "al pulgar"), turned(w.turn, "palma atrás", "palma delante"), w.reach * 100.0);
            if w.swivel.abs() > 1.0 {
                s.push_str(&format!(" · codo {:+.0}°", w.swivel));
            }
            if w.held {
                s.push_str(" · EN SU TOPE");
            }
            s
        })
    }

    /// What the HUD shows of it: the wheel while it is open, and how to get out of what is up.
    pub fn hud(&self, out: &mut crate::hud::Hud) {
        use crate::input::{Action, shown};
        if self.axes && self.hands.is_some() {
            let said = self.wrists();
            for (i, name) in ["Mano izq.", "Mano der."].into_iter().enumerate() {
                let level = if self.wrists[i].held { crate::hud::Level::Caution } else { crate::hud::Level::Normal };
                out.status.push((name.into(), said[i].clone(), level));
            }
        }
        let Some(h) = &self.hands else { return };
        let list = &h.kit.gestures.list;
        if self.wheel.open {
            out.wheel = Some(gestures::WheelView { names: list.iter().map(|g| g.name.clone()).collect(), pick: self.wheel.pick, at: self.wheel.at() });
            out.hints.push("Gestos: mueve el ratón hacia uno (o pulsa su número) y suelta la tecla".into());
        } else if let Some(g) = usize::from(h.gesture.id).checked_sub(1).and_then(|k| list.get(k)).filter(|g| g.hold) {
            out.hints.push(format!("{} · {}: dejar de hacerlo", g.name, shown(Action::Gesture)));
        }
        if h.wrist.up {
            out.hints.push(format!("Ordenador de muñeca · Clic: pasar página · {}: bajarlo", shown(Action::Wrist)));
        }
    }
}

/// The hands' own axes, drawn on them (a tool to see how a hand is turned): from the middle of
/// each palm, green the way the palm faces, blue the way the fingers point, red from the index
/// finger to the little one; from the wrist, thin and grey, where the palm would face and the
/// fingers point with the wrist at ease (as the forearm alone has them: what is between the two
/// is what the wrist does); and an amber ball on a wrist held at what a wrist does.
fn axes(body: &Body, st: &Stance, inside: bool, out: &mut lunar_core::props::PropScene) {
    use lunar_core::props::{BOX, Prop, PropFrame, SPHERE};
    let rig = &body.rig;
    for i in 0..2 {
        let (wrist, fore, hand) = body.wrist_frames(i, st);
        let palm = &rig.palms[i];
        let frame = out.frames.len() as u16;
        out.frames.push(PropFrame { pos: wrist, rot: Quat::IDENTITY, inside });
        let mut rod = |from: Vec3, way: Vec3, long: f32, thick: f32, color: [u8; 3]| {
            let way = way.normalize_or(Vec3::Y);
            out.props.push(Prop { frame, mesh: BOX, pos: from + way * (long * 0.5), rot: Quat::from_rotation_arc(Vec3::Y, way), size: Vec3::new(thick, long, thick), color, emissive: -2.5, rough: 200, metal: 0 });
            out.props.push(Prop { frame, mesh: SPHERE, pos: from + way * long, rot: Quat::IDENTITY, size: Vec3::splat(thick * 2.6), color, emissive: -2.5, rough: 200, metal: 0 });
        };
        let middle = (body.hands[i].at - wrist).as_vec3();
        rod(middle, hand * palm.normal, 0.10, 0.005, [60, 255, 90]);
        rod(middle, hand * palm.fingers, 0.13, 0.005, [70, 150, 255]);
        rod(middle, hand * palm.across, 0.08, 0.005, [255, 70, 60]);
        rod(Vec3::ZERO, fore * palm.normal, 0.13, 0.0025, [170, 170, 170]);
        rod(Vec3::ZERO, fore * palm.fingers, 0.19, 0.0025, [170, 170, 170]);
        if body.wrists[i].held {
            out.props.push(Prop { frame, mesh: SPHERE, pos: Vec3::ZERO, rot: Quat::IDENTITY, size: Vec3::splat(0.03), color: [255, 180, 40], emissive: -3.0, rough: 200, metal: 0 });
        }
    }
}

/// How far the nearest ship is looked for, for the wrist computer (m).
const NEAR: f64 = 400.0;

/// The hand data and the kit for the rig at `assets/defs/rigs/astronauta.jsonc`: for tests.
#[cfg(test)]
pub fn test_kit() -> (HandData, RigDef, Kit) {
    let root = crate::root();
    let data = HandData::load(&root.join("assets/defs")).unwrap_or_else(|e| panic!("{e}"));
    let rig: RigDef = lunar_core::defs::load(&root.join("assets/defs/rigs/astronauta.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let kit = Kit::new(&data, &rig).unwrap_or_else(|e| panic!("{e}"));
    (data, rig, kit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::{Rig, Rigged};
    use lunar_core::{
        body::{Body as Planet, BodyDef},
        defs,
        scenario::ScenarioDef,
        scene::Site,
    };

    /// The Moon, a ship of a kind standing on it with its systems, and the game's astronaut
    /// with its hands (none: its model is not there).
    struct Stage {
        set: Structures,
        ships: Ships,
        bodies: Arc<BodyRegistry>,
        id: u64,
        body: Body,
        hands: Handwork,
        cockpits: Cockpits,
        kit: Arc<Kit>,
    }

    fn stage(kind: &str) -> Option<Stage> {
        let root = crate::root();
        let dir = root.join("assets/defs");
        let (_, def, kit) = test_kit();
        let model = Rigged::load(&root.join("assets/models").join(format!("{}.glb", def.modelo))).ok()?;
        let body = Body::new(Rig::new(def, &model).unwrap(), None, None);
        let mut lib = lunar_core::structure::Library::load(&dir.join("structures")).unwrap();
        let (kinds, bps) = lunar_ship::ShipLibrary::load(&dir, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        lib.blueprints.extend(bps);
        let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        let sc: ScenarioDef = defs::parse("scenario", include_str!("../../../assets/defs/scenario.jsonc")).unwrap();
        let bodies = Arc::new(BodyRegistry::new(vec![Planet::from_def("luna", &moon).unwrap()]));
        let site = Site::from_def(&sc.site, &bodies).unwrap();
        let kind = kinds.get(kind).unwrap().clone();
        let mut set = Structures::new(Arc::new(lib));
        let id = set.place(&kind.blueprint, &bodies, 0, site.at(40.0, 40.0), 0.0, f64::from(kind.lift)).unwrap();
        let mut ship = lunar_ship::Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
        ship.update(&mut set.list[0], &lunar_ship::World::default(), 0.0);
        set.rest_on_ground(id, &bodies);
        let font = lunar_core::font::Font::parse(&std::fs::read_to_string(root.join("assets/fonts/serigrafia.json")).unwrap()).unwrap();
        let mut ships = Ships::new(vec![kind], font);
        ships.list.push(ship);
        let kit = Arc::new(kit);
        Some(Stage { set, ships, bodies, id, body, hands: Handwork::new(kit.clone()), cockpits: Cockpits::default(), kit })
    }

    impl Stage {
        /// The stance of whoever sits in seat `k` of the ship.
        fn seated(&self, k: usize) -> Stance {
            let s = self.set.get(self.id).unwrap();
            let d = &self.ships.list[0].kind.seats[k].def;
            let yaw = d.rumbo.to_radians();
            Stance {
                eye: s.to_world(Vec3::from_array(d.ojos)),
                up: (s.rot * Vec3::Y).as_dvec3(),
                ahead: (s.rot * Vec3::new(yaw.sin(), 0.0, yaw.cos())).as_dvec3(),
                eye_h: 1.2,
                vel: DVec3::ZERO,
                grounded: true,
                g: 1.62,
                ride: Some(self.id),
                seated: true,
                inside: true,
                own_eyes: false,
            }
        }

        /// The stance of whoever stands on the ground a way off the ship.
        fn standing(&self) -> Stance {
            let s = self.set.get(self.id).unwrap();
            let moon = self.bodies.get(0);
            let up = moon.up(s.pos + (s.rot * Vec3::new(30.0, 0.0, 0.0)).as_dvec3());
            let ahead = up.any_orthonormal_vector();
            let eye_h = f64::from(self.body.eye_height());
            Stance { eye: moon.above_ground(up, eye_h), up, ahead, eye_h, vel: DVec3::ZERO, grounded: true, g: 1.62, ride: None, seated: false, inside: false, own_eyes: true }
        }

        fn frames(&mut self, n: usize, st: &Stance, d: &Doing) {
            for _ in 0..n {
                self.cockpits.update(1.0 / 60.0, &self.kit.cockpit, &self.ships, &self.set, st.eye, Some(self.id));
                self.hands.drive(1.0 / 60.0, &mut self.body, st, &self.set, &self.bodies, &self.ships, &self.cockpits, [None, None], d);
            }
        }

        /// How each wrist is, in a line.
        fn wrists(&self) -> String {
            let w = self.body.wrists;
            format!("izq dobla {:+.0}° ladea {:+.0}° gira {:+.0}° brazo {:.0} %{} | der dobla {:+.0}° ladea {:+.0}° gira {:+.0}° brazo {:.0} %{}", w[0].flex, w[0].lean, w[0].turn, w[0].reach * 100.0, if w[0].held { " TOPE" } else { "" }, w[1].flex, w[1].lean, w[1].turn, w[1].reach * 100.0, if w[1].held { " TOPE" } else { "" })
        }
    }

    #[test]
    fn sat_at_any_ships_controls_the_hands_hold_the_stick_and_the_throttle_as_hands_do() {
        // (the left hand was on its throttle backwards, its fingers into its own forearm)
        for name in ["alcotan", "abejorro", "cachalote"] {
            let Some(mut g) = stage(name) else { return };
            let kind = g.ships.list[0].kind.clone();
            let mut flown = 0;
            for k in (0..kind.seats.len()).filter(|k| !kind.seats[*k].def.mandos.is_empty()) {
                flown += 1;
                let st = g.seated(k);
                let doing = Doing { seat: Some((g.id, k)), look: st.ahead, ..Doing::default() };
                g.frames(150, &st, &doing);
                let who = format!("{name}, asiento {}", kind.seats[k].def.id);
                eprintln!("{who}: {}", g.wrists());
                let (s, sh) = (g.set.get(g.id).unwrap(), &g.ships.list[0]);
                let seat = g.cockpits.seat(g.id, k).unwrap_or_else(|| panic!("{who}: no se sigue lo que se vuela desde él"));
                let want = cockpit::hands(&g.kit.cockpit, seat, sh, (s.pos, s.rot), [true, true]);
                for i in 0..2 {
                    let t = want[i].unwrap_or_else(|| panic!("{who}: la mano {i} no tiene mando"));
                    let (h, w) = (g.body.hands[i], g.body.wrists[i]);
                    // on its control, its wrist doing what a wrist does, its arm not stretched
                    assert!(h.at.distance(t.at) < 0.015, "{who}: la mano {i} a {:.3} m de su mando", h.at.distance(t.at));
                    assert!(!w.held, "{who}: la muñeca {i} en su tope: {w:?}");
                    assert!(w.flex.abs() < 45.0 && w.lean.abs() < 25.0 && w.turn.abs() < 89.0, "{who}: la muñeca {i} forzada: {w:?}");
                    assert!(w.reach < 0.95 && g.body.short[i] == Vec3::ZERO, "{who}: el brazo {i} estirado ({:.0} %)", w.reach * 100.0);
                    // its fingers go on from its forearm, ahead: not back into it
                    let (_, fore, hand) = g.body.wrist_frames(i, &st);
                    let fingers = g.body.rig.palms[i].fingers;
                    assert!((hand * fingers).dot(fore * fingers) > 0.6, "{who}: la mano {i} doblada contra su antebrazo");
                    assert!((hand * fingers).as_dvec3().dot(st.ahead) > 0.0 || (hand * fingers).as_dvec3().dot(st.up) > 0.3, "{who}: los dedos de la mano {i} hacia atrás");
                    // and the elbow is down and out, not up over the shoulder nor across the chest
                    let arm = g.body.rig.arms[i];
                    let (shoulder, elbow) = (g.body.joint(arm.upper, &st), g.body.joint(arm.lower, &st));
                    assert!((elbow - shoulder).dot(st.up) < 0.02, "{who}: el codo {i} por encima del hombro");
                }
            }
            assert!(flown > 0, "{name}: no se vuela desde ningún asiento");
        }
    }

    #[test]
    fn every_control_of_a_seats_panels_is_worked_with_a_wrist_that_does_what_a_wrist_does() {
        let Some(mut g) = stage("alcotan") else { return };
        for name in ["alcotan", "abejorro", "cachalote"] {
            if name != "alcotan" {
                g = stage(name).unwrap();
            }
            let kind = g.ships.list[0].kind.clone();
            let range = g.body.rig.def.muneca;
            let (mut worked, mut held, mut far, mut worst) = (0, Vec::new(), 0, 0.0f32);
            for k in 0..kind.seats.len() {
                let seat = &kind.seats[k].def;
                if seat.paneles.is_empty() {
                    continue;
                }
                let st = g.seated(k);
                let doing = Doing { seat: Some((g.id, k)), look: st.ahead, ..Doing::default() };
                g.frames(60, &st, &doing);
                let controls: Vec<(usize, String)> = g.ships.list[0].panels.controls.iter().enumerate().filter(|(_, c)| seat.paneles.contains(&kind.panels[c.panel].id)).map(|(n, c)| (n, c.id.clone())).collect();
                for (n, id) in controls {
                    // (what the seat's own keys fly is worked from the stick and the throttle)
                    if g.cockpits.seat(g.id, k).is_some_and(|s| s.flies(n as u16)) {
                        continue;
                    }
                    g.hands.worked(g.id, n as u16, None);
                    g.frames(24, &st, &doing);
                    worked += 1;
                    let Some(i) = (0..2).find(|i| g.hands.owner[*i] == Owner::Control) else { panic!("{name}, {id}: ninguna mano va al mando") };
                    let w = g.body.wrists[i];
                    assert!(w.flex.is_finite() && w.flex <= range.flexion + 0.5 && w.flex >= -range.extension - 0.5 && w.lean <= range.cubital + 0.5 && w.lean >= -range.radial - 0.5 && w.turn.abs() <= range.pronacion.max(range.supinacion) + 0.5, "{name}, {id}: {w:?}");
                    // an arm is never stretched straight for a control: one out of reach is worked from short of it
                    assert!(w.reach < 0.985, "{name}, {id}: el brazo estirado ({:.1} %)", w.reach * 100.0);
                    worst = worst.max(w.reach);
                    if w.held {
                        held.push(id.clone());
                    }
                    far += usize::from(g.hands.reach.short[i] > 0.01);
                    // (the hand back before the next one)
                    g.frames(100, &st, &doing);
                }
            }
            eprintln!("{name}: {worked} mandos accionados desde sus asientos; {} con la muñeca en su tope{}; {far} fuera del alcance del brazo (se accionan desde donde llega); el brazo, como mucho al {:.0} %", held.len(), if held.is_empty() { String::new() } else { format!(" ({})", held.iter().take(12).cloned().collect::<Vec<_>>().join(", ")) }, worst * 100.0);
            assert!(worked > 5, "{name}: {worked} mandos");
            assert!(held.len() * 4 <= worked, "{name}: {} de {worked} mandos se accionan con la muñeca en su tope", held.len());
        }
    }

    #[test]
    fn a_control_out_of_reach_is_not_stretched_for_and_the_way_there_has_no_jump() {
        let Some(g) = stage("abejorro") else { return };
        let st = g.standing();
        let (frame, scale) = (g.body.frame(&st), g.body.rig.def.escala);
        let arm = Arm { rig: &g.body.rig, side: 1, shoulder: g.body.joint(g.body.rig.arms[1].upper, &st), rot: frame.1, scale };
        let lim = g.kit.reach.limits;
        let pose = g.kit.reach.works[g.kit.reach.work_of("pulsador", 1)].pose;
        // a button on a wall ahead of the shoulder, from 0.3 m to 2.4 m off (as far as a click reaches)
        let (ahead, out) = (st.ahead.as_vec3(), -st.ahead.as_vec3());
        let wrist_far = |p: &Placed| {
            let joint = g.body.rig.skeleton.bind[g.body.rig.arms[1].end].pos;
            let palm = g.body.rig.palms[1];
            let hand = p.target.rot * frame_turn(Vec3::Y, Vec3::X, palm.normal, palm.across).inverse();
            ((p.target.at - (hand * (palm.at - joint) * scale).as_dvec3()) - arm.shoulder).length() as f32 / arm.reach()
        };
        let (mut last, mut steps) = (None::<f32>, 0);
        let mut d = 0.3f32;
        while d < 2.4 {
            let at = arm.shoulder + (ahead * d).as_dvec3() - st.up * 0.1;
            let p = poke(&arm, &lim, Tip::Indice, pose, at, out, 0.0, 0.0);
            let share = wrist_far(&p);
            assert!(share <= lim.arm + 0.01, "a {d:.2} m: el brazo al {:.0} %", share * 100.0);
            if d < 0.55 {
                // within reach: the fingertip on it
                assert!(p.short < 1e-3 && p.point.distance(at) < 1e-3, "a {d:.2} m no llega: le faltan {:.3} m", p.short);
            }
            if d > 1.2 {
                // well out of reach: the arm held out no more than it is for that
                assert!(p.short > 0.3 && (share - lim.far).abs() < 0.03, "a {d:.2} m: el brazo al {:.0} %", share * 100.0);
            }
            if let Some(was) = last {
                assert!((share - was).abs() < 0.02, "de {:.2} a {d:.2} m la mano salta: {:.0} % -> {:.0} %", d - 0.01, was * 100.0, share * 100.0);
            }
            (last, steps) = (Some(share), steps + 1);
            d += 0.01;
        }
        assert!(steps > 200);
    }

    #[test]
    fn the_wrist_computer_is_raised_with_its_screen_to_the_eyes() {
        // (it faced up: one read it edge on)
        let Some(mut g) = stage("alcotan") else { return };
        let pilot = (0..g.ships.list[0].kind.seats.len()).find(|k| !g.ships.list[0].kind.seats[*k].def.mandos.is_empty()).unwrap();
        for seated in [false, true] {
            let st = if seated { g.seated(pilot) } else { g.standing() };
            let doing = Doing { seat: seated.then_some((g.id, pilot)), look: st.ahead, ..Doing::default() };
            g.hands.set_wrist(false);
            g.frames(90, &st, &doing);
            g.hands.set_wrist(true);
            g.frames(150, &st, &doing);
            let how = if seated { "sentado" } else { "de pie" };
            eprintln!("ordenador de muñeca, {how}: {}", g.wrists());
            let (at, normal, right, up) = wrist::Wrist::screen(&g.kit.wrist, &g.body, &st).unwrap();
            let (_, rot) = g.body.frame(&st);
            let eyes = st.eye;
            let to = (eyes - at).as_vec3().normalize();
            // toward the eyes, a reading distance from them, where its data says
            let facing = normal.dot(to).clamp(-1.0, 1.0).acos().to_degrees();
            assert!(facing < 20.0, "{how}: la pantalla mira {facing:.0}° fuera de los ojos");
            let far = eyes.distance(at);
            assert!((0.2..0.4).contains(&far), "{how}: la pantalla a {far:.2} m de los ojos");
            let en = g.kit.wrist.def.arriba.en;
            let want = eyes + (rot * Vec3::new(en[0], en[1], en[2])).as_dvec3();
            assert!(at.distance(want) < 0.04, "{how}: la pantalla a {:.3} m de donde se pide", at.distance(want));
            // its lines run to the reader's right and its top is the far side: the right way up
            let left = rot * Vec3::X;
            assert!(right.dot(left) < -0.5, "{how}: sus renglones no van hacia la derecha: {:.2}", right.dot(left));
            assert!(up.dot(rot * Vec3::new(0.0, 0.6, 0.8)) > 0.5, "{how}: está del revés");
            // the arm that wears it: not stretched, its wrist at ease, its elbow out and no higher than its shoulder
            let w = g.body.wrists[0];
            assert!(!w.held && w.flex.abs() < 12.0 && w.lean.abs() < 12.0 && w.turn.abs() < 12.0, "{how}: la muñeca que lo lleva no está suelta: {w:?}");
            assert!(w.reach < 0.97, "{how}: el brazo al {:.0} %", w.reach * 100.0);
            let arm = g.body.rig.arms[0];
            let (shoulder, elbow) = (g.body.joint(arm.upper, &st), g.body.joint(arm.lower, &st));
            let out = (elbow - shoulder).as_vec3();
            assert!(out.dot(left) > -0.05 && out.dot(rot * Vec3::Y) < 0.12, "{how}: el codo en {:?} del hombro (ejes del cuerpo)", rot.inverse() * out);
            // a tap: the other index comes to the glass, and the page turns
            let page = g.hands.wrist.page;
            assert!(g.hands.wrist.tap());
            let mut nearest = f64::MAX;
            for _ in 0..40 {
                g.frames(1, &st, &doing);
                let (screen, ..) = wrist::Wrist::screen(&g.kit.wrist, &g.body, &st).unwrap();
                let (tip, _) = hand_point(&g.body.rig, 1, Tip::Indice, &g.kit.wrist.tap_pose());
                let (_, _, hand) = g.body.wrist_frames(1, &st);
                let joint = g.body.rig.skeleton.bind[g.body.rig.arms[1].end].pos;
                let finger = g.body.wrist_frames(1, &st).0 + (hand * (tip - joint) * g.body.rig.def.escala).as_dvec3();
                nearest = nearest.min(finger.distance(screen));
            }
            assert_ne!(g.hands.wrist.page, page, "{how}: el toque no pasa la página");
            assert!(nearest < 0.03, "{how}: el índice se queda a {nearest:.3} m del cristal");
            g.hands.set_wrist(false);
        }
    }

    #[test]
    fn clapping_the_palms_meet() {
        // (they stopped a hand's breadth apart)
        let Some(mut g) = stage("abejorro") else { return };
        let st = g.standing();
        let doing = Doing { look: st.ahead, ..Doing::default() };
        g.frames(30, &st, &doing);
        let n = g.kit.gestures.number("aplaudir");
        assert!(n != 0);
        g.hands.gesture.start(&g.kit.gestures, n);
        // the skin of a palm is this far behind its point (the middle of a handle held)
        let skin = g.body.rig.def.mango * 0.5 * g.body.rig.def.escala;
        let (mut nearest, mut farthest) = (f32::MAX, f32::MIN);
        for k in 0..110 {
            g.frames(1, &st, &doing);
            if k < 24 {
                continue;
            }
            let (l, r) = (g.body.hands[0], g.body.hands[1]);
            // from the left palm's skin to the right palm's, along the way the left one faces
            let gap = (r.at - l.at).as_vec3().dot(l.rot * Vec3::Y) + 2.0 * skin;
            (nearest, farthest) = (nearest.min(gap), farthest.max(gap));
            // palm to palm
            assert!((l.rot * Vec3::Y).dot(r.rot * Vec3::Y) < -0.9, "las palmas no se miran");
        }
        eprintln!("aplaudir: entre palma y palma de {:.1} a {:.1} cm; {}", nearest * 100.0, farthest * 100.0, g.wrists());
        assert!(nearest < 0.02 && nearest > -0.015, "las palmas quedan a {:.3} m", nearest);
        assert!(farthest > 0.15, "no se separan: {farthest:.3} m");
    }

    /// How far into the trunk each arm of the body is as last posed (model units): its vertices
    /// as the renderer would place them (from halfway down the upper arm: what is by the
    /// armpit is always against it) against the trunk's front as the renderer would place it
    /// (`bulk::Trunk` of the posed mesh, the bones' lines and thickness the solver goes by), all
    /// in the chest's frame (a chest turned to reach is told from its own sides), and where the
    /// worst is.
    fn arms_into_trunk(g: &Stage, model: &Rigged, st: &Stance) -> [(f32, Vec3, String); 2] {
        let mut scene = lunar_core::anim::BodyScene::default();
        g.body.show(&mut scene, st);
        let (sk, rig) = (&g.body.rig.skeleton, &g.body.rig);
        let chest = rig.bulk.chest.unwrap();
        let to_chest = sk.bind[chest].then(g.body.xf(chest).inverse());
        let mut posed = model.mesh.clone();
        for (i, p) in posed.pos.iter_mut().enumerate() {
            let v = Vec3::from(model.mesh.pos[i]);
            let mut o = Vec3::ZERO;
            for k in 0..4 {
                let b = &scene.bones[usize::from(model.mesh.joints[i][k])];
                o += model.mesh.weights[i][k] * Vec3::new(b[0] * v.x + b[1] * v.y + b[2] * v.z + b[3], b[4] * v.x + b[5] * v.y + b[6] * v.z + b[7], b[8] * v.x + b[9] * v.y + b[10] * v.z + b[11]);
            }
            *p = to_chest.point(o).to_array();
        }
        let strongest = |i: usize| {
            let w = posed.weights[i];
            usize::from(posed.joints[i][(0..4).max_by(|&a, &b| w[a].total_cmp(&w[b])).unwrap()])
        };
        let trunk_bones: Vec<usize> = std::iter::once(rig.pelvis).chain(rig.spine.iter().copied()).collect();
        let trunk = crate::bulk::Trunk::measure(&posed, &trunk_bones);
        std::array::from_fn(|side| {
            let (arm, own) = (rig.arms[side], if side == 0 { 1.0 } else { -1.0 });
            let trunk = trunk.grown(0.0, own, rig.bulk.side);
            let (head, elbow) = (sk.bind[arm.upper].pos, sk.bind[arm.lower].pos);
            let mut worst = (0.0f32, Vec3::ZERO, String::new());
            for i in 0..posed.pos.len() {
                let b = strongest(i);
                if !sk.under(b, arm.upper) || (b == arm.upper && (Vec3::from(model.mesh.pos[i]) - head).dot(elbow - head) < 0.5 * (elbow - head).length_squared()) {
                    continue;
                }
                let p = Vec3::from(posed.pos[i]);
                let d = trunk.depth(p);
                if d > worst.0 {
                    worst = (d, p, sk.names[b].clone());
                }
            }
            worst
        })
    }

    #[test]
    fn an_arm_on_a_tool_goes_round_the_trunk_not_through_it() {
        // (the left elbow went into the chest holding the launcher's front grip: its upper arm
        // 14 cm into the trunk)
        let Some(mut g) = stage("abejorro") else { return };
        let model = Rigged::load(&crate::root().join("assets/models/astronauta.glb")).unwrap();
        let tools: Vec<crate::gear::ToolDef> = lunar_core::defs::parse("gear", include_str!("../../../assets/defs/gear.jsonc")).unwrap();
        let st = g.standing();
        let doing = Doing { look: st.ahead, ..Doing::default() };
        // with nothing in its hands: as far as a hanging arm is "in" (its sleeve by its side)
        g.frames(60, &st, &doing);
        let rest = arms_into_trunk(&g, &model, &st);
        eprintln!("sin nada: izq {:.3} ({}), der {:.3} ({})", rest[0].0, rest[0].2, rest[1].0, rest[1].2);
        assert!(rest[0].0 < 0.005 && rest[1].0 < 0.005, "colgando, el brazo ya está dentro: lo que se mide no es lo que se busca");
        for pitch in [0.0f64, -25.0, 30.0] {
            for t in tools.iter().filter(|t| !t.sujecion.manos.is_empty()) {
                let p = pitch.to_radians();
                let forward = st.ahead * p.cos() + st.up * p.sin();
                let view = lunar_render::View { eye: st.eye, forward, up: st.up, fov_y: 1.0, near: 0.1 };
                let mut holding = crate::holding::Holding::default();
                let doing = Doing { look: forward, ..Doing::default() };
                for _ in 0..90 {
                    let pose = holding.update(1.0 / 60.0, &t.sujecion, crate::gear::eye_frame(&view), crate::gear::level_frame(&view), Default::default(), Vec3::ZERO, false);
                    let grips = crate::gear::Gear::grips_of(&pose, &view, &g.body, &st);
                    g.cockpits.update(1.0 / 60.0, &g.kit.cockpit, &g.ships, &g.set, st.eye, Some(g.id));
                    g.hands.drive(1.0 / 60.0, &mut g.body, &st, &g.set, &g.bodies, &g.ships, &g.cockpits, grips, &doing);
                }
                let into = arms_into_trunk(&g, &model, &st);
                eprintln!("{} mirando {pitch:+}°: izq {:.3} ({} en {:.2?}), der {:.3} ({}); {}; codos {:.0}° {:.0}°", t.id, into[0].0, into[0].2, into[0].1, into[1].0, into[1].2, g.wrists(), g.body.wrists[0].swivel, g.body.wrists[1].swivel);
                for side in 0..2 {
                    assert!(into[side].0 <= 0.02, "{} mirando {pitch}°: el brazo {side} se mete {:.3} en el tronco ({} en {:?})", t.id, into[side].0, into[side].2, into[side].1);
                }
            }
        }
    }
}
