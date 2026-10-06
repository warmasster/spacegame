//! Robustness: every control of every ship worked at random for a few simulated minutes (two a
//! run, four runs a ship), on the ground and in flight — and, in a second run, with things breaking as it goes (a machine
//! destroyed, a panel gone, a hull plate holed, a conduit cut). Whatever is done to it:
//! - nothing panics, no signal and no port figure is ever NaN or infinite, every store stays
//!   within its bounds, every joint within its travel, the air of every room a real gas;
//! - every network stays in balance at every tick;
//! - no breaker trips on a ship nothing has broken (its circuits are sized for what a crew can
//!   switch on at once);
//! - a tick of its systems stays within the budget `presupuesto.rs` sets;
//! - and the same seed gives the same ship at the end, to the last bit, twice: what multiplayer
//!   relies on.
//! The generator is a small one of its own (`comun::Rng`), seeded: a failure prints its seed and
//! the tick it happened at, and running it again gives the same failure. `LUNA_FUZZ_SEMILLA` and
//! `LUNA_FUZZ_MINUTOS` run another seed, or for longer.
mod comun;
use comun::*;
use lunar_controls::{Intent, Mods, intent::F_TRIPPED};
use lunar_ship::{ShipKind, geom::Role, ship::TICK};
use std::{sync::Arc, time::Instant};

/// Milliseconds a tick of a ship's systems may cost (`presupuesto.rs`: its_systems_and_its_scene_cost_little).
const TICK_BUDGET_MS: f64 = 1.0;

struct Run {
    print: u64,
    /// The fingerprint every 500 ticks: where two runs part, if they do.
    trail: Vec<u64>,
    bad: Vec<String>,
    actions: usize,
    failures: Vec<String>,
    trips: Vec<String>,
    /// What a tick cost: the middle one of them all (what a busy machine does not spoil), the
    /// mean and the worst (ms).
    tick_ms: f64,
    mean_ms: f64,
    worst_ms: f64,
    /// The costliest tick: when, and the last thing done to the ship before it.
    worst_at: String,
}

fn settings() -> (u64, f64) {
    let seed = std::env::var("LUNA_FUZZ_SEMILLA").ok().and_then(|s| s.parse().ok()).unwrap_or(20_261_004);
    let minutes = std::env::var("LUNA_FUZZ_MINUTOS").ok().and_then(|s| s.parse().ok()).unwrap_or(2.0);
    (seed, minutes)
}

/// One thing a hand might do to control `k`.
fn act(r: &mut Rig, rng: &mut Rng, k: usize, held: &mut Vec<(usize, u32)>) {
    let m = Mods { coarse: rng.below(4) == 0, fine: rng.below(6) == 0 };
    let i = match rng.below(9) {
        0..=2 => {
            held.push((k, 1 + rng.below(40) as u32));
            // (the part of it a hand can land on: a key of a keypad, a button of a bezel or its glass)
            let elem = match r.ctl_kind(k) {
                "teclado" => rng.below(13),
                "bisel" => [rng.below(20), 255][usize::from(rng.below(8) == 0)],
                _ => 0,
            };
            Intent::Press { elem: elem as u8 }
        }
        3 => Intent::Turn { notches: (rng.below(7) as f32 - 3.0), rate: 1.0 + rng.unit() as f32 * 20.0, m },
        4 => Intent::Set { value: [0.0, 1.0, 2.0, rng.unit(), -rng.unit(), rng.unit() * 200e3][rng.below(6)] },
        5 => Intent::Axis { axis: rng.below(2) as u8, value: rng.unit() * 2.0 - 1.0 },
        6 => Intent::Drag { dx: rng.unit() as f32 - 0.5, dy: rng.unit() as f32 - 0.5, m },
        7 => Intent::Hold { secs: rng.unit() as f32 },
        _ => Intent::Type(['0', '7', '.', 'C', 'E', 'x'][rng.below(6)]),
    };
    r.intent(k, &i);
}

/// One thing that might break.
fn fail(r: &mut Rig, rng: &mut Rng) -> String {
    let kind = r.kind.clone();
    let pick = |rng: &mut Rng, of: &[usize]| of[rng.below(of.len())];
    let part = match rng.below(5) {
        0 | 1 => {
            let machines: Vec<usize> = kind.machines.iter().filter_map(|m| m.part.map(|p| p as usize)).collect();
            if machines.is_empty() { return "nada".into() } else { pick(rng, &machines) }
        }
        2 => {
            let hull: Vec<usize> = kind.compartments.iter().flat_map(|c| c.hull.iter().map(|p| *p as usize)).collect();
            if hull.is_empty() { rng.below(kind.parts.len()) } else { pick(rng, &hull) }
        }
        3 => {
            let conduits: Vec<usize> = (0..kind.parts.len()).filter(|&i| kind.roles[i] == Role::Conduit).collect();
            if conduits.is_empty() { rng.below(kind.parts.len()) } else { pick(rng, &conduits) }
        }
        _ => rng.below(kind.parts.len()),
    };
    // gone, or only hurt (it leaks, it flickers, it gives less)
    if rng.below(3) == 0 {
        let p = &mut r.s.parts[part];
        p.hp = p.max_hp * (0.05 + 0.6 * rng.unit() as f32);
        p.working = p.hp > p.max_hp * 0.25;
        format!("dañada {}", kind.parts[part])
    } else {
        r.destroy(part);
        format!("destruida {}", kind.parts[part])
    }
}

fn fuzz(kind: &Arc<ShipKind>, seed: u64, minutes: f64, breaking: bool) -> Run {
    let mut r = fleet().rig(kind);
    let mut rng = Rng(seed ^ comun_hash(&kind.id));
    let mut run = Run { print: 0, trail: Vec::new(), bad: Vec::new(), actions: 0, failures: Vec::new(), trips: Vec::new(), tick_ms: 0.0, mean_ms: 0.0, worst_ms: 0.0, worst_at: String::new() };
    let n = r.ship.panels.controls.len();
    let breakers: Vec<usize> = (0..n).filter(|&k| r.ctl_kind(k) == "disyuntor").collect();
    let mut tripped = vec![false; n];
    let mut held: Vec<(usize, u32)> = Vec::new();
    let ticks = (minutes * 60.0 / TICK) as usize;
    let mut cost: Vec<f32> = Vec::with_capacity(ticks);
    let mut last = (0usize, usize::MAX);
    for tick in 0..ticks {
        // a hand on something four times a second or so; let go a little later
        if n > 0 && rng.below(12) == 0 {
            let k = rng.below(n);
            act(&mut r, &mut rng, k, &mut held);
            run.actions += 1;
            last = (tick, k);
        }
        for h in &mut held {
            h.1 -= 1;
        }
        for &(k, _) in held.iter().filter(|h| h.1 == 0) {
            r.intent(k, &Intent::Release);
        }
        held.retain(|h| h.1 > 0);
        // off the ground and back now and then (what waits for the weight on the gear sees both)
        if rng.below(1500) == 0 {
            r.w = if r.w.altitude > 1.0 { grounded() } else { airborne() };
        }
        if breaking && rng.below(1000) == 0 {
            let what = fail(&mut r, &mut rng);
            run.failures.push(format!("t {:.1}: {what}", r.ship.t));
        }
        let t = Instant::now();
        r.tick();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        cost.push(ms as f32);
        if ms > run.worst_ms {
            run.worst_ms = ms;
            run.worst_at = format!("tic {tick} (t {:.1} s), {} tics después de tocar {}", r.ship.t, tick - last.0, r.ship.panels.controls.get(last.1).map_or("nada", |c| c.id.as_str()));
        }
        if run.bad.len() < 12 {
            run.bad.extend(r.broken().into_iter().chain(r.out_of_balance()).map(|b| format!("semilla {seed}, tic {tick}: {b}")));
        }
        for &k in &breakers {
            let now = r.ship.panels.controls[k].st.has(F_TRIPPED);
            if now && !tripped[k] {
                run.trips.push(format!("t {:.1}: {}", r.ship.t, r.ship.panels.controls[k].id));
            }
            tripped[k] = now;
        }
        if tick % 500 == 499 {
            run.trail.push(r.fingerprint());
        }
    }
    run.print = r.fingerprint();
    run.mean_ms = cost.iter().map(|c| f64::from(*c)).sum::<f64>() / ticks.max(1) as f64;
    cost.sort_by(f32::total_cmp);
    run.tick_ms = cost.get(ticks / 2).copied().map_or(0.0, f64::from);
    run
}

/// A number of a ship's name, to give each its own run from one seed.
fn comun_hash(id: &str) -> u64 {
    id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// Every ship fuzzed twice with the same seed, each on a thread of its own; what went wrong.
fn all(breaking: bool) -> Vec<String> {
    let (seed, minutes) = settings();
    let kinds = &fleet().kinds;
    let runs: Vec<(Run, Run)> = std::thread::scope(|scope| {
        let handles: Vec<_> = kinds.iter().map(|kind| scope.spawn(move || (fuzz(kind, seed, minutes, breaking), fuzz(kind, seed, minutes, breaking)))).collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    });
    let mut bad = Vec::new();
    for (kind, (a, b)) in kinds.iter().zip(runs) {
        eprintln!(
            "{}: semilla {seed}, {minutes} min{}: {} acciones, {} averías, {} disparos de disyuntor; tic {:.3} ms (mediana; media {:.3}, peor {:.2}: {}); huella {:016x}",
            kind.id,
            if breaking { " con averías" } else { "" },
            a.actions,
            a.failures.len(),
            a.trips.len(),
            a.tick_ms,
            a.mean_ms,
            a.worst_ms,
            a.worst_at,
            a.print
        );
        for f in a.failures.iter().take(12) {
            eprintln!("    {f}");
        }
        bad.extend(a.bad.iter().map(|x| format!("{}: {x}", kind.id)));
        if a.print != b.print {
            let at = a.trail.iter().zip(&b.trail).position(|(x, y)| x != y);
            bad.push(format!("{}: la misma semilla ({seed}) da dos naves distintas al final ({:016x} y {:016x}); se separan {}", kind.id, a.print, b.print, at.map_or("en los últimos tics".to_string(), |k| format!("entre los tics {} y {}", k * 500, k * 500 + 499))));
        }
        if a.tick_ms > TICK_BUDGET_MS {
            bad.push(format!("{}: {:.3} ms por tic (mediana; media {:.3}) con todo en uso (presupuesto: {TICK_BUDGET_MS} ms)", kind.id, a.tick_ms, a.mean_ms));
        }
        if !breaking {
            bad.extend(a.trips.iter().map(|t| format!("{}: sin que nada se rompa salta un disyuntor ({t}): su circuito puede pedir más que su nominal", kind.id)));
        }
    }
    bad
}

#[test]
fn every_control_worked_at_random_breaks_nothing_and_the_same_seed_ends_the_same() {
    report("mandos al azar", &all(false));
}

#[test]
fn with_things_breaking_at_random_it_still_holds_together_and_ends_the_same() {
    report("mandos y averías al azar", &all(true));
}
