//! The budget of a ship, measured on the Alcotán: what it costs to hold, to draw, to run and to
//! shoot at. Battles are tens or hundreds of these at once, so each number has a ceiling here and
//! whoever goes over it finds out at once. Run with `--nocapture` to read the measurements.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    font::Font,
    props::PropScene,
    structure::{
        Library,
        breakup::Rules,
        look::{FlatMesher, Mesher},
        set::Structures,
        state::Structure,
    },
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, geom::Role};
use std::{path::Path, sync::Arc, time::Instant};

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn library() -> (Library, Arc<ShipKind>) {
    let defs = assets().join("defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    (lib, kind)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

#[test]
fn a_ship_is_few_parts_and_few_triangles() {
    let (lib, kind) = library();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let conduits = kind.roles.iter().filter(|r| **r == Role::Conduit).count();
    let mut look = Vec::new();
    FlatMesher.basic(&s, &lib.catalog, &mut look);
    let wiring = look.iter().filter(|v| v.inner & 2 != 0).count();
    eprintln!("piezas {} (canalizaciones {conduits}), uniones {}, vértices {} ({} de canalizaciones)", s.parts.len(), s.joints.len(), look.len(), wiring);
    assert!(s.parts.len() <= 700, "{} piezas", s.parts.len());
    assert!(conduits <= 220, "{conduits} piezas de canalización");
    assert!(s.joints.len() <= 3000, "{} uniones", s.joints.len());
    assert!(look.len() <= 90_000, "{} vértices", look.len());
    assert!(wiring <= 45_000, "{wiring} vértices de canalizaciones");
}

#[test]
fn its_systems_and_its_scene_cost_little() {
    let (lib, kind) = library();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
    ship.update(&mut s, &w, 1.0);
    let t = Instant::now();
    let n = 250;
    for _ in 0..n {
        ship.update(&mut s, &w, lunar_ship::ship::TICK);
    }
    let tick = ms(t) / f64::from(n);
    let font = Font::parse(&std::fs::read_to_string(assets().join("fonts/serigrafia.json")).unwrap()).unwrap();
    let (mut scene, mut lamps) = (PropScene::default(), Vec::new());
    let t = Instant::now();
    for _ in 0..20 {
        scene.props.clear();
        scene.frames.clear();
        scene.decals.clear();
        scene.glyphs.clear();
        lamps.clear();
        ship.scene(&s, &lib.catalog, &font, DVec3::new(0.0, 1.6, -2.0), &mut scene, &mut lamps);
    }
    let show = ms(t) / 20.0;
    eprintln!("tic de sistemas {tick:.3} ms; escena desde dentro {show:.3} ms ({} piezas sueltas)", scene.props.len());
    // generous: the test profile is not the release one
    assert!(tick < 1.0, "tic de {tick:.3} ms");
    assert!(show < 3.0, "escena de {show:.3} ms");
}

#[test]
fn shots_and_blasts_on_it_are_cheap() {
    let (lib, _) = library();
    let lib = Arc::new(lib);
    let mut set = Structures::new(lib.clone());
    let id = set.spawn("alcotan", DVec3::ZERO, glam::Quat::IDENTITY).unwrap();
    let rules = Rules::standard(&lib.catalog).unwrap();
    let mut events = Vec::new();
    let (mut worst, mut hits) = (0.0f64, 0);
    let t = Instant::now();
    for k in 0..60u64 {
        let (y, z) = (0.5 + (k % 6) as f64 * 0.4, -8.0 + (k / 6) as f64 * 1.5);
        let one = Instant::now();
        if set.shoot(DVec3::new(20.0, y, z), DVec3::NEG_X, 3500.0, 4.6e-5, 50.0, &rules, &mut events, k).is_some() {
            hits += 1;
        }
        worst = worst.max(ms(one));
    }
    let shot = ms(t) / 60.0;
    let pieces = set.list.len();
    let one = Instant::now();
    set.blast(DVec3::new(3.5, 1.5, -6.0), 2.0e5, 6.0, &rules, &mut events, 99);
    let blast = ms(one);
    eprintln!("disparo {shot:.3} ms de media (peor {worst:.2}), {hits} impactos; explosión {blast:.2} ms; estructuras {}", set.list.len());
    assert!(hits > 50);
    assert!(set.get(id).is_some());
    // sixty rounds spread over it damage it; they knock nothing off (every part is held)
    assert_eq!(pieces, 1, "pieces came off under machine-gun fire");
    assert!(shot < 2.0, "disparo de {shot:.3} ms");
    assert!(blast < 40.0, "explosión de {blast:.2} ms");
}

#[test]
fn ships_run_into_each_other_cheaply() {
    // four dropped on one spot, one through another: the worst there is for contacts
    let (lib, _) = library();
    let def: lunar_core::body::BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(assets().join("defs/bodies/luna.jsonc")).unwrap()).unwrap();
    let bodies = lunar_core::body::BodyRegistry::new(vec![lunar_core::body::Body::from_def("luna", &def).unwrap()]);
    let mut set = Structures::new(Arc::new(lib));
    for k in 0..4 {
        set.place("alcotan", &bodies, 0, DVec3::Y, 0.4 * f64::from(k), 0.0).unwrap();
        let s = set.list.last_mut().unwrap();
        s.anchored = false;
        s.resting = false;
        let up = bodies.get(0).up(s.pos);
        s.pos += up * (3.0 + f64::from(k) * 2.5);
    }
    let (mut worst, mut total) = (0.0f64, 0.0);
    for _ in 0..240 {
        let t = Instant::now();
        set.step(1.0 / 60.0, &bodies);
        let d = ms(t);
        worst = worst.max(d);
        total += d;
    }
    eprintln!("4 naves una sobre otra: {:.3} ms por paso de media, peor {worst:.2} ms", total / 240.0);
    assert!(total / 240.0 < 4.0, "{:.2} ms por paso", total / 240.0);
}
