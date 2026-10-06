//! When a state is worth sending: when its bytes changed, twice more after the last change (so
//! the final state survives a lost datagram), and — for what asks for it — once every so often
//! while it stays the same (so a long silence is covered). A player standing still costs a
//! twentieth of one that moves; a thing at rest costs nothing at all.
pub const REPEATS: u8 = 2;
/// Seconds between two sends of a player that does not change.
pub const HEARTBEAT: f64 = 1.0;
/// For what is never sent again while it stays the same (things: a crate lying still).
pub const NEVER: f64 = f64::INFINITY;

#[derive(Default)]
pub struct Throttle {
    last: Vec<u8>,
    sent_at: f64,
    repeats: u8,
    /// Something has been sent since the last `reset`.
    fresh: bool,
    /// A tick went by without sending since the last send.
    skipped: bool,
}

impl Throttle {
    /// Called once a tick with the state's bytes: `None` if it need not go now; `Some(held)` if it
    /// must, `held` saying that ticks were skipped before it (the state stood still until now, so
    /// whoever interpolates must not stretch this change over the silence). `heartbeat`: seconds
    /// after which it goes again though it has not changed.
    pub fn due(&mut self, key: &[u8], now: f64, heartbeat: f64) -> Option<bool> {
        let changed = !self.fresh || key != self.last.as_slice();
        if !changed && self.repeats == 0 && now - self.sent_at < heartbeat {
            self.skipped = true;
            return None;
        }
        if changed {
            self.last.clear();
            self.last.extend_from_slice(key);
            self.repeats = REPEATS;
        } else {
            self.repeats = self.repeats.saturating_sub(1);
        }
        let held = self.fresh && self.skipped;
        (self.fresh, self.skipped, self.sent_at) = (true, false, now);
        Some(held)
    }
    /// Whether the last change still has to go again (so it survives a lost datagram).
    pub fn pending(&self) -> bool {
        self.fresh && self.repeats > 0
    }
    /// A tick went by in which the state was not even looked at (nobody set it: it is at rest).
    pub fn skip(&mut self) {
        self.skipped = true;
    }
    /// Forgets what was sent: whatever comes next goes.
    pub fn reset(&mut self) {
        (self.fresh, self.skipped, self.repeats) = (false, false, 0);
    }
}
