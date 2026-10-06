//! A ship nobody touches, any ship:
//! - left alone it settles: it stops being busy (so the game may run it at its slow pace) and
//!   stays so;
//! - the slow pace and the sleeping one keep the same accounts as the full one: over the same
//!   stretch its batteries, tanks and bottles end where they would have ticking every tick, its
//!   clock where it should, nothing broken, every network in balance;
//! - a ship that slept answers the moment it is worked again;
//! - with everything off (every contactor and breaker a hand has, open; every valve shut) nothing
//!   drains: not a battery, not a tank, not a bottle, not the air.
//! What it spends as it is put down (its idle load, how long its batteries last on it) is printed
//! for whoever tunes it: the figures are the ship's own.
mod comun;
use comun::*;
use lunar_machines::net::Medium;
use lunar_ship::{Pace, Ship, ship::TICK};
use lunar_signals::SignalId;

/// What a ship holds that can run out: (name, signal), every `<machine>.soc`, `.masa` and
/// `.nivel` its machines write.
fn holdings(r: &Rig) -> Vec<(String, SignalId)> {
    let mut out = Vec::new();
    for (m, plan) in r.kind.machines.iter().enumerate() {
        for field in ["soc", "masa", "nivel"] {
            let name = format!("{}.{field}", plan.id);
            if let Some(s) = r.find(&name).filter(|s| r.ship.store.meta(*s).writer == lunar_signals::Writer::Machine(m as u32)) {
                out.push((name, s));
            }
        }
    }
    out
}

#[test]
fn left_alone_every_ship_settles_and_stays_settled() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        if !r.until(900.0, |r| !r.ship.busy()) {
            bad.push(format!("{}: sin tocarla sigue ocupada tras un cuarto de hora", kind.id));
            continue;
        }
        let at = r.ship.t;
        for _ in 0..3000 {
            r.tick();
            if r.ship.busy() {
                bad.push(format!("{}: en reposo vuelve a estar ocupada sola a los {:.0} s (empuje {:?}, aire {:?} Pa/s)", kind.id, r.ship.t, r.s.force, r.ship.atmos.rate));
                break;
            }
        }
        bad.extend(r.broken());
        eprintln!("{}: deja de estar ocupada a los {at:.0} s y sigue así un minuto", kind.id);
    }
    report("reposo", &bad);
}

/// A ship run for `secs` as the game runs one at `pace` (a frame at a time).
fn at_pace(r: &mut Rig, secs: f64, pace: Pace) {
    let frame = 1.0 / 60.0;
    for _ in 0..(secs / frame).round() as usize {
        r.ship.run(&mut r.s, &r.w, frame, pace);
    }
}

#[test]
fn the_slow_paces_keep_the_same_accounts_as_the_full_one() {
    let mut bad = Vec::new();
    const SECS: f64 = 240.0;
    for kind in &fleet().kinds {
        // three of them, settled alike; then the same four minutes at each pace
        let mut rigs: Vec<Rig> = (0..3).map(|_| fleet().rig(kind)).collect();
        for r in &mut rigs {
            r.until(900.0, |r| !r.ship.busy());
        }
        let what = holdings(&rigs[0]);
        let start: Vec<f64> = what.iter().map(|(_, s)| rigs[0].get(*s)).collect();
        let t0 = rigs[0].ship.t;
        for (r, pace) in rigs.iter_mut().zip([Pace::Full, Pace::Slow, Pace::Asleep]) {
            at_pace(r, SECS, pace);
        }
        for (k, name) in [(1, "lento"), (2, "dormido")] {
            let (full, slow) = (&rigs[0], &rigs[k]);
            if (slow.ship.t - full.ship.t).abs() > lunar_ship::ship::ASLEEP_EVERY + 1.0 {
                bad.push(format!("{}: a ritmo {name} su reloj va por {:.1} s y a ritmo normal por {:.1} s", kind.id, slow.ship.t - t0, full.ship.t - t0));
            }
            for (i, (signal, s)) in what.iter().enumerate() {
                let (a, b) = (full.get(*s) - start[i], slow.get(*s) - start[i]);
                // (what moved at the full pace moved as much, give or take the stretch it was
                // last brought up to date; what did not, did not)
                let slack = 0.05 * a.abs() + (lunar_ship::ship::ASLEEP_EVERY + 1.0) / SECS * a.abs() + 1e-9;
                if (a - b).abs() > slack {
                    bad.push(format!("{}: en {SECS} s a ritmo normal {signal} cambia {a:+.6} y a ritmo {name} {b:+.6}", kind.id));
                }
            }
            bad.extend(slow.broken().into_iter().map(|b| format!("a ritmo {name}: {b}")));
            bad.extend(slow.out_of_balance().into_iter().map(|b| format!("a ritmo {name}: {b}")));
            if slow.ship.busy() {
                bad.push(format!("{}: a ritmo {name} se pone ocupada sola", kind.id));
            }
        }
        // asleep, then worked: it answers at once
        let r = &mut rigs[2];
        if let Some(&(k, n, e)) = r.switch_controls().first() {
            let was = r.value(k) >= 0.5;
            r.set(k, if was { 0.0 } else { 1.0 });
            r.ship.touch();
            r.ticks(3);
            if r.ship.nets[n].edges[e].closed == was {
                bad.push(format!("{}: tras dormir, {} no responde", kind.id, r.ship.panels.controls[k].id));
            }
        }
        let moved = what.iter().enumerate().filter(|(i, (_, s))| (rigs[0].get(*s) - start[*i]).abs() > 1e-9).count();
        eprintln!("{}: {SECS} s a los tres ritmos; {moved} de {} cosas que se gastan cambian en ese tiempo", kind.id, what.len());
    }
    report("ritmo lento", &bad);
}

/// The gas of every room, mole by mole.
fn air(ship: &Ship) -> [f64; 3] {
    ship.atmos.air.iter().fold([0.0; 3], |a, x| [a[0] + x.o2, a[1] + x.n2, a[2] + x.co2])
}

#[test]
fn with_everything_off_nothing_drains() {
    let mut bad = Vec::new();
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        r.run(5.0);
        // ---- as it is put down: what it spends, for whoever tunes it
        let owners = r.owners();
        let mut loads: Vec<(f64, String)> = r.nets_of(Medium::Electrico).into_iter().flat_map(|n| r.ports_on(n).filter(|(_, p)| p.got > 0.0).map(|(k, p)| (p.got, r.owner_name(&owners[k]))).collect::<Vec<_>>()).collect();
        loads.sort_by(|a, b| b.0.total_cmp(&a.0));
        eprintln!("{}: recién puesta gasta {:.0} W (baterías para {:.0} min); lo que más: {}", kind.id, r.sig("energia.consumo"), r.sig("energia.autonomia") / 60.0, loads.iter().take(6).map(|(w, who)| format!("{who} {w:.0} W")).collect::<Vec<_>>().join(", "));
        // ---- everything off: every switch a hand has on any network, open
        let switches = r.switch_controls();
        for &(k, ..) in &switches {
            r.uncover(k);
            r.set(k, 0.0);
        }
        r.run(10.0);
        let what = holdings(&r);
        let start: Vec<f64> = what.iter().map(|(_, s)| r.get(*s)).collect();
        let gas = air(&r.ship);
        let lost: f64 = (0..15_000).map(|_| {
            r.tick();
            r.ship.atmos.out.iter().sum::<f64>() * TICK
        }).sum();
        for (i, (name, s)) in what.iter().enumerate() {
            let d = r.get(*s) - start[i];
            if d.abs() > 1e-12 * start[i].abs() {
                bad.push(format!("{}: con todo desconectado, {name} cambia {d:+.3e} en 5 min (de {})", kind.id, start[i]));
            }
        }
        let now = air(&r.ship);
        for (k, gas_name) in ["O₂", "N₂", "CO₂"].iter().enumerate() {
            if (now[k] - gas[k]).abs() > 1e-9 * gas[k].abs() + 1e-9 {
                bad.push(format!("{}: con todo desconectado, el {gas_name} de sus compartimentos pasa de {:.6} a {:.6} mol (al vacío: {lost:.6} kg)", kind.id, gas[k], now[k]));
            }
        }
        // (nothing out of the batteries, nothing spent that is not being made that moment)
        let (spent, made, from) = (r.sig("energia.consumo"), r.sig("energia.generacion"), r.sig("energia.baterias"));
        if from.abs() > 1e-9 || spent > made + 1e-9 {
            bad.push(format!("{}: con todo desconectado sigue gastando {spent:.3} W ({from:.3} W de las baterías)", kind.id));
        }
        bad.extend(r.broken());
        eprintln!("{}: con sus {} interruptores abiertos, 5 min sin que cambie ninguna de {} reservas", kind.id, switches.len(), what.len());
    }
    report("con todo desconectado", &bad);
}
