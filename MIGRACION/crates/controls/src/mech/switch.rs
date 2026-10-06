//! Toggle switches (2 or 3 positions, sprung positions, pull-to-move) and rotary selectors (N
//! detents, end stops or free, pull-to-turn). Pose e0: lever or knob angle (rad), e1: pulled out.
use super::{Mechanism, gate_block, pos_label};
use crate::{
    def::ControlDef,
    intent::{Blocked, ControlState, Event, F_PRESSED, F_PULLED, Gate, Intent, Outcome, Pose},
};

/// Seconds of hold that pull a protected position free.
const PULL: f32 = 0.35;

fn values(d: &ControlDef, n: usize) -> Result<Vec<f64>, String> {
    if d.valores.is_empty() {
        return Ok((0..n).map(|i| i as f64).collect());
    }
    if d.valores.len() != n {
        return Err(format!("{}: {} valores para {} posiciones", d.id, d.valores.len(), n));
    }
    d.valores.iter().map(|v| v.si().map_err(|e| e.0)).collect()
}

fn index_of(labels: &[String], name: &str, id: &str) -> Result<usize, String> {
    labels.iter().position(|l| l == name).ok_or_else(|| format!("{id}: no hay posición '{name}'"))
}

fn nearest(values: &[f64], v: f64) -> usize {
    let mut best = 0;
    for (i, x) in values.iter().enumerate() {
        if (x - v).abs() < (values[best] - v).abs() {
            best = i;
        }
    }
    best
}

fn default_of(d: &ControlDef, values: &[f64]) -> Result<usize, String> {
    match &d.defecto {
        Some(q) => Ok(nearest(values, q.si().map_err(|e| e.0)?)),
        None => Ok(0),
    }
}

pub struct Switch {
    labels: Vec<String>,
    values: Vec<f64>,
    /// Where each position springs back to (itself if it stays).
    spring: Vec<usize>,
    pull: Vec<bool>,
    default: usize,
    /// Lever throw per position step (rad).
    step: f32,
}

impl Switch {
    pub fn new(d: &ControlDef) -> Result<Switch, String> {
        let labels = if d.posiciones.is_empty() { vec!["APAGADO".into(), "ENCENDIDO".into()] } else { d.posiciones.clone() };
        let n = labels.len();
        if !(2..=3).contains(&n) {
            return Err(format!("{}: un interruptor tiene 2 o 3 posiciones", d.id));
        }
        let mut spring: Vec<usize> = (0..n).collect();
        if let Some(m) = &d.muelle {
            for (from, to) in m {
                spring[index_of(&labels, from, &d.id)?] = index_of(&labels, to, &d.id)?;
            }
        }
        let mut pull = vec![false; n];
        for p in &d.tirar_para_mover {
            pull[index_of(&labels, p, &d.id)?] = true;
        }
        let values = values(d, n)?;
        let default = default_of(d, &values)?;
        let step = if n == 2 { 0.9 } else { 0.55 };
        Ok(Switch { labels, values, spring, pull, default, step })
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    fn go(&self, st: &mut ControlState, to: usize) -> Outcome {
        let from = st.x as usize;
        if to == from {
            return Outcome { changed: false, event: Some(Event::Stop) };
        }
        if self.pull[to] && !st.has(F_PULLED) {
            return Outcome::blocked(Blocked::Pull);
        }
        st.x = to as f64;
        Outcome::click(true)
    }
}

impl Mechanism for Switch {
    fn kind(&self) -> &'static str {
        "interruptor"
    }

    fn init(&self) -> ControlState {
        ControlState { x: self.default as f64, ..Default::default() }
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        let n = self.labels.len();
        let cur = st.x as usize;
        if let Some(b) = gate_block(g, false)
            && !matches!(i, Intent::Release | Intent::Hover(_) | Intent::Set { .. })
        {
            return Outcome::blocked(b);
        }
        match *i {
            Intent::Press { .. } => {
                st.set(F_PRESSED, true);
                // two positions: flip; three: one up, and from the top back to the bottom
                let to = if n == 2 { 1 - cur.min(1) } else if cur + 1 < n { cur + 1 } else { 0 };
                self.go(st, to)
            }
            Intent::Turn { notches, .. } => {
                let to = if notches > 0.0 { (cur + 1).min(n - 1) } else { cur.saturating_sub(1) };
                self.go(st, to)
            }
            Intent::Hold { secs } => {
                if secs >= PULL && !st.has(F_PULLED) {
                    st.set(F_PULLED, true);
                    // a press held toward a protected position goes there now
                    let to = if n == 2 { 1 - cur.min(1) } else { (cur + 1).min(n - 1) };
                    if self.pull[to] {
                        return self.go(st, to);
                    }
                }
                Outcome::default()
            }
            Intent::Release => {
                st.set(F_PRESSED, false);
                st.set(F_PULLED, false);
                // a sprung position goes back when let go
                let back = self.spring[cur];
                if back != cur {
                    st.x = back as f64;
                    return Outcome { changed: true, event: Some(Event::Spring) };
                }
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
        self.values[(st.x as usize).min(self.values.len() - 1)]
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        let mid = (self.labels.len() - 1) as f32 * 0.5;
        out.e[0] = (st.x as f32 - mid) * self.step;
        out.e[1] = if st.has(F_PULLED) { 0.004 } else { 0.0 };
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        pos_label(&self.labels, st.x as usize, out);
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let to = nearest(&self.values, value) as f64;
        let changed = st.x != to;
        st.x = to;
        changed
    }
}

pub struct Selector {
    labels: Vec<String>,
    values: Vec<f64>,
    wrap: bool,
    pull: Vec<bool>,
    default: usize,
    /// Knob angle per detent (rad).
    step: f32,
}

impl Selector {
    pub fn new(d: &ControlDef) -> Result<Selector, String> {
        let labels = d.posiciones.clone();
        let n = labels.len();
        if n < 2 {
            return Err(format!("{}: un selector necesita al menos 2 posiciones", d.id));
        }
        let wrap = match d.vuelta.as_deref() {
            None | Some("tope") => false,
            Some("libre") => true,
            Some(v) => return Err(format!("{}: vuelta '{v}' (tope o libre)", d.id)),
        };
        let mut pull = vec![false; n];
        for p in &d.tirar_para_girar {
            pull[index_of(&labels, p, &d.id)?] = true;
        }
        let values = values(d, n)?;
        let default = default_of(d, &values)?;
        let travel = d.recorrido.unwrap_or(if wrap { 360.0 } else { 270.0 }).to_radians() as f32;
        let step = travel / if wrap { n as f32 } else { (n - 1) as f32 };
        Ok(Selector { labels, values, wrap, pull, default, step })
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Knob angle per detent (rad): the silkscreen marks go there.
    pub fn step(&self) -> f32 {
        self.step
    }

    pub fn wraps(&self) -> bool {
        self.wrap
    }

    fn turn(&self, st: &mut ControlState, dir: i32) -> Outcome {
        let n = self.labels.len() as i32;
        let cur = st.x as i32;
        let mut to = cur + dir;
        if self.wrap {
            to = to.rem_euclid(n);
        } else if to < 0 || to >= n {
            return Outcome { changed: false, event: Some(Event::Stop) };
        }
        // leaving or entering a protected position needs a pull
        if (self.pull[to as usize] || self.pull[cur as usize]) && !st.has(F_PULLED) {
            return Outcome::blocked(Blocked::Pull);
        }
        st.x = f64::from(to);
        Outcome::click(true)
    }
}

impl Mechanism for Selector {
    fn kind(&self) -> &'static str {
        "selector"
    }

    fn init(&self) -> ControlState {
        ControlState { x: self.default as f64, ..Default::default() }
    }

    fn intent(&self, st: &mut ControlState, i: &Intent, g: &Gate) -> Outcome {
        if let Some(b) = gate_block(g, false)
            && !matches!(i, Intent::Release | Intent::Hover(_) | Intent::Set { .. })
        {
            return Outcome::blocked(b);
        }
        match *i {
            Intent::Press { .. } => {
                st.set(F_PRESSED, true);
                let o = self.turn(st, 1);
                if o.event == Some(Event::Stop) {
                    // at the end stop: a press goes back to the first position
                    st.x = 0.0;
                    return Outcome::click(true);
                }
                o
            }
            Intent::Turn { notches, .. } => {
                let dir = if notches > 0.0 { 1 } else { -1 };
                let mut out = Outcome::default();
                for _ in 0..(notches.abs().round() as u32).max(1) {
                    let o = self.turn(st, dir);
                    out.changed |= o.changed;
                    out.event = o.event.or(out.event);
                    if !o.changed {
                        break;
                    }
                }
                out
            }
            Intent::Hold { secs } => {
                if secs >= PULL {
                    st.set(F_PULLED, true);
                }
                Outcome::default()
            }
            Intent::Release => {
                st.set(F_PRESSED, false);
                st.set(F_PULLED, false);
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
        self.values[(st.x as usize).min(self.values.len() - 1)]
    }

    fn pose(&self, st: &ControlState, out: &mut Pose) {
        let n = self.labels.len();
        let mid = if self.wrap { 0.0 } else { (n - 1) as f32 * 0.5 };
        out.e[0] = (st.x as f32 - mid) * self.step;
        out.e[1] = if st.has(F_PULLED) { 0.004 } else { 0.0 };
    }

    fn describe(&self, st: &ControlState, out: &mut String) {
        pos_label(&self.labels, st.x as usize, out);
    }

    fn set(&self, st: &mut ControlState, value: f64) -> bool {
        let to = nearest(&self.values, value) as f64;
        let changed = st.x != to;
        st.x = to;
        changed
    }
}
