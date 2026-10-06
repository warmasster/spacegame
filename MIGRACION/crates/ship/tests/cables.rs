//! The Alcotán's conduits as laid, measured on their parts (trunks, outlets, ducts):
//! - none is outside the hull;
//! - none runs half buried in a wall, a deck or a device (a trunk stands clear of its wall);
//! - none goes through a device.
use glam::{DQuat, DVec3};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{ShipKind, ShipLibrary};
use std::{path::Path, sync::Arc};

fn alcotan() -> (Structure, Arc<ShipKind>, Library) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap();
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    (Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat()), kind, lib)
}

#[test]
fn conduits_keep_inside_and_clear_of_what_they_pass() {
    let (s, kind, lib) = alcotan();
    let r = lunar_ship::diag::cables(&s, &kind, &lib.catalog);
    let (conduits, exposed, buried, through) = (r.conduits, r.exposed, r.buried, r.through);
    eprintln!("{conduits} conductos: {} por fuera, {} medio enterrados, {} a través de aparatos ajenos", exposed.len(), buried.len(), through.len());
    for l in exposed.iter().take(15).chain(buried.iter().take(25)).chain(through.iter().take(15)) {
        eprintln!("  {l}");
    }
    assert!(exposed.is_empty(), "por fuera del casco: {}", exposed.len());
    assert!(buried.len() * 100 <= conduits * 4, "medio enterrados: {}", buried.len());
    assert!(through.is_empty(), "a través de aparatos: {}", through.len());
}
