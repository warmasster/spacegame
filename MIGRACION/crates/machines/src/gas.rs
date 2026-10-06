//! Gas, as the machines and whoever owns the air agree on it: an ideal gas by moles of O₂, N₂ and
//! CO₂, the compressible flow through an orifice (γ = 1.4: choked under a pressure ratio of
//! 0.528), and a vessel that keeps a mix under pressure. One definition for the compartments of a
//! ship, its valves, its compressors and its tanks.

/// The gas constant (J/mol·K).
pub const R: f64 = 8.314;
/// Discharge coefficient of a hole.
pub const CD: f64 = 0.65;
/// Molar masses (kg/mol): O₂, N₂, CO₂, and cabin air.
pub const M_O2: f64 = 0.032;
pub const M_N2: f64 = 0.028;
pub const M_CO2: f64 = 0.044;
pub const M_AIR: f64 = 0.029;

/// Flow function ψ of the pressure ratio (γ = 1.4: choked below r* = 0.528).
pub fn psi(r: f64) -> f64 {
    if r <= 0.5283 { 0.6847 } else { (7.0 * (r.powf(1.4286) - r.powf(1.7143))).max(0.0).sqrt() }
}

/// Mass flow (kg/s) through an orifice of `area` m² from upstream `pu` Pa, `tu` K, molar mass
/// `mu` into `pd` Pa.
pub fn orifice_mdot(area: f64, pu: f64, tu: f64, mu: f64, pd: f64) -> f64 {
    if pu <= 0.0 || area <= 0.0 {
        return 0.0;
    }
    CD * area * pu * (mu / (R * tu)).sqrt() * psi((pd / pu).max(0.0))
}

/// A mix of gases, by moles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mix {
    pub o2: f64,
    pub n2: f64,
    pub co2: f64,
}

impl Mix {
    pub fn moles(&self) -> f64 {
        self.o2 + self.n2 + self.co2
    }
    pub fn mass(&self) -> f64 {
        self.o2 * M_O2 + self.n2 * M_N2 + self.co2 * M_CO2
    }
    /// A share `k` (0..1) of it taken out.
    pub fn take(&mut self, k: f64) -> Mix {
        let k = k.clamp(0.0, 1.0);
        let out = Mix { o2: self.o2 * k, n2: self.n2 * k, co2: self.co2 * k };
        self.o2 -= out.o2;
        self.n2 -= out.n2;
        self.co2 -= out.co2;
        out
    }
    pub fn give(&mut self, b: &Mix) {
        self.o2 += b.o2;
        self.n2 += b.n2;
        self.co2 += b.co2;
    }
}

/// A vessel of gas kept by composition (a tank of recovered air): what it holds, its volume
/// (m³) and the temperature it is kept at (K: its walls take the heat of filling and give it back
/// on emptying). Its owner fills it and draws from it; `leak` is the share of what it holds that
/// gets out of it each second, into wherever it stands (a holed vessel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vessel {
    pub gas: Mix,
    pub volume: f64,
    pub t: f64,
    pub leak: f64,
}

impl Vessel {
    pub fn pressure(&self) -> f64 {
        self.gas.moles() * R * self.t / self.volume.max(1e-6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mix_keeps_its_moles_and_its_shares() {
        let mut a = Mix { o2: 30.0, n2: 70.0, co2: 1.0 };
        let b = a.take(0.25);
        assert!((a.moles() + b.moles() - 101.0).abs() < 1e-12);
        assert!((b.o2 / b.moles() - 30.0 / 101.0).abs() < 1e-12, "what leaves is what there was");
        a.give(&b);
        assert!((a.o2 - 30.0).abs() < 1e-12 && (a.co2 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn an_orifice_chokes() {
        let full = orifice_mdot(1e-4, 8e6, 294.0, M_AIR, 0.0);
        assert!((orifice_mdot(1e-4, 8e6, 294.0, M_AIR, 3e6) - full).abs() < 1e-12, "the same under the critical ratio");
        assert!(orifice_mdot(1e-4, 8e6, 294.0, M_AIR, 7.5e6) < full * 0.6);
        assert_eq!(orifice_mdot(1e-4, 8e6, 294.0, M_AIR, 8e6), 0.0);
    }
}
