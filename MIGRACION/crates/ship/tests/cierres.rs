//! Closures of the Alcotán: the ramp and the doors open and shut smoothly — no jump at the end:
//! the latches draw the leaf in and lock it — and their leaves stay in one piece with the hull.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World};
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

const TICK: f64 = lunar_ship::ship::TICK;

fn world() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), ..World::default() }
}

fn press(ship: &mut Ship, s: &mut Structure, id: &str) {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no control {id}"));
    let kind = ship.kind.clone();
    ship.panels.intent(k, &lunar_controls::Intent::Press { elem: 0 }, s, &kind, &ship.store);
    for _ in 0..5 {
        ship.update(s, &world(), TICK);
    }
    ship.panels.intent(k, &lunar_controls::Intent::Release, s, &kind, &ship.store);
}

/// The way joint `id` moves for `secs`: (largest step in one tick, fastest in the last
/// `near` of travel, latched at the end).
fn watch(ship: &mut Ship, s: &mut Structure, id: &str, closure: &str, secs: f64, near: f64) -> (f64, f64, bool) {
    let k = ship.kind.joints.iter().position(|j| j.id == id).unwrap();
    let (mut jump, mut last_speed) = (0.0f64, 0.0f64);
    let mut q = ship.joints[k].q;
    for _ in 0..(secs / TICK) as usize {
        ship.update(s, &world(), TICK);
        let now = ship.joints[k].q;
        let step = (now - q).abs();
        jump = jump.max(step);
        if now.abs() < near {
            last_speed = last_speed.max(step / TICK);
        }
        q = now;
    }
    (jump, last_speed, ship.signal(&format!("{closure}.cerrada")).unwrap() > 0.5)
}

#[test]
fn the_ramp_shuts_without_a_jump() {
    let (mut s, mut ship) = spawn();
    for _ in 0..50 {
        ship.update(&mut s, &world(), TICK);
    }
    press(&mut ship, &mut s, "techo/hpu_a");
    for _ in 0..(6.0 / TICK) as usize {
        ship.update(&mut s, &world(), TICK);
    }
    press(&mut ship, &mut s, "rampa_bodega/rampa");
    let (jump, end, latched) = watch(&mut ship, &mut s, "rampa", "rampa", 30.0, 3f64.to_radians());
    eprintln!("ramp: largest step {:.2}° in a tick, last 3° at {:.2}°/s, latched {latched}", jump.to_degrees(), end.to_degrees());
    for e in ship.blackbox.entries.iter().filter(|e| e.text.contains("ampa")) {
        eprintln!("  t {:.1}: {}", e.t, e.text);
    }
    assert!(latched, "it latched shut");
    assert!(jump < 1.5f64.to_radians(), "no jump: {:.2}° in one tick", jump.to_degrees());
    assert!(end < 20f64.to_radians(), "it seats gently: {:.1}°/s over the last 3°", end.to_degrees());
    // and opens again
    press(&mut ship, &mut s, "rampa_bodega/rampa");
    for _ in 0..(30.0 / TICK) as usize {
        ship.update(&mut s, &world(), TICK);
    }
    assert!(ship.signal("rampa.abierta").unwrap() > 0.9, "open again: {}", ship.signal("rampa.abierta").unwrap());
}

#[test]
fn the_doors_open_and_shut_smoothly() {
    let (mut s, mut ship) = spawn();
    for _ in 0..50 {
        ship.update(&mut s, &world(), TICK);
    }
    // the bridge door: the same pressure on both sides
    press(&mut ship, &mut s, "puerta_puente_c/abrir");
    for _ in 0..(8.0 / TICK) as usize {
        ship.update(&mut s, &world(), TICK);
    }
    assert!(ship.signal("puerta_puente.abierta").unwrap() > 0.9, "it opened");
    press(&mut ship, &mut s, "puerta_puente_p/abrir");
    let (jump, end, latched) = watch(&mut ship, &mut s, "puerta_puente", "puerta_puente", 10.0, 0.03);
    eprintln!("bridge door: largest step {:.1} mm, last 3 cm at {:.0} mm/s, latched {latched}", jump * 1000.0, end * 1000.0);
    assert!(latched);
    assert!(jump < 0.03, "no jump");
}

#[test]
fn closure_leaves_seal_their_openings() {
    let (_, ship) = spawn();
    for c in &ship.kind.closures {
        let area: f32 = c.joints.iter().filter_map(|&j| ship.kind.joints[j].opening.map(|o| o.area)).sum();
        eprintln!("{}: {} leaves, {} parts, {area:.2} m² of opening", c.id, c.joints.len(), c.parts.len());
        assert!(!c.parts.is_empty(), "{} has its leaves", c.id);
        for &j in &c.joints {
            let o = ship.kind.joints[j].opening.unwrap_or_else(|| panic!("{}: its joint opens nothing", c.id));
            assert!(o.area > 0.1, "{}: an opening of {} m²", c.id, o.area);
            assert!((o.n.length() - 1.0).abs() < 1e-3);
        }
    }
}
