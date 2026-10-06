//! Levers: throttles, sticks, mixture levers. One or two axes of continuous travel, named detents
//! (idle, MIL...), a gate that must be lifted to pass (Shift while dragging, or a hold), friction
//! that keeps it where it is left or a spring back to the centre. Pose: e0 axis 1 angle, e1 axis 2
//! angle (rad).
use super::{Mechanism, gate_block};
use crate::{
    def::ControlDef,
    intent::{Blocked, ControlState, Event, F_AXIS_X, F_AXIS_Y, F_PRESSED, F_PULLED, Gate, Intent, Outcome, Pose},
};
use lunar_signals::units::{self, Unit};

pub struct Lever {
    lo: f64,
    hi: f64,
    axes: u8,
    detents: Vec<(f64, String)>,
    /// A gate at this value: crossing it needs a lift.
    gate: Option<f64>,
    /// Return to the centre at this rate (range per second); 0: stays (friction).
    spring: f64,
    /// Throw of the handle over the range (rad).
    sweep: f32,
    /// Range per screen of drag.
    drag_gain: f64,
    unit: Unit,
    unit_name: String,
    name: String,
    default: f64,
}

/// Width of a detent's catch, as a share of the range.
const CATCH: f64 = 0.015;

impl Lever {
    pub fn new(d: &ControlDef) -> Result<Lever, String> {
        let [lo, hi] = d.rango.clone().unwrap_or([0.0.into(), 1.0.into()]);
        let (lo, hi) = (lo.si().map_err(|e| e.0)?, hi.si().map_err(|e| e.0)?);
        if hi <= lo {
            return Err(format!("{}: rango vacío", d.id));
        }
        let axes = d.ejes.unwrap_or(1);
        if !(1..=2).contains(&axes) {
            return Err(format!("{}: una palanca tiene 1 o 2 ejes", d.id));
        }
        let mut detents = Vec::new();
        for r in &d.retenes {
            detents.push((r.en.si().map_err(|e| e.0)?, r.nombre.clone().unwrap_or_default()));
        }
        let gate = match &d.compuerta {
            Some(g) => Some(g.en.si().map_err(|e| e.0)?),
            None => None,
        };
        let spring = if d.muelle_al_centro { (hi - lo) * 2.5 } else { 0.0 };
        let unit_name = d.unidad.clone().unwrap_or_default();
        let unit = units::unit(&unit_name).map_err(|e| format!("{}: {}", d.id, e.0))?;
        let default = match &d.defecto {
            Some(q) => q.si().map_err(|e| e.0)?,
            None if d.muelle_al_centro => (lo + hi) * 0.5,
            None => lo,
        };
        let sweep = d.recorrido.unwrap_or(if axes == 2 { 40.0 } else { 70.0 }).to_radians() as f32;
        let _ = d.friccion;
        Ok(Lever { lo, hi, axes, detents, gate, spring, sweep, drag_gain: (hi - lo) * 1.6, unit, unit_name, name: d.label().to_string(), default })
    }

    pub fn range(&self) -> (f64, f64) {
        (self.lo, self.hi)
    }

    pub fn axes(&self) -> u8 {
        self.axes
    }

    pub fn detents(&self) -> &[(f64, String)] {
        &self.detents
    }

    /// Move axis 1 toward `to`, stopping at the gate unless lifted; detents catch it.
    fn move_to(&self, st: &mut ControlState, to: f64, lifted: bool, fine: bool) -> Outcome {
        let from = st.x;
        let mut x = to.clamp(self.lo, self.hi);
        let mut ev = None;
        if let Some(g) = self.gate
            && !lifted
            && ((from < g - 1e-9 && x > g) || (from > g + 1e-9 && x < g))
        {
            x = g;
            ev = Some(Event::Blocked(Blocked::Gate));
        }
        if !fine {
            let catch = CATCH * (self.hi - self.lo);
            for (at, _) in &self.detents {
                if (x - at).abs() < catch {
                    if (from - at).abs() >= catch {
                        ev = ev.or(Some(Event::Click));
                    }
                    x = *at;
                }
            }
        }
        if (x == self.lo || x == self.hi) && from != x && ev.is_none() {
            ev = Some(Event::Stop);
        }
        st.x = x;
        Outcome { changed: x != from, event: ev }
    }

    fn norm(&self, x: f64) -> f32 {
        ((x - self.lo) / (self.hi - self.lo)) as f32 - 0.5
    }
}

impl Mechanism for Lever {
    fn kind(&self) -> &'static str {
        "palanca"
    }

    fn init(&self) -> ControlState {
        let c = if self.spring > 0.0 { (self.lo + self.hi) * 0.5 } else { self.default };
        ControlState { x: self.default, y: c, ..Default::default() }
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        if let Some(b) = gate_block(g, false)
            && !matches!(i, Intent::Release | Intent::Hover(_) | Intent::Set { .. } | Intent::Axis { .. })
        {
            return Outcome::blocked(b);
        }
        match *i {
            Intent::Press { .. } => {
                st.set(F_PRESSED, true);
                Outcome::default()
            }
            Intent::Hold { secs } => {
                if secs >= 0.35 {
                    st.set(F_PULLED, true);
                }
                Outcome::default()
            }
            Intent::Release => {
                st.set(F_PRESSED, false);
                st.set(F_PULLED, false);
                Outcome::default()
            }
            Intent::Drag { dx, dy, m } => {
                let gain = self.drag_gain * if m.fine { 0.2 } else { 1.0 };
                let lifted = m.coarse || st.has(F_PULLED);
                let mut o = self.move_to(st, st.x + f64::from(dy) * gain, lifted, m.fine);
                if self.axes == 2 {
                    let y = (st.y + f64::from(dx) * gain).clamp(self.lo, self.hi);
                    o.changed |= y != st.y;
                    st.y = y;
                }
                o
            }
            Intent::Turn { notches, m, .. } => {
                let step = (self.hi - self.lo) * if m.fine { 0.002 } else if m.coarse { 0.05 } else { 0.01 };
                self.move_to(st, st.x + f64::from(notches) * step, m.coarse || st.has(F_PULLED), m.fine)
            }
            Intent::Set { value } => self.move_to(st, value, true, true),
            Intent::Axis { axis, value } => {
                // a key holds a sprung lever where it puts it for as long as it is down; let go
                // (back to the centre), the spring takes it there
                if self.spring > 0.0 {
                    let flag = if axis == 0 { F_AXIS_X } else { F_AXIS_Y };
                    let held = value != (self.lo + self.hi) * 0.5;
                    st.set(flag, held);
                    if !held {
                        return Outcome::default();
                    }
                }
                if axis == 0 {
                    self.move_to(st, value, true, true)
                } else {
                    let y = value.clamp(self.lo, self.hi);
                    let changed = y != st.y;
                    st.y = y;
                    Outcome { changed, event: None }
                }
            }
            _ => Outcome::default(),
        }
    }

    fn advance(&self, st: &mut ControlState, dt: f32, _: f32) -> Outcome {
        if self.spring <= 0.0 || st.has(F_PRESSED) {
            return Outcome::default();
        }
        let c = (self.lo + self.hi) * 0.5;
        let k = self.spring * f64::from(dt);
        let (x0, y0) = (st.x, st.y);
        if !st.has(F_AXIS_X) {
            st.x = if (st.x - c).abs() <= k { c } else { st.x - k * (st.x - c).signum() };
        }
        if !st.has(F_AXIS_Y) {
            st.y = if (st.y - c).abs() <= k { c } else { st.y - k * (st.y - c).signum() };
        }
        Outcome { changed: st.x != x0 || st.y != y0, event: None }
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn value2(&self, st: &ControlState) -> Option<f64> {
        (self.axes == 2).then_some(st.y)
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = self.norm(st.x) * self.sweep;
        out.e[1] = if self.axes == 2 { self.norm(st.y) * self.sweep } else { 0.0 };
        out.e[2] = if st.has(F_PULLED) { 0.006 } else { 0.0 };
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        out.push_str(&self.name);
        out.push(' ');
        if let Some((_, n)) = self.detents.iter().find(|(at, n)| (st.x - at).abs() < 1e-9 && !n.is_empty()) {
            out.push_str(n);
            return;
        }
        let decimals = if self.unit_name == "%" { 0 } else { 2 };
        units::format(st.x, &self.unit_name, &self.unit, decimals, out);
        if self.axes == 2 {
            out.push_str(" / ");
            units::format(st.y, &self.unit_name, &self.unit, decimals, out);
        }
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let x = value.clamp(self.lo, self.hi);
        let changed = x != st.x;
        st.x = x;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::Mods;

    #[test]
    fn throttle_gate_and_detents() {
        let d: ControlDef = serde_json::from_str(
            r#"{ "id": "acelerador", "kind": "palanca", "rango": [0.0, 1.05], "unidad": "%",
                 "retenes": [ { "en": 0.4, "nombre": "MÍN" }, { "en": 1.0, "nombre": "100 %" } ],
                 "compuerta": { "en": 1.0, "levantar": true } }"#,
        )
        .unwrap();
        let l = Lever::new(&d).unwrap();
        let g = Gate::default();
        let mut st = l.init();
        // drag up to the top: the gate stops it at 100 %
        let o = l.intent(&mut st, &Intent::Drag { dx: 0.0, dy: 1.0, m: Mods::default() }, &g);
        assert_eq!(st.x, 1.0);
        assert_eq!(o.event, Some(Event::Blocked(Blocked::Gate)));
        // lifted (Shift) it passes
        l.intent(&mut st, &Intent::Drag { dx: 0.0, dy: 0.1, m: Mods { coarse: true, fine: false } }, &g);
        assert!((st.x - 1.05).abs() < 1e-9);
        // a detent catches near it
        l.intent(&mut st, &Intent::Set { value: 0.3 }, &g);
        l.intent(&mut st, &Intent::Turn { notches: 9.5, rate: 1.0, m: Mods::default() }, &g);
        assert_eq!(st.x, 0.4);
        let mut s = String::new();
        l.describe(&st, &mut s);
        assert!(s.ends_with("MÍN"));
    }

    #[test]
    fn a_key_holds_a_sprung_stick_until_it_lets_go() {
        let d: ControlDef = serde_json::from_str(r#"{ "id": "palanca", "kind": "palanca", "ejes": 2, "rango": [-1.0, 1.0], "muelle_al_centro": true }"#).unwrap();
        let l = Lever::new(&d).unwrap();
        let g = Gate::default();
        let mut st = l.init();
        // W down: full forward, and it stays there for as long as the key is held
        l.intent(&mut st, &Intent::Axis { axis: 0, value: 1.0 }, &g);
        for _ in 0..240 {
            l.advance(&mut st, 1.0 / 60.0, 0.0);
        }
        assert_eq!((st.x, st.y), (1.0, 0.0), "se vuelve sola con la tecla pulsada");
        // the other axis too, each on its own
        l.intent(&mut st, &Intent::Axis { axis: 1, value: -1.0 }, &g);
        l.intent(&mut st, &Intent::Axis { axis: 0, value: 0.0 }, &g);
        for _ in 0..240 {
            l.advance(&mut st, 1.0 / 60.0, 0.0);
        }
        assert_eq!((st.x, st.y), (0.0, -1.0), "el eje soltado vuelve, el otro sigue");
        // let go: the spring takes it back, not at once
        l.intent(&mut st, &Intent::Axis { axis: 1, value: 0.0 }, &g);
        l.advance(&mut st, 1.0 / 60.0, 0.0);
        assert!(st.y < 0.0 && st.y > -1.0, "vuelve de golpe: {}", st.y);
        for _ in 0..60 {
            l.advance(&mut st, 1.0 / 60.0, 0.0);
        }
        assert_eq!(st.y, 0.0);
        // a hand's drag is still let go by the spring as before
        l.intent(&mut st, &Intent::Drag { dx: 0.0, dy: 0.2, m: Mods::default() }, &g);
        for _ in 0..120 {
            l.advance(&mut st, 1.0 / 60.0, 0.0);
        }
        assert_eq!(st.x, 0.0);
    }
}
