//! Every breaker, contactor and valve a hand can work, on every ship, worked on the ship as it
//! stands (batteries on, tanks full): opened, its edge of the network opens with it, what is
//! behind it loses its feed and nothing else on that network does, the voltage read at every
//! named junction behind it falls to nothing; shut again, all of it comes back as it was.
//! What is behind a switch is not written anywhere: it is worked out from the network's plan
//! (what no longer reaches any source without that edge), so a new ship or a new circuit needs
//! no line here.
mod comun;
use comun::*;
use lunar_machines::net::Medium;

/// The ports of `net` (their place in the ship's list) and whether each is fed now.
fn fed(r: &Rig, net: usize) -> Vec<(usize, bool)> {
    r.ports_on(net).map(|(k, p)| (k, p.fed)).collect()
}

/// The nodes of `net` holding something that feeds an island now (a producer, a store).
fn feeding(r: &Rig, net: usize) -> Vec<u32> {
    r.ports_on(net).filter(|(_, p)| p.produce > 0.0 || p.store_out > 0.0 || ((p.store_out > 0.0 || p.store_in > 0.0) && p.potential > 0.0)).map(|(_, p)| p.node).collect()
}

/// A switch put open or shut as a hand does: a click where a click does it, else set there.
fn put(r: &mut Rig, k: usize, shut: bool) -> bool {
    r.uncover(k);
    if (r.value(k) >= 0.5) != shut {
        r.click(k);
    }
    if (r.value(k) >= 0.5) != shut {
        r.set(k, if shut { 1.0 } else { 0.0 });
    }
    r.ticks(3);
    (r.value(k) >= 0.5) == shut
}

#[test]
fn every_switch_opened_cuts_what_is_behind_it_and_shut_again_gives_it_back() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(3.0);
        let controls = r.switch_controls();
        let (mut worked, mut dark) = (0, 0);
        for &(k, n, e) in &controls {
            let plan = &kind.nets[n];
            let what = format!("{}: {} (interruptor {} de la red {})", kind.id, r.ship.panels.controls[k].id, plan.edges[e].id, plan.name);
            let was_shut = r.value(k) >= 0.5;
            // the signal the handle writes and the edge agree before anything is touched
            if r.ship.nets[n].edges[e].closed != was_shut {
                bad.push(format!("{what}: el mando está {} y la red lo tiene {}", state(was_shut), state(r.ship.nets[n].edges[e].closed)));
                continue;
            }
            // from shut: what its opening must cut, from the plan and what feeds the network now
            if !was_shut && !put(&mut r, k, true) {
                bad.push(format!("{what}: no se deja cerrar"));
                continue;
            }
            let before = fed(&r, n);
            let sources = feeding(&r, n);
            let live: Vec<bool> = r.ship.nets[n].edges.iter().map(|x| x.conducts()).collect();
            let reached = r.reach(n, &sources, &|x| x != e && live[x]);
            if !put(&mut r, k, false) {
                bad.push(format!("{what}: no se deja abrir ({:?})", r.ship.panels.controls[k].last));
                continue;
            }
            worked += 1;
            if r.ship.nets[n].edges[e].closed {
                bad.push(format!("{what}: abierto en el panel, la red lo sigue teniendo cerrado"));
            }
            for (&(p, was), (_, now)) in before.iter().zip(fed(&r, n)) {
                let behind = was && !reached[r.ship.ports[p].node as usize];
                dark += usize::from(behind);
                if behind && now {
                    bad.push(format!("{what}: abierto, {} sigue con alimentación", r.port_name(p)));
                } else if !behind && now != was {
                    bad.push(format!("{what}: abierto, {} {} y no está detrás de él", r.port_name(p), if now { "pasa a tener alimentación" } else { "se queda sin alimentación" }));
                }
            }
            // what a panel reads of each named junction: nothing behind it, something elsewhere
            if plan.medium == Medium::Electrico {
                for (name, &node) in &plan.names {
                    let v = r.sig(&format!("{}.{name}", plan.name));
                    if (v > 1.0) != reached[node as usize] && !sources.is_empty() {
                        bad.push(format!("{what}: abierto, {}.{name} marca {v:.1} V y {}", plan.name, if reached[node as usize] { "sigue unido a una fuente" } else { "ya no lo alimenta nada" }));
                    }
                }
            }
            // and back
            if !put(&mut r, k, true) {
                bad.push(format!("{what}: abierto no se deja volver a cerrar ({:?})", r.ship.panels.controls[k].last));
                continue;
            }
            if !r.ship.nets[n].edges[e].closed {
                bad.push(format!("{what}: cerrado en el panel, la red lo sigue teniendo abierto"));
            }
            for (&(p, was), (_, now)) in before.iter().zip(fed(&r, n)) {
                if now != was {
                    bad.push(format!("{what}: vuelto a cerrar, {} no queda como estaba ({})", r.port_name(p), if now { "ahora con alimentación" } else { "sin alimentación" }));
                }
            }
            if !was_shut {
                put(&mut r, k, false);
            }
        }
        eprintln!("{}: {worked} de {} interruptores con mando abiertos y vueltos a cerrar; {dark} puertos se quedaron a oscuras entre todos", kind.id, controls.len());
        if worked != controls.len() {
            bad.push(format!("{}: solo {worked} de {} interruptores se pudieron accionar", kind.id, controls.len()));
        }
    }
    report("interruptores", &bad);
}

fn state(shut: bool) -> &'static str {
    if shut { "cerrado" } else { "abierto" }
}

/// What a machine says of itself agrees with its network: with the switch that feeds it open, a
/// lamp is dark and a load reports itself off (`<id>.on`), whatever its own switch says.
#[test]
fn a_load_with_its_feed_cut_says_it_is_off() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(3.0);
        let owners = r.owners();
        let mut checked = 0;
        for &(k, n, e) in &r.switch_controls() {
            if kind.nets[n].medium != Medium::Electrico || r.value(k) < 0.5 {
                continue;
            }
            let sources = feeding(&r, n);
            let live: Vec<bool> = r.ship.nets[n].edges.iter().map(|x| x.conducts()).collect();
            let reached = r.reach(n, &sources, &|x| x != e && live[x]);
            // the machines behind it that were on
            let behind: Vec<(usize, String)> = r
                .ports_on(n)
                .filter(|(_, p)| !reached[p.node as usize])
                .filter_map(|(p, _)| match &owners[p] {
                    Owner::Machine(m, _) => Some((*m, format!("{}.on", kind.machines[*m].id))),
                    _ => None,
                })
                .filter(|(_, on)| r.ship.signal(on).is_some_and(|v| v > 0.5))
                .collect();
            if behind.is_empty() {
                continue;
            }
            put(&mut r, k, false);
            r.run(1.0);
            for (m, on) in &behind {
                checked += 1;
                if r.sig(on) > 0.5 {
                    bad.push(format!("{}: con {} abierto, {on} sigue diciendo que está encendida", kind.id, r.ship.panels.controls[k].id));
                }
                if r.ship.machines[*m].m.light() > 0.02 {
                    bad.push(format!("{}: con {} abierto, {} sigue luciendo ({:.2})", kind.id, r.ship.panels.controls[k].id, kind.machines[*m].id, r.ship.machines[*m].m.light()));
                }
            }
            // (each comes back in its own time — a computer boots, a wheel spins up —: none is
            // measured here, only that all do)
            put(&mut r, k, true);
            r.until(300.0, |r| behind.iter().all(|(_, on)| r.sig(on) > 0.5));
            for (_, on) in &behind {
                if r.sig(on) < 0.5 {
                    bad.push(format!("{}: con {} vuelto a cerrar, {on} no vuelve", kind.id, r.ship.panels.controls[k].id));
                }
            }
        }
        eprintln!("{}: {checked} máquinas encendidas apagadas por su interruptor y vueltas a encender", kind.id);
    }
    report("lo que dice una máquina sin alimentación", &bad);
}
