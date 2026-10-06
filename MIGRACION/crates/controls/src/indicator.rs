//! Indicators: lamps, needles, bars, seven-segment displays, mechanical counters, screens and
//! annunciators. They read signals (and expressions over them) and keep only what their look
//! needs: a needle's swing, a lamp's light, a screen's lines. Without power they go dark (a
//! needle drops to its rest, a counter keeps its digits: it is mechanical).
use crate::def::{ColorDef, ControlDef, named_color};
use lunar_signals::{
    Eval, Program, Quality, SignalId, Store, compile_in,
    units::{self, Unit},
};

#[derive(Clone, Debug)]
pub struct LampRule {
    pub cond: Program,
    pub color: [u8; 3],
    pub blink: f64,
}

/// A piece of a screen line.
#[derive(Clone, Debug)]
pub enum Piece {
    Text(String),
    /// A signal in its display unit, with decimals.
    Value(SignalId, usize),
    /// The label of a signal's index.
    Label(SignalId, Vec<String>),
}

#[derive(Clone, Debug)]
pub struct Cell {
    /// As the data wrote it (a "\n" asks for two lines).
    pub label: String,
    /// As it is lettered: one or two lines of `CELL_LETTERS` at most each (`caption`).
    pub lines: Vec<String>,
    pub cond: Program,
    pub level: u8,
}

/// Letters a line of an annunciator cell has at most: every cell of every annunciator is lettered
/// at one size, the one at which this many fit across.
pub const CELL_LETTERS: usize = 6;

/// A cell's label as it is lettered: the two lines the data gives ("MOTOR\nIZQ"), or its words
/// laid in two lines when it is too long for one, each line cut to `CELL_LETTERS` (a full stop
/// in the label marks an abbreviation and takes no room).
pub fn caption(label: &str) -> Vec<String> {
    let cut = |l: &str| -> String { l.trim().trim_end_matches('.').chars().take(CELL_LETTERS).collect() };
    let clean = label.trim();
    if clean.contains('\n') {
        return clean.split('\n').take(2).map(cut).filter(|l| !l.is_empty()).collect();
    }
    if clean.trim_end_matches('.').chars().count() <= CELL_LETTERS {
        return vec![cut(clean)];
    }
    // too long for a line: its words in two, the first as full as it gets
    let words: Vec<&str> = clean.split_whitespace().collect();
    if words.len() < 2 {
        return vec![cut(clean)];
    }
    let mut first = String::new();
    let mut k = 0;
    while k < words.len() - 1 && first.chars().count() + words[k].chars().count() + usize::from(!first.is_empty()) <= CELL_LETTERS {
        if !first.is_empty() {
            first.push(' ');
        }
        first.push_str(words[k]);
        k += 1;
    }
    if first.is_empty() {
        first = words[0].to_string();
        k = 1;
    }
    vec![cut(&first), cut(&words[k..].join(" "))]
}

#[derive(Clone, Debug)]
pub enum IndKind {
    Lamp { rules: Vec<LampRule> },
    Needle { signal: SignalId, lo: f64, hi: f64, sweep: f32, freq: f32, damp: f32, zones: Vec<(f64, f64, [u8; 3])> },
    Bar { signal: SignalId, lo: f64, hi: f64, color: [u8; 3] },
    Seven { signal: SignalId, unit: Unit, decimals: usize, digits: usize, color: [u8; 3] },
    Counter { signal: SignalId, unit: Unit, digits: usize },
    Screen { pages: Vec<(String, Vec<Vec<Piece>>)>, page: Option<SignalId>, color: [u8; 3] },
    Annunciator { cells: Vec<Cell>, ack: Option<SignalId>, columns: u32 },
    /// A multi-function display (`mfd`).
    Mfd(Box<crate::mfd::Mfd>),
}

#[derive(Clone, Debug)]
pub struct Indicator {
    pub kind: IndKind,
    /// Power it draws while lit (W).
    pub power: f64,
    /// Slots of expression state it needs.
    pub slots: usize,
}

/// One annunciator cell now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellState {
    pub active: bool,
    pub acked: bool,
    /// How lit (0..1) and its colour.
    pub lit: f32,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Default)]
pub struct IndState {
    /// Needle angle (rad) and its speed, or the bar's fill (0..1).
    pub x: f32,
    pub v: f32,
    pub lit: f32,
    pub color: [u8; 3],
    /// Display text, or the screen's title.
    pub text: String,
    pub lines: Vec<String>,
    pub cells: Vec<CellState>,
    pub slots: Vec<f64>,
    /// A multi-function display's pages now.
    pub mfd: crate::mfd::MfdState,
    /// Screens refresh at 10 Hz.
    refresh: f32,
    ack_seen: bool,
    /// The reading a display's digits were last written for (its bits; none: dark, or not yet
    /// written): while it stands, they are not written again.
    seen: Option<u64>,
}

fn color(c: &Option<ColorDef>, or: [u8; 3]) -> Result<[u8; 3], String> {
    c.as_ref().map_or(Ok(or), ColorDef::rgb)
}

fn signal(d: &ControlDef, store: &Store) -> Result<SignalId, String> {
    let name = d.senal.as_deref().or(d.bind.senal.as_deref()).ok_or_else(|| format!("{}: falta 'senal'", d.id))?;
    store.find(name).ok_or_else(|| format!("{}: señal desconocida '{name}'", d.id))
}

/// "EMPUJE {motor.empuje:1} kN" → pieces.
fn parse_line(src: &str, store: &Store, id: &str) -> Result<Vec<Piece>, String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            out.push(Piece::Text(rest[..open].to_string()));
        }
        let close = rest[open..].find('}').ok_or_else(|| format!("{id}: falta '}}' en '{src}'"))? + open;
        let inner = &rest[open + 1..close];
        if let Some((name, labels)) = inner.split_once('|') {
            let s = store.find(name.trim()).ok_or_else(|| format!("{id}: señal desconocida '{name}'"))?;
            out.push(Piece::Label(s, labels.split('|').map(str::to_string).collect()));
        } else {
            let (name, dec) = inner.split_once(':').map_or((inner, 1), |(n, d)| (n, d.trim().parse().unwrap_or(1)));
            let s = store.find(name.trim()).ok_or_else(|| format!("{id}: señal desconocida '{name}'"))?;
            out.push(Piece::Value(s, dec));
        }
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_string()));
    }
    Ok(out)
}

/// A screen line's pieces as text now.
pub fn render_line(l: &[Piece], store: &Store, out: &mut String) {
    for p in l {
        match p {
            Piece::Text(s) => out.push_str(s),
            Piece::Value(s, dec) => {
                if store.quality(*s) == Quality::Failed {
                    out.push_str("----");
                } else {
                    store.format(*s, *dec, out);
                }
            }
            Piece::Label(s, labels) => {
                let i = store.get(*s).round().max(0.0) as usize;
                out.push_str(labels.get(i).map_or("?", String::as_str));
            }
        }
    }
}

impl Indicator {
    /// The indicator of a definition, or None when the kind is a mechanism.
    pub fn build(d: &ControlDef, store: &Store) -> Result<Option<Indicator>, String> {
        let err = |e: lunar_signals::expr::ExprError| format!("{}: {}", d.id, e.0);
        let mut slots = 0;
        let power = match &d.requiere.energia {
            Some(q) => q.si_as("W").map_err(|e| e.0)?,
            None => 0.0,
        };
        let kind = match d.kind.as_str() {
            "lampara" => {
                let mut rules = Vec::new();
                for r in &d.reglas {
                    let cond = compile_in(&r.si, store).map_err(err)?;
                    slots += cond.slots;
                    rules.push(LampRule { cond, color: color(&r.color, [60, 255, 90])?, blink: r.parpadeo.unwrap_or(0.0) });
                }
                if rules.is_empty() {
                    // a plain lamp: lit when its signal is on
                    let s = signal(d, store)?;
                    let cond = compile_in(store.name(s), store).map_err(err)?;
                    rules.push(LampRule { cond, color: color(&d.color, [60, 255, 90])?, blink: 0.0 });
                }
                IndKind::Lamp { rules }
            }
            "aguja" => {
                let s = signal(d, store)?;
                let [lo, hi] = d.escala.as_ref().ok_or_else(|| format!("{}: falta 'escala'", d.id))?;
                let (lo, hi) = (lo.si().map_err(|e| e.0)?, hi.si().map_err(|e| e.0)?);
                let (mut freq, mut damp) = (3.0, 0.7);
                if let Some(i) = &d.inercia {
                    freq = i.frecuencia.unwrap_or(3.0) as f32;
                    damp = i.amortiguamiento.unwrap_or(0.7) as f32;
                }
                let mut zones = Vec::new();
                if let Some(z) = &d.zonas {
                    for (r, c) in [(&z.verde, "verde"), (&z.ambar, "ambar"), (&z.roja, "rojo")] {
                        if let Some([a, b]) = r {
                            zones.push((a.si().map_err(|e| e.0)?, b.si().map_err(|e| e.0)?, named_color(c).unwrap_or([255; 3])));
                        }
                    }
                }
                let sweep = d.recorrido.unwrap_or(270.0).to_radians() as f32;
                IndKind::Needle { signal: s, lo, hi, sweep, freq, damp, zones }
            }
            "barra" => {
                let s = signal(d, store)?;
                let (lo, hi) = match &d.escala {
                    Some([a, b]) => (a.si().map_err(|e| e.0)?, b.si().map_err(|e| e.0)?),
                    None => (0.0, 1.0),
                };
                IndKind::Bar { signal: s, lo, hi, color: color(&d.color, [60, 255, 90])? }
            }
            "display7" | "contador" => {
                let s = signal(d, store)?;
                let unit_name = d.unidad.clone().unwrap_or_else(|| store.meta(s).show.to_string());
                let unit = units::unit(&unit_name).map_err(|e| format!("{}: {}", d.id, e.0))?;
                let digits = d.digitos.unwrap_or(4);
                if d.kind == "contador" {
                    IndKind::Counter { signal: s, unit, digits }
                } else {
                    IndKind::Seven { signal: s, unit, decimals: d.decimales.unwrap_or(0), digits, color: color(&d.color, [255, 120, 30])? }
                }
            }
            "pantalla" => {
                let mut pages = Vec::new();
                for p in &d.paginas {
                    let lines = p.lineas.iter().map(|l| parse_line(l, store, &d.id)).collect::<Result<Vec<_>, _>>()?;
                    pages.push((p.titulo.clone(), lines));
                }
                let page = match &d.pagina {
                    Some(n) => Some(store.find(n).ok_or_else(|| format!("{}: señal de página desconocida '{n}'", d.id))?),
                    None => None,
                };
                IndKind::Screen { pages, page, color: color(&d.color, [110, 255, 140])? }
            }
            "mfd" => IndKind::Mfd(Box::new(crate::mfd::Mfd::build(d, store, |l| parse_line(l, store, &d.id))?)),
            "anunciador" => {
                let mut cells = Vec::new();
                for w in &d.avisos {
                    let cond = compile_in(&w.si, store).map_err(err)?;
                    slots += cond.slots;
                    cells.push(Cell { label: w.rotulo.replace('\n', " "), lines: caption(&w.rotulo), cond, level: w.nivel });
                }
                let ack = match &d.reconocer {
                    Some(n) => Some(store.find(n).ok_or_else(|| format!("{}: señal de reconocer desconocida '{n}'", d.id))?),
                    None => None,
                };
                IndKind::Annunciator { columns: d.columnas.unwrap_or(cells.len().min(6) as u32).max(1), cells, ack }
            }
            _ => return Ok(None),
        };
        Ok(Some(Indicator { kind, power, slots }))
    }

    pub fn init(&self) -> IndState {
        let mut st = IndState { slots: vec![0.0; self.slots], ..Default::default() };
        if let IndKind::Annunciator { cells, .. } = &self.kind {
            st.cells = vec![CellState::default(); cells.len()];
        }
        if let IndKind::Screen { pages, .. } = &self.kind {
            st.lines = vec![String::new(); pages.iter().map(|p| p.1.len()).max().unwrap_or(0)];
        }
        if let IndKind::Mfd(m) = &self.kind {
            st.mfd = m.init();
        }
        st
    }

    /// Follow the signals for `dt` s (time now `t`), powered or not.
    pub fn update(&self, st: &mut IndState, store: &Store, powered: bool, dt: f32, t: f64, eval: &mut Eval) {
        let dtd = f64::from(dt);
        match &self.kind {
            IndKind::Lamp { rules } => {
                st.lit = 0.0;
                let mut off = 0;
                let mut hit = None;
                for r in rules {
                    let n = r.cond.slots;
                    let on = eval.run(&r.cond, store, &mut st.slots[off..off + n], dtd, t) >= 0.5;
                    off += n;
                    if on && hit.is_none() {
                        hit = Some(r);
                    }
                }
                if let Some(r) = hit
                    && powered
                {
                    let blink_on = r.blink <= 0.0 || (t * r.blink).fract() < 0.5;
                    st.lit = if blink_on { 1.0 } else { 0.0 };
                    st.color = r.color;
                }
            }
            IndKind::Needle { signal, lo, hi, sweep, freq, damp, .. } => {
                let rest = -0.5 * sweep;
                let target = if powered && store.quality(*signal) != Quality::Failed {
                    // a little past the ends: the pins
                    let u = ((store.get(*signal) - lo) / (hi - lo)).clamp(-0.03, 1.03) as f32;
                    (u - 0.5) * sweep
                } else {
                    rest
                };
                // at rest on its reading: nothing to move
                if st.v == 0.0 && st.x == target {
                    return;
                }
                // spring and damper, in small steps
                let w = std::f32::consts::TAU * freq;
                let n = ((dt * w * 4.0).ceil() as u32).clamp(1, 16);
                let h = dt / n as f32;
                for _ in 0..n {
                    let a = w * w * (target - st.x) - 2.0 * damp * w * st.v;
                    st.v += a * h;
                    st.x += st.v * h;
                }
                // (there, it stops: a needle does not creep for ever)
                if (target - st.x).abs() < 1e-5 && st.v.abs() < 1e-4 {
                    (st.x, st.v) = (target, 0.0);
                }
            }
            IndKind::Bar { signal, lo, hi, color } => {
                let u = if powered { ((store.get(*signal) - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 } else { 0.0 };
                st.x += (u - st.x) * (1.0 - (-dt * 8.0).exp());
                st.color = *color;
                st.lit = if powered { 1.0 } else { 0.0 };
            }
            IndKind::Seven { signal, unit, decimals, digits, color } => {
                st.color = *color;
                if !powered {
                    if st.lit != 0.0 || st.seen.is_some() {
                        st.text.clear();
                        (st.lit, st.seen) = (0.0, None);
                    }
                    return;
                }
                // the same reading as last time: its digits stand as they are
                let v = store.get(*signal);
                let failed = store.quality(*signal) == Quality::Failed || !v.is_finite();
                let key = if failed { u64::MAX } else { v.to_bits() };
                if st.seen == Some(key) {
                    return;
                }
                st.seen = Some(key);
                st.text.clear();
                st.lit = 1.0;
                {
                    if failed {
                        st.text.extend(std::iter::repeat_n('-', *digits));
                    } else {
                        // it has so many cells: fewer decimals, then over range ("OL")
                        let mut dec = *decimals;
                        loop {
                            st.text.clear();
                            units::format(v, "", unit, dec, &mut st.text);
                            let cells = st.text.chars().filter(|c| !matches!(c, ',' | '.') && !c.is_whitespace()).count();
                            if cells <= *digits {
                                break;
                            }
                            if dec == 0 {
                                st.text.clear();
                                st.text.push_str(if v < 0.0 { "-OL" } else { "OL" });
                                break;
                            }
                            dec -= 1;
                        }
                    }
                }
            }
            IndKind::Counter { signal, unit, digits } => {
                if powered {
                    st.text.clear();
                    use std::fmt::Write;
                    let v = unit.from_si(store.get(*signal)).max(0.0).round() as u64;
                    let _ = write!(st.text, "{:0width$}", v % 10u64.pow(*digits as u32), width = *digits);
                }
            }
            IndKind::Screen { pages, page, color } => {
                st.color = *color;
                st.lit = if powered { 1.0 } else { 0.0 };
                st.refresh -= dt;
                if st.refresh > 0.0 {
                    return;
                }
                st.refresh = 0.1;
                st.text.clear();
                for l in &mut st.lines {
                    l.clear();
                }
                if !powered || pages.is_empty() {
                    return;
                }
                let k = page.map_or(0, |p| store.get(p).round().max(0.0) as usize).min(pages.len() - 1);
                let (title, lines) = &pages[k];
                st.text.push_str(title);
                for (i, l) in lines.iter().enumerate() {
                    render_line(l, store, &mut st.lines[i]);
                }
            }
            IndKind::Mfd(m) => {
                st.color = m.color;
                st.lit = if powered { 1.0 } else { 0.0 };
                st.refresh -= dt;
                let refresh = st.refresh <= 0.0;
                if refresh {
                    st.refresh = 0.1;
                }
                m.update(&mut st.mfd, store, powered, dt, refresh);
            }
            IndKind::Annunciator { cells, ack, .. } => {
                let ack_now = ack.is_some_and(|a| store.on(a));
                let ack_edge = ack_now && !st.ack_seen;
                st.ack_seen = ack_now;
                let mut off = 0;
                for (c, cs) in cells.iter().zip(st.cells.iter_mut()) {
                    let n = c.cond.slots;
                    let on = eval.run(&c.cond, store, &mut st.slots[off..off + n], dtd, t) >= 0.5;
                    off += n;
                    if on && !cs.active {
                        cs.active = true;
                        cs.acked = false;
                    } else if !on {
                        cs.active = false;
                    }
                    if ack_edge && cs.active {
                        cs.acked = true;
                    }
                    cs.color = if c.level >= 2 { [255, 40, 30] } else { [255, 170, 20] };
                    let blink = !cs.acked && c.level >= 2 && (t * 2.0).fract() >= 0.5;
                    cs.lit = if powered && cs.active && !blink { 1.0 } else { 0.0 };
                }
            }
        }
    }

    /// Something it shows needs attention (an unacknowledged cell): master caution.
    pub fn alarming(&self, st: &IndState) -> bool {
        st.cells.iter().any(|c| c.active && !c.acked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needle_settles_and_annunciator_latches() {
        let mut store = Store::new();
        let p = store.define_unit("motor.empuje", "kN", 30_000.0).unwrap();
        let ack = store.define("panel.reconocer");
        let d: ControlDef = serde_json::from_str(r#"{ "id": "empuje", "kind": "aguja", "senal": "motor.empuje", "escala": ["0 kN", "50 kN"] }"#).unwrap();
        let ind = Indicator::build(&d, &store).unwrap().unwrap();
        let mut st = ind.init();
        let mut e = Eval::default();
        for _ in 0..300 {
            ind.update(&mut st, &store, true, 0.01, 0.0, &mut e);
        }
        let IndKind::Needle { sweep, .. } = ind.kind else { panic!() };
        assert!((st.x - (0.6 - 0.5) * sweep).abs() < 1e-3, "{}", st.x);
        // unpowered it falls to its rest
        for _ in 0..300 {
            ind.update(&mut st, &store, false, 0.01, 0.0, &mut e);
        }
        assert!((st.x + 0.5 * sweep).abs() < 1e-3);
        let d: ControlDef = serde_json::from_str(r#"{ "id": "a", "kind": "anunciador", "reconocer": "panel.reconocer",
            "avisos": [ { "rotulo": "EMPUJE", "si": "motor.empuje > 40 kN", "nivel": 2 } ] }"#)
        .unwrap();
        let ann = Indicator::build(&d, &store).unwrap().unwrap();
        let mut a = ann.init();
        ann.update(&mut a, &store, true, 0.05, 0.0, &mut e);
        assert!(!ann.alarming(&a));
        store.set(p, 45_000.0);
        ann.update(&mut a, &store, true, 0.05, 0.1, &mut e);
        assert!(ann.alarming(&a));
        store.set(ack, 1.0);
        ann.update(&mut a, &store, true, 0.05, 0.2, &mut e);
        assert!(!ann.alarming(&a) && a.cells[0].lit == 1.0);
    }

    #[test]
    fn annunciator_cells_are_lettered_in_short_lines() {
        let l = |s: &str| caption(s);
        assert_eq!(l("TREN"), ["TREN"]);
        assert_eq!(l("MOTOR
IZQ"), ["MOTOR", "IZQ"]);
        // too long for a line: its words in two, each cut to what fits
        assert_eq!(l("FUGA COMB"), ["FUGA", "COMB"]);
        assert_eq!(l("BUS A"), ["BUS A"]);
        assert_eq!(l("HIDRÁUL."), ["HIDRÁU"]);
        assert_eq!(l("ORDENADOR DE VUELO"), ["ORDENA", "DE VUE"]);
        for label in ["BATERÍA", "COMBUST.", "CABINA P", "X
Y
Z", ""] {
            let c = l(label);
            assert!(c.len() <= 2 && c.iter().all(|x| x.chars().count() <= CELL_LETTERS), "{label}: {c:?}");
        }
    }
}
