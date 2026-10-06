//! Dynamic resolution: scales the render target down when frames run over the target time and
//! back up when there is room, measured over half-second windows.
use std::time::Instant;

pub struct Governor {
    sum_ms: f32,
    frames: u32,
    seconds: f32,
    last: Instant,
}

impl Governor {
    pub fn new() -> Governor {
        Governor { sum_ms: 0.0, frames: 0, seconds: 0.0, last: Instant::now() }
    }

    /// `gpu_ms`: GPU time when timestamps work; `frame`: frames so far; returns the new scale.
    pub fn update(&mut self, enabled: bool, target_fps: f32, gpu_ms: Option<f32>, frame: u64, scale: f32) -> f32 {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;
        if !enabled {
            return 1.0;
        }
        self.sum_ms += gpu_ms.unwrap_or(dt * 1000.0);
        self.frames += 1;
        self.seconds += dt;
        if self.seconds < 0.5 || frame < 60 {
            return scale;
        }
        let avg = self.sum_ms / self.frames as f32;
        (self.sum_ms, self.frames, self.seconds) = (0.0, 0, 0.0);
        let target = 1000.0 / target_fps.max(10.0);
        if avg > target * 0.92 {
            (scale - 0.07).max(0.5)
        } else if avg < target * 0.7 {
            (scale + 0.04).min(1.0)
        } else {
            scale
        }
    }
}

impl Default for Governor {
    fn default() -> Self {
        Governor::new()
    }
}
