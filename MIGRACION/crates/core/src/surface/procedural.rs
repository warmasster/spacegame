//! The cratered procedural ground (f64 port of the TS terrain), parametrised by `ProceduralDef`.
//! No per-sample heap allocations or shared mutable scratch.
use super::{Sample, Surface};
use crate::cube_sphere::{FACES, V3, cube_arc, params_on};
use crate::noise::{Noise3, Random, crater_profile, hash2i, hash3i, smoothstep};
use serde::Deserialize;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

/// Octave limits of the GPU generator (its noise table has a fixed size).
pub const MAX_MOUNTAIN_OCTAVES: u32 = 3;
pub const MAX_RELIEF_OCTAVES: u32 = 15;
pub const MAX_CRATER_LAYERS: usize = 12;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Octaves {
    pub amp: f64,
    pub wavelength: f64,
    pub count: u32,
    pub gain: f64,
    pub lacunarity: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraterLayer {
    pub cell: f64,
    pub p: f64,
    pub mare: f64,
    pub r_min: f64,
    pub r_max: f64,
    /// Depth scale of this layer (1 = the TS craters, 0 = none): tiny craters can be toned down.
    #[serde(default = "one")]
    pub depth: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProceduralDef {
    pub seed: u32,
    pub datum: f64,
    pub relief: Octaves,
    pub highland_amp: f64,
    pub highland_wavelength: f64,
    pub mountains: Octaves,
    pub mare_depth: f64,
    pub mare_wavelength: f64,
    pub mare_from: f64,
    pub mare_to: f64,
    pub mare_smooth: f64,
    pub complex_from: f64,
    pub albedo_high: f64,
    pub albedo_mare: f64,
    /// Largest first.
    pub craters: Vec<CraterLayer>,
    /// Irregular rims and central peaks (0 = the TS craters exactly).
    pub crater_detail: f64,
    /// Craters whose odds change across the ground (maria) fade in over this share of a layer's
    /// odds instead of switching on at a line, which drew straight cliffs (0 = the TS switch).
    #[serde(default = "default_odds_fade")]
    pub crater_odds_fade: f64,
}

fn default_odds_fade() -> f64 {
    0.3
}

/// How much of a crater drawn `x` (uniform 0..1) exists where its layer's odds are `p`: all of it
/// under `p - band`, none over `p`, a ramp between (a hard switch when `band` is 0).
pub fn crater_presence(x: f64, p: f64, band: f64) -> f64 {
    if band > 0. {
        ((p - x) / band).clamp(0., 1.)
    } else if x > p {
        0.
    } else {
        1.
    }
}

impl ProceduralDef {
    pub fn validate(&self) -> Result<(), String> {
        if self.mountains.count > MAX_MOUNTAIN_OCTAVES {
            return Err(format!("mountains.count {} > {MAX_MOUNTAIN_OCTAVES}", self.mountains.count));
        }
        if self.relief.count > MAX_RELIEF_OCTAVES {
            return Err(format!("relief.count {} > {MAX_RELIEF_OCTAVES}", self.relief.count));
        }
        if self.craters.len() > MAX_CRATER_LAYERS {
            return Err(format!("{} crater layers > {MAX_CRATER_LAYERS}", self.craters.len()));
        }
        if let Some(c) = self.craters.iter().find(|c| c.r_max > 0.45 || c.r_min > c.r_max) {
            return Err(format!("crater layer {} m: needs r_min <= r_max <= 0.45", c.cell));
        }
        Ok(())
    }
}

pub struct Procedural {
    def: ProceduralDef,
    radius: f64,
    seed: u32,
    offset: f64,
    relief: Noise3,
    broad: Noise3,
    mare: Noise3,
    crater_cells: Vec<i32>,
}

impl Surface for Procedural {
    fn sample(&self, dir: V3, min_feature: f64) -> Sample {
        Procedural::sample(self, dir, min_feature)
    }
    fn procedural(&self) -> Option<&Procedural> {
        Some(self)
    }
}

impl Procedural {
    /// `world_seed` 0 keeps the definition's own seed; `level` puts height 0 at that direction.
    pub fn new(def: ProceduralDef, radius: f64, world_seed: u32, level: Option<V3>) -> Self {
        let seed = if world_seed == 0 { def.seed } else { hash2i(def.seed, world_seed, 0x5eed) };
        let crater_cells = def.craters.iter().map(|c| 2_i32.pow((cube_arc(2., radius) / c.cell).log2().round().max(0.) as u32)).collect();
        let mut s = Self { def, radius, seed, offset: 0.0, relief: Noise3::new(seed), broad: Noise3::new(seed.wrapping_mul(7).wrapping_add(3)), mare: Noise3::new(seed.wrapping_mul(13).wrapping_add(5)), crater_cells };
        if let Some(d) = level {
            s.offset = -s.sample(d, 0.).height;
        }
        s
    }
    pub fn offset(&self) -> f64 {
        self.offset
    }
    pub fn def(&self) -> &ProceduralDef {
        &self.def
    }
    pub fn radius(&self) -> f64 {
        self.radius
    }
    pub fn seed(&self) -> u32 {
        self.seed
    }
    pub fn crater_cells(&self) -> &[i32] {
        &self.crater_cells
    }
    /// Permutation tables in GPU order: relief, broad, mare.
    pub fn noises(&self) -> [&Noise3; 3] {
        [&self.relief, &self.broad, &self.mare]
    }
    fn depth(&self, radius: f64) -> f64 {
        let k = if radius < self.def.complex_from { 1. } else { (self.def.complex_from / radius).powf(0.85) };
        0.36 * radius * k
    }
    pub fn sample(&self, d: V3, min_feature: f64) -> Sample {
        let def = &self.def;
        let r = self.radius;
        let [px, py, pz] = [d[0] * r, d[1] * r, d[2] * r];
        let f = 1. / def.highland_wavelength;
        let mut h = def.datum + self.offset;
        h += def.highland_amp * (self.broad.noise(px * f, py * f, pz * f) + 0.45 * self.broad.noise(px * f * 2.3 + 11., py * f * 2.3, pz * f * 2.3 - 7.));
        let f = 1. / def.mare_wavelength;
        let m = self.mare.noise(px * f, py * f, pz * f) + 0.5 * self.mare.noise(px * f * 2.1 - 5., py * f * 2.1 + 3., pz * f * 2.1);
        let mare = smoothstep(def.mare_from, def.mare_to, m);
        h -= def.mare_depth * mare;
        let mountains = &def.mountains;
        let f0 = 1. / mountains.wavelength;
        let belt = smoothstep(-0.25, 0.55, self.broad.noise(px * f0 * 0.28 + 31., py * f0 * 0.28 - 13., pz * f0 * 0.28));
        let mut amp = mountains.amp * belt * (1. - 0.85 * mare);
        let mut f = f0;
        for i in 0..mountains.count {
            if i > 0 && 1. / f < min_feature * 2. {
                break;
            }
            let ridge = 1. - self.relief.noise(px * f + 43. + f64::from(i) * 7., py * f - 19., pz * f + 61.).abs();
            h += amp * ridge * ridge * ridge;
            amp *= mountains.gain;
            f *= mountains.lacunarity;
        }
        let relief = &def.relief;
        let mut amp = relief.amp * (1. - def.mare_smooth * mare);
        let mut f = 1. / relief.wavelength;
        for i in 0..relief.count {
            if 1. / f < min_feature * 2. {
                break;
            }
            let i = f64::from(i);
            h += amp * self.relief.noise(px * f + i * 17.3, py * f - i * 9.1, pz * f + i * 3.7);
            amp *= relief.gain;
            f *= relief.lacunarity;
        }
        let mut albedo = def.albedo_high + (def.albedo_mare - def.albedo_high) * mare;
        let fp = std::array::from_fn::<_, 6, _>(|face| params_on(face, d));
        for (li, l) in def.craters.iter().enumerate() {
            if l.cell * l.r_max * 2. < min_feature * 1.5 {
                break;
            }
            if l.depth <= 0. {
                continue;
            }
            let p = l.p * (1. - (1. - l.mare) * mare);
            let band = def.crater_odds_fade * l.p;
            let n = self.crater_cells[li];
            let cell_m = FRAC_PI_2 * r / f64::from(n);
            let margin = 2. / f64::from(n);
            for (face, params) in fp.iter().enumerate() {
                let Some([fa, fb]) = *params else {
                    continue;
                };
                if fa < -1. - margin || fa > 1. + margin || fb < -1. - margin || fb > 1. + margin {
                    continue;
                }
                let basis = &FACES[face];
                let ci = ((fa + 1.) * 0.5 * f64::from(n)).floor() as i32;
                let cj = ((fb + 1.) * 0.5 * f64::from(n)).floor() as i32;
                for j in cj - 1..=cj + 1 {
                    if j < 0 || j >= n {
                        continue;
                    }
                    for i in ci - 1..=ci + 1 {
                        if i < 0 || i >= n {
                            continue;
                        }
                        let mut rnd = Random(hash3i(i as u32, j as u32, (face + li * 8) as u32, self.seed.wrapping_mul(31)));
                        let presence = crater_presence(rnd.next_f64(), p, band);
                        if presence <= 0. {
                            continue;
                        }
                        let a = (((f64::from(i) + rnd.next_f64()) / f64::from(n) * 2. - 1.) * FRAC_PI_4).tan();
                        let b = (((f64::from(j) + rnd.next_f64()) / f64::from(n) * 2. - 1.) * FRAC_PI_4).tan();
                        let s2 = 1. + a * a + b * b;
                        let k2 = r / s2.sqrt();
                        let qx = (basis.n[0] + a * basis.u[0] + b * basis.v[0]) * k2;
                        let qy = (basis.n[1] + a * basis.u[1] + b * basis.v[1]) * k2;
                        let qz = (basis.n[2] + a * basis.u[2] + b * basis.v[2]) * k2;
                        let local = ((1. + a * a) * (1. + b * b).sqrt()).min((1. + b * b) * (1. + a * a).sqrt()) / s2;
                        let u = rnd.next_f64();
                        let radius = (l.r_min + (l.r_max - l.r_min) * u * u) * cell_m * local;
                        let age = rnd.next_f64();
                        let (w1, w2, pk) = (rnd.next_f64(), rnd.next_f64(), rnd.next_f64());
                        let ex = px - qx;
                        let ey = py - qy;
                        let ez = pz - qz;
                        let d = (ex * ex + ey * ey + ez * ez).sqrt();
                        let amp = def.crater_detail * (0.03 + 0.05 * w1) * (1. - 0.75 * smoothstep(1500., 15000., radius));
                        if d / radius > 2.2 * (1. + 1.5 * amp) {
                            continue;
                        }
                        // irregular rim: a smooth function of the direction from the centre
                        let (nx, ny, nz) = if d > 0. { (ex / d, ey / d, ez / d) } else { (0., 0., 0.) };
                        let wob = amp * ((nx * 4.1 + ny * 5.3 - nz * 3.7 + w2 * 6.283).sin() + 0.5 * (-nx * 7.9 + ny * 6.1 + nz * 8.3 + w1 * 6.283).sin());
                        let rr = d / radius * (1. + wob);
                        if rr > 2.2 {
                            continue;
                        }
                        // central peaks only in complex craters: h = 0.0006 D^1.97 km (Hale & Head), at most half the depth
                        let cf = def.complex_from;
                        let peak = if pk < 0.85 && radius > 0.5 * cf {
                            let hp = (0.6 * (radius * 2e-3).powf(1.97)).min(0.5 * self.depth(radius));
                            def.crater_detail * smoothstep(0.5 * cf, cf, radius) * hp * (1. - 0.6 * age) * (-(rr / 0.2) * (rr / 0.2)).exp()
                        } else {
                            0.
                        };
                        h += (crater_profile(rr, radius, age) * self.depth(radius) / (0.36 * radius) + peak) * l.depth * presence;
                        if age < 0.15 {
                            albedo += l.depth * presence * 0.3 * (1. - age / 0.15) * (-(rr - 1.) * (rr - 1.) * 1.6).exp() * (1. - smoothstep(1.6, 2.2, rr));
                        }
                    }
                }
            }
        }
        let broad_fade = 1. - smoothstep(775., 1550., min_feature);
        let detail_fade = 1. - smoothstep(14., 28., min_feature);
        Sample { height: h, albedo: (albedo * (1. + 0.05 * broad_fade * self.relief.noise(px / 3100., py / 3100., pz / 3100.) + 0.025 * detail_fade * self.relief.noise(px / 57., py / 57., pz / 57.))).clamp(0.5, 1.6) }
    }
}
