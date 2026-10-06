//! Warnings and alarms, on every ship: what its black box logs (`alarmas`) and what its
//! annunciators light (`avisos`).
//! - a ship as it is put down, left alone, raises none;
//! - a blackout (every contactor a hand has, opened) raises whatever watches its buses; power
//!   back and the alarm acknowledged, everything is as before: nothing stays lit, no horn sounds;
//! - a room holed raises a warning; mended, what watched the leak clears;
//! - a breaker pulled is announced, and stops being when it is pushed back;
//! - the failure of every main engine, of every reactor and of every generator is watched by an
//!   alarm, a warning or a lamp (not only written on a page nobody has up).
//! What each cause is and how it is made is worked out from the ship (its switches, its hull,
//! its machines): nothing names an alarm.
mod comun;
use comun::*;
use lunar_controls::indicator::IndKind;
use lunar_machines::{EdgeKind, net::Medium};
use lunar_signals::{Eval, Program, SignalId, compile_in};
use std::collections::BTreeSet;

/// The alarms of a ship as programs, each with its state.
struct Alarms {
    list: Vec<(String, Program, Vec<f64>)>,
    eval: Eval,
}

impl Alarms {
    fn new(r: &Rig) -> Alarms {
        let list = r.kind.def.alarmas.iter().map(|(text, cond)| {
            let p = compile_in(cond, &r.ship.store).unwrap_or_else(|e| panic!("{}: alarma '{text}': {}", r.id(), e.0));
            let slots = vec![0.0; p.slots];
            (text.clone(), p, slots)
        });
        Alarms { list: list.collect(), eval: Eval::default() }
    }
}

/// Everything that is telling the crew something now: the alarms whose condition holds and the
/// annunciator cells that are up (lit or not: a dead panel's are up all the same).
fn telling(r: &Rig, alarms: &mut Alarms) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (text, p, slots) in &mut alarms.list {
        if alarms.eval.run(p, &r.ship.store, slots, lunar_ship::ship::TICK, r.ship.t) >= 0.5 {
            out.insert(format!("alarma «{text}»"));
        }
    }
    for i in &r.ship.panels.indicators {
        if let IndKind::Annunciator { cells, .. } = &i.ind.kind {
            for (c, st) in cells.iter().zip(&i.st.cells) {
                if st.active {
                    out.insert(format!("aviso {}", c.label));
                }
            }
        }
    }
    out
}

/// Run `secs`, the alarms followed tick by tick (some wait: `for`), and what was told at any time.
fn follow(r: &mut Rig, alarms: &mut Alarms, secs: f64) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    for _ in 0..(secs / lunar_ship::ship::TICK) as usize {
        r.tick();
        all.extend(telling(r, alarms));
    }
    all
}

/// The acknowledge button of its annunciators pressed (and the loops it sounds with, for whoever
/// checks they stop).
fn acknowledge(r: &mut Rig) {
    let acks: Vec<SignalId> = r.ship.panels.indicators.iter().filter_map(|i| if let IndKind::Annunciator { ack, .. } = &i.ind.kind { *ack } else { None }).collect();
    for k in acks.into_iter().filter_map(|s| r.control_of(s)).collect::<BTreeSet<_>>() {
        r.click(k);
    }
    r.ticks(5);
}

fn horns(r: &Rig) -> Vec<String> {
    r.ship.store.ids().filter(|s| r.name(*s).starts_with("sonido.alarma") && r.get(*s) >= 0.5).map(|s| r.name(s).to_string()).collect()
}

#[test]
fn a_ship_left_alone_raises_nothing() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let mut alarms = Alarms::new(&r);
        // (as it is put down it boots: its flight computer is not up for a moment, and its
        // annunciator says so until it is. Once whatever that is has gone, nothing more)
        let mut booting = BTreeSet::new();
        let quiet = (0..15_000).any(|_| {
            r.tick();
            let now = telling(&r, &mut alarms);
            booting.extend(now.iter().cloned());
            now.is_empty()
        });
        if !quiet {
            bad.push(format!("{}: recién puesta y sin tocarla no deja de decir: {}", kind.id, telling(&r, &mut alarms).into_iter().collect::<Vec<_>>().join(", ")));
        }
        if !booting.is_empty() {
            eprintln!("{}: al ponerla, mientras arranca ({:.1} s): {}", kind.id, r.ship.t, booting.iter().cloned().collect::<Vec<_>>().join(", "));
        }
        let told = follow(&mut r, &mut alarms, 30.0);
        for t in &told {
            bad.push(format!("{}: en reposo: {t}", kind.id));
        }
        for e in r.ship.blackbox.entries.iter().filter(|e| e.level >= 1) {
            bad.push(format!("{}: en reposo la caja negra apunta (nivel {}): {}", kind.id, e.level, e.text));
        }
        bad.extend(horns(&r).into_iter().map(|h| format!("{}: en reposo suena {h}", kind.id)));
        eprintln!("{}: {} alarmas y {} avisos, ninguno en reposo", kind.id, alarms.list.len(), r.ship.panels.indicators.iter().map(|i| i.st.cells.len()).sum::<usize>());
    }
    report("avisos en reposo", &bad);
}

#[test]
fn a_blackout_is_told_and_power_back_clears_it() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let mut alarms = Alarms::new(&r);
        let before = follow(&mut r, &mut alarms, 5.0);
        // every contactor a hand has on an electrical network (not the breakers): open
        let contactors: Vec<usize> = r.switch_controls().into_iter().filter(|&(k, n, _)| kind.nets[n].medium == Medium::Electrico && r.ctl_kind(k) != "disyuntor" && r.value(k) >= 0.5).map(|x| x.0).collect();
        contactors.iter().for_each(|&k| {
            r.set(k, 0.0);
        });
        let during = follow(&mut r, &mut alarms, 30.0);
        let raised: Vec<&String> = during.difference(&before).collect();
        // (whoever watches a bus: an alarm or a warning that reads the voltage of a named node)
        let watches = kind.def.alarmas.values().any(|c| kind.nets.iter().filter(|n| n.medium == Medium::Electrico).any(|n| c.contains(&format!("{}.", n.name)))) || !r.ship.panels.indicators.iter().all(|i| i.st.cells.is_empty());
        if raised.is_empty() && watches {
            bad.push(format!("{}: abiertos {} contactores (apagón total) y ni una alarma ni un aviso", kind.id, contactors.len()));
        }
        // power back, the alarm acknowledged: as it was
        contactors.iter().for_each(|&k| {
            r.set(k, 1.0);
        });
        r.until(300.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
        follow(&mut r, &mut alarms, 5.0);
        acknowledge(&mut r);
        let mut left = BTreeSet::new();
        let cleared = (0..12_000).any(|_| {
            r.tick();
            left = telling(&r, &mut alarms).difference(&before).cloned().collect();
            left.is_empty()
        });
        if !cleared {
            bad.push(format!("{}: vuelta la corriente y reconocida la alarma siguen: {}", kind.id, left.iter().cloned().collect::<Vec<_>>().join(", ")));
        }
        acknowledge(&mut r);
        bad.extend(horns(&r).into_iter().map(|h| format!("{}: vuelta la corriente y reconocida la alarma sigue sonando {h}", kind.id)));
        for i in &r.ship.panels.indicators {
            if i.ind.alarming(&i.st) {
                bad.push(format!("{}: vuelta la corriente y reconocida la alarma queda un aviso sin reconocer", kind.id));
            }
        }
        eprintln!("{}: apagón con {} contactores: {} cosas dichas ({}); con corriente otra vez, ninguna", kind.id, contactors.len(), raised.len(), raised.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    report("apagón", &bad);
}

#[test]
fn a_holed_room_is_told_and_mended_the_leak_warnings_clear() {
    let mut bad = Vec::new();
    for kind in fleet().kinds.iter().filter(|k| !k.compartments.is_empty()) {
        for (c, room) in kind.compartments.iter().enumerate() {
            let mut r = fleet().rig(kind);
            if r.ship.atmos.pressure(c) < 30e3 || room.hull.is_empty() {
                continue;
            }
            let mut alarms = Alarms::new(&r);
            let before = follow(&mut r, &mut alarms, 3.0);
            // a plate of its hull cracked through: it hisses out
            let plate = room.hull[room.hull.len() / 2] as usize;
            r.s.parts[plate].hp = r.s.parts[plate].max_hp * 0.04;
            let during = follow(&mut r, &mut alarms, 20.0);
            let raised: Vec<&String> = during.difference(&before).collect();
            let sealed = r.sig(&format!("{}.estanco", room.id));
            if raised.is_empty() {
                bad.push(format!("{}: {} pierde aire por una chapa rajada ({:.1} kPa menos en 20 s) y nada lo dice", kind.id, room.name, (room.pressure - r.ship.atmos.pressure(c)) / 1e3));
            }
            if sealed >= 0.5 && r.s.parts[plate].alive {
                bad.push(format!("{}: {} con una chapa rajada sigue diciéndose estanca", kind.id, room.name));
            }
            // mended (whatever the pressure tore meanwhile, too): the leak is over, and what
            // watched it lets go
            for p in 0..r.s.parts.len() {
                if !r.s.parts[p].alive || r.s.parts[p].hp < r.s.parts[p].max_hp {
                    r.mend(p);
                }
            }
            let leak_only = |p: &Program| !p.inputs.is_empty() && p.inputs.iter().all(|s| [".fuga", ".estanco", ".salida", ".choque"].iter().any(|f| r.name(*s).ends_with(f)));
            let watching: Vec<String> = alarms.list.iter().filter(|a| leak_only(&a.1)).map(|a| format!("alarma «{}»", a.0)).collect();
            let mut left = Vec::new();
            let cleared = (0..15_000).any(|_| {
                r.tick();
                let now = telling(&r, &mut alarms);
                left = watching.iter().filter(|w| now.contains(*w)).cloned().collect();
                left.is_empty() && r.sig(&format!("{}.estanco", room.id)) >= 0.5
            });
            if !cleared {
                bad.push(format!("{}: {} reparada: estanco = {}, y siguen {}", kind.id, room.name, r.sig(&format!("{}.estanco", room.id)), left.join(", ")));
            }
            eprintln!("{}: {} con una chapa rajada: {}", kind.id, room.name, raised.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        }
    }
    report("fugas", &bad);
}

#[test]
fn a_breaker_pulled_is_announced_until_it_is_pushed_back() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let mut alarms = Alarms::new(&r);
        let before = follow(&mut r, &mut alarms, 3.0);
        let breakers: Vec<usize> = (0..r.ship.panels.controls.len()).filter(|&k| r.ctl_kind(k) == "disyuntor" && r.ship.panels.controls[k].breaker.is_some()).collect();
        let mut told = 0;
        for &k in &breakers {
            let id = r.ship.panels.controls[k].id.clone();
            r.click(k);
            let during = follow(&mut r, &mut alarms, 1.0);
            if during.difference(&before).next().is_none() {
                bad.push(format!("{}: sacado el disyuntor {id}, ni una alarma ni un aviso lo dicen", kind.id));
            } else {
                told += 1;
            }
            r.click(k);
            if r.value(k) < 0.5 {
                bad.push(format!("{}: el disyuntor {id} sacado a mano no se deja volver a meter ({:?})", kind.id, r.ship.panels.controls[k].last));
                r.set(k, 1.0);
            }
            // (what its circuit fed comes back — a computer boots — before the next one)
            r.until(300.0, |r| r.ship.panels.panels.iter().all(|p| p.powered && p.linked));
            acknowledge(&mut r);
            let mut left = BTreeSet::new();
            if !(0..6000).any(|_| {
                r.tick();
                left = telling(&r, &mut alarms).difference(&before).cloned().collect();
                left.is_empty()
            }) {
                bad.push(format!("{}: metido otra vez el disyuntor {id} siguen: {}", kind.id, left.iter().cloned().collect::<Vec<_>>().join(", ")));
            }
        }
        eprintln!("{}: {told} de {} disyuntores anunciados al sacarlos", kind.id, breakers.len());
    }
    report("disyuntores sin aviso", &bad);
}

/// The failure of what the ship flies and lives on is watched where the crew looks without
/// asking: an alarm of the black box, a warning of an annunciator, a lamp.
#[test]
fn the_failure_of_every_engine_reactor_and_generator_is_watched() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let r = fleet().rig(kind);
        let mut watched: BTreeSet<SignalId> = BTreeSet::new();
        for cond in kind.def.alarmas.values() {
            watched.extend(compile_in(cond, &r.ship.store).unwrap().inputs);
        }
        for i in &r.ship.panels.indicators {
            match &i.ind.kind {
                IndKind::Annunciator { cells, .. } => cells.iter().for_each(|c| watched.extend(&c.cond.inputs)),
                IndKind::Lamp { rules } => rules.iter().for_each(|x| watched.extend(&x.cond.inputs)),
                _ => {}
            }
        }
        // back through the derived logic
        let derived: Vec<(SignalId, Program)> = kind.def.derivadas.iter().filter_map(|(n, src)| Some((r.find(n)?, compile_in(src, &r.ship.store).ok()?))).collect();
        loop {
            let n = watched.len();
            for (target, p) in &derived {
                if watched.contains(target) {
                    watched.extend(&p.inputs);
                }
            }
            if watched.len() == n {
                break;
            }
        }
        let mut count = 0;
        for m in kind.machines.iter().filter(|m| matches!(m.def.modelo.as_str(), "motor_cohete" | "reactor" | "generador")) {
            count += 1;
            let state = format!("{}.estado", m.def.params.get("telemetria").and_then(|v| v.as_str()).unwrap_or(&m.id));
            if !r.find(&state).is_some_and(|s| watched.contains(&s)) {
                bad.push(format!("{}: el fallo de {} ({}) no lo vigila ninguna alarma, aviso ni lámpara: '{state}' solo se ve si alguien pone su página", kind.id, m.id, m.def.modelo));
            }
        }
        // and every breaker with a handle is in what announces a breaker out
        for plan in kind.nets.iter().filter(|n| n.medium == Medium::Electrico) {
            for e in plan.edges.iter().filter(|e| e.kind == EdgeKind::Switch && e.measure) {
                if let Some(s) = e.signal.as_ref().and_then(|s| r.find(s))
                    && !watched.contains(&s)
                {
                    bad.push(format!("{}: el disyuntor {} no está en ninguna alarma ni aviso", kind.id, e.id));
                }
            }
        }
        eprintln!("{}: {count} motores, reactores y generadores vigilados", kind.id);
    }
    report("fallos sin vigilar", &bad);
}

/// The black box says a thing once: no two alarms of a ship watch the very same condition.
#[test]
fn no_two_alarms_say_the_same_thing() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let alarms: Vec<(&String, &String)> = kind.def.alarmas.iter().collect();
        for (i, (text, cond)) in alarms.iter().enumerate() {
            for (other, same) in alarms.iter().skip(i + 1).filter(|x| x.1 == *cond) {
                bad.push(format!("{}: las alarmas «{text}» y «{other}» son la misma condición ({same}): la caja negra lo apunta dos veces", kind.id));
            }
        }
    }
    report("alarmas repetidas", &bad);
}
