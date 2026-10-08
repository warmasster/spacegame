//! Simulation levels: each structure runs at the level its policy gives it from what it is (every
//! frame, coarsely or not at all); a sleeper that wakes is where it would have been had it run all
//! along; hits wake sleepers; and what lives among structures (someone floating by) changes
//! nothing of them: the world is the same whoever is in it.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    structure::{
        Library,
        breakup::Rules,
        schedule::{Full, SimLevel},
        set::Structures,
        state::Structure,
    },
};
use std::{path::Path, sync::Arc};

fn world() -> (BodyRegistry, Structures) {
    let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    (BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]), Structures::new(lib))
}

fn along(bodies: &BodyRegistry, m: f64) -> DVec3 {
    (DVec3::Y + DVec3::X * (m / bodies.get(0).radius)).normalize()
}

#[test]
fn each_runs_at_its_level_and_sleepers_cost_nothing() {
    let (bodies, mut set) = world();
    for m in [100.0, 5_000.0, 50_000.0] {
        set.place("modulo_lunar", &bodies, 0, along(&bodies, m), 0.0, 0.0).unwrap();
    }
    // (the first, the second and the third as a policy might have them)
    let first = set.list[0].id;
    let policy = move |s: &Structure| match s.id - first {
        0 => SimLevel::Active,
        1 => SimLevel::Coarse,
        _ => SimLevel::Dormant,
    };
    let mut steps = 0;
    for f in 1..=60 {
        let st = set.simulate(f64::from(f) / 60.0, 1.0 / 60.0, &bodies, &policy);
        assert_eq!((st.active, st.coarse, st.dormant), (1, 1, 1));
        steps += st.steps;
    }
    let levels: Vec<SimLevel> = set.list.iter().map(|s| s.sim).collect();
    assert_eq!(levels, [SimLevel::Active, SimLevel::Coarse, SimLevel::Dormant]);
    // one every frame, one four times a second, none
    assert!((63..=66).contains(&steps), "{steps} steps");
}

#[test]
fn a_sleeper_in_flight_wakes_where_it_would_have_been() {
    let (bodies, mut set) = world();
    let b = bodies.get(0);
    let dir = along(&bodies, 20_000.0);
    for _ in 0..2 {
        set.place("modulo_lunar", &bodies, 0, dir, 0.0, 300.0).unwrap();
    }
    for s in &mut set.list {
        s.anchored = false;
        s.resting = false;
        s.vel = b.up(s.pos).any_orthonormal_vector() * 12.0;
    }
    let (mut a, mut c) = (set.list.remove(0), set.list.remove(0));
    a.id = 1;
    c.id = 2;
    let mut always = Structures::new(set.lib.clone());
    always.list.push(a);
    let mut sleeper = Structures::new(set.lib.clone());
    sleeper.list.push(c);
    for f in 1..=360 {
        let now = f64::from(f) / 60.0;
        always.simulate(now, 1.0 / 60.0, &bodies, &Full);
        // asleep for the first five seconds, then awake
        let level = if now < 5.0 { SimLevel::Dormant } else { SimLevel::Active };
        let st = sleeper.simulate(now, 1.0 / 60.0, &bodies, &move |_: &Structure| level);
        if now < 5.0 {
            assert_eq!(st.dormant, 1);
        }
    }
    let (p, q) = (always.list[0].pos, sleeper.list[0].pos);
    println!("after 6 s: {:.3} m apart, {:.1} m up", p.distance(q), b.altitude(p));
    assert!(p.distance(q) < 0.25, "{} m apart", p.distance(q));
    assert!((always.list[0].vel - sleeper.list[0].vel).length() < 0.05);
}

#[test]
fn a_hit_wakes_a_sleeper() {
    let (bodies, mut set) = world();
    let dir = along(&bodies, 50_000.0);
    set.place("estacion", &bodies, 0, dir, 0.0, 0.0).unwrap();
    let policy = |_: &Structure| SimLevel::Dormant;
    set.simulate(1.0, 1.0 / 60.0, &bodies, &policy);
    assert_eq!(set.list[0].sim, SimLevel::Dormant);
    let rules = Rules::standard(&set.lib.catalog).unwrap();
    let at = set.list[0].to_world(set.list[0].center);
    set.blast(at, 5.0e6, 10.0, &rules, &mut Vec::new(), 1);
    set.simulate(1.0 + 1.0 / 60.0, 1.0 / 60.0, &bodies, &policy);
    assert!(set.list.iter().all(|s| s.sim == SimLevel::Active), "everything the blast touched runs");
}

/// A point falling free beside a structure, stepped by the world as what lives among structures is.
struct Beside {
    at: DVec3,
    vel: DVec3,
    /// The structure it goes with, how far from it it started, and the farthest it has been
    /// from there at any slice (m); the slices taken and their time.
    with: u64,
    from: DVec3,
    worst: f64,
    slices: usize,
    time: f64,
}

impl lunar_core::structure::schedule::Among for Beside {
    fn slice(&mut self, set: &Structures, bodies: &BodyRegistry, dt: f64) {
        self.vel += bodies.field(self.at).pull * dt;
        self.at += self.vel * dt;
        let s = set.get(self.with).unwrap();
        self.worst = self.worst.max((self.at - s.to_world(s.com) - self.from).length());
        self.slices += 1;
        self.time += dt;
    }
}

#[test]
fn what_lives_among_structures_is_of_their_instant_at_every_slice() {
    // a structure let go high up, going fast, and a point beside it going the same: at every
    // slice the world takes, at any speed and with frames of any length, the point is where it
    // was beside the structure (both fall the same). Stepped on a clock of its own it would be
    // up to a frame of the structure's speed off.
    for speed in [0.0, 300.0, 1600.0, 7800.0] {
        for frames in [&[1.0 / 240.0][..], &[1.0 / 60.0], &[1.0 / 30.0], &[0.1], &[0.0069, 0.0111, 0.02, 0.0167, 0.0143, 0.009, 0.025, 0.0167, 0.05]] {
            let (bodies, mut set) = world();
            let b = bodies.get(0);
            // (where the Moon pulls: its pull is whole up to 20 km)
            let at = b.above_ground(DVec3::Y, 12_000.0);
            let id = set.spawn("modulo_lunar", at, glam::Quat::IDENTITY).unwrap();
            set.list[0].vel = DVec3::X * speed;
            let com = set.list[0].to_world(set.list[0].com);
            let from = DVec3::new(3.0, 25.0, -2.0);
            let mut p = Beside { at: com + from, vel: set.list[0].vel, with: id, from, worst: 0.0, slices: 0, time: 0.0 };
            let (mut now, mut k) = (0.0, 0);
            while now < 4.0 {
                let dt = frames[k % frames.len()];
                (now, k) = (now + dt, k + 1);
                set.simulate_with(now, dt, &bodies, &Full, &mut [&mut p]);
            }
            assert!((p.time - now).abs() < 1e-6, "{speed} m/s: {} s in slices for {now} s of frames", p.time);
            assert!(p.slices >= k, "{speed} m/s: {} slices in {k} frames", p.slices);
            assert!(p.worst < 0.05, "{speed} m/s, frames of {:.1} ms: the point is {:.3} m off the structure at some slice", frames[0] * 1e3, p.worst);
            assert!(set.list[0].acc.length() > 1.5, "the structure is not falling: the test is not of a fall");
        }
    }
}

#[test]
fn what_lives_among_structures_changes_nothing_of_them_it_does_not_touch() {
    // the same structure falling fast, alone and with someone floating by at 100 m, 5 km and
    // 500 km, with frames of every length: at the end it is where it is to the bit, going as it
    // goes. Who is in the world, and where, is no part of what happens to what they do not touch.
    for speed in [0.0, 1600.0, 7800.0] {
        let frames = [0.0069, 0.0111, 0.02, 0.0167, 0.0143, 0.009, 0.025, 0.0167, 0.05];
        let run = |away: Option<f64>| {
            let (bodies, mut set) = world();
            let b = bodies.get(0);
            let at = b.above_ground(DVec3::Y, 12_000.0);
            let id = set.spawn("modulo_lunar", at, glam::Quat::IDENTITY).unwrap();
            set.list[0].vel = DVec3::X * speed;
            let com = set.list[0].to_world(set.list[0].com);
            let from = DVec3::new(0.0, 0.0, away.unwrap_or(0.0));
            let mut p = Beside { at: com + from, vel: set.list[0].vel, with: id, from, worst: 0.0, slices: 0, time: 0.0 };
            let (mut now, mut k) = (0.0, 0);
            while now < 4.0 {
                let dt = frames[k % frames.len()];
                (now, k) = (now + dt, k + 1);
                if away.is_some() {
                    set.simulate_with(now, dt, &bodies, &Full, &mut [&mut p]);
                } else {
                    set.simulate(now, dt, &bodies, &Full);
                }
            }
            let s = &set.list[0];
            (s.pos, s.rot, s.vel, s.spin)
        };
        let alone = run(None);
        for away in [100.0, 5_000.0, 500_000.0] {
            assert!(run(Some(away)) == alone, "{speed} m/s: with someone {away} m off, the structure is not where it is alone");
        }
    }
}
