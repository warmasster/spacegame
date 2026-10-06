//! Cargo and its clamps on the Alcotán (`lunar_ship::cargo`):
//! - every piece of cargo holds to its clamp and to nothing else;
//! - a clamp let go (its order, a hand) frees what it held whole, as a body of its own that falls
//!   to the deck and stays in the hold; nothing of it is then missing from the ship;
//! - a clamp destroyed frees it too;
//! - loose cargo rides a ship that lifts off (it does not hold the ship down, nor fall through);
//! - what bursts when destroyed (a drum of propellant) says so.
use glam::{DVec3, Vec3};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{
        Library,
        breakup::{Event, Rules},
        set::Structures,
        state::Structure,
    },
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, ship::TICK};
use std::{path::Path, sync::Arc};

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// An Alcotán standing on the Moon, its systems started.
fn world() -> (Structures, BodyRegistry, Ship, u64, Arc<ShipKind>) {
    let defs = assets().join("defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs.join("bodies/luna.jsonc")).unwrap()).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let mut set = Structures::new(Arc::new(lib));
    let id = set.place("alcotan", &bodies, 0, level_ground(&bodies), 0.0, f64::from(kind.lift)).unwrap();
    let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
    // on its gear as it stands (its joints posed), as the game sets a ship down; and still
    ship.update(ship_of(&mut set, id), &w(), 0.0);
    set.rest_on_ground(id, &bodies);
    for _ in 0..600 {
        set.step(1.0 / 60.0, &bodies);
        if set.get(id).unwrap().resting {
            break;
        }
    }
    assert!(set.get(id).unwrap().resting, "la nave no se queda quieta donde se posa");
    (set, bodies, ship, id, kind)
}

/// A level place to stand a ship on: of a few hundred spots, the one whose ground changes least
/// over a ship's length.
fn level_ground(bodies: &BodyRegistry) -> DVec3 {
    let b = bodies.get(0);
    let mut best = (f64::MAX, DVec3::Y);
    let mut seed = 12345u64;
    for _ in 0..400 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = |k: u32| ((seed >> (11 + 17 * k)) & 0xffff) as f64 / 65535.0 * 2.0 - 1.0;
        let dir = DVec3::new(r(0), r(1), r(2)).normalize_or(DVec3::Y);
        let side = dir.any_orthonormal_vector();
        let fwd = dir.cross(side);
        let at = |x: f64, z: f64| b.height((dir * b.radius + side * x + fwd * z).normalize());
        let hs = [at(0.0, 0.0), at(12.0, 0.0), at(-12.0, 0.0), at(0.0, 12.0), at(0.0, -12.0), at(8.0, 8.0), at(-8.0, -8.0), at(8.0, -8.0), at(-8.0, 8.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn ship_of(set: &mut Structures, id: u64) -> &mut Structure {
    set.list.iter_mut().find(|s| s.id == id).unwrap()
}

fn clamp(kind: &ShipKind, id: &str) -> usize {
    kind.clamps.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay anclaje {id}"))
}

/// Order clamp `id` to let go, and take what comes off the ship.
fn let_go(set: &mut Structures, ship: &mut Ship, id: u64, clamp: &str) -> usize {
    ship.set_signal(&format!("{clamp}.soltar"), 1.0);
    ship.update(ship_of(set, id), &w(), TICK);
    set.separate()
}

fn settle(set: &mut Structures, bodies: &BodyRegistry, secs: f32) {
    for _ in 0..(secs * 60.0) as usize {
        set.step(1.0 / 60.0, bodies);
    }
}

/// Where structure `k` is in the ship's frame (its centre).
fn aboard(set: &Structures, id: u64, k: usize) -> Vec3 {
    let ship = set.get(id).unwrap();
    let c = &set.list[k];
    ship.to_local(c.to_world(c.center))
}

#[test]
fn cargo_holds_to_its_clamp_and_to_nothing_else() {
    let (set, _, _, id, kind) = world();
    let s = set.get(id).unwrap();
    assert_eq!(kind.clamps.len(), 4);
    let all: Vec<u32> = kind.clamps.iter().flat_map(|c| c.cargo.iter().copied()).collect();
    assert!(all.len() >= 20, "{} piezas de carga", all.len());
    for c in &kind.clamps {
        eprintln!("{} ({}): sujeta {:?}, {} piezas, {} amarres", c.id, c.name, c.holds, c.cargo.len(), lunar_ship::cargo::lashings(c, s).len());
        assert!(!c.holds.is_empty() && !c.parts.is_empty());
        assert!(!lunar_ship::cargo::lashings(c, s).is_empty(), "{}: nada lo une a su carga", c.id);
    }
    // every joint of a cargo part is to cargo of the same clamp or to its clamp
    for j in s.joints.iter() {
        for (x, y) in [(j.a, j.b), (j.b, j.a)] {
            if !all.contains(&x) {
                continue;
            }
            let c = kind.clamps.iter().find(|c| c.cargo.contains(&x)).unwrap();
            assert!(c.cargo.contains(&y) || c.parts.contains(&y), "{} está unida a {}", kind.parts[x as usize], kind.parts[y as usize]);
        }
    }
    // the stacked crate, which touches no clamp, is lashed to its own all the same
    let top = kind.parts.iter().position(|p| p == "carga_caja_4").unwrap() as u32;
    let cajas = &kind.clamps[clamp(&kind, "anclaje_cajas")];
    assert!(cajas.cargo.contains(&top) && cajas.holds.len() == 4);
    // a clamp is of the ship: a hand finds it by its parts
    assert!(cajas.parts.iter().any(|p| kind.parts[*p as usize].ends_with("palanca")));
}

#[test]
fn a_clamp_let_go_frees_its_cargo_whole_and_it_stays_in_the_hold() {
    let (mut set, bodies, mut ship, id, kind) = world();
    let plan = kind.clamps[clamp(&kind, "anclaje_pale")].clone();
    let mass = set.get(id).unwrap().mass;
    assert_eq!(ship.signal("anclaje_pale.sujeta"), Some(1.0));
    assert_eq!(let_go(&mut set, &mut ship, id, "anclaje_pale"), 1, "el palé sale entero, como una sola pieza suelta");
    let k = set.list.len() - 1;
    assert_eq!(set.list[k].parts.len(), plan.cargo.len());
    assert!(set.list[k].mass > 100.0 && (set.get(id).unwrap().mass + set.list[k].mass - mass).abs() < 1.0, "la masa del palé ya no es de la nave");
    // the ship: its clamp holds nothing now, the others do; nothing is missing from it
    ship.update(ship_of(&mut set, id), &w(), TICK);
    assert_eq!(ship.signal("anclaje_pale.sujeta"), Some(0.0));
    assert_eq!(ship.signal("anclaje_cajas.sujeta"), Some(1.0));
    let s = set.get(id).unwrap();
    assert!(plan.cargo.iter().all(|&p| !s.parts[p as usize].alive && s.parts[p as usize].left));
    assert!(plan.parts.iter().all(|&p| s.parts[p as usize].alive), "el anclaje sigue en la nave");
    let at = kind.centers[plan.cargo[0] as usize];
    assert!(s.raycast_gone(at + Vec3::new(0.0, 2.0, 0.0), Vec3::NEG_Y, 3.0).is_none(), "el escáner da el palé por perdido");
    let mut words = Vec::new();
    lunar_core::structure::look::part_states(s, s.parts.len(), &mut words);
    assert!(plan.cargo.iter().all(|&p| words[p as usize] == lunar_core::structure::look::AWAY));
    // again does nothing more
    assert_eq!(let_go(&mut set, &mut ship, id, "anclaje_pale"), 0);
    // loose, it falls the little it can and rests on the deck, in the hold
    let before = aboard(&set, id, k);
    settle(&mut set, &bodies, 4.0);
    let after = aboard(&set, id, k);
    eprintln!("palé suelto: de {before:.2?} a {after:.2?}, a {:.2} m/s", set.list[k].vel.length());
    assert!(after.distance(before) < 0.25, "se va de su sitio: {after:.2?}");
    assert!(after.y > 0.2 && after.y < 0.6, "atraviesa el suelo o flota: y = {:.2}", after.y);
    assert!(set.list[k].vel.length() < 0.2);
}

#[test]
fn a_clamp_destroyed_frees_its_cargo() {
    let (mut set, _, mut ship, id, kind) = world();
    let plan = kind.clamps[clamp(&kind, "anclaje_agua")].clone();
    let rules = Rules::standard(&set.lib.catalog).unwrap();
    let mut events = Vec::new();
    for (n, &p) in plan.parts.iter().enumerate() {
        set.blow_out(id, p, Vec3::Y, 1.0e3, &rules, &mut events, n as u64);
    }
    let before = set.list.len();
    set.separate();
    // the two drums, each a body of its own (they held to the clamp, not to each other)
    let drums: Vec<&Structure> = set.list[before..].iter().filter(|s| s.parts.len() >= 3).collect();
    assert_eq!(drums.len(), 2, "{} piezas sueltas de {} piezas", set.list.len() - before, plan.cargo.len());
    ship.update(ship_of(&mut set, id), &w(), TICK);
    assert_eq!(ship.signal("anclaje_agua.sujeta"), Some(0.0));
    assert_eq!(ship.signal("anclaje_prop.sujeta"), Some(1.0));
}

#[test]
fn loose_cargo_rides_a_ship_that_lifts_off() {
    let (mut set, bodies, mut ship, id, _) = world();
    assert_eq!(let_go(&mut set, &mut ship, id, "anclaje_pale"), 1);
    let k = set.list.len() - 1;
    settle(&mut set, &bodies, 3.0);
    let (at0, h0) = (aboard(&set, id, k), set.get(id).unwrap().pos);
    // the ship pushed up at 1.5 m/s² over gravity for four seconds
    let up = bodies.get(0).up(h0);
    let g = bodies.field(h0).g() as f32;
    for _ in 0..240 {
        let s = ship_of(&mut set, id);
        s.force = up.as_vec3() * s.mass * (g + 1.5);
        set.step(1.0 / 60.0, &bodies);
    }
    let rose = (set.get(id).unwrap().pos - h0).dot(up);
    let at = aboard(&set, id, k);
    eprintln!("la nave sube {rose:.1} m; el palé, de {at0:.2?} a {at:.2?} a bordo");
    assert!(rose > 8.0, "la carga suelta retiene la nave: sube {rose:.1} m");
    assert!(at.distance(at0) < 0.5 && at.y > 0.15, "el palé no va con la nave: {at:.2?}");
}

#[test]
fn what_bursts_says_so_when_it_is_destroyed() {
    let (mut set, _, _, id, kind) = world();
    let rules = Rules::standard(&set.lib.catalog).unwrap();
    let part = |name: &str| kind.parts.iter().position(|p| p == name).unwrap() as u32;
    let mut events = Vec::new();
    set.blow_out(id, part("carga_agua_1"), Vec3::Y, 1.0e3, &rules, &mut events, 1);
    assert!(!events.iter().any(|e| matches!(e, Event::Burst { .. })), "el agua no estalla");
    set.blow_out(id, part("carga_prop"), Vec3::Y, 1.0e3, &rules, &mut events, 2);
    let burst = events.iter().find_map(|e| if let Event::Burst { kind, .. } = e { Some(*kind) } else { None }).expect("el bidón de propelente estalla");
    let b = set.lib.catalog.parts[usize::from(burst)].def.burst.clone().unwrap();
    assert!(b.energy > 1.0e5 && b.radius > 2.0 && !b.effect.is_empty());
    // once: its pieces do not burst again
    events.clear();
    set.blow_out(id, part("carga_prop"), Vec3::Y, 1.0e3, &rules, &mut events, 3);
    assert!(!events.iter().any(|e| matches!(e, Event::Burst { .. })));
}

#[test]
fn a_clamp_shut_takes_back_what_is_loose_on_it() {
    let (mut set, bodies, mut ship, id, kind) = world();
    let c = clamp(&kind, "anclaje_cajas");
    let mass = set.get(id).unwrap().mass;
    assert_eq!(let_go(&mut set, &mut ship, id, "anclaje_cajas"), 4, "cuatro cajas, cada una suelta por su lado");
    settle(&mut set, &bodies, 3.0);
    let loose: Vec<u64> = set.list.iter().filter(|s| s.id != id).map(|s| s.id).collect();
    eprintln!("cajas sueltas: {:?} kg", set.list.iter().filter(|s| s.id != id).map(|s| s.mass.round()).collect::<Vec<_>>());
    assert!(set.get(id).unwrap().mass < mass - 50.0);
    // its lever again: shut, it takes what is loose in its zone
    assert!(!ship.clamp_holds(set.get(id).unwrap(), c));
    ship.work_clamp(c, false);
    lunar_ship::cargo::serve(&mut ship, &mut set);
    ship.update(ship_of(&mut set, id), &w(), TICK);
    assert_eq!(ship.clamp_held[c].len(), 4, "no las ha tomado todas: {:?}", ship.clamp_held[c]);
    assert_eq!(ship.signal("anclaje_cajas.sujeta"), Some(1.0));
    assert!(loose.iter().all(|l| set.get(*l).unwrap().held.is_some_and(|h| h.by == id)));
    assert!((set.get(id).unwrap().mass - mass).abs() < 1.0, "la nave vuelve a cargar con su peso");
    // each where it lay, set down on the deck; the one on top still on top
    let ys: Vec<f32> = loose.iter().map(|l| aboard(&set, id, set.index_of(*l).unwrap()).y).collect();
    assert!(ys.iter().filter(|y| **y < 0.4).count() == 3 && ys.iter().filter(|y| **y > 0.6).count() == 1, "alturas {ys:.2?}");
    // held, they go with the ship wherever it goes, at once
    let before: Vec<Vec3> = loose.iter().map(|l| aboard(&set, id, set.index_of(*l).unwrap())).collect();
    let up = bodies.get(0).up(set.get(id).unwrap().pos);
    for _ in 0..180 {
        let s = ship_of(&mut set, id);
        s.force = up.as_vec3() * s.mass * 4.0;
        set.step(1.0 / 60.0, &bodies);
    }
    for (l, b) in loose.iter().zip(&before) {
        assert!(aboard(&set, id, set.index_of(*l).unwrap()).distance(*b) < 1e-3, "una caja anclada se mueve a bordo");
    }
    // opened again in flight, they are let go with the ship's speed
    let v = set.get(id).unwrap().vel;
    assert!(v.length() > 3.0);
    ship.work_clamp(c, true);
    ship.update(ship_of(&mut set, id), &w(), TICK);
    lunar_ship::cargo::serve(&mut ship, &mut set);
    assert!(ship.clamp_held[c].is_empty() && ship.signal("anclaje_cajas.sujeta") == Some(1.0) || ship.clamp_held[c].is_empty());
    for l in &loose {
        let s = set.get(*l).unwrap();
        assert!(s.held.is_none() && (s.vel - v).length() < 0.5, "suelta sin la velocidad de la nave: {:.1?}", s.vel);
    }
}

#[test]
fn loose_cargo_does_not_go_through_the_deck_when_the_ship_stops_dead() {
    // as if the ship had been falling at 3 m/s and stopped: the pallet keeps going, into the deck
    let (mut set, bodies, mut ship, id, _) = world();
    assert_eq!(let_go(&mut set, &mut ship, id, "anclaje_pale"), 1);
    let k = set.list.len() - 1;
    settle(&mut set, &bodies, 1.0);
    let at = aboard(&set, id, k);
    let up = bodies.get(0).up(set.get(id).unwrap().pos);
    for speed in [1.0, 3.0, 6.0] {
        set.list[k].vel = -up * speed;
        set.list[k].resting = false;
        settle(&mut set, &bodies, 3.0);
        let now = aboard(&set, id, k);
        eprintln!("a {speed} m/s contra la cubierta: queda en y = {:.3} (estaba en {:.3})", now.y, at.y);
        assert!(now.y > at.y - 0.05, "a {speed} m/s atraviesa la cubierta: y = {:.2}", now.y);
    }
}

#[test]
fn the_hold_has_a_walkway_beside_the_cargo() {
    // from the cabin door to the ramp along the starboard side: a body 0.6 m wide passes
    // upright without touching anything but the deck (no jumping over cargo or clamps)
    let (set, _, _, id, _) = world();
    let ship = set.get(id).unwrap();
    let up = (ship.rot * Vec3::Y).as_dvec3();
    let mut out = Vec::new();
    let mut blocked = Vec::new();
    let mut z = -3.9f32;
    while z > -9.2 {
        for y in [0.36, 0.85, 1.45] {
            set.sphere_contacts(ship.to_world(Vec3::new(0.8, y, z)), 0.3, 0.02, &mut out);
            if out.iter().any(|c| c.normal.dot(up) < 0.6) {
                blocked.push((z, y));
            }
        }
        z -= 0.1;
    }
    assert!(blocked.is_empty(), "el pasillo de estribor está cortado en (z, y): {blocked:.1?}");
}
