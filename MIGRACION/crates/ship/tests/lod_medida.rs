//! How heavy the Alcotán's look is: triangles of the full look, by role and by the size of the
//! part they belong to, and how long meshing takes.
use glam::{DQuat, DVec3};
use lunar_core::{
    structure::{
        Library,
        look::{FlatMesher, Mesher},
        state::Structure,
    },
};
use lunar_ship::{ShipLibrary, geom::Role};
use std::path::Path;

#[test]
fn alcotan_look_weight() {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap();
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let t = std::time::Instant::now();
    let mut v = Vec::new();
    FlatMesher.mesh(&s, &lib.catalog, &mut v);
    eprintln!("look: {} triangles, {} parts, meshed in {:.1} ms", v.len() / 3, s.parts.len(), t.elapsed().as_secs_f64() * 1e3);
    let mut by_role = std::collections::BTreeMap::new();
    for (i, p) in s.parts.iter().enumerate() {
        let kind_ = &lib.catalog.parts[usize::from(p.kind)];
        let tris = match kind_.look {
            Some(l) => lib.catalog.looks[l as usize].1.idx.len() / 3,
            None => p.shape.faces.iter().map(|f| f.verts.len() - 2).sum(),
        };
        let role = format!("{:?}", kind.roles.get(i).copied().unwrap_or(Role::Component));
        let e = by_role.entry(role).or_insert((0usize, 0usize));
        e.0 += 1;
        e.1 += tris;
    }
    for (r, (n, t)) in &by_role {
        eprintln!("  {r:12} {n:5} partes {t:7} triángulos");
    }
}

#[test]
fn alcotan_tick_cost() {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap();
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = lunar_ship::Ship::new(kind, 1, 7).unwrap();
    let w = lunar_ship::World::default();
    for _ in 0..60 {
        ship.update(&mut s, &w, 1.0 / 60.0);
    }
    let t = std::time::Instant::now();
    for _ in 0..600 {
        ship.update(&mut s, &w, 1.0 / 60.0);
    }
    eprintln!("tick del Alcotán: {:.3} ms", t.elapsed().as_secs_f64() * 1e3 / 600.0);
}
