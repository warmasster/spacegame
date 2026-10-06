//! The mixer: a fixed pool of voices, three ways to the ear, one limiter.
//!
//! It runs on the sound card's thread. The game talks to it through a wait-free queue (`Cmd`):
//! it never waits for the game nor the game for it, nothing is allocated here, and a queue
//! found full only drops a sound. The first `LOOPS` voices are held levels (an engine, a fan:
//! the game says each frame how loud); the rest are blows, the oldest making room when all are
//! busy.
use crate::{
    def::Via,
    medium::{CONTACT_HZ, TOUCH_SHARE},
    synth::{Sound, Voice},
};
use std::sync::Arc;

/// Voices in all, and how many of them are loops (slots the game names).
pub const VOICES: usize = 32;
pub const LOOPS: usize = 12;
/// Samples rendered at a time.
pub const BLOCK: usize = 128;

/// What the game asks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    /// A blow: sound, level, pitch (1: as written), where (−1 left .. 1 right), whether the
    /// listener touches its source.
    Play { sound: u16, gain: f32, pitch: f32, pan: f32, touch: bool },
    /// A held level in a slot (under `LOOPS`): 0 lets it die away. Sent whenever it changes.
    Loop { slot: u8, sound: u16, gain: f32, pitch: f32, touch: bool },
    /// The air round the head (its gain, `medium::air_gain`).
    Air(f32),
    /// The whole mix.
    Master(f32),
}

/// Two one-pole low-passes in a row (12 dB an octave): what touching lets through.
#[derive(Clone, Copy, Debug, Default)]
struct Dull([f32; 2]);

impl Dull {
    #[inline]
    fn pass(&mut self, x: f32, k: f32) -> f32 {
        self.0[0] += (x - self.0[0]) * k;
        self.0[1] += (self.0[0] - self.0[1]) * k;
        self.0[1]
    }
}

pub struct Mixer {
    sounds: Arc<[Sound]>,
    voices: [Voice; VOICES],
    rx: rtrb::Consumer<Cmd>,
    /// The air's gain now and where it goes; the whole mix likewise.
    air: f32,
    air_target: f32,
    master: f32,
    master_target: f32,
    /// A level's smoothing per sample (about 12 ms), and a block's.
    smooth: f32,
    dull: [Dull; 2],
    dull_k: f32,
    seed: u32,
    /// Voices sounding after the last fill (for whoever shows it).
    pub busy: usize,
}

impl Mixer {
    pub fn new(sounds: Arc<[Sound]>, rate: f32, rx: rtrb::Consumer<Cmd>) -> Mixer {
        Mixer {
            sounds,
            voices: [Voice::default(); VOICES],
            rx,
            air: 0.0,
            air_target: 0.0,
            master: 1.0,
            master_target: 1.0,
            smooth: 1.0 - (-1.0 / (0.012 * rate)).exp(),
            dull: [Dull::default(); 2],
            dull_k: 1.0 - (-std::f32::consts::TAU * CONTACT_HZ / rate).exp(),
            seed: 0x1234_5678,
            busy: 0,
        }
    }

    fn take(&mut self, c: Cmd) {
        match c {
            Cmd::Play { sound, gain, pitch, pan, touch } => {
                let Some(s) = self.sounds.get(usize::from(sound)) else { return };
                if s.len.is_none() {
                    return;
                }
                // a free voice, or the oldest blow
                let k = (LOOPS..VOICES).find(|&k| !self.voices[k].active).unwrap_or_else(|| (LOOPS..VOICES).max_by_key(|&k| self.voices[k].age).unwrap_or(LOOPS));
                self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = &mut self.voices[k];
                v.start(sound, gain, pitch, pan, touch, self.seed);
                let vary = 1.0 + s.vary * v.chance();
                (v.pitch, v.pitch_target) = (pitch * vary, pitch * vary);
            }
            Cmd::Loop { slot, sound, gain, pitch, touch } => {
                let k = usize::from(slot);
                if k >= LOOPS || self.sounds.get(usize::from(sound)).is_none_or(|s| s.len.is_some()) {
                    return;
                }
                let v = &mut self.voices[k];
                if !v.active || v.sound != sound {
                    if gain <= 0.0 {
                        return;
                    }
                    self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    v.start(sound, 0.0, pitch, 0.0, touch, self.seed);
                }
                (v.target, v.pitch_target, v.touch) = (gain.max(0.0), pitch, touch);
            }
            Cmd::Air(a) => self.air_target = a.clamp(0.0, 1.0),
            Cmd::Master(m) => self.master_target = m.clamp(0.0, 2.0),
        }
    }

    /// Fill `out` (frames of `channels` samples): the first two channels left and right (one:
    /// both together), any other silent.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        while let Ok(c) = self.rx.pop() {
            self.take(c);
        }
        let channels = channels.max(1);
        let sounds = self.sounds.clone();
        for chunk in out.chunks_mut(BLOCK * channels) {
            let n = chunk.len() / channels;
            // the three ways: through the air, through what is touched, inside the suit
            let mut air = [[0.0f32; BLOCK]; 2];
            let mut touch = [[0.0f32; BLOCK]; 2];
            let mut suit = [[0.0f32; BLOCK]; 2];
            let mut mono = [0.0f32; BLOCK];
            for v in self.voices.iter_mut().filter(|v| v.active) {
                let Some(s) = sounds.get(usize::from(v.sound)) else {
                    v.active = false;
                    continue;
                };
                mono[..n].fill(0.0);
                v.render(s, &mut mono[..n], self.smooth);
                let (l, r) = (v.pan[0], v.pan[1]);
                let bus = match s.via {
                    Via::Aire => &mut air,
                    Via::Contacto => &mut touch,
                    Via::Traje => &mut suit,
                };
                for i in 0..n {
                    bus[0][i] += mono[i] * l;
                    bus[1][i] += mono[i] * r;
                }
                if s.via == Via::Aire && v.touch {
                    for i in 0..n {
                        touch[0][i] += mono[i] * l * TOUCH_SHARE;
                        touch[1][i] += mono[i] * r * TOUCH_SHARE;
                    }
                }
            }
            // (a level that has all but arrived is there: a vacuum is silence, not nearly)
            if (self.air_target - self.air).abs() < 1e-4 {
                self.air = self.air_target;
            }
            for i in 0..n {
                self.air += (self.air_target - self.air) * self.smooth;
                self.master += (self.master_target - self.master) * self.smooth;
                let mut frame = [0.0f32; 2];
                for (c, f) in frame.iter_mut().enumerate() {
                    let x = (air[c][i] * self.air + self.dull[c].pass(touch[c][i], self.dull_k) + suit[c][i]) * self.master;
                    // a soft ceiling: nothing leaves over 1 (tanh, by its Padé)
                    let x = x.clamp(-3.0, 3.0);
                    *f = x * (27.0 + x * x) / (27.0 + 9.0 * x * x);
                }
                let o = &mut chunk[i * channels..(i + 1) * channels];
                if channels == 1 {
                    o[0] = 0.5 * (frame[0] + frame[1]);
                } else {
                    o[0] = frame[0];
                    o[1] = frame[1];
                    o[2..].fill(0.0);
                }
            }
        }
        self.busy = self.voices.iter().filter(|v| v.active).count();
    }
}
