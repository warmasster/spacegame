//! The wiring of every ship of the library, read off its networks as built (no ship is named
//! here: a new one is checked the day its file is dropped in `assets/defs/ships`):
//! - every port has a way, with every switch shut, to something that gives or keeps what its
//!   network carries; no network without ports, no junction that nothing reaches or that leads
//!   nowhere;
//! - on an electrical network that has breakers, every load sits behind one of its own (pull it
//!   and that load, alone with its circuit, goes dark): none is fed two ways round its breaker;
//! - every switch of every network is worked by something (a hand on a panel, the ship's own
//!   logic) or says in the data why not; every breaker on a panel is a switch of a network, and
//!   measures the current through it;
//! - opening any one switch cuts exactly what the plan says is behind it and nothing else (the
//!   solver on the ship's real networks, every source given all it could want).
mod comun;
use comun::*;
use lunar_machines::{EdgeKind, PortIo, Solver, net::Medium};
use lunar_signals::Writer;

/// What every port of a ship was seen to do at rest and with every machine a hand can switch on
/// set going: enough for a pump, a fan or a generator to show what it is.
fn seen(r: &mut Rig) -> Seen {
    let mut seen = Seen::default();
    r.observe(2.0, &mut seen);
    r.switch_on(&|_, _| true);
    r.observe(20.0, &mut seen);
    seen
}

#[test]
fn every_port_has_a_way_to_a_source_and_no_junction_is_left_alone() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let seen = seen(&mut r);
        let owners = r.owners();
        let by_role = r.source_roles(&owners);
        for (n, plan) in kind.nets.iter().enumerate() {
            let ports: Vec<usize> = r.ports_on(n).map(|(k, _)| k).collect();
            if ports.is_empty() {
                bad.push(format!("{}: la red {} no tiene nada enchufado", kind.id, plan.name));
                continue;
            }
            // (the one table kept by hand, held to what the ports do: an electrical port that
            // gives without a source's role is a model whose role is missing from it)
            if plan.medium == Medium::Electrico {
                for &k in &ports {
                    if seen.gives[k] && !by_role[k] {
                        bad.push(format!("{}: {} da corriente y su papel no está en comun::ELECTRIC_SOURCES: añádelo", kind.id, r.port_name(k)));
                    }
                }
            }
            let gives: Vec<u32> = ports.iter().filter(|&&k| seen.gives[k] || (plan.medium == Medium::Electrico && by_role[k])).map(|&k| r.ship.ports[k].node).collect();
            if gives.is_empty() {
                bad.push(format!("{}: en la red {} ({}) nada da ni guarda: {} puertos que solo piden", kind.id, plan.name, plan.medium, ports.len()));
                continue;
            }
            let reached = r.reach(n, &gives, &|_| true);
            for &k in &ports {
                if !reached[r.ship.ports[k].node as usize] {
                    bad.push(format!("{}: {} no tiene camino a ninguna fuente ni con todos los interruptores cerrados", kind.id, r.port_name(k)));
                }
            }
            // junctions the data names: each reached, each with something plugged in or on the
            // way somewhere (as the data wrote them: a run laid two ways round a room is one run)
            let def = &kind.def.redes[&plan.name];
            for (name, &node) in &plan.names {
                let spec = format!("{}:{name}", plan.name);
                let plugged = kind.machines.iter().flat_map(|m| m.def.puertos.values()).chain(kind.actuators.iter().flat_map(|a| a.def.redes.values())).chain(kind.def.paneles.iter().flat_map(|p| p.energia.iter().chain(p.datos.iter()))).filter(|s| **s == spec).count();
                let runs = def.tramos.iter().filter(|t| t.de == *name || t.a == *name).count() + def.interruptores.iter().filter(|s| s.de == *name || s.a == *name).count();
                if !reached[node as usize] {
                    bad.push(format!("{}: el nodo {}:{name} no llega a ninguna fuente", kind.id, plan.name));
                } else if plugged == 0 && runs == 0 {
                    bad.push(format!("{}: el nodo {}:{name} está suelto (ni tramos ni aparatos)", kind.id, plan.name));
                } else if plugged == 0 && runs == 1 {
                    bad.push(format!("{}: el nodo {}:{name} es un cabo suelto (un tramo llega a él y no hay nada enchufado)", kind.id, plan.name));
                }
            }
        }
        eprintln!("{}: {} redes, {} puertos ({} sin red)", kind.id, kind.nets.len(), r.ship.ports.len(), r.ship.ports.iter().filter(|p| p.net == u16::MAX).count());
    }
    report("cableado sin fuente o suelto", &bad);
}


#[test]
fn every_load_on_a_network_with_breakers_sits_behind_one_of_its_own() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let r = fleet().rig(kind);
        let owners = r.owners();
        let by_role = r.source_roles(&owners);
        for n in r.nets_of(Medium::Electrico) {
            let plan = &kind.nets[n];
            if !plan.edges.iter().any(|e| e.measure) {
                continue;
            }
            let ports: Vec<usize> = r.ports_on(n).map(|(k, _)| k).collect();
            let sources: Vec<u32> = ports.iter().filter(|&&k| by_role[k]).map(|&k| r.ship.ports[k].node).collect();
            // each breaker pulled alone: the loads it leaves without any source are its own
            let loads: Vec<usize> = ports.iter().copied().filter(|&k| !by_role[k]).collect();
            let mut guarded = vec![false; r.ship.ports.len()];
            let mut circuits = 0;
            for (e, edge) in plan.edges.iter().enumerate().filter(|(_, e)| e.kind == EdgeKind::Switch && e.measure) {
                let reached = r.reach(n, &sources, &|k| k != e);
                let own: Vec<usize> = loads.iter().copied().filter(|&k| !reached[r.ship.ports[k].node as usize]).collect();
                // and no breaker guards nothing
                if own.is_empty() {
                    bad.push(format!("{}: el disyuntor {} de la red {} no protege ninguna carga: nada cuelga de su circuito", kind.id, edge.id, plan.name));
                }
                circuits += 1;
                own.iter().for_each(|&k| guarded[k] = true);
            }
            for &k in loads.iter().filter(|&&k| !guarded[k]) {
                bad.push(format!("{}: {} no está detrás de ningún disyuntor (o le llega corriente por dos)", kind.id, r.port_name(k)));
            }
            eprintln!("{}: red {}: {} cargas en {circuits} circuitos con disyuntor", kind.id, plan.name, loads.len());
        }
    }
    report("cargas sin disyuntor propio", &bad);
}

#[test]
fn every_switch_is_worked_by_something_and_every_breaker_on_a_panel_is_a_switch() {
    let (mut bad, mut no_handle) = (Vec::new(), Vec::new());
    for kind in &fleet().kinds {
        let r = fleet().rig(kind);
        for plan in &kind.nets {
            for e in plan.edges.iter().filter(|e| e.kind == EdgeKind::Switch) {
                let what = format!("{}: el interruptor {} de la red {}", kind.id, e.id, plan.name);
                let Some(sig) = e.signal.as_ref().and_then(|s| r.find(s)) else {
                    bad.push(format!("{what} no tiene señal"));
                    continue;
                };
                match (&r.ship.store.meta(sig).writer, r.control_of(sig)) {
                    (_, Some(k)) => {
                        // a breaker's handle senses the current through its own edge, so that edge is measured
                        let c = &r.ship.panels.controls[k];
                        if c.mech.kind() == "disyuntor" && !e.measure {
                            bad.push(format!("{what} lo lleva el disyuntor {} y no mide la corriente (\"medir\": true): no saltaría nunca", c.id));
                        }
                    }
                    (Writer::None, None) if kind.def.senales.contains_key(r.name(sig)) => {
                        // declared: shut (or open) for good by the data, no hand on it
                        no_handle.push(format!("{what} no tiene mando: lo fija \"senales\" a {} ({})", r.get(sig), if e.measure { "es un disyuntor que ni salta ni se puede sacar ni rearmar" } else { "fijo" }));
                    }
                    (Writer::None, None) => bad.push(format!("{what} no lo acciona nada (señal '{}' sin escritor): está {} para siempre", r.name(sig), if r.get(sig) >= 0.5 { "cerrado" } else { "abierto" })),
                    _ => {}
                }
            }
        }
        for c in &r.ship.panels.controls {
            let d = &kind.panels[c.panel].def.mandos[c.index];
            if let Some(u) = &d.bind.union
                && c.breaker.is_none()
            {
                bad.push(format!("{}: el mando {} dice ser la unión '{u}' y ninguna red la tiene", kind.id, c.id));
            }
            if c.mech.kind() == "disyuntor" && c.breaker.is_none() {
                bad.push(format!("{}: el disyuntor {} no es interruptor de ninguna red (le falta \"bind\": {{ \"union\" }}): ni corta ni salta", kind.id, c.id));
            }
        }
    }
    for n in &no_handle {
        eprintln!("  sin mando (declarado): {n}");
    }
    report("interruptores sin dueño o disyuntores que no lo son", &bad);
}

/// On the ship's real networks, every source given all it could want and every other port
/// asking: all are served; then each switch opened alone, in turn.
#[test]
fn opening_any_one_switch_cuts_exactly_what_is_behind_it() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let seen = seen(&mut r);
        let owners = r.owners();
        let by_role = r.source_roles(&owners);
        let mut solver = Solver::default();
        let (mut switches, mut cut) = (0, 0);
        for (n, plan) in kind.nets.iter().enumerate() {
            let mut net = r.ship.nets[n].clone();
            for e in &mut net.edges {
                (e.alive, e.closed, e.leak) = (true, true, 0.0);
            }
            // the ship's own ports, in its own order: those that give made ample regulated
            // sources, the rest plain loads
            let gives = |k: usize| seen.gives[k] || (plan.medium == Medium::Electrico && by_role[k]);
            let mut ports: Vec<PortIo> = r.ship.ports.iter().map(|p| PortIo::on(p.net, p.node)).collect();
            for (k, p) in ports.iter_mut().enumerate().filter(|(_, p)| usize::from(p.net) == n) {
                if gives(k) {
                    (p.produce, p.potential, p.regulated) = (1e9, plan.nominal.max(1.0), true);
                } else {
                    (p.demand, p.priority) = (1.0, 100);
                }
            }
            let sources: Vec<u32> = (0..ports.len()).filter(|&k| usize::from(ports[k].net) == n && gives(k)).map(|k| ports[k].node).collect();
            solver.solve(n as u16, &mut net, &mut ports);
            for (k, p) in ports.iter().enumerate().filter(|(k, p)| usize::from(p.net) == n && !gives(*k)) {
                if !p.fed || p.share < 1.0 {
                    bad.push(format!("{}: con todo cerrado y fuentes de sobra, {} no recibe (parte {:.2})", kind.id, r.port_name(k), p.share));
                }
            }
            for e in (0..plan.edges.len()).filter(|&e| plan.edges[e].kind == EdgeKind::Switch) {
                switches += 1;
                net.edges[e].closed = false;
                solver.solve(n as u16, &mut net, &mut ports);
                net.edges[e].closed = true;
                let reached = r.reach(n, &sources, &|k| k != e);
                for (k, p) in ports.iter().enumerate().filter(|(k, p)| usize::from(p.net) == n && !gives(*k)) {
                    let want = reached[p.node as usize];
                    cut += usize::from(!want);
                    if p.fed != want || (p.share >= 1.0) != want {
                        bad.push(format!("{}: abierto {} (red {}), {} {}", kind.id, plan.edges[e].id, plan.name, r.port_name(k), if want { "se queda sin servicio y no está detrás de él" } else { "sigue servido y está detrás de él" }));
                    }
                }
            }
        }
        eprintln!("{}: {switches} interruptores abiertos uno a uno, {cut} cortes de puerto comprobados", kind.id);
    }
    report("lo que corta cada interruptor", &bad);
}
