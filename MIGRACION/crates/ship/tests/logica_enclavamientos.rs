//! Interlocks and sequences, on every ship:
//! - what the data declares (`enclavamientos`, the ship's own and the ones every compartment
//!   brings): with its condition true the control refuses the move it guards (and only that
//!   one), saying why; with it false it moves. Tried on the ship's own panels with its signals
//!   put where the condition needs them, and live: on the ground and in flight, what the panels
//!   refuse is exactly what the conditions say;
//! - what the machines' models declare, for every machine of that model on any ship, found and
//!   worked through the ship's own panels and networks (the switch that feeds a port is worked
//!   out from the network's plan):
//!   · a rocket engine does not start unarmed, nor without power, and fails to light without
//!     propellant; with all three it runs, pushes, and stops when told;
//!   · a generator does not start with its starter's breaker pulled, fails without fuel, runs
//!     and gives with both;
//!   · a reactor does not start without control power nor without coolant flowing; running, it
//!     trips the moment its control power goes, and again when its coolant stops; tripped, it
//!     rearms cool and starts again;
//!   · a leg held by hydraulic locks does not move until there is pressure;
//!   · a magnet keeps hold with the ship dead, cannot switch once its capacitor is spent, and
//!     can again when the bus charges it.
//! Nothing here measures how long anything takes: each wait ends when what is waited for
//! happens, with a generous bound for a test that fails.
mod comun;
use comun::*;
use lunar_controls::{Blocked, Event, Intent};
use lunar_machines::net::Medium;
use lunar_signals::{Eval, Program, SignalId};

#[test]
fn every_declared_interlock_refuses_when_it_should_and_only_then() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(2.0);
        let mut trial = Trial::new(&r, 0xe7c1a);
        let mut count = 0;
        for k in 0..r.ship.panels.controls.len() {
            if r.ship.panels.controls[k].interlocks.is_empty() {
                continue;
            }
            r.uncover(k);
            let id = r.ship.panels.controls[k].id.clone();
            let locks: Vec<(Option<f64>, Program, usize)> = r.ship.panels.controls[k].interlocks.clone();
            let rest = r.ship.panels.controls[k].st;
            let kind_ = r.kind.clone();
            // a move the interlock guards (toward its value; any, if it guards them all), and one
            // it does not
            let go = |r: &mut Rig, store: &lunar_signals::Store, toward: Option<f64>, away: bool| {
                r.ship.panels.controls[k].st = rest;
                // (a move toward its value starts from the far side of it)
                if let Some(t) = toward
                    && (r.value(k) - t).abs() < 1e-9
                {
                    let c = &mut r.ship.panels.controls[k];
                    c.mech.set(&mut c.st, if t >= 0.5 { 0.0 } else { 1.0 });
                }
                let from = r.value(k);
                let o = match toward {
                    Some(t) => r.ship.panels.intent(k, &Intent::Set { value: if away { from + (from - t) } else { t } }, &r.s, &kind_, store),
                    None => r.ship.panels.intent(k, &Intent::Press { elem: 0 }, &r.s, &kind_, store),
                };
                let moved = (r.value(k) - from).abs() > 1e-9;
                r.ship.panels.intent(k, &Intent::Release, &r.s, &kind_, store);
                r.ship.panels.controls[k].st = rest;
                (o, moved)
            };
            for (n, (toward, p, reason)) in locks.iter().enumerate() {
                count += 1;
                let why = r.ship.panels.reasons[*reason].clone();
                let what = format!("{}: enclavamiento de {id} («{}», {})", kind.id, p.source, why);
                if why.trim().is_empty() {
                    bad.push(format!("{what}: no dice por qué"));
                }
                // ---- its condition true: the move it guards is refused, with its reason
                if !trial.make(p, true, 4000) {
                    bad.push(format!("{what}: no hay valores que lo cumplan"));
                    continue;
                }
                let (o, moved) = go(&mut r, &trial.store, *toward, false);
                match o.event {
                    Some(Event::Blocked(Blocked::Veto(_))) if !moved => {}
                    e => bad.push(format!("{what}: se cumple y el mando {} ({e:?})", if moved { "se mueve" } else { "no dice que lo retiene" })),
                }
                // (and only toward what it guards: away from it the control is free, unless
                // another of its interlocks holds it)
                if toward.is_some() && locks.iter().enumerate().all(|(m, l)| m == n || l.0 == *toward) {
                    let (o, _) = go(&mut r, &trial.store, *toward, true);
                    if matches!(o.event, Some(Event::Blocked(Blocked::Veto(_)))) {
                        bad.push(format!("{what}: retiene también el movimiento contrario"));
                    }
                }
                // ---- every condition of this control false: it moves
                let mut slots: Vec<Vec<f64>> = locks.iter().map(|l| vec![0.0; l.1.slots]).collect();
                let sources: Vec<&str> = locks.iter().map(|l| l.1.source.as_str()).collect();
                let candidates = Trial::candidates(&sources);
                let mut inputs: Vec<SignalId> = locks.iter().flat_map(|l| l.1.inputs.iter().copied()).collect();
                inputs.sort_unstable();
                inputs.dedup();
                let free = (0..4000).any(|_| {
                    trial.shake(&inputs, &candidates);
                    locks.iter().zip(&mut slots).all(|(l, s)| {
                        trial.holds(&l.1, s);
                        !trial.holds(&l.1, s)
                    })
                });
                if !free {
                    bad.push(format!("{what}: no hay valores con los que el mando quede libre de todos sus enclavamientos"));
                    continue;
                }
                let (o, moved) = go(&mut r, &trial.store, *toward, false);
                if matches!(o.event, Some(Event::Blocked(Blocked::Veto(_)))) || !moved {
                    bad.push(format!("{what}: no se cumple y el mando sigue sin moverse ({:?})", o.event));
                }
            }
        }
        eprintln!("{}: {count} enclavamientos declarados, cada uno probado cumplido y sin cumplir", kind.id);
    }
    report("enclavamientos declarados", &bad);
}

/// On the ship as it is, in several places: what the panels refuse is what the conditions say.
#[test]
fn live_the_panels_refuse_exactly_what_the_conditions_say_on_the_ground_and_in_flight() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let mut eval = Eval::default();
        // per interlock: (control, its place among that control's) and whether it held in each world
        let mut held: Vec<Vec<bool>> = Vec::new();
        let worlds = [("posada", grounded()), ("en vuelo, a 400 m", airborne())];
        for (w, (place, world)) in worlds.iter().enumerate() {
            r.w = *world;
            r.run(1.0);
            let mut n = 0;
            for k in 0..r.ship.panels.controls.len() {
                let locks = r.ship.panels.controls[k].interlocks.clone();
                if locks.is_empty() {
                    continue;
                }
                r.uncover(k);
                let id = r.ship.panels.controls[k].id.clone();
                let rest = r.ship.panels.controls[k].st;
                for (toward, p, _) in &locks {
                    let holds = eval.run(p, &r.ship.store, &mut vec![0.0; p.slots], 0.0, r.ship.t) >= 0.5;
                    if held.len() <= n {
                        held.push(vec![false; worlds.len()]);
                    }
                    held[n][w] = holds;
                    n += 1;
                    // the move it guards, tried by hand
                    let v0 = r.value(k);
                    let o = match toward {
                        Some(t) if (v0 - t).abs() > 1e-9 => r.intent(k, &Intent::Set { value: *t }),
                        Some(_) => continue,
                        None => r.intent(k, &Intent::Press { elem: 0 }),
                    };
                    let vetoed = matches!(o.event, Some(Event::Blocked(Blocked::Veto(_))));
                    r.intent(k, &Intent::Release);
                    r.ship.panels.controls[k].st = rest;
                    // (another interlock of the same control may be what holds it)
                    let any = locks.iter().any(|(t, q, _)| t.is_none_or(|t| (v0 - t).abs() > 1e-9) && eval.run(q, &r.ship.store, &mut vec![0.0; q.slots], 0.0, r.ship.t) >= 0.5);
                    if vetoed != any {
                        bad.push(format!("{}, {place}: {id}: «{}» {} y el mando {}", kind.id, p.source, if holds { "se cumple" } else { "no se cumple" }, if vetoed { "se niega" } else { "se deja" }));
                    }
                }
            }
        }
        // what waits for the ship to be off the ground lets go off the ground
        let world_only = |p: &Program| !p.inputs.is_empty() && p.inputs.iter().all(|s| r.name(*s).starts_with("nave."));
        let mut n = 0;
        for c in &r.ship.panels.controls {
            for (_, p, reason) in &c.interlocks {
                if world_only(p) {
                    eprintln!("{}: {} ({}): posada {}, en vuelo {}", kind.id, c.id, r.ship.panels.reasons[*reason], if held[n][0] { "retenido" } else { "libre" }, if held[n][1] { "retenido" } else { "libre" });
                    if held[n][0] == held[n][1] {
                        bad.push(format!("{}: el enclavamiento de {} («{}») dice lo mismo posada que a 400 m", kind.id, c.id, p.source));
                    }
                }
                n += 1;
            }
        }
    }
    report("enclavamientos en vivo", &bad);
}

// ---------------------------------------------------------------- sequences of the machines

/// The controls that are switches which, opened alone as the ship stands, leave port `p` without
/// anything that gives into its network: breakers first.
fn feeds(r: &Rig, p: usize, gives: &[bool]) -> Vec<usize> {
    let n = usize::from(r.ship.ports[p].net);
    if n >= r.kind.nets.len() {
        return Vec::new();
    }
    let sources: Vec<u32> = r.ports_on(n).filter(|(k, _)| gives[*k]).map(|(_, x)| x.node).collect();
    let live: Vec<bool> = r.ship.nets[n].edges.iter().map(|e| e.conducts()).collect();
    let mut out: Vec<usize> = r.switch_controls().into_iter().filter(|&(_, m, e)| m == n && live[e] && !r.reach(n, &sources, &|x| x != e && live[x])[r.ship.ports[p].node as usize]).map(|(k, ..)| k).collect();
    out.sort_by_key(|&k| r.ctl_kind(k) != "disyuntor");
    out
}

/// What every port gives, by its role (electrical) or by what it was seen to do at rest.
fn givers(r: &mut Rig) -> Vec<bool> {
    let mut seen = Seen::default();
    r.observe(2.0, &mut seen);
    let owners = r.owners();
    let by_role = r.source_roles(&owners);
    (0..r.ship.ports.len()).map(|k| if r.kind.nets.get(usize::from(r.ship.ports[k].net)).is_some_and(|n| n.medium == Medium::Electrico) { by_role[k] } else { seen.gives[k] }).collect()
}

/// The port of machine `m` with role `role`, if it is on a network.
fn port(r: &Rig, m: usize, role: &str) -> Option<usize> {
    let rt = &r.ship.machines[m];
    rt.m.ports().iter().zip(rt.ports.clone()).find(|(s, k)| s.role == role && r.ship.ports[*k].net != u16::MAX).map(|x| x.1)
}

/// A control put at a value, its cover lifted first.
fn put(r: &mut Rig, k: usize, value: f64) {
    r.uncover(k);
    r.set(k, value);
}

fn machines_of<'a>(r: &'a Rig, model: &'a str) -> impl Iterator<Item = usize> + 'a {
    (0..r.kind.machines.len()).filter(move |&m| r.kind.machines[m].def.modelo == model)
}

fn telemetry(r: &Rig, m: usize, field: &str) -> SignalId {
    let plan = &r.kind.machines[m];
    let prefix = plan.def.params.get("telemetria").and_then(|v| v.as_str()).unwrap_or(&plan.id);
    r.find(&format!("{prefix}.{field}")).unwrap_or_else(|| panic!("{}: la máquina {} no tiene telemetría '{field}'", r.id(), plan.id))
}

/// The control a hand has for an order of a machine.
fn handle(r: &Rig, m: usize, role: &str) -> Option<usize> {
    r.order(m, role).and_then(|s| r.control_of(s))
}

#[test]
fn a_rocket_engine_needs_arming_power_and_propellant_to_start() {
    use lunar_machines::models::engine::{FAILED, OFF, RUNNING};
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let gives = givers(&mut r);
        let engines: Vec<usize> = machines_of(&r, "motor_cohete").collect();
        for &m in &engines {
            let id = kind.machines[m].id.clone();
            let (Some(arm), Some(start)) = (handle(&r, m, "armado"), handle(&r, m, "arranque")) else {
                bad.push(format!("{}: el motor {id} no tiene en ningún panel con qué armarlo o arrancarlo", kind.id));
                continue;
            };
            let state = telemetry(&r, m, "estado");
            let top = if r.ship.panels.controls[start].mech.kind() == "interruptor" { 2.0 } else { 1.0 };
            // START as a hand gives it: up to its top position, let go to RUN once it has caught
            let try_start = |r: &mut Rig| {
                put(r, start, top);
                let lit = r.until(60.0, |r| r.get(state) >= RUNNING - 1.0 || r.get(state) == FAILED);
                r.set(start, 1.0);
                r.until(60.0, |r| r.get(state) == RUNNING || r.get(state) == FAILED || r.get(state) == OFF);
                lit
            };
            // (off, and the last of its flame gone: its thrust dies away a moment after)
            let stop = |r: &mut Rig| {
                r.set(start, 0.0);
                r.set(arm, 0.0);
                r.until(120.0, |r| r.get(state) == OFF && r.ship.machines[m].m.thrust() == 0.0)
            };
            let fuel = port(&r, m, "propelente").expect("un motor con propelente");
            // ---- unarmed: nothing (whatever another test left running, stopped first)
            stop(&mut r);
            try_start(&mut r);
            if r.get(state) != OFF || r.ship.machines[m].m.thrust() > 0.0 || r.ship.ports[fuel].got > 0.0 {
                bad.push(format!("{}: {id} arranca sin armar (estado {}, {:.0} N, {:.3} kg/s)", kind.id, r.get(state), r.ship.machines[m].m.thrust(), r.ship.ports[fuel].got));
            }
            stop(&mut r);
            // ---- armed, its power cut: nothing
            match port(&r, m, "energia").map(|p| feeds(&r, p, &gives)) {
                Some(cut) if !cut.is_empty() => {
                    put(&mut r, arm, 1.0);
                    r.ticks(5);
                    r.click(cut[0]);
                    r.ticks(5);
                    try_start(&mut r);
                    if r.get(state) != OFF || r.ship.machines[m].m.thrust() > 0.0 {
                        bad.push(format!("{}: {id} arranca con {} abierto (estado {})", kind.id, r.ship.panels.controls[cut[0]].id, r.get(state)));
                    }
                    stop(&mut r);
                    put(&mut r, cut[0], 1.0);
                    // (whatever else lost its feed with it is back before going on)
                    r.until(120.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
                    r.run(2.0);
                }
                _ => bad.push(format!("{}: nada de un panel corta la corriente de {id}", kind.id)),
            }
            // ---- armed and powered, its propellant shut off: it tries and fails, and pushes nothing
            match feeds(&r, fuel, &gives).first().copied() {
                Some(valve) => {
                    put(&mut r, valve, 0.0);
                    r.ticks(5);
                    put(&mut r, arm, 1.0);
                    r.ticks(5);
                    try_start(&mut r);
                    if r.get(state) != FAILED || r.ship.machines[m].m.thrust() > 0.0 {
                        bad.push(format!("{}: {id} con {} cerrada queda en estado {} con {:.0} N (debía fallar el encendido)", kind.id, r.ship.panels.controls[valve].id, r.get(state), r.ship.machines[m].m.thrust()));
                    }
                    if !stop(&mut r) {
                        bad.push(format!("{}: {id} fallado no vuelve a apagado al desarmarlo", kind.id));
                    }
                    put(&mut r, valve, 1.0);
                    r.ticks(5);
                }
                None => bad.push(format!("{}: ninguna válvula de un panel corta el propelente de {id}", kind.id)),
            }
            // ---- all three: it runs and pushes; told to stop, it stops
            put(&mut r, arm, 1.0);
            r.ticks(5);
            try_start(&mut r);
            let running = r.until(60.0, |r| r.get(state) == RUNNING && r.ship.machines[m].m.thrust() > 0.0);
            if !running {
                bad.push(format!("{}: {id} armado, con corriente y propelente no llega a empujar (estado {}, alimentación {:.0} kPa)", kind.id, r.get(state), r.ship.ports[fuel].level / 1e3));
            } else if r.ship.ports[fuel].got <= 0.0 {
                bad.push(format!("{}: {id} empuja sin gastar propelente", kind.id));
            }
            if !stop(&mut r) || r.ship.machines[m].m.thrust() > 0.0 {
                bad.push(format!("{}: {id} no se para al quitarle la marcha y el armado", kind.id));
            }
        }
        if !engines.is_empty() {
            eprintln!("{}: {} motores cohete: sin armar, sin corriente, sin propelente y con todo", kind.id, engines.len());
        }
        bad.extend(r.broken());
    }
    report("arranque de motores", &bad);
}

#[test]
fn a_generator_needs_its_starter_fed_and_fuel() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let gives = givers(&mut r);
        for m in machines_of(&r, "generador").collect::<Vec<_>>() {
            let id = kind.machines[m].id.clone();
            let Some(run) = handle(&r, m, "marcha") else {
                bad.push(format!("{}: el generador {id} no tiene en ningún panel con qué arrancarlo", kind.id));
                continue;
            };
            let state = telemetry(&r, m, "estado");
            let out = port(&r, m, "salida").expect("un generador con salida");
            // (its starter's own circuit if it has one, else the bus its output is on)
            let starter = port(&r, m, "arranque").unwrap_or(out);
            // ---- the switch that feeds its starter open: it does not start
            match feeds(&r, starter, &gives.iter().enumerate().map(|(k, g)| *g && k != out).collect::<Vec<_>>()).first().copied() {
                Some(cut) => {
                    r.click(cut);
                    r.ticks(5);
                    put(&mut r, run, 1.0);
                    r.run(60.0);
                    if r.get(state) == 2.0 || r.ship.ports[out].gave > 0.0 {
                        bad.push(format!("{}: {id} arranca con {} abierto", kind.id, r.ship.panels.controls[cut].id));
                    }
                    r.set(run, 0.0);
                    put(&mut r, cut, 1.0);
                    r.until(120.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
                }
                None => bad.push(format!("{}: nada de un panel corta la corriente de arranque de {id}", kind.id)),
            }
            // ---- its fuel shut off: it fails
            if let Some(fuel) = port(&r, m, "combustible") {
                match feeds(&r, fuel, &gives).first().copied() {
                    Some(valve) => {
                        put(&mut r, valve, 0.0);
                        r.ticks(5);
                        put(&mut r, run, 1.0);
                        if !r.until(120.0, |r| r.get(state) == 3.0) || r.ship.ports[out].gave > 0.0 {
                            bad.push(format!("{}: {id} sin combustible ({} cerrada) queda en estado {} (debía fallar)", kind.id, r.ship.panels.controls[valve].id, r.get(state)));
                        }
                        r.set(run, 0.0);
                        put(&mut r, valve, 1.0);
                        r.ticks(5);
                    }
                    None => bad.push(format!("{}: ninguna válvula de un panel corta el combustible de {id}", kind.id)),
                }
            }
            // ---- both: it runs and gives
            put(&mut r, run, 1.0);
            if !r.until(300.0, |r| r.get(state) == 2.0 && r.ship.ports[out].gave > 0.0) {
                bad.push(format!("{}: {id} con corriente de arranque y combustible no llega a dar corriente (estado {})", kind.id, r.get(state)));
            }
            r.set(run, 0.0);
            r.ticks(5);
            if r.ship.ports[out].gave > 0.0 {
                bad.push(format!("{}: {id} parado sigue dando corriente", kind.id));
            }
            eprintln!("{}: generador {id}: sin arranque, sin combustible y con todo", kind.id);
        }
    }
    report("arranque de generadores", &bad);
}

#[test]
fn a_reactor_needs_control_power_and_coolant_and_trips_without_them() {
    use lunar_machines::models::reactor::{OFF, ONLINE, SCRAM};
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let gives = givers(&mut r);
        let owners = r.owners();
        for m in machines_of(&r, "reactor").collect::<Vec<_>>() {
            let id = kind.machines[m].id.clone();
            let (Some(run), Some(rearm)) = (handle(&r, m, "marcha"), handle(&r, m, "rearme")) else {
                bad.push(format!("{}: el reactor {id} no tiene en ningún panel su marcha o su rearme", kind.id));
                continue;
            };
            let (state, n) = (telemetry(&r, m, "estado"), telemetry(&r, m, "n"));
            let control = port(&r, m, "control").map(|p| feeds(&r, p, &gives)).unwrap_or_default();
            // the pumps of its coolant: whatever gives into the network its flow port is on, each
            // with its own switch on a panel
            let flow = port(&r, m, "flujo").expect("un reactor con refrigerante");
            let pumps: Vec<usize> = r.ports_on(usize::from(r.ship.ports[flow].net)).filter(|(k, _)| gives[*k]).filter_map(|(k, _)| if let Owner::Machine(p, _) = &owners[k] { handle(&r, *p, "marcha") } else { None }).collect();
            let (Some(&breaker), false) = (control.first(), pumps.is_empty()) else {
                bad.push(format!("{}: {id}: nada de un panel corta su corriente de control ({} interruptores) o sus bombas ({})", kind.id, control.len(), pumps.len()));
                continue;
            };
            let online = |r: &mut Rig| r.until(900.0, |r| r.get(state) == ONLINE);
            // back to off from whatever it is in: stopped, cool enough, its key turned
            let reset = |r: &mut Rig| {
                r.set(run, 0.0);
                r.until(60.0, |r| r.get(state) == OFF || r.get(state) == SCRAM);
                put(r, rearm, 1.0);
                let off = r.until(1800.0, |r| r.get(state) == OFF);
                r.set(rearm, 0.0);
                off
            };
            // ---- no control power: it stays off
            r.click(breaker);
            r.ticks(5);
            put(&mut r, run, 1.0);
            r.run(20.0);
            if r.get(state) != OFF {
                bad.push(format!("{}: {id} arranca con {} abierto (estado {})", kind.id, r.ship.panels.controls[breaker].id, r.get(state)));
            }
            r.set(run, 0.0);
            put(&mut r, breaker, 1.0);
            r.until(120.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
            reset(&mut r);
            // ---- no coolant flowing: it stays off
            let were: Vec<f64> = pumps.iter().map(|&p| r.value(p)).collect();
            pumps.iter().for_each(|&p| put(&mut r, p, 0.0));
            r.until(60.0, |r| r.ship.ports[flow].share < 0.5 || !r.ship.ports[flow].fed);
            put(&mut r, run, 1.0);
            r.run(20.0);
            if r.get(state) != OFF {
                bad.push(format!("{}: {id} arranca sin refrigerante circulando (estado {})", kind.id, r.get(state)));
            }
            r.set(run, 0.0);
            pumps.iter().zip(&were).for_each(|(&p, &v)| put(&mut r, p, v.max(1.0)));
            r.until(60.0, |r| r.ship.ports[flow].share >= 0.5 && r.ship.ports[flow].fed);
            reset(&mut r);
            // ---- both: it comes on line; its control power gone, it trips at once
            put(&mut r, run, 1.0);
            if !online(&mut r) {
                bad.push(format!("{}: {id} con corriente de control y refrigerante no llega a estar en línea (estado {})", kind.id, r.get(state)));
                continue;
            }
            r.click(breaker);
            r.ticks(10);
            if r.get(state) != SCRAM {
                bad.push(format!("{}: {id} en línea no hace SCRAM al perder la corriente de control (estado {})", kind.id, r.get(state)));
            }
            put(&mut r, breaker, 1.0);
            r.until(120.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
            // tripped, it does not come back by itself; rearmed, it does
            r.run(5.0);
            if r.get(state) != SCRAM {
                bad.push(format!("{}: {id} sale solo del SCRAM al volver la corriente (estado {})", kind.id, r.get(state)));
            }
            if !reset(&mut r) {
                bad.push(format!("{}: {id} no se rearma tras un SCRAM en frío (estado {})", kind.id, r.get(state)));
                continue;
            }
            // ---- on line and at power: its coolant stopped, it trips
            put(&mut r, run, 1.0);
            if !online(&mut r) || !r.until(900.0, |r| r.get(n) > 0.12) {
                bad.push(format!("{}: {id} rearmado no vuelve a subir de potencia (estado {}, n {:.3})", kind.id, r.get(state), r.get(n)));
                continue;
            }
            pumps.iter().for_each(|&p| put(&mut r, p, 0.0));
            if !r.until(120.0, |r| r.get(state) == SCRAM) {
                bad.push(format!("{}: {id} a potencia no hace SCRAM al parar sus bombas (estado {}, caudal {:.2})", kind.id, r.get(state), r.ship.ports[flow].share));
            }
            eprintln!("{}: reactor {id}: sin control, sin refrigerante, en línea, SCRAM por control, rearme, SCRAM por caudal (t = {:.0} s de nave)", kind.id, r.ship.t);
        }
        bad.extend(r.broken());
    }
    report("reactor", &bad);
}

#[test]
fn a_leg_held_by_hydraulic_locks_does_not_move_without_pressure() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        // in flight: nothing holds the gear lever for the weight on it
        r.w = airborne();
        let gives = givers(&mut r);
        let owners = r.owners();
        let locked: Vec<usize> = (0..kind.actuators.len()).filter(|&a| kind.actuators[a].def.bloqueos.iter().any(|l| l.liberacion.starts_with("hidr"))).collect();
        if locked.is_empty() {
            continue;
        }
        // one order moves several: each order once
        let mut orders: Vec<SignalId> = locked.iter().filter_map(|&a| r.find(kind.actuators[a].def.control.orden.as_deref().unwrap_or(&format!("{}.orden", kind.actuators[a].id)))).collect();
        orders.sort_unstable();
        orders.dedup();
        for order in orders {
            let Some(lever) = r.control_of(order) else {
                bad.push(format!("{}: la orden '{}' no tiene mando en ningún panel", kind.id, r.name(order)));
                continue;
            };
            let mine: Vec<usize> = locked.iter().copied().filter(|&a| r.find(kind.actuators[a].def.control.orden.as_deref().unwrap_or(&format!("{}.orden", kind.actuators[a].id))) == Some(order)).collect();
            let at = |r: &Rig| mine.iter().map(|&a| r.ship.joints[kind.actuators[a].joint].q).collect::<Vec<f64>>();
            let rest = at(&r);
            let to = if r.value(lever) >= 0.5 { 0.0 } else { 1.0 };
            put(&mut r, lever, to);
            if (r.value(lever) - to).abs() > 1e-9 {
                bad.push(format!("{}: {} no se deja mover en vuelo ({:?})", kind.id, r.ship.panels.controls[lever].id, r.ship.panels.controls[lever].last));
                continue;
            }
            // no pressure: locked where they are
            r.run(8.0);
            for (&a, (was, now)) in mine.iter().zip(rest.iter().zip(at(&r))) {
                if (was - now).abs() > 1e-3 {
                    bad.push(format!("{}: {} se mueve sin presión hidráulica (de {was:.3} a {now:.3})", kind.id, kind.actuators[a].id));
                }
            }
            // pressure: the pumps of the network their fluid is on, each by its own switch
            let net = r.ship.ports[r.ship.ports.iter().zip(&owners).position(|(p, o)| matches!(o, Owner::Actuator(a, _) if *a == mine[0]) && r.kind.nets.get(usize::from(p.net)).is_some_and(|n| n.medium == Medium::Hidraulico)).expect("un actuador hidráulico en una red hidráulica")].net;
            let pumps: Vec<usize> = (0..kind.machines.len()).filter(|&m| r.ship.machines[m].ports.clone().any(|k| r.ship.ports[k].net == net)).filter_map(|m| handle(&r, m, "marcha")).collect();
            pumps.iter().for_each(|&p| put(&mut r, p, 1.0));
            let moved = r.until(300.0, |r| mine.iter().zip(&rest).all(|(&a, was)| (r.ship.joints[kind.actuators[a].joint].q - was).abs() > 0.05));
            if !moved {
                bad.push(format!("{}: con {} bombas en marcha y {} en {to}, no se mueven: {:?} (estaban en {rest:?})", kind.id, pumps.len(), r.ship.panels.controls[lever].id, at(&r)));
            }
            let _ = &gives;
            eprintln!("{}: {} actuadores con blocaje hidráulico tras '{}': quietos sin presión, se mueven con ella", kind.id, mine.len(), r.name(order));
        }
    }
    report("blocajes hidráulicos", &bad);
}

#[test]
fn a_magnet_holds_with_the_ship_dead_and_switches_only_with_charge() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let gives = givers(&mut r);
        for m in machines_of(&r, "electroiman").collect::<Vec<_>>() {
            let id = kind.machines[m].id.clone();
            let Some(grip) = handle(&r, m, "agarrar") else {
                bad.push(format!("{}: el imán {id} no tiene mando en ningún panel", kind.id));
                continue;
            };
            let (on, ready) = (telemetry(&r, m, "activo"), telemetry(&r, m, "listo"));
            let Some(cut) = port(&r, m, "energia").map(|p| feeds(&r, p, &gives)).and_then(|c| c.first().copied()) else {
                bad.push(format!("{}: nada de un panel corta la corriente del imán {id}", kind.id));
                continue;
            };
            r.until(300.0, |r| r.get(ready) >= 0.5);
            put(&mut r, grip, 1.0);
            if !r.until(30.0, |r| r.get(on) >= 0.5) {
                bad.push(format!("{}: {id} cargado no agarra", kind.id));
                continue;
            }
            // dead: it holds
            r.click(cut);
            r.run(20.0);
            if r.get(on) < 0.5 {
                bad.push(format!("{}: {id} suelta al quedarse sin corriente", kind.id));
            }
            // and switches only while its capacitor has a pulse left: then no more
            let mut switches = 0;
            for _ in 0..40 {
                let was = r.get(on);
                let had = r.get(ready) >= 0.5;
                let k = r.value(grip);
                put(&mut r, grip, if k >= 0.5 { 0.0 } else { 1.0 });
                r.run(1.0);
                let did = (r.get(on) - was).abs() > 0.5;
                switches += usize::from(did);
                if did && !had {
                    bad.push(format!("{}: {id} conmuta sin pulso en el condensador", kind.id));
                }
                if !had {
                    break;
                }
            }
            if r.get(ready) >= 0.5 {
                bad.push(format!("{}: {id} sin corriente sigue conmutando tras {switches} cambios: su condensador no se gasta", kind.id));
            }
            // fed again: it can
            put(&mut r, cut, 1.0);
            if !r.until(600.0, |r| r.get(ready) >= 0.5) {
                bad.push(format!("{}: {id} con corriente no recarga su condensador", kind.id));
            }
            let was = r.get(on);
            let k = r.value(grip);
            put(&mut r, grip, if k >= 0.5 { 0.0 } else { 1.0 });
            if !r.until(30.0, |r| (r.get(on) - was).abs() > 0.5) {
                bad.push(format!("{}: {id} recargado no conmuta", kind.id));
            }
            eprintln!("{}: imán {id}: sujeta sin corriente, {switches} cambios con lo que le quedaba, vuelve con corriente", kind.id);
        }
    }
    report("imanes", &bad);
}
