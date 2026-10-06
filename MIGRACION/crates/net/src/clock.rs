//! Clocks: the process's own (`now`) and a client's estimate of the server's, so that every
//! client stamps and reads states on one clock.
use std::sync::OnceLock;
use std::time::Instant;

/// Seconds since the first call, never going back: the `now` every `update` of this library
/// wants. Any other monotonic clock in seconds does as well, as long as it is always the same one.
pub fn now() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Seconds to microseconds, as the wire carries times.
pub fn micros(seconds: f64) -> u64 {
    (seconds.max(0.0) * 1e6) as u64
}

/// Microseconds to seconds.
pub fn seconds(micros: u64) -> f64 {
    micros as f64 * 1e-6
}

/// Pings remembered: the estimate comes from the one that went and came back fastest (the least
/// room for the way there and the way back to differ).
const KEPT: usize = 8;
/// The first pings set the estimate at once; after them it only drifts toward the best one.
const SETTLING: u32 = 5;
/// Seconds of estimate corrected per second: a correction is a slide, not a jump, because every
/// millisecond of it moves what is drawn from the others (by metres, at orbital speed).
const SLEW: f64 = 0.01;
/// A difference so large is no drift: the estimate jumps.
const JUMP: f64 = 0.25;

/// What a client knows of the server's clock: `server = ours + offset`.
pub struct ServerClock {
    offset: f64,
    target: f64,
    kept: [(f32, f64); KEPT],
    count: u32,
}

impl Default for ServerClock {
    fn default() -> Self {
        ServerClock::new()
    }
}

impl ServerClock {
    pub fn new() -> ServerClock {
        ServerClock { offset: 0.0, target: 0.0, kept: [(f32::MAX, 0.0); KEPT], count: 0 }
    }
    /// A pong came at `now`: our ping left at `sent` (our clock) and the server's clock said `server` when it answered.
    pub fn sample(&mut self, sent: f64, server: f64, now: f64) {
        let rtt = now - sent;
        if !(0.0..10.0).contains(&rtt) {
            return;
        }
        self.kept[self.count as usize % KEPT] = (rtt as f32, server + rtt * 0.5 - now);
        self.count += 1;
        let best = self.kept.iter().fold(self.kept[0], |a, b| if b.0 < a.0 { *b } else { a });
        self.target = best.1;
        if self.count <= SETTLING || (self.target - self.offset).abs() > JUMP {
            self.offset = self.target;
        }
    }
    /// Lets the estimate slide toward its best measure; call it every frame with the seconds passed.
    pub fn advance(&mut self, dt: f64) {
        let step = SLEW * dt.clamp(0.0, 1.0);
        self.offset += (self.target - self.offset).clamp(-step, step);
    }
    /// Whether any pong has come.
    pub fn known(&self) -> bool {
        self.count > 0
    }
    /// The server's clock when ours says `now`.
    pub fn server_time(&self, now: f64) -> f64 {
        now + self.offset
    }
    /// The fastest round trip among the pings remembered, seconds.
    pub fn best_rtt(&self) -> f32 {
        if self.count == 0 { 0.0 } else { self.kept.iter().fold(f32::MAX, |a, b| a.min(b.0)) }
    }
}
