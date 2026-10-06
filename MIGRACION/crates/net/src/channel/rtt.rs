//! Round-trip time of a channel and, from it, how long to wait before sending a reliable message
//! again. The smoothing is TCP's (RFC 6298: an eighth of each new sample, a quarter for the
//! spread); the floor is a game's, not TCP's second.
#[derive(Clone, Copy)]
pub(super) struct Rtt {
    smooth: f32,
    spread: f32,
    known: bool,
}

/// Before the first measure: a guess that suits a home connection.
const GUESS: f32 = 0.2;
const FLOOR: f32 = 0.05;
const CEILING: f32 = 1.0;

impl Rtt {
    pub fn new() -> Rtt {
        Rtt { smooth: 0.0, spread: 0.0, known: false }
    }
    pub fn sample(&mut self, r: f32) {
        if !(0.0..=10.0).contains(&r) {
            return;
        }
        if self.known {
            self.spread = 0.75 * self.spread + 0.25 * (self.smooth - r).abs();
            self.smooth = 0.875 * self.smooth + 0.125 * r;
        } else {
            (self.smooth, self.spread, self.known) = (r, r * 0.5, true);
        }
    }
    /// Seconds there and back, smoothed; 0 before the first measure.
    pub fn seconds(&self) -> f32 {
        self.smooth
    }
    /// Seconds without an ack after which a reliable message goes again.
    pub fn resend_after(&self) -> f64 {
        if self.known { (self.smooth + 4.0 * self.spread).clamp(FLOOR, CEILING) as f64 } else { GUESS as f64 }
    }
}
