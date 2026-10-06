//! The ship's electrical balance (`power`): on batteries, what the loads take comes out of them
//! and the time left is finite; with the reactor up, generation covers it and they stop draining.
//! And the generic "energia" section is on a panel of every ship that mounts it.
use glam::{DQuat, DVec3};
use lunar_controls::Intent;
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World, power::NO_DRAIN, ship::TICK};
use std::path::Path;

fn spawn(name: &str) -> (Structure, Ship) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get(name).unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship)
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = World::default();
    for _ in 0..(secs / TICK) as usize {
        ship.update(s, &w, TICK);
    }
}

#[test]
fn the_balance_says_what_is_made_what_is_spent_and_how_long_the_batteries_last() {
    let (mut s, mut ship) = spawn("alcotan");
    let read = |ship: &Ship, n: &str| ship.signal(n).unwrap_or_else(|| panic!("no hay señal {n}"));
    // ---- on batteries: nothing made, what is spent comes out of them, and they will run out
    // (the time left is right within seconds of looking, not after a long average)
    run(&mut s, &mut ship, 4.0);
    let early = read(&ship, "energia.autonomia");
    run(&mut s, &mut ship, 26.0);
    assert!((early / read(&ship, "energia.autonomia") - 1.0).abs() < 0.25, "a los 4 s dice {:.0} min y a los 30 s {:.0}", early / 60.0, read(&ship, "energia.autonomia") / 60.0);
    let (made, spent, from, left, charge) = (read(&ship, "energia.generacion"), read(&ship, "energia.consumo"), read(&ship, "energia.baterias"), read(&ship, "energia.autonomia"), read(&ship, "energia.carga"));
    eprintln!("a baterías: genera {made:.0} W, consume {spent:.0} W, de las baterías {from:.0} W, quedan {:.0} min, carga {:.0} %", left / 60.0, charge * 100.0);
    assert!(made.abs() < 50.0 && spent > 100.0, "genera {made}, consume {spent}");
    assert!((from - spent).abs() < spent * 0.25, "lo que sale de las baterías ({from:.0} W) es lo que se gasta ({spent:.0} W)");
    assert!((read(&ship, "energia.balance") - (made - spent)).abs() < 1e-6);
    assert!(left > 600.0 && left < NO_DRAIN, "quedan {left} s");
    assert!((0.5..=1.0).contains(&charge));
    // (the time left is the charge over how fast it goes: within a third of what the figures say)
    let soc0 = charge;
    run(&mut s, &mut ship, 60.0);
    let rate = (soc0 - read(&ship, "energia.carga")) / 60.0;
    let honest = read(&ship, "energia.carga") / rate;
    assert!((read(&ship, "energia.autonomia") / honest - 1.0).abs() < 0.35, "dice {:.0} min, al ritmo medido serían {:.0}", read(&ship, "energia.autonomia") / 60.0, honest / 60.0);
    // ---- the reactor up and warm: it makes what is spent and more, and the batteries fill
    let k = |ship: &Ship, id: &str| ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay {id}"));
    let kind = ship.kind.clone();
    for id in ["reactor/tapa_marcha", "reactor/marcha"] {
        let c = k(&ship, id);
        ship.panels.intent(c, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    }
    run(&mut s, &mut ship, 420.0);
    let (made, spent, from, left) = (read(&ship, "energia.generacion"), read(&ship, "energia.consumo"), read(&ship, "energia.baterias"), read(&ship, "energia.autonomia"));
    eprintln!("con el reactor (7 min): genera {made:.0} W, consume {spent:.0} W, de las baterías {from:.0} W, quedan {:.0} min", left / 60.0);
    assert!(made > 3000.0 && from < spent * 0.3, "genera {made:.0} W, consume {spent:.0} W, de las baterías {from:.0} W");
    // what is made is what the reactor's converter gives: the current that goes through the
    // converters between its bus and the ship's is not made twice (only their loss is spent)
    let source = read(&ship, "conversor.p");
    assert!((made / source - 1.0).abs() < 0.02, "genera {made:.0} W y el conversor da {source:.0} W");
    assert!(spent < 9500.0, "consume {spent:.0} W: lo de antes, el reactor y lo que pierden los convertidores");
    // at full power it makes more than is spent: the batteries fill and there is no countdown
    let wheel = k(&ship, "reactor/potencia");
    ship.panels.intent(wheel, &Intent::Set { value: 1.0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 300.0);
    let (made, spent, from) = (read(&ship, "energia.generacion"), read(&ship, "energia.consumo"), read(&ship, "energia.baterias"));
    eprintln!("a plena potencia (5 min más): genera {made:.0} W, consume {spent:.0} W, de las baterías {from:.0} W");
    // (charging, or already full: nothing comes out of them)
    assert!(from < 1.0 && made > spent - 1.0 && (from < 0.0 || read(&ship, "energia.carga") > 0.98), "las baterías se cargan: de ellas salen {from:.0} W (carga {:.2})", read(&ship, "energia.carga"));
    assert_eq!(read(&ship, "energia.autonomia"), NO_DRAIN, "cargando no hay cuenta atrás");
}

#[test]
fn the_energy_section_is_the_same_on_every_panel_that_carries_it() {
    for name in ["alcotan", "cachalote"] {
        let (_, ship) = spawn(name);
        // the panel is mounted, and its plate carries the section's five instruments, each
        // called after its group
        let plan = ship.kind.panels.iter().find(|p| p.id == "energia").unwrap_or_else(|| panic!("{name}: no hay panel de energía"));
        for c in ["genera", "consumo", "quedan", "carga", "estado"] {
            let id = format!("energia_{c}");
            assert!(plan.def.mandos.iter().any(|x| x.id == id && x.grupo.as_deref() == Some("energia")), "{name}: al panel le falta {id}");
        }
        assert_eq!(plan.def.grupos.iter().find(|g| g.id == "energia").and_then(|g| g.titulo.as_deref()), Some("ENERGÍA"));
    }
}
