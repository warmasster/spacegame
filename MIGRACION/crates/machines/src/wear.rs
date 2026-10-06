//! Where a thing comes from and how it wears (the design reports: "un tornillo con biografía").
//! - `Provenance`: maker, model, serial, lot, build date and the fingerprint of its tolerances
//!   (8 traits, standard normal), all from a seed: the same seed gives the same piece.
//! - `Wear`: cycles and hours, and a hidden crack that grows with every load cycle by Paris' law
//!   (m = 3, closed form in a^(-1/2)); when it reaches its critical size the piece fails. When it
//!   will fail can be computed, not checked every tick.
//! - `Rng`: the deterministic generator every piece uses (splitmix64).

/// Deterministic random numbers (splitmix64): the same seed, the same sequence, everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }
    pub fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Standard normal (Box–Muller).
    pub fn normal(&mut self) -> f64 {
        let u = self.f64().max(1e-12);
        let v = self.f64();
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
}

/// A stable hash of text and a seed (ids → seeds).
pub fn hash(text: &str, seed: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    Rng(h).u64()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    pub maker: String,
    pub model: String,
    /// "40-2291": lot and unit.
    pub serial: String,
    pub lot: u32,
    /// Day it was built (days before the world's epoch, negative = in the past).
    pub built: i32,
    /// Plant and shift.
    pub plant: u8,
    pub night_shift: bool,
    /// Tolerances: 8 traits, each standard normal.
    pub fingerprint: [f64; 8],
}

impl Provenance {
    /// A piece of `model` by `maker`, from `seed` (the same seed, the same piece).
    pub fn new(maker: &str, model: &str, seed: u64) -> Provenance {
        let mut r = Rng::new(hash(model, seed));
        let lot = 10 + (r.u64() % 80) as u32;
        let unit = 1000 + r.u64() % 9000;
        let mut fingerprint = [0.0; 8];
        for f in &mut fingerprint {
            *f = r.normal();
        }
        Provenance {
            maker: maker.to_string(),
            model: model.to_string(),
            serial: format!("{lot}-{unit}"),
            lot,
            built: -((r.u64() % 9000) as i32),
            plant: 1 + (r.u64() % 4) as u8,
            night_shift: r.f64() < 0.33,
            fingerprint,
        }
    }

    /// Engine fingerprints of two readings are the same engine when D² < χ²(8; 0.99) (sensor
    /// noise `sigma`).
    pub fn matches(a: &[f64; 8], b: &[f64; 8], sigma: f64) -> bool {
        let d2: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>() / (2.0 * sigma * sigma);
        d2 < 20.09
    }
}

/// Wear of a piece that takes load cycles (hinges, rods, pressure vessels, gear legs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wear {
    pub cycles: f64,
    pub hours: f64,
    /// u = a^(-1/2) of the crack (m^-1/2): it falls by k/2 · s³ per cycle of stress share s.
    pub u: f64,
    /// Paris constant folded with geometry, and the critical crack (m).
    pub k: f64,
    pub critical: f64,
}

impl Wear {
    /// A new piece with an initial flaw `a0` (m) failing at `critical` (m) after `life` cycles
    /// at full stress.
    pub fn new(a0: f64, critical: f64, life: f64) -> Wear {
        let u0 = a0.max(1e-7).powf(-0.5);
        let uc = critical.powf(-0.5);
        Wear { cycles: 0.0, hours: 0.0, u: u0, k: 2.0 * (u0 - uc) / life.max(1.0), critical }
    }

    /// A worse piece (a bad lot, a skipped inspection): `factor` times the initial flaw.
    pub fn flawed(mut self, factor: f64) -> Wear {
        let a = self.crack() * factor;
        self.u = a.powf(-0.5);
        self
    }

    pub fn crack(&self) -> f64 {
        if self.u <= 0.0 { f64::INFINITY } else { self.u.powi(-2) }
    }

    /// One load cycle at a share `s` (0..1+) of the design stress.
    pub fn cycle(&mut self, s: f64) {
        self.cycles += 1.0;
        self.u -= 0.5 * self.k * s.max(0.0).powi(3);
    }

    pub fn failed(&self) -> bool {
        self.crack() >= self.critical
    }

    /// 0 new .. 1 at its critical crack (for gauges and inspections).
    pub fn share(&self) -> f64 {
        (self.crack() / self.critical).clamp(0.0, 1.0)
    }

    /// Cycles left at stress share `s` before it fails (closed form).
    pub fn life_left(&self, s: f64) -> f64 {
        let uc = self.critical.powf(-0.5);
        let per = 0.5 * self.k * s.max(1e-6).powi(3);
        ((self.u - uc) / per).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_40_2291_fails_on_schedule() {
        // the report: a 0.2 mm flaw, failure at 4 mm, K = 0.0236 → cycle 147
        let mut w = Wear { cycles: 0.0, hours: 0.0, u: (0.2f64).powf(-0.5), k: 0.0236, critical: 4.0 };
        let mut n = 0;
        while !w.failed() {
            w.cycle(1.0);
            n += 1;
        }
        assert!((147..=148).contains(&n), "{n}");
        // and the same piece from the same seed
        assert_eq!(Provenance::new("Orell", "K-7", 7), Provenance::new("Orell", "K-7", 7));
        let a = Provenance::new("Orell", "K-7", 7);
        assert!(Provenance::matches(&a.fingerprint, &a.fingerprint, 0.1));
        assert!(!Provenance::matches(&a.fingerprint, &Provenance::new("Orell", "K-7", 8).fingerprint, 0.1));
    }
}
