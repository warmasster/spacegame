//! Air moved on purpose (`docs/AIRE.md`), on the ships that have it, with nothing but their panels:
//! - a hand valve through a bulkhead brings both sides to one pressure, faster the more it is
//!   open; shut, nothing goes through it; its wheels on either side are one mechanism;
//! - the compressor draws the hold down into the tank, slower and slower, to its ultimate
//!   pressure and no further; the rest is vented by hand; what was recovered goes back into the
//!   hold from the tank, by itself;
//! - it stops without power, with its breaker open, with the tank full;
//! - whatever the valves and the compressor do, no gas is made nor lost;
//! - an idle ship (valves shut, compressor stopped) does none of this work.
//!
//! Run with `--nocapture` to read the numbers (times, pressures, energy, what is recovered).
use glam::{DQuat, DVec3, Vec3};
use lunar_controls::{Intent, Mods};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_machines::gas::Mix;
use lunar_ship::{Pace, Ship, ShipLibrary, World, atmos::Air};
use std::{path::Path, time::Instant};

fn spawn(id: &str) -> (Structure, Ship) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get(id).unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &world(), 0.0);
    (s, ship)
}

fn world() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

/// The ship's systems for `secs` of its own time.
fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = world();
    let until = ship.t + secs - 1e-6;
    while ship.t < until {
        ship.update(s, &w, 0.06);
    }
}

fn room(ship: &Ship, id: &str) -> usize {
    ship.kind.compartments.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay compartimento {id}"))
}

fn kpa(ship: &Ship, id: &str) -> f64 {
    ship.atmos.pressure(room(ship, id)) / 1000.0
}

fn signal(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("no hay señal {name}"))
}

fn control(ship: &Ship, id: &str) -> usize {
    ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"))
}

/// Press and let go of a control, as a hand does; whether it did anything.
fn press(ship: &mut Ship, s: &mut Structure, id: &str) -> bool {
    let k = control(ship, id);
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
    run(s, ship, 0.16);
    ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
    run(s, ship, 0.08);
    o.changed
}

/// Turn a wheel or a selector `notches` (the mouse wheel on it); whether it moved.
fn turn(ship: &mut Ship, s: &mut Structure, id: &str, notches: f32) -> bool {
    let k = control(ship, id);
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &Intent::Turn { notches, rate: 4.0, m: Mods::default() }, s, &kind, &ship.store);
    run(s, ship, 0.08);
    o.changed
}

/// The ramp shut and latched (hydraulics up first), the make-up gas of the cabin off and the
/// fans stopped: nothing adds gas, nothing lets it out, and nothing but what is being tried
/// carries it from one room to another (the ducts mix the rooms their vents feed).
fn seal(ship: &mut Ship, s: &mut Structure) {
    press(ship, s, "techo/hpu_a");
    run(s, ship, 6.0);
    press(ship, s, "rampa_bodega/rampa");
    run(s, ship, 25.0);
    assert!(signal(ship, "rampa.cerrada") > 0.5, "la rampa no cierra");
    press(ship, s, "techo/o2");
    press(ship, s, "techo/n2");
    press(ship, s, "techo/vent");
    assert!(signal(ship, "aire.o2") < 0.5 && signal(ship, "aire.n2") < 0.5 && signal(ship, "aire.vent") < 0.5);
    run(s, ship, 5.0);
}

/// The hold at cabin pressure, as its make-up gas leaves it (set straight: no bottle is touched).
fn fill_hold(ship: &mut Ship, s: &mut Structure) -> f64 {
    let b = room(ship, "bodega");
    ship.atmos.air[b] = Air::standard(70e3, ship.atmos.volume[b]);
    run(s, ship, 1.0);
    ship.atmos.air[b].moles()
}

/// Everything aboard, by gas: the compartments and the tanks (mol of O₂, N₂, CO₂).
fn all_gas(ship: &mut Ship) -> Mix {
    let mut all = ship.atmos.transfers.held(&mut ship.machines);
    for a in &ship.atmos.air {
        all.give(&Mix { o2: a.o2, n2: a.n2, co2: a.co2 });
    }
    all
}

fn same_gas(a: &Mix, b: &Mix, what: &str) {
    for (x, y, gas) in [(a.o2, b.o2, "O2"), (a.n2, b.n2, "N2"), (a.co2, b.co2, "CO2")] {
        assert!((x - y).abs() <= 1e-6 * x.abs().max(1.0), "{what}: {gas} {x:.6} → {y:.6} mol");
    }
}

/// The compressor's panel: lift the guard, press MARCHA (or press it again: stop).
fn start(ship: &mut Ship, s: &mut Structure) {
    if ship.panels.controls[control(ship, "panel_compresor_aire/tapa_marcha")].mech.value(&ship.panels.controls[control(ship, "panel_compresor_aire/tapa_marcha")].st) < 0.5 {
        assert!(press(ship, s, "panel_compresor_aire/tapa_marcha"), "la tapa no se levanta");
    }
    assert!(press(ship, s, "panel_compresor_aire/marcha"), "MARCHA no se mueve");
}

fn tank_mpa(ship: &Ship) -> f64 {
    signal(ship, "deposito_aire.p") / 1e6
}

#[test]
fn a_hand_valve_equalises_faster_the_more_it_is_open_and_passes_nothing_shut() {
    // the hold shut and empty, the cabin at its pressure: how long the valve between them takes
    // to bring them within 1 kPa, wide open and a quarter open
    let mut times = Vec::new();
    for notches in [20.0f32, 5.0] {
        let (mut s, mut ship) = spawn("alcotan");
        seal(&mut ship, &mut s);
        let before = all_gas(&mut ship);
        // shut: a minute, and nothing has gone through it
        let (cab0, hold0) = (kpa(&ship, "cabina"), kpa(&ship, "bodega"));
        run(&mut s, &mut ship, 60.0);
        assert!(hold0 < 0.01 && kpa(&ship, "bodega") < 0.01 && (kpa(&ship, "cabina") - cab0).abs() < 0.3, "cerrada deja pasar: bodega {:.3} kPa, cabina {cab0:.2} → {:.2}", kpa(&ship, "bodega"), kpa(&ship, "cabina"));
        assert_eq!(ship.atmos.flowing(), 0);
        // its wheel on the hold's side; the one on the cabin's side goes with it
        assert!(turn(&mut ship, &mut s, "vi_bodega_a/volante", notches));
        let (a, b) = (control(&ship, "vi_bodega_a/volante"), control(&ship, "vi_bodega_b/volante"));
        assert_eq!(ship.panels.controls[a].st, ship.panels.controls[b].st, "los dos volantes son uno");
        assert!((signal(&ship, "vi_bodega.apertura") - f64::from(notches) * 0.05).abs() < 1e-9);
        let t0 = ship.t;
        let mut hissed: f64 = 0.0;
        while (kpa(&ship, "cabina") - kpa(&ship, "bodega")).abs() > 1.0 {
            run(&mut s, &mut ship, 0.4);
            hissed = hissed.max(signal(&ship, "sonido.silbido"));
            assert!(ship.t - t0 < 1200.0, "no iguala");
        }
        let t = ship.t - t0;
        eprintln!(
            "válvula bodega–cabina al {:.0} %: iguala (a menos de 1 kPa) en {t:.0} s; cabina {:.1} kPa, bodega {:.1} kPa; ΔP {:.2} kPa; silbido {hissed:.2}",
            f64::from(notches) * 5.0,
            kpa(&ship, "cabina"),
            kpa(&ship, "bodega"),
            signal(&ship, "vi_bodega.dp") / 1000.0
        );
        assert!(hissed > 0.2, "no se oye el aire pasar");
        // one pressure on both sides: what the two rooms' air makes of both volumes
        run(&mut s, &mut ship, 60.0);
        let (pc, ph) = (kpa(&ship, "cabina"), kpa(&ship, "bodega"));
        // (a little under what the volumes alone say: the air that went through the valve has cooled)
        assert!((pc - ph).abs() < 0.3 && (31.5..38.5).contains(&pc), "cabina {pc:.2} kPa, bodega {ph:.2} kPa");
        same_gas(&before, &all_gas(&mut ship), "igualando");
        // from the other side, shut again: the door's own panel reads it
        assert!(turn(&mut ship, &mut s, "vi_bodega_b/volante", -20.0));
        assert_eq!(signal(&ship, "vi_bodega.apertura"), 0.0);
        assert_eq!(signal(&ship, "sonido.silbido"), 0.0);
        times.push(t);
    }
    assert!(times[1] > times[0] * 2.5, "más abierta, más deprisa: {times:?} s");
    assert!((8.0..40.0).contains(&times[0]), "abierta del todo: {} s", times[0]);
}

#[test]
fn cabin_and_bridge_are_equalised_by_hand_from_either_side() {
    let (mut s, mut ship) = spawn("alcotan");
    seal(&mut ship, &mut s);
    // the bridge half empty (as after a leak mended)
    let p = room(&ship, "puente");
    ship.atmos.air[p] = Air::standard(35e3, ship.atmos.volume[p]);
    run(&mut s, &mut ship, 1.0);
    let before = all_gas(&mut ship);
    assert!(signal(&ship, "vi_puente.dp") > 30e3);
    // (shut, each keeps its own)
    run(&mut s, &mut ship, 30.0);
    assert!((kpa(&ship, "puente") - 35.0).abs() < 0.5 && (kpa(&ship, "cabina") - 70.0).abs() < 0.5, "cerrada: cabina {:.2}, puente {:.2}", kpa(&ship, "cabina"), kpa(&ship, "puente"));
    // from the bridge's side
    assert!(turn(&mut ship, &mut s, "vi_puente_b/volante", 20.0));
    run(&mut s, &mut ship, 120.0);
    let (pc, pp) = (kpa(&ship, "cabina"), kpa(&ship, "puente"));
    eprintln!("cabina {pc:.1} kPa, puente {pp:.1} kPa tras 120 s con la válvula abierta");
    assert!((pc - pp).abs() < 0.5 && (58.0..64.0).contains(&pc), "cabina {pc:.2}, puente {pp:.2}");
    same_gas(&before, &all_gas(&mut ship), "igualando cabina y puente");
    // and from the cabin's, shut
    assert!(turn(&mut ship, &mut s, "vi_puente_a/volante", -20.0));
    assert_eq!(signal(&ship, "vi_puente.apertura"), 0.0);
}

/// The whole of it, as the crew does it: the hold at cabin pressure and the ramp to open. The
/// compressor draws the hold down into the tank; what it leaves is vented by hand; the ramp
/// opens; shut again, the hold is filled from the tank.
#[test]
fn the_hold_is_drawn_down_into_the_tank_vented_and_filled_again() {
    let (mut s, mut ship) = spawn("alcotan");
    seal(&mut ship, &mut s);
    let n0 = fill_hold(&mut ship, &mut s);
    let start_gas = all_gas(&mut ship);
    let p0 = kpa(&ship, "bodega");
    assert!((69.0..71.0).contains(&p0) && tank_mpa(&ship) == 0.0);
    // the ramp will not come down on a pressurised hold
    press(&mut ship, &mut s, "rampa_bodega/rampa");
    run(&mut s, &mut ship, 2.0);
    assert!(signal(&ship, "rampa.cerrada") > 0.5);
    // ---- the compressor: from the hold (where its selector starts) into the tank ----
    assert_eq!((signal(&ship, "compresor_aire.origen"), signal(&ship, "compresor_aire.destino")), (1.0, 0.0), "de la bodega al depósito");
    start(&mut ship, &mut s);
    let t0 = ship.t;
    let mut marks: Vec<(f64, f64)> = Vec::new();
    let mut energy = 0.0;
    let mut peak: f64 = 0.0;
    let mut lowest = p0;
    let mut heard: f64 = 0.0;
    let mut next = [60.0, 50.0, 40.0, 30.0, 20.0, 10.0, 7.0].into_iter().peekable();
    let mut hottest: f64 = 0.0;
    while signal(&ship, "compresor_aire.estado") != 3.0 {
        let t = ship.t;
        run(&mut s, &mut ship, 0.25);
        let power = signal(&ship, "compresor_aire.potencia");
        energy += power * (ship.t - t);
        peak = peak.max(power);
        hottest = hottest.max(signal(&ship, "bodega.t") - 273.15);
        heard = heard.max(signal(&ship, "sonido.compresor"));
        let p = kpa(&ship, "bodega");
        lowest = lowest.min(p);
        if next.peek().is_some_and(|m| p <= *m) {
            marks.push((next.next().unwrap(), ship.t - t0));
        }
        assert!(ship.t - t0 < 3600.0, "no llega a su límite: bodega {p:.2} kPa, estado {}", signal(&ship, "compresor_aire.estado"));
        assert!(matches!(signal(&ship, "compresor_aire.estado"), 1.0 | 2.0 | 3.0), "estado {}", signal(&ship, "compresor_aire.estado"));
    }
    let t_down = ship.t - t0;
    let p_ult = kpa(&ship, "bodega");
    let in_tank = ship.atmos.transfers.held(&mut ship.machines).moles();
    eprintln!(
        "bodega {p0:.1} → {p_ult:.2} kPa en {t_down:.0} s; depósito {:.2} MPa; {:.3} kWh, pico {:.1} kW; recuperado {:.1} % del aire de la bodega; la bodega llega a {hottest:.0} °C; baterías al {:.0} % y {:.0} %",
        tank_mpa(&ship),
        energy / 3.6e6,
        peak / 1e3,
        100.0 * in_tank / n0,
        signal(&ship, "bateria_1.soc") * 100.0,
        signal(&ship, "bateria_2.soc") * 100.0
    );
    eprintln!("  (kPa, s): {marks:.1?}");
    // slower and slower: each 10 kPa takes longer than the one before
    let steps: Vec<f64> = marks.windows(2).filter(|w| w[0].0 - w[1].0 == 10.0).map(|w| w[1].1 - w[0].1).collect();
    assert!(steps.len() >= 4 && steps.windows(2).all(|w| w[1] > w[0]), "cada 10 kPa tarda más que el anterior: {steps:.2?} s");
    // (in a minute and a half or two: its efficiency is invented so that nobody waits — AIRE.md)
    assert!((80.0..125.0).contains(&t_down), "{t_down:.0} s");
    assert!(peak <= 15_100.0, "su motor es de 15 kW: {peak:.0} W");
    assert!(heard > 0.9, "no se oye");
    // its ultimate pressure, and no further however long it is left: not a mole more leaves the
    // hold (what its gauge loses afterwards is the hold cooling down, with the compressor quiet)
    assert!((5.0..5.6).contains(&p_ult) && lowest >= 5.0, "{p_ult:.2} kPa (la más baja {lowest:.2})");
    let moved = ship.atmos.transfers.moved;
    run(&mut s, &mut ship, 300.0);
    assert_eq!(ship.atmos.transfers.moved, moved, "en su límite sigue sacando aire");
    eprintln!("  cinco minutos después, sin sacar nada más: {:.2} kPa a {:.0} °C", kpa(&ship, "bodega"), signal(&ship, "bodega.t") - 273.15);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 3.0);
    assert!(signal(&ship, "compresor_aire.giro") < 0.01 && signal(&ship, "compresor_aire.potencia") < 100.0, "en su límite el motor se para");
    assert!(in_tank / n0 > 0.9, "recuperado {:.3}", in_tank / n0);
    same_gas(&start_gas, &all_gas(&mut ship), "tras vaciar la bodega");
    // what it holds is what the hold had: cabin air, oxygen and nitrogen in the same shares
    let held = ship.atmos.transfers.held(&mut ship.machines);
    assert!((held.o2 / held.moles() - start_gas.o2 / start_gas.moles()).abs() < 0.01, "O2 en el depósito: {:.3}", held.o2 / held.moles());
    start(&mut ship, &mut s);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 0.0);
    // ---- the rest: by hand, through the valve by the ramp ----
    press(&mut ship, &mut s, "rampa_bodega/rampa");
    run(&mut s, &mut ship, 2.0);
    assert!(signal(&ship, "rampa.cerrada") > 0.5, "la rampa baja con {:.1} kPa", kpa(&ship, "bodega"));
    assert!(turn(&mut ship, &mut s, "vv_bodega_a/volante", 20.0));
    let t1 = ship.t;
    while kpa(&ship, "bodega") > 3.0 {
        run(&mut s, &mut ship, 1.0);
        assert!(ship.t - t1 < 600.0);
    }
    let t_vent = ship.t - t1;
    assert!(signal(&ship, "bodega.estanco") < 0.5, "venteando no está estanca");
    run(&mut s, &mut ship, 60.0);
    eprintln!("válvula de venteo abierta del todo: de {p_ult:.2} a 3 kPa en {t_vent:.0} s; un minuto después {:.2} kPa", kpa(&ship, "bodega"));
    assert!(t_vent < 25.0 && kpa(&ship, "bodega") < 1.0);
    let lost = n0 - in_tank;
    // the ramp opens, and shuts again
    assert!(press(&mut ship, &mut s, "rampa_bodega/rampa"));
    run(&mut s, &mut ship, 30.0);
    assert!(signal(&ship, "rampa.abierta") > 0.9, "la rampa no baja");
    assert!(turn(&mut ship, &mut s, "vv_bodega_a/volante", -20.0));
    press(&mut ship, &mut s, "rampa_bodega/rampa");
    run(&mut s, &mut ship, 30.0);
    assert!(signal(&ship, "rampa.cerrada") > 0.5 && signal(&ship, "bodega.estanco") > 0.5, "cerrada y estanca");
    // ---- back from the tank: its selectors the other way round ----
    assert!(turn(&mut ship, &mut s, "panel_compresor_aire/origen", -1.0) && turn(&mut ship, &mut s, "panel_compresor_aire/destino", 1.0));
    assert_eq!((signal(&ship, "compresor_aire.origen"), signal(&ship, "compresor_aire.destino")), (0.0, 1.0), "del depósito a la bodega");
    let before_refill = all_gas(&mut ship);
    start(&mut ship, &mut s);
    let t2 = ship.t;
    let mut free = 0.0;
    energy = 0.0;
    while signal(&ship, "compresor_aire.estado") != 3.0 {
        let t = ship.t;
        run(&mut s, &mut ship, 0.25);
        energy += signal(&ship, "compresor_aire.potencia") * (ship.t - t);
        if signal(&ship, "compresor_aire.estado") == 2.0 {
            free = ship.t - t2;
        }
        assert!(ship.t - t2 < 1800.0, "no acaba de llenar: estado {}", signal(&ship, "compresor_aire.estado"));
    }
    let t_fill = ship.t - t2;
    let back = ship.atmos.air[room(&ship, "bodega")].moles();
    eprintln!(
        "del depósito a la bodega: {t_fill:.0} s ({free:.0} de ellos el aire pasa solo), {:.4} kWh; bodega {:.1} kPa (O2 {:.1} kPa); depósito {:.3} MPa; vuelve el {:.1} % de lo recuperado, el {:.1} % del aire que tenía la bodega (se venteó el {:.1} %)",
        energy / 3.6e6,
        kpa(&ship, "bodega"),
        signal(&ship, "bodega.o2") / 1000.0,
        tank_mpa(&ship),
        100.0 * back / in_tank,
        100.0 * back / n0,
        100.0 * lost / n0
    );
    assert!(back / in_tank > 0.995, "vuelve {:.4} de lo recuperado", back / in_tank);
    assert!(back / n0 > 0.9 && kpa(&ship, "bodega") > 63.0);
    assert!((18.0..23.0).contains(&(signal(&ship, "bodega.o2") / 1000.0)), "respirable");
    assert!(t_fill < 45.0 && free > 10.0, "{t_fill:.0} s, {free:.0} de ellos por el paso libre");
    same_gas(&before_refill, &all_gas(&mut ship), "llenando la bodega");
}

#[test]
fn the_compressor_stops_without_power_with_its_breaker_open_and_with_the_tank_full() {
    let (mut s, mut ship) = spawn("alcotan");
    seal(&mut ship, &mut s);
    fill_hold(&mut ship, &mut s);
    start(&mut ship, &mut s);
    run(&mut s, &mut ship, 20.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 1.0);
    assert!(signal(&ship, "compresor_aire.caudal") > 0.01 && signal(&ship, "compresor_aire.potencia") > 1000.0);
    assert!(ship.busy(), "con el compresor en marcha la nave va a todo ritmo");
    // its breaker pulled: it stops, and the hold keeps what it has
    let moving = |ship: &mut Ship, s: &mut Structure, secs: f64| {
        let before = ship.atmos.transfers.moved;
        run(s, ship, secs);
        ship.atmos.transfers.moved - before
    };
    assert!(press(&mut ship, &mut s, "disyuntores/compresor"));
    run(&mut s, &mut ship, 3.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 5.0, "sin energía");
    assert_eq!(moving(&mut ship, &mut s, 30.0), 0.0, "con el disyuntor abierto sigue sacando aire");
    assert!(signal(&ship, "compresor_aire.potencia") == 0.0 && signal(&ship, "compresor_aire.giro") < 0.01);
    // pushed back in, it goes on
    assert!(press(&mut ship, &mut s, "disyuntores/compresor"));
    run(&mut s, &mut ship, 5.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 1.0);
    assert!(moving(&mut ship, &mut s, 5.0) > 1.0);
    // bus A dead (its contactor open): the same
    assert!(press(&mut ship, &mut s, "techo/bus_a"));
    run(&mut s, &mut ship, 3.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 5.0, "sin tensión en el bus");
    assert_eq!(moving(&mut ship, &mut s, 20.0), 0.0);
    assert!(press(&mut ship, &mut s, "techo/bus_a"));
    run(&mut s, &mut ship, 5.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 1.0);
    // the tank nearly full: it tops it up and stops there
    let k = ship.kind.machines.iter().position(|m| m.id == "deposito_aire").unwrap();
    let v = ship.machines[k].m.vessel().unwrap();
    let n = 11.98e6 * v.volume / (lunar_ship::atmos::R * v.t);
    v.gas = Mix { o2: n * 0.3, n2: n * 0.7, co2: 0.0 };
    let hold = kpa(&ship, "bodega");
    let t0 = ship.t;
    while signal(&ship, "compresor_aire.estado") != 4.0 {
        run(&mut s, &mut ship, 1.0);
        assert!(ship.t - t0 < 300.0, "no se para con el depósito lleno: {:.3} MPa", tank_mpa(&ship));
    }
    let t_full = ship.t - t0;
    run(&mut s, &mut ship, 30.0);
    eprintln!("depósito lleno a {:.3} MPa tras {t_full:.0} s; la bodega se queda en {:.1} kPa (estaba en {hold:.1})", tank_mpa(&ship), kpa(&ship, "bodega"));
    assert!((11.98..12.02).contains(&tank_mpa(&ship)) && signal(&ship, "deposito_aire.nivel") > 0.99);
    assert_eq!(moving(&mut ship, &mut s, 30.0), 0.0, "lleno, sigue metiendo");
    assert!(kpa(&ship, "bodega") > hold - 6.0 && kpa(&ship, "bodega") < hold);
    // into a room open to space it sends nothing: the ramp's vent open, the hold as destination
    press(&mut ship, &mut s, "panel_compresor_aire/marcha");
    assert!(turn(&mut ship, &mut s, "panel_compresor_aire/origen", 1.0));
    assert!(turn(&mut ship, &mut s, "panel_compresor_aire/destino", 1.0));
    // (from the cabin into the hold now; and not from a place into itself)
    assert_eq!((signal(&ship, "compresor_aire.origen"), signal(&ship, "compresor_aire.destino")), (2.0, 1.0));
    assert!(turn(&mut ship, &mut s, "vv_bodega_a/volante", 2.0));
    run(&mut s, &mut ship, 1.0);
    press(&mut ship, &mut s, "panel_compresor_aire/marcha");
    run(&mut s, &mut ship, 3.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 6.0, "destino abierto al vacío");
    assert_eq!(moving(&mut ship, &mut s, 10.0), 0.0);
    press(&mut ship, &mut s, "panel_compresor_aire/marcha");
    assert!(turn(&mut ship, &mut s, "panel_compresor_aire/destino", 1.0));
    let k = control(&ship, "panel_compresor_aire/marcha");
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    assert!(!o.changed && signal(&ship, "compresor_aire.marcha") < 0.5, "arranca de un sitio a sí mismo: {o:?}");
}

#[test]
fn an_idle_ship_does_none_of_this_work() {
    for id in ["alcotan", "cachalote"] {
        let (mut s, mut ship) = spawn(id);
        let w = world();
        // (a ship just made is busy a few minutes while its rooms come to their temperature)
        let t0 = Instant::now();
        while ship.busy() {
            run(&mut s, &mut ship, 10.0);
            assert!(ship.t < 900.0, "{id}: en reposo sigue ocupada tras un cuarto de hora");
        }
        eprintln!("{id}: deja de estar ocupada a los {:.0} s ({:.1} s de reloj)", ship.t, t0.elapsed().as_secs_f64());
        // nothing moved, nothing open, nothing asked for
        let pump = ship.kind.machines.iter().position(|m| m.id == "compresor_aire").unwrap();
        let check = |ship: &Ship, when: &str| {
            assert_eq!((ship.atmos.transfers.ticks, ship.atmos.transfers.moved), (0, 0.0), "{id} {when}: algo movió aire");
            assert!(!ship.atmos.transfers.active);
            assert!(ship.atmos.paths.iter().all(|p| !p.valve), "{id} {when}: una válvula abierta");
            assert_eq!(ship.atmos.flowing(), 0, "{id} {when}: aire pasando por alguna abertura");
            assert_eq!(ship.ports[ship.machines[pump].ports.start].demand, 0.0, "{id} {when}: el compresor parado pide energía");
            assert_eq!((signal(ship, "sonido.compresor"), signal(ship, "sonido.silbido")), (0.0, 0.0));
            assert_eq!(signal(ship, "compresor_aire.estado"), 0.0);
        };
        check(&ship, "a todo ritmo");
        // it is not kept busy: left alone it goes on its slow pace, and stays there
        let t = Instant::now();
        let n = 2000;
        for _ in 0..n {
            ship.update(&mut s, &w, lunar_ship::ship::TICK);
        }
        let tick = t.elapsed().as_secs_f64() * 1e3 / f64::from(n);
        check(&ship, "tras 2000 tics");
        assert!(!ship.busy(), "{id}: en reposo vuelve a estar ocupada");
        for _ in 0..600 {
            ship.run(&mut s, &w, 0.1, Pace::Slow);
        }
        assert!(!ship.busy(), "{id}: a ritmo lento se pone ocupada sola");
        check(&ship, "a ritmo lento");
        eprintln!("{id}: en reposo, tic de sistemas {tick:.3} ms; {} máquinas, {} mandos", ship.kind.machines.len(), ship.panels.controls.len());
        assert!(tick < 0.5, "{id}: {tick:.3} ms por tic");
    }
}

#[test]
fn every_ship_that_moves_air_has_its_valves_its_compressor_and_its_page() {
    use lunar_controls::indicator::IndKind;
    for id in ["alcotan", "cachalote"] {
        let (mut s, mut ship) = spawn(id);
        let kind = ship.kind.clone();
        // each hand valve: a wheel and a gauge on each side that is a compartment
        assert_eq!(kind.def.trasvase.valvulas.len(), 3, "{id}");
        for v in &kind.def.trasvase.valvulas {
            for side in ["a", "b"].iter().take(if v.entre[1] == "vacio" { 1 } else { 2 }) {
                let panel = format!("{}_{side}", v.id);
                control(&ship, &format!("{panel}/volante"));
                assert!(kind.panels.iter().any(|p| p.id == panel && p.def.mandos.iter().any(|m| m.id == "dp" && m.kind == "aguja")), "{id}: {panel} sin su manómetro");
            }
        }
        // the compressor's panel: its selectors, its guarded start, its lamps and gauges
        let panel = kind.panels.iter().find(|p| p.id == "panel_compresor_aire").unwrap_or_else(|| panic!("{id}: sin panel del compresor"));
        for (m, k) in [
            ("origen", "selector"),
            ("destino", "selector"),
            ("tapa_marcha", "tapa"),
            ("marcha", "pulsador"),
            ("l_marcha", "lampara"),
            ("l_limite", "lampara"),
            ("l_lleno", "lampara"),
            ("l_energia", "lampara"),
            ("p_origen", "aguja"),
            ("p_deposito", "aguja"),
            ("caudal", "display7"),
        ] {
            assert!(panel.def.mandos.iter().any(|c| c.id == m && c.kind == k), "{id}: falta {m} ({k})");
        }
        let sel = panel.def.mandos.iter().find(|c| c.id == "origen").unwrap();
        assert_eq!(sel.posiciones, ["DEPÓS.", "BODEGA", "CABINA", "PUENTE"], "{id}");
        // the AIRE page of its screens says the tank, the compressor and the valves
        let mut pages = 0;
        for ind in &ship.panels.indicators {
            let IndKind::Mfd(m) = &ind.ind.kind else { continue };
            for p in m.pages.iter().filter(|p| p.title == "AIRE") {
                pages += 1;
                let d = kind.panels[ind.panel].def.mandos[ind.index].paginas.iter().find(|x| x.titulo == "AIRE").unwrap();
                assert!(d.elementos.iter().any(|e| e.tipo == "deposito" && e.senal.as_deref() == Some("deposito_aire.nivel")), "{id}: la página AIRE no enseña el depósito");
                let text: Vec<&String> = d.elementos.iter().filter(|e| e.tipo == "texto").flat_map(|e| &e.lineas).collect();
                assert!(text.iter().any(|l| l.starts_with("COMPRESOR")) && text.iter().filter(|l| l.starts_with("V. ")).count() == 3, "{id}: {text:?}");
                assert_eq!(p.widgets.len(), d.elementos.len());
            }
        }
        assert!(pages >= 1, "{id}: sin página AIRE");
        // and it works: the bridge drawn down into the tank for a minute, nothing made nor lost
        press(&mut ship, &mut s, "techo/o2");
        press(&mut ship, &mut s, "techo/n2");
        let before = all_gas(&mut ship);
        assert!(turn(&mut ship, &mut s, "panel_compresor_aire/origen", 2.0));
        assert_eq!(signal(&ship, "compresor_aire.origen"), 3.0);
        let p0 = kpa(&ship, "puente");
        start(&mut ship, &mut s);
        run(&mut s, &mut ship, 60.0);
        eprintln!("{id}: puente {p0:.1} → {:.1} kPa en un minuto; depósito {:.2} MPa; {:.0} g/s", kpa(&ship, "puente"), tank_mpa(&ship), signal(&ship, "compresor_aire.caudal") * 1000.0);
        assert!(kpa(&ship, "puente") < p0 - 5.0 && tank_mpa(&ship) > 0.2);
        same_gas(&before, &all_gas(&mut ship), id);
    }
}

#[test]
fn a_wrecked_tank_lets_its_air_out_into_the_room_it_stands_in() {
    let (mut s, mut ship) = spawn("alcotan");
    seal(&mut ship, &mut s);
    // something in the tank: the bridge drawn down for a minute
    assert!(turn(&mut ship, &mut s, "panel_compresor_aire/origen", 2.0));
    start(&mut ship, &mut s);
    run(&mut s, &mut ship, 60.0);
    start(&mut ship, &mut s);
    let before = all_gas(&mut ship);
    let in_tank = ship.atmos.transfers.held(&mut ship.machines).moles();
    let hold = ship.atmos.air[room(&ship, "bodega")].moles();
    assert!(in_tank > 40.0 && signal(&ship, "deposito_aire.estanco") > 0.5, "{in_tank:.1} mol en el depósito");
    // the tank destroyed
    let k = ship.kind.machines.iter().position(|m| m.id == "deposito_aire").unwrap();
    s.parts[ship.kind.machines[k].part.unwrap() as usize].alive = false;
    s.version += 1;
    run(&mut s, &mut ship, 10.0);
    let left = ship.atmos.transfers.held(&mut ship.machines).moles();
    let gained = ship.atmos.air[room(&ship, "bodega")].moles() - hold;
    eprintln!("depósito destruido: tenía {in_tank:.1} mol, le quedan {left:.3}; la bodega gana {gained:.1} mol ({:.2} kPa)", kpa(&ship, "bodega"));
    assert!(left < in_tank * 0.001 && (gained - in_tank).abs() < in_tank * 0.01);
    same_gas(&before, &all_gas(&mut ship), "con el depósito roto");
    // and nothing more is sent into it
    assert_eq!(signal(&ship, "deposito_aire.estanco"), 0.0);
    start(&mut ship, &mut s);
    run(&mut s, &mut ship, 3.0);
    assert_eq!(signal(&ship, "compresor_aire.estado"), 6.0);
}

/// The quick way, as a station does with its airlock: the hold drawn down into the cabin (no
/// tank to fill against), as far as the cabin takes; and let back by hand through the valve.
#[test]
fn into_another_compartment_it_stops_at_one_atmosphere_and_the_valve_lets_it_back() {
    let (mut s, mut ship) = spawn("alcotan");
    seal(&mut ship, &mut s);
    fill_hold(&mut ship, &mut s);
    let before = all_gas(&mut ship);
    assert!(turn(&mut ship, &mut s, "panel_compresor_aire/destino", 2.0));
    assert_eq!((signal(&ship, "compresor_aire.origen"), signal(&ship, "compresor_aire.destino")), (1.0, 2.0), "de la bodega a la cabina");
    start(&mut ship, &mut s);
    let t0 = ship.t;
    let mut energy = 0.0;
    while signal(&ship, "compresor_aire.estado") != 4.0 {
        let t = ship.t;
        run(&mut s, &mut ship, 1.0);
        energy += signal(&ship, "compresor_aire.potencia") * (ship.t - t);
        assert!(ship.t - t0 < 1200.0, "no se para: cabina {:.1} kPa, estado {}", kpa(&ship, "cabina"), signal(&ship, "compresor_aire.estado"));
    }
    let (cabin, hold) = (kpa(&ship, "cabina"), kpa(&ship, "bodega"));
    eprintln!("de la bodega a la cabina: {:.1} min, {:.2} kWh; la cabina llega a {cabin:.1} kPa y la bodega queda en {hold:.1} kPa", (ship.t - t0) / 60.0, energy / 3.6e6);
    assert!((100.5..102.0).contains(&cabin) && (30.0..45.0).contains(&hold), "cabina {cabin:.1}, bodega {hold:.1}");
    assert!(ship.t - t0 < 600.0);
    same_gas(&before, &all_gas(&mut ship), "de la bodega a la cabina");
    // stopped, and the valve between them open: back to what they had
    start(&mut ship, &mut s);
    assert!(turn(&mut ship, &mut s, "vi_bodega_b/volante", 20.0));
    run(&mut s, &mut ship, 240.0);
    let (cabin, hold) = (kpa(&ship, "cabina"), kpa(&ship, "bodega"));
    eprintln!("  y de vuelta por la válvula del mamparo, 4 min: cabina {cabin:.1} kPa, bodega {hold:.1} kPa");
    assert!((cabin - hold).abs() < 0.5 && (67.0..73.0).contains(&cabin));
    same_gas(&before, &all_gas(&mut ship), "de vuelta por la válvula");
}
