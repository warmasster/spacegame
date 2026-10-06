//! Bodies that move by a skeleton: nothing of graphics here, only what is posed and how.
//!
//! - `skeleton`: bones (a tree, at rest), a pose of them, the matrices a mesh is bent with;
//! - `ik`: a limb of two bones reaching for a point (an arm to a grip, a leg to the ground);
//! - `spring`: things that follow what they are after and settle (a tool swaying behind the
//!   look, a recoil, a landing taken on the knees);
//! - `clip`: movements written in data, key by key (a hatch opening, a hand going for a rocket);
//! - `gait`: walking made step by step: each foot set down on the ground where it will be
//!   needed, lifted and carried to the next place when it is left behind.
//!
//! What draws them takes a `BodyScene` each frame: which bodies, where, and every bone of each.
pub mod clip;
pub mod gait;
pub mod ik;
pub mod skeleton;
pub mod spring;

use glam::{DVec3, Quat};

pub use skeleton::{Pose, Skeleton, Xf};

/// A body to draw this frame.
#[derive(Clone, Copy, Debug)]
pub struct BodyDraw {
    /// The mesh it is to the eye and to the sun (as the renderer numbered them; none: not drawn
    /// there). They may differ: whoever looks out of a helmet does not see it, its shadow does.
    pub mesh: Option<u16>,
    pub shadow: Option<u16>,
    /// Where its model's origin is in the world, its turn, its size (1: the model's own).
    pub pos: DVec3,
    pub rot: Quat,
    pub scale: f32,
    /// Inside a hull: lit by the lamps in there only.
    pub inside: bool,
    /// Its first bone in `BodyScene::bones` (as many follow as its skeleton has).
    pub first: u32,
}

/// One frame's bodies, filled by whoever poses them (reused: clear and refill).
#[derive(Default)]
pub struct BodyScene {
    pub bodies: Vec<BodyDraw>,
    /// Every bone of every body: the rows of the 3×4 matrix that takes a vertex at rest to where
    /// the bone has it now, in the body's model space.
    pub bones: Vec<[f32; 12]>,
}

impl BodyScene {
    pub fn clear(&mut self) {
        self.bodies.clear();
        self.bones.clear();
    }
}
