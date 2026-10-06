//! Too little for everybody, on every ship and in every network:
//! - with less than is asked, what there is goes by priority: nothing of a lower priority gets a
//!   drop while something of a higher one goes short, equals share alike, and what is served is
//!   exactly what there was (the solver, on each ship's own networks and what its machines ask,
//!   the sources cut down step by step);
//! - live, with its stores hurt until they cannot cover the ship: the same order, tick after tick
//!   the same loads on and the same off (no flicker), and with the stores mended every one of
//!   them back;
//! - breakers: what each senses is what its circuit takes; with everything a hand can switch on
//!   running none is near its rating; each trips on an overload (by its own curve, at once on a
//!   short), says so, stays out while hot and goes back in by hand once cool, its circuit with it.
mod comun;
use comun::*;
use lunar_controls::{Blocked, Event, Intent, intent::F_TRIPPED};
use lunar_machines::{EdgeKind, PortIo, Solver, net::Medium};
use lunar_ship::ship::TICK;

/// What is wrong with how one solved network shared what it had (see the module).
fn sharing(r: &Rig, n: usize, net: &lunar_machines::Net, ports: &[PortIo], when: &str, bad: &mut Vec<String>) {
    let name = &r.kind.nets[n].name;
    for i in 0..net.islands.len() as u32 {
        let here: Vec<usize> = (0..ports.len()).filter(|&k| usize::from(ports[k].net) == n && net.island[ports[k].node as usize] == i).collect();
        let asked: f64 = here.iter().map(|&k| ports[k].demand).sum();
        let had: f64 = here.iter().map(|&k| ports[k].produce + ports[k].store_out).sum();
        let got: f64 = here.iter().map(|&k| ports[k].got).sum();
        let leaked = net.islands[i as usize].leaked;
        if (got - asked.min((had - leaked).max(0.0))).abs() > 1e-9 * (asked + had) + 1e-12 {
            bad.push(format!("{}: red {name} {when}: se pedían {asked:.6}, había {had:.6} (fugas {leaked:.6}) y se sirvieron {got:.6}", r.id()));
        }
        for &a in &here {
            let pa = &ports[a];
            if pa.demand <= 0.0 {
                continue;
            }
            if !(0.0..=1.0 + 1e-12).contains(&pa.share) || (pa.got - pa.demand * pa.share).abs() > 1e-9 * pa.demand {
                bad.push(format!("{}: red {name} {when}: {} recibe {} de {} (parte {})", r.id(), r.port_name(a), pa.got, pa.demand, pa.share));
            }
            for &b in &here {
                let pb = &ports[b];
                if pb.demand <= 0.0 {
                    continue;
                }
                if pa.priority > pb.priority && pb.share > 1e-12 && pa.share < 1.0 - 1e-12 {
                    bad.push(format!("{}: red {name} {when}: {} (prioridad {}) recibe {:.3} mientras {} (prioridad {}) se queda en {:.3}", r.id(), r.port_name(b), pb.priority, pb.share, r.port_name(a), pa.priority, pa.share));
                } else if pa.priority == pb.priority && (pa.share - pb.share).abs() > 1e-12 {
                    bad.push(format!("{}: red {name} {when}: {} y {} tienen la misma prioridad ({}) y reciben {:.3} y {:.3}", r.id(), r.port_name(a), r.port_name(b), pa.priority, pa.share, pb.share));
                }
            }
        }
    }
}

#[test]
fn with_less_than_is_asked_every_network_serves_by_priority() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(2.0);
        r.switch_on(&|_, _| true);
        r.run(10.0);
        let mut solver = Solver::default();
        let (mut cases, mut short) = (0, 0);
        for n in 0..kind.nets.len() {
            // the ship's own network and what every port on it asks and offers now
            let mut net = r.ship.nets[n].clone();
            let asks: Vec<PortIo> = r.ship.ports.clone();
            solver.solve(n as u16, &mut net, &mut asks.clone());
            let islands = net.island.clone();
            for cut in [1.0, 0.9, 0.6, 0.3, 0.1, 0.02, 0.0] {
                // every island's sources cut down to `cut` of what its loads ask
                let mut ports = asks.clone();
                for i in 0..net.islands.len() as u32 {
                    let here = |p: &PortIo| usize::from(p.net) == n && islands[p.node as usize] == i;
                    let asked: f64 = ports.iter().filter(|p| here(p)).map(|p| p.demand).sum();
                    let had: f64 = ports.iter().filter(|p| here(p)).map(|p| p.produce + p.store_out).sum();
                    if asked <= 0.0 || had <= 0.0 {
                        continue;
                    }
                    for p in ports.iter_mut().filter(|p| here(p)) {
                        p.produce *= cut * asked / had;
                        p.store_out *= cut * asked / had;
                    }
                    cases += 1;
                    short += usize::from(cut < 1.0);
                }
                solver.solve(n as u16, &mut net, &mut ports);
                sharing(&r, n, &net, &ports, &format!("con el {:.0} % de lo pedido", cut * 100.0), &mut bad);
                // the same question, the same answer
                let mut again = ports.clone();
                solver.solve(n as u16, &mut net, &mut again);
                if again != ports {
                    bad.push(format!("{}: red {} resuelta dos veces con lo mismo da dos repartos", kind.id, kind.nets[n].name));
                }
                if bad.len() > 30 {
                    break;
                }
            }
        }
        // (for whoever reads the run: the order in which this ship's electrical loads go)
        let owners = r.owners();
        let mut order: Vec<(u8, String)> = r.nets_of(Medium::Electrico).into_iter().flat_map(|n| r.ports_on(n).filter(|(_, p)| p.demand > 0.0).map(|(k, p)| (p.priority, r.owner_name(&owners[k]))).collect::<Vec<_>>()).collect();
        order.sort();
        let mut steps: Vec<String> = Vec::new();
        for (prio, _) in &order {
            if !steps.iter().any(|s| s.starts_with(&format!("{prio}:"))) {
                steps.push(format!("{prio}: {} cargas", order.iter().filter(|o| o.0 == *prio).count()));
            }
        }
        eprintln!("{}: {cases} repartos ({short} con escasez); orden de corte eléctrico (prioridad: cuántas): {}", kind.id, steps.join(" · "));
    }
    report("reparto con escasez", &bad);
}

/// The parts of the machines that keep an electrical store (batteries): hurt, they give less.
fn store_parts(r: &Rig) -> Vec<(usize, usize)> {
    let owners = r.owners();
    let mut out = Vec::new();
    for n in r.nets_of(Medium::Electrico) {
        for (k, p) in r.ports_on(n) {
            if let (Owner::Machine(m, _), true) = (&owners[k], p.store_out > 0.0)
                && let Some(part) = r.kind.machines[*m].part
            {
                out.push((k, part as usize));
            }
        }
    }
    out
}

/// What every port that asks on the networks `nets` gets now: (port, served — its share at a
/// half or more).
fn served(r: &Rig, nets: &[usize]) -> Vec<(usize, bool)> {
    nets.iter().flat_map(|&n| r.ports_on(n).filter(|(_, p)| p.demand > 0.0).map(|(k, p)| (k, p.fed && p.share >= 0.5)).collect::<Vec<_>>()).collect()
}

#[test]
fn a_ship_short_of_power_sheds_by_priority_without_flicker_and_recovers() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(5.0);
        // the networks its batteries are on, and what is served on them as the ship stands
        let stores = store_parts(&r);
        let mut nets: Vec<usize> = stores.iter().map(|(k, _)| usize::from(r.ship.ports[*k].net)).collect();
        nets.sort_unstable();
        nets.dedup();
        let before = served(&r, &nets);
        let asked: f64 = before.iter().map(|(k, _)| r.ship.ports[*k].demand).sum();
        let had: f64 = stores.iter().map(|(k, _)| r.ship.ports[*k].store_out).sum();
        // its stores hurt until they give a third of what is asked (a store gives by its health;
        // no further than the wreck that still works)
        let health = (0.35 * asked / had.max(1.0)).clamp(0.05, 1.0);
        for &(_, part) in &stores {
            r.s.parts[part].hp = r.s.parts[part].max_hp * health as f32;
        }
        r.run(3.0);
        let now: f64 = stores.iter().map(|(k, _)| r.ship.ports[*k].store_out).sum();
        let (want, got): (f64, f64) = served(&r, &nets).iter().map(|(k, _)| (r.ship.ports[*k].demand, r.ship.ports[*k].got)).fold((0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
        if got >= want * 0.999 {
            eprintln!("{}: ni con las baterías al {:.0} % ({:.0} W) falta energía para lo que pide en reposo ({:.0} W): sin escasez que probar en vivo", kind.id, health * 100.0, now, want);
            continue;
        }
        // tick after tick: the order holds, and the same loads are on and the same off (a load
        // that changes once as the ship settles is not flicker; one that goes back and forth is)
        let mut flips = vec![0u32; r.ship.ports.len()];
        let mut last = served(&r, &nets);
        for _ in 0..500 {
            r.tick();
            for &n in &nets {
                if bad.len() < 20 {
                    sharing(&r, n, &r.ship.nets[n], &r.ship.ports, "en vivo, con las baterías dañadas", &mut bad);
                }
            }
            let s = served(&r, &nets);
            for (a, b) in s.iter().zip(&last) {
                if a.0 == b.0 && a.1 != b.1 {
                    flips[a.0] += 1;
                }
            }
            last = s;
        }
        for (k, n) in flips.iter().enumerate().filter(|(_, n)| **n >= 2) {
            bad.push(format!("{}: con poca energía {} se enciende y se apaga ({n} cambios en 10 s)", kind.id, r.port_name(k)));
        }
        let off: Vec<usize> = last.iter().filter(|x| !x.1).map(|x| x.0).collect();
        let (lo, hi) = (off.iter().map(|&k| r.ship.ports[k].priority).max().unwrap_or(0), last.iter().filter(|x| x.1).map(|x| r.ship.ports[x.0].priority).min().unwrap_or(255));
        eprintln!("{}: baterías al {:.0} %: {:.0} W para {:.0} W pedidos; {} cargas sin servir de {} (las de prioridad hasta {lo}; servidas las de {hi} para arriba)", kind.id, health * 100.0, got, want, off.len(), last.len());
        if off.is_empty() {
            bad.push(format!("{}: falta energía ({got:.0} W de {want:.0} W) y ninguna carga se queda sin servir", kind.id));
        }
        if lo > hi {
            bad.push(format!("{}: con poca energía queda sin servir una carga de prioridad {lo} mientras se sirve otra de prioridad {hi}", kind.id));
        }
        // mended: every load that was served is served again
        for &(_, part) in &stores {
            r.mend(part);
        }
        let all_back = |r: &Rig| {
            let s = served(r, &nets);
            before.iter().filter(|b| b.1).all(|b| s.iter().any(|x| x.0 == b.0 && x.1))
        };
        if !r.until(600.0, all_back) {
            let s = served(&r, &nets);
            bad.push(format!("{}: reparadas las baterías siguen sin servir {}", kind.id, before.iter().filter(|b| b.1 && !s.iter().any(|x| x.0 == b.0 && x.1)).map(|x| r.port_name(x.0)).collect::<Vec<_>>().join(", ")));
        }
    }
    report("escasez en vivo", &bad);
}

/// The breakers of a ship: (control, network, edge, rating in amps).
fn breakers(r: &Rig) -> Vec<(usize, usize, usize, f64)> {
    r.ship.panels.controls.iter().enumerate().filter(|(_, c)| c.mech.kind() == "disyuntor").filter_map(|(k, c)| c.breaker.map(|(n, e, rating)| (k, usize::from(n), e, rating))).collect()
}

/// What a breaker senses now: the amps through it, as its panel works them out.
fn amps(r: &Rig, n: usize, e: usize) -> f64 {
    let net = &r.ship.nets[n];
    let edge = &net.edges[e];
    let v = net.island.get(edge.a as usize).map_or(0.0, |&i| net.islands.get(i as usize).map_or(0.0, |x| x.potential));
    if v > 1.0 { edge.flow / v } else { 0.0 }
}

#[test]
fn what_a_breaker_senses_is_what_its_circuit_takes_and_none_is_near_its_rating() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let all = breakers(&r);
        if all.is_empty() {
            eprintln!("{}: sin disyuntores", kind.id);
            continue;
        }
        let owners = r.owners();
        let by_role = r.source_roles(&owners);
        // the ports behind each: those its opening alone would leave without any source
        let behind: Vec<Vec<usize>> = all
            .iter()
            .map(|&(_, n, e, _)| {
                let sources: Vec<u32> = r.ports_on(n).filter(|(k, _)| by_role[*k]).map(|(_, p)| p.node).collect();
                let reached = r.reach(n, &sources, &|x| x != e);
                r.ports_on(n).filter(|(_, p)| !reached[p.node as usize]).map(|(k, _)| k).collect()
            })
            .collect();
        let mut peak = vec![0.0f64; all.len()];
        let mut on = Vec::new();
        for tick in 0..4500 {
            if tick == 500 {
                on = r.switch_on(&|_, _| true);
            }
            r.tick();
            for (i, &(k, n, e, rating)) in all.iter().enumerate() {
                let edge = &r.ship.nets[n].edges[e];
                let taken: f64 = behind[i].iter().map(|&p| r.ship.ports[p].got + r.ship.ports[p].stored.max(0.0) - r.ship.ports[p].gave + r.ship.ports[p].stored.min(0.0)).sum();
                if edge.conducts() && (edge.flow - taken.max(0.0)).abs() > 1e-6 * taken.abs() + 1e-6 && bad.len() < 20 {
                    bad.push(format!("{} t {:.2}: el disyuntor {} mide {:.3} W y lo que cuelga de él toma {:.3} W", kind.id, r.ship.t, r.ship.panels.controls[k].id, edge.flow, taken));
                }
                peak[i] = peak[i].max(amps(&r, n, e) / rating);
                if r.ship.panels.controls[k].st.has(F_TRIPPED) && bad.len() < 20 {
                    bad.push(format!("{} t {:.2}: el disyuntor {} ({rating} A) salta con la nave sana y todo en marcha: mal dimensionado para lo que cuelga de él", kind.id, r.ship.t, r.ship.panels.controls[k].id));
                }
            }
        }
        // ---- and the worst a crew could do: every control of the ship worked at random for a
        // while, the most each load on a circuit ever asks noted; all of them at once is what its
        // breaker must carry (an electrical load analysis, by what the loads do rather than by a
        // table of them)
        let mut most = vec![0.0f64; r.ship.ports.len()];
        let mut rng = Rng(0x10ad);
        let n = r.ship.panels.controls.len();
        for tick in 0..6000 {
            if tick % 8 == 0 {
                let k = rng.below(n);
                match rng.below(3) {
                    0 => {
                        r.uncover(k);
                        r.intent(k, &Intent::Press { elem: 0 });
                        r.intent(k, &Intent::Release);
                    }
                    1 => drop(r.set(k, [0.0, 1.0, 2.0][rng.below(3)])),
                    _ => drop(r.set(k, rng.unit())),
                }
            }
            r.tick();
            for (k, p) in r.ship.ports.iter().enumerate() {
                most[k] = most[k].max(p.demand);
            }
        }
        let mut worst: Vec<(f64, String)> = Vec::new();
        for (i, &(k, n, _, rating)) in all.iter().enumerate() {
            let watts: f64 = behind[i].iter().map(|&p| most[p]).sum();
            let share = watts / (rating * kind.nets[n].nominal.max(1.0));
            let id = r.ship.panels.controls[k].id.clone();
            worst.push((share, format!("{} {:.0} %", id.rsplit('/').next().unwrap_or(""), share * 100.0)));
            if share > 1.0 {
                let heavy: Vec<String> = behind[i].iter().filter(|&&p| most[p] > 0.0).map(|&p| format!("{} {:.0} W", r.owner_name(&owners[p]), most[p])).collect();
                bad.push(format!("{}: el disyuntor {id} ({rating} A a {} V = {:.0} W) no aguanta todo lo de su circuito a la vez: {watts:.0} W ({})", kind.id, kind.nets[n].nominal, rating * kind.nets[n].nominal, heavy.join(", ")));
            }
        }
        // (a measured switch with no breaker on a panel protects nothing: what its circuit can
        // ask, for whoever gives it one)
        for (n, plan) in kind.nets.iter().enumerate() {
            for (e, edge) in plan.edges.iter().enumerate().filter(|(e, x)| x.kind == EdgeKind::Switch && x.measure && !all.iter().any(|b| b.1 == n && b.2 == *e)) {
                let sources: Vec<u32> = r.ports_on(n).filter(|(k, _)| by_role[*k]).map(|(_, p)| p.node).collect();
                let reached = r.reach(n, &sources, &|x| x != e);
                let watts: f64 = r.ports_on(n).filter(|(_, p)| !reached[p.node as usize]).map(|(k, _)| most[k]).sum();
                eprintln!("  {}: {} mide su corriente y no tiene disyuntor en ningún panel: su circuito llega a pedir {watts:.0} W ({:.0} A a {} V)", kind.id, edge.id, watts / plan.nominal.max(1.0), plan.nominal);
            }
        }
        worst.sort_by(|a, b| b.0.total_cmp(&a.0));
        eprintln!("{}: peor caso por circuito (todo lo que cuelga de él pidiendo a la vez lo más que llega a pedir): {}", kind.id, worst.iter().map(|t| t.1.as_str()).collect::<Vec<_>>().join(", "));
        let mut table: Vec<(f64, String)> = all.iter().zip(&peak).map(|(b, p)| (*p, format!("{} {:.0} %", r.ship.panels.controls[b.0].id.rsplit('/').next().unwrap_or(""), p * 100.0))).collect();
        table.sort_by(|a, b| b.0.total_cmp(&a.0));
        eprintln!("{}: {} disyuntores, con todo en marcha ({} cosas encendidas); pico de corriente sobre su nominal: {}", kind.id, all.len(), on.len(), table.iter().map(|t| t.1.as_str()).collect::<Vec<_>>().join(", "));
        for (i, &(k, ..)) in all.iter().enumerate() {
            if peak[i] > 1.0 {
                bad.push(format!("{}: el disyuntor {} llega al {:.0} % de su nominal con la nave sana: acabaría saltando", kind.id, r.ship.panels.controls[k].id, peak[i] * 100.0));
            }
        }
    }
    report("disyuntores: medida y dimensionado", &bad);
}

#[test]
fn every_breaker_trips_on_an_overload_says_so_and_is_reset_by_hand_once_cool() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(3.0);
        let all = breakers(&r);
        let owners = r.owners();
        let by_role = r.source_roles(&owners);
        for &(k, n, e, rating) in &all {
            let id = r.ship.panels.controls[k].id.clone();
            let what = format!("{}: disyuntor {id} ({rating} A)", kind.id);
            // ---- its mechanism, by its own curve: at its rating it holds; over it, it goes
            let mut st = r.ship.panels.controls[k].st;
            let mech = &r.ship.panels.controls[k].mech;
            if (0..36_000).any(|_| mech.advance(&mut st, 0.05, 1.0).changed) {
                bad.push(format!("{what}: salta a su corriente nominal"));
            }
            let mut st = r.ship.panels.controls[k].st;
            if !(0..36_000).any(|_| mech.advance(&mut st, 0.05, 1.5).changed) {
                bad.push(format!("{what}: no salta ni con media hora al 150 % de su nominal"));
            }
            let mut st = r.ship.panels.controls[k].st;
            if mech.advance(&mut st, 0.05, 20.0).event != Some(Event::Trip) {
                bad.push(format!("{what}: no salta al instante con un cortocircuito (20 veces su nominal)"));
            }
            // ---- on the ship: three times its rating through its edge, as its panel would see it
            let sources: Vec<u32> = r.ports_on(n).filter(|(p, _)| by_role[*p]).map(|(_, p)| p.node).collect();
            let reached = r.reach(n, &sources, &|x| x != e);
            let behind: Vec<usize> = r.ports_on(n).filter(|(_, p)| p.fed && !reached[p.node as usize]).map(|(p, _)| p).collect();
            let logged = r.ship.blackbox.entries.len();
            let mut nets = r.ship.nets.clone();
            let v = amps_volts(&r, n, e);
            nets[n].edges[e].flow = 3.0 * rating * v;
            let kind_ = r.kind.clone();
            let mut tripped = false;
            for step in 0..6000 {
                r.ship.panels.after(&kind_, &r.s, &mut r.ship.store, &r.ship.ports, &nets, TICK, r.ship.t + f64::from(step) * TICK, &mut r.ship.blackbox);
                if r.ship.panels.controls[k].st.has(F_TRIPPED) {
                    tripped = true;
                    break;
                }
            }
            if !tripped || r.value(k) >= 0.5 {
                bad.push(format!("{what}: con el triple de su nominal por él no salta en la nave"));
                continue;
            }
            if !r.ship.blackbox.entries.iter().skip(logged).any(|x| x.text.contains(&id)) {
                bad.push(format!("{what}: salta y la caja negra no lo dice"));
            }
            r.ticks(3);
            if r.ship.nets[n].edges[e].closed {
                bad.push(format!("{what}: saltado, su circuito sigue cerrado"));
            }
            for &p in &behind {
                if r.ship.ports[p].fed {
                    bad.push(format!("{what}: saltado, {} sigue con alimentación", r.port_name(p)));
                }
            }
            // hot, a hand cannot push it back
            let o = r.intent(k, &Intent::Press { elem: 0 });
            r.intent(k, &Intent::Release);
            if o.event != Some(Event::Blocked(Blocked::Hot)) || r.value(k) >= 0.5 {
                bad.push(format!("{what}: recién saltado se deja rearmar ({:?})", o.event));
            }
            // cool, it goes back in and its circuit comes back
            let reset = r.until(600.0, |r| !r.ship.panels.controls[k].st.has(F_TRIPPED) || r.ship.panels.controls[k].st.t <= 0.3) && {
                r.click(k);
                r.ticks(3);
                r.value(k) >= 0.5 && !r.ship.panels.controls[k].st.has(F_TRIPPED)
            };
            if !reset {
                bad.push(format!("{what}: frío no se deja rearmar"));
                continue;
            }
            for &p in &behind {
                if !r.ship.ports[p].fed {
                    bad.push(format!("{what}: rearmado, {} no recupera la alimentación", r.port_name(p)));
                }
            }
        }
        eprintln!("{}: {} disyuntores saltados por sobrecarga y rearmados", kind.id, all.len());
        // and nothing else of the network's switches trips: a contactor, a valve, do not
        for plan in &kind.nets {
            for e in plan.edges.iter().filter(|e| e.kind == EdgeKind::Switch && e.measure) {
                let handled = e.signal.as_ref().and_then(|s| r.find(s)).and_then(|s| r.control_of(s)).is_some_and(|k| r.ctl_kind(k) == "disyuntor");
                if !handled {
                    eprintln!("  {}: {} mide su corriente y no tiene disyuntor que salte por ella", kind.id, e.id);
                }
            }
        }
    }
    report("disyuntores: disparo y rearme", &bad);
}

/// The volts on the bus side of a breaker.
fn amps_volts(r: &Rig, n: usize, e: usize) -> f64 {
    let net = &r.ship.nets[n];
    net.island.get(net.edges[e].a as usize).map_or(0.0, |&i| net.islands.get(i as usize).map_or(0.0, |x| x.potential)).max(1.0)
}
