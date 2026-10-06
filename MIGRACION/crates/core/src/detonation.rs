//! Detonations from a charge: a conventional explosive is only its TNT equivalent and the metal
//! round it; the crater, flash, shake, damage and every particle layer come out of scaling laws
//! whose constants are data (`detonacion.jsonc`). A new round or warhead is one `charge`, nothing
//! more.
//!
//! - What the blast does scales with the cube root of the charge, W^⅓ (radius of light, shake and
//!   damage), and its times with W^⅙.
//! - What the ground does follows gravity-regime cratering. The crater radius is
//!   R = k·W^⅓·(g_ref/g)^⅙. Ejecta leave at multiples of √(g·R), so they land a few crater radii
//!   out, and their times are multiples of T = √(R/g). The same charge digs wider and throws
//!   longer on a smaller moon.
//!
//! Layers are resolved per blast into `Burst`s, the plain emitters `effects` spawns.
use crate::effects::{BlastDef, CraterDef, FlashDef, ShakeDef};
use serde::Deserialize;

/// A conventional explosive charge.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeDef {
    /// Explosive (kg of TNT equivalent).
    pub tnt: f32,
    /// Metal casing round it (kg): hot fragments.
    #[serde(default)]
    pub casing: f32,
}

/// Units a layer is written in.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    /// Sizes, spread and lift in crater radii R, speeds in √(g·R), lives in T = √(R/g).
    #[default]
    Crater,
    /// Metres, m/s and seconds as written.
    Absolute,
}

/// What a layer's count grows with.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Explosive,
    Casing,
}

/// One kind of particle a detonation throws.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDef {
    /// Style name in `particles.jsonc`.
    pub style: String,
    /// count = n · (kg of its source)^⅓, at most `max`.
    pub count: f32,
    pub max: u32,
    #[serde(default)]
    pub of: Source,
    #[serde(default)]
    pub scale: Scale,
    /// Launch speed, min..max.
    pub speed: [f32; 2],
    /// Pareto shape of the launch speeds: 0 even between min and max; ~0.6 most slow, a few fast
    /// (how ejecta leave a crater).
    #[serde(default)]
    pub power: f32,
    /// Half-angle of the launch cone round the vertical (deg), and the angle they bunch toward.
    pub cone: f32,
    #[serde(default)]
    pub tilt: Option<f32>,
    pub size: [f32; 2],
    pub life: [f32; 2],
    #[serde(default)]
    pub spread: f32,
    #[serde(default)]
    pub lift: f32,
    /// Only for a blast on the ground (one that digs a crater).
    #[serde(default)]
    pub ground: bool,
}

/// R = k · W^⅓ · (g_ref / g)^⅙; depth and rim in R; the crater shows after `delay` T, under the
/// dust.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraterLaw {
    pub k: f32,
    pub g_ref: f32,
    pub depth: f32,
    pub rim: f32,
    pub delay: f32,
}

/// Intensity and range per W^⅓ (intensity up to `max_intensity`), duration per W^⅙, lift in R.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashLaw {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub duration: f32,
    pub lift: f32,
    #[serde(default = "unbounded")]
    pub max_intensity: f32,
}

/// Amplitude (up to `max_amplitude`) and range per W^⅓, duration per W^⅙.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShakeLaw {
    pub amplitude: f32,
    pub range: f32,
    pub duration: f32,
    #[serde(default = "unbounded")]
    pub max_amplitude: f32,
}

fn unbounded() -> f32 {
    f32::INFINITY
}

/// Share of the energy that reaches structures, and their reach per W^⅓ (m).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageLaw {
    pub share: f32,
    pub reach: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetonationRules {
    /// J per kg of TNT.
    pub tnt_energy: f32,
    pub crater: CraterLaw,
    pub flash: FlashLaw,
    pub shake: ShakeLaw,
    pub damage: DamageLaw,
    pub layers: Vec<LayerDef>,
}

/// An emitter ready to spawn: style index and every range in metres, m/s and seconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Burst {
    pub style: u8,
    pub count: u32,
    pub speed: [f32; 2],
    pub power: f32,
    pub cone: f32,
    pub tilt: Option<f32>,
    pub size: [f32; 2],
    pub life: [f32; 2],
    pub spread: f32,
    pub lift: f32,
}

impl Burst {
    /// A launch speed for `u` in 0..1 (Pareto between min and max when `power` > 0).
    pub fn speed_at(&self, u: f32) -> f32 {
        let [lo, hi] = self.speed;
        if self.power <= 0.0 || lo <= 0.0 || hi <= lo {
            return lo + (hi - lo) * u;
        }
        let a = self.power;
        let tail = 1.0 - (lo / hi).powf(a);
        lo * (1.0 - u * tail).powf(-1.0 / a)
    }
}

/// What one charge does where it goes off.
#[derive(Clone, Copy, Debug)]
pub struct Detonation {
    /// kg of TNT, the impact's energy included.
    pub tnt: f32,
    pub crater: CraterDef,
    pub flash: FlashDef,
    pub shake: ShakeDef,
    pub damage: BlastDef,
    /// T = √(R/g) (s) and √(g·R) (m/s): the crater's time and speed.
    pub time: f32,
    pub speed: f32,
}

impl DetonationRules {
    pub fn validate(&self, has_style: impl Fn(&str) -> bool) -> Result<(), String> {
        if self.tnt_energy <= 0.0 || self.crater.k <= 0.0 || self.crater.g_ref <= 0.0 {
            return Err("detonacion: tnt_energy, crater.k and crater.g_ref must be > 0".into());
        }
        for l in &self.layers {
            if !has_style(&l.style) {
                return Err(format!("detonacion: unknown particle style '{}'", l.style));
            }
        }
        Ok(())
    }

    /// `charge` going off with `extra` J more (an impact's speed) under gravity `g` (m/s²).
    pub fn resolve(&self, charge: &ChargeDef, extra: f32, g: f32) -> Detonation {
        let w = (charge.tnt + extra.max(0.0) / self.tnt_energy).max(1e-6);
        let (cube, sixth) = (w.cbrt(), w.powf(1.0 / 6.0));
        let g = g.max(1e-3);
        let c = &self.crater;
        let r = c.k * cube * (c.g_ref / g).powf(1.0 / 6.0);
        let (f, s) = (&self.flash, &self.shake);
        Detonation {
            tnt: w,
            crater: CraterDef { radius: r, depth: r * c.depth, rim: c.rim, delay: c.delay * (r / g).sqrt() },
            flash: FlashDef { color: f.color, intensity: (f.intensity * cube).min(f.max_intensity), range: f.range * cube, duration: f.duration * sixth, lift: f.lift * r },
            shake: ShakeDef { amplitude: (s.amplitude * cube).min(s.max_amplitude), range: s.range * cube, duration: s.duration * sixth },
            damage: BlastDef { energy: self.damage.share * self.tnt_energy * w, radius: self.damage.reach * cube },
            time: (r / g).sqrt(),
            speed: (g * r).sqrt(),
        }
    }

    /// The layers of `d` as bursts, appended to `out`. `styles`: each layer's style index;
    /// `grounded`: the blast is on the ground.
    pub fn bursts(&self, d: &Detonation, charge: &ChargeDef, grounded: bool, styles: &[u8], out: &mut Vec<Burst>) {
        let r = d.crater.radius;
        for (l, &style) in self.layers.iter().zip(styles) {
            if l.ground && !grounded {
                continue;
            }
            let kg = match l.of {
                Source::Explosive => d.tnt,
                Source::Casing => charge.casing,
            };
            if kg <= 0.0 {
                continue;
            }
            let count = (l.count * kg.cbrt()).round().min(l.max as f32) as u32;
            let (len, vel, time) = match l.scale {
                Scale::Crater => (r, d.speed, d.time),
                Scale::Absolute => (1.0, 1.0, 1.0),
            };
            out.push(Burst { style, count, speed: l.speed.map(|v| v * vel), power: l.power, cone: l.cone, tilt: l.tilt, size: l.size.map(|v| v * len), life: l.life.map(|v| v * time), spread: l.spread * len, lift: l.lift * len });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> DetonationRules {
        crate::defs::parse("detonacion", include_str!("../../../assets/defs/detonacion.jsonc")).unwrap()
    }

    #[test]
    fn cube_root_and_gravity_scaling() {
        let r = rules();
        let small = r.resolve(&ChargeDef { tnt: 1.0, casing: 0.0 }, 0.0, 1.62);
        let big = r.resolve(&ChargeDef { tnt: 1000.0, casing: 0.0 }, 0.0, 1.62);
        // ten times the radius for a thousand times the charge
        assert!((big.crater.radius / small.crater.radius - 10.0).abs() < 1e-3);
        assert!((big.damage.radius / small.damage.radius - 10.0).abs() < 1e-3);
        // on a world with a sixty-fourth of the gravity the crater is twice as wide
        let light = r.resolve(&ChargeDef { tnt: 1.0, casing: 0.0 }, 0.0, 1.62 / 64.0);
        assert!((light.crater.radius / small.crater.radius - 2.0).abs() < 1e-3);
        // an impact's energy counts as explosive
        let hit = r.resolve(&ChargeDef { tnt: 1.0, casing: 0.0 }, r.tnt_energy * 7.0, 1.62);
        assert!((hit.tnt - 8.0).abs() < 1e-4);
    }

    #[test]
    fn ejecta_speeds_bunch_low_and_stay_in_range() {
        let b = Burst { speed: [2.0, 30.0], power: 0.6, ..Default::default() };
        assert!((b.speed_at(0.0) - 2.0).abs() < 1e-4);
        assert!((b.speed_at(1.0) - 30.0).abs() < 1e-2);
        assert!(b.speed_at(0.5) < 8.0, "median {}", b.speed_at(0.5));
        let even = Burst { speed: [2.0, 30.0], ..Default::default() };
        assert!((even.speed_at(0.5) - 16.0).abs() < 1e-4);
    }

    #[test]
    fn air_bursts_throw_no_ground_layers() {
        let r = rules();
        let c = ChargeDef { tnt: 10.0, casing: 1.0 };
        let d = r.resolve(&c, 0.0, 1.62);
        let styles = vec![0u8; r.layers.len()];
        let (mut ground, mut air) = (Vec::new(), Vec::new());
        r.bursts(&d, &c, true, &styles, &mut ground);
        r.bursts(&d, &c, false, &styles, &mut air);
        assert!(air.len() < ground.len());
        assert!(ground.iter().all(|b| b.count > 0));
    }
}
