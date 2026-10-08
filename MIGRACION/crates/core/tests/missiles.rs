//! Long-range missiles: solved launches land where aimed a hundred kilometres away; a strike on a
//! sleeping structure wakes it in time and wrecks it; every flight is the same.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    missiles::{MissileDef, Missiles, Strike, Target},
    structure::{Library, breakup::Rules, schedule::SimLevel, set::Structures, state::Structure},
};
use std::{path::Path, sync::Arc};

fn moon() -> BodyRegistry {
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
}

fn missiles() -> Missiles {
    Missiles::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/missiles.jsonc")).unwrap()
}

fn away(bodies: &BodyRegistry, km: f64) -> DVec3 {
    (DVec3::Y + DVec3::new(0.6, 0.0, -0.8) * (km * 1000.0 / bodies.get(0).radius)).normalize()
}

/// Fly until everything lands (fixed frames), every structure asleep but for what wakes it; the
/// strikes, and the structure levels seen.
fn fly(m: &mut Missiles, bodies: &BodyRegistry, set: &mut Structures, mut each: impl FnMut(f64, &Structures, &Missiles)) -> Vec<Strike> {
    let policy = |_: &Structure| SimLevel::Dormant;
    let mut strikes = Vec::new();
    let mut t = 0.0;
    while !m.list.is_empty() && t < 1200.0 {
        t += 1.0 / 30.0;
        m.update(1.0 / 30.0, bodies, set, &mut strikes);
        set.simulate(t, 1.0 / 30.0, bodies, &policy);
        each(t, set, m);
    }
    strikes
}

#[test]
fn a_hundred_kilometres_on_target() {
    let bodies = moon();
    let b = bodies.get(0);
    let from = b.above_ground(DVec3::Y, 2.0);
    let to = b.above_ground(away(&bodies, 100.0), 0.0);
    let mut m = missiles();
    m.launch("misil", from, to, &bodies).unwrap();
    let mut set = Structures::new(Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap()));
    let strikes = fly(&mut m, &bodies, &mut set, |_, _, _| {});
    assert_eq!(strikes.len(), 1);
    let miss = strikes[0].at.distance(to);
    println!("100 km shot: lands {miss:.1} m from the aim at {:.0} m/s", strikes[0].vel.length());
    assert!(miss < 60.0, "missed by {miss} m");
    assert_eq!(strikes[0].target, Target::Ground);
}

#[test]
fn a_far_station_wakes_before_the_strike_and_is_wrecked() {
    let bodies = moon();
    let b = bodies.get(0);
    let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
    let rules = Rules::standard(&lib.catalog).unwrap();
    let run = || {
        let mut set = Structures::new(lib.clone());
        set.place("estacion", &bodies, 0, away(&bodies, 100.0), 0.0, 0.0).unwrap();
        let id = set.list[0].id;
        let from = b.above_ground(DVec3::Y, 2.0);
        let aim = set.list[0].to_world(set.list[0].center);
        let mut m = missiles();
        m.launch("misil", from, aim, &bodies).unwrap();
        let mut woke = None;
        let mut flight = 0.0;
        let strikes = fly(&mut m, &bodies, &mut set, |t, set, m| {
            if woke.is_none() && set.list[0].sim == SimLevel::Active {
                woke = Some(t);
            }
            if let Some(x) = m.list.first() {
                flight = x.t;
            }
        });
        (set, id, strikes, woke, flight)
    };
    let (mut set, id, strikes, woke, flight) = run();
    assert_eq!(strikes.len(), 1);
    assert_eq!(strikes[0].target, Target::Structure(id), "strike at {:?}", strikes[0].at);
    let woke = woke.expect("the station woke");
    println!("far strike: woke at {woke:.1} s, struck at {flight:.1} s, {:.2e} J", strikes[0].energy);
    assert!(flight - woke > 2.0, "woke only {:.2} s before", flight - woke);
    // the warhead and the speed, on a station no one is near
    let alive = set.list[0].parts.iter().filter(|p| p.alive).count();
    set.blast(strikes[0].at, 2.0e8 + strikes[0].energy as f32 * 0.5, 45.0, &rules, &mut Vec::new(), 1);
    assert!(set.list[0].parts.iter().filter(|p| p.alive).count() < alive);
    // the same flight lands on the same millimetre
    let again = run();
    assert_eq!(again.2[0].at, strikes[0].at);
}

#[test]
fn out_of_reach_is_refused() {
    let bodies = moon();
    let b = bodies.get(0);
    let mut m = Missiles::new(vec![("corto".into(), MissileDef { name: "Corto".into(), key: None, angle: 45.0, max_speed: 50.0, mass: 1.0, warhead: Some("granada".into()), charge: None, look: None, trail: None, trail_rate: 0.0 })]);
    assert!(m.launch("corto", b.above_ground(DVec3::Y, 2.0), b.above_ground(away(&bodies, 100.0), 0.0), &bodies).is_err());
}
