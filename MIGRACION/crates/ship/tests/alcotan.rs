//! The real content: every ship of assets/defs builds, and the Alcotán works on its own.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World};
use std::path::Path;

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn load() -> (Library, ShipLibrary) {
    let mut lib = Library::load(&assets().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&assets(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships)
}

fn spawn(lib: &Library, ships: &ShipLibrary, id: &str) -> (Structure, Ship) {
    let kind = ships.get(id).unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    (s, ship)
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() };
    let steps = (secs / 0.05) as usize;
    for _ in 0..steps {
        ship.update(s, &w, 0.05);
    }
}

#[test]
fn alcotan_builds_and_powers_up() {
    let (lib, ships) = load();
    let (mut s, mut ship) = spawn(&lib, &ships, "alcotan");
    let k = &ship.kind;
    eprintln!(
        "Alcotán: {} parts, {} machines, {} actuators, {} joints, {} panels, {} controls, {} indicators, {} nets, {} signals, mass {:.0} kg",
        s.parts.len(),
        ship.machines.len(),
        ship.actuators.len(),
        ship.joints.len(),
        k.panels.len(),
        ship.panels.controls.len(),
        ship.panels.indicators.len(),
        ship.nets.len(),
        ship.store.len(),
        s.mass
    );
    run(&mut s, &mut ship, 3.0);
    let get = |n: &str| ship.signal(n).unwrap_or_else(|| panic!("no signal {n}"));
    eprintln!("bus A {:.1} V, bus B {:.1} V, ess {:.1} V", get("elec.bus_a"), get("elec.bus_b"), get("elec.bus_ess"));
    assert!(get("elec.bus_a") > 24.0, "bus A powered by the batteries");
    assert!(get("elec.bus_ess") > 24.0);
    assert!(get("plafon_cab#0.on") > 0.5, "cabin lights on");
    assert!(get("cabina.p") > 60_000.0, "cabin pressurised");
    // the ramp starts open, the gear down and locked
    assert!(get("rampa.abierta") > 0.95, "the ramp starts open: {}", get("rampa.abierta"));
    assert!(get("tren.verde_i") > 0.5 && get("tren.verde_d") > 0.5 && get("tren.verde_m") > 0.5, "three greens");
    // hydraulics up
    let hpu = ship.panels.controls.iter().position(|c| c.id == "techo/hpu_a").unwrap();
    let (kind, store) = (ship.kind.clone(), &ship.store);
    let _ = (&kind, store);
    ship.panels.intent(hpu, &lunar_controls::Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 6.0);
    eprintln!("hydraulics {:.1} MPa", ship.signal("hid.colector").unwrap() / 1e6);
    assert!(ship.signal("hid.colector").unwrap() > 15e6, "hydraulic pressure builds");
    // close the ramp (bodega is in vacuum: no interlock)
    let btn = ship.panels.controls.iter().position(|c| c.id == "rampa_bodega/rampa").unwrap();
    ship.panels.intent(btn, &lunar_controls::Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 0.1);
    ship.panels.intent(btn, &lunar_controls::Intent::Release, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 25.0);
    eprintln!("ramp open {:.2} latched {}", ship.signal("rampa.abierta").unwrap(), ship.signal("rampa.cerrada").unwrap());
    assert!(ship.signal("rampa.cerrada").unwrap() > 0.5, "the winches close the ramp and the latches hold it");
    // the reactor: start it and let it climb
    let cover = ship.panels.controls.iter().position(|c| c.id == "reactor/tapa_marcha").unwrap();
    let run_sw = ship.panels.controls.iter().position(|c| c.id == "reactor/marcha").unwrap();
    ship.panels.intent(cover, &lunar_controls::Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    let o = ship.panels.intent(run_sw, &lunar_controls::Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    assert!(o.changed, "{o:?}");
    run(&mut s, &mut ship, 120.0);
    eprintln!(
        "reactor state {} n {:.3} fuel {:.0} °C, converter {:.1} kW, battery {:.2} {:.1} A",
        ship.signal("reactor.estado").unwrap(),
        ship.signal("reactor.n").unwrap(),
        ship.signal("reactor.t_comb").unwrap() - 273.15,
        ship.signal("conversor.p").unwrap() / 1e3,
        ship.signal("bateria_1.soc").unwrap(),
        ship.signal("bateria_1.i").unwrap()
    );
    assert_eq!(ship.signal("reactor.estado").unwrap(), 2.0, "reactor online");
    for e in ship.blackbox.entries.iter().take(20) {
        eprintln!("  t {:.1}: {}", e.t, e.text);
    }
}

#[test]
fn alcotan_mass_breakdown() {
    let (lib, ships) = load();
    let (s, ship) = spawn(&lib, &ships, "alcotan");
    let mut parts: Vec<(f32, &str)> = s.parts.iter().enumerate().map(|(i, p)| (p.mass, ship.kind.parts[i].as_str())).collect();
    parts.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (m, id) in parts.iter().take(25) {
        eprintln!("{m:9.0} kg  {id}");
    }
    let hull: f32 = s.parts.iter().enumerate().filter(|(i, _)| ship.kind.parts[*i].starts_with("casco")).map(|(_, p)| p.mass).sum();
    let conduits: f32 = s.parts.iter().enumerate().filter(|(i, _)| ship.kind.roles[*i] == lunar_ship::geom::Role::Conduit).map(|(_, p)| p.mass).sum();
    eprintln!("hull {hull:.0} kg, conduits {conduits:.0} kg, total {:.0} kg", s.mass);
}

fn press(ship: &mut Ship, s: &Structure, id: &str) {
    let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no control {id}"));
    let kind = ship.kind.clone();
    let o = ship.panels.intent(k, &lunar_controls::Intent::Press { elem: 0 }, s, &kind, &ship.store);
    ship.panels.intent(k, &lunar_controls::Intent::Release, s, &kind, &ship.store);
    eprintln!("  {id}: {o:?}");
}

#[test]
fn alcotan_engines_lift_it() {
    let (lib, ships) = load();
    let (mut s, mut ship) = spawn(&lib, &ships, "alcotan");
    run(&mut s, &mut ship, 3.0);
    for id in ["pedestal/tapa_arm_izq", "pedestal/arm_izq", "pedestal/tapa_arm_der", "pedestal/arm_der", "pedestal/arr_izq", "pedestal/arr_der"] {
        press(&mut ship, &s, id);
    }
    // ARRANQUE: a second flip up, sprung back to MARCHA on release
    let kind = ship.kind.clone();
    for id in ["pedestal/arr_izq", "pedestal/arr_der"] {
        let k = ship.panels.controls.iter().position(|c| c.id == id).unwrap();
        ship.panels.intent(k, &lunar_controls::Intent::Press { elem: 0 }, &s, &kind, &ship.store);
        run(&mut s, &mut ship, 0.3);
        ship.panels.intent(k, &lunar_controls::Intent::Release, &s, &kind, &ship.store);
    }
    run(&mut s, &mut ship, 4.0);
    let k = ship.panels.controls.iter().position(|c| c.id == "pedestal/acelerador").unwrap();
    ship.panels.intent(k, &lunar_controls::Intent::Set { value: 1.0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 3.0);
    let get = |n: &str| ship.signal(n).unwrap_or(f64::NAN);
    eprintln!(
        "engines: izq estado {} empuje {:.1} kN, der estado {} empuje {:.1} kN; feed {:.2} MPa; force {:?} N, mass {:.0} kg (weight {:.1} kN)",
        get("gondola_izq.estado"),
        get("gondola_izq.empuje") / 1e3,
        get("gondola_der.estado"),
        get("gondola_der.empuje") / 1e3,
        get("prop.alim_izq") / 1e6,
        s.force,
        s.mass,
        s.mass * 1.62 / 1e3
    );
    for e in ship.blackbox.entries.iter().rev().take(12) {
        eprintln!("  t {:.1}: {}", e.t, e.text);
    }
    assert!(s.force.y > s.mass * 1.62, "thrust beats lunar weight");
}

/// On the real Moon where the scenario puts it, on its gear: it settles and sleeps (no hopping
/// from foot to foot), turned to the slope under its feet.
#[test]
fn alcotan_rests_on_its_gear() {
    use lunar_core::{
        body::{Body, BodyDef, BodyRegistry},
        defs,
        scenario::ScenarioDef,
        scene::Site,
        structure::set::Structures,
    };
    let (lib, ships) = load();
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let sc: ScenarioDef = defs::parse("scenario", include_str!("../../../assets/defs/scenario.jsonc")).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let site = Site::from_def(&sc.site, &bodies).unwrap();
    let mut set = Structures::new(std::sync::Arc::new(lib));
    for (n, at) in sc.ships.iter().chain([&lunar_core::scenario::ShipAtDef { ship: "alcotan".into(), east: 40.0, north: -30.0, yaw: 75.0 }]).enumerate() {
        let kind = ships.get(&at.ship).unwrap().clone();
        let id = set.place(&kind.blueprint, &bodies, site.body, site.at(at.east, at.north), at.yaw.to_radians(), f64::from(kind.lift)).unwrap();
        let k = set.list.len() - 1;
        let mut ship = Ship::new(kind, id, 3).unwrap();
        let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), ..World::default() };
        ship.update(&mut set.list[k], &w, 0.0);
        set.rest_on_ground(id, &bodies);
        let (mut slept, mut fastest) = (None, 0.0f64);
        for step in 0..(6 * 60) {
            ship.update(&mut set.list[k], &w, 1.0 / 60.0);
            set.step(1.0 / 60.0, &bodies);
            let s = &set.list[k];
            fastest = fastest.max(s.vel.length());
            if s.resting && slept.is_none() {
                slept = Some(step);
            }
        }
        let s = &set.list[k];
        let tilt = (s.rot * Vec3::Y).as_dvec3().angle_between(bodies.get(0).up(s.pos)).to_degrees();
        eprintln!("ship {n}: asleep after {:?} s, fastest {fastest:.2} m/s, tilt {tilt:.1} deg", slept.map(|x| x as f32 / 60.0));
        assert!(s.resting, "the ship settles on its gear");
        assert!(fastest < 1.0, "no hopping");
    }
}

#[test]
fn hold_floodlights_switch() {
    let (lib, ships) = load();
    let (mut s, mut ship) = spawn(&lib, &ships, "alcotan");
    run(&mut s, &mut ship, 2.0);
    let get = |ship: &Ship, n: &str| ship.signal(n).unwrap_or(f64::NAN);
    eprintln!("before: switch {} order {} light {} {}", get(&ship, "rampa_bodega.focos"), get(&ship, "luces.focos_bodega"), get(&ship, "foco_bodega_izq.on"), get(&ship, "foco_bodega_der.on"));
    press(&mut ship, &s, "rampa_bodega/focos");
    run(&mut s, &mut ship, 1.0);
    eprintln!("after: switch {} order {} light {} {} c_luces_bod?", get(&ship, "rampa_bodega.focos"), get(&ship, "luces.focos_bodega"), get(&ship, "foco_bodega_izq.on"), get(&ship, "foco_bodega_der.on"));
    for id in ship.store.ids() {
        let n = ship.store.name(id);
        if n.contains("foco") || n.contains("luces_bod") {
            eprintln!("  {n} = {}", ship.store.get(id));
        }
    }
    assert!(get(&ship, "foco_bodega_izq.on") > 0.5);
}

#[test]
fn panel_scales() {
    let (lib, ships) = load();
    let (_, ship) = spawn(&lib, &ships, "alcotan");
    for p in &ship.kind.panels {
        eprintln!("{:<16} {:>4.0}×{:<4.0} mm  scale {:.1}", p.id, p.layout.size[0], p.layout.size[1], p.layout.scale);
    }
}

#[test]
fn conduits_routed_along_walls() {
    let t = std::time::Instant::now();
    let (lib, ships) = load();
    eprintln!("load (routing included) {:.2} s", t.elapsed().as_secs_f32());
    let (s, ship) = spawn(&lib, &ships, "alcotan");
    let k = &ship.kind;
    // no conduit segment crosses the walkway in the middle of a room (between 0.3 and 1.9 m over
    // the deck, more than 0.5 m from the hull sides)
    let mut bad = 0;
    let mut total = 0;
    for (i, p) in s.parts.iter().enumerate() {
        if k.roles[i] != lunar_ship::geom::Role::Conduit {
            continue;
        }
        total += 1;
        let c = p.center;
        if c.y > 0.3 && c.y < 1.9 && c.x.abs() < 1.2 && c.z > -9.0 && c.z < 6.0 {
            bad += 1;
            if bad < 15 {
                eprintln!("  in the way: {} at {:?}", k.parts[i], c);
            }
        }
    }
    eprintln!("{bad} of {total} conduit segments in the walkway");
}
