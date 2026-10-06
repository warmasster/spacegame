//! Mending and putting back: what a welder does to a ship, and what the ship does with it.
//! - a hull plate gone is found where it was (the scanner's ray), put back and mended, and the
//!   room it holes is sealed again;
//! - a lamp destroyed goes dark and lights again once put back; a trunk's outlet destroyed cuts
//!   what plugs into it, and put back feeds it again;
//! - a damaged part mended is as good as it was.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World, geom::Role};
use std::path::Path;

fn spawn() -> (Structure, Ship, Library) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship, lib)
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
    for _ in 0..(secs / lunar_ship::ship::TICK).round() as usize {
        ship.update(s, &w, lunar_ship::ship::TICK);
    }
}

fn destroy(s: &mut Structure, i: usize) {
    s.parts[i].alive = false;
    s.parts[i].working = false;
    s.refresh();
}

#[test]
fn a_hull_plate_gone_is_found_put_back_and_seals_again() {
    let (mut s, mut ship, lib) = spawn();
    run(&mut s, &mut ship, 5.0);
    let cabin = ship.kind.compartments.iter().find(|c| c.id == "cabina").unwrap().clone();
    let p0 = ship.signal("cabina.p").unwrap();
    assert!(p0 > 60_000.0 && ship.signal("cabina.estanco").unwrap() > 0.5, "the cabin starts sealed at {p0:.0} Pa");
    // a plate of its hull that is not glass
    let plate = cabin.hull.iter().copied().find(|&k| ship.kind.roles[k as usize] == Role::Hull).unwrap() as usize;
    let (centre, radius) = (s.parts[plate].center, s.parts[plate].radius);
    destroy(&mut s, plate);
    run(&mut s, &mut ship, 8.0);
    let p1 = ship.signal("cabina.p").unwrap();
    assert!(p1 < p0 * 0.5 && ship.signal("cabina.estanco").unwrap() < 0.5, "holed, it leaks: {p1:.0} Pa");
    // the scanner's ray from inside the room finds the plate that is not there, where it was
    let eye = Vec3::new(0.0, 1.5, centre.z);
    let dir = (centre - eye).normalize();
    assert!(s.raycast(eye, dir, eye.distance(centre) + radius).is_none_or(|h| h.part as usize != plate));
    let gone = s.raycast_gone(eye, dir, 10.0).expect("the missing plate under the ray");
    assert_eq!(gone.part as usize, plate);
    assert_eq!(ship.kind.describe(plate).0, "Panel de casco");
    // put back (weak), then mended: whole again, and the room sealed
    assert!(s.mend(&lib.catalog, plate, 1.0).is_none(), "what is gone is not mended, it is put back");
    assert!(s.rebuild(&lib.catalog, plate, 0.2));
    assert!(!s.rebuild(&lib.catalog, plate, 0.2), "it is there already");
    assert!(s.parts[plate].alive && (s.parts[plate].damage() - 0.8).abs() < 0.01);
    let max = s.parts[plate].max_hp;
    assert_eq!(s.mend(&lib.catalog, plate, max), Some(1.0));
    run(&mut s, &mut ship, 3.0);
    assert!(ship.signal("cabina.estanco").unwrap() > 0.5, "sealed again");
    let p2 = ship.signal("cabina.p").unwrap();
    run(&mut s, &mut ship, 5.0);
    assert!(ship.signal("cabina.p").unwrap() >= p2 - 50.0, "and it holds what it has (or is made up)");
}

#[test]
fn a_lamp_and_an_outlet_put_back_work_again() {
    let (mut s, mut ship, lib) = spawn();
    run(&mut s, &mut ship, 5.0);
    let lamp = ship.kind.machines.iter().position(|m| m.id == "plafon_cab#0").unwrap();
    let part = ship.kind.machines[lamp].part.unwrap() as usize;
    assert!(ship.machines[lamp].m.light() > 0.5, "the cabin lamp is lit");
    assert_eq!(ship.kind.describe(part).0, "Plafón");
    destroy(&mut s, part);
    run(&mut s, &mut ship, 1.0);
    assert!(ship.machines[lamp].m.light() < 0.05, "destroyed, it is dark");
    assert!(s.rebuild(&lib.catalog, part, 1.0));
    run(&mut s, &mut ship, 2.0);
    assert!(ship.machines[lamp].m.light() > 0.5, "put back, it lights again");
    // every outlet in turn: gone, something that was fed goes dead; put back, it is fed again
    let fed = |ship: &Ship| ship.machines.iter().filter(|m| ship.ports[m.ports.clone()].iter().any(|p| p.demand > 0.0 && p.got >= p.demand * 0.5)).count();
    let before = fed(&ship);
    let outlets: Vec<usize> = (0..s.parts.len()).filter(|&i| ship.kind.describe(i).0 == "Toma de corriente").collect();
    assert!(outlets.len() >= 10, "{} tomas", outlets.len());
    let mut cut_something = 0;
    for &o in &outlets {
        destroy(&mut s, o);
        run(&mut s, &mut ship, 1.0);
        cut_something += usize::from(fed(&ship) < before);
        assert!(s.rebuild(&lib.catalog, o, 1.0));
        run(&mut s, &mut ship, 1.5);
        assert_eq!(fed(&ship), before, "{} put back", ship.kind.parts[o]);
    }
    assert!(cut_something >= 5, "only {cut_something} outlets cut anything");
}

#[test]
fn a_damaged_part_mended_is_as_good_as_new() {
    let (mut s, _ship, lib) = spawn();
    let i = (0..s.parts.len()).find(|&i| !s.parts[i].ghost && s.parts[i].max_hp > 100.0).unwrap();
    s.parts[i].hp = s.parts[i].max_hp * 0.3;
    let v = s.version;
    let step = s.parts[i].max_hp * 0.1;
    let mut share = 0.3;
    for _ in 0..10 {
        let now = s.mend(&lib.catalog, i, step).unwrap();
        assert!(now >= share);
        share = now;
    }
    assert_eq!(share, 1.0);
    assert!(s.parts[i].working && s.version > v, "what is kept per part follows");
    assert!(s.mend(&lib.catalog, 1 << 30, 1.0).is_none());
}
