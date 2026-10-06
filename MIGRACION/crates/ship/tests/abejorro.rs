//! The Abejorro, the one-seat cargo tug: its four engines lift it; its momentum wheels turn it
//! without propellant until they are full, and unload with the cold gas; its magnet takes a load
//! from under it, carries it and lets it go — and holds it with the tug dead.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, set::Structures, state::Structure},
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, ship::TICK};
use std::{path::Path, sync::Arc};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn kinds() -> (Library, ShipLibrary) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships)
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    for _ in 0..(secs / TICK).round() as usize {
        ship.update(s, &w(), TICK);
    }
}

fn press(ship: &mut Ship, s: &mut Structure, id: &str) {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"));
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
    run(s, ship, 0.25);
    ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
    run(s, ship, 0.1);
}

fn set(ship: &mut Ship, s: &Structure, id: &str, value: f64) {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"));
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Set { value }, s, &kind, &ship.store);
}

/// A key held on one axis of a sprung lever (as a seat's key does).
fn hold(ship: &mut Ship, s: &Structure, id: &str, value: f64) {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"));
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Axis { axis: 0, value }, s, &kind, &ship.store);
}

fn alone() -> (Structure, Ship, Arc<ShipKind>) {
    let (lib, ships) = kinds();
    let kind = ships.get("abejorro").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, glam::Quat::IDENTITY);
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &w(), 0.0);
    (s, ship, kind)
}

#[test]
fn its_engines_lift_it_with_a_tonne_under_it() {
    let (mut s, mut ship, _) = alone();
    run(&mut s, &mut ship, 5.0);
    for id in ["consola/tapa_arm", "consola/arm", "consola/arr", "consola/arr"] {
        press(&mut ship, &mut s, id);
    }
    run(&mut s, &mut ship, 3.0);
    set(&mut ship, &s, "consola/acelerador", 1.0);
    run(&mut s, &mut ship, 2.0);
    let get = |n: &str| ship.signal(n).unwrap_or(f64::NAN);
    eprintln!("motores: estado {} empuje {:.2} kN cada uno; fuerza {:.0} N; masa {:.0} kg (peso {:.1} kN)", get("motor_proa_izq.estado"), get("motor_proa_izq.empuje") / 1e3, s.force.y, s.mass, s.mass * 1.62 / 1e3);
    for e in ship.blackbox.entries.iter().rev().take(6) {
        eprintln!("  t {:.1}: {}", e.t, e.text);
    }
    assert!(s.force.y > (s.mass + 1000.0) * 1.62 * 1.5, "no levanta una tonelada con margen: {:.0} N", s.force.y);
    // and it hovers well inside its throttle: near the bottom of it, it does not climb
    set(&mut ship, &s, "consola/acelerador", 0.12);
    run(&mut s, &mut ship, 2.0);
    assert!(s.force.y < s.mass * 1.62, "al mínimo ya sube: {:.0} N contra {:.0}", s.force.y, s.mass * 1.62);
}

#[test]
fn it_turns_on_its_wheels_until_they_are_full_and_unloads_them_with_the_cold_gas() {
    let (mut s, mut ship, _) = alone();
    // (the cold gas off: only the wheels)
    press(&mut ship, &mut s, "consola/rcs");
    run(&mut s, &mut ship, 40.0);
    assert_eq!(ship.signal("rcs.maestro"), Some(0.0));
    assert!(ship.signal("giroscopo.giro").unwrap() > 0.95, "los giróscopos no cogen vueltas");
    let fuel = ship.signal("deposito_izq.masa").unwrap();
    // the stick held: a torque on the hull and no propellant spent
    hold(&mut ship, &s, "consola/palanca", 1.0);
    run(&mut s, &mut ship, 0.5);
    assert!(s.torque.length() > 300.0, "par {:.0} N·m", s.torque.length());
    assert_eq!(s.force, Vec3::ZERO);
    // the hull held still (as against the ground), they fill and stop answering
    run(&mut s, &mut ship, 8.0);
    assert!(ship.signal("giroscopo.carga").unwrap() > 0.99, "carga {:.2}", ship.signal("giroscopo.carga").unwrap());
    assert!(s.torque.length() < 1.0, "llenos y siguen girando: {:.0} N·m", s.torque.length());
    assert!((ship.signal("deposito_izq.masa").unwrap() - fuel).abs() < 1e-6, "girar con los giróscopos gasta propelente");
    // stick centred, DESCARGA and the cold gas: they empty, the nozzles holding the hull against them
    hold(&mut ship, &s, "consola/palanca", 0.0);
    press(&mut ship, &mut s, "consola/sas");
    press(&mut ship, &mut s, "consola/rcs");
    press(&mut ship, &mut s, "consola/descarga");
    run(&mut s, &mut ship, 12.0);
    let load = ship.signal("giroscopo.carga").unwrap();
    assert!(load < 0.6, "no se descargan: {load:.2}");
    assert!(ship.signal("deposito_izq.masa").unwrap() < fuel, "la descarga no gasta gas");
}

/// The Moon, a tug standing on level ground, and a crate come off an Alcotán set under it.
fn with_a_crate() -> (Structures, BodyRegistry, Ship, u64, u64) {
    let (lib, ships) = kinds();
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs().join("bodies/luna.jsonc")).unwrap()).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let (tug, alc) = (ships.get("abejorro").unwrap().clone(), ships.get("alcotan").unwrap().clone());
    let mut set = Structures::new(Arc::new(lib));
    let dir = level(&bodies);
    let id = set.place(&tug.blueprint, &bodies, 0, dir, 0.0, 0.0).unwrap();
    let mut ship = Ship::new(tug, id, 7).unwrap();
    ship.update(set.list.iter_mut().find(|s| s.id == id).unwrap(), &w(), 0.0);
    set.rest_on_ground(id, &bodies);
    // an Alcotán well away, whose crates are let go: one of them is the load
    let side = dir.any_orthonormal_vector();
    let far = (dir * bodies.get(0).radius + side * 80.0).normalize();
    let other = set.place(&alc.blueprint, &bodies, 0, far, 0.0, f64::from(alc.lift)).unwrap();
    let mut alcotan = Ship::new(alc, other, 3).unwrap();
    alcotan.set_signal("anclaje_cajas.soltar", 1.0);
    alcotan.update(set.list.iter_mut().find(|s| s.id == other).unwrap(), &w(), TICK);
    assert_eq!(set.separate(), 4);
    let crate_id = set.list.last().unwrap().id;
    // under the tug's magnet, on the ground
    let t = set.get(id).unwrap();
    let (pos, rot) = (t.to_world(Vec3::new(0.0, -1.2, 0.05)), t.rot);
    let k = set.index_of(crate_id).unwrap();
    let c = &mut set.list[k];
    c.rot = rot;
    c.pos = pos - (rot * c.center).as_dvec3();
    (c.vel, c.spin, c.resting) = (DVec3::ZERO, Vec3::ZERO, false);
    for _ in 0..180 {
        set.step(1.0 / 60.0, &bodies);
    }
    (set, bodies, ship, id, crate_id)
}

fn level(bodies: &BodyRegistry) -> DVec3 {
    let b = bodies.get(0);
    let mut best = (f64::MAX, DVec3::Y);
    let mut seed = 99u64;
    for _ in 0..400 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = |k: u32| ((seed >> (11 + 17 * k)) & 0xffff) as f64 / 65535.0 * 2.0 - 1.0;
        let dir = DVec3::new(r(0), r(1), r(2)).normalize_or(DVec3::Y);
        let side = dir.any_orthonormal_vector();
        let fwd = dir.cross(side);
        let at = |x: f64, z: f64| b.height((dir * b.radius + side * x + fwd * z).normalize());
        let hs = [at(0.0, 0.0), at(6.0, 0.0), at(-6.0, 0.0), at(0.0, 6.0), at(0.0, -6.0), at(80.0, 0.0), at(90.0, 8.0), at(72.0, -8.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

#[test]
fn its_magnet_takes_a_load_carries_it_and_holds_it_with_the_tug_dead() {
    let (mut set, bodies, mut ship, id, load) = with_a_crate();
    let tug = |set: &mut Structures| set.index_of(id).unwrap();
    let mass = set.get(id).unwrap().mass;
    let k = tug(&mut set);
    run(&mut set.list[k], &mut ship, 2.0);
    assert_eq!(ship.signal("iman.sujeta"), Some(0.0));
    // the magnet on: it takes the crate under it, up to its face
    press(&mut ship, &mut set.list[k], "consola/tapa_iman");
    press(&mut ship, &mut set.list[k], "consola/agarre");
    run(&mut set.list[k], &mut ship, 0.6);
    lunar_ship::cargo::serve(&mut ship, &mut set);
    run(&mut set.list[k], &mut ship, 0.1);
    assert_eq!(ship.signal("iman.activo"), Some(1.0));
    assert_eq!(ship.signal("iman.sujeta"), Some(1.0), "el imán no ha cogido la caja");
    let held = set.get(load).unwrap();
    assert!(held.held.is_some_and(|h| h.by == id));
    let t = set.get(id).unwrap();
    let top = held.parts.iter().flat_map(|p| p.shape.verts().map(|v| t.to_local(held.to_world(p.local.transform_point3(v))).y)).fold(f32::MIN, f32::max);
    assert!((top + 0.4).abs() < 0.02, "la caja no queda contra la cara del imán: su techo a y = {top:.2}");
    assert!((t.mass - mass - held.mass).abs() < 1.0, "el remolcador no carga con su peso");
    // carried: lifted for four seconds, the crate goes with it
    let up = bodies.get(0).up(t.pos);
    let before = t.to_local(held.to_world(held.center));
    for _ in 0..240 {
        let s = &mut set.list[k];
        s.force = up.as_vec3() * s.mass * 3.0;
        set.step(1.0 / 60.0, &bodies);
    }
    let (t, held) = (set.get(id).unwrap(), set.get(load).unwrap());
    assert!(t.to_local(held.to_world(held.center)).distance(before) < 1e-3);
    // the tug dead (battery off): it still holds; and it cannot let go without a pulse to spare
    press(&mut ship, &mut set.list[k], "consola/bat");
    run(&mut set.list[k], &mut ship, 30.0);
    lunar_ship::cargo::serve(&mut ship, &mut set);
    assert!(ship.signal("elec.bus").unwrap() < 1.0);
    assert_eq!(ship.signal("iman.sujeta"), Some(1.0), "sin corriente suelta la carga");
    // power back, the switch to SUELTA: the crate is free, with the tug's speed
    press(&mut ship, &mut set.list[k], "consola/bat");
    run(&mut set.list[k], &mut ship, 3.0);
    press(&mut ship, &mut set.list[k], "consola/agarre");
    run(&mut set.list[k], &mut ship, 0.6);
    lunar_ship::cargo::serve(&mut ship, &mut set);
    run(&mut set.list[k], &mut ship, 0.1);
    assert_eq!(ship.signal("iman.sujeta"), Some(0.0));
    let (t, held) = (set.get(id).unwrap(), set.get(load).unwrap());
    assert!(held.held.is_none() && (held.vel - t.vel).length() < 0.5);
    assert!((t.mass - mass).abs() < 1.0);
}
