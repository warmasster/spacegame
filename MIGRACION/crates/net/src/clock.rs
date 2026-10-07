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

/// Pings remembered (one every 2 s: half a minute): the estimate comes from those that went and
/// came back fastest (the least room for the way there and the way back to differ), averaged:
/// every one within `NEAR` of the fastest. A new fastest one, or the fastest one forgotten,
/// moves it by a fraction of what one alone would.
const KEPT: usize = 16;
const NEAR: f64 = 0.002;
/// The first pings set the estimate at once; after them it only drifts toward the best one.
const SETTLING: u32 = 5;
/// Seconds of estimate corrected per second: a correction is a slow slide, not a jump, because
/// every millisecond of it moves what is drawn from the others by the way they go (at orbital
/// speed, 8 m): sliding at half a millisecond a second, a ship at 7.8 km/s seems to drift at 4 m/s
/// while it lasts, and one at 300 m/s at 15 cm/s.
pub const SLEW: f64 = 0.0005;
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
        let kept = &self.kept[..(self.count as usize).min(KEPT)];
        let best = kept.iter().fold(f32::MAX, |a, b| a.min(b.0));
        let (sum, n) = kept.iter().filter(|k| f64::from(k.0) <= f64::from(best) + NEAR).fold((0.0, 0.0), |(s, n), k| (s + k.1, n + 1.0));
        self.target = sum / n;
        if self.count <= SETTLING || (self.target - self.offset).abs() > JUMP {
            self.offset = self.target;
        }
    }
    /// Lets the estimate slide toward its best measure; call it every frame with the seconds passed.
    pub fn advance(&mut self, dt: f64) {
        let step = SLEW * dt.clamp(0.0, 1.0);
        self.offset += (self.target - self.offset).clamp(-step, step);
    }
    /// Whether it is past its first pings, which set it at once: from here on it only slides.
    pub fn settled(&self) -> bool {
        self.count > SETTLING
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
    #[doc(hidden)]
    pub fn offset(&self) -> f64 {
        self.offset
    }

    pub fn best_rtt(&self) -> f32 {
        if self.count == 0 { 0.0 } else { self.kept.iter().fold(f32::MAX, |a, b| a.min(b.0)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_estimate_of_the_servers_clock_is_close_and_never_slides_fast() {
        // the server 3.2 s ahead; each way 40 ms and up to `jitter` more at random: the estimate
        // can only be as good as the fastest round trips are even (half the jitter, at worst)
        for (jitter, within) in [(0.03, 0.008), (0.005, 0.002), (0.0, 0.0002)] {
            let truth = 3.2;
            let mut c = ServerClock::new();
            let mut rng = 0x9E37_79B9_7F4A_7C15u64;
            let mut rand = || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                (rng >> 11) as f64 / (1u64 << 53) as f64
            };
            let (mut now, mut worst_after, mut fastest) = (0.0, 0.0f64, 0.0f64);
            let mut was = c.offset();
            for n in 0..600 {
                // a ping every 2 s (quicker at first), the estimate advanced every frame between
                let gap = if n < 6 { 0.1 } else { 2.0 };
                let (up, down) = (0.04 + jitter * rand(), 0.04 + jitter * rand());
                let sent = now;
                c.sample(sent, sent + up + truth, sent + up + down);
                for _ in 0..(gap * 60.0) as usize {
                    now += 1.0 / 60.0;
                    c.advance(1.0 / 60.0);
                    if n > 30 {
                        fastest = fastest.max((c.offset() - was).abs() * 60.0);
                        worst_after = worst_after.max((c.offset() - truth).abs());
                    }
                    was = c.offset();
                }
            }
            // never more than its slide; within what the jitter allows once settled
            assert!(fastest <= SLEW + 1e-9, "slid at {fastest} s/s");
            assert!(worst_after < within, "jitter {jitter}: off by {:.2} ms", worst_after * 1000.0);
        }
    }
}
