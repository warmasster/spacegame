//! Recipes compiled for a sample rate, and the voice that renders one.
//!
//! A layer is an oscillator (phase accumulator) or a noise (xorshift; pink by Kellet's three
//! poles, brown by a leaky integrator) through a state-variable filter (Zavalishin's trapezoidal
//! one: stable at any cutoff, three outputs for the price of one), times an envelope. A few
//! multiplies a sample; nothing is looked up or allocated.
use crate::def::{FilterKind, SoundDef, Via, Wave};
use std::f32::consts::PI;

/// Layers a sound may have.
pub const LAYERS: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
struct Layer {
    wave: Wave,
    /// Phase per sample (cycles) at the start, and at the end of a blow that glides.
    w: f32,
    w_end: f32,
    /// Alternating: the other frequency's phase per sample, and changes per sample.
    alt: Option<(f32, f32)>,
    /// The filter: which output, `g` and `k`.
    filter: Option<(FilterKind, f32, f32)>,
    gain: f32,
    /// Per sample: the attack's rise, the decay's factor (1: it holds).
    attack: f32,
    decay: f32,
    /// Tremolo: cycles per sample, depth.
    trem: Option<(f32, f32)>,
}

/// A sound ready to render at one sample rate.
#[derive(Clone, Debug, Default)]
pub struct Sound {
    layers: [Layer; LAYERS],
    count: usize,
    /// A blow's length (samples); `None`: a loop.
    pub len: Option<u32>,
    pub via: Via,
    pub gain: f32,
    pub vary: f32,
}

impl Sound {
    pub fn new(def: &SoundDef, rate: f32) -> Result<Sound, String> {
        if def.capas.is_empty() || def.capas.len() > LAYERS {
            return Err(format!("de 1 a {LAYERS} capas, no {}", def.capas.len()));
        }
        let mut layers = [Layer::default(); LAYERS];
        for (l, d) in layers.iter_mut().zip(&def.capas) {
            if !(d.hz > 0.0 && d.hz < rate * 0.45) {
                return Err(format!("frecuencia {} Hz fuera de 0..{:.0}", d.hz, rate * 0.45));
            }
            let filter = match d.filtro {
                Some(f) => {
                    if !(f.hz > 0.0 && f.q > 0.05) {
                        return Err(format!("filtro a {} Hz, q {}", f.hz, f.q));
                    }
                    Some((f.tipo, (PI * f.hz.min(rate * 0.45) / rate).tan(), 1.0 / f.q))
                }
                None => None,
            };
            *l = Layer {
                wave: d.onda,
                w: d.hz / rate,
                w_end: d.hasta.unwrap_or(d.hz).clamp(1.0, rate * 0.45) / rate,
                alt: d.alterna.map(|[hz, per_s]| (hz.clamp(1.0, rate * 0.45) / rate, per_s.max(0.0) / rate)),
                filter,
                gain: d.ganancia,
                attack: if d.ataque > 0.0 { 1.0 / (d.ataque * rate) } else { 1.0 },
                decay: d.caida.map_or(1.0, |t| (-1.0 / (t.max(1e-4) * rate)).exp()),
                trem: d.tremolo.map(|[hz, depth]| (hz / rate, depth.clamp(0.0, 1.0))),
            };
        }
        let len = match def.dura {
            Some(t) if t > 0.0 => Some((t * rate) as u32),
            Some(t) => return Err(format!("dura {t} s")),
            None => None,
        };
        Ok(Sound { layers, count: def.capas.len(), len, via: def.via, gain: def.ganancia, vary: def.variacion.clamp(0.0, 0.5) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct LayerState {
    phase: f32,
    alt: f32,
    trem: f32,
    /// The attack so far (0..1), the decay so far (1..0).
    rise: f32,
    fall: f32,
    /// The filter's two integrators; the noise's poles.
    ic: [f32; 2],
    pink: [f32; 3],
    brown: f32,
}

/// One sound sounding.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    pub sound: u16,
    pub active: bool,
    /// Samples rendered.
    pub age: u32,
    /// Its level now and the one it goes to (smoothed: no clicks), its pitch likewise.
    pub gain: f32,
    pub target: f32,
    pub pitch: f32,
    pub pitch_target: f32,
    /// Left and right.
    pub pan: [f32; 2],
    /// The listener touches its source (see `medium::TOUCH_SHARE`).
    pub touch: bool,
    rng: u32,
    layers: [LayerState; LAYERS],
}

impl Default for Voice {
    fn default() -> Voice {
        Voice { sound: 0, active: false, age: 0, gain: 0.0, target: 0.0, pitch: 1.0, pitch_target: 1.0, pan: [0.707; 2], touch: false, rng: 0x9E37_79B9, layers: [LayerState::default(); LAYERS] }
    }
}

/// sin(2π·p) for p in 0..1, by a parabola bent once more (within a thousandth of the real one:
/// no ear tells, and it is a few multiplies where the library's is a call).
#[inline]
fn sine(p: f32) -> f32 {
    let x = 2.0 * p - 1.0;
    let y = 4.0 * x * (1.0 - x.abs());
    -(0.225 * (y * y.abs() - y) + y)
}

/// `p + w` kept in 0..1 (`w` under 1).
#[inline]
fn wrap(p: f32) -> f32 {
    if p >= 1.0 { p - 1.0 } else { p }
}

/// The last milliseconds of a blow fade (samples), so that it does not end on a step.
const TAIL: u32 = 96;

impl Voice {
    /// Start `sound` from silence (`seed` tells this voice's noise from the others').
    pub fn start(&mut self, sound: u16, gain: f32, pitch: f32, pan: f32, touch: bool, seed: u32) {
        let p = pan.clamp(-1.0, 1.0) * 0.25 * PI + 0.25 * PI;
        *self = Voice { sound, active: true, age: 0, gain, target: gain, pitch, pitch_target: pitch, pan: [p.cos(), p.sin()], touch, rng: seed | 1, layers: [LayerState { fall: 1.0, ..LayerState::default() }; LAYERS] };
    }

    #[inline]
    fn noise(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as i32) as f32 * (1.0 / 2_147_483_648.0)
    }

    /// A number in −1..1 (for whoever starts voices: the pitch of each blow).
    pub fn chance(&mut self) -> f32 {
        self.noise()
    }

    /// Add this voice's next `out.len()` samples to `out` (mono), its level moving toward its
    /// target by `smooth` a sample. A blow that ends, and a loop faded to nothing, go idle.
    pub fn render(&mut self, s: &Sound, out: &mut [f32], smooth: f32) {
        let n = out.len();
        let mut env = [0.0f32; crate::mixer::BLOCK];
        // the level over the block: toward the target; a blow's end fades
        for (i, e) in env[..n].iter_mut().enumerate() {
            self.gain += (self.target - self.gain) * smooth;
            let tail = match s.len {
                Some(len) => (len.saturating_sub(self.age + i as u32)).min(TAIL) as f32 / TAIL as f32,
                None => 1.0,
            };
            *e = self.gain * tail * s.gain;
        }
        self.pitch += (self.pitch_target - self.pitch) * (smooth * n as f32).min(1.0);
        let pitch = self.pitch;
        let glide = s.len.map_or(0.0, |len| 1.0 / len.max(1) as f32);
        for k in 0..s.count {
            let l = s.layers[k];
            let mut st = self.layers[k];
            let (a1, a2, a3, kq) = match l.filter {
                Some((_, g, kq)) => {
                    let a1 = 1.0 / (1.0 + g * (g + kq));
                    (a1, g * a1, g * g * a1, kq)
                }
                None => (0.0, 0.0, 0.0, 0.0),
            };
            for i in 0..n {
                // the source
                let x = match l.wave {
                    Wave::Ruido => self.noise(),
                    Wave::RuidoRosa => {
                        let w = self.noise();
                        st.pink[0] = 0.997_65 * st.pink[0] + w * 0.099_046;
                        st.pink[1] = 0.963_00 * st.pink[1] + w * 0.296_516_4;
                        st.pink[2] = 0.570_00 * st.pink[2] + w * 1.052_691_3;
                        (st.pink[0] + st.pink[1] + st.pink[2] + w * 0.1848) * 0.25
                    }
                    Wave::RuidoMarron => {
                        st.brown = (st.brown + 0.02 * self.noise()) / 1.02;
                        st.brown * 3.5
                    }
                    wave => {
                        let mut w = l.w + (l.w_end - l.w) * ((self.age + i as u32) as f32 * glide).min(1.0);
                        if let Some((other, rate)) = l.alt {
                            st.alt = wrap(st.alt + rate);
                            if st.alt >= 0.5 {
                                w = other;
                            }
                        }
                        st.phase = wrap(st.phase + (w * pitch).min(0.49));
                        let p = st.phase;
                        match wave {
                            Wave::Seno => sine(p),
                            Wave::Triangulo => 4.0 * (p - 0.5).abs() - 1.0,
                            Wave::Sierra => 2.0 * p - 1.0,
                            _ => {
                                if p < 0.5 {
                                    1.0
                                } else {
                                    -1.0
                                }
                            }
                        }
                    }
                };
                // the filter
                let y = match l.filter {
                    Some((kind, _, _)) => {
                        let v3 = x - st.ic[1];
                        let v1 = a1 * st.ic[0] + a2 * v3;
                        let v2 = st.ic[1] + a2 * st.ic[0] + a3 * v3;
                        st.ic = [2.0 * v1 - st.ic[0], 2.0 * v2 - st.ic[1]];
                        match kind {
                            FilterKind::Bajo => v2,
                            FilterKind::Banda => v1,
                            FilterKind::Alto => x - kq * v1 - v2,
                        }
                    }
                    None => x,
                };
                // its envelope
                st.rise = (st.rise + l.attack).min(1.0);
                st.fall *= l.decay;
                let mut level = l.gain * st.rise * st.fall;
                if let Some((rate, depth)) = l.trem {
                    st.trem = wrap(st.trem + rate);
                    level *= 1.0 - depth * (0.5 - 0.5 * sine(wrap(st.trem + 0.25)));
                }
                out[i] += y * level * env[i];
            }
            // (a filter fed nothing for long can drift into denormals: slow, and silent)
            if st.ic[0].abs() < 1e-20 {
                st.ic[0] = 0.0;
            }
            if st.ic[1].abs() < 1e-20 {
                st.ic[1] = 0.0;
            }
            self.layers[k] = st;
        }
        self.age = self.age.saturating_add(n as u32);
        match s.len {
            Some(len) if self.age >= len => self.active = false,
            None if self.target <= 0.0 && self.gain < 1e-4 => self.active = false,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quick_sine_is_the_sine() {
        for k in 0..1000 {
            let p = k as f32 / 1000.0;
            assert!((sine(p) - (p * std::f32::consts::TAU).sin()).abs() < 0.0012, "{p}: {} contra {}", sine(p), (p * std::f32::consts::TAU).sin());
        }
    }
}
