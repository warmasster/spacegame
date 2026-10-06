//! Every ship of the library, the same way: it builds, it fits a person (no panel cut by a wall or
//! crossed by a cable, every control reached from its seats), its rooms are worked from inside,
//! its conduits stay where they should, it is tight, and it keeps within a budget for its size.
//! A new ship is a file in `assets/defs/ships`: it is checked here without a line of test.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{
        Library,
        look::{FlatMesher, Mesher},
        set::Structures,
        state::Structure,
    },
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, diag, geom::Role};
use std::{path::Path, sync::Arc, time::Instant};

fn library() -> (Library, Vec<Arc<ShipKind>>) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships.kinds.clone())
}

fn spawn(lib: &Library, kind: &Arc<ShipKind>) -> (Structure, Ship) {
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{}: {e}", kind.id));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship)
}

#[test]
fn there_are_ships_of_three_sizes() {
    let (lib, kinds) = library();
    let mut sizes: Vec<(String, f32, f32)> = kinds
        .iter()
        .map(|k| {
            let (s, _) = spawn(&lib, k);
            (k.id.clone(), s.radius * 2.0, s.mass)
        })
        .collect();
    sizes.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (id, size, mass) in &sizes {
        eprintln!("{id}: {size:.0} m, {:.1} t", mass / 1000.0);
    }
    assert!(sizes.len() >= 3, "{} naves", sizes.len());
    let alcotan = sizes.iter().position(|s| s.0 == "alcotan").expect("el Alcotán");
    assert!(alcotan > 0 && alcotan + 1 < sizes.len(), "hace falta una nave más pequeña y otra más grande que el Alcotán");
    assert!(sizes[0].1 < sizes[alcotan].1 * 0.6 && sizes[sizes.len() - 1].1 > sizes[alcotan].1 * 1.6, "tamaños {sizes:?}");
}

#[test]
fn every_ship_fits_a_person_and_is_worked_from_where_it_should() {
    let (lib, kinds) = library();
    let mut bad = Vec::new();
    for k in &kinds {
        let (s, ship) = spawn(&lib, k);
        let mut found = diag::panels_cut(&ship, &s);
        found.extend(diag::panel_faces(k));
        found.extend(diag::seat_reach(&ship, &s));
        found.extend(diag::seat_fit(&ship.kind, &s));
        found.extend(diag::usability(&ship, &s));
        let c = diag::cables(&s, k, &lib.catalog);
        found.extend(c.exposed.iter().map(|e| format!("canalización por fuera: {e}")));
        found.extend(c.through.iter().map(|e| format!("canalización a través de un aparato: {e}")));
        if c.buried.len() * 100 > c.conduits.max(1) * 4 {
            found.push(format!("{} de {} canalizaciones medio enterradas: {}", c.buried.len(), c.conduits, c.buried.iter().take(8).cloned().collect::<Vec<_>>().join("; ")));
        }
        // every seat at the controls says which panels it works
        for seat in &k.seats {
            if !seat.def.mandos.is_empty() && seat.def.paneles.is_empty() {
                found.push(format!("asiento {}: tiene teclas y no dice qué paneles trabaja", seat.def.id));
            }
        }
        eprintln!("{}: {} paneles, {} asientos, {} mandos, {} compartimentos, {} anclajes: {} problemas", k.id, k.panels.len(), k.seats.len(), ship.panels.controls.len(), k.compartments.len(), k.clamps.len(), found.len());
        bad.extend(found.into_iter().map(|f| format!("{}: {f}", k.id)));
    }
    for b in &bad {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty(), "{} problemas", bad.len());
}

#[test]
fn every_ship_is_tight() {
    let (lib, kinds) = library();
    for k in kinds.iter().filter(|k| !k.compartments.is_empty()) {
        // every closure shut, as the leak check wants
        let bp = lib.blueprint(&k.blueprint).unwrap();
        let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
        let mut ship = Ship::new(k.clone(), 1, 7).unwrap();
        for j in k.joints.iter().filter(|j| k.closures.iter().any(|c| c.joints.iter().any(|&x| k.joints[x].id == j.id))) {
            ship.set_joint(&j.id, 0.0);
        }
        ship.update(&mut s, &World::default(), 0.0);
        let leaks = diag::leaks(&s, k);
        for l in &leaks {
            eprintln!("  {}: {} se sale por {:?}, llega a {:?}", k.id, l.room, l.hole, l.reaches);
        }
        assert!(leaks.is_empty(), "{}: {} fugas", k.id, leaks.len());
    }
}

/// What a ship may cost, by its size: parts and vertices per metre of length, a tick of its
/// systems. (The Alcotán, 20 m: 527 parts, 49 650 vertices, 0.08 ms.)
#[test]
fn every_ship_keeps_within_its_budget() {
    let (lib, kinds) = library();
    for k in &kinds {
        let (mut s, mut ship) = spawn(&lib, k);
        let length = s.radius * 2.0;
        let conduits = k.roles.iter().filter(|r| **r == Role::Conduit).count();
        // (what it is from a little way off, which is what a fleet of them costs; and with its
        // models, which only the ones within 40 m show)
        let (mut look, mut fine) = (Vec::new(), Vec::new());
        FlatMesher.basic(&s, &lib.catalog, &mut look);
        FlatMesher.mesh(&s, &lib.catalog, &mut fine);
        let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
        ship.update(&mut s, &w, 1.0);
        let t = Instant::now();
        let n = 250;
        for _ in 0..n {
            ship.update(&mut s, &w, lunar_ship::ship::TICK);
        }
        let tick = t.elapsed().as_secs_f64() * 1e3 / f64::from(n);
        eprintln!("{}: {:.0} m, {} piezas ({conduits} canalizaciones), {} uniones, {} vértices ({} de cerca, con sus modelos), tic {tick:.3} ms, {} máquinas", k.id, length, s.parts.len(), s.joints.len(), look.len(), fine.len(), k.machines.len());
        assert!(s.parts.len() as f32 <= 40.0 * length.max(12.0), "{}: {} piezas para {length:.0} m", k.id, s.parts.len());
        assert!(look.len() as f32 <= 4500.0 * length.max(12.0), "{}: {} vértices para {length:.0} m", k.id, look.len());
        assert!(fine.len() as f32 <= 45_000.0 * length.max(12.0), "{}: {} vértices de cerca para {length:.0} m", k.id, fine.len());
        assert!(s.joints.len() <= 6 * s.parts.len(), "{}: {} uniones", k.id, s.joints.len());
        assert!(tick < 0.5, "{}: {tick:.3} ms por tic", k.id);
    }
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
        let hs = [at(0.0, 0.0), at(24.0, 0.0), at(-24.0, 0.0), at(0.0, 24.0), at(0.0, -24.0), at(16.0, 16.0), at(-16.0, -16.0), at(16.0, -16.0), at(-16.0, 16.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

/// A ship standing on the ground with its systems running costs the physics nothing: it goes to
/// sleep (whatever trims it — a wheel, a fan — does not keep it awake; its stabiliser leaves the
/// ground alone), where it was set down. On level ground and on the rough ground round it, a
/// dozen spots each, slopes of up to 17° among them.
#[test]
fn every_ship_set_down_goes_to_sleep() {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs.join("bodies/luna.jsonc")).unwrap()).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let base = level_ground(&bodies);
    let side = base.any_orthonormal_vector();
    let fwd = base.cross(side);
    let radius = bodies.get(0).radius;
    let (_, kinds) = library();
    let mut bad = Vec::new();
    for k in &kinds {
        let (lib, _) = library();
        let lib = Arc::new(lib);
        for spot in 0..12 {
            let (i, j) = (f64::from(spot % 4), f64::from(spot / 4));
            let at = (base * radius + side * (i * 170.0) + fwd * (j * 170.0)).normalize();
            let mut set = Structures::new(lib.clone());
            let id = set.place(&k.blueprint, &bodies, 0, at, 0.7 * f64::from(spot), f64::from(k.lift)).unwrap();
            let mut ship = Ship::new(k.clone(), id, 7).unwrap();
            let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
            ship.update(&mut set.list[0], &w, 0.0);
            set.rest_on_ground(id, &bodies);
            // how steep the ground is there, over the ship's own size
            let slope = {
                let b = bodies.get(0);
                let st = &set.list[0];
                let c = st.to_world(st.center);
                let probe = |p: DVec3| b.center + b.up(p) * (b.radius + b.height(b.up(p)));
                let sx = at.any_orthonormal_vector() * f64::from(st.radius) * 0.5;
                let sz = at.cross(sx);
                let n = (probe(c + sx) - probe(c - sx)).cross(probe(c + sz) - probe(c - sz)).normalize();
                n.angle_between(at).min((-n).angle_between(at)).to_degrees()
            };
            let start = set.list[0].pos;
            let lean = (set.list[0].rot * Vec3::Y).as_dvec3().angle_between(at).to_degrees();
            let (mut awake, mut cost, mut slept_at) = (0u32, 0.0f64, None);
            // (and what its systems push it with while it settles: nothing — a ship set down must
            // not fire its thrusters at the ground)
            let mut pushed = 0.0f32;

            let tick = lunar_ship::ship::TICK;
            let per = (1.0 / 60.0 / tick).round().max(1.0) as usize;
            for f in 0..(30 * 60) {
                for _ in 0..per {
                    ship.update(&mut set.list[0], &w, tick);
                }
                pushed = pushed.max(set.list[0].force.length());
                let t = Instant::now();
                set.step(1.0 / 60.0, &bodies);

                if set.list[0].resting {
                    slept_at.get_or_insert(f as f32 / 60.0);
                } else {
                    awake += 1;
                    cost += t.elapsed().as_secs_f64() * 1e3;
                }
            }
            let s = &set.list[0];
            eprintln!(
                "{} en {spot} (suelo a {slope:.0}°, inclinada {lean:.0}°): dormida a los {:?} s; despierta {awake} pasos a {:.3} ms; se ha ido {:.2} m; v {:.2} m/s, fuerza {:.0} N, par {:.0} N·m",
                k.id,
                slept_at,
                cost / f64::from(awake.max(1)),
                s.pos.distance(start),
                s.vel.length(),
                s.force.length(),
                s.torque.length()
            );
            // (on ground steeper than friction holds — a crater's wall — it may slide down it; it
            // still comes to rest)
            if pushed > 0.0 {
                bad.push(format!("{} en {spot}: posada, sus sistemas la empujan con {pushed:.0} N", k.id));
            }
            // (set down leaning on uneven ground it comes down on its sprung legs one after
            // another and shifts a few metres as it finds its footing; on anything near level
            // it stays where it is put, give or take what its legs sink)
            let steep = slope > 25.0;
            let room = if lean > 15.0 { 4.5 } else { 1.5 };
            if !s.resting || (!steep && (slept_at.is_none_or(|t| t > 15.0) || s.pos.distance(start) > room)) {
                bad.push(format!("{} en {spot} (suelo a {slope:.0}°): dormida {:?}, a {:.1} m de donde se posó", k.id, slept_at, s.pos.distance(start)));
            }
        }
    }
    assert!(bad.is_empty(), "posadas y con sus sistemas en marcha no se quedan quietas: {bad:?}");
}
