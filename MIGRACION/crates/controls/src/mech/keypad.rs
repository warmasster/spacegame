//! A keypad with its own seven-segment line: digits and a point build an entry, CLR clears it,
//! ENTER writes it (in the declared unit) to its signal. Electronic: dead without power.
//! Sub-elements (`Press { elem }`), in reading order: 1 2 3 / 4 5 6 / 7 8 9 / . 0 CLR / ENTER.
//! State: x committed value (SI), y entry, t decimals typed (−1: no point yet), v digits typed.
//! Pose e0: key held (index + 1, 0 none).
use super::{Mechanism, gate_block};
use crate::{
    def::ControlDef,
    intent::{Blocked, ControlState, Event, F_PRESSED, Gate, Intent, Outcome, Pose},
};
use lunar_signals::units::{self, Unit};

pub const KEYS: [char; 13] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '0', 'C', 'E'];

pub struct Keypad {
    unit: Unit,
    unit_name: String,
    /// Most digits it takes.
    digits: u32,
    default: f64,
}

impl Keypad {
    pub fn new(d: &ControlDef) -> Result<Keypad, String> {
        let unit_name = d.unidad.clone().unwrap_or_default();
        let unit = units::unit(&unit_name).map_err(|e| format!("{}: {}", d.id, e.0))?;
        let digits = d.mascara.as_ref().map_or(6, |m| m.chars().filter(|c| *c == '#').count() as u32).max(1);
        let default = match &d.defecto {
            Some(q) => q.si().map_err(|e| e.0)?,
            None => 0.0,
        };
        Ok(Keypad { unit, unit_name, digits, default })
    }

    fn key(&self, st: &mut ControlState, c: char) -> Outcome {
        match c {
            '0'..='9' => {
                if st.v as u32 >= self.digits {
                    return Outcome::blocked(Blocked::Stop);
                }
                let d = f64::from(c as u8 - b'0');
                if st.t < 0.0 {
                    st.y = st.y * 10.0 + d;
                } else {
                    st.t += 1.0;
                    st.y += d * 10f64.powi(-(st.t as i32));
                }
                st.v += 1.0;
                Outcome { changed: false, event: Some(Event::Click) }
            }
            '.' | ',' => {
                if st.t < 0.0 {
                    st.t = 0.0;
                }
                Outcome { changed: false, event: Some(Event::Click) }
            }
            'C' | 'c' => {
                st.y = 0.0;
                st.t = -1.0;
                st.v = 0.0;
                Outcome { changed: false, event: Some(Event::Click) }
            }
            'E' | '\n' | '\r' => {
                let before = st.x;
                if st.v > 0.0 {
                    st.x = self.unit.to_si(st.y);
                }
                st.y = 0.0;
                st.t = -1.0;
                st.v = 0.0;
                Outcome::click(st.x != before)
            }
            _ => Outcome::default(),
        }
    }
}

impl Mechanism for Keypad {
    fn kind(&self) -> &'static str {
        "teclado"
    }

    fn init(&self) -> ControlState {
        ControlState { x: self.default, t: -1.0, ..Default::default() }
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        if let Some(b) = gate_block(g, true)
            && !matches!(i, Intent::Release | Intent::Hover(_) | Intent::Set { .. })
        {
            return Outcome::blocked(b);
        }
        match *i {
            Intent::Press { elem } => {
                st.set(F_PRESSED, true);
                let k = KEYS.get(usize::from(elem)).copied().unwrap_or('E');
                st.flags = (st.flags & 0xffff) | ((u32::from(elem) + 1) << 16);
                self.key(st, k)
            }
            Intent::Type(c) => self.key(st, c),
            Intent::Release => {
                st.set(F_PRESSED, false);
                st.flags &= 0xffff;
                Outcome::default()
            }
            Intent::Set { value } => Outcome::click(self.set(st, value)),
            _ => Outcome::default(),
        }
    }

    fn advance(&self, _: &mut ControlState, _: f32, _: f32) -> Outcome {
        Outcome::default()
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = (st.flags >> 16) as f32;
    }

    fn text(&self, st: &ControlState, out: &mut String) {
        use std::fmt::Write;
        if st.v > 0.0 || st.t >= 0.0 {
            let dec = st.t.max(0.0) as usize;
            let _ = write!(out, "{:.*}", dec, st.y);
            if st.t == 0.0 {
                out.push('.');
            }
        } else {
            units::format(st.x, "", &self.unit, 1, out);
        }
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        out.push_str("Valor ");
        units::format(st.x, &self.unit_name, &self.unit, 2, out);
        if st.v > 0.0 {
            out.push_str("  · entrada ");
            self.text(st, out);
        }
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let changed = st.x != value;
        st.x = value;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_entry_is_written_on_enter() {
        let d: ControlDef = serde_json::from_str("{ \"id\": \"k\", \"kind\": \"teclado\", \"unidad\": \"kPa\", \"mascara\": \"###.#\" }").unwrap();
        let k = Keypad::new(&d).unwrap();
        let mut st = k.init();
        let g = Gate::default();
        for c in ['7', '0', '.', '5'] {
            k.intent(&mut st, &Intent::Type(c), &g);
        }
        assert_eq!(k.value(&st), 0.0);
        assert!(k.intent(&mut st, &Intent::Type('E'), &g).changed);
        assert!((k.value(&st) - 70_500.0).abs() < 1e-6);
        let dead = Gate { supply: 0.0, ..Gate::default() };
        assert_eq!(k.intent(&mut st, &Intent::Type('1'), &dead).event, Some(Event::Blocked(Blocked::NoPower)));
    }
}
