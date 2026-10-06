//! The panels of every ship, control by control and instrument by instrument:
//! - no dead controls: every control can be worked by a hand (or says what keeps it: an
//!   interlock), its signal follows it, and that signal is read by something that does something
//!   with it — a machine, an actuator, a valve, a switch of a network, the flight computer, an
//!   instrument — straight or through the ship's derived logic; and no derived signal is worked
//!   out for nobody;
//! - a guarded control does not move with its cover down, and does with it up;
//! - every instrument reads signals that something writes;
//! - every lamp rule can win and every warning, alarm and interlock condition can both hold and
//!   not hold (none is written so that it never lights, or never goes out);
//! - what a display shows is the signal it names in the unit it names, within its last digit; a
//!   needle and a bar stand where their signal puts them on their scale; and every page of every
//!   multi-function display has something on it that exists.
//! All of it from each ship's own panels as built: nothing is listed here.
mod comun;
use comun::*;
use lunar_controls::{
    Blocked, Event, Indicator, Intent,
    indicator::{IndKind, Piece},
    mfd::WKind,
};
use lunar_machines::EdgeKind;
use lunar_signals::{Program, SignalId, Writer, compile_in};
use std::collections::HashSet;

/// The signals an instrument reads.
fn reads(ind: &Indicator) -> Vec<SignalId> {
    let pieces = |lines: &[Vec<Piece>], out: &mut Vec<SignalId>| {
        for p in lines.iter().flatten() {
            match p {
                Piece::Value(s, _) | Piece::Label(s, _) => out.push(*s),
                Piece::Text(_) => {}
            }
        }
    };
    let mut out = Vec::new();
    match &ind.kind {
        IndKind::Lamp { rules } => rules.iter().for_each(|r| out.extend(&r.cond.inputs)),
        IndKind::Needle { signal, .. } | IndKind::Bar { signal, .. } | IndKind::Seven { signal, .. } | IndKind::Counter { signal, .. } => out.push(*signal),
        IndKind::Screen { pages, page, .. } => {
            out.extend(page);
            pages.iter().for_each(|p| pieces(&p.1, &mut out));
        }
        IndKind::Annunciator { cells, ack, .. } => {
            out.extend(ack);
            cells.iter().for_each(|c| out.extend(&c.cond.inputs));
        }
        IndKind::Mfd(m) => {
            out.extend(m.page);
            for w in m.pages.iter().flat_map(|p| &p.widgets) {
                out.extend(w.signal);
                match &w.kind {
                    WKind::Text(lines) => pieces(lines, &mut out),
                    WKind::Map(areas, _) => out.extend(areas.iter().map(|a| a.1)),
                    _ => {}
                }
            }
        }
    }
    out
}

/// What an instrument is called, for a message.
fn ind_name(r: &Rig, i: usize) -> String {
    let ind = &r.ship.panels.indicators[i];
    format!("{}/{}", r.kind.panels[ind.panel].id, r.kind.panels[ind.panel].def.mandos[ind.index].id)
}

/// The derived signals of a ship (its own and its panels'): each target with what it reads.
fn derived(r: &Rig) -> Vec<(SignalId, Program)> {
    r.kind.def.derivadas.iter().chain(r.kind.panels.iter().flat_map(|p| p.def.derivadas.iter())).filter_map(|(name, src)| Some((r.find(name)?, compile_in(src, &r.ship.store).ok()?))).collect()
}

/// The signals something acts on: what the machines, the actuators, the closures, the clamps,
/// the valves, the switches of the networks, the flight computer, the circuit priorities, the
/// instruments, the interlocks, the alarms and the sounds read; and, through the derived logic,
/// whatever those are worked out from.
fn acted_on(r: &Rig) -> HashSet<SignalId> {
    let k = &r.kind;
    let mut set: HashSet<SignalId> = HashSet::new();
    let mut name = |n: &str| {
        if let Some(s) = r.find(n) {
            set.insert(s);
        }
    };
    for plan in &k.nets {
        for e in plan.edges.iter().filter(|e| e.kind == EdgeKind::Switch) {
            e.signal.iter().for_each(|s| name(s));
            if let Some(circuit) = e.id.strip_prefix("brk.") {
                name(&format!("prio.{circuit}"));
            }
        }
    }
    for m in &k.machines {
        // the orders its data names, and whatever of its parameters is the name of a signal (a
        // sensor, a set point, a permit, a damper)
        m.def.ordenes.values().chain(m.def.params.values()).filter_map(|v| v.as_str()).for_each(&mut name);
    }
    for a in &k.actuators {
        name(a.def.control.orden.as_deref().unwrap_or(&format!("{}.orden", a.id)));
    }
    for c in &k.closures {
        name(c.order.as_deref().unwrap_or(&format!("{}.mano", c.id)));
    }
    for c in &k.clamps {
        name(&format!("{}.soltar", c.id));
    }
    for v in k.compartments.iter().flat_map(|c| &c.vents) {
        name(&v.signal);
    }
    if let Some(f) = &k.def.vuelo {
        [&f.cabeceo, &f.alabeo, &f.guinada, &f.acelerador, &f.estabilizador, &f.mantener, &f.descarga, &f.perfil, &f.limite].into_iter().flatten().for_each(|s| name(s));
        f.traslacion.iter().flatten().for_each(|s| name(s));
    }
    // what says whether its own gravity works
    k.def.gravedad.senal.iter().for_each(|s| name(s));
    // what its tactical system and its autopilot read (their buttons, their selectors)
    if let Some(t) = &r.ship.tactical {
        set.extend(t.reads());
    }
    if let Some(a) = &r.ship.autopilot {
        set.extend(a.reads());
    }
    for (text, cond) in &k.def.alarmas {
        let p = compile_in(cond, &r.ship.store).unwrap_or_else(|e| panic!("{}: alarma '{text}': {}", k.id, e.0));
        set.extend(p.inputs);
    }
    for ind in &r.ship.panels.indicators {
        set.extend(reads(&ind.ind));
    }
    for c in &r.ship.panels.controls {
        c.interlocks.iter().for_each(|i| set.extend(&i.1.inputs));
        let d = &k.panels[c.panel].def.mandos[c.index];
        if let Some(p) = d.luz.as_ref().and_then(|l| compile_in(l, &r.ship.store).ok()) {
            set.extend(p.inputs);
        }
        if let Some(s) = d.destino.as_ref().and_then(|n| r.find(n)) {
            set.insert(s);
        }
    }
    // an order a machine or an actuator reads under its own name (`<id>.<role>`), written by
    // someone else; and the loops the ship sounds with (`sonido.<id>`, the game's)
    let ids: HashSet<&str> = k.machines.iter().map(|m| m.id.as_str()).chain(k.actuators.iter().map(|a| a.id.as_str())).collect();
    for s in r.ship.store.ids() {
        let n = r.name(s);
        let head = n.split('.').next().unwrap_or("");
        let theirs = matches!(r.ship.store.meta(s).writer, Writer::Machine(_) | Writer::Actuator(_) | Writer::World);
        if head == "sonido" || (ids.contains(head) && !theirs) {
            set.insert(s);
        }
    }
    // back through the derived logic
    let derived = derived(r);
    loop {
        let before = set.len();
        for (target, p) in &derived {
            if set.contains(target) {
                set.extend(&p.inputs);
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

/// The ways a hand has of working a control, each a few intents one after another.
fn ways() -> Vec<Vec<Intent>> {
    let turn = |n: f32| Intent::Turn { notches: n, rate: 4.0, m: lunar_controls::Mods::default() };
    vec![
        vec![Intent::Press { elem: 0 }],
        vec![Intent::Press { elem: 1 }],
        vec![turn(1.0)],
        vec![turn(-1.0)],
        vec![Intent::Axis { axis: 0, value: 1.0 }],
        vec![Intent::Drag { dx: 0.0, dy: 0.3, m: lunar_controls::Mods::default() }],
        // a keypad: a digit, then ENTER
        vec![Intent::Press { elem: 0 }, Intent::Release, Intent::Press { elem: 12 }],
    ]
}

#[test]
fn every_control_can_be_worked_and_its_signal_is_acted_on() {
    let (mut bad, mut notes) = (Vec::new(), Vec::new());
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(5.0);
        let acted = acted_on(&r);
        let n = r.ship.panels.controls.len();
        let (mut worked, mut vetoed) = (0, 0);
        for k in 0..n {
            let (id, sig, sig2, kind_) = {
                let c = &r.ship.panels.controls[k];
                (c.id.clone(), c.sig, c.sig2, c.mech.kind())
            };
            let d = kind.panels[r.ship.panels.controls[k].panel].def.mandos[r.ship.panels.controls[k].index].clone();
            // ---- what it writes is acted on (a cover guards; it writes nothing anybody needs)
            if kind_ == "tapa" {
                if !r.ship.panels.controls.iter().any(|c| c.cover == Some(k)) {
                    bad.push(format!("{}: la tapa {id} no protege ningún mando de su panel ({:?})", kind.id, d.protege));
                }
                continue;
            }
            // (the bezel of a display with one page has nothing to choose)
            let single_page = kind_ == "bisel" && d.posiciones.len() <= 1;
            if !acted.contains(&sig) && !sig2.is_some_and(|s| acted.contains(&s)) {
                if single_page {
                    notes.push(format!("{}: {id} es el marco de una pantalla de una sola página: sus botones no eligen nada", kind.id));
                } else {
                    bad.push(format!("{}: mando muerto {id}: escribe '{}' y nada lo lee (ni máquinas, ni instrumentos, ni la lógica de la nave)", kind.id, r.name(sig)));
                }
            }
            if single_page {
                continue;
            }
            // ---- a hand can work it, and its signal follows
            r.uncover(k);
            let (before, sig_before) = (r.value(k), r.get(sig));
            let mut outcome = None;
            for way in ways() {
                let mut last = None;
                for i in &way {
                    last = Some(r.intent(k, i));
                }
                if (r.value(k) - before).abs() > 1e-12 || last.is_some_and(|o| o.changed) {
                    outcome = Some(Ok(()));
                    break;
                }
                if let Some(Event::Blocked(b)) = last.and_then(|o| o.event)
                    && !matches!(b, Blocked::Stop)
                {
                    outcome = Some(Err(r.ship.panels.reason(b)));
                }
                r.intent(k, &Intent::Release);
            }
            match outcome {
                Some(Ok(())) => {
                    worked += 1;
                    // (written at the next tick; a pulse is out for one)
                    r.tick();
                    let moved = (r.get(sig) - sig_before).abs() > 1e-12 || sig2.is_some_and(|s| r.get(s) != 0.0) || {
                        r.tick();
                        (r.get(sig) - sig_before).abs() > 1e-12
                    };
                    if !moved {
                        let p = &r.ship.panels.panels[r.ship.panels.controls[k].panel];
                        bad.push(format!("{}: {id} se acciona (de {before} a {}) y su señal '{}' se queda en {sig_before} (panel {}{})", kind.id, r.value(k), r.name(sig), if p.alive { "vivo" } else { "destruido" }, if p.linked { "" } else { ", sin datos" }));
                    }
                }
                Some(Err(why)) => {
                    vetoed += 1;
                    notes.push(format!("{}: {id} no se deja accionar en reposo: {why}", kind.id));
                }
                None => bad.push(format!("{}: {id} ({kind_}) no responde a nada que haga una mano (pulsar, girar, arrastrar, teclear)", kind.id)),
            }
            // as it was, and the ship with power and data again before the next one
            r.intent(k, &Intent::Release);
            r.set(k, before);
            r.ticks(2);
            r.until(120.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
        }
        // ---- and nothing is worked out for nobody
        for (target, p) in derived(&r) {
            if !acted.contains(&target) {
                bad.push(format!("{}: la derivada '{}' = {} se calcula y nada la lee", kind.id, r.name(target), p.source));
            }
        }
        bad.extend(r.broken());
        eprintln!("{}: {worked} mandos accionados, {vetoed} retenidos por un enclavamiento, de {n}", kind.id);
    }
    for n in &notes {
        eprintln!("  {n}");
    }
    report("mandos muertos o que no se dejan accionar", &bad);
}

#[test]
fn a_guarded_control_does_not_move_with_its_cover_down() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(2.0);
        let guarded: Vec<(usize, usize)> = r.ship.panels.controls.iter().enumerate().filter_map(|(k, c)| c.cover.map(|cv| (k, cv))).collect();
        for &(k, cover) in &guarded {
            let id = r.ship.panels.controls[k].id.clone();
            if r.value(cover) >= 0.5 {
                bad.push(format!("{}: la tapa de {id} está levantada en la nave recién puesta", kind.id));
                r.set(cover, 0.0);
            }
            let before = r.ship.panels.controls[k].st;
            let mut moved = false;
            for way in ways() {
                for i in &way {
                    let o = r.intent(k, i);
                    moved |= o.changed;
                    // (a press says why not)
                    if matches!(i, Intent::Press { .. }) && o.event != Some(Event::Blocked(Blocked::Covered)) {
                        bad.push(format!("{}: {id} con la tapa bajada responde a una pulsación con {:?} en vez de «tapa cerrada»", kind.id, o.event));
                    }
                }
                r.intent(k, &Intent::Release);
            }
            if moved || r.ship.panels.controls[k].st.x != before.x {
                bad.push(format!("{}: {id} se mueve con su tapa bajada", kind.id));
            }
            // cover up: it answers (it moves, or an interlock of its own says no)
            r.click(cover);
            if r.value(cover) < 0.5 {
                bad.push(format!("{}: la tapa de {id} no se levanta", kind.id));
                continue;
            }
            let o = r.intent(k, &Intent::Press { elem: 0 });
            r.intent(k, &Intent::Release);
            if !o.changed && !matches!(o.event, Some(Event::Blocked(Blocked::Veto(_)))) && r.ship.panels.controls[k].st.x == before.x {
                bad.push(format!("{}: {id} tampoco se mueve con la tapa levantada ({:?})", kind.id, o.event));
            }
            r.set(k, before.x);
            r.click(cover);
        }
        eprintln!("{}: {} mandos bajo tapa", kind.id, guarded.len());
    }
    report("tapas", &bad);
}

#[test]
fn every_instrument_reads_signals_that_something_writes() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let r = fleet().rig(kind);
        let mut count = 0;
        for (i, ind) in r.ship.panels.indicators.iter().enumerate() {
            let mut seen = reads(&ind.ind);
            seen.sort_unstable();
            seen.dedup();
            for s in seen {
                count += 1;
                // written by a machine, an actuator, the ship, the derived logic or a control (one
                // of several handles of one mechanism writes for all); or a figure the data sets
                let written = r.ship.store.meta(s).writer != Writer::None || r.control_of(s).is_some() || kind.def.senales.contains_key(r.name(s));
                if !written {
                    bad.push(format!("{}: el instrumento {} lee '{}' y nadie la escribe (vale {} para siempre)", kind.id, ind_name(&r, i), r.name(s), r.get(s)));
                }
            }
        }
        eprintln!("{}: {} instrumentos, {count} lecturas de señal", kind.id, r.ship.panels.indicators.len());
    }
    report("instrumentos que leen señales sin dueño", &bad);
}

#[test]
fn every_lamp_rule_warning_alarm_and_interlock_can_both_hold_and_not() {
    let mut bad = Vec::new();
    const TRIES: usize = 4000;
    for kind in &fleet().kinds {
        let r = fleet().rig(kind);
        let mut trial = Trial::new(&r, 0x10_61ca);
        let mut conditions = 0;
        let mut both = |what: String, p: &Program, bad: &mut Vec<String>| {
            conditions += 1;
            match trial.both_ways(p, TRIES) {
                (true, true) => {}
                (true, false) => bad.push(format!("{}: {what} se cumple siempre: «{}»", kind.id, p.source)),
                (false, _) => bad.push(format!("{}: {what} no se cumple nunca: «{}»", kind.id, p.source)),
            }
        };
        for (text, cond) in &kind.def.alarmas {
            both(format!("la alarma '{text}'"), &compile_in(cond, &r.ship.store).unwrap(), &mut bad);
        }
        for c in &r.ship.panels.controls {
            for (_, p, reason) in &c.interlocks {
                both(format!("el enclavamiento de {} ({})", c.id, r.ship.panels.reasons[*reason]), p, &mut bad);
            }
            let d = &kind.panels[c.panel].def.mandos[c.index];
            if let Some(l) = &d.luz {
                both(format!("la luz propia de {}", c.id), &compile_in(l, &r.ship.store).unwrap(), &mut bad);
            }
        }
        for (i, ind) in r.ship.panels.indicators.iter().enumerate() {
            match &ind.ind.kind {
                IndKind::Annunciator { cells, .. } => {
                    for c in cells {
                        both(format!("el aviso '{}' de {}", c.label, ind_name(&r, i)), &c.cond, &mut bad);
                    }
                }
                _ => {}
            }
        }
        // a lamp: each of its rules can be the one that lights it (none is hidden by the ones
        // before it), each can also not hold
        for (i, ind) in r.ship.panels.indicators.iter().enumerate() {
            let IndKind::Lamp { rules } = &ind.ind.kind else { continue };
            let sources: Vec<&str> = rules.iter().map(|x| x.cond.source.as_str()).collect();
            let candidates = Trial::candidates(&sources);
            let mut inputs: Vec<SignalId> = rules.iter().flat_map(|x| x.cond.inputs.iter().copied()).collect();
            inputs.sort_unstable();
            inputs.dedup();
            let mut slots: Vec<Vec<f64>> = rules.iter().map(|x| vec![0.0; x.cond.slots]).collect();
            let (mut wins, mut fails) = (vec![false; rules.len()], vec![false; rules.len()]);
            for _ in 0..TRIES {
                trial.shake(&inputs, &candidates);
                let truth: Vec<bool> = rules.iter().zip(&mut slots).map(|(x, s)| trial.holds(&x.cond, s)).collect();
                if let Some(w) = truth.iter().position(|t| *t) {
                    wins[w] = true;
                }
                truth.iter().enumerate().filter(|(_, t)| !**t).for_each(|(k, _)| fails[k] = true);
                if wins.iter().all(|w| *w) && fails.iter().all(|f| *f) {
                    break;
                }
            }
            conditions += rules.len();
            for (k, x) in rules.iter().enumerate() {
                if !wins[k] {
                    bad.push(format!("{}: la lámpara {}: su regla {} («{}») nunca es la que la enciende (la tapan las anteriores o no se cumple nunca)", kind.id, ind_name(&r, i), k + 1, x.cond.source));
                } else if !fails[k] && rules.len() == 1 {
                    bad.push(format!("{}: la lámpara {} está siempre encendida: «{}»", kind.id, ind_name(&r, i), x.cond.source));
                }
            }
        }
        eprintln!("{}: {conditions} condiciones (alarmas, avisos, enclavamientos, luces, reglas de lámpara)", kind.id);
    }
    report("condiciones que no pueden cambiar", &bad);
}

/// A number as a display writes it ("12,5", "-3", "62,4 %"): the figure and its decimals.
fn shown(text: &str) -> Option<(f64, usize)> {
    let figure = text.split_whitespace().next()?;
    let decimals = figure.split(',').nth(1).map_or(0, str::len);
    figure.replace(',', ".").parse().ok().map(|v| (v, decimals))
}

#[test]
fn every_readout_shows_its_signal_in_its_unit() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let mut checked = 0;
        // at rest, then with every machine a hand can switch on set going: more to read
        for phase in 0..2 {
            if phase == 1 {
                r.switch_on(&|_, _| true);
            }
            r.run(12.0);
            // every page of every multi-function display in turn, by its own bezel
            let pages = r.ship.panels.indicators.iter().map(|i| if let IndKind::Mfd(m) = &i.ind.kind { m.pages.len() } else { 1 }).max().unwrap_or(1);
            for page in 0..pages {
                for i in 0..r.ship.panels.indicators.len() {
                    if let IndKind::Mfd(m) = &r.ship.panels.indicators[i].ind.kind
                        && let Some(k) = m.page.and_then(|s| r.control_of(s))
                        && page < m.pages.len()
                    {
                        r.intent(k, &Intent::Press { elem: page as u8 });
                        r.intent(k, &Intent::Release);
                    }
                }
                // what each signal was over the last moments (a screen is redrawn ten times a second)
                let n = r.ship.store.len();
                let (mut lo, mut hi) = (vec![f64::MAX; n], vec![f64::MIN; n]);
                r.ticks(15);
                for _ in 0..8 {
                    r.tick();
                    for (i, v) in r.ship.store.values().iter().enumerate() {
                        lo[i] = lo[i].min(*v);
                        hi[i] = hi[i].max(*v);
                    }
                }
                let within = |s: SignalId, unit: &lunar_signals::Unit, v: f64, decimals: usize| {
                    let half = 0.5 * 10f64.powi(-(decimals as i32)) + 1e-9;
                    let (a, b) = (unit.from_si(lo[s.index()]), unit.from_si(hi[s.index()]));
                    v >= a.min(b) - half && v <= a.max(b) + half
                };
                for (i, x) in r.ship.panels.indicators.iter().enumerate() {
                    let panel = &r.ship.panels.panels[x.panel];
                    if !(panel.alive && panel.powered) {
                        continue;
                    }
                    let name = ind_name(&r, i);
                    match &x.ind.kind {
                        IndKind::Seven { signal, unit, .. } => {
                            checked += 1;
                            let own = r.ship.store.meta(*signal).unit;
                            if !own.is_dimensionless() && !own.compatible(unit) {
                                bad.push(format!("{}: {name} enseña '{}' en una unidad que no es la suya", kind.id, r.name(*signal)));
                            }
                            match shown(&x.st.text) {
                                Some((v, dec)) if !within(*signal, unit, v, dec) => bad.push(format!("{}: {name} marca «{}» y '{}' vale {} (en su unidad, {:.4})", kind.id, x.st.text, r.name(*signal), r.get(*signal), unit.from_si(r.get(*signal)))),
                                Some(_) => {}
                                None => bad.push(format!("{}: {name} marca «{}»: '{}' vale {} y no cabe en sus dígitos (o no es un número)", kind.id, x.st.text, r.name(*signal), r.get(*signal))),
                            }
                        }
                        IndKind::Bar { signal, lo: a, hi: b, .. } => {
                            checked += 1;
                            let want = ((r.get(*signal) - a) / (b - a)).clamp(0.0, 1.0) as f32;
                            if (x.st.x - want).abs() > 0.05 {
                                bad.push(format!("{}: la barra {name} está al {:.0} % y '{}' = {} la pone al {:.0} %", kind.id, x.st.x * 100.0, r.name(*signal), r.get(*signal), want * 100.0));
                            }
                        }
                        IndKind::Needle { signal, lo: a, hi: b, sweep, .. } => {
                            checked += 1;
                            let u = ((r.get(*signal) - a) / (b - a)).clamp(-0.03, 1.03) as f32;
                            if (x.st.x - (u - 0.5) * sweep).abs() > 0.06 * sweep {
                                bad.push(format!("{}: la aguja {name} está a {:.2} rad y '{}' = {} la pone a {:.2}", kind.id, x.st.x, r.name(*signal), r.get(*signal), (u - 0.5) * sweep));
                            }
                        }
                        IndKind::Mfd(m) => {
                            let p = &m.pages[x.st.mfd.page];
                            if p.widgets.is_empty() {
                                bad.push(format!("{}: la página '{}' de {name} está vacía", kind.id, p.title));
                            }
                            for (w, ws) in p.widgets.iter().zip(&x.st.mfd.widgets) {
                                let (Some(s), WKind::Value | WKind::Bar | WKind::Tank | WKind::Dial) = (w.signal, &w.kind) else { continue };
                                checked += 1;
                                let own = r.ship.store.meta(s).unit;
                                if !own.is_dimensionless() && !own.compatible(&w.unit) {
                                    bad.push(format!("{}: {name}, página '{}': '{}' se enseña en «{}», que no es su unidad", kind.id, p.title, r.name(s), w.unit_name));
                                }
                                match shown(&ws.text) {
                                    Some((v, dec)) if !within(s, &w.unit, v, dec) => bad.push(format!("{}: {name}, página '{}': {} marca «{}» y '{}' vale {:.4} en esa unidad", kind.id, p.title, w.label, ws.text, r.name(s), w.unit.from_si(r.get(s)))),
                                    Some(_) => {}
                                    None => bad.push(format!("{}: {name}, página '{}': {} marca «{}» ('{}' = {})", kind.id, p.title, w.label, ws.text, r.name(s), r.get(s))),
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // what the ship says of its air adds up: the pressure of a room is that of its gases
        for c in &kind.compartments {
            let (p, parts) = (r.sig(&format!("{}.p", c.id)), r.sig(&format!("{}.o2", c.id)) + r.sig(&format!("{}.n2", c.id)) + r.sig(&format!("{}.co2", c.id)));
            if (p - parts).abs() > 1e-6 * p.abs() + 1e-6 {
                bad.push(format!("{}: {}.p = {p} Pa y sus gases suman {parts} Pa", kind.id, c.id));
            }
        }
        eprintln!("{}: {checked} lecturas comprobadas contra su señal", kind.id);
    }
    report("instrumentos que no dicen lo que hay", &bad);
}
