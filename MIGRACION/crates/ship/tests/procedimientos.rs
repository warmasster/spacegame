//! Nothing aboard a ship makes its crew wait an eternity, and a ship cannot grow something that
//! does without this test saying so (`docs/TIEMPOS.md`).
//!
//! Every procedure of the registry (`assets/defs/procedimientos.jsonc`) is done on every ship of
//! `assets/defs/ships` it applies to, headless, each from a cold ship (as built, what goes before
//! it done first), as a hand would: it must be done within its budget. And every ship is looked
//! over for what takes time: each joint something drives (ramps, doors, gear, radiators, masts,
//! nacelles, crane axes...) must have gone its whole travel in a procedure that kept its budget,
//! and each machine that says it has a start-up or a transition (`Machine::settling`) must have
//! been seen through it by one; a machine model that says nothing must be declared to have none
//! (`sin_transitorio`). What is let off is let off by name, with its reason (`exentos`).
//!
//! `--nocapture` prints the table (procedure · ship · seconds · budget).
//! `LUNA_TIEMPOS=medir`: nothing is held to its budget and everything is given an hour (the
//! table "before"). `LUNA_TIEMPOS_SOLO=<text>`: only the procedures whose id has that text.
use glam::{DQuat, DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{
    Ship, ShipKind, ShipLibrary, World,
    procedures::{Bindings, Checklist, Outcome, Registry, Standing, What, drive},
    ship::TICK,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

fn defs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn load() -> (Library, ShipLibrary) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships)
}

/// A cold ship: as built, its systems not yet ticked.
fn spawn(lib: &Library, kind: &Arc<ShipKind>) -> (Structure, Ship) {
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship)
}

/// One line of the table: a procedure on a ship (the slowest of its family there).
struct Row {
    proc: String,
    name: String,
    ship: String,
    /// How many of its family the ship has, the slowest (or the one that failed), and each of
    /// them (what its pattern stood for, its seconds).
    count: usize,
    worst: String,
    each: Vec<(String, Option<f64>)>,
    secs: Option<f64>,
    budget: f64,
    why: String,
}

#[derive(Default)]
struct Audit {
    rows: Vec<Row>,
    /// What was let off (ship, what, why).
    exempt: Vec<(String, String, String)>,
    problems: Vec<String>,
    /// Seconds of ship time simulated, procedures done.
    simulated: f64,
    runs: usize,
}

/// Every procedure of `reg` done on every ship (or on `only`), and what is wrong with the
/// result. `measure`: nothing held to its budget, everything given an hour.
fn audit(lib: &Library, ships: &ShipLibrary, reg: &Registry, only: Option<&str>, measure: bool) -> Audit {
    let filter = std::env::var("LUNA_TIEMPOS_SOLO").ok();
    let mut out = Audit::default();
    let hand = reg.hand();
    let kinds: Vec<&Arc<ShipKind>> = ships.kinds.iter().filter(|k| only.is_none_or(|o| k.id == o)).collect();
    // ---- what applies to each ship ----
    let bound: Vec<Bindings> = kinds.iter().map(|k| reg.bind(&spawn(lib, k).1)).collect();
    for p in &reg.procedimientos {
        if only.is_none() && !bound.iter().any(|b| b.list.iter().any(|x| x.proc == p.id)) {
            let why: Vec<String> = kinds.iter().zip(&bound).map(|(k, b)| format!("{}: {}", k.id, b.missing.iter().find(|m| m.0 == p.id || m.0.starts_with(&format!("{}:", p.id))).map_or("?", |m| m.1.as_str()))).collect();
            out.problems.push(format!("el procedimiento '{}' no se aplica a ninguna nave ({})", p.id, why.join("; ")));
        }
    }
    // ---- every one of them done, each on a cold ship of its own, the slowest first ----
    let mut jobs: Vec<(usize, usize)> = Vec::new();
    for (si, b) in bound.iter().enumerate() {
        for (i, x) in b.list.iter().enumerate() {
            if filter.as_ref().is_some_and(|f| !x.proc.contains(f.as_str())) {
                continue;
            }
            match reg.exempt(&kinds[si].id, &x.id) {
                Some(why) => out.exempt.push((kinds[si].id.clone(), x.id.clone(), why.to_string())),
                None => jobs.push((si, i)),
            }
        }
    }
    jobs.sort_by(|a, b| bound[b.0].list[b.1].budget.total_cmp(&bound[a.0].list[a.1].budget));
    let cap = |b: &lunar_ship::procedures::Bound| if measure { 3600.0 } else { (b.budget * 3.0).max(30.0) };
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, usize, Outcome)>> = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let Some(&(si, i)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) else { break };
                    let (mut s, mut ship) = spawn(lib, kinds[si]);
                    let o = drive(&bound[si].list, i, hand, &mut ship, &mut s, &cap);
                    done.lock().unwrap().push((si, i, o));
                }
            });
        }
    });
    let done = done.into_inner().unwrap();
    out.runs = done.len();
    out.simulated = done.iter().filter_map(|d| d.2.secs).sum();
    // ---- the table: by procedure, by ship, the slowest of each family ----
    let kept = |si: usize, i: usize, o: &Outcome| o.secs.is_some_and(|t| t <= bound[si].list[i].budget);
    for p in &reg.procedimientos {
        for (si, k) in kinds.iter().enumerate() {
            let mine: Vec<&(usize, usize, Outcome)> = done.iter().filter(|d| d.0 == si && bound[si].list[d.1].proc == p.id).collect();
            // (one that did not finish is the worst; then the slowest)
            let Some(worst) = mine.iter().max_by(|a, b| a.2.secs.unwrap_or(f64::MAX).total_cmp(&b.2.secs.unwrap_or(f64::MAX))) else { continue };
            let b = &bound[si].list[worst.1];
            let mut each: Vec<(String, Option<f64>)> = mine.iter().map(|d| (bound[si].list[d.1].id.strip_prefix(&format!("{}:", p.id)).unwrap_or("").to_string(), d.2.secs)).collect();
            each.sort_by(|a, b| a.0.cmp(&b.0));
            let name = if mine.len() == 1 { b.name.clone() } else { p.nombre.replace("{1}", "*").replace("{2}", "*") };
            out.rows.push(Row { proc: p.id.clone(), name, ship: k.id.clone(), count: mine.len(), worst: b.id.clone(), each, secs: worst.2.secs, budget: b.budget, why: worst.2.why.clone() });
            if measure {
                continue;
            }
            for d in mine {
                let b = &bound[si].list[d.1];
                match d.2.secs {
                    Some(t) if t <= b.budget => {}
                    Some(t) => out.problems.push(format!("{}: '{}' tarda {t:.1} s y su presupuesto es de {:.0} s", k.id, b.id, b.budget)),
                    None => out.problems.push(format!("{}: '{}' no se completa: {}", k.id, b.id, d.2.why)),
                }
            }
        }
    }
    if measure || filter.is_some() {
        return out;
    }
    // ---- nothing that takes time is left out ----
    for (si, k) in kinds.iter().enumerate() {
        let (_, ship) = spawn(lib, k);
        let off = |what: &str| reg.exempt(&k.id, what).is_some();
        let passed: Vec<&Outcome> = done.iter().filter(|d| d.0 == si && kept(si, d.1, &d.2)).map(|d| &d.2).collect();
        // every joint something drives: its whole travel in a procedure that kept its budget
        for (j, plan) in k.joints.iter().enumerate() {
            let driven = k.actuators.iter().any(|a| a.joint == j) || k.closures.iter().any(|c| c.joints.contains(&j));
            if !driven || plan.spring.is_some() || plan.follows.is_some() || off(&plan.id) {
                continue;
            }
            let range = f64::from(plan.hi - plan.lo).abs();
            let far = passed.iter().filter_map(|o| o.watch.travel.get(j)).map(|t| t.1 - t.0).fold(0.0, f64::max);
            if far < range * 0.9 {
                out.problems.push(format!("{}: la articulación '{}' ({}) no hace su recorrido en ningún procedimiento con presupuesto (lo más, el {:.0} %)", k.id, plan.id, plan.name, 100.0 * far / range.max(1e-9)));
            }
        }
        // every machine: either its model has no start-up of its own (and the data says so), or
        // a procedure that kept its budget saw it through one
        let mut unknown: Vec<&str> = Vec::new();
        for (m, rt) in ship.machines.iter().enumerate() {
            let (id, model) = (k.machines[m].id.as_str(), rt.m.kind());
            match rt.m.settling() {
                None => {
                    if !reg.sin_transitorio.contains_key(model) && !unknown.contains(&model) {
                        unknown.push(model);
                        out.problems.push(format!("{}: el modelo de máquina '{model}' ({id}) no dice si tarda en algo: dale `Machine::settling` o decláralo en 'sin_transitorio'", k.id));
                    }
                }
                Some(_) => {
                    if reg.sin_transitorio.contains_key(model) && !unknown.contains(&model) {
                        unknown.push(model);
                        out.problems.push(format!("{}: el modelo '{model}' está en 'sin_transitorio' pero dice que tiene transiciones", k.id));
                    }
                    if off(id) {
                        continue;
                    }
                    let seen = passed.iter().any(|o| o.watch.settling.get(m) == Some(&true) && o.watch.unsettled.get(m) == Some(&false));
                    if !seen {
                        out.problems.push(format!("{}: la máquina '{id}' ({model}) arranca o cambia de estado y ningún procedimiento con presupuesto la lleva hasta el final", k.id));
                    }
                }
            }
        }
        // what is let off is there to be let off
        for e in reg.exentos.iter().filter(|e| e.nave == k.id) {
            for q in &e.que {
                let there = bound[si].list.iter().any(|b| b.id == *q) || k.machines.iter().any(|m| m.id == *q) || k.joints.iter().any(|j| j.id == *q);
                if !there {
                    out.problems.push(format!("{}: se exime '{q}', que no es ningún procedimiento, máquina ni articulación suyos", k.id));
                }
            }
        }
        for (what, why) in k.machines.iter().map(|m| &m.id).chain(k.joints.iter().map(|j| &j.id)).filter_map(|id| reg.exempt(&k.id, id).map(|why| (id, why))) {
            out.exempt.push((k.id.clone(), what.clone(), why.to_string()));
        }
    }
    for e in &reg.exentos {
        if !ships.kinds.iter().any(|k| k.id == e.nave) {
            out.problems.push(format!("se exime algo de la nave '{}', que no existe", e.nave));
        }
    }
    out
}

fn print(a: &Audit, title: &str) {
    eprintln!("\n{title}");
    eprintln!("{:<20} {:<10} {:>3} {:>9} {:>7}  {}", "procedimiento", "nave", "n", "segundos", "max", "");
    for r in &a.rows {
        let secs = r.secs.map_or("  --".to_string(), |t| format!("{t:.1}"));
        let mark = match r.secs {
            Some(t) if t <= r.budget => "",
            Some(_) => "  << PASA DE SU PRESUPUESTO",
            None => "  << NO SE COMPLETA",
        };
        // (a family of a few: each of them; of many: the slowest)
        let which = match r.count {
            0 | 1 => String::new(),
            2..=6 => format!(" ({})", r.each.iter().map(|(id, t)| format!("{id} {}", t.map_or("--".to_string(), |t| format!("{t:.1}")))).collect::<Vec<_>>().join(", ")),
            _ => format!(" (el peor: {})", r.worst),
        };
        eprintln!("{:<20} {:<10} {:>3} {:>9} {:>7.0}  {}{which}{mark}{}", r.proc, r.ship, r.count, secs, r.budget, r.name, if r.why.is_empty() { String::new() } else { format!(" [{}]", r.why) });
    }
    for (ship, what, why) in &a.exempt {
        eprintln!("exento: {ship} · {what}: {why}");
    }
}

#[test]
fn every_procedure_of_every_ship_keeps_its_budget_and_nothing_timed_is_left_out() {
    let t = Instant::now();
    let (lib, ships) = load();
    let reg = Registry::load(&defs()).unwrap_or_else(|e| panic!("{e}"));
    let loaded = t.elapsed().as_secs_f64();
    let measure = std::env::var("LUNA_TIEMPOS").is_ok_and(|v| v == "medir");
    let a = audit(&lib, &ships, &reg, None, measure);
    print(&a, "TIEMPOS: cada procedimiento en cada nave, desde la nave fría");
    eprintln!("{} procedimientos hechos, {:.0} s de nave simulados en {:.1} s de reloj (cargar las naves: {loaded:.1} s)", a.runs, a.simulated, t.elapsed().as_secs_f64() - loaded);
    assert!(a.problems.is_empty(), "\n{}\n", a.problems.join("\n"));
    assert!(a.runs > 0);
}

/// The guard itself: a registry with a procedure taken out leaves what that procedure covered
/// without one, and a budget cut short is a budget not kept. (On the smallest ship: it is the
/// test that is tried here, not the ship.)
#[test]
fn the_test_fails_when_something_timed_has_no_procedure_or_goes_over_its_budget() {
    let (lib, ships) = load();
    let reg = Registry::load(&defs()).unwrap_or_else(|e| panic!("{e}"));
    if std::env::var("LUNA_TIEMPOS_SOLO").is_ok() {
        return;
    }
    // without the procedure that spins its wheels up, the tug's wheels are left out
    let mut cut = reg.clone();
    cut.procedimientos.retain(|p| p.id != "giroscopos" && !p.id.starts_with("listo_"));
    let a = audit(&lib, &ships, &cut, Some("abejorro"), false);
    assert!(a.problems.iter().any(|p| p.contains("'giroscopo'")), "sin 'giroscopos' nadie echa de menos los giróscopos: {:?}", a.problems);
    // an engine start given a second: over its budget
    let mut tight = reg.clone();
    for p in tight.procedimientos.iter_mut().filter(|p| p.id == "motores") {
        p.max = lunar_signals::Q::N(1.0);
    }
    let a = audit(&lib, &ships, &tight, Some("abejorro"), false);
    assert!(a.problems.iter().any(|p| p.contains("motores:") && p.contains("presupuesto")), "un arranque de motor en 1 s pasa por bueno: {:?}", a.problems);
    // a model nobody has classified is asked about
    let mut blind = reg.clone();
    blind.sin_transitorio.remove("luz");
    let a = audit(&lib, &ships, &blind, Some("abejorro"), false);
    assert!(a.problems.iter().any(|p| p.contains("'luz'")), "un modelo sin clasificar pasa: {:?}", a.problems);
    // and a joint: with nothing that moves its ramp, the Alcotán's ramp is left out (only what
    // is needed for it is run: its doors, which need the ramp shut first, do not apply)
    let mut few = reg.clone();
    few.procedimientos.retain(|p| p.id == "antena_fuera");
    let a = audit(&lib, &ships, &few, Some("alcotan"), false);
    assert!(a.problems.iter().any(|p| p.contains("'rampa'")), "sin procedimiento para la rampa nadie la echa de menos: {:?}", a.problems);
    assert!(!a.problems.iter().any(|p| p.contains("'mastil'")), "el mástil hizo su recorrido y se le echa de menos: {:?}", a.problems);
}

/// A step an interlock refuses fails with the ship's own reason, at once: a procedure does not
/// sit waiting on something that will not happen.
#[test]
fn a_step_refused_by_an_interlock_fails_with_its_reason() {
    let (lib, ships) = load();
    let reg: Registry = lunar_core::defs::parse(
        "prueba",
        r#"{ "procedimientos": [
            { "id": "cerrar", "nombre": "Cerrar la rampa", "pasos": [ { "pulsar": "rampa_bodega/rampa" } ], "hecho": "rampa.cerrada > 0.5", "max": "60 s" },
            { "id": "abrir_con_presion", "nombre": "Abrir la rampa con la bodega a presión", "tras": ["cerrar"],
              "preparar": [ { "aire": "bodega", "a": "70 kPa" }, { "espera": "0.5 s" } ],
              "pasos": [ { "pulsar": "rampa_bodega/rampa" } ], "hecho": "rampa.abierta > 0.9", "max": "60 s" }
        ] }"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let kind = ships.get("alcotan").unwrap();
    let (mut s, mut ship) = spawn(&lib, kind);
    let b = reg.bind(&ship);
    assert_eq!(b.list.len(), 2, "{:?}", b.missing);
    let o = drive(&b.list, 1, reg.hand(), &mut ship, &mut s, &|_| 120.0);
    assert!(o.secs.is_none() && o.why.contains("presurizada"), "{o:?}");
    assert!(ship.signal("rampa.cerrada").unwrap() > 0.5);
    // and what a ship lacks is said, not guessed
    let tug = ships.get("abejorro").unwrap();
    let b = reg.bind(&spawn(&lib, tug).1);
    assert!(b.list.is_empty() && b.missing.iter().any(|m| m.1.contains("rampa_bodega/rampa")), "{:?}", b.missing);
}

/// The same procedures followed by hand are checklists: each step says the control to work,
/// what to do with it and its line in Spanish; the checklist looks at the ship only when asked,
/// ticks what is done, unticks what is undone, points at the guard before what it guards, and
/// says when what the procedure is for has come.
#[test]
fn a_checklist_follows_a_hand_through_a_procedure() {
    let (lib, ships) = load();
    let reg = Registry::load(&defs()).unwrap_or_else(|e| panic!("{e}"));
    let kind = ships.get("alcotan").unwrap();
    let (mut s, mut ship) = spawn(&lib, kind);
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), ..World::default() };
    let run = |ship: &mut Ship, s: &mut Structure, secs: f64| {
        for _ in 0..(secs / TICK).round() as usize {
            ship.update(s, &w, TICK);
        }
    };
    let act = |ship: &mut Ship, s: &Structure, k: usize, i: &Intent| {
        let kind = ship.kind.clone();
        ship.panels.intent(k, i, s, &kind, &ship.store);
    };
    run(&mut ship, &mut s, 1.0);
    let all = reg.bind(&ship);
    // ---- the reactor to full power: three controls, one of them under a guard ----
    let p = all.get("reactor_pleno:reactor").expect("el Alcotán tiene reactor");
    let lines = p.lines(&ship);
    assert_eq!(lines.len(), p.steps.len());
    for l in &lines {
        eprintln!("  [{}] {} ({:?})", l.id, l.text, l.what);
        assert!(l.control.is_some() && !l.text.is_empty() && l.id.contains('/') && !l.panel.is_empty(), "{l:?}");
    }
    assert!(lines.iter().any(|l| l.id == "reactor/marcha" && l.cover.is_some() && l.text.contains("MARCHA")), "{lines:?}");
    let mut list = Checklist::new(p, &ship);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(list.poll(p, &ship), Standing::Step(i), "paso {i}: {}", line.text);
        assert!(list.ticked()[..i].iter().all(|t| *t) && !list.ticked()[i]);
        let k = line.control.unwrap();
        // (the guard first, while it is down: the checklist points at it)
        if let Some(cover) = line.cover {
            assert_eq!(list.point(p, &ship), Some(cover), "no señala la tapa de {}", line.id);
            act(&mut ship, &s, cover, &Intent::Press { elem: 0 });
            act(&mut ship, &s, cover, &Intent::Release);
        }
        assert_eq!(list.point(p, &ship), Some(k));
        let What::Set(value) = line.what else { panic!("{line:?}") };
        act(&mut ship, &s, k, &Intent::Set { value });
        run(&mut ship, &mut s, 0.1);
    }
    // every step done: it waits for the reactor, and says when it is there
    assert_eq!(list.poll(p, &ship), Standing::Waiting);
    assert!(list.ticked().iter().all(|t| *t) && list.next() == lines.len() && list.point(p, &ship).is_none());
    let t0 = ship.t;
    while list.poll(p, &ship) != Standing::Done {
        run(&mut ship, &mut s, 0.5);
        assert!(ship.t - t0 < 120.0, "la lista no da el reactor por puesto a plena potencia");
    }
    eprintln!("  hecho a los {:.0} s", ship.t - t0);
    // undone, a step unticks: the radiators stowed again
    let k = lines[0].control.unwrap();
    act(&mut ship, &s, k, &Intent::Set { value: 0.0 });
    run(&mut ship, &mut s, 0.1);
    list.poll(p, &ship);
    assert!(!list.ticked()[0] && list.ticked()[1] && list.next() == 0);
    // ---- a push button leaves nothing to look at: its step is ticked by what its order becomes ----
    let p = all.get("rampa_cerrar").unwrap();
    let line = &p.lines(&ship)[0];
    assert_eq!((line.id.as_str(), line.what), ("rampa_bodega/rampa", What::Press));
    let mut list = Checklist::new(p, &ship);
    assert_eq!(list.poll(p, &ship), Standing::Step(0));
    let k = line.control.unwrap();
    act(&mut ship, &s, k, &Intent::Press { elem: 0 });
    run(&mut ship, &mut s, 0.2);
    act(&mut ship, &s, k, &Intent::Release);
    run(&mut ship, &mut s, 0.2);
    assert_eq!(list.poll(p, &ship), Standing::Waiting, "pulsada la rampa, su paso no se da por hecho");
    run(&mut ship, &mut s, 15.0);
    assert_eq!(list.poll(p, &ship), Standing::Done);
    // and on every ship every procedure lays out as a checklist: a line per step, in Spanish
    for k in &ships.kinds {
        let (_, ship) = spawn(&lib, k);
        let all = reg.bind(&ship);
        assert!(!all.list.is_empty(), "{}: ningún procedimiento", k.id);
        for b in &all.list {
            let lines = b.lines(&ship);
            assert_eq!(lines.len(), b.steps.len(), "{}: {}", k.id, b.id);
            assert!(!b.name.is_empty() && !b.name.contains('{') && lines.iter().all(|l| !l.text.is_empty()), "{}: {}", k.id, b.id);
            // (and a look at it costs nothing but the look)
            let mut list = Checklist::new(b, &ship);
            list.poll(b, &ship);
            assert!(list.next() <= lines.len());
        }
    }
}
