//! A quantity in the data: a bare number (already SI) or a string with its unit (`"45 kN"`).
use crate::units::{self, UnitError};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum Q {
    N(f64),
    S(String),
}

impl Default for Q {
    fn default() -> Self {
        Q::N(0.0)
    }
}

impl Q {
    /// SI value.
    pub fn si(&self) -> Result<f64, UnitError> {
        match self {
            Q::N(v) => Ok(*v),
            Q::S(s) => units::parse(s).map(|(v, _)| v),
        }
    }

    /// SI value, checked against the dimensions of `unit` when the data wrote a unit.
    pub fn si_as(&self, unit: &str) -> Result<f64, UnitError> {
        match self {
            Q::N(v) => Ok(*v),
            Q::S(s) => units::si(s, Some(unit)),
        }
    }

    /// The unit the data wrote (empty for a bare number).
    pub fn unit_name(&self) -> &str {
        match self {
            Q::N(_) => "",
            Q::S(s) => {
                let t = s.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'));
                t.trim()
            }
        }
    }
}

impl From<f64> for Q {
    fn from(v: f64) -> Q {
        Q::N(v)
    }
}
