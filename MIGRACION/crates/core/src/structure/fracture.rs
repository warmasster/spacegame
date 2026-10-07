//! Fracture: what a part breaks into. By default ("particulas") a wrecked part is simply gone: what
//! you see is its material's break effect, a burst of generic particles, with nothing cut, no new
//! meshes and no new rigid bodies. "planos" cuts it into convex pieces with their own shape and
//! mass, and chips parts that hold (costly: see docs/OPTIMIZACION.md). Models are chosen per
//! material by name (`MaterialDef::fracture`), so a new way of breaking is a new `Fracture` and one
//! line in `model`.
use super::{
    catalog::MaterialDef,
    convex::{Convex, Plane},
};
use glam::Vec3;

/// Small deterministic generator (xorshift64*): the same hit breaks the same way.
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    /// A new seed from this one: the dice of what follows from it (the same wherever these
    /// dice were rolled the same).
    pub fn seed(&mut self) -> u64 {
        u64::from(self.f32().to_bits()) << 32 | u64::from(self.f32().to_bits())
    }

    pub fn f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform on the unit sphere.
    pub fn dir(&mut self) -> Vec3 {
        let z = self.f32() * 2.0 - 1.0;
        let a = self.f32() * std::f32::consts::TAU;
        let s = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(s * a.cos(), s * a.sin(), z)
    }
}

pub trait Fracture: Send + Sync {
    /// Pieces of a wrecked part (part frame): hit at `at`, `violence` = damage over hit points.
    fn shatter(&self, shape: &Convex, m: &MaterialDef, at: Vec3, violence: f32, rng: &mut Rng) -> Vec<Convex>;
    /// A part that holds may lose a chip near `at`: (what stays, the chip). `share`: this hit's
    /// damage over the part's hit points.
    fn chip(&self, shape: &Convex, m: &MaterialDef, at: Vec3, dir: Vec3, share: f32, rng: &mut Rng) -> Option<(Convex, Convex)>;
}

/// Nothing to cut: the part vanishes into its material's break effect (particles), and parts that
/// hold keep their shape (the damage shows as cracks in the shader).
pub struct ParticleFracture;

impl Fracture for ParticleFracture {
    fn shatter(&self, _: &Convex, _: &MaterialDef, _: Vec3, _: f32, _: &mut Rng) -> Vec<Convex> {
        Vec::new()
    }

    fn chip(&self, _: &Convex, _: &MaterialDef, _: Vec3, _: Vec3, _: f32, _: &mut Rng) -> Option<(Convex, Convex)> {
        None
    }
}

/// Cuts by planes: brittle materials into many shards, smaller near the hit; ductile ones tear
/// across their length into a few big pieces.
pub struct PlaneFracture {
    pub max_pieces: usize,
    /// Pieces smaller than this (m³) turn to dust.
    pub min_volume: f32,
}

impl Default for PlaneFracture {
    fn default() -> Self {
        PlaneFracture { max_pieces: 14, min_volume: 4e-4 }
    }
}

/// Longest extent of a shape: (direction, length).
fn long_axis(shape: &Convex) -> (Vec3, f32) {
    let mut best = (Vec3::X, 0.0);
    for d in [Vec3::X, Vec3::Y, Vec3::Z] {
        let len = shape.support(d) + shape.support(-d);
        if len > best.1 {
            best = (d, len);
        }
    }
    best
}

impl Fracture for PlaneFracture {
    fn shatter(&self, shape: &Convex, m: &MaterialDef, at: Vec3, violence: f32, rng: &mut Rng) -> Vec<Convex> {
        let want = (2.0 + m.brittleness * 9.0 * violence.clamp(0.3, 2.0)).round() as usize;
        let want = want.clamp(2, self.max_pieces);
        let mut pieces = vec![shape.clone()];
        let mut tries = 0;
        while pieces.len() < want && tries < want * 4 {
            tries += 1;
            // cut the biggest piece, through a point pulled toward the hit
            let k = (0..pieces.len()).max_by(|&a, &b| pieces[a].volume().total_cmp(&pieces[b].volume())).unwrap_or(0);
            let (vol, c) = pieces[k].mass_props();
            let size = vol.cbrt();
            let toward = c + (at - c) * (0.2 + 0.4 * rng.f32() * m.brittleness);
            let p = toward + rng.dir() * size * 0.25;
            // ductile metal tears across its length; brittle stuff any way
            let (axis, _) = long_axis(&pieces[k]);
            let n = (rng.dir() * m.brittleness + axis * (1.0 - m.brittleness) * 1.5).normalize_or(axis);
            if let (Some(a), Some(b)) = pieces[k].split(Plane::through(p, n)) {
                pieces.swap_remove(k);
                pieces.push(a);
                pieces.push(b);
            }
        }
        pieces.retain(|p| p.volume() >= self.min_volume);
        pieces
    }

    fn chip(&self, shape: &Convex, m: &MaterialDef, at: Vec3, dir: Vec3, share: f32, rng: &mut Rng) -> Option<(Convex, Convex)> {
        let odds = (m.brittleness * share * 6.0).min(0.9);
        if share < 0.03 || rng.f32() > odds {
            return None;
        }
        let (vol, c) = shape.mass_props();
        // a cut facing the blow, a little way in from where it landed
        let out = ((at - c).normalize_or(-dir) - dir * 0.5).normalize_or(Vec3::Y);
        let n = (out + rng.dir() * 0.35).normalize();
        let depth = vol.cbrt() * (0.08 + 0.3 * (share * m.brittleness).min(1.0));
        let point = c + out * (shape.support(out) - c.dot(out) - depth).max(0.0);
        let (keep, chip) = shape.split(Plane::through(point, n));
        let (keep, chip) = (keep?, chip?);
        (chip.volume() < vol * 0.4 && chip.volume() >= self.min_volume).then_some((keep, chip))
    }
}

/// The fracture model a material names (default "particulas").
pub fn model(name: Option<&str>) -> Result<Box<dyn Fracture>, String> {
    match name.unwrap_or("particulas") {
        "particulas" => Ok(Box::new(ParticleFracture)),
        "planos" => Ok(Box::new(PlaneFracture::default())),
        other => Err(format!("unknown fracture model '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mat(brittleness: f32) -> MaterialDef {
        MaterialDef { name: "m".into(), density: 1000.0, toughness: 1e6, brittleness, color: [1; 3], rough: 1, metal: 0, glass: false, acabado: None, finish: 0, fracture: None, breaks: None }
    }

    #[test]
    fn brittle_things_shatter_into_more_pieces_and_keep_their_volume() {
        let b = Convex::cuboid(Vec3::new(1.0, 0.5, 0.05));
        let f = PlaneFracture { min_volume: 0.0, ..Default::default() };
        let glass = f.shatter(&b, &mat(0.95), Vec3::new(0.8, 0.0, 0.0), 1.0, &mut Rng::new(1));
        let steel = f.shatter(&b, &mat(0.1), Vec3::new(0.8, 0.0, 0.0), 1.0, &mut Rng::new(1));
        assert!(glass.len() > steel.len() + 4, "glass {} steel {}", glass.len(), steel.len());
        for pieces in [&glass, &steel] {
            let v: f32 = pieces.iter().map(Convex::volume).sum();
            assert!((v - b.volume()).abs() < 1e-3 * b.volume().max(1.0), "{v}");
        }
    }

    #[test]
    fn chips_come_off_where_it_was_hit() {
        let b = Convex::cuboid(Vec3::ONE);
        let f = PlaneFracture::default();
        let at = Vec3::new(1.0, 0.3, 0.0);
        let mut got = None;
        for seed in 0..20 {
            if let Some(c) = f.chip(&b, &mat(0.9), at, Vec3::NEG_X, 0.4, &mut Rng::new(seed)) {
                got = Some(c);
                break;
            }
        }
        let (keep, chip) = got.expect("a brittle block chips");
        assert!((keep.volume() + chip.volume() - 8.0).abs() < 1e-3);
        assert!(chip.mass_props().1.x > 0.5, "the chip comes from the struck side");
        assert!(f.chip(&b, &mat(0.9), at, Vec3::NEG_X, 0.01, &mut Rng::new(3)).is_none());
    }
}
