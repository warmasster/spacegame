//! Snapshot interpolation: the last states received of something, each with the moment it was
//! sampled, and the state at any moment asked: between two snapshots, mixed; a little past the
//! newest, carried on by its own velocity; further past, held.
use crate::game::{PlayerState, RigidState};

/// Snapshots kept per thing: at 20 a second, 0.8 s of history.
pub const SNAPS: usize = 16;
/// Seconds a state is carried past its newest snapshot before it is held where it is.
pub const EXTRAPOLATE: f64 = 0.25;

/// A state that can live in a buffer.
pub trait Snap: Default {
    fn set(&mut self, from: &Self);
    fn mix_into(a: &Self, b: &Self, t: f32, out: &mut Self);
    fn carry(&mut self, dt: f32);
}

impl Snap for PlayerState {
    fn set(&mut self, from: &Self) {
        *self = *from;
    }
    fn mix_into(a: &Self, b: &Self, t: f32, out: &mut Self) {
        *out = PlayerState::mix(a, b, t);
    }
    fn carry(&mut self, dt: f32) {
        PlayerState::carry(self, dt);
    }
}

impl Snap for RigidState {
    fn set(&mut self, from: &Self) {
        RigidState::set(self, from);
    }
    fn mix_into(a: &Self, b: &Self, t: f32, out: &mut Self) {
        RigidState::mix_into(a, b, t, out);
    }
    fn carry(&mut self, dt: f32) {
        RigidState::carry(self, dt);
    }
}

#[derive(Default)]
struct Shot<T> {
    stamp: f64,
    /// The thing stood still from the snapshot before until one tick before this one.
    held: bool,
    state: T,
}

/// A ring of snapshots, oldest to newest.
pub struct SnapBuffer<T: Snap> {
    ring: Vec<Shot<T>>,
    head: usize,
    len: usize,
}

impl<T: Snap> Default for SnapBuffer<T> {
    fn default() -> Self {
        SnapBuffer { ring: Vec::new(), head: 0, len: 0 }
    }
}

impl<T: Snap> SnapBuffer<T> {
    pub fn clear(&mut self) {
        (self.head, self.len) = (0, 0);
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn at(&self, i: usize) -> &Shot<T> {
        &self.ring[(self.head + i) % SNAPS]
    }
    /// The newest snapshot: its moment and its state.
    pub fn newest(&self) -> Option<(f64, &T)> {
        (self.len > 0).then(|| {
            let s = self.at(self.len - 1);
            (s.stamp, &s.state)
        })
    }
    /// Adds the snapshot of moment `stamp` if it is newer than all we have (an older one is of no
    /// use any more) and gives its place to be filled.
    pub fn push(&mut self, stamp: f64, held: bool) -> Option<&mut T> {
        if self.newest().is_some_and(|(newest, _)| stamp <= newest) {
            return None;
        }
        if self.ring.len() < SNAPS {
            self.ring.resize_with(SNAPS, Shot::default);
        }
        if self.len == SNAPS {
            self.head = (self.head + 1) % SNAPS;
            self.len -= 1;
        }
        let slot = &mut self.ring[(self.head + self.len) % SNAPS];
        self.len += 1;
        (slot.stamp, slot.held) = (stamp, held);
        Some(&mut slot.state)
    }
    /// The state at moment `t` into `out`; false if there is no snapshot at all. `tick`: seconds
    /// between two sends of a sender that has something new.
    pub fn sample(&self, t: f64, tick: f64, out: &mut T) -> bool {
        let Some((newest, state)) = self.newest() else { return false };
        if t >= newest {
            out.set(state);
            out.carry((t - newest).min(EXTRAPOLATE) as f32);
            return true;
        }
        // From the newest back: the moment asked is almost always between the last two or three.
        for i in (0..self.len - 1).rev() {
            let (a, b) = (self.at(i), self.at(i + 1));
            if t >= a.stamp {
                let gap = b.stamp - a.stamp;
                // After a silence the change happened in the last tick, not along the whole silence.
                let f = if b.held && gap > tick { (t - (b.stamp - tick)) / tick } else { (t - a.stamp) / gap };
                T::mix_into(&a.state, &b.state, f.clamp(0.0, 1.0) as f32, out);
                return true;
            }
        }
        out.set(&self.at(0).state);
        true
    }
}
