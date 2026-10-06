//! A small expression language over signals, compiled to a stack bytecode of its own (nothing
//! external). Derived signals, lamp rules, annunciator conditions and interlocks are written in
//! it:
//!
//! ```text
//! deposito_ox.p < 1.5 MPa && motor_1.encendido
//! latch(alarma.presion_baja || reactor.scram, panel.reconocer)
//! if(bus_a.v > 24 V, 1, blink(2))
//! ```
//!
//! - numbers may carry a unit (`1.5 MPa`, `1100 K`, `40 %`, or `2.5[kg/s]` for compound ones) and
//!   become SI;
//! - operators `+ - * / < > <= >= == != && || !` (truth is ≥ 0.5; true = 1, false = 0);
//! - functions `min max clamp abs lerp if sqrt pow exp ln floor round sign sin cos`;
//! - with state (one slot each, kept by the owner of the program): `lag(x, τ)` first order,
//!   `rate(x)` derivative, `blink(hz)`, `latch(set, reset)`, `edge(x)` rising edge (one tick),
//!   `hold(x, s)` true for `s` after `x` rises, `for(x, s)` true once `x` held true `s` seconds,
//!   `toggle(x)` flips on each rising edge of `x` (push buttons in several places, one order).
use crate::{store::{SignalId, Store}, units};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Const(f64),
    Load(SignalId),
    Neg,
    Not,
    Bin(Bin),
    Call(Func, u8),
    State(StateFn, u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Func {
    Min,
    Max,
    Clamp,
    Abs,
    Lerp,
    If,
    Sqrt,
    Pow,
    Exp,
    Ln,
    Floor,
    Round,
    Sign,
    Sin,
    Cos,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum StateFn {
    Lag,
    Rate,
    Blink,
    Latch,
    Edge,
    Hold,
    For,
    Toggle,
}

impl StateFn {
    /// Slots of state it keeps.
    fn slots(self) -> u16 {
        match self {
            StateFn::Rate | StateFn::Hold | StateFn::For | StateFn::Toggle => 2,
            _ => 1,
        }
    }
    fn args(self) -> u8 {
        match self {
            StateFn::Rate | StateFn::Blink | StateFn::Edge | StateFn::Toggle => 1,
            _ => 2,
        }
    }
}

/// A compiled expression. Its state slots live outside (`Program::slots` of them).
#[derive(Clone, Debug, Default)]
pub struct Program {
    ops: Vec<Op>,
    /// Signals it reads (deduplicated).
    pub inputs: Vec<SignalId>,
    /// State slots it needs.
    pub slots: usize,
    /// Depends on time itself (lag, blink, hold...): evaluate every tick even with clean inputs.
    pub timed: bool,
    /// The source, for messages.
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExprError(pub String);

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExprError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '#' | '$')
}

fn unit_char(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '%' | '°' | 'µ' | 'Ω' | '²' | '³')
}

fn lex(src: &str) -> Result<Vec<Tok>, ExprError> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && cs.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let start = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.' || ((cs[i] == 'e' || cs[i] == 'E') && cs.get(i + 1).is_some_and(|n| n.is_ascii_digit() || *n == '-'))) {
                if cs[i] == 'e' || cs[i] == 'E' {
                    i += 1;
                }
                i += 1;
            }
            let text: String = cs[start..i].iter().collect();
            let mut v: f64 = text.parse().map_err(|_| ExprError(format!("número mal escrito: '{text}'")))?;
            // a unit: [compound] right after, or a simple unit word after spaces
            let mut j = i;
            while j < cs.len() && cs[j] == ' ' {
                j += 1;
            }
            if cs.get(i) == Some(&'[') {
                let end = cs[i..].iter().position(|&c| c == ']').ok_or_else(|| ExprError("falta ']' tras la unidad".into()))? + i;
                let u: String = cs[i + 1..end].iter().collect();
                v = units::unit(&u).map_err(|e| ExprError(e.0))?.to_si(v);
                i = end + 1;
            } else if j < cs.len() && unit_char(cs[j]) {
                let mut k = j;
                while k < cs.len() && (unit_char(cs[k]) || cs[k].is_ascii_digit() || (cs[k] == '/' && cs.get(k + 1).is_some_and(|c| unit_char(*c)))) {
                    k += 1;
                }
                // a signal name never follows a number; a unit word is not followed by '.' or '('
                let word: String = cs[j..k].iter().collect();
                let next = cs.get(k).copied();
                if next != Some('.') && next != Some('(') {
                    let u = units::unit(&word).map_err(|e| ExprError(format!("{} en '{src}'", e.0)))?;
                    v = u.to_si(v);
                    i = k;
                }
            }
            out.push(Tok::Num(v));
            continue;
        }
        if ident_char(c) {
            let start = i;
            while i < cs.len() && ident_char(cs[i]) {
                i += 1;
            }
            out.push(Tok::Ident(cs[start..i].iter().collect()));
            continue;
        }
        let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
        let op2 = ["<=", ">=", "==", "!=", "&&", "||"].into_iter().find(|o| *o == two);
        if let Some(o) = op2 {
            out.push(Tok::Op(o));
            i += 2;
            continue;
        }
        let t = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            ',' => Tok::Comma,
            '+' => Tok::Op("+"),
            '-' => Tok::Op("-"),
            '*' => Tok::Op("*"),
            '/' => Tok::Op("/"),
            '<' => Tok::Op("<"),
            '>' => Tok::Op(">"),
            '!' => Tok::Op("!"),
            _ => return Err(ExprError(format!("carácter inesperado '{c}' en '{src}'"))),
        };
        out.push(t);
        i += 1;
    }
    Ok(out)
}

struct Parser<'a, F: FnMut(&str) -> Option<SignalId>> {
    toks: Vec<Tok>,
    at: usize,
    prog: Program,
    resolve: &'a mut F,
}

fn prec(op: &str) -> Option<(u8, Bin)> {
    Some(match op {
        "||" => (1, Bin::Or),
        "&&" => (2, Bin::And),
        "==" => (3, Bin::Eq),
        "!=" => (3, Bin::Ne),
        "<" => (4, Bin::Lt),
        ">" => (4, Bin::Gt),
        "<=" => (4, Bin::Le),
        ">=" => (4, Bin::Ge),
        "+" => (5, Bin::Add),
        "-" => (5, Bin::Sub),
        "*" => (6, Bin::Mul),
        "/" => (6, Bin::Div),
        _ => return None,
    })
}

impl<F: FnMut(&str) -> Option<SignalId>> Parser<'_, F> {
    fn err(&self, what: &str) -> ExprError {
        ExprError(format!("{what} en '{}'", self.prog.source))
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn expr(&mut self, min: u8) -> Result<(), ExprError> {
        self.unary()?;
        while let Some(Tok::Op(o)) = self.peek() {
            let Some((p, b)) = prec(o) else { break };
            if p < min {
                break;
            }
            self.at += 1;
            self.expr(p + 1)?;
            self.prog.ops.push(Op::Bin(b));
        }
        Ok(())
    }

    fn unary(&mut self) -> Result<(), ExprError> {
        match self.next() {
            Some(Tok::Op("-")) => {
                self.unary()?;
                self.prog.ops.push(Op::Neg);
            }
            Some(Tok::Op("!")) => {
                self.unary()?;
                self.prog.ops.push(Op::Not);
            }
            Some(Tok::Num(v)) => self.prog.ops.push(Op::Const(v)),
            Some(Tok::LParen) => {
                self.expr(0)?;
                if self.next() != Some(Tok::RParen) {
                    return Err(self.err("falta ')'"));
                }
            }
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.at += 1;
                    self.call(&name)?;
                } else if name == "true" || name == "cierto" {
                    self.prog.ops.push(Op::Const(1.0));
                } else if name == "false" || name == "falso" {
                    self.prog.ops.push(Op::Const(0.0));
                } else {
                    let id = (self.resolve)(&name).ok_or_else(|| self.err(&format!("señal desconocida '{name}'")))?;
                    if !self.prog.inputs.contains(&id) {
                        self.prog.inputs.push(id);
                    }
                    self.prog.ops.push(Op::Load(id));
                }
            }
            t => return Err(self.err(&format!("se esperaba un valor, no {t:?}"))),
        }
        Ok(())
    }

    fn call(&mut self, name: &str) -> Result<(), ExprError> {
        let mut argc = 0u8;
        if self.peek() == Some(&Tok::RParen) {
            self.at += 1;
        } else {
            loop {
                self.expr(0)?;
                argc += 1;
                match self.next() {
                    Some(Tok::Comma) => continue,
                    Some(Tok::RParen) => break,
                    _ => return Err(self.err("falta ')' o ','")),
                }
            }
        }
        let stateful = match name {
            "lag" => Some(StateFn::Lag),
            "rate" => Some(StateFn::Rate),
            "blink" => Some(StateFn::Blink),
            "latch" => Some(StateFn::Latch),
            "edge" => Some(StateFn::Edge),
            "hold" => Some(StateFn::Hold),
            "for" | "durante" => Some(StateFn::For),
            "toggle" | "alternar" => Some(StateFn::Toggle),
            _ => None,
        };
        if let Some(s) = stateful {
            if argc != s.args() {
                return Err(self.err(&format!("{name} lleva {} argumentos", s.args())));
            }
            let slot = self.prog.slots as u16;
            self.prog.slots += usize::from(s.slots());
            self.prog.timed |= !matches!(s, StateFn::Latch | StateFn::Toggle);
            self.prog.ops.push(Op::State(s, slot));
            return Ok(());
        }
        let (f, want): (Func, &[u8]) = match name {
            "min" => (Func::Min, &[]),
            "max" => (Func::Max, &[]),
            "clamp" => (Func::Clamp, &[3]),
            "abs" => (Func::Abs, &[1]),
            "lerp" => (Func::Lerp, &[3]),
            "if" | "si" => (Func::If, &[3]),
            "sqrt" => (Func::Sqrt, &[1]),
            "pow" => (Func::Pow, &[2]),
            "exp" => (Func::Exp, &[1]),
            "ln" => (Func::Ln, &[1]),
            "floor" => (Func::Floor, &[1]),
            "round" => (Func::Round, &[1]),
            "sign" => (Func::Sign, &[1]),
            "sin" => (Func::Sin, &[1]),
            "cos" => (Func::Cos, &[1]),
            _ => return Err(self.err(&format!("función desconocida '{name}'"))),
        };
        if (want.is_empty() && argc == 0) || (!want.is_empty() && !want.contains(&argc)) {
            return Err(self.err(&format!("número de argumentos de {name}")));
        }
        self.prog.ops.push(Op::Call(f, argc));
        Ok(())
    }
}

/// Compile `src`, resolving every signal name with `resolve`.
pub fn compile(src: &str, mut resolve: impl FnMut(&str) -> Option<SignalId>) -> Result<Program, ExprError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, at: 0, prog: Program { source: src.to_string(), ..Default::default() }, resolve: &mut resolve };
    p.expr(0)?;
    if p.at < p.toks.len() {
        return Err(p.err("sobra algo al final"));
    }
    Ok(p.prog)
}

/// Compile against a store (every name must exist in it).
pub fn compile_in(src: &str, store: &Store) -> Result<Program, ExprError> {
    compile(src, |n| store.find(n))
}

fn truth(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Reusable evaluation stack: one per owner, no allocation once grown.
#[derive(Clone, Debug, Default)]
pub struct Eval {
    stack: Vec<f64>,
}

impl Eval {
    /// Value of `p` now. `state` are its slots (`p.slots` of them, zero at first), `dt` the time
    /// since its last evaluation and `t` the time now (s).
    pub fn run(&mut self, p: &Program, store: &Store, state: &mut [f64], dt: f64, t: f64) -> f64 {
        let st = &mut self.stack;
        st.clear();
        for op in &p.ops {
            match *op {
                Op::Const(v) => st.push(v),
                Op::Load(id) => st.push(store.get(id)),
                Op::Neg => {
                    let v = st.pop().unwrap_or(0.0);
                    st.push(-v);
                }
                Op::Not => {
                    let v = st.pop().unwrap_or(0.0);
                    st.push(truth(v < 0.5));
                }
                Op::Bin(b) => {
                    let y = st.pop().unwrap_or(0.0);
                    let x = st.pop().unwrap_or(0.0);
                    st.push(match b {
                        Bin::Add => x + y,
                        Bin::Sub => x - y,
                        Bin::Mul => x * y,
                        Bin::Div => {
                            if y == 0.0 {
                                0.0
                            } else {
                                x / y
                            }
                        }
                        Bin::Lt => truth(x < y),
                        Bin::Gt => truth(x > y),
                        Bin::Le => truth(x <= y),
                        Bin::Ge => truth(x >= y),
                        Bin::Eq => truth((x - y).abs() < 1e-9),
                        Bin::Ne => truth((x - y).abs() >= 1e-9),
                        Bin::And => truth(x >= 0.5 && y >= 0.5),
                        Bin::Or => truth(x >= 0.5 || y >= 0.5),
                    });
                }
                Op::Call(f, n) => {
                    let n = usize::from(n);
                    let base = st.len() - n;
                    let a = &st[base..];
                    let v = match f {
                        Func::Min => a.iter().copied().fold(f64::INFINITY, f64::min),
                        Func::Max => a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        Func::Clamp => a[0].clamp(a[1].min(a[2]), a[2].max(a[1])),
                        Func::Abs => a[0].abs(),
                        Func::Lerp => a[0] + (a[1] - a[0]) * a[2],
                        Func::If => {
                            if a[0] >= 0.5 {
                                a[1]
                            } else {
                                a[2]
                            }
                        }
                        Func::Sqrt => a[0].max(0.0).sqrt(),
                        Func::Pow => a[0].powf(a[1]),
                        Func::Exp => a[0].exp(),
                        Func::Ln => a[0].max(1e-300).ln(),
                        Func::Floor => a[0].floor(),
                        Func::Round => a[0].round(),
                        Func::Sign => a[0].signum() * truth(a[0] != 0.0),
                        Func::Sin => a[0].sin(),
                        Func::Cos => a[0].cos(),
                    };
                    st.truncate(base);
                    st.push(v);
                }
                Op::State(s, slot) => {
                    let k = usize::from(slot);
                    let n = usize::from(s.args());
                    let base = st.len() - n;
                    let a = [st[base], if n > 1 { st[base + 1] } else { 0.0 }];
                    st.truncate(base);
                    let v = match s {
                        StateFn::Lag => {
                            let tau = a[1].max(1e-6);
                            let y = &mut state[k];
                            *y += (a[0] - *y) * (1.0 - (-dt / tau).exp());
                            *y
                        }
                        StateFn::Rate => {
                            let (prev, init) = (state[k], state[k + 1]);
                            state[k] = a[0];
                            state[k + 1] = 1.0;
                            if init < 0.5 || dt <= 0.0 { 0.0 } else { (a[0] - prev) / dt }
                        }
                        StateFn::Blink => truth((t * a[0]).fract() < 0.5),
                        StateFn::Latch => {
                            if a[1] >= 0.5 {
                                state[k] = 0.0;
                            } else if a[0] >= 0.5 {
                                state[k] = 1.0;
                            }
                            state[k]
                        }
                        StateFn::Edge => {
                            let rise = a[0] >= 0.5 && state[k] < 0.5;
                            state[k] = a[0];
                            truth(rise)
                        }
                        StateFn::Hold => {
                            // [k]: previous input, [k+1]: time left
                            if a[0] >= 0.5 && state[k] < 0.5 {
                                state[k + 1] = a[1];
                            } else {
                                state[k + 1] = (state[k + 1] - dt).max(0.0);
                            }
                            state[k] = a[0];
                            truth(state[k + 1] > 0.0 || a[0] >= 0.5 && a[1] <= 0.0)
                        }
                        StateFn::Toggle => {
                            // [k]: previous input, [k+1]: the state, flipped on each rising edge
                            if a[0] >= 0.5 && state[k] < 0.5 {
                                state[k + 1] = 1.0 - state[k + 1];
                            }
                            state[k] = a[0];
                            state[k + 1]
                        }
                        StateFn::For => {
                            // [k]: how long it has been true
                            if a[0] >= 0.5 {
                                state[k] += dt;
                            } else {
                                state[k] = 0.0;
                            }
                            truth(a[0] >= 0.5 && state[k] >= a[1])
                        }
                    };
                    st.push(v);
                }
            }
        }
        st.pop().unwrap_or(0.0)
    }
}

/// Replace every `$name` in `src` by its value in `vars` (templates: one panel for several
/// engines). Unknown `$names` are left as they are (the compiler reports them).
pub fn substitute(src: &str, vars: &[(String, String)]) -> String {
    if !src.contains('$') {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '$' {
            let start = i + 1;
            let mut j = start;
            while j < cs.len() && (cs[j].is_alphanumeric() || cs[j] == '_') {
                j += 1;
            }
            let name: String = cs[start..j].iter().collect();
            if let Some((_, v)) = vars.iter().find(|(k, _)| *k == name) {
                out.push_str(v);
                i = j;
                continue;
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut s = Store::new();
        s.define_unit("deposito_ox.p", "MPa", 2.0e6).unwrap();
        s.define("motor_1.encendido");
        s.define("x");
        s
    }

    #[test]
    fn units_and_logic() {
        let mut s = store();
        let p = compile_in("deposito_ox.p < 1.5 MPa && motor_1.encendido", &s).unwrap();
        let mut e = Eval::default();
        assert_eq!(e.run(&p, &s, &mut [], 0.1, 0.0), 0.0);
        let m = s.find("motor_1.encendido").unwrap();
        s.set(m, 1.0);
        let d = s.find("deposito_ox.p").unwrap();
        s.set(d, 1.0e6);
        assert_eq!(e.run(&p, &s, &mut [], 0.1, 0.0), 1.0);
        let q = compile_in("2.5[kg/s] * 2 + -1 + max(1, 3, 2) * if(x, 10, 100)", &s).unwrap();
        assert!((e.run(&q, &s, &mut [], 0.1, 0.0) - (5.0 - 1.0 + 300.0)).abs() < 1e-9);
        assert!(compile_in("nadie > 3", &s).is_err());
        assert!(compile_in("(x > 3", &s).is_err());
    }

    #[test]
    fn state_functions() {
        let mut s = store();
        let x = s.find("x").unwrap();
        let mut e = Eval::default();
        // lag: first order toward 1 with τ = 1 s
        let p = compile_in("lag(x, 1 s)", &s).unwrap();
        let mut st = vec![0.0; p.slots];
        s.set(x, 1.0);
        let mut v = 0.0;
        for _ in 0..100 {
            v = e.run(&p, &s, &mut st, 0.01, 0.0);
        }
        assert!((v - (1.0 - (-1.0f64).exp())).abs() < 1e-6);
        // latch: set by x, cleared by reset
        let p = compile_in("latch(x, 0)", &s).unwrap();
        let mut st = vec![0.0; p.slots];
        assert_eq!(e.run(&p, &s, &mut st, 0.1, 0.0), 1.0);
        s.set(x, 0.0);
        assert_eq!(e.run(&p, &s, &mut st, 0.1, 0.0), 1.0);
        // for: true after 0.5 s held
        let p = compile_in("for(x, 0.5 s)", &s).unwrap();
        let mut st = vec![0.0; p.slots];
        s.set(x, 1.0);
        let mut t: f64 = 0.0;
        while e.run(&p, &s, &mut st, 0.1, 0.0) < 0.5 {
            t += 0.1;
            assert!(t < 1.0);
        }
        assert!((t - 0.4).abs() < 1e-9);
        // edge: one tick
        let p = compile_in("edge(x)", &s).unwrap();
        let mut st = vec![0.0; p.slots];
        assert_eq!(e.run(&p, &s, &mut st, 0.1, 0.0), 1.0);
        assert_eq!(e.run(&p, &s, &mut st, 0.1, 0.0), 0.0);
    }

    #[test]
    fn templates() {
        let v = vec![("motor".to_string(), "motor_2".to_string())];
        assert_eq!(substitute("$motor.t_pared > 1100 K", &v), "motor_2.t_pared > 1100 K");
    }
}
