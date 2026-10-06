//! Using the ship, as a crew would, with nothing but its panels:
//! - what every compartment needs is worked from inside it (its pressure read, vented, made up;
//!   each of its doors opened, its pressure difference read and equalised from that side; its
//!   lights), and the airlock from outside (vented, opened) — for any ship, from its definition;
//! - and the plans a crew falls back on actually work in the simulation: a hold breached for good
//!   with you in the cabin (vent the cabin, the door opens), a hold left in vacuum (equalise with
//!   the hand valve in the bulkhead, the door opens), coming in from outside to a pressurised hold (vent it from outside, the ramp
//!   opens), a bridge window gone (equalise and vent, the door opens), a room made up again.
use glam::{DQuat, DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World};
use std::path::Path;

fn spawn() -> (Structure, Ship) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship)
}

#[test]
fn every_room_is_worked_from_inside_and_the_airlock_from_outside() {
    let (s, ship) = spawn();
    let missing = lunar_ship::diag::usability(&ship, &s);
    assert!(
        missing.is_empty(),
        "{}",
        missing.join(
            "
"
        )
    );
}

// ---- in the simulation ----

const STEP: f64 = 0.05;

fn world() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = world();
    for _ in 0..(secs / STEP).round() as usize {
        ship.update(s, &w, STEP);
    }
}

fn kpa(ship: &Ship, room: &str) -> f64 {
    ship.signal(&format!("{room}.p")).unwrap_or(f64::NAN) / 1000.0
}

/// Press and let go of a control, as a hand does; whether it did anything.
fn press(ship: &mut Ship, s: &mut Structure, id: &str) -> bool {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"));
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
    run(s, ship, 0.2);
    ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
    run(s, ship, 0.1);
    o.changed
}

/// Turn a wheel `notches` of its steps (a valve's hand wheel: twenty from shut to wide open), as
/// a hand does with the mouse wheel; whether it moved.
fn turn(ship: &mut Ship, s: &mut Structure, id: &str, notches: f32) -> bool {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"));
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &Intent::Turn { notches, rate: 4.0, m: lunar_controls::Mods::default() }, s, &kind, &ship.store);
    run(s, ship, 0.1);
    o.changed
}

/// Lift its cover and throw a guarded switch.
fn guarded(ship: &mut Ship, s: &mut Structure, cover: &str, switch: &str) {
    assert!(press(ship, s, cover), "{cover} no se abre");
    assert!(press(ship, s, switch), "{switch} no se mueve");
}

/// The hydraulics up, the ramp shut and latched.
fn shut_ramp(ship: &mut Ship, s: &mut Structure) {
    press(ship, s, "techo/hpu_a");
    run(s, ship, 5.0);
    press(ship, s, "rampa_bodega/rampa");
    run(s, ship, 20.0);
    assert!(ship.signal("rampa.cerrada").unwrap_or(0.0) > 0.5, "la rampa no cierra");
}

fn open(ship: &Ship, closure: &str) -> bool {
    ship.signal(&format!("{closure}.cerrada")).unwrap_or(1.0) < 0.5
}

/// A plate of `room`'s hull gone for good.
fn breach(ship: &Ship, s: &mut Structure, room: &str) {
    let c = ship.kind.compartments.iter().find(|c| c.id == room).unwrap();
    let p = c.hull[c.hull.len() / 2];
    s.parts[p as usize].alive = false;
}

#[test]
fn a_hold_breached_for_good_with_you_in_the_cabin() {
    let (mut s, mut ship) = spawn();
    shut_ramp(&mut ship, &mut s);
    press(&mut ship, &mut s, "sala_bodega/repres");
    run(&mut s, &mut ship, 150.0);
    press(&mut ship, &mut s, "sala_bodega/repres");
    assert!(kpa(&ship, "bodega") > 50.0, "the hold made up: {:.1} kPa", kpa(&ship, "bodega"));
    breach(&ship, &mut s, "bodega");
    run(&mut s, &mut ship, 30.0);
    assert!(kpa(&ship, "bodega") < 3.0, "the hold leaks out");
    // from the cabin the door will not open across the difference
    press(&mut ship, &mut s, "sala_cabina/abrir_puerta_bodega");
    run(&mut s, &mut ship, 3.0);
    assert!(!open(&ship, "puerta_bodega"), "the door opened across {:.0} kPa", kpa(&ship, "cabina"));
    // vent the cabin (suits on): then it opens
    guarded(&mut ship, &mut s, "sala_cabina/tapa_venteo", "sala_cabina/venteo");
    run(&mut s, &mut ship, 60.0);
    eprintln!("cabina {:.1} kPa, bodega {:.1} kPa", kpa(&ship, "cabina"), kpa(&ship, "bodega"));
    assert!(kpa(&ship, "cabina") < 5.0, "the cabin vents: {:.1} kPa", kpa(&ship, "cabina"));
    press(&mut ship, &mut s, "sala_cabina/abrir_puerta_bodega");
    run(&mut s, &mut ship, 4.0);
    assert!(open(&ship, "puerta_bodega"), "the door opens once the pressures match");
}

#[test]
fn a_hold_left_in_vacuum_is_equalised_from_the_cabin() {
    let (mut s, mut ship) = spawn();
    shut_ramp(&mut ship, &mut s);
    assert!(kpa(&ship, "bodega") < 3.0);
    press(&mut ship, &mut s, "sala_cabina/abrir_puerta_bodega");
    run(&mut s, &mut ship, 3.0);
    assert!(!open(&ship, "puerta_bodega"));
    // the equalising valve in the bulkhead, its wheel on the cabin's side: the cabin's air into the
    // hold until they match
    assert!(turn(&mut ship, &mut s, "vi_bodega_b/volante", 20.0), "el volante no gira");
    run(&mut s, &mut ship, 90.0);
    eprintln!("cabina {:.1} kPa, bodega {:.1} kPa", kpa(&ship, "cabina"), kpa(&ship, "bodega"));
    assert!((kpa(&ship, "cabina") - kpa(&ship, "bodega")).abs() < 5.0);
    press(&mut ship, &mut s, "sala_cabina/abrir_puerta_bodega");
    run(&mut s, &mut ship, 4.0);
    assert!(open(&ship, "puerta_bodega"));
}

#[test]
fn coming_in_from_outside_to_a_pressurised_hold() {
    let (mut s, mut ship) = spawn();
    shut_ramp(&mut ship, &mut s);
    press(&mut ship, &mut s, "sala_bodega/repres");
    run(&mut s, &mut ship, 150.0);
    press(&mut ship, &mut s, "sala_bodega/repres");
    assert!(kpa(&ship, "bodega") > 50.0);
    // outside: the ramp will not come down on a pressurised hold
    press(&mut ship, &mut s, "rampa_ext/rampa");
    run(&mut s, &mut ship, 3.0);
    assert!(!open(&ship, "rampa"), "the ramp opened on {:.0} kPa", kpa(&ship, "bodega"));
    // vent the hold from outside, then it does
    guarded(&mut ship, &mut s, "rampa_ext/tapa_venteo", "rampa_ext/venteo");
    run(&mut s, &mut ship, 60.0);
    assert!(kpa(&ship, "bodega") < 3.0, "the hold vents from outside: {:.1} kPa", kpa(&ship, "bodega"));
    press(&mut ship, &mut s, "rampa_ext/rampa");
    run(&mut s, &mut ship, 25.0);
    assert!(open(&ship, "rampa"), "the ramp comes down");
}

#[test]
fn a_bridge_window_gone_and_the_bridge_made_up_again() {
    let (mut s, mut ship) = spawn();
    // a window of the bridge out
    let glass = (0..ship.kind.parts.len()).find(|&i| ship.kind.roles[i] == lunar_ship::geom::Role::Glass && ship.kind.centers[i].z > 2.7).unwrap();
    s.parts[glass].alive = false;
    run(&mut s, &mut ship, 90.0);
    assert!(kpa(&ship, "puente") < 3.0, "the bridge leaks out: {:.1}", kpa(&ship, "puente"));
    // from the cabin: equalise (the hand valve by the bridge door), vent, and the door opens
    assert!(turn(&mut ship, &mut s, "vi_puente_a/volante", 20.0), "el volante no gira");
    guarded(&mut ship, &mut s, "sala_cabina/tapa_venteo", "sala_cabina/venteo");
    run(&mut s, &mut ship, 90.0);
    press(&mut ship, &mut s, "sala_cabina/abrir_puerta_puente");
    run(&mut s, &mut ship, 4.0);
    assert!(open(&ship, "puerta_puente"), "the bridge door opens (cabina {:.1}, puente {:.1})", kpa(&ship, "cabina"), kpa(&ship, "puente"));
    // the window mended: shut the door, the vents, and make the bridge up from its own panel
    s.parts[glass].alive = true;
    press(&mut ship, &mut s, "sala_cabina/abrir_puerta_puente");
    press(&mut ship, &mut s, "sala_cabina/venteo");
    turn(&mut ship, &mut s, "vi_puente_a/volante", -20.0);
    run(&mut s, &mut ship, 8.0);
    assert!(!open(&ship, "puerta_puente"));
    press(&mut ship, &mut s, "sala_puente/repres");
    run(&mut s, &mut ship, 240.0);
    eprintln!("puente {:.1} kPa", kpa(&ship, "puente"));
    assert!(kpa(&ship, "puente") > 50.0, "the bridge made up: {:.1} kPa", kpa(&ship, "puente"));
}

#[test]
fn a_circuit_set_high_or_low_takes_its_loads_with_it() {
    let (mut s, mut ship) = spawn();
    // the life-support panel's own power port is on the "soporte" circuit
    let pi = ship.kind.panels.iter().position(|p| p.id == "soporte").unwrap();
    let port = ship.panels.panels[pi].power.expect("the panel draws power");
    let k = ship.panels.controls.iter().position(|c| c.id == "prioridades/soporte").expect("a priority selector per breaker");
    let kind = ship.kind.clone();
    for (value, want) in [(2.0, 255u8), (0.0, 20u8)] {
        ship.panels.intent(k, &Intent::Set { value }, &s, &kind, &ship.store);
        run(&mut s, &mut ship, 0.5);
        let p = &ship.ports[port];
        assert!(p.demand <= 0.0 || p.priority == want, "prioridad {} con el selector a {value}", p.priority);
    }
    eprintln!("{} mandos en total", ship.panels.controls.len());
}

#[test]
fn every_pressurised_room_starts_breathable() {
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 5.0);
    for c in &ship.kind.compartments.clone() {
        let p = ship.signal(&format!("{}.p", c.id)).unwrap() / 1000.0;
        let o2 = ship.signal(&format!("{}.o2", c.id)).unwrap() / 1000.0;
        eprintln!("{}: {p:.1} kPa, O2 {o2:.1} kPa", c.id);
        if p > 30.0 {
            assert!((18.0..=24.0).contains(&o2), "{}: O2 {o2:.1} kPa a {p:.1} kPa", c.id);
        }
    }
}

#[test]
fn a_ship_at_rest_raises_no_alarm() {
    // parked, cold, its pumps off, the ramp down and then shut: nothing is wrong with it, so its
    // annunciator is dark and its black box has no caution (pumps off is not a hydraulic fault)
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 20.0);
    let lit = |ship: &Ship| -> Vec<String> {
        (0..ship.panels.indicators.len())
            .filter(|&i| matches!(ship.panels.indicators[i].ind.kind, lunar_controls::indicator::IndKind::Annunciator { .. }))
            .flat_map(|i| ship.panels.indicators[i].st.cells.iter().enumerate().filter(|(_, c)| c.active).map(|(k, _)| format!("aviso {k}")).collect::<Vec<_>>())
            .collect()
    };
    assert!(lit(&ship).is_empty(), "avisos encendidos en reposo: {:?}", lit(&ship));
    press(&mut ship, &mut s, "rampa_bodega/rampa");
    run(&mut s, &mut ship, 40.0);
    assert!(ship.signal("rampa.cerrada").unwrap_or(0.0) > 0.5, "la rampa no cierra");
    assert!(lit(&ship).is_empty(), "avisos encendidos con la rampa cerrada: {:?}", lit(&ship));
    assert!(ship.signal("luces.emergencia").unwrap_or(0.0) < 0.5, "luces de emergencia encendidas");
    let cautions: Vec<&str> = ship.blackbox.entries.iter().filter(|e| e.level >= 1).map(|e| e.text.as_str()).collect();
    assert!(cautions.is_empty(), "avisos en la caja negra: {cautions:?}");
}
