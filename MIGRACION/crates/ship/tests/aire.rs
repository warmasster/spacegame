//! The air of the Alcotán, as the web model has it: sealed rooms hold, openings vent them as fast
//! as the orifice lets them (choked), the hold repressurises from the bottles and vents to vacuum,
//! a breach is an explosive decompression that pulls toward it and kicks the ship, a damaged plate
//! tears under the pressure it can no longer hold, the gas cools as it expands and comes back to
//! temperature, mass is kept. No value ever runs away.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World, atmos::Atmos};
use std::path::Path;

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn load() -> (Library, ShipLibrary) {
    let mut lib = Library::load(&assets().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&assets(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships)
}

fn spawn() -> (Structure, Ship) {
    let (lib, ships) = load();
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    (s, ship)
}

const STEP: f64 = 0.05;

fn world() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

/// Run `secs`, checking every step that no compartment's air runs away.
fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = world();
    for _ in 0..(secs / STEP) as usize {
        ship.update(s, &w, STEP);
        sane(ship);
    }
}

fn sane(ship: &Ship) {
    for (c, plan) in ship.kind.compartments.iter().enumerate() {
        let a = ship.atmos.air[c];
        let p = ship.atmos.pressure(c);
        assert!(p.is_finite() && (0.0..=150e3).contains(&p), "{} a {p} Pa (t {:.1} s)", plan.id, ship.t);
        assert!(a.t.is_finite() && (100.0..=400.0).contains(&a.t), "{} a {} K (t {:.1} s)", plan.id, a.t, ship.t);
        assert!(a.o2 >= 0.0 && a.n2 >= 0.0 && a.co2 >= 0.0, "{}: {a:?}", plan.id);
    }
}

fn comp(ship: &Ship, id: &str) -> usize {
    ship.kind.compartments.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no compartment {id}"))
}

fn kpa(ship: &Ship, id: &str) -> f64 {
    ship.atmos.pressure(comp(ship, id)) / 1000.0
}

fn hold(ship: &mut Ship, s: &mut Structure, id: &str, secs: f64) {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no control {id}"));
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &lunar_controls::Intent::Press { elem: 0 }, s, &kind, &ship.store);
    run(s, ship, secs);
    ship.panels.intent(k, &lunar_controls::Intent::Release, s, &kind, &ship.store);
    eprintln!("  {id}: {o:?}");
}

/// The ramp shut and latched (hydraulics up first).
fn shut_ramp(ship: &mut Ship, s: &mut Structure) {
    hold(ship, s, "techo/hpu_a", 0.1);
    run(s, ship, 6.0);
    hold(ship, s, "rampa_bodega/rampa", 0.1);
    run(s, ship, 25.0);
    assert!(ship.signal("rampa.cerrada").unwrap() > 0.5, "the ramp latched shut");
}

#[test]
fn sealed_rooms_hold_their_air() {
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 120.0);
    for id in ["cabina", "puente"] {
        let p = kpa(&ship, id);
        eprintln!("{id}: {p:.1} kPa, {:.1} °C, estanco {}", ship.atmos.air[comp(&ship, id)].t - 273.15, ship.atmos.sealed[comp(&ship, id)]);
        assert!((66.0..74.0).contains(&p), "{id} holds ~70 kPa: {p}");
        assert!(ship.atmos.sealed[comp(&ship, id)]);
    }
    // the hold starts in vacuum, open to it by its ramp: not sealed
    assert!(kpa(&ship, "bodega") < 0.5);
    assert!(!ship.atmos.sealed[comp(&ship, "bodega")]);
}

#[test]
fn everything_open_vents_the_cabin_in_seconds() {
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 1.0);
    // the hold's door blown out with the ramp down: the cabin goes out through the hold
    for (k, id) in ship.kind.parts.iter().enumerate() {
        if id.starts_with("puerta_bodega.hoja") {
            s.parts[k].alive = false;
        }
    }
    s.version += 1;
    let c = comp(&ship, "cabina");
    let mut coldest: f64 = 400.0;
    let mut pulled = false;
    let mut kicked = 0.0f32;
    let mut t_10 = None;
    for k in 0..(20.0 / STEP) as usize {
        ship.update(&mut s, &world(), STEP);
        sane(&ship);
        coldest = coldest.min(ship.atmos.air[c].t);
        if !ship.atmos.vents.is_empty() {
            // somebody in the middle of the cabin is pulled aft, toward the hold
            let q = ship.atmos.push(Vec3::new(0.0, 1.0, -1.0), Some(c));
            let a = Atmos::accel(q, lunar_ship::atmos::DRAG_STANDING);
            if a.z < -0.5 {
                pulled = true;
            }
            kicked = kicked.max(ship.atmos.kick(s.com).0.length());
        }
        if t_10.is_none() && kpa(&ship, "cabina") < 10.0 {
            t_10 = Some(k as f64 * STEP);
        }
    }
    eprintln!("cabin under 10 kPa after {t_10:?} s, coldest {:.0} K, kick {kicked:.0} N, puente {:.1} kPa", coldest, kpa(&ship, "puente"));
    for e in ship.blackbox.entries.iter().take(12) {
        eprintln!("  t {:.1}: {}", e.t, e.text);
    }
    assert!(t_10.is_some_and(|t| t < 8.0), "the cabin empties fast");
    assert!(kpa(&ship, "cabina") < 1.0);
    assert!(!ship.atmos.sealed[c]);
    assert!(coldest < 280.0, "the gas left behind cools as it expands");
    assert!(pulled, "the flow pulls toward the opening");
    assert!(kicked > 100.0, "the jet out of the ramp pushes the ship");
    assert!(ship.blackbox.entries.iter().any(|e| e.text.contains("DESCOMPRESIÓN")), "announced");
    // the bridge's door was shut: it holds
    assert!(kpa(&ship, "puente") > 60.0);
}

#[test]
fn a_breach_is_an_explosive_decompression() {
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 1.0);
    let c = comp(&ship, "cabina");
    // its biggest hull plate goes
    let p = *ship.kind.compartments[c].hull.iter().max_by(|a, b| s.parts[**a as usize].shape.area().total_cmp(&s.parts[**b as usize].shape.area())).unwrap();
    s.parts[p as usize].alive = false;
    s.version += 1;
    let mut shock: f64 = 0.0;
    for _ in 0..(6.0 / STEP) as usize {
        ship.update(&mut s, &world(), STEP);
        sane(&ship);
        shock = shock.max(ship.atmos.shock[c]);
    }
    eprintln!("plate {} gone: cabin {:.2} kPa after 6 s, shock {shock:.2}", ship.kind.parts[p as usize], kpa(&ship, "cabina"));
    assert!(kpa(&ship, "cabina") < 5.0, "a plate's worth of hole empties the cabin in seconds");
    assert!(shock > 0.3, "violent");
    assert!(ship.blackbox.entries.iter().any(|e| e.text.contains("DESCOMPRESIÓN EXPLOSIVA")));
}

#[test]
fn the_hold_repressurises_and_vents() {
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 1.0);
    shut_ramp(&mut ship, &mut s);
    assert!(ship.atmos.sealed[comp(&ship, "bodega")], "shut and latched, the hold is sealed");
    hold(&mut ship, &mut s, "soporte/rep_bodega", 0.1);
    let mut t70 = None;
    for k in 0..(400.0 / STEP) as usize {
        ship.update(&mut s, &world(), STEP);
        sane(&ship);
        if t70.is_none() && kpa(&ship, "bodega") > 65.0 {
            t70 = Some(k as f64 * STEP);
        }
    }
    let b = comp(&ship, "bodega");
    let a = ship.atmos.air[b];
    let po2 = ship.signal("bodega.o2").unwrap() / 1000.0;
    eprintln!("hold at {:.1} kPa (O2 {po2:.1} kPa, {:.1} °C) after 400 s; over 65 kPa at {t70:?} s; cabin {:.1} kPa", kpa(&ship, "bodega"), a.t - 273.15, kpa(&ship, "cabina"));
    assert!(t70.is_some_and(|t| t < 300.0), "repressurised within minutes");
    assert!((65.0..78.0).contains(&kpa(&ship, "bodega")));
    assert!((18.0..24.0).contains(&po2), "breathable: {po2} kPa of O2");
    assert!((66.0..76.0).contains(&kpa(&ship, "cabina")), "the cabin left alone");
    // REPRES. closed, the vent valve open: back to vacuum
    hold(&mut ship, &mut s, "soporte/rep_bodega", 0.1);
    hold(&mut ship, &mut s, "soporte/tapa_venteo", 0.1);
    hold(&mut ship, &mut s, "soporte/venteo", 0.1);
    run(&mut s, &mut ship, 300.0);
    eprintln!("vented: hold {:.2} kPa, cabin {:.1} kPa", kpa(&ship, "bodega"), kpa(&ship, "cabina"));
    assert!(kpa(&ship, "bodega") < 2.0, "the vent valve empties the hold");
    assert!((66.0..76.0).contains(&kpa(&ship, "cabina")));
}

#[test]
fn a_damaged_plate_tears_under_pressure() {
    let (mut s, mut ship) = spawn();
    run(&mut s, &mut ship, 1.0);
    let c = comp(&ship, "cabina");
    let p = ship.kind.compartments[c].hull[0];
    let part = &mut s.parts[p as usize];
    // at 35 % a hull plate holds 400 kPa × 0.35² = 49 kPa: less than the cabin's 70
    part.hp = part.max_hp * 0.35;
    let mut torn = false;
    for _ in 0..(60.0 / STEP) as usize {
        ship.update(&mut s, &world(), STEP);
        sane(&ship);
        torn |= !s.parts[p as usize].alive;
    }
    eprintln!("plate {}: torn {torn}, cabin {:.1} kPa", ship.kind.parts[p as usize], kpa(&ship, "cabina"));
    assert!(ship.blackbox.entries.iter().any(|e| e.text.contains("cediendo")), "it creaks first");
    assert!(torn, "overloaded, it tears");
    assert!(ship.torn.iter().any(|(q, _)| *q == p), "handed over for its debris");
    assert!(kpa(&ship, "cabina") < 10.0);
}

#[test]
fn mass_is_kept_between_rooms() {
    let (mut s, mut ship) = spawn();
    // nothing adds gas: the injectors broken
    for m in ship.kind.machines.iter().filter(|m| m.id.starts_with("inyector") || m.id.starts_with("repres")) {
        s.parts[m.part.unwrap() as usize].alive = false;
    }
    s.version += 1;
    run(&mut s, &mut ship, 1.0);
    let total = |ship: &Ship| ship.atmos.air.iter().map(|a| a.mass()).sum::<f64>();
    let before = total(&ship);
    // the bridge door opened: cabin and bridge one room
    hold(&mut ship, &mut s, "puerta_puente_c/abrir", 0.1);
    run(&mut s, &mut ship, 30.0);
    assert!(ship.signal("puerta_puente.abierta").unwrap() > 0.5, "the door opened");
    let after = total(&ship);
    eprintln!("mass {before:.3} → {after:.3} kg; cabin {:.2}, bridge {:.2} kPa", kpa(&ship, "cabina"), kpa(&ship, "puente"));
    assert!((after - before).abs() / before < 1e-6, "no gas made or lost");
    assert!((kpa(&ship, "cabina") - kpa(&ship, "puente")).abs() < 0.5, "equal pressures");
}
