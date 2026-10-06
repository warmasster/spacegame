//! Damage and fracture on the real builds: blasts wreck glass and walls (by default into their
//! break effect, with nothing cut; with "planos", into pieces that fly, land and settle);
//! projectiles cut through parts until their energy runs out; the same hit always breaks the
//! same way.
use glam::{DVec3, Vec3};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    structure::{
        Library,
        breakup::{Event, Rules},
        set::Structures,
    },
};
use std::{path::Path, sync::Arc};

fn world() -> (BodyRegistry, Structures, Rules) {
    let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let rules = Rules::standard(&lib.catalog).unwrap();
    (bodies, Structures::new(lib), rules)
}

fn total_mass(set: &Structures) -> f32 {
    set.list.iter().map(|s| s.mass).sum()
}

/// World point of a part of structure 0 by its blueprint name.
fn part_at(set: &Structures, build: &str, name: &str) -> (usize, DVec3) {
    let bp = set.lib.blueprint(build).unwrap();
    let i = bp.ids.iter().position(|n| n == name).unwrap();
    let s = &set.list[0];
    (i, s.to_world(s.parts[i].center))
}

#[test]
fn a_grenade_shatters_the_window_next_to_it() {
    let (bodies, mut set, rules) = world();
    set.place("estacion", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let (i, w) = part_at(&set, "estacion", "ventana_a");
    let s = &set.list[0];
    let out = (s.rot * Vec3::Z).as_dvec3();
    let mut events = Vec::new();
    set.blast(w + out * 1.5, 1.5e6, 8.0, &rules, &mut events, 7);
    assert!(!set.list[0].parts[i].alive, "the window holds");
    assert!(events.iter().any(|e| matches!(e, Event::Shattered { .. })));
    // gone into its break effect: nothing cut, no loose shards to simulate
    let shards = set.list.iter().filter(|s| s.parts.len() == 1 && s.parts[0].fragment).count();
    assert_eq!(shards, 0, "{shards} shards");
    // the habitat behind it is scratched, not wrecked
    let hab = set.lib.blueprint("estacion").unwrap().ids.iter().position(|n| n == "hab_a").unwrap();
    let p = &set.list[0].parts[hab];
    assert!(p.alive && p.damage() < 0.5, "habitat damage {}", p.damage());
}

#[test]
fn a_bomb_wrecks_parts_into_particles_without_loose_shards() {
    let (bodies, mut set, rules) = world();
    set.place("torre", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let before = total_mass(&set);
    let corner = set.list[0].to_world(Vec3::new(3.6, 1.5, 3.6));
    let mut events = Vec::new();
    set.blast(corner, 2.0e8, 45.0, &rules, &mut events, 11);
    let wrecked = events.iter().filter(|e| matches!(e, Event::Shattered { .. })).count();
    let shards = set.list.iter().filter(|s| s.parts.iter().any(|p| p.fragment)).count();
    println!("bomb (particulas): {wrecked} parts wrecked, {} structures, mass {before:.0} -> {:.0} kg", set.list.len(), total_mass(&set));
    assert!(wrecked >= 3);
    assert_eq!(shards, 0);
    assert!(total_mass(&set) < before, "the wrecked parts are gone");
}

/// The rigid bodies with many pieces flying ("planos"; the default turns wrecked parts into
/// particles, so only whole groups that come off fly).
#[test]
fn a_bomb_brings_part_of_the_tower_down_and_everything_settles() {
    let (bodies, mut set, _) = world();
    let rules = Rules::with_fracture(&set.lib.catalog, "planos").unwrap();
    set.place("torre", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let before = total_mass(&set);
    let s = &set.list[0];
    let corner = s.to_world(Vec3::new(3.6, 1.5, 3.6));
    let mut events = Vec::new();
    set.blast(corner, 2.0e8, 45.0, &rules, &mut events, 11);
    let after = total_mass(&set);
    let free = set.list.iter().filter(|s| !s.anchored).count();
    let wrecked = events.iter().filter(|e| matches!(e, Event::Shattered { .. })).count();
    println!("bomb: {wrecked} parts wrecked, {} structures, {free} loose, mass {before:.0} -> {after:.0} kg", set.list.len());
    assert!(wrecked >= 3);
    assert!(free >= 10);
    assert!(after <= before * 1.0001 && after > before * 0.5, "mass {before} -> {after}");
    // everything loose falls, lands and stops, on the ground and not under it
    // pieces thrown at 45 m/s are a minute in the air on the Moon
    for _ in 0..(90 * 60) {
        set.step(1.0 / 60.0, &bodies);
    }
    let b = bodies.get(0);
    let moving = set.list.iter().filter(|s| !s.anchored && !s.resting).count();
    let lowest = set.list.iter().flat_map(|s| s.parts.iter().filter(|p| p.alive).flat_map(move |p| p.shape.verts().map(move |v| s.to_world(p.local.transform_point3(v))))).map(|w| b.altitude(w)).fold(f64::MAX, f64::min);
    println!("after 90 s: {moving} still moving, lowest corner {lowest:.2} m");
    for s in &set.list {
        let low = s.parts.iter().filter(|p| p.alive).flat_map(|p| p.shape.verts().map(move |v| s.to_world(p.local.transform_point3(v)))).map(|w| b.altitude(w)).fold(f64::MAX, f64::min);
        if low < -0.4 || (!s.anchored && !s.resting) {
            println!("  {} parts={} m={:.0} r={:.2} rest={} v={:.2} w={:.2} low={:.2} com_alt={:.2}", s.name, s.parts.iter().filter(|p| p.alive).count(), s.mass, s.radius, s.resting, s.vel.length(), s.spin.length(), low, b.altitude(s.to_world(s.com)));
        }
    }
    assert!(moving * 20 <= free, "{moving} of {free} still moving");
    assert!(lowest > -1.5, "something sank {lowest} m");
}

#[test]
fn a_shot_cuts_through_until_its_energy_runs_out() {
    let (bodies, mut set, rules) = world();
    set.place("modulo_lunar", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let s = &set.list[0];
    let c = s.to_world(s.parts[0].center);
    let side = (s.rot * Vec3::X).as_dvec3();
    let mut events = Vec::new();
    let hp: f32 = set.list[0].parts.iter().map(|p| p.hp).sum();
    let at = set.shoot(c + side * 30.0, -side, 2.0e5, 0.002, 100.0, &rules, &mut events, 3).expect("it hits");
    assert!(at.distance(c) < 4.0);
    let taken = hp - set.list.iter().flat_map(|s| s.parts.iter()).filter(|p| !p.fragment && p.alive).map(|p| p.hp).sum::<f32>();
    assert!(taken > 0.0 && taken <= 2.0e5 * 1.001, "took {taken} J of 2e5");
    // a feeble one stops in the first part it meets
    let mut set2 = world().1;
    set2.place("modulo_lunar", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    set2.shoot(c + side * 30.0, -side, 50.0, 0.002, 100.0, &rules, &mut events, 3).unwrap();
    let hurt = set2.list[0].parts.iter().filter(|p| p.hp < p.max_hp).count();
    assert_eq!(hurt, 1);
}

#[test]
fn the_same_hit_breaks_the_same_way() {
    let run = || {
        let (bodies, mut set, rules) = world();
        set.place("estacion", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
        let s = &set.list[0];
        let at = s.to_world(Vec3::new(-8.0, 2.0, 0.0));
        set.blast(at, 3.0e7, 25.0, &rules, &mut Vec::new(), 99);
        for _ in 0..300 {
            set.step(1.0 / 60.0, &bodies);
        }
        (set.list.len(), total_mass(&set), set.list.iter().map(|s| s.pos.x).sum::<f64>())
    };
    assert_eq!(run(), run());
}
