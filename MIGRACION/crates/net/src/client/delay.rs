//! How far in the past the others are drawn. To have something to interpolate towards, the
//! newest snapshot must be newer than the moment drawn: the delay must cover how old a snapshot is
//! when it arrives (the way here, plus its wait at the server for the next tick) and one interval
//! between snapshots. On a good connection that is about 100 ms; on a worse one the delay grows
//! with what is measured, and comes back down slowly when things improve.
const FLOOR: f64 = 0.1;
const CEILING: f64 = 0.4;
/// Slack over what was measured.
const MARGIN: f64 = 0.01;
/// The oldest arrival remembered fades this many seconds each second.
const FADE: f64 = 0.02;
/// The delay grows at most this many seconds a second (the others are seen that much slower meanwhile)...
const GROW: f64 = 0.1;
/// ...and shrinks at most this many (seen that much faster).
const SHRINK: f64 = 0.03;

pub(super) struct Delay {
    value: f64,
    /// How old the oldest recent snapshot was when it arrived.
    oldest: f64,
}

impl Delay {
    pub fn new() -> Delay {
        Delay { value: FLOOR, oldest: 0.0 }
    }
    /// A snapshot arrived `age` seconds after it was sampled.
    pub fn observe(&mut self, age: f64) {
        if (0.0..1.0).contains(&age) {
            self.oldest = self.oldest.max(age);
        }
    }
    /// `dt` seconds passed; `tick`: seconds between snapshots.
    pub fn advance(&mut self, dt: f64, tick: f64) {
        let dt = dt.clamp(0.0, 0.25);
        self.oldest = (self.oldest - FADE * dt).max(0.0);
        let target = (self.oldest + tick + MARGIN).clamp(FLOOR, CEILING);
        self.value += (target - self.value).clamp(-SHRINK * dt, GROW * dt);
    }
    pub fn seconds(&self) -> f64 {
        self.value
    }
}
