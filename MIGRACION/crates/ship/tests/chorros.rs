//! Exhaust (`lunar_ship::exhaust`): every thruster of every ship has a nozzle whose mouth is
//! where its bell ends and whose gas leaves against its push; a nacelle that tilts takes its
//! mouth along; and what fires is what pushes the ship — nothing while it is still.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::structure::{Library, state::Structure};
use lunar_ship::{
    Ship, ShipLibrary, World,
    exhaust::{self, Firing},
    kind::resolve,
    ship::TICK,
};
use std::path::Path;

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

fn alone(lib: &Library, ships: &ShipLibrary, id: &str) -> (Structure, Ship) {
    let kind = ships.get(id).unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, glam::Quat::IDENTITY);
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &w(), 0.0);
    (s, ship)
}

#[test]
fn every_thruster_of_every_ship_has_a_nozzle() {
    let (_, ships) = kinds();
    for kind in &ships.kinds {
        let Some(f) = &kind.def.vuelo else { continue };
        let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
        let listed: Vec<u32> = f.rcs.iter().chain(&f.motores).flat_map(|p| resolve(&ids, p)).collect();
        assert!(!listed.is_empty(), "{}: sin propulsores", kind.id);
        for &m in &listed {
            let plan = &kind.machines[m as usize];
            let jet = plan.jet.as_ref().unwrap_or_else(|| panic!("{}: '{}' empuja y no tiene tobera", kind.id, plan.id));
            // the gas leaves against the push, from beyond the middle of its part
            assert!(jet.dir.dot(plan.thrust) < -0.999 && (jet.dir.length() - 1.0).abs() < 1e-4, "{} {}: sale hacia {:?} y empuja hacia {:?}", kind.id, plan.id, jet.dir, plan.thrust);
            assert!(jet.exit.dot(jet.dir) > 0.03, "{} {}: la boca en {:?}, no al final de la tobera", kind.id, plan.id, jet.exit);
            let across = (jet.exit - jet.dir * jet.exit.dot(jet.dir)).length();
            assert!(across < 0.02, "{} {}: la boca a {across:.3} m de su eje", kind.id, plan.id);
            assert!(jet.radius > 0.03 && jet.radius < 1.2 && jet.rated >= 100.0, "{} {}: boca de {:.3} m, {:.0} N", kind.id, plan.id, jet.radius, jet.rated);
        }
        // and nothing else has one
        let all: Vec<usize> = exhaust::thrusters(kind).map(|(m, _)| m).collect();
        assert_eq!(all.len(), listed.len(), "{}: toberas en máquinas que no son del vuelo", kind.id);
        let (engines, jets): (Vec<_>, Vec<_>) = exhaust::thrusters(kind).partition(|(m, _)| kind.machines[*m].def.modelo == "motor_cohete");
        let one = |v: &[(usize, &exhaust::Jet)]| v.first().map_or(String::new(), |(_, j)| format!("boca de {:.3} m a {:.2} m, {:.1} kN", j.radius, j.exit.dot(j.dir), j.rated / 1e3));
        eprintln!("{}: {} motores ({}), {} toberas de maniobra ({})", kind.id, engines.len(), one(&engines), jets.len(), one(&jets));
    }
    // the Abejorro's jets are cold gas, by its data; the others say nothing (their model's style)
    let style = |ship: &str, machine: &str| ships.get(ship).unwrap().machines.iter().find(|m| m.id == machine).unwrap().jet.as_ref().unwrap().style.clone();
    assert_eq!(style("abejorro", "rcs_proa_izq_fuera").as_deref(), Some("gas_frio"));
    assert_eq!(style("abejorro", "rcs_popa_der_popa").as_deref(), Some("gas_frio"));
    assert_eq!(style("abejorro", "motor_proa_izq"), None);
    assert_eq!(style("alcotan", "gondola_der"), None);
    assert_eq!(style("alcotan", "rcs_proa_izq_arriba"), None);
}

#[test]
fn a_nacelles_mouth_is_the_end_of_its_bell_and_tilts_with_it() {
    let (lib, ships) = kinds();
    let (mut s, mut ship) = alone(&lib, &ships, "alcotan");
    let m = ship.kind.machines.iter().position(|m| m.id == "gondola_izq").unwrap();
    let jet = ship.kind.machines[m].jet.clone().unwrap();
    // the bell's rim: 0.5 m widened by a quarter, 2.07 m behind the middle of the nacelle
    assert!((jet.radius - 0.625).abs() < 0.03 && (jet.exit.dot(jet.dir) - 2.075).abs() < 0.03, "boca de {:.3} m a {:.3} m", jet.radius, jet.exit.dot(jet.dir));
    assert!((jet.rated - 48_000.0).abs() < 1.0);
    let pivot = Vec3::new(5.05, 1.0, -2.5);
    // as it stands, its nacelles up: the gas leaves downward, from under the pivot
    let up = exhaust::thruster(&ship, &s, m).unwrap();
    assert!(up.dir.dot(Vec3::NEG_Y) > 0.999 && (up.at - (pivot - Vec3::Y * 2.075)).length() < 0.05, "{up:?}");
    assert_eq!((up.thrust, up.level), (0.0, 0.0));
    // level: it leaves aft, from behind it
    ship.set_joint("gondola_izq", 0.0);
    ship.update(&mut s, &w(), TICK);
    let aft = exhaust::thruster(&ship, &s, m).unwrap();
    assert!(aft.dir.dot(Vec3::NEG_Z) > 0.999 && (aft.at - (pivot - Vec3::Z * 2.075)).length() < 0.05, "{aft:?}");
    // half way: half way
    ship.set_joint("gondola_izq", -std::f64::consts::FRAC_PI_4);
    ship.update(&mut s, &w(), TICK);
    let mid = exhaust::thruster(&ship, &s, m).unwrap();
    assert!(mid.dir.dot(Vec3::new(0.0, -1.0, -1.0).normalize()) > 0.999, "{mid:?}");
    // its part gone: it gives nothing
    let p = ship.machines[m].part.unwrap() as usize;
    s.parts[p].alive = false;
    assert_eq!(exhaust::thruster(&ship, &s, m).unwrap().thrust, 0.0);
    // a machine that does not push is no thruster
    let battery = ship.kind.machines.iter().position(|m| m.def.modelo == "bateria").unwrap();
    assert!(exhaust::thruster(&ship, &s, battery).is_none());
}

#[test]
fn what_fires_is_what_pushes_and_nothing_fires_in_a_ship_that_is_still() {
    let (lib, ships) = kinds();
    let (mut s, mut ship) = alone(&lib, &ships, "abejorro");
    let mut firing: Vec<Firing> = Vec::new();
    // standing with everything off: nothing
    s.grounded = true;
    run(&mut s, &mut ship, 5.0);
    exhaust::firing(&ship, &s, &mut firing);
    assert!(firing.is_empty() && s.force == Vec3::ZERO, "{firing:?}");
    // its four engines lit and at full throttle
    for id in ["consola/tapa_arm", "consola/arm", "consola/arr", "consola/arr"] {
        press(&mut ship, &mut s, id);
    }
    run(&mut s, &mut ship, 3.0);
    let k = ship.panels.controls.iter().position(|c| c.id == "consola/acelerador").unwrap();
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Set { value: 1.0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 2.0);
    exhaust::firing(&ship, &s, &mut firing);
    let engines: Vec<&Firing> = firing.iter().filter(|f| kind.machines[f.machine as usize].def.modelo == "motor_cohete").collect();
    assert_eq!(engines.len(), 4, "{firing:?}");
    for f in &engines {
        assert!(f.level > 0.6 && f.level < 1.3 && f.dir.dot(Vec3::NEG_Y) > 0.99 && f.at.y < -0.4, "{f:?}");
    }
    // the gas of all that fires, the other way, is the push on the hull
    let push = |firing: &[Firing]| firing.iter().map(|f| -f.dir * f.thrust).sum::<Vec3>();
    assert!((push(&firing) - s.force).length() < s.force.length() * 0.01 + 1.0, "{:?} contra {:?}", push(&firing), s.force);
    eprintln!("abejorro a todo gas: {} propulsores, {:.1} kN", firing.len(), s.force.length() / 1e3);
    // off the ground, sliding sideways on its cold gas: some of its nozzles fire, outboard
    s.grounded = false;
    run(&mut s, &mut ship, 2.0);
    let k = ship.panels.controls.iter().position(|c| c.id == "consola/traslacion").unwrap();
    ship.panels.intent(k, &Intent::Axis { axis: 1, value: 1.0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 0.5);
    exhaust::firing(&ship, &s, &mut firing);
    let jets: Vec<&Firing> = firing.iter().filter(|f| kind.machines[f.machine as usize].def.modelo == "rcs").collect();
    assert!(!jets.is_empty() && jets.iter().all(|f| f.level > 0.0 && f.level <= 1.01 && f.thrust <= 501.0), "{jets:?}");
    assert!(jets.iter().any(|f| f.dir.x.abs() > 0.99), "ninguna tobera hacia fuera: {jets:?}");
    assert!((push(&firing) - s.force).length() < s.force.length() * 0.01 + 1.0, "{:?} contra {:?}", push(&firing), s.force);
    eprintln!("deslizando de lado: {} toberas de gas frío, {:?}", jets.len(), jets.iter().map(|f| (kind.machines[f.machine as usize].id.as_str(), (f.level * 100.0).round())).collect::<Vec<_>>());
}
