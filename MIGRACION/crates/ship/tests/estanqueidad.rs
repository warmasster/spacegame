//! Leak-tightness of the Alcotán's geometry, apart from the air model (`diag::leaks`): with the
//! ramp and every door shut, the inside of each compartment, flood-filled over a voxel grid, must
//! not reach the outside nor another compartment through anything but its declared openings — a
//! missing plate, a gap round a leaf, a bulkhead short of the hull would show here as the place
//! the fill got out.
use glam::DQuat;
use glam::DVec3;
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World, diag};
use std::path::Path;

fn spawn() -> (Structure, Ship) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&dir.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&dir, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    (s, ship)
}

#[test]
fn every_compartment_is_sealed_with_its_closures_shut() {
    let (mut s, mut ship) = spawn();
    // the ramp shut (it starts open), every door already shut; posed
    ship.set_joint("rampa", 0.0);
    ship.update(&mut s, &World::default(), 0.0);
    let leaks = diag::leaks(&s, &ship.kind);
    for l in &leaks {
        eprintln!("{}: fuga cerca de {:?}; llega a {:?}", l.room, l.hole, l.reaches);
    }
    assert!(leaks.is_empty(), "fugas: {leaks:?}");
}

#[test]
fn an_open_ramp_lets_the_hold_out() {
    // the check sees what it must: the ramp open, the hold reaches space
    let (mut s, mut ship) = spawn();
    ship.update(&mut s, &World::default(), 0.0);
    let leaks = diag::leaks(&s, &ship.kind);
    assert!(leaks.iter().any(|l| l.room == "bodega" && l.hole.is_some()), "with the ramp down the hold is open to space: {leaks:?}");
}
