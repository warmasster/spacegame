//! What the crosshair tells about every control and instrument of the Alcotán: a name, how it
//! stands, what it is for (written in the data, or made from what it is), and an instrument out of
//! its green zone says so.
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
fn every_control_and_instrument_says_what_it_is() {
    let (_, ship) = spawn();
    let mut bad = Vec::new();
    for k in 0..ship.panels.controls.len() {
        let c = ship.panels.card_control(&ship.kind, &ship.store, k);
        if c.title.is_empty() || c.value.is_empty() || c.lines.first().is_none_or(|l| l.len() < 8) {
            bad.push(format!("mando {}: {c:?}", ship.panels.controls[k].id));
        }
    }
    for i in 0..ship.panels.indicators.len() {
        let c = ship.panels.card_indicator(&ship.kind, &ship.store, i);
        let ind = &ship.panels.indicators[i];
        let id = &ship.kind.panels[ind.panel].def.mandos[ind.index].id;
        if c.title.is_empty() || c.value.is_empty() || c.lines.is_empty() {
            bad.push(format!("indicador {}/{id}: {c:?}", ship.kind.panels[ind.panel].id));
        }
    }
    eprintln!("{} mandos y {} indicadores", ship.panels.controls.len(), ship.panels.indicators.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn an_instrument_out_of_its_green_zone_says_so() {
    let (_, ship) = spawn();
    // the hold starts open to vacuum: its gauge on the hold's own panel reads red
    let i = ship.panels.indicators.iter().position(|x| ship.kind.panels[x.panel].id == "sala_bodega" && ship.kind.panels[x.panel].def.mandos[x.index].id == "p").expect("the hold's pressure gauge");
    let c = ship.panels.card_indicator(&ship.kind, &ship.store, i);
    eprintln!("{c:?}");
    assert!(c.level == 2 || c.level == 3, "{c:?}");
}
