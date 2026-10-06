//! Single failures: a ship keeps what it says is essential (`esenciales` in its data) whatever
//! one conduit is lost — a piece of trunk, an outlet, a duct, a pass through a bulkhead. Each is
//! destroyed in turn on a ship with its systems up, and the essentials read. Run with
//! `--nocapture` to see every cut and what, if anything, it took.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World, diag, geom::Role};
use std::path::Path;

#[test]
fn no_one_conduit_lost_takes_an_essential() {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    for kind in &ships.kinds {
        if kind.def.esenciales.is_empty() {
            continue;
        }
        let make = || {
            let bp = lib.blueprint(&kind.blueprint).unwrap();
            let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
            let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
            ship.update(&mut s, &World::default(), 0.0);
            (s, ship)
        };
        let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
        let conduits = kind.roles.iter().filter(|r| **r == Role::Conduit).count();
        let found = diag::single_failures(&make, &w, &|_, role| role == Role::Conduit).unwrap_or_else(|e| panic!("{e}"));
        eprintln!("{}: {conduits} canalizaciones cortadas una a una, {} esenciales; {} cortes pierden algo", kind.id, kind.def.esenciales.len(), found.len());
        for f in &found {
            eprintln!("  {}: {}", if f.part.is_empty() { "(sin cortar nada)" } else { &f.part }, f.lost.join("; "));
        }
        assert!(found.is_empty(), "{}: {} cortes únicos se llevan algo esencial", kind.id, found.len());
    }
}

#[test]
fn every_trunk_gone_does_take_them() {
    // the other side of the same coin: the cuts are real. With every conduit of the ship gone,
    // nothing reaches anything
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
    for _ in 0..200 {
        ship.update(&mut s, &w, lunar_ship::ship::TICK);
    }
    assert!(ship.signal("elec.bus_ess").unwrap() > 20.0 && ship.signal("elec.c_soporte").unwrap() > 20.0);
    let mut cut = 0;
    for i in 0..s.parts.len() {
        if kind.roles[i] == Role::Conduit && kind.parts[i].starts_with("canal.") {
            s.parts[i].alive = false;
            cut += 1;
        }
    }
    s.refresh();
    for _ in 0..100 {
        ship.update(&mut s, &w, lunar_ship::ship::TICK);
    }
    assert!(cut > 20, "{cut} trozos de tronco");
    for node in ["elec.bus_a", "elec.bus_b", "elec.bus_ess", "elec.c_soporte", "elec.c_avionica"] {
        assert!(ship.signal(node).unwrap() < 1.0, "{node} sigue con {:.1} V sin un solo tronco", ship.signal(node).unwrap());
    }
}
