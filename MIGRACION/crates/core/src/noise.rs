//! Same integer overflow, permutation and evaluation order as shared/noise.ts.
pub fn hash2i(x: u32, y: u32, salt: u32) -> u32 {
    avalanche(x.wrapping_mul(0x27d4eb2d) ^ y.wrapping_mul(0x165667b1) ^ salt.wrapping_mul(0x9e3779b9))
}

pub fn hash3i(x: u32, y: u32, z: u32, salt: u32) -> u32 {
    avalanche(x.wrapping_mul(0x27d4eb2d) ^ y.wrapping_mul(0x165667b1) ^ z.wrapping_mul(0x1b873593) ^ salt.wrapping_mul(0x9e3779b9))
}

fn avalanche(mut h: u32) -> u32 {
    h = (h ^ (h >> 15)).wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2ae35);
    h ^ (h >> 16)
}

pub struct Random(pub u32);

impl Random {
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4294967296.0
    }
}

pub struct Noise3 {
    perm: [usize; 512],
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn grad(h: usize, x: f64, y: f64, z: f64) -> f64 {
    let g = h & 15;
    let u = if g < 8 { x } else { y };
    let v = if g < 4 {
        y
    } else if g == 12 || g == 14 {
        x
    } else {
        z
    };
    (if g & 1 == 0 { u } else { -u }) + (if g & 2 == 0 { v } else { -v })
}

impl Noise3 {
    pub fn new(seed: u32) -> Self {
        let mut rng = Random(seed);
        let mut p = std::array::from_fn::<_, 256, _>(|i| i);
        for i in (1..256).rev() {
            let j = (rng.next_f64() * (i + 1) as f64).floor() as usize;
            p.swap(i, j);
        }
        Self { perm: std::array::from_fn(|i| p[i & 255]) }
    }

    /// The 512-entry permutation, for GPU upload.
    pub fn perm(&self) -> &[usize; 512] {
        &self.perm
    }

    pub fn noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let p = &self.perm;
        let fx = x.floor();
        let fy = y.floor();
        let fz = z.floor();
        let ix = (fx as i64 & 255) as usize;
        let iy = (fy as i64 & 255) as usize;
        let iz = (fz as i64 & 255) as usize;
        let x = x - fx;
        let y = y - fy;
        let z = z - fz;
        let u = fade(x);
        let v = fade(y);
        let w = fade(z);
        let a = p[ix] + iy;
        let aa = p[a] + iz;
        let ab = p[a + 1] + iz;
        let b = p[ix + 1] + iy;
        let ba = p[b] + iz;
        let bb = p[b + 1] + iz;
        lerp(
            lerp(lerp(grad(p[aa], x, y, z), grad(p[ba], x - 1.0, y, z), u), lerp(grad(p[ab], x, y - 1.0, z), grad(p[bb], x - 1.0, y - 1.0, z), u), v),
            lerp(lerp(grad(p[aa + 1], x, y, z - 1.0), grad(p[ba + 1], x - 1.0, y, z - 1.0), u), lerp(grad(p[ab + 1], x, y - 1.0, z - 1.0), grad(p[bb + 1], x - 1.0, y - 1.0, z - 1.0), u), v),
            w,
        )
    }
}

pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn smin(a: f64, b: f64, k: f64) -> f64 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}
pub fn crater_profile(r: f64, radius: f64, age: f64) -> f64 {
    let floor = -0.42 + 0.18 * age;
    let width = 0.55 + 0.3 * age;
    let k = 0.16 + 0.45 * age;
    let rim_x = (r - 1.0 - width).min(0.0);
    let mut shape = smin(r * r - 1.0, 0.42 * rim_x * rim_x, k);
    shape = -smin(-shape, -floor, k.min(-floor * 0.9));
    let blanket = if r > 1.0 { (-(r - 1.0) * 2.2).exp() * (1.0 - smoothstep(1.6, 2.2, r)) } else { 1.0 };
    shape += 0.03 * blanket * (1.0 - age);
    shape * radius * 0.85 * (1.0 - 0.72 * age)
}
