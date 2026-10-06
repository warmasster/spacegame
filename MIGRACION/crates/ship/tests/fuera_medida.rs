//! Which conduit parts of the Alcotán run outside its hull, and whose they are.
use lunar_ship::{ShipLibrary, geom::Role};
use std::{collections::BTreeMap, path::Path};

#[test]
fn conduits_outside_the_hull() {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = lunar_core::structure::Library::load(&defs.join("structures")).unwrap();
    let (ships, _) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let kind = ships.get("alcotan").unwrap();
    let hull = kind.def.casco.as_ref().unwrap();
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    for (i, id) in kind.parts.iter().enumerate() {
        if kind.roles[i] == Role::Conduit && !lunar_ship::far::in_hull(hull, kind.centers[i]) {
            let key: String = id.split('.').take(2).collect::<Vec<_>>().join(".");
            *by.entry(key).or_default() += 1;
        }
    }
    for (k, n) in &by {
        eprintln!("  {k}: {n}");
    }
    for (i, id) in kind.parts.iter().enumerate() {
        if id.starts_with("act_tren_morro.acometida") || id.starts_with("canal.410.") {
            let c = kind.centers[i];
            eprintln!("    {id} ({:.2}, {:.2}, {:.2})", c.x, c.y, c.z);
        }
    }
    let m = kind.machines.iter().find(|m| m.id == "act_tren_morro");
    let a = kind.actuators.iter().find(|a| a.id == "act_tren_morro");
    eprintln!("    actuador tren morro: {:?} {:?}", m.map(|m| m.part), a.map(|a| a.part.map(|p| kind.centers[p as usize])));
}
