//! Sources, stores and what flows between them, on every ship, in every medium its networks
//! carry (power, propellant, breathing gas, hydraulic fluid, coolant, heat, air, data):
//! - every tick, in every network, what the producers gave less what went into the stores is what
//!   the consumers got and what leaked: nothing is made or lost in a network;
//! - the ship's electrical balance (`energia.*`, `power.rs`) is that same account: what is made
//!   and what comes out of the batteries is what is spent, worked out here a second way (owner by
//!   owner) with the reactor up and its converters carrying power; a converter between two
//!   electrical networks never feeds back, gives no more than it takes, and what passes through
//!   it is counted neither as made nor as spent;
//! - batteries: they give on a deficit and take on a surplus, what they say (`.v`, `.i`, `.soc`)
//!   is what their terminals did, a charge counts for as many coulombs as a discharge, and they
//!   never leave 0..1 whether drained flat or filled to the brim;
//! - what is kept by weight (propellant, bottled gas): what the tanks and bottles lose is what
//!   their consumers got; the gas aboard (the air of every compartment, the tanks of recovered
//!   air, the bottles) changes only by what goes out to space.
//! No ship, machine or signal of one is named: stores are found by what their ports do, what they
//! hold by the `<machine>.masa` every store by weight publishes.
mod comun;
use comun::*;
use lunar_machines::net::Medium;
use lunar_ship::ship::TICK;
use std::collections::BTreeMap;

/// At rest, then with every machine a hand can switch on set going: every network in balance at
/// every tick.
#[test]
fn nothing_is_made_or_lost_in_any_network() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let mut ticks = 0;
        for phase in 0..2 {
            if phase == 1 {
                r.switch_on(&|_, _| true);
            }
            for _ in 0..(if phase == 0 { 250 } else { 2000 }) {
                r.tick();
                ticks += 1;
                if bad.len() < 20 {
                    bad.extend(r.out_of_balance());
                    bad.extend(r.broken());
                }
            }
        }
        eprintln!("{}: {} redes en equilibrio durante {ticks} tics", kind.id, kind.nets.len());
    }
    report("redes que crean o pierden", &bad);
}

/// The machines with ports on more than one electrical network (a converter between buses):
/// (machine, the ports that take, the ports that give).
fn links(r: &Rig) -> Vec<(usize, Vec<usize>, Vec<usize>)> {
    let mut out = Vec::new();
    for (m, rt) in r.ship.machines.iter().enumerate() {
        let electric: Vec<(usize, &str)> = rt.m.ports().iter().zip(rt.ports.clone()).filter(|(s, k)| s.medium == Medium::Electrico && r.ship.ports[*k].net != u16::MAX).map(|(s, k)| (k, s.role)).collect();
        let mut nets: Vec<u16> = electric.iter().map(|(k, _)| r.ship.ports[*k].net).collect();
        nets.sort_unstable();
        nets.dedup();
        if nets.len() > 1 {
            out.push((m, electric.iter().filter(|(_, role)| !ELECTRIC_SOURCES.contains(role)).map(|x| x.0).collect(), electric.iter().filter(|(_, role)| ELECTRIC_SOURCES.contains(role)).map(|x| x.0).collect()));
        }
    }
    out
}

/// The ship's electrical balance worked out owner by owner: what each machine, actuator and
/// panel gives net of what it takes on its electrical ports is made if it gives more than it
/// takes, spent otherwise (so what a converter passes on is neither); the stores apart.
fn balance(r: &Rig, owners: &[Owner]) -> (f64, f64, f64) {
    let mut net: BTreeMap<(u8, usize), f64> = BTreeMap::new();
    let mut stored = 0.0;
    for n in r.nets_of(Medium::Electrico) {
        for (k, p) in r.ports_on(n) {
            let who = match &owners[k] {
                Owner::Machine(m, _) => (0, *m),
                Owner::Actuator(a, _) => (1, *a),
                Owner::PanelPower(p) | Owner::PanelData(p) => (2, *p),
            };
            *net.entry(who).or_default() += p.gave - p.got;
            stored += p.stored;
        }
    }
    (net.values().filter(|v| **v > 0.0).sum(), -net.values().filter(|v| **v < 0.0).sum::<f64>(), -stored)
}

#[test]
fn the_electrical_balance_adds_up_and_converters_neither_make_nor_spend_what_they_pass() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let owners = r.owners();
        let links = links(&r);
        let on = r.switch_on(&|_, _| true);
        // (heat into power, where the ship has such a machine: what it takes hot, and what it
        // gives as power and as heat thrown to its cold side)
        let thermal: Vec<(usize, Vec<usize>, Vec<usize>)> = r
            .ship
            .machines
            .iter()
            .enumerate()
            .filter(|(_, rt)| rt.m.ports().iter().any(|s| s.medium == Medium::Termico) && rt.m.ports().iter().any(|s| s.medium == Medium::Electrico && ELECTRIC_SOURCES.contains(&s.role)))
            .map(|(m, rt)| (m, rt.m.ports().iter().zip(rt.ports.clone()).filter(|(s, _)| matches!(s.medium, Medium::Termico | Medium::Electrico)).map(|(_, k)| k).collect::<Vec<_>>(), Vec::new()))
            .collect();
        let (mut through, mut taken, mut given) = (vec![0.0; links.len()], vec![0.0; links.len()], vec![0.0; links.len()]);
        let (mut heat_in, mut heat_out) = (vec![0.0; thermal.len()], vec![0.0; thermal.len()]);
        let mut carried = f64::MAX;
        let mut ticks = 0u32;
        // until every converter carries power, and half a minute more (a ship with none: a minute)
        let limit = (1800.0 / TICK) as u32;
        while ticks < limit && bad.len() < 20 {
            r.tick();
            ticks += 1;
            let (made, spent, from) = balance(&r, &owners);
            let (g, c, b) = (r.sig("energia.generacion"), r.sig("energia.consumo"), r.sig("energia.baterias"));
            let tol = 1e-6 * (made + spent + from.abs()) + 1e-6;
            if (g + b - c).abs() > tol {
                bad.push(format!("{} t {:.2}: energia.generacion {g:.3} + energia.baterias {b:.3} ≠ energia.consumo {c:.3}", kind.id, r.ship.t));
            }
            if (g - made).abs() > tol || (c - spent).abs() > tol || (b - from).abs() > tol {
                bad.push(format!("{} t {:.2}: energia.* dice genera {g:.3}, consume {c:.3}, baterías {b:.3}; puerto a puerto son {made:.3}, {spent:.3}, {from:.3}", kind.id, r.ship.t));
            }
            if (r.sig("energia.balance") - (g - c)).abs() > tol {
                bad.push(format!("{} t {:.2}: energia.balance no es generación menos consumo", kind.id, r.ship.t));
            }
            bad.extend(r.out_of_balance());
            for (i, (m, ins, outs)) in links.iter().enumerate() {
                let (inn, out): (f64, f64) = (ins.iter().map(|&k| r.ship.ports[k].got).sum(), outs.iter().map(|&k| r.ship.ports[k].gave).sum());
                taken[i] += inn * TICK;
                given[i] += out * TICK;
                through[i] = out;
                // no way back: its input never gives, its output never takes
                if ins.iter().any(|&k| r.ship.ports[k].gave > 0.0 || r.ship.ports[k].produce > 0.0 || r.ship.ports[k].stored != 0.0) || outs.iter().any(|&k| r.ship.ports[k].got > 0.0 || r.ship.ports[k].demand > 0.0) {
                    bad.push(format!("{} t {:.2}: el convertidor {} da por su entrada o toma por su salida", kind.id, r.ship.t, kind.machines[*m].id));
                }
                // (it offers what it took the tick before: a tick's worth of slack)
                if given[i] > taken[i] + 1e-9 {
                    bad.push(format!("{} t {:.2}: el convertidor {} ha dado {:.3} J y solo ha tomado {:.3} J", kind.id, r.ship.t, kind.machines[*m].id, given[i], taken[i]));
                }
            }
            for (i, (_, ports, _)) in thermal.iter().enumerate() {
                for &k in ports {
                    let p = &r.ship.ports[k];
                    heat_in[i] += p.got * TICK;
                    heat_out[i] += p.gave * TICK;
                }
            }
            if !links.is_empty() && through.iter().all(|w| *w > 200.0) && carried == f64::MAX {
                carried = r.ship.t;
            }
            if (links.is_empty() && r.ship.t > 60.0) || r.ship.t > carried + 30.0 {
                break;
            }
        }
        if !links.is_empty() && carried == f64::MAX {
            bad.push(format!("{}: con todo en marcha ({}) sus convertidores entre barras no llegan a llevar corriente en {:.0} s: {:?} W", kind.id, on.join(", "), r.ship.t, through));
        }
        for (i, (m, ..)) in links.iter().enumerate() {
            eprintln!("{}: convertidor {}: tomó {:.0} kJ, dio {:.0} kJ ({:.0} %)", kind.id, kind.machines[*m].id, taken[i] / 1e3, given[i] / 1e3, 100.0 * given[i] / taken[i].max(1e-9));
        }
        for (i, (m, ..)) in thermal.iter().enumerate() {
            // (what it takes it gives back as power and as heat: never more)
            eprintln!("{}: {}: tomó {:.0} kJ de calor (y de arranque), dio {:.0} kJ entre corriente y calor cedido ({:.0} %)", kind.id, kind.machines[*m].id, heat_in[i] / 1e3, heat_out[i] / 1e3, 100.0 * heat_out[i] / heat_in[i].max(1e-9));
            if heat_out[i] > heat_in[i] * (1.0 + 1e-9) + 1e-6 {
                bad.push(format!("{}: {} da más de lo que toma: {:.3} J de {:.3} J", kind.id, kind.machines[*m].id, heat_out[i], heat_in[i]));
            }
        }
        eprintln!("{}: {ticks} tics, {} convertidores entre barras{}", kind.id, links.len(), if carried < f64::MAX { format!(", con corriente desde los {carried:.0} s") } else { String::new() });
    }
    report("balance eléctrico", &bad);
}

/// The batteries of a ship: the machine, its port, and its signals of charge, volts and amps.
fn batteries(r: &Rig) -> Vec<(usize, usize, [lunar_signals::SignalId; 3])> {
    r.kind
        .machines
        .iter()
        .enumerate()
        .filter(|(_, m)| m.def.modelo == "bateria")
        .filter_map(|(m, plan)| {
            let sig = |f: &str| r.find(&format!("{}.{f}", plan.id));
            Some((m, r.ship.machines[m].ports.start, [sig("soc")?, sig("v")?, sig("i")?]))
        })
        .collect()
}

/// A battery's charge put where a test wants it (its state is its charge first, as it saves it).
fn set_charge(r: &mut Rig, m: usize, soc: f64) {
    let mut st = Vec::new();
    r.ship.machines[m].m.save(&mut st);
    st[0] = soc;
    r.ship.machines[m].m.load(&st);
}

#[test]
fn batteries_give_on_a_deficit_take_on_a_surplus_and_never_leave_their_bounds() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let bats = batteries(&r);
        if bats.is_empty() {
            eprintln!("{}: sin baterías", kind.id);
            continue;
        }
        r.run(1.0);
        // what a battery says is what its terminals do, tick after tick; and its charge moves the
        // way the current goes
        let check = |r: &Rig, bad: &mut Vec<String>| {
            for &(m, port, [soc, v, i]) in &bats {
                let (p, soc, v, i) = (&r.ship.ports[port], r.get(soc), r.get(v), r.get(i));
                if (v * i + p.stored).abs() > 1e-6 * p.stored.abs() + 1e-6 {
                    bad.push(format!("{} t {:.2}: {} dice {v:.2} V × {i:.2} A = {:.1} W y por sus bornes salen {:.1} W", kind.id, r.ship.t, kind.machines[m].id, v * i, -p.stored));
                }
                if !(0.0..=1.0).contains(&soc) {
                    bad.push(format!("{} t {:.2}: {}.soc = {soc}", kind.id, r.ship.t, kind.machines[m].id));
                }
            }
        };
        // ---- on a deficit (nothing generating): they give, and go down
        let (soc0, mut amps) = (bats.iter().map(|b| r.get(b.2[0])).collect::<Vec<_>>(), vec![0.0; bats.len()]);
        for _ in 0..500 {
            r.tick();
            check(&r, &mut bad);
            for (k, b) in bats.iter().enumerate() {
                amps[k] += r.get(b.2[2]) * TICK;
                if r.ship.ports[b.1].stored > 1e-9 {
                    bad.push(format!("{} t {:.2}: {} se carga y nada genera", kind.id, r.ship.t, kind.machines[b.0].id));
                }
            }
        }
        let mut coulombs = Vec::new();
        for (k, b) in bats.iter().enumerate() {
            let fell = soc0[k] - r.get(b.2[0]);
            if amps[k] > 1.0 && fell <= 0.0 {
                bad.push(format!("{}: {} ha dado {:.0} A·s y su carga no baja", kind.id, kind.machines[b.0].id, amps[k]));
            }
            coulombs.push(if fell > 0.0 { amps[k] / fell } else { 0.0 });
        }
        // ---- on a surplus: every generator a hand can start, started; they take, and go up
        let started = r.switch_on(&|r, m| r.is_source(m));
        let charging = r.until(900.0, |r| bats.iter().any(|b| r.ship.ports[b.1].stored > 1.0));
        if started.is_empty() {
            eprintln!("{}: ningún generador que arrancar desde un panel: sus baterías solo se descargan", kind.id);
        } else if !charging {
            bad.push(format!("{}: con {} en marcha las baterías no llegan a cargarse", kind.id, started.join(", ")));
        } else {
            let (soc1, mut amps) = (bats.iter().map(|b| r.get(b.2[0])).collect::<Vec<_>>(), vec![0.0; bats.len()]);
            for _ in 0..1000 {
                r.tick();
                check(&r, &mut bad);
                for (k, b) in bats.iter().enumerate() {
                    amps[k] -= r.get(b.2[2]) * TICK;
                }
            }
            for (k, b) in bats.iter().enumerate() {
                let rose = r.get(b.2[0]) - soc1[k];
                if amps[k] > 1.0 && rose <= 0.0 {
                    bad.push(format!("{}: {} ha tomado {:.0} A·s y su carga no sube", kind.id, kind.machines[b.0].id, amps[k]));
                }
                // a coulomb in is a coulomb out, less what the model says charging loses (a few
                // %) and times what the data declares it counts for (`ganancia_carga`: charging
                // made quicker for play, docs/TIEMPOS.md): nothing beyond what is declared
                if rose > 0.0 && coulombs[k] > 0.0 {
                    let gain = kind.machines[b.0].def.params.get("ganancia_carga").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    let ratio = (amps[k] / rose) * gain / coulombs[k];
                    eprintln!("{}: {}: {:.0} A·s por unidad de carga al descargar, {:.0} al cargar (ganancia de carga declarada: {gain})", kind.id, kind.machines[b.0].id, coulombs[k], amps[k] / rose);
                    if !(0.98..=1.06).contains(&ratio) {
                        bad.push(format!("{}: {} al cargar cuenta cada culombio {:.3} veces lo que declara su ganancia de carga ({gain})", kind.id, kind.machines[b.0].id, 1.0 / ratio));
                    }
                }
            }
            // ---- filled to the brim with the surplus still there: it stops at full
            for b in &bats {
                set_charge(&mut r, b.0, 0.9995);
            }
            for _ in 0..1500 {
                r.tick();
                check(&r, &mut bad);
            }
            for b in &bats {
                if r.ship.ports[b.1].stored > 1e-9 && r.get(b.2[0]) >= 0.999 {
                    bad.push(format!("{}: {} llena ({}) sigue tomando {:.0} W", kind.id, kind.machines[b.0].id, r.get(b.2[0]), r.ship.ports[b.1].stored));
                }
            }
        }
        // ---- drained flat with nothing generating: it stops at empty, and gives nothing
        let mut r = fleet().rig(kind);
        let bats = batteries(&r);
        for b in &bats {
            set_charge(&mut r, b.0, 0.004);
        }
        for _ in 0..3000 {
            r.tick();
            for &(m, port, [soc, ..]) in &bats {
                let (p, soc) = (&r.ship.ports[port], r.get(soc));
                if !(0.0..=1.0).contains(&soc) || !soc.is_finite() {
                    bad.push(format!("{} t {:.2}: {}.soc = {soc} al agotarse", kind.id, r.ship.t, kind.machines[m].id));
                }
                if soc <= 0.0 && p.stored < -1e-9 {
                    bad.push(format!("{} t {:.2}: {} vacía sigue dando {:.0} W", kind.id, r.ship.t, kind.machines[m].id, -p.stored));
                }
            }
            if bad.len() > 20 {
                break;
            }
        }
        bad.extend(r.broken());
        eprintln!("{}: {} baterías; agotadas quedan en {:?}", kind.id, bats.len(), bats.iter().map(|b| r.get(b.2[0])).collect::<Vec<_>>());
    }
    report("baterías", &bad);
}

/// The stores of the networks that carry something by weight (propellant, bottled gas): the
/// machine of each port that keeps a store there, by what its port does over a stretch.
fn stores_by_weight(r: &mut Rig, bad: &mut Vec<String>) -> Vec<(usize, Vec<lunar_signals::SignalId>)> {
    r.run(1.0);
    let owners = r.owners();
    let mut out = Vec::new();
    for (n, plan) in r.kind.nets.iter().enumerate().filter(|(_, p)| matches!(p.medium, Medium::Propelente | Medium::Gas)) {
        let mut masses = Vec::new();
        for (k, p) in r.ports_on(n) {
            if p.store_out > 0.0 || p.store_in > 0.0 {
                let Owner::Machine(m, _) = &owners[k] else { continue };
                match r.find(&format!("{}.masa", r.kind.machines[*m].id)) {
                    Some(s) => masses.push(s),
                    None => bad.push(format!("{}: {} guarda en la red {} y no publica cuánto lleva ({}.masa): no se puede comprobar que no se pierde", r.id(), r.kind.machines[*m].id, plan.name, r.kind.machines[*m].id)),
                }
            }
        }
        out.push((n, masses));
    }
    out
}

#[test]
fn what_is_kept_by_weight_leaves_its_tanks_only_into_what_takes_it() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let stores = stores_by_weight(&mut r, &mut bad);
        if stores.is_empty() {
            continue;
        }
        // something to burn it: every generator started; and the air of every pressurised room
        // thinned a little, so that what makes it up from the bottles has work to do
        r.switch_on(&|r, m| r.is_source(m));
        for a in &mut r.ship.atmos.air {
            a.n2 *= 0.9;
            a.o2 *= 0.9;
        }
        let held = |r: &Rig, masses: &[lunar_signals::SignalId]| masses.iter().map(|s| r.get(*s)).sum::<f64>();
        let start: Vec<f64> = stores.iter().map(|(_, m)| held(&r, m)).collect();
        let mut used = vec![0.0; stores.len()];
        for tick in 0..6000 {
            r.tick();
            for (i, (n, masses)) in stores.iter().enumerate() {
                // (the stores' own ports ask nothing: what is got on the network, its consumers got)
                used[i] += (r.ports_on(*n).map(|(_, p)| p.got).sum::<f64>() + r.ship.nets[*n].islands.iter().map(|x| x.leaked).sum::<f64>()) * TICK;
                let now = held(&r, masses);
                if tick % 50 == 49 && (start[i] - now - used[i]).abs() > 1e-6 * start[i] + 1e-9 && bad.len() < 20 {
                    bad.push(format!("{} t {:.1}: red {}: sus depósitos han bajado {:.6} kg y sus consumidores han tomado {:.6} kg", kind.id, r.ship.t, kind.nets[*n].name, start[i] - now, used[i]));
                }
            }
        }
        for (i, (n, masses)) in stores.iter().enumerate() {
            eprintln!("{}: red {}: {} depósitos, {:.3} kg gastados de {:.1} kg", kind.id, kind.nets[*n].name, masses.len(), used[i], start[i]);
        }
    }
    report("lo que se guarda al peso", &bad);
}

/// All the gas aboard (kg): the air of every compartment, what the tanks of recovered air hold,
/// what is left in the bottles.
fn gas_aboard(r: &mut Rig, bottles: &[lunar_signals::SignalId]) -> f64 {
    let rooms: f64 = r.ship.atmos.air.iter().map(|a| a.mass()).sum();
    let tanks = r.ship.atmos.transfers.held(&mut r.ship.machines).mass();
    rooms + tanks + bottles.iter().map(|s| r.get(*s)).sum::<f64>()
}

#[test]
fn the_gas_aboard_changes_only_by_what_goes_out_to_space() {
    let mut bad = Vec::new();
    for kind in fleet().kinds.iter().filter(|k| !k.compartments.is_empty()) {
        let mut r = fleet().rig(kind);
        let mut none = Vec::new();
        let bottles: Vec<lunar_signals::SignalId> = stores_by_weight(&mut r, &mut none).into_iter().filter(|(n, _)| kind.nets[*n].medium == Medium::Gas).flat_map(|(_, m)| m).collect();
        // (a machine that makes gas out of something that is not gas says how much: an electrolyser)
        let makers: Vec<lunar_signals::SignalId> = kind.machines.iter().filter(|m| m.def.modelo == "generador_o2").filter_map(|m| r.find(&format!("{}.produccion", m.id))).collect();
        // rooms thinned (what makes them up has work to do), and one vented a while by its own valve
        for a in &mut r.ship.atmos.air {
            a.n2 *= 0.85;
            a.o2 *= 0.85;
        }
        let vent = kind.compartments.iter().enumerate().filter(|(c, _)| r.ship.atmos.pressure(*c) > 30e3).find_map(|(_, plan)| {
            plan.vents.iter().filter(|v| v.to == lunar_ship::kind::SPACE).find_map(|v| {
                let sig = r.find(&v.signal)?;
                r.control_of(sig).or_else(|| lunar_ship::diag::reads(kind.def.derivadas.get(&v.signal)?).iter().find_map(|name| r.control_of(r.find(name)?)))
            })
        });
        let start = gas_aboard(&mut r, &bottles);
        let (mut out, mut made, mut worst) = (0.0, 0.0, 0.0f64);
        for tick in 0..5000 {
            if let (Some(k), true) = (vent, tick == 1500 || tick == 1750) {
                r.uncover(k);
                r.set(k, if tick == 1500 { 1.0 } else { 0.0 });
            }
            r.tick();
            out += r.ship.atmos.out.iter().sum::<f64>() * TICK;
            made += makers.iter().map(|s| r.get(*s)).sum::<f64>() * lunar_machines::gas::M_O2 * TICK;
            if tick % 25 == 24 {
                let now = gas_aboard(&mut r, &bottles);
                let off = now + out - made - start;
                worst = worst.max(off.abs());
                if off.abs() > 1e-6 * start && bad.len() < 10 {
                    bad.push(format!("{} t {:.1}: había {start:.4} kg de gas a bordo; quedan {now:.4}, han salido al vacío {out:.4} y se han fabricado {made:.4}: sobran {off:.6} kg", kind.id, r.ship.t));
                }
            }
        }
        if vent.is_some() && out <= 0.0 {
            bad.push(format!("{}: abierta la válvula de venteo {} no sale gas al vacío", kind.id, r.ship.panels.controls[vent.unwrap()].id));
        }
        let left = gas_aboard(&mut r, &bottles);
        eprintln!("{}: {start:.2} kg de gas a bordo, {out:.2} kg al vacío por {}, quedan {left:.2} kg; desajuste máximo {worst:.2e} kg", kind.id, vent.map_or("(sin válvula de venteo con mando)", |k| &r.ship.panels.controls[k].id));
    }
    report("gas a bordo", &bad);
}
