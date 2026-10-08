use glam::{Affine3A, DVec3, Quat};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    rounds::{Flight, Rounds, round},
    structure::{
        Library,
        motion::Sweep,
        schedule::Full,
        set::Structures,
        state::{Part, Structure},
    },
};
use std::{path::Path, sync::Arc, time::Instant};

fn world() -> (BodyRegistry, Structures) {
    let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    (BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]), Structures::new(lib))
}

fn block(set: &mut Structures, at: DVec3) -> u64 {
    let part = Part::new(&set.lib.catalog, set.lib.catalog.part("bloque").unwrap(), Affine3A::IDENTITY, None, false);
    let id = set.next_id();
    set.list.push(Structure::assemble(id, "bloque".into(), at, Quat::IDENTITY, false, vec![part], vec![]));
    id
}

#[test]
fn a_moving_wall_hits_even_a_world_stationary_round() {
    let (bodies, mut set) = world();
    let from = DVec3::new(4e6, 3e6, 2e6);
    let id = block(&mut set, from + DVec3::X * 20.0);
    set.list[0].vel = DVec3::NEG_X * 2400.0;
    let mut rounds = Rounds::new(2);
    let mut shot = round(0, from, DVec3::Z, 150.0, 1000.0, 0, 0, 0.1, 0.0);
    shot.vel = DVec3::ZERO;
    rounds.fire(shot);
    rounds.fire(round(1, from + DVec3::Y * 100_000.0, DVec3::Z, 150.0, 1000.0, 0, 0, 0.1, 0.0));
    rounds.list.swap(0, 1);
    let mut impacts = Vec::with_capacity(2);
    let mut sweep = Sweep::default();
    let mut flight = Flight { rounds: &mut rounds, impacts: &mut impacts, sweep: &mut sweep };
    set.simulate_with(1.0 / 60.0, 1.0 / 60.0, &bodies, &Full, &mut [&mut flight]);
    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0].surface.unwrap().id, id);
    assert!(impacts[0].surface.unwrap().dir.x > 0.99);
}

#[test]
fn the_relative_sweep_finds_the_wall_the_old_world_ray_missed() {
    let (_, mut set) = world();
    let from = DVec3::new(4e6, 3e6, 2e6);
    block(&mut set, from + DVec3::Z * 2.0);
    let mut sweep = Sweep::default();
    sweep.begin(&set);
    let travel = DVec3::new(130.0, 0.0, 0.0);
    set.list[0].pos += travel;
    sweep.end(&set);
    let to = from + travel + DVec3::Z * 2.5;
    assert!(set.raycast(from, (to - from).normalize(), to.distance(from)).is_none());
    let hit = sweep.hit(&set, from, to).expect("debe tocar la pared que se ha movido con el cohete");
    assert!(hit.surface.point.x.abs() < 0.001);
    assert!(hit.surface.dir.z > 0.99);
}

#[test]
fn sweeping_hundreds_of_structures_reuses_memory_and_prunes_candidates() {
    let (_, mut set) = world();
    let origin = DVec3::new(4e6, 3e6, 2e6);
    for index in 0..500 {
        block(&mut set, origin + DVec3::new((index % 25) as f64 * 100.0, 0.0, (index / 25) as f64 * 100.0));
    }
    let mut sweep = Sweep::default();
    sweep.begin(&set);
    sweep.end(&set);
    let start = Instant::now();
    let mut candidates = 0;
    for _ in 0..100 {
        sweep.begin(&set);
        sweep.end(&set);
        for shot in 0..2000 {
            let from = origin + DVec3::new((shot % 25) as f64 * 100.0, 0.0, (shot / 25 % 20) as f64 * 100.0 - 2.0);
            let to = from + DVec3::Z * 5.0;
            sweep.candidates(from, to, |_| candidates += 1);
            std::hint::black_box(sweep.hit(&set, from, to));
        }
    }
    let elapsed = start.elapsed();
    eprintln!("500 estructuras, 2000 segmentos: {:.3} ms/loncha; {} candidatos/segmento", elapsed.as_secs_f64() * 10.0, candidates as f64 / 200_000.0);
    assert!(candidates < 200_000 * 10);
}
