//! The player's own body: the suit one looks down at, the arms and gloves that hold the tools,
//! the legs that walk, the shadow on the ground. A rigged model (`rig`) posed every frame from
//! what the player is doing — nothing of it is a recorded animation:
//!
//! - it hangs from the eyes: the camera is where its eyes are, its feet where the walker's are,
//!   and what is between bends to fit (crouching, a step taken, a landing);
//! - its feet are on the ground, each where the gait has it (`lunar_core::anim::gait`): set down
//!   on the terrain or the deck under it, lifted and carried to the next place; its legs reach
//!   them by inverse kinematics, knees ahead;
//! - its hands go where they are asked (`Grip`: a tool's grips, something on the suit) with the
//!   elbows down, and close on what they hold; with nothing to hold, the arms swing with the
//!   steps and rise a little off the ground;
//! - a wrist only bends and turns so far (`WristRange`, the rig's): a hand asked past that has
//!   its elbow go round to ease it, and what is left over the hand gives — its palm stays where
//!   it was asked, turned as far as a wrist goes (`ease`);
//! - seated, it sits.
use crate::rig::{Palm, Rig, WristAxes, WristRange, palm_turn};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    anim::{
        BodyDraw, BodyScene, Pose,
        gait::{Gait, Walker},
        skeleton::frame_turn,
        spring::Spring,
    },
    body::BodyRegistry,
    structure::set::Structures,
};

/// How the body stands this frame.
#[derive(Clone, Copy, Debug)]
pub struct Stance {
    /// The eyes (where the camera is), the way up and the way the body faces (level, unit).
    pub eye: DVec3,
    pub up: DVec3,
    pub ahead: DVec3,
    /// How high the eyes are over the feet now (m).
    pub eye_h: f64,
    /// How fast it goes over what it stands on (world axes).
    pub vel: DVec3,
    pub grounded: bool,
    /// Its weight per kilo here (m/s²).
    pub g: f64,
    /// What it stands on or goes with (a structure), if anything.
    pub ride: Option<u64>,
    pub seated: bool,
    /// Under a hull's lamps.
    pub inside: bool,
    /// Seen from its own eyes (else from outside: with its helmet on).
    pub own_eyes: bool,
}

/// Where a hand is asked to be (world): the middle of its palm, the way the palm faces, the way
/// from its index finger to its little one; how closed each finger is, thumb first (-1 flat, 0
/// at rest, 1 a fist).
#[derive(Clone, Copy, Debug)]
pub struct Grip {
    pub at: DVec3,
    pub normal: Vec3,
    pub across: Vec3,
    pub fingers: [f32; 5],
}

/// Where a hand is (world): its palm's middle and its frame — x from index to little finger,
/// y the way the palm faces, z the two crossed — for whatever it carries.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hand {
    pub at: DVec3,
    pub rot: Quat,
}

/// How a wrist is, as the body was last posed (degrees): bent toward the palm (negative: toward
/// the back of the hand), leant toward the little finger (negative: the thumb), turned about
/// the forearm palm back (negative: palm ahead); whether its hand was asked past what a wrist
/// does and held to that; how far round its elbow went to ease it; and how much of its arm's
/// length the hand is from its shoulder (1: stretched).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WristPose {
    pub flex: f32,
    pub lean: f32,
    pub turn: f32,
    pub held: bool,
    pub swivel: f32,
    pub reach: f32,
    /// Its hand is asked somewhere (else it hangs, as the body has it).
    pub asked: bool,
}

/// A foot's ankle over its sole and how far ahead its ball is (model units, at rest).
#[derive(Clone, Copy, Debug)]
struct FootShape {
    ankle: Vec3,
    ball: Vec3,
}

pub struct Body {
    pub rig: Rig,
    /// The model to its own eyes and whole (as the renderer numbered them).
    seen: Option<u16>,
    whole: Option<u16>,
    pub gait: Gait,
    pose: Pose,
    /// The frame the gait's feet are in: a structure's, or the world's.
    ground: Option<u64>,
    feet: [FootShape; 2],
    stand_eye: f32,
    fingers: [[Spring; 5]; 2],
    /// 0 arms down .. 1 held out (off the ground).
    aloft: Spring,
    /// How far the hips are down for the legs to reach their feet (model units).
    sag: Spring,
    /// 0 no grip .. 1 on it, per hand: a hand goes to a grip and comes off it, it does not jump.
    reach: [Spring; 2],
    /// How far the chest is turned for the hands to reach (rad, to its left), and how far each
    /// shoulder is brought round (0..1).
    twist: Spring,
    shrug: [Spring; 2],
    last_grip: [Option<(Vec3, Quat)>; 2],
    /// The way each elbow goes when its hand is asked somewhere, if not down and out (model
    /// axes): whoever asks sets it (`handwork`: an arm raised to be read, a salute).
    pub poles: [Option<Vec3>; 2],
    pub hands: [Hand; 2],
    /// How far the hands fell short of where they were asked to be (world; zero if they
    /// reached): whoever holds a tool out brings it in by this.
    pub short: [Vec3; 2],
    /// Each wrist as it was last posed, and how far round each elbow has gone to ease it (rad).
    pub wrists: [WristPose; 2],
    swivel: [Spring; 2],
    clock: f64,
}

/// A foot's toes go down as it leaves the ground and up before it comes down (rad).
const TOE_OFF: f32 = 0.5;
const TOE_UP: f32 = 0.25;
/// The heel comes up behind the body as far as this (rad).
const HEEL_OFF: f32 = 0.5;
/// A hand asked further than its arm goes: the chest turns to it as far as this (rad) and the
/// shoulder comes round as far as this (rad).
const TWIST: f32 = 0.6;
const SHRUG: f32 = 0.6;
/// How far the arms swing at a walk (rad), and how bent the elbow is at rest and walking.
/// Seated: how far ahead of the knee the floor under the foot is looked for (model units).
const SHIN_AHEAD: f32 = 0.06;
const ARM_SWING: f32 = 0.22;
const ELBOW: [f32; 2] = [0.2, 0.42];
/// The way an elbow goes when its hand is asked somewhere (model axes, the left arm's: x is
/// its own side): down and out.
pub const ELBOW_POLE: Vec3 = Vec3::new(0.55, -1.0, -0.35);
/// An arm asked to put its hand somewhere is solved from this bend of its elbow (rad, forward
/// from rest): the solver keeps the forearm turned as it was to the plane the arm bends in, so
/// from one bend a hand asked the same is turned the same whatever the arm was doing before
/// (hanging, swinging, sat).
const HINGE: f32 = 0.5;

/// The side of forearm `side` (a way square to it, model space at rest) that the solver lays
/// along the normal of the plane the arm bends in (`anim::ik`: the side its elbow goes to,
/// crossed with the way to the hand): whoever needs to know how a forearm will be turned
/// before it is posed (what is worn on it, `wrist`) tells by this.
pub fn hinge(rig: &Rig, side: usize) -> Vec3 {
    let (sk, arm) = (&rig.skeleton, rig.arms[side]);
    let (a, b, c) = (sk.bind[arm.upper].pos, sk.bind[arm.lower].pos, sk.bind[arm.end].pos);
    let along = (c - b).normalize_or(Vec3::NEG_Y);
    let n = (Quat::from_rotation_x(HINGE) * (b - a)).cross(c - b);
    (n - along * n.dot(along)).normalize_or(along.any_orthonormal_vector())
}

impl Body {
    pub fn new(rig: Rig, seen: Option<u16>, whole: Option<u16>) -> Body {
        let sk = &rig.skeleton;
        let feet = std::array::from_fn(|i| {
            let ankle = sk.bind[rig.legs[i].end].pos;
            let ball = rig.toes[i].map_or(ankle + Vec3::new(0.0, -ankle.y, 0.12), |t| sk.bind[t].pos);
            FootShape { ankle: Vec3::new(0.0, ankle.y, 0.0), ball: Vec3::new(0.0, 0.0, (ball.z - ankle.z).max(0.05)) }
        });
        let pose = Pose::rest(sk);
        let stand_eye = rig.def.ojos[1] * rig.def.escala;
        Body {
            seen,
            whole,
            gait: Gait::default(),
            pose,
            ground: None,
            feet,
            stand_eye,
            fingers: Default::default(),
            aloft: Spring::default(),
            sag: Spring::default(),
            reach: Default::default(),
            twist: Spring::default(),
            shrug: Default::default(),
            last_grip: [None; 2],
            poles: [None; 2],
            hands: Default::default(),
            short: [Vec3::ZERO; 2],
            wrists: Default::default(),
            swivel: Default::default(),
            clock: 0.0,
            rig,
        }
    }

    /// How high its eyes are standing (m): what the walker's eye height should be for it.
    #[cfg(test)]
    pub fn eye_height(&self) -> f32 {
        self.stand_eye
    }

    /// A body point's place now (world), as a place a hand may be at.
    pub fn point(&self, name: &str, s: &Stance) -> Option<(DVec3, Vec3, Vec3)> {
        let (bone, rel) = self.rig.points.get(name)?;
        let x = self.pose.model[*bone].then(*rel);
        let (origin, rot) = self.frame(s);
        Some((origin + (rot * (x.pos * self.rig.def.escala)).as_dvec3(), rot * (x.rot * Vec3::Y), rot * (x.rot * Vec3::X)))
    }

    /// How far a body point is (world, m) from where it would be with the body rigid under its
    /// eyes: what the hips' sag, a stride, a landing and the chest's turn have moved it by. What
    /// rests on that point (a launcher on a shoulder) goes with it.
    pub fn carried(&self, name: &str, s: &Stance) -> Option<Vec3> {
        let (bone, rel) = self.rig.points.get(name)?;
        let scale = self.rig.def.escala;
        let now = self.pose.model[*bone].then(*rel).pos;
        let rest = self.rig.skeleton.bind[*bone].then(*rel).pos + Vec3::Y * (s.eye_h as f32 / scale - self.rig.def.ojos[1]);
        Some(self.frame(s).1 * ((now - rest) * scale))
    }

    /// Where a wrist is now (world), the turn of its forearm and of its hand (model axes at
    /// rest to the world's): for whoever draws how a hand is turned.
    pub fn wrist_frames(&self, side: usize, s: &Stance) -> (DVec3, Quat, Quat) {
        let (arm, sk) = (self.rig.arms[side], &self.rig.skeleton);
        let (origin, rot) = self.frame(s);
        let at = origin + (rot * (self.pose.model[arm.end].pos * self.rig.def.escala)).as_dvec3();
        let from_rest = |b: usize| (rot * self.pose.model[b].rot * sk.bind[b].rot.inverse()).normalize();
        (at, from_rest(arm.lower), from_rest(arm.end))
    }

    /// This frame's pose. `grips`: where each hand is asked to be (left, right), if anywhere.
    pub fn update(&mut self, dt: f64, s: &Stance, set: &Structures, bodies: &BodyRegistry, grips: [Option<Grip>; 2]) {
        self.clock += dt;
        let scale = self.rig.def.escala;
        let (origin, rot) = self.frame(s);
        let inv = rot.inverse();
        let to_model = |p: DVec3| inv * ((p - origin).as_vec3() / scale);
        let dir_model = |d: DVec3| inv * d.as_vec3();

        // ---- the ground the feet are on: the ship's own frame aboard one, else the world's
        let ride = s.ride.and_then(|id| set.get(id));
        if self.ground != ride.map(|r| r.id) {
            let old = self.ground.and_then(|id| set.get(id));
            let (pw, dw) = (|p: DVec3| old.map_or(p, |o| o.to_world(p.as_vec3())), |d: DVec3| old.map_or(d, |o| (o.rot * d.as_vec3()).as_dvec3()));
            self.gait.rebase(|p| ride.map_or(pw(p), |r| r.to_local(pw(p)).as_dvec3()), |d| ride.map_or(dw(d), |r| (r.rot.inverse() * dw(d).as_vec3()).as_dvec3()));
            self.ground = ride.map(|r| r.id);
        }
        let to_ground = |p: DVec3| ride.map_or(p, |r| r.to_local(p).as_dvec3());
        let dir_ground = |d: DVec3| ride.map_or(d, |r| (r.rot.inverse() * d.as_vec3()).as_dvec3());
        let to_world = |p: DVec3| ride.map_or(p, |r| r.to_world(p.as_vec3()));
        let dir_world = |d: DVec3| ride.map_or(d, |r| (r.rot * d.as_vec3()).as_dvec3());
        let body = bodies.get(bodies.dominant(origin));
        // the ground at a point given at about the feet's level: the deck or the terrain within
        // a step of it
        const NEAR: f64 = 0.6;
        let mut probe = |p: DVec3| -> Option<(DVec3, DVec3)> {
            let w = to_world(p);
            let deck = set.raycast_solid(w + s.up * NEAR, -s.up, 2.0 * NEAR, 0.06).map(|(_, t, n)| (w + s.up * (NEAR - t), n));
            let dir = body.up(w);
            let floor = body.center + dir * (body.radius + body.height(dir));
            let gap = (floor - w).dot(s.up);
            let soil = (gap.abs() <= NEAR).then_some((w + s.up * gap, dir));
            // (whichever is higher: a deck over the ground, a rock through a grating)
            let best = match (deck, soil) {
                (Some(d), Some(g)) => Some(if (d.0 - g.0).dot(s.up) >= 0.0 { d } else { g }),
                (d, g) => d.or(g),
            }?;
            Some((to_ground(best.0), dir_ground(best.1)))
        };
        let walker = Walker { at: to_ground(origin), up: dir_ground(s.up), ahead: dir_ground(s.ahead), vel: dir_ground(s.vel), grounded: s.grounded && !s.seated, g: s.g };
        if s.seated {
            // (its feet are the seat's business; back on them when it stands)
            self.gait.stand(&self.rig.def.marcha, &walker, &mut probe);
        } else {
            self.gait.update(&self.rig.def.marcha, &walker, dt, &mut probe);
        }

        // ---- the pose, from rest
        let (sk, rig) = (&self.rig.skeleton, &self.rig);
        let pose = &mut self.pose;
        pose.reset(sk);
        // the eyes are where the camera is: what is between them and the feet bends to fit
        let drop = ((self.stand_eye - s.eye_h as f32) / scale).max(-0.05);
        pose.shift(sk, rig.pelvis, Vec3::new(0.0, -drop, 0.0));
        let walk = self.gait.walk as f32;
        let swing = (std::f64::consts::TAU * self.gait.phase) as f32;
        if !s.seated {
            // the hips go round with the legs, the chest stays facing ahead
            let twist = 0.1 * walk * swing.cos();
            pose.turn(sk, rig.pelvis, Quat::from_rotation_y(twist));
            if let Some(&b) = rig.spine.first() {
                pose.turn(sk, b, Quat::from_rotation_y(-twist));
            }
        }

        // ---- the legs: where each ankle must be and how its foot is turned
        let mut targets = [(Vec3::ZERO, Quat::IDENTITY, Quat::IDENTITY, Vec3::Z, 0.0f32); 2];
        for i in 0..2 {
            let shape = self.feet[i];
            let f = self.gait.feet[i];
            let sole = to_model(to_world(f.at));
            let normal = dir_model(dir_world(f.normal)).normalize_or(Vec3::Y);
            let ahead = dir_model(dir_world(f.dir));
            let ahead = (ahead - normal * ahead.dot(normal)).normalize_or(Vec3::Z);
            let turn = frame_turn(Vec3::Y, Vec3::Z, normal, ahead);
            // the toes down as it leaves the ground and hangs, up before it comes down; planted,
            // the heel comes up as the body leaves it behind
            let (pitch, heel) = if f.planted {
                let stride = self.gait.stride as f32;
                (0.0, if stride > 0.05 { ((f.lag as f32 - 0.45 * stride) / (0.6 * stride)).clamp(0.0, 1.0) * HEEL_OFF } else { 0.0 })
            } else if self.gait.airborne {
                (0.35, 0.0)
            } else {
                let t = f.swing as f32;
                let bump = |a: f32, b: f32| if t > a && t < b { (std::f32::consts::PI * (t - a) / (b - a)).sin().powi(2) } else { 0.0 };
                (TOE_OFF * bump(-0.25, 0.5) - TOE_UP * bump(0.55, 1.05), 0.0)
            };
            let ankle = sole + turn * (shape.ball + Quat::from_rotation_x(heel) * (shape.ankle - shape.ball));
            let knee_way = turn * Vec3::new(if i == 0 { 0.12 } else { -0.12 }, 0.0, 1.0) + Vec3::Z * 0.6;
            targets[i] = (ankle, turn * Quat::from_rotation_x(pitch + heel) * sk.bind[rig.legs[i].end].rot, turn, knee_way, heel);
        }
        // a leg is only so long: with a foot far ahead or behind, the hips come down to it (the
        // head with them: the body bobs under the eyes as it strides)
        let mut need = 0.0f32;
        if !s.seated && !self.gait.airborne {
            // (the foot in the air too: the hips are down for it by the time it lands)
            for i in 0..2 {
                // (as if it were down already, under where it is carried)
                let down = Vec3::Y * (self.gait.feet[i].lift as f32 / scale);
                let d = targets[i].0 - down - pose.model[rig.legs[i].upper].pos;
                let reach = rig.legs[i].reach(sk) * 0.992;
                let flat = (d.x * d.x + d.z * d.z).sqrt();
                need = need.max((-d.y - (reach * reach - flat * flat).max(0.0).sqrt()).clamp(0.0, 0.3));
            }
        }
        let sag = self.sag.step(need, 0.045, dt as f32).max(0.0);
        if sag > 1e-4 {
            pose.shift(sk, rig.pelvis, Vec3::new(0.0, -sag, 0.0));
        }
        for i in 0..2 {
            let leg = rig.legs[i];
            if s.seated {
                // sitting: thighs level ahead and a little apart, shins down to the floor that
                // is there under the feet (a leg too short for it hangs; too long, its foot
                // goes further forward)
                let hip = pose.model[leg.upper].pos;
                let (thigh, shin) = leg.lengths(sk);
                let knee = hip + Vec3::new(if i == 0 { 0.02 } else { -0.02 }, -0.035, thigh * 0.985);
                let under = knee + Vec3::new(0.0, -shin, SHIN_AHEAD);
                let floor = probe(to_ground(origin + (rot * (under * scale)).as_dvec3())).map(|(g, _)| to_model(to_world(g)).y);
                let fall = floor.map_or(shin * 0.98, |y| (knee.y - y - self.feet[i].ankle.y).clamp(shin * 0.55, shin * 0.985));
                let ankle = knee + Vec3::new(0.0, -fall, (shin * shin - fall * fall).max(0.0).sqrt());
                leg.solve(sk, pose, ankle, Vec3::new(0.0, 0.4, 1.0), Some(sk.bind[leg.end].rot));
                continue;
            }
            let (ankle, foot, turn, knee_way, heel) = targets[i];
            leg.solve(sk, pose, ankle, knee_way, Some(foot));
            // (on its toes, the toes stay flat on the ground)
            if let (Some(toe), true) = (rig.toes[i], heel > 0.0) {
                pose.turn(sk, toe, turn * Quat::from_rotation_x(-heel) * turn.inverse());
            }
        }

        // ---- the arms
        self.aloft.step(if s.grounded || s.seated { 0.0 } else { 1.0 }, 0.12, dt as f32);
        let aloft = self.aloft.x.clamp(0.0, 1.0);
        // where each hand is asked to be (its bone's head and turn, model space)
        let wants: [Option<(Vec3, Quat)>; 2] = std::array::from_fn(|i| {
            let (arm, palm) = (rig.arms[i], rig.palms[i]);
            let asked = grips[i].map(|g| (to_model(g.at), dir_model(g.normal.as_dvec3()).normalize_or(Vec3::NEG_Y), dir_model(g.across.as_dvec3())));
            // seated with nothing to hold, the hand rests where the rig says (on its thigh)
            let rest = rig.seated_hands[i].filter(|_| s.seated).map(|(bone, rel)| {
                let x = pose.model[bone].then(rel);
                (x.pos, x.rot * Vec3::Y, x.rot * Vec3::X)
            });
            asked.or(rest).map(|(at, normal, across)| {
                let q = palm_turn(&palm, normal, across);
                (at - q * (palm.at - sk.bind[arm.end].pos), (q * sk.bind[arm.end].rot).normalize())
            })
        });
        // a hand asked further than its arm goes: the chest turns to bring that shoulder round
        // (the waist of a suit turns), and the shoulder itself comes forward
        let far: [f32; 2] = std::array::from_fn(|i| {
            wants[i].map_or(0.0, |(at, _)| {
                let d = (at - pose.model[rig.arms[i].upper].pos).length() / rig.arms[i].reach(sk).max(1e-3);
                ((d - 0.78) / 0.3).clamp(0.0, 1.0)
            })
        });
        let twist = self.twist.step((far[1] - far[0]) * TWIST, 0.08, dt as f32);
        if twist.abs() > 1e-4 && !rig.spine.is_empty() {
            let part = Quat::from_rotation_y(twist / rig.spine.len() as f32);
            for &b in &rig.spine {
                pose.turn(sk, b, part);
            }
        }
        for i in 0..2 {
            let arm = rig.arms[i];
            let side = if i == 0 { 1.0 } else { -1.0 };
            let palm = rig.palms[i];
            // the shoulder round toward what is asked of the hand, if it is far
            let shrug = self.shrug[i].step(far[i], 0.08, dt as f32).clamp(0.0, 1.0);
            if let (Some(clav), Some((at, _)), true) = (rig.clavicles[i], wants[i], shrug > 1e-3) {
                let (root, tip) = (pose.model[clav].pos, pose.model[arm.upper].pos);
                let to = lunar_core::anim::ik::swing(tip - root, at - root);
                let (axis, angle) = to.to_axis_angle();
                pose.turn(sk, clav, Quat::from_axis_angle(axis, angle.min(SHRUG) * shrug));
            }
            // free: down by the side, swinging against the legs, a little out off the ground
            let phase = swing + if i == 0 { std::f32::consts::PI } else { 0.0 };
            let breath = 0.012 * ((self.clock * 0.9) as f32 + i as f32).sin();
            let fore = ARM_SWING * walk * phase.cos() + breath + 0.18 * aloft;
            let out = 0.06 + 0.3 * aloft;
            let (shoulder, elbow) = (pose.model[arm.upper].pos, pose.model[arm.lower].pos);
            let hang = Quat::from_rotation_z(side * out) * Quat::from_rotation_x(-fore) * (elbow - shoulder);
            pose.turn(sk, arm.upper, lunar_core::anim::ik::swing(elbow - shoulder, hang));
            let bend = ELBOW[0] + (ELBOW[1] - ELBOW[0]) * walk + 0.25 * aloft + if s.seated { 0.9 } else { 0.0 };
            pose.turn(sk, arm.lower, Quat::from_rotation_x(-bend));
            let free = (pose.model[arm.end].pos, pose.model[arm.end].rot);
            // asked somewhere: the hand goes there, elbow down and out
            let want = wants[i];
            let k = self.reach[i].step(if want.is_some() { 1.0 } else { 0.0 }, 0.07, dt as f32).clamp(0.0, 1.0);
            if let Some(w) = want {
                self.last_grip[i] = Some(w);
            }
            self.short[i] = Vec3::ZERO;
            if let (Some((at, turn)), true) = (self.last_grip[i], k > 0.002) {
                let (at, turn) = (free.0.lerp(at, k), free.1.slerp(turn, k));
                let pole = self.poles[i].unwrap_or(ELBOW_POLE * Vec3::new(side, 1.0, 1.0));
                // (solved from the same bend whatever the arm was doing: `HINGE`)
                let bent = pose.model[arm.upper].rot * sk.bind[arm.upper].rot.inverse() * Quat::from_rotation_x(-HINGE) * sk.bind[arm.lower].rot;
                pose.aim(sk, arm.lower, bent);
                // a wrist only goes so far: the elbow round to ease it (unless whoever asks
                // says where it goes), and the hand held to what a wrist does
                let (at, turn, pole, wrist) = ease(rig, pose, i, at, turn, pole, self.poles[i].is_none(), &mut self.swivel[i], dt as f32);
                let short = arm.solve(sk, pose, at, pole, Some(turn));
                if short > 1e-4 && want.is_some() {
                    self.short[i] = rot * ((at - pose.model[arm.end].pos) * scale);
                }
                self.wrists[i] = WristPose { asked: want.is_some(), ..wrist };
            } else {
                self.last_grip[i] = None;
                self.swivel[i] = Spring::default();
                let reach = (pose.model[arm.end].pos - pose.model[arm.upper].pos).length() / arm.reach(sk).max(1e-3);
                self.wrists[i] = WristPose { reach, ..WristPose::default() };
            }
            // the fingers close on what the hand holds
            let hand_turn = pose.model[arm.end].rot * sk.bind[arm.end].rot.inverse();
            let pose_of = grips[i].map_or([0.0; 5], |g| g.fingers);
            for (f, finger) in rig.fingers[i].iter().enumerate().take(5) {
                let c = self.fingers[i][f].step(pose_of[f], 0.05, dt as f32);
                let axis = (hand_turn * finger.axis).normalize_or(Vec3::X);
                for (j, &bone) in finger.bones.iter().enumerate().take(3) {
                    let deg = if c >= 0.0 { c * finger.travel.cerrar[j] } else { c * finger.travel.abrir[j] };
                    pose.turn(sk, bone, Quat::from_axis_angle(axis, deg.to_radians()));
                }
            }
            // where the hand ended up, for what it carries
            let q = pose.model[arm.end].rot * sk.bind[arm.end].rot.inverse();
            let at = pose.model[arm.end].pos + q * (palm.at - sk.bind[arm.end].pos);
            self.hands[i] = Hand { at: origin + (rot * (at * scale)).as_dvec3(), rot: rot * q * palm_frame(&palm) };
        }
    }

    /// The body into `out`, as posed by the last `update`.
    pub fn show(&self, out: &mut BodyScene, s: &Stance) {
        let (origin, rot) = self.frame(s);
        let first = out.bones.len() as u32;
        self.pose.palette(&self.rig.skeleton, &mut out.bones);
        out.bodies.push(BodyDraw { mesh: if s.own_eyes { self.seen } else { self.whole }, shadow: self.whole, pos: origin, rot, scale: self.rig.def.escala, inside: s.inside, first });
    }
}

impl Body {
    /// Where a bone's head is now (world).
    pub fn joint(&self, bone: usize, s: &Stance) -> DVec3 {
        let (origin, rot) = self.frame(s);
        origin + (rot * (self.pose.model[bone].pos * self.rig.def.escala)).as_dvec3()
    }

    /// Where the model's origin (between its feet) is in the world, and its turn: x to its
    /// left, y up, z the way it faces. Its eyes are over that by as much as they are high, and
    /// as far ahead of it as the model has them.
    pub fn frame(&self, s: &Stance) -> (DVec3, Quat) {
        let left = s.up.cross(s.ahead).normalize_or(s.up.any_orthonormal_vector());
        let ahead = left.cross(s.up);
        let eyes = self.rig.def.ojos;
        let off = (left * f64::from(eyes[0]) + ahead * f64::from(eyes[2])) * f64::from(self.rig.def.escala);
        (s.eye - s.up * s.eye_h - off, Quat::from_mat3(&glam::Mat3::from_cols(left.as_vec3(), s.up.as_vec3(), ahead.as_vec3())).normalize())
    }
}

/// A palm's own frame at rest (model space): x from index to little finger, y the way it faces.
fn palm_frame(p: &Palm) -> Quat {
    frame_turn(Vec3::Y, Vec3::X, p.normal, p.across)
}

/// A turn `d` (a hand's from its forearm's, both as turns from rest) as a wrist has it: how far
/// it bends toward the palm, leans toward the little finger and turns about the forearm (rad).
/// The turn about the forearm comes first, then the bend: so a wrist works.
pub fn wrist_angles(ax: &WristAxes, d: Quat) -> (f32, f32, f32) {
    let d = if d.w < 0.0 { -d } else { d };
    let along = Vec3::new(d.x, d.y, d.z).dot(ax.along);
    let twist = Quat::from_xyzw(ax.along.x * along, ax.along.y * along, ax.along.z * along, d.w);
    let twist = if twist.length_squared() > 1e-10 { twist.normalize() } else { Quat::IDENTITY };
    let swing = (d * twist.inverse()).to_scaled_axis();
    (swing.dot(twist * ax.flex), swing.dot(twist * ax.lean), 2.0 * along.atan2(d.w))
}

/// The turn of a hand from its forearm that bends, leans and turns so (`wrist_angles` back).
pub fn wrist_turn(ax: &WristAxes, flex: f32, lean: f32, turn: f32) -> Quat {
    let twist = Quat::from_axis_angle(ax.along, turn);
    (Quat::from_scaled_axis(twist * ax.flex * flex + twist * ax.lean * lean) * twist).normalize()
}

/// What a wrist does of what is asked of it (rad): its angles held within `range`, and whether
/// they had to be. `own`: 1 for a left wrist, -1 for a right one (they turn opposite ways).
pub fn wrist_held(range: &WristRange, own: f32, flex: f32, lean: f32, turn: f32) -> (f32, f32, f32, bool) {
    let palm_back = (turn * own).clamp(-range.supinacion.to_radians(), range.pronacion.to_radians());
    let most = (if flex >= 0.0 { range.flexion } else { range.extension }.to_radians().max(1e-3), if lean >= 0.0 { range.cubital } else { range.radial }.to_radians().max(1e-3));
    // (bent and leant at once it goes less far each way: an oval, not a box)
    let k = ((flex / most.0).powi(2) + (lean / most.1).powi(2)).sqrt().max(1.0);
    let held = k > 1.0 + 1e-4 || (palm_back - turn * own).abs() > 1e-4;
    (flex / k, lean / k, palm_back * own, held)
}

/// How far past what a wrist does a turn is (rad, squared and added: 0 within it).
fn strain(range: &WristRange, own: f32, ax: &WristAxes, d: Quat) -> f32 {
    let (flex, lean, turn) = wrist_angles(ax, d);
    let (f, l, t, _) = wrist_held(range, own, flex, lean, turn);
    (flex - f).powi(2) + (lean - l).powi(2) + (turn - t).powi(2)
}

/// The elbow goes no further round than this to ease a wrist (rad), tried every so far (rad);
/// going round costs this much (per rad², against the wrist's strain in rad²).
const SWIVEL_MOST: f32 = 1.3;
const SWIVEL_STEP: f32 = 0.2;
const SWIVEL_COST: f32 = 0.1;

/// Arm `i` of a body about to be asked to put its hand's bone at `at` turned `turn` (model
/// space), its elbow toward `pole`: what to ask of it instead so that its wrist does only what
/// a wrist does. With a `free` elbow, the elbow goes round the line from shoulder to hand to
/// where the wrist is strained least (never across the chest nor up over the shoulder for it);
/// then, if the hand is still turned past what its wrist does, it is turned back to that, its
/// palm staying where it was asked. The arm must be posed as it will be solved from.
#[allow(clippy::too_many_arguments)]
fn ease(rig: &Rig, pose: &Pose, i: usize, at: Vec3, turn: Quat, pole: Vec3, free: bool, swivel: &mut Spring, dt: f32) -> (Vec3, Quat, Vec3, WristPose) {
    let (sk, arm, ax, range) = (&rig.skeleton, rig.arms[i], &rig.wrists[i], &rig.def.muneca);
    let own = if i == 0 { 1.0 } else { -1.0 };
    let (a, b, c) = (pose.model[arm.upper].pos, pose.model[arm.lower].pos, pose.model[arm.end].pos);
    let (l1, l2) = ((b - a).length(), (c - b).length());
    let plane = (b - a).cross(c - b);
    let fore_now = pose.model[arm.lower].rot * sk.bind[arm.lower].rot.inverse();
    // the hand's turn from rest, and where its palm is asked
    let mut hand = (turn * sk.bind[arm.end].rot.inverse()).normalize();
    let grip = rig.palms[i].at - sk.bind[arm.end].pos;
    let palm = at + hand * grip;
    // the forearm's turn from rest with the wrist at `w` and the elbow toward `pole`: what the
    // solver will make of it (`anim::ik`)
    let fore = |w: Vec3, pole: Vec3| -> Option<Quat> {
        let to = w - a;
        let far = to.length();
        if far < 1e-5 {
            return None;
        }
        let d = far.clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * 0.999);
        let dir = to / far;
        let side = (pole - dir * pole.dot(dir)).normalize_or(dir.any_orthonormal_vector());
        let along = (l1 * l1 + d * d - l2 * l2) / (2.0 * d);
        let mid = a + dir * along + side * (l1 * l1 - along * along).max(0.0).sqrt();
        let normal = side.cross(dir);
        let was = if plane.length_squared() > 1e-10 { plane } else { normal };
        Some((frame_turn(c - b, was, a + dir * d - mid, normal) * fore_now).normalize())
    };
    let mut at = at;
    let mut pole = pole;
    // the elbow round the line from shoulder to hand, to where the wrist is strained least
    let dir = (at - a).normalize_or_zero();
    let mut round = 0.0;
    if free && dir != Vec3::ZERO {
        let inward = Vec3::new(-own, 0.0, 0.0);
        let cost = |by: f32| -> f32 {
            let p = Quat::from_axis_angle(dir, by) * pole;
            let side = (p - dir * p.dot(dir)).normalize_or_zero();
            let wrist = fore(at, p).map_or(0.0, |f| strain(range, own, ax, f.inverse() * hand));
            wrist + SWIVEL_COST * by * by + 2.0 * (side.dot(inward) - 0.25).max(0.0).powi(2) + (side.y - 0.45).max(0.0).powi(2)
        };
        let here = cost(0.0);
        let mut best = (0.0, here);
        // (a wrist at ease: the elbow where it always goes)
        if here > 1e-5 {
            let n = (SWIVEL_MOST / SWIVEL_STEP) as i32;
            for k in (-n..=n).filter(|k| *k != 0) {
                let by = k as f32 * SWIVEL_STEP;
                let c = cost(by);
                if c < best.1 {
                    best = (by, c);
                }
            }
            // (between the steps tried: where the three round the best say the least is)
            let (lo, hi) = (cost(best.0 - SWIVEL_STEP), cost(best.0 + SWIVEL_STEP));
            let bend = lo + hi - 2.0 * best.1;
            if bend > 1e-6 {
                best.0 += (0.5 * (lo - hi) / bend).clamp(-0.5, 0.5) * SWIVEL_STEP;
            }
        }
        round = swivel.step(best.0, 0.09, dt);
        pole = Quat::from_axis_angle(dir, round) * pole;
    } else {
        *swivel = Spring::default();
    }
    // what is still past what the wrist does, the hand gives: turned back to it about its palm
    let mut out = WristPose { swivel: round.to_degrees(), reach: (at - a).length() / (l1 + l2).max(1e-3), ..WristPose::default() };
    for _ in 0..3 {
        let Some(f) = fore(at, pole) else { break };
        let (flex, lean, turned) = wrist_angles(ax, f.inverse() * hand);
        let (flex, lean, turned, held) = wrist_held(range, own, flex, lean, turned);
        (out.flex, out.lean, out.turn) = (flex.to_degrees(), lean.to_degrees(), (turned * own).to_degrees());
        if !held {
            break;
        }
        out.held = true;
        hand = (f * wrist_turn(ax, flex, lean, turned)).normalize();
        at = palm - hand * grip;
    }
    (at, (hand * sk.bind[arm.end].rot).normalize(), pole, out)
}

/// (for tests: the pose as it is)
#[cfg(test)]
impl Body {
    /// A bone's head in the world.
    pub fn bone_at(&self, bone: usize, s: &Stance) -> DVec3 {
        let (origin, rot) = self.frame(s);
        origin + (rot * (self.pose.model[bone].pos * self.rig.def.escala)).as_dvec3()
    }

    pub fn xf(&self, bone: usize) -> lunar_core::anim::Xf {
        self.pose.model[bone]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seated_it_measures_what_every_seat_is_held_to() {
        // (`lunar_ship::diag::seat_fit` checks every seat's eyes against a seated crew member:
        // that crew member is this body)
        use lunar_ship::diag;
        let Some((body, ..)) = world() else { return };
        let (sk, rig) = (&body.rig.skeleton, &body.rig);
        let scale = rig.def.escala;
        let (hip, eyes) = (sk.bind[rig.legs[0].upper].pos, Vec3::from(rig.def.ojos));
        let over = (eyes.y - hip.y) * scale + rig.def.sentado.asiento;
        assert!((over - diag::SEATED_EYE).abs() < 0.01, "sentado, los ojos a {over:.3} m del asiento");
        assert!(((eyes.z - hip.z) * scale - diag::EYE_AHEAD).abs() < 0.01, "los ojos {:.3} m por delante de la cadera", (eyes.z - hip.z) * scale);
        let thigh = rig.legs[0].lengths(sk).0 * scale;
        assert!((thigh - diag::THIGH).abs() < 0.015, "muslo de {thigh:.3} m");
    }
    use crate::rig::{Rig, RigDef, Rigged};
    use lunar_core::{
        body::{Body as Planet, BodyDef},
        defs,
    };
    use std::sync::Arc;

    /// The astronaut of the game standing on the Moon, an empty set of structures, the Moon, and
    /// the stance it stands with (none if its model is not there).
    fn world() -> Option<(Body, Structures, BodyRegistry, Stance)> {
        let root = crate::root();
        let def: RigDef = defs::load(&root.join("assets/defs/rigs/astronauta.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let model = Rigged::load(&root.join("assets/models").join(format!("{}.glb", def.modelo))).ok()?;
        let body = Body::new(Rig::new(def, model.skeleton).unwrap(), None, None);
        let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        let bodies = BodyRegistry::new(vec![Planet::from_def("luna", &moon).unwrap()]);
        let lib = lunar_core::structure::Library::load(&root.join("assets/defs/structures")).unwrap();
        let set = Structures::new(Arc::new(lib));
        let b = bodies.get(0);
        let up = DVec3::new(0.3, 0.9, 0.2).normalize();
        let ahead = up.any_orthonormal_vector();
        let eye_h = f64::from(body.eye_height());
        let stance = Stance { eye: b.above_ground(up, eye_h), up, ahead, eye_h, vel: DVec3::ZERO, grounded: true, g: 1.62, ride: None, seated: false, inside: false, own_eyes: true };
        Some((body, set, bodies, stance))
    }

    /// Stands `s` on the ground where it is (its eyes over the ground under them).
    fn on_ground(s: &mut Stance, bodies: &BodyRegistry) {
        let b = bodies.get(0);
        let dir = b.up(s.eye);
        s.up = dir;
        s.ahead = (s.ahead - dir * s.ahead.dot(dir)).normalize();
        s.eye = b.above_ground(dir, s.eye_h);
    }

    #[test]
    fn it_stands_with_its_feet_on_the_ground_and_its_eyes_where_the_camera_is() {
        let Some((mut body, set, bodies, s)) = world() else { return };
        for _ in 0..30 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        let rig = &body.rig;
        let moon = bodies.get(0);
        for i in 0..2 {
            let ankle = body.bone_at(rig.legs[i].end, &s);
            let h = moon.altitude(ankle);
            assert!(h > 0.08 && h < 0.2, "el tobillo {i} a {h:.3} m del suelo");
            let knee = body.bone_at(rig.legs[i].lower, &s);
            assert!(moon.altitude(knee) > 0.4, "la rodilla {i}");
        }
        // the eyes: where the stance has them
        let chest = rig.spine[rig.spine.len() - 1];
        let eyes = body.xf(chest).point(rig.skeleton.bind[chest].inverse().point(Vec3::from(rig.def.ojos)));
        let (origin, rot) = body.frame(&s);
        let head = origin + (rot * (eyes * rig.def.escala)).as_dvec3();
        assert!(head.distance(s.eye) < 0.03, "los ojos del cuerpo a {:.3} m de la cámara", head.distance(s.eye));
        // every bone a finite matrix
        let mut scene = BodyScene::default();
        body.show(&mut scene, &s);
        assert_eq!(scene.bones.len(), rig.skeleton.len());
        assert!(scene.bones.iter().all(|m| m.iter().all(|x| x.is_finite())));
    }

    #[test]
    fn walking_its_feet_do_not_slide_and_its_knees_go_ahead() {
        let Some((mut body, set, bodies, mut s)) = world() else { return };
        let dt = 1.0 / 60.0;
        let speed = 1.8;
        let mut held: [Option<DVec3>; 2] = [None, None];
        let (mut slid, mut steps, mut behind, mut deep) = (0.0f64, 0, 0, 0.0f64);
        for k in 0..600 {
            s.eye += s.ahead * speed * dt;
            on_ground(&mut s, &bodies);
            s.vel = s.ahead * speed;
            body.update(dt, &s, &set, &bodies, [None, None]);
            steps += body.gait.landed.count_ones();
            for i in 0..2 {
                let rig = &body.rig;
                let toe = rig.toes[i].unwrap();
                // the ball of the foot stays put while the foot is down (the heel may come up)
                let ball = body.bone_at(toe, &s);
                if body.gait.feet[i].planted && k > 60 {
                    if let Some(was) = held[i] {
                        slid = slid.max(was.distance(ball));
                    }
                    held[i] = Some(ball);
                    deep = deep.max(-bodies.get(0).altitude(ball));
                } else {
                    held[i] = None;
                }
                // the knee ahead of the line from hip to ankle
                let (hip, knee, ankle) = (body.bone_at(rig.legs[i].upper, &s), body.bone_at(rig.legs[i].lower, &s), body.bone_at(rig.legs[i].end, &s));
                let line = (ankle - hip).normalize();
                let out = (knee - hip) - line * (knee - hip).dot(line);
                behind += usize::from(out.dot(s.ahead) < -0.01);
            }
        }
        eprintln!("{steps} pasos en 10 s; el pie plantado se mueve {:.1} mm entre fotogramas; {behind} rodillas atrás", slid * 1000.0);
        assert!(steps > 15 && steps < 30, "{steps} pasos");
        // (as it comes down it settles a centimetre: the heel strikes, then the sole)
        assert!(slid < 0.02, "un pie plantado resbala {slid:.3} m entre fotogramas");
        assert!(deep < 0.03, "un pie se hunde {deep:.3} m en el suelo");
        assert_eq!(behind, 0, "rodillas dobladas hacia atrás");
    }

    #[test]
    fn its_hands_go_where_they_are_asked_and_say_when_they_cannot() {
        let Some((mut body, set, bodies, s)) = world() else { return };
        let (_, rot) = body.frame(&s);
        let left = (rot * Vec3::X).as_dvec3();
        // the right hand on a grip ahead of the right shoulder, palm to the left; the left one on
        // a foregrip further out
        let right_at = s.eye + s.ahead * 0.2 - left * 0.26 - s.up * 0.32;
        let left_at = s.eye + s.ahead * 0.34 - left * 0.06 - s.up * 0.3;
        let grip = |at: DVec3, normal: Vec3| Grip { at, normal: rot * normal, across: rot * Vec3::new(0.0, -1.0, 0.2).normalize(), fingers: [0.5, 0.8, 0.9, 0.9, 0.9] };
        let grips = [Some(grip(left_at, Vec3::NEG_X)), Some(grip(right_at, Vec3::X))];
        for _ in 0..90 {
            body.update(1.0 / 60.0, &s, &set, &bodies, grips);
        }
        for (i, g) in grips.iter().enumerate() {
            let g = g.unwrap();
            let h = body.hands[i];
            assert!(h.at.distance(g.at) < 0.01, "la mano {i} a {:.3} m de su agarre", h.at.distance(g.at));
            assert!((h.rot * Vec3::Y).dot(g.normal) > 0.99 && (h.rot * Vec3::X).dot(g.across) > 0.99, "la mano {i} no está girada a su agarre");
            assert!(body.short[i] == Vec3::ZERO);
            // elbows down: below the line from shoulder to wrist
            let arm = body.rig.arms[i];
            let (sh, el, wr) = (body.bone_at(arm.upper, &s), body.bone_at(arm.lower, &s), body.bone_at(arm.end, &s));
            let mid = (sh + wr) * 0.5;
            assert!((el - mid).dot(s.up) < 0.0, "el codo {i} hacia arriba");
        }
        // a grip three metres away: the arm stretches at it and says how far short it is
        let far = [None, Some(grip(s.eye + s.ahead * 3.0, Vec3::X))];
        for _ in 0..90 {
            body.update(1.0 / 60.0, &s, &set, &bodies, far);
        }
        assert!(body.short[1].length() > 2.0 && (rot.inverse() * body.short[1]).z > 2.0, "{:?}", body.short[1]);
        // let go: the arm comes down to the side again
        for _ in 0..120 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        let wrist = body.bone_at(body.rig.arms[1].end, &s);
        assert!((wrist - s.eye).dot(s.up) < -0.6, "suelta, la mano no baja");
    }

    #[test]
    fn a_wrists_angles_are_told_and_made_again_and_held_to_what_a_wrist_does() {
        let Some((body, ..)) = world() else { return };
        let range = body.rig.def.muneca;
        for side in 0..2 {
            let ax = &body.rig.wrists[side];
            // square to each other: the forearm's line, what the hand bends about, what it leans about
            assert!(ax.along.dot(ax.flex).abs() < 1e-3 && ax.along.dot(ax.lean).abs() < 1e-3 && ax.flex.dot(ax.lean).abs() < 0.08, "{ax:?}");
            for (flex, lean, turn) in [(0.0f32, 0.0f32, 0.0f32), (0.6, 0.0, 0.0), (-0.5, 0.2, 0.0), (0.0, 0.0, 1.2), (0.4, -0.2, -1.0), (0.3, 0.3, 2.4)] {
                let d = wrist_turn(ax, flex, lean, turn);
                let (f, l, t) = wrist_angles(ax, d);
                assert!((f - flex).abs() < 0.03 && (l - lean).abs() < 0.03 && (t - turn).abs() < 0.01, "{:?} -> {:?}", (flex, lean, turn), (f, l, t));
            }
            // bent toward the palm, the fingers go the palm's way; leant, toward the little finger
            let palm = body.rig.palms[side];
            assert!((wrist_turn(ax, 0.5, 0.0, 0.0) * palm.fingers).dot(palm.normal) > 0.3);
            assert!((wrist_turn(ax, 0.0, 0.4, 0.0) * palm.fingers).dot(palm.across) > 0.25);
            // turned palm back (the arm hanging: the palm faces the thigh, then behind)
            let own = if side == 0 { 1.0 } else { -1.0 };
            assert!((wrist_turn(ax, 0.0, 0.0, 1.2 * own) * palm.normal).z < -0.5, "la palma {side} no gira hacia atrás");
            // within what a wrist does, as asked; past it, held to it
            let (f, l, t, held) = wrist_held(&range, own, 0.3, 0.1, 0.5 * own);
            assert!(!held && (f, l, t) == (0.3, 0.1, 0.5 * own));
            let (f, l, t, held) = wrist_held(&range, own, 2.0, 0.0, 3.0 * own);
            assert!(held && (f - range.flexion.to_radians()).abs() < 1e-4 && l == 0.0 && (t * own - range.pronacion.to_radians()).abs() < 1e-4);
            let (f, l, _, held) = wrist_held(&range, own, -1.0, -1.0, 0.0);
            assert!(held && f > -range.extension.to_radians() && l > -range.radial.to_radians() && f < 0.0 && l < 0.0, "doblada y ladeada a la vez llega menos lejos: {f} {l}");
        }
    }

    #[test]
    fn a_hand_asked_past_what_its_wrist_does_keeps_its_palm_there_and_gives() {
        let Some((mut body, set, bodies, s)) = world() else { return };
        let (_, rot) = body.frame(&s);
        let left = (rot * Vec3::X).as_dvec3();
        let range = body.rig.def.muneca;
        // every way a palm can face and be turned, ahead of the right shoulder: wherever it is
        // asked, the wrist does only what a wrist does and the palm is where it was asked
        let at = s.eye + s.ahead * 0.2 - left * 0.24 - s.up * 0.34;
        let (mut worst, mut held, mut round) = (0.0f64, 0, 0);
        let ways = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
        for normal in ways {
            for across in ways.into_iter().filter(|a| a.dot(normal) == 0.0) {
                let grip = Grip { at, normal: rot * normal, across: rot * across, fingers: [0.0; 5] };
                for _ in 0..90 {
                    body.update(1.0 / 60.0, &s, &set, &bodies, [None, Some(grip)]);
                }
                let w = body.wrists[1];
                assert!(w.asked && w.flex <= range.flexion + 0.5 && w.flex >= -range.extension - 0.5 && w.lean <= range.cubital + 0.5 && w.lean >= -range.radial - 0.5 && w.turn <= range.pronacion + 0.5 && w.turn >= -range.supinacion - 0.5, "palma a {normal:?}, de través {across:?}: {w:?}");
                // (what the body says of its wrist is what its bones do)
                let (arm, sk, ax) = (body.rig.arms[1], &body.rig.skeleton, &body.rig.wrists[1]);
                let from_rest = |b: usize| body.xf(b).rot * sk.bind[b].rot.inverse();
                let (f, l, t) = wrist_angles(ax, from_rest(arm.lower).inverse() * from_rest(arm.end));
                assert!((f.to_degrees() - w.flex).abs() < 2.0 && (l.to_degrees() - w.lean).abs() < 2.0 && (-t.to_degrees() - w.turn).abs() < 2.0, "dice {w:?}, los huesos {:.0} {:.0} {:.0}", f.to_degrees(), l.to_degrees(), -t.to_degrees());
                worst = worst.max(body.hands[1].at.distance(at));
                held += usize::from(w.held);
                round += usize::from(w.swivel.abs() > 5.0);
                // as asked, when a wrist can
                if !w.held {
                    assert!((body.hands[1].rot * Vec3::Y).dot(grip.normal) > 0.99 && (body.hands[1].rot * Vec3::X).dot(grip.across) > 0.99, "palma a {normal:?}: no está girada como se pidió y no dice que su muñeca esté en su tope");
                }
            }
        }
        eprintln!("24 maneras de pedir la mano: {held} con la muñeca en su tope, {round} con el codo girado; la palma a {:.1} mm de donde se pidió como mucho", worst * 1000.0);
        assert!(worst < 0.012, "la palma se va {worst:.3} m de donde se pidió");
        assert!(held > 4 && held < 24 && round > 2, "{held} en su tope, {round} con el codo girado");
        // let go: the wrist is nobody's again
        for _ in 0..120 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        assert!(!body.wrists[1].asked && !body.wrists[1].held);
    }

    #[test]
    fn a_hand_asked_the_same_is_turned_the_same_whatever_the_arm_was_doing() {
        // (the solver keeps the forearm turned as it was to the plane the arm bends in: from
        // one bend, the same; it used to depend on how bent the arm hung — standing, walking, sat)
        let Some((mut body, set, bodies, s)) = world() else { return };
        let (_, rot) = body.frame(&s);
        let left = (rot * Vec3::X).as_dvec3();
        let grip = Grip { at: s.eye + s.ahead * 0.3 - left * 0.22 - s.up * 0.3, normal: rot * Vec3::X, across: rot * Vec3::new(0.0, -1.0, 0.2).normalize(), fingers: [0.0; 5] };
        let mut turns = Vec::new();
        for (seated, aloft) in [(false, false), (true, false), (false, true)] {
            let st = Stance { seated, grounded: !aloft, ..s };
            for _ in 0..150 {
                body.update(1.0 / 60.0, &st, &set, &bodies, [None, Some(grip)]);
            }
            let arm = body.rig.arms[1];
            turns.push(body.xf(arm.lower).rot);
            assert!(body.hands[1].at.distance(grip.at) < 0.01);
        }
        for t in &turns[1..] {
            assert!(t.angle_between(turns[0]).to_degrees() < 3.0, "el antebrazo gira {:.0}° según lo que hiciera el brazo", t.angle_between(turns[0]).to_degrees());
        }
        // and where a forearm's hinge side will be is known before it is posed
        let (sk, arm) = (&body.rig.skeleton, body.rig.arms[1]);
        let (a, b, c) = (body.xf(arm.upper).pos, body.xf(arm.lower).pos, body.xf(arm.end).pos);
        let normal = (b - a).cross(c - b).normalize();
        let side = (body.xf(arm.lower).rot * sk.bind[arm.lower].rot.inverse()) * hinge(&body.rig, 1);
        assert!(side.dot(normal) > 0.995, "{side:?} / {normal:?}");
    }

    #[test]
    fn what_rests_on_the_body_goes_with_it_and_stays_put_when_it_only_crouches() {
        let Some((mut body, set, bodies, mut s)) = world() else { return };
        for _ in 0..60 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        let still = body.carried("hombro_der", &s).expect("the rig has a right shoulder to rest a launcher on");
        assert!(still.length() < 0.012, "de pie y quieto el hombro está a {:.3} m de su sitio", still.length());
        assert!(body.carried("nada", &s).is_none());
        // crouched: the whole body comes down with the eyes, the shoulder as far under them as before
        s.eye -= s.up * 0.5;
        s.eye_h -= 0.5;
        for _ in 0..60 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        assert!(body.carried("hombro_der", &s).unwrap().length() < 0.02);
        // the eyes 0.3 m higher over the feet than a body is tall (a step down not yet caught up
        // with, a ramp): the body is lower under them, and so is its shoulder
        s.eye += s.up * 0.5;
        s.eye_h += 0.5 + 0.3;
        s.eye += s.up * 0.3;
        for _ in 0..60 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        let low = body.carried("hombro_der", &s).unwrap();
        assert!(low.as_dvec3().dot(s.up) < -0.2, "{low:?}");
    }

    #[test]
    fn crouching_and_in_the_air_it_bends_and_hangs() {
        let Some((mut body, set, bodies, mut s)) = world() else { return };
        let stand = s.eye_h;
        // crouched: the eyes 0.6 m lower, the feet still on the ground, the knees bent
        s.eye -= s.up * 0.6;
        s.eye_h -= 0.6;
        for _ in 0..60 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        let rig = &body.rig;
        let (hip, knee, ankle) = (body.bone_at(rig.legs[0].upper, &s), body.bone_at(rig.legs[0].lower, &s), body.bone_at(rig.legs[0].end, &s));
        let bend = (hip - knee).normalize().dot((ankle - knee).normalize()).acos().to_degrees();
        assert!(bend < 110.0, "agachado, la rodilla a {bend:.0}°");
        assert!(bodies.get(0).altitude(ankle) < 0.2);
        // off the ground: the feet come up under it and nothing is planted
        s.eye += s.up * (3.0 + 0.6);
        s.eye_h = stand;
        s.grounded = false;
        for _ in 0..60 {
            body.update(1.0 / 60.0, &s, &set, &bodies, [None, None]);
        }
        assert!(body.gait.airborne && !body.gait.feet[0].planted);
        let ankle = body.bone_at(body.rig.legs[0].end, &s);
        let under = (s.eye - ankle).dot(s.up);
        assert!(under > 1.3 && under < s.eye_h - 0.15, "en el aire el tobillo a {under:.2} m bajo los ojos");
    }
}
