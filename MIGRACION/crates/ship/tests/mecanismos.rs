//! Mechanisms stop at what is in their way (`Ship::stop_at_obstacles`): a crane's hoist comes
//! down as far as the load under it and its magnet takes what it touches; a ramp lies on the
//! ground and holds nothing up; a ramp does not shut on what stands in its doorway; a door does
//! not shut on whoever stands in it, and shuts once they are gone.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{
        Library,
        obstruct::{What, depth_now},
        set::Structures,
        state::Structure,
    },
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, ship::TICK};
use std::{path::Path, sync::Arc};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn level(bodies: &BodyRegistry) -> DVec3 {
    let b = bodies.get(0);
    let mut best = (f64::MAX, DVec3::Y);
    let mut seed = 4242u64;
    for _ in 0..400 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = |k: u32| ((seed >> (11 + 17 * k)) & 0xffff) as f64 / 65535.0 * 2.0 - 1.0;
        let dir = DVec3::new(r(0), r(1), r(2)).normalize_or(DVec3::Y);
        let side = dir.any_orthonormal_vector();
        let fwd = dir.cross(side);
        let at = |x: f64, z: f64| b.height((dir * b.radius + side * x + fwd * z).normalize());
        let hs = [at(0.0, 0.0), at(24.0, 0.0), at(-24.0, 0.0), at(0.0, 24.0), at(0.0, -24.0), at(16.0, 16.0), at(-16.0, -16.0), at(16.0, -16.0), at(-16.0, 16.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

/// The Moon, and a ship of kind `id` standing on level ground on its legs.
fn world(kind: &str) -> (Structures, BodyRegistry, Ship, u64, Arc<ShipKind>) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs().join("bodies/luna.jsonc")).unwrap()).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let kind = ships.get(kind).unwrap().clone();
    let mut set = Structures::new(Arc::new(lib));
    let id = set.place(&kind.blueprint, &bodies, 0, level(&bodies), 0.0, f64::from(kind.lift)).unwrap();
    let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut set.list[0], &w(), 0.0);
    set.rest_on_ground(id, &bodies);
    let up = bodies.get(0).up(set.list[0].pos);
    let sink = ship.rest_on_legs(&mut set.list[0], 1.62);
    set.list[0].pos -= up * f64::from(sink);
    (set, bodies, ship, id, kind)
}

fn ship_of(set: &mut Structures, id: u64) -> &mut Structure {
    set.list.iter_mut().find(|s| s.id == id).unwrap()
}

/// The ship and the world on for `secs`: its systems, what its clamps ask, what is in the way
/// of what moves (`people` too), the bodies.
fn live(set: &mut Structures, bodies: &BodyRegistry, ship: &mut Ship, id: u64, secs: f64, people: &[(DVec3, f32)]) {
    let per = (1.0 / 60.0 / TICK).round().max(1.0) as usize;
    for _ in 0..(secs * 60.0) as usize {
        for _ in 0..per {
            ship.update(ship_of(set, id), &w(), TICK);
        }
        set.separate();
        lunar_ship::cargo::serve(ship, set);
        ship.stop_at_obstacles(set, bodies, people);
        set.step(1.0 / 60.0, bodies);
    }
}

fn control(ship: &Ship, id: &str) -> usize {
    ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"))
}

fn press(set: &mut Structures, bodies: &BodyRegistry, ship: &mut Ship, id: u64, what: &str) {
    let (k, kind) = (control(ship, what), ship.kind.clone());
    ship.panels.intent(k, &Intent::Press { elem: 0 }, set.get(id).unwrap(), &kind, &ship.store);
    live(set, bodies, ship, id, 0.25, &[]);
    ship.panels.intent(k, &Intent::Release, set.get(id).unwrap(), &kind, &ship.store);
    live(set, bodies, ship, id, 0.1, &[]);
}

fn lever(set: &Structures, ship: &mut Ship, id: u64, what: &str, value: f64) {
    let (k, kind) = (control(ship, what), ship.kind.clone());
    ship.panels.intent(k, &Intent::Set { value }, set.get(id).unwrap(), &kind, &ship.store);
}

/// The parts of `kind` whose id starts with `name`.
fn parts(kind: &ShipKind, name: &str) -> Vec<u32> {
    kind.parts.iter().enumerate().filter(|(_, n)| n.starts_with(name)).map(|(i, _)| i as u32).collect()
}

#[test]
fn a_hoist_stops_on_the_load_and_its_magnet_takes_what_it_touches() {
    let (mut set, bodies, mut ship, id, kind) = world("cachalote");
    live(&mut set, &bodies, &mut ship, id, 2.0, &[]);
    let structures = set.list.len();
    ship.set_signal("anclaje_b1.soltar", 1.0);
    live(&mut set, &bodies, &mut ship, id, 1.0, &[]);
    assert_eq!(set.list.len(), structures + 1, "el palé no se suelta de su anclaje");
    let pallet = set.list.last().unwrap().id;
    // over it, and ALL the way down: it comes down on the pallet and stays there
    lever(&set, &mut ship, id, "grua/puente", 0.0774);
    live(&mut set, &bodies, &mut ship, id, 8.0, &[]);
    lever(&set, &mut ship, id, "grua/gancho", 1.0);
    live(&mut set, &bodies, &mut ship, id, 14.0, &[]);
    let hoist = ship.signal("grua_gancho.pos").unwrap();
    eprintln!("izado al {:.2}; bloqueada {:?}", hoist, ship.signal("grua_gancho.bloqueada"));
    assert!(hoist > 0.2 && hoist < 0.9, "el izado baja hasta {hoist:.2} de su recorrido: no se para en el palé");
    assert_eq!(ship.signal("grua_gancho.bloqueada"), Some(1.0));
    let k = set.index_of(id).unwrap();
    let magnet = parts(&kind, "grua_iman");
    let deep = depth_now(&set, k, &magnet, &bodies, What { ground: false, others: true, own: &[], plane: None });
    assert!(deep < 0.03, "el imán está {deep:.3} m dentro del palé");
    // the pallet did not move under it
    let p = set.get(pallet).unwrap();
    assert!(p.vel.length() < 0.05, "el palé se mueve a {:.2} m/s", p.vel.length());
    // the magnet takes what it touches; up it goes with it
    press(&mut set, &bodies, &mut ship, id, "grua/tapa_iman");
    press(&mut set, &bodies, &mut ship, id, "grua/agarre");
    live(&mut set, &bodies, &mut ship, id, 1.5, &[]);
    assert_eq!(ship.signal("grua_iman.sujeta"), Some(1.0), "el imán no toma el palé sobre el que está");
    let before = set.get(id).unwrap().to_local(set.get(pallet).unwrap().to_world(set.get(pallet).unwrap().center));
    lever(&set, &mut ship, id, "grua/gancho", 0.1);
    live(&mut set, &bodies, &mut ship, id, 8.0, &[]);
    let after = set.get(id).unwrap().to_local(set.get(pallet).unwrap().to_world(set.get(pallet).unwrap().center));
    assert!(after.y > before.y + 0.4, "no lo levanta: {:.2} m", after.y - before.y);
    assert_eq!(ship.signal("grua_gancho.bloqueada"), Some(0.0), "sigue diciendo que está bloqueada");
    // set down again on the deck: the load stops on the deck, the hoist with it
    lever(&set, &mut ship, id, "grua/gancho", 1.0);
    live(&mut set, &bodies, &mut ship, id, 14.0, &[]);
    let down = set.get(id).unwrap().to_local(set.get(pallet).unwrap().to_world(set.get(pallet).unwrap().center));
    eprintln!("palé: {:.2} -> {:.2} -> {:.2}", before.y, after.y, down.y);
    assert!((down.y - before.y).abs() < 0.08, "bajado de nuevo queda a {:.2} m de donde estaba", down.y - before.y);
}

#[test]
fn a_ramp_lies_on_the_ground_and_holds_nothing_up() {
    for name in ["alcotan", "cachalote"] {
        let (mut set, bodies, mut ship, id, kind) = world(name);
        live(&mut set, &bodies, &mut ship, id, 6.0, &[]);
        let k = set.index_of(id).unwrap();
        let s = &set.list[k];
        assert_eq!(ship.signal("rampa.cerrada"), Some(0.0), "{name}: la rampa empieza abierta");
        // the hull is carried by its legs alone, all pressed in alike
        let rest: Vec<f32> = s.springs.iter().map(|sp| sp.x / sp.stroke).collect();
        let carried: f32 = s.springs.iter().map(|sp| sp.load).sum();
        eprintln!("{name}: patas a {rest:.2?}, llevan {carried:.0} N de {:.0}", s.mass * 1.62);
        assert!(rest.iter().all(|x| (0.55..0.95).contains(x)), "{name}: patas a {rest:?}");
        let (lo, hi) = rest.iter().fold((f32::MAX, f32::MIN), |(l, h), &x| (l.min(x), h.max(x)));
        assert!(hi - lo < 0.25, "{name}: unas patas llevan más que otras: {rest:?}");
        // its ramp: on the ground, not in it and not over it
        let ramp = parts(&kind, "rampa");
        assert!(!ramp.is_empty());
        let b = bodies.get(0);
        let low = ramp.iter().flat_map(|&p| s.parts[p as usize].shape.verts().map(move |v| s.to_world(s.parts[p as usize].local.transform_point3(v)))).map(|w| (w - b.center).length() - (b.radius + b.height(b.up(w)))).fold(f64::MAX, f64::min);
        eprintln!("{name}: la rampa a {low:+.3} m del suelo; rampa.pos {:?}", ship.signal("rampa.pos"));
        assert!(low > -0.03, "{name}: la rampa está {:.3} m dentro del suelo", -low);
        assert!(low < 0.12, "{name}: la rampa se queda a {low:.3} m del suelo");
    }
}

#[test]
fn a_door_does_not_shut_on_whoever_stands_in_it() {
    let (mut set, bodies, mut ship, id, kind) = world("alcotan");
    live(&mut set, &bodies, &mut ship, id, 1.0, &[]);
    // the bridge door, opened (the hold is open to space: its own would not open)
    let was = ship.signal("puerta_puente.pos").unwrap();
    press(&mut set, &bodies, &mut ship, id, "puerta_puente_c/abrir");
    live(&mut set, &bodies, &mut ship, id, 4.0, &[]);
    let open = ship.signal("puerta_puente.pos").unwrap();
    assert!(open > 0.9 && was < 0.1, "la puerta no se abre: {was:.2} -> {open:.2}");
    // someone in the doorway (three spheres up from the deck); ordered shut
    let s = set.get(id).unwrap();
    let door = kind.joints.iter().position(|j| j.id == "puerta_puente").unwrap();
    let centre = kind.joints[door].parts.iter().map(|&p| s.parts[p as usize].rest.translation).fold(Vec3::ZERO, |a, t| a + Vec3::from(t)) / kind.joints[door].parts.len() as f32;
    let person: Vec<(DVec3, f32)> = [0.3f32, 0.85, 1.45].iter().map(|h| (s.to_world(Vec3::new(0.0, *h, centre.z)), 0.3)).collect();
    let (k, kd) = (control(&ship, "puerta_puente_c/abrir"), ship.kind.clone());
    ship.panels.intent(k, &Intent::Press { elem: 0 }, set.get(id).unwrap(), &kd, &ship.store);
    live(&mut set, &bodies, &mut ship, id, 0.25, &person);
    ship.panels.intent(k, &Intent::Release, set.get(id).unwrap(), &kd, &ship.store);
    live(&mut set, &bodies, &mut ship, id, 5.0, &person);
    let held = ship.signal("puerta_puente.pos").unwrap();
    eprintln!("con alguien en el hueco: puerta al {held:.2}, bloqueada {:?}", ship.signal("puerta_puente.bloqueada"));
    assert!(held > 0.2, "la puerta se cierra sobre quien está en el hueco ({held:.2})");
    assert_eq!(ship.signal("puerta_puente.bloqueada"), Some(1.0));
    // gone: it shuts
    live(&mut set, &bodies, &mut ship, id, 5.0, &[]);
    let shut = ship.signal("puerta_puente.pos").unwrap();
    assert!(shut < 0.02, "sin nadie, la puerta no acaba de cerrarse ({shut:.2})");
    assert_eq!(ship.signal("puerta_puente.bloqueada"), Some(0.0));
}

#[test]
fn a_magnet_over_cargo_still_in_its_clamp_says_so_and_takes_it_once_let_go() {
    let (mut set, bodies, mut ship, id, _) = world("cachalote");
    live(&mut set, &bodies, &mut ship, id, 2.0, &[]);
    // over the pallet of bay B1, down on it, the magnet on: the pallet is still in its clamp
    lever(&set, &mut ship, id, "grua/puente", 0.0774);
    live(&mut set, &bodies, &mut ship, id, 6.0, &[]);
    lever(&set, &mut ship, id, "grua/gancho", 1.0);
    live(&mut set, &bodies, &mut ship, id, 12.0, &[]);
    press(&mut set, &bodies, &mut ship, id, "grua/tapa_iman");
    press(&mut set, &bodies, &mut ship, id, "grua/agarre");
    live(&mut set, &bodies, &mut ship, id, 1.5, &[]);
    assert_eq!(ship.signal("grua_iman.activo"), Some(1.0));
    assert_eq!(ship.signal("grua_iman.sujeta"), Some(0.0), "se lleva una carga que sigue anclada");
    assert_eq!(ship.signal("grua_iman.anclada"), Some(1.0), "no dice que la carga sigue en su anclaje");
    let said: Vec<String> = ship.said.iter().map(|s| s.1.clone()).collect();
    eprintln!("dice: {said:?}");
    assert!(said.iter().any(|t| t.contains("ANCLAJE_B1")), "no se lo dice a quien la maneja: {said:?}");
    // the clamp let go: the magnet, still on, takes the pallet by itself
    ship.set_signal("anclaje_b1.soltar", 1.0);
    live(&mut set, &bodies, &mut ship, id, 2.0, &[]);
    assert_eq!(ship.signal("grua_iman.sujeta"), Some(1.0), "soltado el anclaje, el imán no toma el palé");
    assert_eq!(ship.signal("grua_iman.anclada"), Some(0.0));
}
