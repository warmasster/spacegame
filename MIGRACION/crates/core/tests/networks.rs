//! Networks over the joints on the real builds: power reaches every consumer through the cables;
//! short of it, the higher priorities keep theirs; a part cut off goes dark; a battery lasts its
//! capacity over the load, also across a long sleep caught up at once.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    structure::{Library, schedule::DistancePolicy, set::Structures, state::Structure},
};
use std::{path::Path, sync::Arc};

fn world(build: &str) -> (BodyRegistry, Structures, DVec3) {
    let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let mut set = Structures::new(lib);
    set.place(build, &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let eye = bodies.get(0).above_ground(DVec3::Y, 2.0);
    (bodies, set, eye)
}

fn part(set: &Structures, build: &str, name: &str) -> usize {
    set.lib.blueprint(build).unwrap().ids.iter().position(|n| n == name).unwrap_or_else(|| panic!("no part {name}"))
}

/// Run `secs` of simulation from `t` at 20 Hz.
fn run(set: &mut Structures, bodies: &BodyRegistry, eye: DVec3, t: &mut f64, secs: f64) {
    let policy = DistancePolicy::default();
    let end = *t + secs;
    while *t < end {
        *t += 0.05;
        set.simulate(*t, 0.05, bodies, &policy, &[eye]);
    }
}

fn lit(s: &Structure, i: usize) -> bool {
    s.parts[i].supplied >= 0.5
}

#[test]
fn short_of_power_the_lamps_go_first() {
    let (bodies, mut set, eye) = world("estacion");
    let up = bodies.get(0).up(set.list[0].pos);
    let mut t = 0.0;
    set.sun = up;
    run(&mut set, &bodies, eye, &mut t, 1.0);
    let (hab, lamp, window) = (part(&set, "estacion", "hab_a"), part(&set, "estacion", "foco#0"), part(&set, "estacion", "ventana_a"));
    assert!(lit(&set.list[0], hab) && lit(&set.list[0], lamp) && lit(&set.list[0], window), "everything on with the reactor");
    // reactor gone, the sun low: the panels give less than everything asks
    let reactor = part(&set, "estacion", "reactor");
    set.list[0].parts[reactor].alive = false;
    let side = up.any_orthonormal_vector();
    set.sun = (up + side).normalize();
    run(&mut set, &bodies, eye, &mut t, 1.0);
    let s = &set.list[0];
    assert!(lit(s, hab), "the habitats keep theirs ({})", s.parts[hab].supplied);
    assert!(!lit(s, lamp) && !lit(s, window), "the lamps go dark ({})", s.parts[lamp].supplied);
    // night: nothing left
    set.sun = -up;
    run(&mut set, &bodies, eye, &mut t, 1.0);
    assert!(!lit(&set.list[0], hab));
}

#[test]
fn a_lamp_cut_off_goes_dark_alone() {
    let (bodies, mut set, eye) = world("estacion");
    set.sun = bodies.get(0).up(set.list[0].pos);
    let mut t = 0.0;
    let (a, b) = (part(&set, "estacion", "foco#0") as u32, part(&set, "estacion", "foco#1"));
    for j in set.list[0].joints.iter_mut().filter(|j| j.a == a || j.b == a) {
        j.alive = false;
    }
    run(&mut set, &bodies, eye, &mut t, 0.5);
    assert!(!lit(&set.list[0], a as usize) && lit(&set.list[0], b));
}

#[test]
fn a_battery_lasts_its_capacity_over_the_load_even_asleep() {
    let (bodies, mut set, eye) = world("torre");
    let battery = part(&set, "torre", "bateria");
    let lamp = part(&set, "torre", "foco");
    let mut t = 0.0;
    run(&mut set, &bodies, eye, &mut t, 1.0);
    assert!(lit(&set.list[0], lamp));
    let load = 0.3 + 6.0 * 0.05;
    let level = set.list[0].parts[battery].store;
    assert!((level - (200_000.0 - load)).abs() < 1.0, "after 1 s: {level}");
    // far away for three days and a half, then back: the nap caught up at once
    let far = bodies.get(0).above_ground(DVec3::X, 2.0);
    let policy = DistancePolicy::default();
    t += 300_000.0;
    set.simulate(t, 0.05, &bodies, &policy, &[far]);
    set.simulate(t + 0.05, 0.05, &bodies, &policy, &[eye]);
    let level = set.list[0].parts[battery].store;
    assert!((level - (200_000.0 - load * 300_001.0)).abs() < 50.0, "after the nap: {level}");
    assert!(lit(&set.list[0], lamp));
    t += 0.05;
    run(&mut set, &bodies, eye, &mut t, 1.0);
    t += 50_000.0;
    set.simulate(t, 0.05, &bodies, &policy, &[far]);
    set.simulate(t + 0.05, 0.05, &bodies, &policy, &[eye]);
    assert!(set.list[0].parts[battery].store < 1.0 && !lit(&set.list[0], lamp), "flat by now");
}
