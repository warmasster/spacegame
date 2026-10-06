//! Procedures: what a crew does to get something out of a ship — power it up, start its reactor,
//! shut its ramp, fill a room — written once, as data (`assets/defs/procedimientos.jsonc`), for
//! every ship that has what the procedure works. Each says:
//!
//! - its steps (`pasos`): controls set, pressed or turned, by their id (`"techo/apu"`) or by the
//!   signal they write (`"senal:apu.marcha"`: whichever panel carries it on this ship); a guard
//!   over a control is lifted on the way; waits (`espera`, `hasta` a condition);
//! - the condition that says it is done (`hecho`): an expression over the ship's signals, in the
//!   language of lamp rules and derived signals (`lunar_signals::expr`);
//! - how long a crew should have to wait for it at most (`max`): its budget of fun, in seconds;
//! - what is done before it (`tras`), where it starts from (`preparar`: steps that are not
//!   timed, and the rig's — a room's air, a machine's state — for whoever measures it), and the
//!   world it is done in (`mundo.altura`: in the air);
//! - a family (`cada`): one procedure for every control, machine, joint or room of a ship whose
//!   name fits a pattern (`"mando:sala_*/venteo"`, `"modelo:giroscopo"`); what the `*` stand for
//!   are `{1}`, `{2}`... in its texts, and `{orden:marcha}` is the signal that orders a machine.
//!
//! A procedure applies to a ship when everything it names is there: the same data runs the
//! Alcotán and whatever is built after it. Nothing here costs anything in play: a ship knows
//! nothing of its procedures. Whoever wants them loads the registry, binds it to a ship
//! (`Registry::bind`: names to indices, conditions compiled, once) and runs a `Run` tick by tick,
//! which works the panels as a hand does (nothing allocated per tick). `drive` does that
//! headless for the test that holds every ship to its budgets (`tests/procedimientos.rs`).
//!
//! The same procedures followed by hand are checklists (`Checklist`): `Bound::lines` says each
//! step as a hand needs it (the control to work, what to do with it, the line in Spanish) and
//! `Checklist::poll` looks at the ship when asked — never by itself — and says which steps are
//! done and which is next.
use crate::{
    atmos::{Air, R},
    ship::{Ship, TICK, World},
};
use glam::Vec3;
use lunar_controls::{Event, Intent, Mods, intent::F_PRESSED};
use lunar_core::{
    defs::{self, DefError},
    structure::state::Structure,
};
use lunar_signals::{Eval, Program, Q, SignalId, Writer, compile_in};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

/// The registry as written (`procedimientos.jsonc`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    /// What a hand takes to work one control (s).
    #[serde(default)]
    pub mano: Option<Q>,
    /// Machine models with no start-up nor transition of their own, and why: every model of
    /// every ship is either here or says when it is on its way somewhere (`Machine::settling`).
    #[serde(default)]
    pub sin_transitorio: BTreeMap<String, String>,
    /// What is let off, ship by ship, and why (never silently).
    #[serde(default)]
    pub exentos: Vec<Exempt>,
    pub procedimientos: Vec<ProcDef>,
}

/// Things of a ship (`que`: instances of procedures, machines, joints, by id) that no budget is
/// held to, and the reason.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exempt {
    pub nave: String,
    pub que: Vec<String>,
    pub motivo: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcDef {
    pub id: String,
    pub nombre: String,
    /// A family: `"<what>:<pattern>"`, what being `mando`, `senal`, `modelo`, `maquina`,
    /// `articulacion`, `cierre`, `compartimento` or `anclaje`.
    #[serde(default)]
    pub cada: Option<String>,
    /// Controls (`panel/mando`) and signals the ship must have besides those its steps name.
    #[serde(default)]
    pub requiere: Vec<String>,
    #[serde(default)]
    pub tras: Vec<String>,
    #[serde(default)]
    pub mundo: Option<WorldDef>,
    #[serde(default)]
    pub preparar: Vec<StepDef>,
    #[serde(default)]
    pub pasos: Vec<StepDef>,
    pub hecho: String,
    pub max: Q,
    /// What was bent to get it there, for whoever reads the table.
    #[serde(default)]
    pub nota: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDef {
    /// Height over the ground it is done at (in the air: the gear comes up).
    #[serde(default)]
    pub altura: Option<Q>,
}

/// One step: exactly one of its verbs.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepDef {
    /// Bring a control to `a` (a number, or a quantity: `"100 %"`).
    #[serde(default)]
    pub poner: Option<String>,
    #[serde(default)]
    pub a: Option<Value>,
    /// Press a control and let it go.
    #[serde(default)]
    pub pulsar: Option<String>,
    /// Turn a wheel or a selector `muescas` notches.
    #[serde(default)]
    pub girar: Option<String>,
    #[serde(default)]
    pub muescas: Option<f32>,
    /// Hold axis `eje` of a sprung lever at `a` (a seat's key held down).
    #[serde(default)]
    pub mantener: Option<String>,
    #[serde(default)]
    pub eje: Option<u8>,
    #[serde(default)]
    pub espera: Option<Q>,
    /// Wait until a condition holds, `max` at most.
    #[serde(default)]
    pub hasta: Option<String>,
    #[serde(default)]
    pub max: Option<Q>,
    /// The rig's (`preparar` only): a room's air set straight to pressure `a` (`"*"`: every
    /// room), with `co2` of it carbon dioxide.
    #[serde(default)]
    pub aire: Option<String>,
    #[serde(default)]
    pub co2: Option<Q>,
    /// The rig's: a machine's state set straight to `a` (its `Machine::save` figures).
    #[serde(default)]
    pub estado: Option<String>,
    /// Its line on a checklist, in Spanish (else it is made from what the control is called).
    #[serde(default)]
    pub texto: Option<String>,
    /// What says it is done, for a checklist: a condition. Without it a control set is done
    /// while it stands there and a condition waited for while it holds; a press says it for
    /// what it has no lasting sign of (a push button: what its order becomes).
    #[serde(default)]
    pub hecho: Option<String>,
}

/// What a step does, on one ship.
#[derive(Clone, Debug)]
pub enum Act {
    Set {
        control: usize,
        value: f64,
    },
    Press {
        control: usize,
    },
    Turn {
        control: usize,
        notches: f32,
    },
    Hold {
        control: usize,
        axis: u8,
        value: f64,
    },
    /// A hand on something that is on no panel (a clamp's lever): a signal the data owns.
    Signal {
        signal: SignalId,
        value: f64,
    },
    Wait {
        secs: f64,
    },
    /// (its condition, where its state is in the run's slots, how long at most)
    Until {
        cond: Program,
        slot: usize,
        max: f64,
    },
    Air {
        room: usize,
        pressure: f64,
        co2: f64,
    },
    State {
        machine: usize,
        state: Vec<f64>,
    },
}

#[derive(Clone, Debug)]
pub struct Step {
    pub act: Act,
    /// What the data says of it (a checklist's line), if anything.
    pub text: Option<String>,
    /// What the data says ticks it on a checklist (and where its state is in the slots).
    pub done: Option<(Program, usize)>,
}

/// What a step asks of whoever does it by hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum What {
    /// Bring the control to this value (SI).
    Set(f64),
    /// Press it and let it go.
    Press,
    /// Turn it so many notches.
    Turn(f32),
    /// Hold its axis there.
    Hold { axis: u8, value: f64 },
    /// Wait so many seconds.
    Wait(f64),
    /// Wait for something to come true.
    Until,
}

/// One line of a checklist: a step as whoever does it by hand needs it.
#[derive(Clone, Debug)]
pub struct Line {
    /// The control to work: its place in `Ship::panels.controls`. None: nothing to touch (a
    /// wait), or something that is on no panel (a clamp's lever).
    pub control: Option<usize>,
    /// Its id (`panel/mando`; of something on no panel, its signal), and what its panel is called.
    pub id: String,
    pub panel: String,
    /// The guard over it (a control too), to lift first while it is down.
    pub cover: Option<usize>,
    pub what: What,
    /// The line, in Spanish: what the data says, else what the control is called and where
    /// it goes ("Contactor batería 1: CONEC").
    pub text: String,
}

/// A procedure on one ship: everything it names found, its conditions compiled.
#[derive(Clone, Debug)]
pub struct Bound {
    /// The procedure and what its family's pattern stood for: `ventear:cabina`.
    pub id: String,
    pub proc: String,
    pub name: String,
    /// Seconds a crew should wait for it at most.
    pub budget: f64,
    /// The instances (ids) done before it, on this ship.
    pub after: Vec<String>,
    pub altitude: Option<f64>,
    pub prepare: Vec<Step>,
    pub steps: Vec<Step>,
    pub done: Program,
    done_slot: usize,
    slots: usize,
    pub note: Option<String>,
}

/// What a registry is on one ship: the procedures that apply, in the registry's order, and
/// those that do not with what they miss.
#[derive(Clone, Debug, Default)]
pub struct Bindings {
    pub list: Vec<Bound>,
    pub missing: Vec<(String, String)>,
}

impl Bindings {
    /// The procedure called `id` (`reactor_linea:reactor`, `ventear:cabina`, `energia`).
    pub fn get(&self, id: &str) -> Option<&Bound> {
        self.list.iter().find(|b| b.id == id)
    }
}

impl Registry {
    pub fn load(dir: &Path) -> Result<Registry, DefError> {
        let path = defs::file(dir, "procedimientos");
        let reg: Registry = defs::load(&path)?;
        // (ids are what `tras` and the exemptions name: one each)
        for (k, p) in reg.procedimientos.iter().enumerate() {
            if reg.procedimientos[..k].iter().any(|q| q.id == p.id) {
                return Err(DefError::new(path.display().to_string(), format!("procedimiento '{}' repetido", p.id)));
            }
            for t in &p.tras {
                let t = t.strip_suffix('?').unwrap_or(t);
                if !reg.procedimientos.iter().any(|q| q.id == t) {
                    return Err(DefError::new(path.display().to_string(), format!("{}: 'tras' nombra un procedimiento que no hay: '{t}'", p.id)));
                }
            }
        }
        Ok(reg)
    }

    /// Seconds a hand takes to work one control.
    pub fn hand(&self) -> f64 {
        self.mano.as_ref().and_then(|q| q.si_as("s").ok()).unwrap_or(0.3)
    }

    /// Why `what` (an instance, a machine, a joint) of ship `ship` is let off, if it is.
    pub fn exempt(&self, ship: &str, what: &str) -> Option<&str> {
        self.exentos.iter().find(|e| e.nave == ship && e.que.iter().any(|q| q == what)).map(|e| e.motivo.as_str())
    }

    /// Every procedure that applies to `ship`, bound to it.
    pub fn bind(&self, ship: &Ship) -> Bindings {
        let mut out = Bindings::default();
        // (each with what its family's pattern stood for, to find what goes before it)
        let mut caps: Vec<Vec<String>> = Vec::new();
        for p in &self.procedimientos {
            let members = match &p.cada {
                Some(c) => match family(ship, c) {
                    Ok(m) => m,
                    Err(e) => {
                        out.missing.push((p.id.clone(), e));
                        continue;
                    }
                },
                None => vec![(Vec::new(), None)],
            };
            if members.is_empty() {
                out.missing.push((p.id.clone(), format!("nada en la nave es '{}'", p.cada.as_deref().unwrap_or(""))));
            }
            for (c, machine) in members {
                let id = instance(&p.id, &c);
                match bind_one(ship, p, &id, &c, machine) {
                    Ok(b) => {
                        out.list.push(b);
                        caps.push(c);
                    }
                    Err(e) => out.missing.push((id, e)),
                }
            }
        }
        // what goes before each: of a family, its member with the same pattern, else all of it.
        // A procedure whose `tras` does not apply to this ship does not apply either, unless it
        // was written with a `?` ("if the ship has it": a ramp to shut before filling a room)
        let mut k = 0;
        while k < out.list.len() {
            let def = self.procedimientos.iter().find(|p| p.id == out.list[k].proc);
            let wanted: Vec<String> = def.map(|d| d.tras.clone()).unwrap_or_default();
            let (mut after, mut lacks) = (Vec::new(), None);
            for t in wanted {
                let (t, optional) = t.strip_suffix('?').map_or((t.as_str(), false), |t| (t, true));
                let same = instance(t, &caps[k]);
                let found: Vec<String> = match out.list.iter().find(|b| b.proc == t && b.id == same) {
                    Some(b) => vec![b.id.clone()],
                    None => out.list.iter().filter(|b| b.proc == t).map(|b| b.id.clone()).collect(),
                };
                if found.is_empty() && !optional {
                    lacks = Some(t.to_string());
                }
                after.extend(found);
            }
            match lacks {
                Some(t) => {
                    out.missing.push((out.list[k].id.clone(), format!("antes va '{t}', que esta nave no tiene")));
                    out.list.remove(k);
                    caps.remove(k);
                    // (what waited for it is looked at again)
                    k = 0;
                }
                None => {
                    out.list[k].after = after;
                    k += 1;
                }
            }
        }
        out
    }
}

fn instance(proc: &str, caps: &[String]) -> String {
    if caps.is_empty() { proc.to_string() } else { format!("{proc}:{}", caps.join(":")) }
}

/// `text` against a pattern with `*`: what each `*` stood for.
fn glob(pat: &str, text: &str) -> Option<Vec<String>> {
    let segs: Vec<&str> = pat.split('*').collect();
    if segs.len() == 1 {
        return (pat == text).then(Vec::new);
    }
    let mut caps = Vec::new();
    let mut rest = text.strip_prefix(segs[0])?;
    for (i, seg) in segs.iter().enumerate().skip(1) {
        if i + 1 == segs.len() {
            caps.push(rest.strip_suffix(seg)?.to_string());
        } else {
            let at = rest.find(seg)?;
            caps.push(rest[..at].to_string());
            rest = &rest[at + seg.len()..];
        }
    }
    Some(caps)
}

/// The members of a family on `ship`: what the pattern's `*` stood for in each and, for the
/// families of machines, which machine it is.
fn family(ship: &Ship, cada: &str) -> Result<Vec<(Vec<String>, Option<usize>)>, String> {
    let (what, pat) = cada.split_once(':').ok_or_else(|| format!("'cada' se escribe \"qué:patrón\": '{cada}'"))?;
    let kind = &ship.kind;
    let names = |it: &mut dyn Iterator<Item = &str>| -> Vec<(Vec<String>, Option<usize>)> { it.filter_map(|n| glob(pat, n)).map(|c| (c, None)).collect() };
    let mut out = match what {
        "mando" => names(&mut ship.panels.controls.iter().map(|c| c.id.as_str())),
        "senal" => names(&mut ship.store.ids().map(|i| ship.store.name(i))),
        "articulacion" => names(&mut kind.joints.iter().map(|j| j.id.as_str())),
        "cierre" => names(&mut kind.closures.iter().map(|c| c.id.as_str())),
        "compartimento" => names(&mut kind.compartments.iter().map(|c| c.id.as_str())),
        "anclaje" => names(&mut kind.clamps.iter().map(|c| c.id.as_str())),
        "maquina" => kind.machines.iter().enumerate().filter_map(|(k, m)| glob(pat, &m.id).map(|c| (c, Some(k)))).collect(),
        // (by what the machine is: {1} is its id)
        "modelo" => ship.machines.iter().enumerate().filter(|(_, m)| m.m.kind() == pat).map(|(k, _)| (vec![kind.machines[k].id.clone()], Some(k))).collect(),
        _ => return Err(format!("'cada': no hay familias de '{what}' (mando, senal, modelo, maquina, articulacion, cierre, compartimento, anclaje)")),
    };
    out.dedup();
    Ok(out)
}

/// `text` with `{1}`, `{2}`... and `{orden:<rol>}` filled in.
fn fill(text: &str, caps: &[String], ship: &Ship, machine: Option<usize>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}').ok_or_else(|| format!("falta '}}' en '{text}'"))? + open;
        let key = &rest[open + 1..close];
        if let Some(role) = key.strip_prefix("orden:") {
            let k = machine.ok_or_else(|| format!("'{{{key}}}' solo vale en una familia de máquinas ('{text}')"))?;
            let m = &ship.kind.machines[k];
            out.push_str(&m.def.ordenes.get(role).and_then(Value::as_str).map_or_else(|| format!("{}.{role}", m.id), str::to_string));
        } else {
            let n: usize = key.parse().map_err(|_| format!("'{{{key}}}' en '{text}': se esperaba {{1}}, {{2}}... u {{orden:rol}}"))?;
            out.push_str(caps.get(n.wrapping_sub(1)).ok_or_else(|| format!("'{{{n}}}' en '{text}': el patrón de la familia no tiene tantos '*'"))?);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

enum Target {
    Control(usize),
    Signal(SignalId),
}

/// What a step names: a control by its id, or (`senal:`) whatever writes a signal — the control
/// bound to it, or nobody (a signal the data owns: a hand sets it).
fn target(ship: &Ship, name: &str) -> Result<Target, String> {
    match name.strip_prefix("senal:") {
        Some(sig) => {
            let id = ship.store.find(sig).ok_or_else(|| format!("no hay señal '{sig}'"))?;
            match &ship.store.meta(id).writer {
                Writer::Control(k) => Ok(Target::Control(*k as usize)),
                Writer::None => Ok(Target::Signal(id)),
                w => Err(format!("la señal '{sig}' no la manda un mando (la escribe {w:?})")),
            }
        }
        None => ship.panels.controls.iter().position(|c| c.id == name).map(Target::Control).ok_or_else(|| format!("no hay mando '{name}'")),
    }
}

fn control(ship: &Ship, name: &str) -> Result<usize, String> {
    match target(ship, name)? {
        Target::Control(k) => Ok(k),
        Target::Signal(_) => Err(format!("'{name}' no es un mando")),
    }
}

fn number(v: &Option<Value>, what: &str) -> Result<f64, String> {
    match v {
        Some(Value::Number(n)) => n.as_f64().ok_or_else(|| format!("{what}: número fuera de rango")),
        Some(Value::Bool(b)) => Ok(f64::from(u8::from(*b))),
        Some(Value::String(s)) => Q::S(s.clone()).si().map_err(|e| format!("{what}: {}", e.0)),
        Some(_) => Err(format!("{what}: 'a' ha de ser un número o una cantidad")),
        None => Err(format!("{what}: falta 'a' (a qué valor)")),
    }
}

fn seconds(q: &Option<Q>, or: f64) -> Result<f64, String> {
    q.as_ref().map_or(Ok(or), |q| q.si_as("s").map_err(|e| e.0))
}

fn bind_one(ship: &Ship, p: &ProcDef, id: &str, caps: &[String], machine: Option<usize>) -> Result<Bound, String> {
    let f = |text: &str| fill(text, caps, ship, machine);
    for r in &p.requiere {
        let r = f(r)?;
        if r.contains('/') {
            control(ship, &r)?;
        } else if ship.store.find(&r).is_none() {
            return Err(format!("no hay señal '{r}'"));
        }
    }
    let mut slots = 0;
    let mut steps = |list: &[StepDef], rig: bool| -> Result<Vec<Step>, String> {
        let mut out = Vec::new();
        for d in list {
            let verbs = [d.poner.is_some(), d.pulsar.is_some(), d.girar.is_some(), d.mantener.is_some(), d.espera.is_some(), d.hasta.is_some(), d.aire.is_some(), d.estado.is_some()];
            if verbs.iter().filter(|v| **v).count() != 1 {
                return Err("cada paso lleva un verbo y solo uno (poner, pulsar, girar, mantener, espera, hasta, aire, estado)".to_string());
            }
            let text = d.texto.as_deref().map(&f).transpose()?;
            let ticks = d.hecho.as_deref().map(&f).transpose()?;
            let mut push = |act: Act, slots: &mut usize| -> Result<(), String> {
                let done = match &ticks {
                    Some(src) => {
                        let prog = compile_in(src, &ship.store).map_err(|e| e.0)?;
                        let at = *slots;
                        *slots += prog.slots;
                        Some((prog, at))
                    }
                    None => None,
                };
                out.push(Step { act, text: text.clone(), done });
                Ok(())
            };
            if let Some(n) = &d.poner {
                let n = f(n)?;
                let value = number(&d.a, &n)?;
                if n.contains('*') {
                    // every control whose id (or whose signal's name) fits: all the contactors
                    let (by_signal, pat) = n.strip_prefix("senal:").map_or((false, n.as_str()), |p| (true, p));
                    let found: Vec<usize> = (0..ship.panels.controls.len())
                        .filter(|&k| {
                            let c = &ship.panels.controls[k];
                            if by_signal { ship.store.meta(c.sig).writer == Writer::Control(k as u32) && glob(pat, ship.store.name(c.sig)).is_some() } else { glob(pat, &c.id).is_some() }
                        })
                        .collect();
                    if found.is_empty() {
                        return Err(format!("ningún mando es '{n}'"));
                    }
                    for control in found {
                        push(Act::Set { control, value }, &mut slots)?;
                    }
                    continue;
                }
                let act = match target(ship, &n)? {
                    Target::Control(control) => Act::Set { control, value },
                    Target::Signal(signal) => Act::Signal { signal, value },
                };
                push(act, &mut slots)?;
            } else if let Some(n) = &d.pulsar {
                push(Act::Press { control: control(ship, &f(n)?)? }, &mut slots)?;
            } else if let Some(n) = &d.girar {
                push(Act::Turn { control: control(ship, &f(n)?)?, notches: d.muescas.ok_or("girar: faltan las 'muescas'")? }, &mut slots)?;
            } else if let Some(n) = &d.mantener {
                let n = f(n)?;
                push(Act::Hold { control: control(ship, &n)?, axis: d.eje.unwrap_or(0), value: number(&d.a, &n)? }, &mut slots)?;
            } else if d.espera.is_some() {
                push(Act::Wait { secs: seconds(&d.espera, 0.0)? }, &mut slots)?;
            } else if let Some(c) = &d.hasta {
                let cond = compile_in(&f(c)?, &ship.store).map_err(|e| e.0)?;
                let slot = slots;
                slots += cond.slots;
                push(Act::Until { cond, slot, max: seconds(&d.max, 3600.0)? }, &mut slots)?;
            } else if let Some(room) = &d.aire {
                if !rig {
                    return Err("'aire' es del banco de pruebas: solo en 'preparar'".to_string());
                }
                let room = f(room)?;
                let pressure = number(&d.a, "aire")?;
                let co2 = d.co2.as_ref().map_or(Ok(0.0), |q| q.si_as("Pa").map_err(|e| e.0))?;
                let rooms: Vec<usize> = if room == "*" { (0..ship.kind.compartments.len()).collect() } else { vec![ship.kind.compartments.iter().position(|c| c.id == room).ok_or_else(|| format!("no hay compartimento '{room}'"))?] };
                for room in rooms {
                    push(Act::Air { room, pressure, co2 }, &mut slots)?;
                }
            } else if let Some(m) = &d.estado {
                if !rig {
                    return Err("'estado' es del banco de pruebas: solo en 'preparar'".to_string());
                }
                let m = f(m)?;
                let state: Vec<f64> = d.a.clone().and_then(|v| serde_json::from_value(v).ok()).ok_or("estado: 'a' es la lista de cifras del estado de la máquina")?;
                // (one machine by its id, or every machine of a model)
                let found: Vec<usize> = match m.strip_prefix("modelo:") {
                    Some(model) => (0..ship.machines.len()).filter(|&k| ship.machines[k].m.kind() == model).collect(),
                    None => ship.kind.machines.iter().position(|x| x.id == m).into_iter().collect(),
                };
                if found.is_empty() {
                    return Err(format!("no hay máquina '{m}'"));
                }
                for machine in found {
                    push(Act::State { machine, state: state.clone() }, &mut slots)?;
                }
            }
        }
        Ok(out)
    };
    let prepare = steps(&p.preparar, true)?;
    let steps = steps(&p.pasos, false)?;
    let done = compile_in(&f(&p.hecho)?, &ship.store).map_err(|e| e.0)?;
    let done_slot = slots;
    slots += done.slots;
    let altitude = p.mundo.as_ref().and_then(|w| w.altura.as_ref()).map(|q| q.si_as("m").map_err(|e| e.0)).transpose()?;
    Ok(Bound { id: id.to_string(), proc: p.id.clone(), name: f(&p.nombre)?, budget: p.max.si_as("s").map_err(|e| e.0)?, after: Vec::new(), altitude, prepare, steps, done, done_slot, slots, note: p.nota.clone() })
}

impl Bound {
    /// Step `i` as whoever does it by hand needs it: the control to work, what to do with it,
    /// its line in Spanish. (Made when asked: for whoever lays a checklist out, not per frame.)
    pub fn line(&self, ship: &Ship, i: usize) -> Option<Line> {
        let step = self.steps.get(i)?;
        let on = |k: usize, what: What, does: String| {
            let c = &ship.panels.controls[k];
            let panel = &ship.kind.panels[c.panel];
            // (a wheel says its own name with its figure: not twice)
            let label = panel.def.mandos[c.index].label();
            let text = if does.starts_with(label) { does } else { format!("{label}: {does}") };
            Line { control: Some(k), id: c.id.clone(), panel: panel.def.name.clone(), cover: c.cover, what, text }
        };
        let off = |id: &str, what: What, text: String| Line { control: None, id: id.to_string(), panel: String::new(), cover: None, what, text };
        let mut line = match &step.act {
            Act::Set { control, value } => {
                // (where it goes, as the control itself says it: "CONEC", "100,0 %")
                let c = &ship.panels.controls[*control];
                let (mut st, mut to) = (c.st, String::new());
                c.mech.set(&mut st, *value);
                c.mech.describe(&st, &mut to);
                on(*control, What::Set(*value), to)
            }
            Act::Press { control } => on(*control, What::Press, "pulsar".to_string()),
            Act::Turn { control, notches } => on(*control, What::Turn(*notches), format!("girar {} muescas {}", notches.abs(), if *notches >= 0.0 { "a más" } else { "a menos" })),
            Act::Hold { control, axis, value } => on(*control, What::Hold { axis: *axis, value: *value }, "mantener".to_string()),
            Act::Signal { signal, value } => off(ship.store.name(*signal), What::Set(*value), format!("{}: {}", ship.store.name(*signal), if *value >= 0.5 { "accionar" } else { "soltar" })),
            Act::Wait { secs } => off("", What::Wait(*secs), format!("Esperar {secs} s")),
            Act::Until { cond, .. } => off("", What::Until, format!("Esperar: {}", cond.source)),
            // (the rig's: never a hand's, never on a checklist)
            Act::Air { .. } | Act::State { .. } => return None,
        };
        if let Some(text) = &step.text {
            line.text.clone_from(text);
        }
        Some(line)
    }

    /// Every step as a checklist shows it.
    pub fn lines(&self, ship: &Ship) -> Vec<Line> {
        (0..self.steps.len()).filter_map(|i| self.line(ship, i)).collect()
    }

    /// The world it is done in.
    pub fn world(&self) -> World {
        World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: self.altitude.unwrap_or(0.0), ..World::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Getting to where it starts from (not timed).
    Preparing,
    Working,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Prepare,
    Steps,
    Check,
    Done,
    Failed,
}

enum Progress {
    Next,
    Busy,
    Fail(String),
}

/// One procedure being done on one ship. `tick` once before each tick of the ship's systems.
pub struct Run {
    phase: Phase,
    at: usize,
    stage: u8,
    timer: f64,
    hand: f64,
    /// Seconds since its first timed step.
    pub elapsed: f64,
    /// Why it failed.
    pub why: String,
    slots: Vec<f64>,
    eval: Eval,
}

impl Run {
    /// `hand`: seconds a hand takes to work one control (`Registry::hand`).
    pub fn new(b: &Bound, hand: f64) -> Run {
        Run { phase: Phase::Prepare, at: 0, stage: 0, timer: 0.0, hand, elapsed: 0.0, why: String::new(), slots: vec![0.0; b.slots.max(1)], eval: Eval::default() }
    }

    /// The step being done (of its timed ones), for a checklist.
    pub fn step(&self) -> Option<usize> {
        (self.phase == Phase::Steps).then_some(self.at)
    }

    /// What is due now is done (a control worked, a condition looked at); `dt`: the tick that
    /// follows. Nothing allocated unless it fails.
    pub fn tick(&mut self, b: &Bound, ship: &mut Ship, s: &Structure, dt: f64) -> Status {
        loop {
            let list = match self.phase {
                Phase::Prepare => &b.prepare,
                Phase::Steps => &b.steps,
                Phase::Check => {
                    let n = b.done.slots;
                    if self.eval.run(&b.done, &ship.store, &mut self.slots[b.done_slot..b.done_slot + n], dt, ship.t) >= 0.5 {
                        self.phase = Phase::Done;
                        return Status::Done;
                    }
                    break;
                }
                Phase::Done => return Status::Done,
                Phase::Failed => return Status::Failed,
            };
            let Some(step) = list.get(self.at) else {
                let prepared = self.phase == Phase::Prepare;
                (self.phase, self.at) = (if prepared { Phase::Steps } else { Phase::Check }, 0);
                if prepared {
                    // (a tick goes by before it is timed: the ship's signals say where it starts)
                    return Status::Preparing;
                }
                continue;
            };
            match self.work(&step.act, ship, s, dt) {
                Progress::Next => {
                    self.at += 1;
                    (self.stage, self.timer) = (0, 0.0);
                }
                Progress::Busy => break,
                Progress::Fail(why) => {
                    self.why = why;
                    self.phase = Phase::Failed;
                    return Status::Failed;
                }
            }
        }
        if self.phase == Phase::Prepare {
            return Status::Preparing;
        }
        self.elapsed += dt;
        Status::Working
    }

    fn work(&mut self, act: &Act, ship: &mut Ship, s: &Structure, dt: f64) -> Progress {
        let hand = self.hand;
        match act {
            Act::Wait { secs } => self.wait(*secs, dt),
            Act::Until { cond, slot, max } => {
                if self.eval.run(cond, &ship.store, &mut self.slots[*slot..*slot + cond.slots], dt, ship.t) >= 0.5 {
                    return Progress::Next;
                }
                self.timer += dt;
                if self.timer > *max { Progress::Fail(format!("no llega en {max} s: {}", cond.source)) } else { Progress::Busy }
            }
            Act::Air { room, pressure, co2 } => {
                let v = ship.atmos.volume[*room];
                let mut air = if *pressure > 0.0 { Air::standard(*pressure, v) } else { Air { t: 250.0, ..Default::default() } };
                // (so much of it carbon dioxide, in place of nitrogen)
                let n = (co2 * v / (R * air.t.max(1.0))).min(air.n2);
                air.n2 -= n;
                air.co2 += n;
                ship.atmos.air[*room] = air;
                Progress::Next
            }
            Act::State { machine, state } => {
                ship.machines[*machine].m.load(state);
                Progress::Next
            }
            Act::Signal { signal, value } => match self.stage {
                0 => {
                    ship.store.set(*signal, *value);
                    ship.touch();
                    self.stage = 1;
                    Progress::Busy
                }
                _ => self.wait(hand, dt),
            },
            Act::Hold { control, axis, value } => self.act(ship, s, *control, &Intent::Axis { axis: *axis, value: *value }),
            Act::Set { control, value } => {
                let c = &ship.panels.controls[*control];
                if self.stage == 0 && (c.mech.value(&c.st) - value).abs() < 1e-9 {
                    return Progress::Next;
                }
                match self.stage {
                    0..=2 => self.guard(ship, s, *control, dt),
                    3 => {
                        let p = self.act(ship, s, *control, &Intent::Set { value: *value });
                        self.stage = 4;
                        if let Progress::Fail(_) = p { p } else { Progress::Busy }
                    }
                    _ => self.wait(hand, dt),
                }
            }
            Act::Press { control } => match self.stage {
                0..=2 => self.guard(ship, s, *control, dt),
                3 => {
                    let p = self.act(ship, s, *control, &Intent::Press { elem: 0 });
                    self.stage = 4;
                    if let Progress::Fail(_) = p { p } else { Progress::Busy }
                }
                4 => {
                    if let Progress::Next = self.wait(hand * 0.6, dt) {
                        let kind = ship.kind.clone();
                        ship.panels.intent(*control, &Intent::Release, s, &kind, &ship.store);
                        (self.stage, self.timer) = (5, 0.0);
                    }
                    Progress::Busy
                }
                _ => self.wait(hand * 0.4, dt),
            },
            Act::Turn { control, notches } => match self.stage {
                0..=2 => self.guard(ship, s, *control, dt),
                3 => {
                    let p = self.act(ship, s, *control, &Intent::Turn { notches: *notches, rate: 4.0, m: Mods::default() });
                    self.stage = 4;
                    if let Progress::Fail(_) = p { p } else { Progress::Busy }
                }
                _ => self.wait(hand, dt),
            },
        }
    }

    fn wait(&mut self, secs: f64, dt: f64) -> Progress {
        if self.timer >= secs - 1e-9 {
            return Progress::Next;
        }
        self.timer += dt;
        Progress::Busy
    }

    /// Stages 0 to 2 of working a control: the guard over it, if it is down, lifted first.
    fn guard(&mut self, ship: &mut Ship, s: &Structure, control: usize, dt: f64) -> Progress {
        let Some(cover) = ship.panels.controls[control].cover else {
            self.stage = 3;
            return Progress::Busy;
        };
        let open = |ship: &Ship| {
            let c = &ship.panels.controls[cover];
            c.mech.value(&c.st) >= 0.5
        };
        let kind = ship.kind.clone();
        match self.stage {
            0 if open(ship) => self.stage = 3,
            0 => {
                if let Progress::Fail(why) = self.act(ship, s, cover, &Intent::Press { elem: 0 }) {
                    return Progress::Fail(why);
                }
                (self.stage, self.timer) = (1, 0.0);
            }
            1 => {
                if let Progress::Next = self.wait(self.hand * 0.5, dt) {
                    ship.panels.intent(cover, &Intent::Release, s, &kind, &ship.store);
                    (self.stage, self.timer) = (2, 0.0);
                }
            }
            _ => {
                if let Progress::Next = self.wait(self.hand * 0.5, dt) {
                    if !open(ship) {
                        return Progress::Fail(format!("{}: la tapa no se levanta", ship.panels.controls[cover].id));
                    }
                    (self.stage, self.timer) = (3, 0.0);
                }
            }
        }
        Progress::Busy
    }

    /// A hand's intent on a control: refused (a guard, an interlock, a dead panel), it fails
    /// with the reason the ship gives.
    fn act(&mut self, ship: &mut Ship, s: &Structure, control: usize, i: &Intent) -> Progress {
        let kind = ship.kind.clone();
        let gate = ship.panels.gate(control, s, &kind);
        if !gate.working {
            return Progress::Fail(format!("{}: averiado", ship.panels.controls[control].id));
        }
        let o = ship.panels.intent(control, i, s, &kind, &ship.store);
        ship.touch();
        match o.event {
            Some(Event::Blocked(b)) => Progress::Fail(format!("{}: {}", ship.panels.controls[control].id, ship.panels.reason(b))),
            _ => Progress::Next,
        }
    }
}

/// Where a procedure followed by hand stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// This step is the next to do.
    Step(usize),
    /// Every step is done; what the procedure is for has not come yet (a reactor climbing).
    Waiting,
    /// What says the procedure is done (`hecho`) holds.
    Done,
}

/// A procedure followed by hand — a checklist on a datapad. It works nothing and costs nothing
/// between looks: `poll` reads the ship's controls and signals when whoever shows it asks, and
/// says which steps are done and which is next. A step is ticked while what it asks holds (a
/// control at its value, a condition true: undone, it unticks); a press, a turn or a wait leave
/// nothing behind to look at, so they are ticked when seen done while they are the next step
/// (or by the condition the data gives them: `"hecho"` on the step).
pub struct Checklist {
    ticked: Vec<bool>,
    /// The next step, when it became so (ship time) and how its control stood then.
    at: usize,
    since: f64,
    was: f64,
    /// Ship time at the last look.
    t: f64,
    slots: Vec<f64>,
    eval: Eval,
}

impl Checklist {
    pub fn new(b: &Bound, ship: &Ship) -> Checklist {
        Checklist { ticked: vec![false; b.steps.len()], at: usize::MAX, since: ship.t, was: 0.0, t: ship.t, slots: vec![0.0; b.slots.max(1)], eval: Eval::default() }
    }

    /// Which steps are done, as of the last look.
    pub fn ticked(&self) -> &[bool] {
        &self.ticked
    }

    /// The next step, as of the last look (the number of steps: none is left).
    pub fn next(&self) -> usize {
        self.at.min(self.ticked.len())
    }

    /// The control to touch now, as of the last look: the next step's, or the guard over it
    /// while that is down. None: nothing to touch (waiting, done, something on no panel).
    pub fn point(&self, b: &Bound, ship: &Ship) -> Option<usize> {
        let k = match &b.steps.get(self.at)?.act {
            Act::Set { control, .. } | Act::Press { control } | Act::Turn { control, .. } | Act::Hold { control, .. } => *control,
            _ => return None,
        };
        let down = |cover: usize| {
            let c = &ship.panels.controls[cover];
            c.mech.value(&c.st) < 0.5
        };
        Some(ship.panels.controls[k].cover.filter(|c| down(*c)).unwrap_or(k))
    }

    /// A look at the ship: every step's tick, the next step, whether it is all done. Nothing is
    /// allocated; call it while the checklist is being looked at, as often or as seldom as that.
    pub fn poll(&mut self, b: &Bound, ship: &Ship) -> Standing {
        let dt = (ship.t - self.t).max(0.0);
        self.t = ship.t;
        let value = |k: usize| {
            let c = &ship.panels.controls[k];
            c.mech.value(&c.st)
        };
        for (i, step) in b.steps.iter().enumerate() {
            let next = i == self.at;
            let now = match (&step.done, &step.act) {
                (Some((prog, slot)), _) => Some(self.eval.run(prog, &ship.store, &mut self.slots[*slot..*slot + prog.slots], dt, ship.t) >= 0.5),
                (None, Act::Set { control, value: to }) => Some((value(*control) - to).abs() < 1e-6),
                (None, Act::Signal { signal, value: to }) => Some((ship.store.get(*signal) - to).abs() < 1e-6),
                (None, Act::Hold { control, axis, value: to }) => {
                    let c = &ship.panels.controls[*control];
                    let x = if *axis == 0 { c.mech.value(&c.st) } else { c.mech.value2(&c.st).unwrap_or(0.0) };
                    Some((x - to).abs() < 0.05)
                }
                (None, Act::Until { cond, slot, .. }) => Some(self.eval.run(cond, &ship.store, &mut self.slots[*slot..*slot + cond.slots], dt, ship.t) >= 0.5),
                // (nothing of these stays to be looked at: seen while they are the next step)
                (None, Act::Press { control }) if next => (ship.panels.controls[*control].st.has(F_PRESSED) || (value(*control) - self.was).abs() > 1e-9).then_some(true),
                (None, Act::Turn { control, notches }) if next => ((value(*control) - self.was) * f64::from(*notches) > 0.0).then_some(true),
                (None, Act::Wait { secs }) if next => (ship.t - self.since >= *secs).then_some(true),
                _ => None,
            };
            if let Some(v) = now {
                self.ticked[i] = v;
            }
        }
        // the next: the first not ticked; one that has just become so starts from how its
        // control stands now
        let next = self.ticked.iter().position(|t| !t).unwrap_or(b.steps.len());
        if next != self.at {
            (self.at, self.since) = (next, ship.t);
            self.was = match b.steps.get(next).map(|s| &s.act) {
                Some(Act::Press { control } | Act::Turn { control, .. }) => value(*control),
                _ => 0.0,
            };
        }
        let n = b.done.slots;
        if self.eval.run(&b.done, &ship.store, &mut self.slots[b.done_slot..b.done_slot + n], dt, ship.t) >= 0.5 {
            Standing::Done
        } else if next < b.steps.len() {
            Standing::Step(next)
        } else {
            Standing::Waiting
        }
    }
}

/// What a headless run saw of the ship while the procedure was being timed: how far each joint
/// went (its lowest and highest), which machines were on their way somewhere at some point
/// (`Machine::settling`) and which still were when it was done.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    pub travel: Vec<(f64, f64)>,
    pub settling: Vec<bool>,
    pub unsettled: Vec<bool>,
}

impl Watch {
    fn see(&mut self, ship: &Ship) {
        if self.travel.is_empty() {
            self.travel = ship.joints.iter().map(|j| (j.q, j.q)).collect();
            self.settling = vec![false; ship.machines.len()];
            self.unsettled = vec![false; ship.machines.len()];
        }
        for (t, j) in self.travel.iter_mut().zip(&ship.joints) {
            *t = (t.0.min(j.q), t.1.max(j.q));
        }
        for (k, m) in ship.machines.iter().enumerate() {
            let now = m.m.settling() == Some(true);
            self.settling[k] |= now;
            self.unsettled[k] = now;
        }
    }
}

/// How a headless run of a procedure went.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Seconds from its first step to done; none: it did not get there.
    pub secs: Option<f64>,
    pub why: String,
    pub watch: Watch,
}

/// The instances to do before `i` and `i` itself, each once, in the order they are done.
pub fn chain(list: &[Bound], i: usize) -> Vec<usize> {
    fn visit(list: &[Bound], i: usize, out: &mut Vec<usize>, depth: usize) {
        if out.contains(&i) || depth > list.len() {
            return;
        }
        for a in &list[i].after {
            if let Some(k) = list.iter().position(|b| b.id == *a) {
                visit(list, k, out, depth + 1);
            }
        }
        if !out.contains(&i) {
            out.push(i);
        }
    }
    let mut out = Vec::new();
    visit(list, i, &mut out, 0);
    out
}

/// Procedure `i` of `list` done on a ship as it stands (a cold one, for the budgets), headless:
/// what goes before it first, then it, timed and watched; the ship's systems ticked in the world
/// each asks for. `cap`: how long each is given at most (s).
pub fn drive(list: &[Bound], i: usize, hand: f64, ship: &mut Ship, s: &mut Structure, cap: &dyn Fn(&Bound) -> f64) -> Outcome {
    let mut out = Outcome::default();
    for k in chain(list, i) {
        let b = &list[k];
        let (w, limit, last) = (b.world(), cap(b), k == i);
        let mut run = Run::new(b, hand);
        loop {
            match run.tick(b, ship, s, TICK) {
                Status::Done => break,
                Status::Failed => {
                    out.why = if last { run.why } else { format!("antes, {}: {}", b.id, run.why) };
                    return out;
                }
                Status::Preparing | Status::Working => {}
            }
            if run.elapsed > limit {
                out.why = format!("{}no termina en {limit:.0} s: {}", if last { String::new() } else { format!("antes, {}: ", b.id) }, b.done.source);
                return out;
            }
            ship.update(s, &w, TICK);
            if last && run.phase != Phase::Prepare {
                out.watch.see(ship);
            }
        }
        if last {
            out.secs = Some(run.elapsed);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_says_what_its_stars_stood_for() {
        let caps = |pat: &str, text: &str| glob(pat, text).map(|c| c.join(","));
        assert_eq!(caps("sala_*/abrir_puerta_*", "sala_cabina/abrir_puerta_bodega").as_deref(), Some("cabina,bodega"));
        assert_eq!(caps("anclaje_*", "anclaje_b1").as_deref(), Some("b1"));
        assert_eq!(caps("*", "iman").as_deref(), Some("iman"));
        assert_eq!(caps("techo/apu", "techo/apu").as_deref(), Some(""));
        assert_eq!(caps("sala_*/venteo", "sala_cabina/repres"), None);
        assert_eq!(caps("vi_*_a/volante", "vi_bodega_b/volante"), None);
        assert_eq!(instance("ventear", &["cabina".to_string()]), "ventear:cabina");
        assert_eq!(instance("energia", &[]), "energia");
    }
}
