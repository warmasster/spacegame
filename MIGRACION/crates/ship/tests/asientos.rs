//! The seats of the Alcotán and the panels worked from them: seated, every control and instrument
//! of a seat's panels is in sight (the head turns that far, the plate is not seen edge-on, nothing
//! is in the way) and within the hand's reach, and at arm's length from one of the seats that
//! share its panel. Both pilots work the main panel, the pedestal and the overhead; each, the
//! side console by them.
use glam::{DQuat, DVec3};
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
fn every_control_is_worked_from_its_seats() {
    let (s, ship) = spawn();
    let seat = |id: &str| ship.kind.seats.iter().find(|x| x.def.id == id).unwrap_or_else(|| panic!("no hay asiento {id}"));
    // what each pilot works
    for (id, panels) in [("piloto", ["principal", "pedestal", "techo", "lateral"]), ("copiloto", ["principal", "pedestal", "techo", "combustible"])] {
        for p in panels {
            assert!(seat(id).def.paneles.iter().any(|x| x == p), "el asiento {id} no trabaja el panel {p}");
        }
    }
    let bad = lunar_ship::diag::seat_reach(&ship, &s);
    for e in &bad {
        eprintln!("  {e}");
    }
    assert!(bad.is_empty(), "{} mandos o instrumentos fuera del alcance de su asiento", bad.len());
}

#[test]
fn the_pilots_sit_close_to_their_panels() {
    let (_, ship) = spawn();
    let main = ship.kind.panels.iter().find(|p| p.id == "principal").unwrap();
    let (w, h) = (main.layout.size[0] / 1000.0, main.layout.size[1] / 1000.0);
    let top = ship.kind.panels.iter().find(|p| p.id == "techo").unwrap();
    let over = top.frame.transform_point3(glam::Vec3::new(top.layout.size[0] / 2000.0, top.layout.size[1] / 2000.0, 0.0));
    for id in ["piloto", "copiloto"] {
        let eye = glam::Vec3::from_array(ship.kind.seats.iter().find(|x| x.def.id == id).unwrap().def.ojos);
        // the main panel in front of each, on their side: about a metre from the eyes
        let d = [0.25, 0.75].map(|x| main.frame.transform_point3(glam::Vec3::new(w * x, h * 0.5, 0.0)).distance(eye)).into_iter().fold(f32::MAX, f32::min);
        assert!((0.75..=1.15).contains(&d), "{id}: su lado del panel principal a {d:.2} m de los ojos");
        // the overhead ahead of the head and not far over it: looked at without craning back
        assert!(over.z > eye.z + 0.2 && over.y - eye.y < 1.0, "{id}: el panel de techo a ({:.2}, {:.2}) de los ojos (adelante, arriba)", over.z - eye.z, over.y - eye.y);
    }
}
