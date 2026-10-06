//! The continuous wheel (`docs/MANDOS_Y_MAQUINAS.md` §4.1): a value in [min, max] at its own
//! resolution, steps normal / coarse (Shift) / fine (Ctrl), faster steps when spun fast, inertia
//! that keeps it turning, hard stops or endless, detents that pull the value in (Ctrl ignores
//! them), linear or logarithmic, and a press that resets it or toggles coarse steps.
//! Pose e0: knob angle (rad).
//!
//! A hand wheel (`volante`: a valve's, on its bulkhead) is the same mechanism with a valve's
//! ways: from shut to wide open (0 to 100 %) in three turns, a twentieth of it a notch, a quarter
//! with Shift, a hundredth with Ctrl, unless its data says otherwise.
use super::{Mechanism, gate_block, q};
use crate::{
    def::ControlDef,
    intent::{ControlState, Event, F_COARSE, Gate, Intent, Mods, Outcome, Pose},
};
use lunar_signals::units::{self, Unit};

#[derive(Clone, Copy, Debug)]
struct Detent {
    at: f64,
    /// Share of the distance pulled in per step (0..1).
    strength: f64,
    width: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    None,
    Reset,
    Coarse,
}

pub struct Wheel {
    lo: f64,
    hi: f64,
    res: f64,
    default: f64,
    step: f64,
    coarse: f64,
    fine: f64,
    /// Acceleration: (from notches/s, k, gamma, max multiplier).
    accel: Option<(f64, f64, f64, f64)>,
    /// Inertia: damping (1/s) of the coasting speed; none: no coasting.
    damping: Option<f64>,
    hard: bool,
    detents: Vec<Detent>,
    log: bool,
    /// Knob angle over the whole range (rad).
    sweep: f32,
    press: Press,
    unit: Unit,
    unit_name: String,
    decimals: usize,
    name: String,
    /// "rueda", or "volante": a valve's hand wheel.
    kind: &'static str,
}

impl Wheel {
    pub fn new(d: &ControlDef) -> Result<Wheel, String> {
        let valve = d.kind == "volante";
        let whole = [lunar_signals::Q::S("0 %".into()), lunar_signals::Q::S("100 %".into())];
        let [lo, hi] = match d.rango.as_ref() {
            Some(r) => r,
            None if valve => &whole,
            None => return Err(format!("{}: una rueda necesita 'rango'", d.id)),
        };
        let unit_name = d.unidad.clone().unwrap_or_else(|| lo.unit_name().to_string());
        let unit = units::unit(&unit_name).map_err(|e| format!("{}: {}", d.id, e.0))?;
        let (lo, hi) = (lo.si().map_err(|e| e.0)?, hi.si().map_err(|e| e.0)?);
        if hi <= lo {
            return Err(format!("{}: rango vacío", d.id));
        }
        let log = match d.curva.as_deref() {
            None | Some("lineal") => false,
            Some("log") => true,
            Some(c) => return Err(format!("{}: curva '{c}' (lineal o log)", d.id)),
        };
        if log && lo <= 0.0 {
            return Err(format!("{}: una rueda logarítmica necesita un rango positivo", d.id));
        }
        // a step is a difference: "0.5 °C" is half a kelvin, not 273.65 K (log: a ratio)
        let delta = |v: &Option<lunar_signals::Q>, or: f64| -> Result<f64, String> {
            match v {
                None => Ok(or),
                Some(lunar_signals::Q::N(x)) => Ok(*x),
                Some(lunar_signals::Q::S(t)) => units::parse(t).map(|(x, u)| x - u.offset).map_err(|e| e.0),
            }
        };
        let span = hi - lo;
        let res = delta(&d.resolucion, if log { 0.001 } else if valve { span / 100.0 } else { span / 1000.0 })?.abs().max(1e-12);
        let step = delta(&d.paso, if log { 0.01 } else if valve { span / 20.0 } else { span / 100.0 })?.abs().max(res);
        let coarse = delta(&d.paso_grueso, step * if valve { 5.0 } else { 10.0 })?.abs().max(step);
        let fine = delta(&d.paso_fino, step / if valve { 5.0 } else { 10.0 })?.abs().max(res);
        let default = q(&d.defecto, &unit_name, lo)?.clamp(lo, hi);
        let accel = d.aceleracion.as_ref().map(|a| (a.desde, a.k, a.gamma, a.max.max(1.0)));
        let damping = d.inercia.as_ref().and_then(|i| i.amortiguamiento).map(|x| x.max(0.1));
        let hard = match d.topes.as_deref() {
            None | Some("duros") => true,
            Some("libre") => false,
            Some(t) => return Err(format!("{}: topes '{t}' (duros o libre)", d.id)),
        };
        let mut detents = Vec::new();
        for r in &d.retenes {
            let at = r.en.si().map_err(|e| e.0)?;
            let width = if r.ancho.is_some() { delta(&r.ancho, 0.0)? } else { step * 2.0 };
            detents.push(Detent { at, strength: r.fuerza.unwrap_or(0.5).clamp(0.0, 1.0), width: width.abs() });
        }
        let sweep = match (d.vueltas, d.recorrido) {
            (Some(t), _) => (t * 360.0).to_radians() as f32,
            (None, Some(r)) => r.to_radians() as f32,
            (None, None) if valve => (3.0f32 * 360.0).to_radians(),
            (None, None) => 300f32.to_radians(),
        };
        let press = match d.pulsar.as_deref() {
            None => Press::None,
            Some("reset") => Press::Reset,
            Some("grueso") => Press::Coarse,
            Some(p) => return Err(format!("{}: pulsar '{p}' (reset o grueso)", d.id)),
        };
        let decimals = if log { 3 } else { units::decimals_for(res / unit.scale) };
        Ok(Wheel { lo, hi, res, default, step, coarse, fine, accel, damping, hard, detents, log, sweep, press, unit, unit_name, decimals, name: d.label().to_string(), kind: if valve { "volante" } else { "rueda" } })
    }

    fn step_for(&self, m: Mods, st: &ControlState) -> f64 {
        if m.fine {
            self.fine
        } else if m.coarse || st.has(F_COARSE) {
            self.coarse
        } else {
            self.step
        }
    }

    /// Stops (or wrap), detents and resolution. Says whether it hit a stop or fell in a detent.
    fn settle(&self, st: &mut ControlState, fine: bool, entered: &mut Option<Event>) {
        let mut x = st.x;
        if self.hard {
            if x < self.lo || x > self.hi {
                *entered = Some(Event::Stop);
                st.v = 0.0;
            }
            x = x.clamp(self.lo, self.hi);
        } else if !self.log {
            let span = self.hi - self.lo;
            x = self.lo + (x - self.lo).rem_euclid(span);
        }
        if !fine {
            for d in &self.detents {
                let dist = (x - d.at).abs();
                if dist < d.width {
                    let was_out = (st.x - d.at).abs() >= d.width * 0.5;
                    x += (d.at - x) * d.strength;
                    if dist < d.width * 0.5 {
                        x = d.at;
                        if was_out {
                            *entered = Some(Event::Click);
                        }
                        // a strong detent catches a coasting wheel
                        if d.strength >= 0.5 {
                            st.v = 0.0;
                        }
                    }
                }
            }
        }
        // resolution (relative in log)
        x = if self.log {
            let r = (1.0 + self.res).ln();
            ((x.ln() / r).round() * r).exp()
        } else {
            self.lo + ((x - self.lo) / self.res).round() * self.res
        };
        if self.hard {
            x = x.clamp(self.lo, self.hi);
        }
        st.x = x;
    }

    /// Position in the range, 0..1.
    fn unit_pos(&self, x: f64) -> f64 {
        if self.log { (x / self.lo).ln() / (self.hi / self.lo).ln() } else { (x - self.lo) / (self.hi - self.lo) }
    }

    pub fn range(&self) -> (f64, f64) {
        (self.lo, self.hi)
    }

    pub fn sweep(&self) -> f32 {
        self.sweep
    }
}

impl Mechanism for Wheel {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn init(&self) -> ControlState {
        ControlState { x: self.default, ..Default::default() }
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        if let Some(b) = gate_block(g, false)
            && !matches!(i, Intent::Release | Intent::Hover(_) | Intent::Set { .. })
        {
            return Outcome::blocked(b);
        }
        let before = st.x;
        let mut ev = None;
        match *i {
            Intent::Turn { notches, rate, m } => {
                let mut mult = 1.0;
                if let Some((from, k, gamma, max)) = self.accel {
                    mult = (1.0 + k * (f64::from(rate) - from).max(0.0).powf(gamma)).min(max);
                }
                let amount = f64::from(notches) * self.step_for(m, st) * mult;
                st.x = if self.log { st.x * (1.0 + self.step_for(m, st)).powf(f64::from(notches) * mult) } else { st.x + amount };
                // a fast spin keeps turning
                if let (Some(c), Some((from, ..))) = (self.damping, self.accel)
                    && f64::from(rate) > from
                    && !self.log
                {
                    st.v += amount * c * 0.5;
                }
                self.settle(st, m.fine, &mut ev);
            }
            Intent::Press { .. } => match self.press {
                Press::Reset => {
                    st.x = self.default;
                    st.v = 0.0;
                    ev = Some(Event::Click);
                }
                Press::Coarse => {
                    let c = st.has(F_COARSE);
                    st.set(F_COARSE, !c);
                    ev = Some(Event::Click);
                }
                Press::None => {}
            },
            Intent::Set { value } => {
                st.x = value;
                st.v = 0.0;
                self.settle(st, true, &mut ev);
            }
            _ => {}
        }
        let changed = st.x != before;
        Outcome { changed, event: ev.or(if changed { Some(Event::Click) } else { None }) }
    }

    fn advance(&self, st: &mut ControlState, dt: f32, _: f32) -> Outcome {
        let Some(c) = self.damping else { return Outcome::default() };
        if st.v == 0.0 {
            return Outcome::default();
        }
        let before = st.x;
        let dt = f64::from(dt);
        st.x += st.v * dt;
        st.v *= (-c * dt).exp();
        if st.v.abs() * dt < self.res * 0.5 {
            st.v = 0.0;
        }
        let mut ev = None;
        self.settle(st, false, &mut ev);
        Outcome { changed: st.x != before, event: ev }
    }

    fn value(&self, st: &ControlState) -> f64 {
        st.x
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        out.e[0] = (self.unit_pos(st.x) as f32 - 0.5) * self.sweep;
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        out.push_str(&self.name);
        out.push(' ');
        // a valve says it is shut or wide open in words
        if self.kind == "volante" && (st.x <= self.lo || st.x >= self.hi) {
            out.push_str(if st.x <= self.lo { "CERRADA" } else { "ABIERTA DEL TODO" });
            return;
        }
        units::format(st.x, &self.unit_name, &self.unit, self.decimals, out);
        if st.has(F_COARSE) {
            out.push_str(" (grueso)");
        }
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let before = st.x;
        st.x = value;
        let mut ev = None;
        self.settle(st, true, &mut ev);
        st.x != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::ControlDef;

    fn wheel(extra: &str) -> Wheel {
        let src = format!(
            r#"{{ "id": "consigna_T", "kind": "rueda", "nombre": "Consigna", "rango": ["250 °C", "650 °C"], "resolucion": "0.01 °C",
               "defecto": "420 °C", "paso": "0.5 °C", "paso_grueso": "5 °C", "paso_fino": "0.05 °C" {extra} }}"#
        );
        Wheel::new(&serde_json::from_str::<ControlDef>(&src).unwrap()).unwrap()
    }

    fn turn(n: f32, rate: f32, m: Mods) -> Intent {
        Intent::Turn { notches: n, rate, m }
    }

    #[test]
    fn steps_normal_coarse_fine() {
        let w = wheel("");
        let mut st = w.init();
        let g = Gate::default();
        assert!((st.x - 693.15).abs() < 1e-9);
        w.intent(&mut st, &turn(1.0, 1.0, Mods::default()), &g);
        assert!((st.x - 693.65).abs() < 1e-6, "{}", st.x);
        w.intent(&mut st, &turn(1.0, 1.0, Mods { coarse: true, fine: false }), &g);
        assert!((st.x - 698.65).abs() < 1e-6);
        w.intent(&mut st, &turn(-1.0, 1.0, Mods { coarse: false, fine: true }), &g);
        assert!((st.x - 698.60).abs() < 1e-6);
        let mut s = String::new();
        w.describe(&st, &mut s);
        assert_eq!(s, "Consigna 425,45 °C");
    }

    #[test]
    fn acceleration_detents_stops_and_inertia() {
        let w = wheel(r#", "aceleracion": { "desde": 4.0, "k": 0.6, "gamma": 1.5, "max": 20.0 },
            "inercia": { "momento": 0.002, "amortiguamiento": 6.0 }, "retenes": [ { "en": "420 °C", "fuerza": 0.6, "ancho": "1.5 °C" } ]"#);
        let g = Gate::default();
        // faster spins step further
        let mut a = w.init();
        let mut b = w.init();
        w.intent(&mut a, &turn(1.0, 2.0, Mods::default()), &g);
        w.intent(&mut b, &turn(1.0, 12.0, Mods::default()), &g);
        assert!(b.x - 693.15 > (a.x - 693.15) * 3.0);
        // and keep coasting until they stop
        let x = b.x;
        let mut moved = 0;
        for _ in 0..200 {
            if w.advance(&mut b, 0.01, 0.0).changed {
                moved += 1;
            }
        }
        assert!(moved > 0 && b.x > x && b.v == 0.0);
        // a detent catches a value near it; Ctrl ignores it
        let mut c = w.init();
        w.intent(&mut c, &turn(1.0, 1.0, Mods::default()), &g);
        assert!((c.x - 693.15).abs() < 1e-6, "pulled back into the detent: {}", c.x);
        w.intent(&mut c, &turn(1.0, 1.0, Mods { coarse: false, fine: true }), &g);
        assert!((c.x - 693.20).abs() < 1e-6);
        // hard stops
        let mut d = w.init();
        let o = w.intent(&mut d, &turn(200.0, 1.0, Mods { coarse: true, fine: false }), &g);
        assert!((d.x - 923.15).abs() < 1e-6 && o.event == Some(Event::Stop));
    }

    #[test]
    fn a_hand_wheel_opens_a_valve_by_turns() {
        let d: ControlDef = serde_json::from_str(r#"{ "id": "volante", "kind": "volante", "nombre": "Válvula" }"#).unwrap();
        let w = Wheel::new(&d).unwrap();
        assert_eq!(w.kind(), "volante");
        let g = Gate::default();
        let mut st = w.init();
        let mut s = String::new();
        w.describe(&st, &mut s);
        assert_eq!((w.value(&st), s.as_str()), (0.0, "Válvula CERRADA"));
        // a notch is a twentieth, Shift a quarter, Ctrl a hundredth; it stops shut and wide open
        w.intent(&mut st, &turn(1.0, 1.0, Mods::default()), &g);
        assert!((w.value(&st) - 0.05).abs() < 1e-9);
        w.intent(&mut st, &turn(1.0, 1.0, Mods { coarse: true, fine: false }), &g);
        w.intent(&mut st, &turn(1.0, 1.0, Mods { coarse: false, fine: true }), &g);
        assert!((w.value(&st) - 0.31).abs() < 1e-9, "{}", w.value(&st));
        s.clear();
        w.describe(&st, &mut s);
        assert_eq!(s, "Válvula 31 %");
        let mut pose = Pose::default();
        w.pose(&st, &mut pose);
        let turned = pose.e[0];
        let o = w.intent(&mut st, &turn(40.0, 1.0, Mods::default()), &g);
        assert!(w.value(&st) == 1.0 && o.event == Some(Event::Stop));
        w.pose(&st, &mut pose);
        // three turns from shut to open: the wheel is seen to go round
        assert!((pose.e[0] - turned - 0.69 * 3.0 * std::f32::consts::TAU).abs() < 1e-3, "{} → {}", turned, pose.e[0]);
        let o = w.intent(&mut st, &turn(-40.0, 1.0, Mods::default()), &g);
        assert!(w.value(&st) == 0.0 && o.event == Some(Event::Stop));
    }
}
