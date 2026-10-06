//! Landing gear: every ship of the library stands on sprung legs. Set down, each leg rests
//! pressed in by its share of the weight (about three quarters of its stroke, an oleo's) and the
//! ship goes to sleep; dropped gently the legs take it without bottoming out and without throwing
//! it back up; dropped hard they bottom out and the hull is still held off the ground; lifted
//! off, they come out again.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, set::Structures},
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World};
use std::{path::Path, sync::Arc};

fn library() -> (Arc<Library>, Vec<Arc<ShipKind>>) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (Arc::new(lib), ships.kinds.clone())
}

fn moon() -> BodyRegistry {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs.join("bodies/luna.jsonc")).unwrap()).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
}

/// A level place (the same search the other ship tests use).
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
        let hs = [at(0.0, 0.0), at(24.0, 0.0), at(-24.0, 0.0), at(0.0, 24.0), at(0.0, -24.0), at(16.0, 16.0), at(-16.0, -16.0), at(16.0, -16.0), at(-16.0, 16.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

/// A ship of `kind` set on the ground at `at` with its legs full out, then let fall from `drop` m.
/// Stepped `secs`; what happened to it.
struct Landing {
    /// The deepest any leg went and where each ended (shares of its stroke).
    deepest: f32,
    rest: Vec<f32>,
    /// Slept at (s), the lowest its hull came to the ground under it (m), the highest it went
    /// back up after first touching (m over where it ended).
    slept: Option<f32>,
    clearance: f32,
    rebound: f64,
    loads: Vec<f32>,
    mass: f32,
}

fn land(lib: &Arc<Library>, kind: &Arc<ShipKind>, bodies: &BodyRegistry, at: DVec3, drop: f64, secs: f32) -> Landing {
    let mut set = Structures::new(lib.clone());
    let id = set.place(&kind.blueprint, bodies, 0, at, 0.4, f64::from(kind.lift)).unwrap();
    let mut ship = Ship::new(kind.clone(), id, 7).unwrap();
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
    ship.update(&mut set.list[0], &w, 0.0);
    set.rest_on_ground(id, bodies);
    let up = bodies.get(0).up(set.list[0].pos);
    set.list[0].pos += up * drop;
    set.list[0].resting = false;
    let tick = lunar_ship::ship::TICK;
    let per = (1.0 / 60.0 / tick).round().max(1.0) as usize;
    let (mut deepest, mut slept, mut clearance) = (0.0f32, None, f32::MAX);
    let (mut lowest, mut top) = (f64::MAX, f64::MIN);
    let height = |s: &lunar_core::structure::state::Structure| (s.pos - bodies.get(0).center).length();
    for f in 0..(secs * 60.0) as usize {
        for _ in 0..per {
            ship.update(&mut set.list[0], &w, tick);
        }
        set.step(1.0 / 60.0, bodies);
        let s = &set.list[0];
        for sp in &s.springs {
            deepest = deepest.max(sp.x / sp.stroke);
        }
        // how high it comes back up after the lowest it went
        let h = height(s);
        if h < lowest {
            (lowest, top) = (h, h);
        }
        top = top.max(h);
        // the hull (what is not a leg) over the ground under it, corner by corner
        if f % 6 == 0 {
            let b = bodies.get(0);
            // (what moves on a joint — the ramp set down on the ground — is not the hull)
            for p in s.parts.iter().filter(|p| p.alive && p.collide && p.bone == 0) {
                let c = s.to_world(p.center);
                let ground = b.radius + b.height(b.up(c));
                if ((c - b.center).length() - ground) as f32 - p.radius > clearance {
                    continue;
                }
                for v in p.shape.verts() {
                    let q = s.to_world(p.local.transform_point3(v));
                    clearance = clearance.min(((q - b.center).length() - (b.radius + b.height(b.up(q)))) as f32);
                }
            }
        }
        if s.resting {
            slept.get_or_insert(f as f32 / 60.0);
        }
    }
    let s = &set.list[0];
    assert!(!s.springs.is_empty(), "{}: sin patas con muelle", kind.id);
    Landing { deepest, rest: s.springs.iter().map(|sp| sp.x / sp.stroke).collect(), slept, clearance, rebound: top - height(s), loads: s.springs.iter().map(|sp| sp.load).collect(), mass: s.mass }
}

#[test]
fn a_gas_spring_is_soft_at_first_and_hard_at_the_end() {
    let sp = lunar_core::structure::state::Spring { bone: 1, foot: Vec3::ZERO, axis: Vec3::Y, stroke: 0.32, preload: 2500.0, end: 30_000.0, gas: 1.2, damp: [0.0; 2], grip: 0.9, active: true, x: 0.0, load: 0.0 };
    let (f0, k0) = sp.force(0.0);
    let (f1, k1) = sp.force(0.32);
    assert!((f0 - 2500.0).abs() < 1.0 && (f1 - 30_000.0).abs() < 30.0, "{f0} .. {f1}");
    assert!(k1 > k0 * 5.0, "se endurece: {k0} -> {k1} N/m");
    // what it rests at under a load is where it pushes with that load
    for load in [3000.0f32, 10_000.0, 25_000.0] {
        let x = sp.at_rest(load);
        assert!((sp.force(x).0 - load).abs() < load * 1e-3, "{load} N: {x} m da {} N", sp.force(x).0);
    }
    assert_eq!(sp.at_rest(1000.0), 0.0);
    // under what it is built for (4 × preload, a third of its end) it rests about three quarters in
    let x = sp.at_rest(10_000.0) / sp.stroke;
    assert!((0.65..0.85).contains(&x), "en reposo a {x:.2} de su carrera");
}

#[test]
fn every_ship_stands_on_its_legs_and_sleeps() {
    let bodies = moon();
    let at = level_ground(&bodies);
    let (lib, kinds) = library();
    for k in &kinds {
        let l = land(&lib, k, &bodies, at, 0.0, 20.0);
        eprintln!("{}: patas a {:?} de su carrera, la más honda {:.2}; duerme a los {:?} s; casco a {:.2} m del suelo; cargas {:?} N de {:.0} N", k.id, l.rest.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>(), l.deepest, l.slept, l.clearance, l.loads.iter().map(|x| x.round()).collect::<Vec<_>>(), l.mass * 1.62);
        assert!(l.rest.iter().all(|x| (0.4..0.97).contains(x)), "{}: patas a {:?} de su carrera", k.id, l.rest);
        assert!(l.slept.is_some_and(|t| t < 12.0), "{}: no se duerme ({:?})", k.id, l.slept);
        assert!(l.clearance > 0.15, "{}: el casco toca el suelo ({:.2} m)", k.id, l.clearance);
    }
}

#[test]
fn a_gentle_landing_is_taken_by_the_legs_and_a_hard_one_bottoms_out() {
    let bodies = moon();
    let at = level_ground(&bodies);
    let (lib, kinds) = library();
    for k in &kinds {
        // from 0.35 m: 1.1 m/s at the ground, a firm landing
        let soft = land(&lib, k, &bodies, at, 0.35, 14.0);
        eprintln!("{}: suave: hasta {:.2} de la carrera, rebota {:.2} m, queda a {:?}", k.id, soft.deepest, soft.rebound, soft.rest.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>());
        assert!(soft.deepest < 0.999, "{}: una toma a 1 m/s hace tope ({:.3})", k.id, soft.deepest);
        assert!(soft.deepest > soft.rest.iter().copied().fold(0.0, f32::max) - 0.02, "{}: las patas no pasaron de donde descansan", k.id);
        assert!(soft.rebound < 0.2, "{}: rebota {:.2} m", k.id, soft.rebound);
        assert!(soft.clearance > 0.1 && soft.slept.is_some(), "{}: casco a {:.2} m, duerme {:?}", k.id, soft.clearance, soft.slept);
        // from 5 m: 4 m/s, more than a gear is built for
        let hard = land(&lib, k, &bodies, at, 5.0, 16.0);
        eprintln!("{}: dura: hasta {:.2} de la carrera, casco a {:.2} m", k.id, hard.deepest, hard.clearance);
        assert!(hard.deepest > 0.999, "{}: una toma a 4 m/s no hace tope ({:.3})", k.id, hard.deepest);
        assert!(hard.clearance > 0.0, "{}: el casco dio en el suelo ({:.2} m)", k.id, hard.clearance);
    }
}

#[test]
fn lifted_off_its_legs_come_out_again() {
    let bodies = moon();
    let at = level_ground(&bodies);
    let (lib, kinds) = library();
    let k = kinds.iter().find(|k| k.id == "alcotan").unwrap();
    let mut set = Structures::new(lib.clone());
    let id = set.place(&k.blueprint, &bodies, 0, at, 0.0, f64::from(k.lift)).unwrap();
    let mut ship = Ship::new(k.clone(), id, 7).unwrap();
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
    ship.update(&mut set.list[0], &w, 0.0);
    set.rest_on_ground(id, &bodies);
    // set down as it rests: nothing moves, the legs carry it between them
    let sink = ship.rest_on_legs(&mut set.list[0], 1.62);
    let up = bodies.get(0).up(set.list[0].pos);
    set.list[0].pos -= up * f64::from(sink);
    assert!(sink > 0.15 && sink < 0.32, "se asienta {sink:.2} m");
    let start = set.list[0].pos;
    for f in 0..240 {
        ship.update(&mut set.list[0], &w, 1.0 / 60.0);
        set.step(1.0 / 60.0, &bodies);
        let _ = f;
    }
    let s = &set.list[0];
    // (the ground is not flat under its three feet: it finds its own level, a little further)
    assert!(s.pos.distance(start) < 0.2 && s.resting, "puesta como descansa, se mueve {:.3} m (dormida: {})", s.pos.distance(start), s.resting);
    let carried: f32 = s.springs.iter().map(|sp| sp.load).sum();
    assert!(s.resting || (carried - s.mass * 1.62).abs() < s.mass * 1.62 * 0.15, "las patas llevan {carried:.0} N de {:.0}", s.mass * 1.62);
    assert_eq!(ship.signal("amort_izq.pos").map(|v| v > 0.4), Some(true), "la señal de la pata dice cuánto está metida");
    // taken up by a quarter of a metre a second (as its engines would): the feet stay on the
    // ground until the legs are full out, then leave it
    set.list[0].resting = false;
    for _ in 0..300 {
        set.list[0].vel = up * 0.25;
        set.list[0].resting = false;
        ship.update(&mut set.list[0], &w, 1.0 / 60.0);
        set.step(1.0 / 60.0, &bodies);
    }
    let s = &set.list[0];
    assert!(s.springs.iter().all(|sp| sp.x < 0.01), "en el aire, las patas siguen metidas: {:?}", s.springs.iter().map(|sp| sp.x).collect::<Vec<_>>());
    assert_eq!(ship.signal("amort_izq.pos").map(|v| v < 0.05), Some(true));
    let _ = DQuat::IDENTITY;
}
