//! Physical units. Everything inside is SI in `f64`; data may write quantities with their unit
//! (`"45 kN"`, `"6.5 MPa"`, `"600 °C"`, `"2.5 kg/s"`, `"12 kWh"`, `"35 %"`) and every reading is shown
//! in the unit its indicator declares. A unit is its dimensions (powers of m kg s A K mol cd), a
//! scale and an offset (°C): `si = value * scale + offset`.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    /// Powers of m, kg, s, A, K, mol, cd.
    pub dims: [i8; 7],
    pub scale: f64,
    pub offset: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnitError(pub String);

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnitError {}

const M: usize = 0;
const KG: usize = 1;
const S: usize = 2;
const A: usize = 3;
const K: usize = 4;

const fn d(m: i8, kg: i8, s: i8, a: i8, k: i8) -> [i8; 7] {
    [m, kg, s, a, k, 0, 0]
}

impl Unit {
    pub const ONE: Unit = Unit { dims: [0; 7], scale: 1.0, offset: 0.0 };

    pub const fn new(dims: [i8; 7], scale: f64) -> Unit {
        Unit { dims, scale, offset: 0.0 }
    }

    pub fn is_dimensionless(&self) -> bool {
        self.dims == [0; 7]
    }

    /// Same physical quantity (convertible).
    pub fn compatible(&self, o: &Unit) -> bool {
        self.dims == o.dims
    }

    pub fn to_si(&self, v: f64) -> f64 {
        v * self.scale + self.offset
    }

    pub fn from_si(&self, si: f64) -> f64 {
        (si - self.offset) / self.scale
    }

    fn mul(self, o: Unit) -> Unit {
        let mut dims = [0; 7];
        for (k, x) in dims.iter_mut().enumerate() {
            *x = self.dims[k] + o.dims[k];
        }
        Unit { dims, scale: self.scale * o.scale, offset: 0.0 }
    }

    fn powi(self, e: i8) -> Unit {
        let mut dims = self.dims;
        for x in &mut dims {
            *x *= e;
        }
        Unit { dims, scale: self.scale.powi(i32::from(e)), offset: 0.0 }
    }
}

/// Named units (no prefix). Longer names first where one is a prefix of another is not needed:
/// lookup is exact, then prefix + exact.
const NAMED: &[(&str, Unit)] = &[
    // dimensionless
    ("%", Unit::new([0; 7], 0.01)),
    ("ppm", Unit::new([0; 7], 1e-6)),
    ("rad", Unit::new([0; 7], 1.0)),
    ("°", Unit::new([0; 7], std::f64::consts::PI / 180.0)),
    ("deg", Unit::new([0; 7], std::f64::consts::PI / 180.0)),
    ("vueltas", Unit::new([0; 7], std::f64::consts::TAU)),
    // base
    ("m", Unit::new(d(1, 0, 0, 0, 0), 1.0)),
    ("g", Unit::new(d(0, 1, 0, 0, 0), 1e-3)),
    ("t", Unit::new(d(0, 1, 0, 0, 0), 1e3)),
    ("s", Unit::new(d(0, 0, 1, 0, 0), 1.0)),
    ("min", Unit::new(d(0, 0, 1, 0, 0), 60.0)),
    ("h", Unit::new(d(0, 0, 1, 0, 0), 3600.0)),
    ("A", Unit::new(d(0, 0, 0, 1, 0), 1.0)),
    ("K", Unit::new(d(0, 0, 0, 0, 1), 1.0)),
    ("°C", Unit { dims: d(0, 0, 0, 0, 1), scale: 1.0, offset: 273.15 }),
    ("mol", Unit::new([0, 0, 0, 0, 0, 1, 0], 1.0)),
    // derived
    ("N", Unit::new(d(1, 1, -2, 0, 0), 1.0)),
    ("Pa", Unit::new(d(-1, 1, -2, 0, 0), 1.0)),
    ("bar", Unit::new(d(-1, 1, -2, 0, 0), 1e5)),
    ("atm", Unit::new(d(-1, 1, -2, 0, 0), 101_325.0)),
    ("psi", Unit::new(d(-1, 1, -2, 0, 0), 6894.757)),
    ("J", Unit::new(d(2, 1, -2, 0, 0), 1.0)),
    ("Wh", Unit::new(d(2, 1, -2, 0, 0), 3600.0)),
    ("W", Unit::new(d(2, 1, -3, 0, 0), 1.0)),
    ("V", Unit::new(d(2, 1, -3, -1, 0), 1.0)),
    ("Ω", Unit::new(d(2, 1, -3, -2, 0), 1.0)),
    ("ohm", Unit::new(d(2, 1, -3, -2, 0), 1.0)),
    ("C", Unit::new(d(0, 0, 1, 1, 0), 1.0)),
    ("Ah", Unit::new(d(0, 0, 1, 1, 0), 3600.0)),
    ("Hz", Unit::new(d(0, 0, -1, 0, 0), 1.0)),
    ("rpm", Unit::new(d(0, 0, -1, 0, 0), std::f64::consts::TAU / 60.0)),
    ("L", Unit::new(d(3, 0, 0, 0, 0), 1e-3)),
    ("l", Unit::new(d(3, 0, 0, 0, 0), 1e-3)),
];

const PREFIXES: &[(&str, f64)] = &[("G", 1e9), ("M", 1e6), ("k", 1e3), ("h", 1e2), ("c", 1e-2), ("m", 1e-3), ("µ", 1e-6), ("u", 1e-6), ("n", 1e-9)];

fn named(name: &str) -> Option<Unit> {
    NAMED.iter().find(|(n, _)| *n == name).map(|(_, u)| *u)
}

/// One factor: a (prefixed) unit with an optional power (`m2`, `m^2`, `s²`).
fn factor(t: &str) -> Result<Unit, UnitError> {
    let t = t.trim();
    let (base, power) = split_power(t);
    let unit = named(base)
        .or_else(|| {
            PREFIXES.iter().find_map(|(p, f)| {
                let rest = base.strip_prefix(p)?;
                let u = named(rest)?;
                // no prefixes on offset or dimensionless units
                (u.offset == 0.0 && !u.is_dimensionless()).then(|| Unit { scale: u.scale * f, ..u })
            })
        })
        .ok_or_else(|| UnitError(format!("unidad desconocida: '{t}'")))?;
    Ok(if power == 1 { unit } else { unit.powi(power) })
}

fn split_power(t: &str) -> (&str, i8) {
    for (sup, p) in [("²", 2), ("³", 3)] {
        if let Some(b) = t.strip_suffix(sup) {
            return (b, p);
        }
    }
    if let Some((b, p)) = t.split_once('^') {
        return (b, p.trim().parse().unwrap_or(1));
    }
    // a trailing digit is a power (m2, s2), but not a whole number
    let digits = t.len() - t.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-').len();
    if digits > 0 && digits < t.len() {
        let (b, p) = t.split_at(t.len() - digits);
        if let Ok(p) = p.parse() {
            return (b, p);
        }
    }
    (t, 1)
}

/// A unit expression: factors joined by `*`, `·` or spaces, an optional `/` with the denominator
/// (`kg/s`, `kJ/K`, `W/m2·K`, `L/min`).
pub fn unit(s: &str) -> Result<Unit, UnitError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Unit::ONE);
    }
    if let Some(u) = named(s) {
        return Ok(u);
    }
    let (num, den) = match s.split_once('/') {
        Some((n, d)) => (n, Some(d)),
        None => (s, None),
    };
    let product = |part: &str| -> Result<Unit, UnitError> {
        let mut u = Unit::ONE;
        for f in part.split(['*', '·', ' ']).filter(|f| !f.trim().is_empty()) {
            u = u.mul(factor(f)?);
        }
        Ok(u)
    };
    let mut u = if num.trim() == "1" { Unit::ONE } else { product(num)? };
    if let Some(d) = den {
        u = u.mul(product(d)?.powi(-1));
    }
    Ok(u)
}

/// `"45 kN"` → (45000, the unit as written). A bare number is dimensionless.
pub fn parse(q: &str) -> Result<(f64, Unit), UnitError> {
    let q = q.trim();
    let end = q
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || ((c == 'e' || c == 'E') && i > 0 && q[i + 1..].starts_with(|n: char| n.is_ascii_digit() || n == '-'))))
        .map_or(q.len(), |(i, _)| i);
    let (num, rest) = q.split_at(end);
    let v: f64 = num.parse().map_err(|_| UnitError(format!("cantidad sin número: '{q}'")))?;
    let u = unit(rest)?;
    Ok((u.to_si(v), u))
}

/// SI value of a quantity, which must have the dimensions of `expect` (a unit string) when given.
pub fn si(q: &str, expect: Option<&str>) -> Result<f64, UnitError> {
    let (v, u) = parse(q)?;
    if let Some(e) = expect {
        let want = unit(e)?;
        if !u.is_dimensionless() && !u.compatible(&want) {
            return Err(UnitError(format!("'{q}' no es {e}")));
        }
    }
    Ok(v)
}

/// Dimension names, for messages.
pub fn describe(u: &Unit) -> String {
    const NAMES: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];
    let mut out = String::new();
    for (k, &p) in u.dims.iter().enumerate() {
        if p != 0 {
            if !out.is_empty() {
                out.push('·');
            }
            out.push_str(NAMES[k]);
            if p != 1 {
                out.push_str(&p.to_string());
            }
        }
    }
    if out.is_empty() { "1".into() } else { out }
}

/// Decimals that show a step of `resolution` (display units).
pub fn decimals_for(resolution: f64) -> usize {
    if resolution <= 0.0 || !resolution.is_finite() {
        return 2;
    }
    (-resolution.log10() - 1e-6).ceil().clamp(0.0, 6.0) as usize
}

/// A value in SI shown in `unit` with `decimals`, Spanish style (decimal comma): "62,4 %".
pub fn format(si: f64, unit_name: &str, u: &Unit, decimals: usize, out: &mut String) {
    use std::fmt::Write;
    let v = u.from_si(si);
    let start = out.len();
    let _ = write!(out, "{v:.decimals$}");
    // decimal comma
    if let Some(p) = out[start..].find('.') {
        out.replace_range(start + p..start + p + 1, ",");
    }
    if !unit_name.is_empty() {
        out.push(' ');
        out.push_str(unit_name);
    }
}

// keep the base indices used (documentation of the order)
const _: [usize; 5] = [M, KG, S, A, K];

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn quantities_in_si() {
        assert!(close(parse("45 kN").unwrap().0, 45_000.0));
        assert!(close(parse("6.5 MPa").unwrap().0, 6.5e6));
        assert!(close(parse("600 °C").unwrap().0, 873.15));
        assert!(close(parse("2.5 kg/s").unwrap().0, 2.5));
        assert!(close(parse("12 kWh").unwrap().0, 12.0 * 3.6e6));
        assert!(close(parse("35 %").unwrap().0, 0.35));
        assert!(close(parse("3000 rpm").unwrap().0, 3000.0 * std::f64::consts::TAU / 60.0));
        assert!(close(parse("0.78 m2").unwrap().0, 0.78));
        assert!(close(parse("180 kJ/K").unwrap().0, 180_000.0));
        assert!(close(parse("40 L/min").unwrap().0, 40e-3 / 60.0));
        assert!(close(parse("1e-3 m").unwrap().0, 1e-3));
        assert!(close(parse("12 mm").unwrap().0, 0.012));
        assert!(close(parse("28 V").unwrap().0, 28.0));
        assert!(close(parse("-20 °C").unwrap().0, 253.15));
    }

    #[test]
    fn dimensions_are_checked() {
        assert!(si("45 kN", Some("N")).is_ok());
        assert!(si("45 kW", Some("N")).is_err());
        assert!(si("7", Some("N")).is_ok());
        assert_eq!(unit("kg/s").unwrap().dims, [0, 1, -1, 0, 0, 0, 0]);
        assert_eq!(unit("W/m2·K").unwrap().dims, [0, 1, -3, 0, -1, 0, 0]);
    }

    #[test]
    fn shown_in_their_unit() {
        let mut s = String::new();
        format(0.624, "%", &unit("%").unwrap(), 1, &mut s);
        assert_eq!(s, "62,4 %");
        s.clear();
        format(693.15, "°C", &unit("°C").unwrap(), 0, &mut s);
        assert_eq!(s, "420 °C");
        assert_eq!(decimals_for(0.01), 2);
        assert_eq!(decimals_for(5.0), 0);
    }
}
