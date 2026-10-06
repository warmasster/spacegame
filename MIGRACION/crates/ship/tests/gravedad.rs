//! The gravity a ship makes for whoever it carries (`ShipDef::gravedad`), for every ship of the
//! library, whichever they are: what its data says it makes is what its structure says it makes
//! once its systems run, in its rooms, while what it runs on is there — and it comes and goes
//! over its time, never at a stroke. A ship that makes none says so, and what is aboard it then
//! weighs what is left of the world's pull as it goes (`lunar_core::structure::weight`).
mod comun;

use comun::fleet;
use glam::{DVec3, Vec3};
use lunar_core::structure::weight;

#[test]
fn every_ship_gives_the_gravity_its_data_says_while_what_feeds_it_is_there() {
    let (mut with, mut without) = (0, 0);
    for kind in &fleet().kinds {
        let mut r = fleet().rig(kind);
        let gd = &kind.def.gravedad;
        // as it is made: none yet, and its inside is not known to its structure
        assert_eq!(r.s.gravity.on, 0.0, "{}: tiene gravedad antes de que sus sistemas corran", r.id());
        r.tick();
        // its rooms: its compartments, as its structure has them from the first tick
        assert_eq!(r.s.rooms.len(), kind.compartments.iter().map(|c| c.boxes.len()).sum::<usize>(), "{}", r.id());
        for c in &kind.compartments {
            for [lo, hi] in &c.boxes {
                assert!(r.s.in_rooms((*lo + *hi) * 0.5), "{}: el centro de {} no está en sus salas", r.id(), c.id);
            }
        }
        assert!(!r.s.in_rooms(Vec3::new(0.0, 900.0, 0.0)));
        if gd.g <= 0.0 {
            // it makes none: never, whatever runs
            r.run(5.0);
            assert_eq!((r.s.gravity.g, r.s.gravity.on), (0.0, 0.0), "{}", r.id());
            without += 1;
            continue;
        }
        with += 1;
        let Some(name) = &gd.senal else {
            r.run(f64::from(gd.tiempo) + 1.0);
            assert_eq!((r.s.gravity.g, r.s.gravity.on), (gd.g, 1.0), "{}", r.id());
            continue;
        };
        let signal = r.ship.store.find(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", r.id()));
        // run until what feeds it is there (or it never is: said, not hidden)
        let fed = r.until(120.0, |r| r.ship.store.on(signal));
        eprintln!("{}: gravedad propia de {} m/s² en {} s, según '{name}' (al ponerla en marcha: {})", r.id(), gd.g, gd.tiempo, if fed { "funciona" } else { "no hay con qué" });
        if !fed {
            assert_eq!(r.s.gravity.on, 0.0, "{}: da gravedad sin lo que la alimenta", r.id());
            continue;
        }
        // it comes on over its time: never more in a tick than its time allows, and all of it
        // once that has gone by
        let (mut last, mut worst) = (r.s.gravity.on, 0.0_f32);
        for _ in 0..((f64::from(gd.tiempo) + 1.0) / lunar_ship::ship::TICK) as usize {
            r.tick();
            worst = worst.max((r.s.gravity.on - last).abs());
            last = r.s.gravity.on;
        }
        assert_eq!((r.s.gravity.g, r.s.gravity.on), (gd.g, 1.0), "{}", r.id());
        assert!(worst <= lunar_ship::ship::TICK as f32 / gd.tiempo * 1.01, "{}: su gravedad cambia {worst:.3} de golpe", r.id());
        // what is in its rooms weighs that toward its decks, however it lies and whatever the
        // world pulls with; what is on it outside them, what is left of the pull
        let room = kind.compartments[0].boxes[0];
        let inside = r.s.to_world((room[0] + room[1]) * 0.5);
        let outside = r.s.to_world(Vec3::new(0.0, 900.0, 0.0));
        for pull in [DVec3::ZERO, DVec3::new(0.0, -1.62, 0.0), DVec3::new(3.0, 0.5, -2.0)] {
            let w = weight::felt(pull, inside, Some(&r.s), true, true);
            assert!((w - (r.s.rot * Vec3::NEG_Y).as_dvec3() * f64::from(gd.g)).length() < 1e-6, "{}: dentro pesa {w:?}", r.id());
            assert!((weight::felt(pull, outside, Some(&r.s), true, false) - pull).length() < 1e-9);
        }
    }
    assert!(with > 0 && without > 0, "la biblioteca ya no tiene naves con gravedad y sin ella: {with} y {without}");
}

#[test]
fn aboard_what_makes_none_one_weighs_what_is_left_of_the_pull_as_it_goes() {
    for kind in fleet().kinds.iter().filter(|k| k.def.gravedad.g <= 0.0) {
        let mut r = fleet().rig(kind);
        r.run(1.0);
        let at = r.s.to_world(r.s.com);
        let pull = DVec3::new(0.0, -1.62, 0.0);
        // standing (it does not speed up): all of the pull
        r.s.acc = DVec3::ZERO;
        assert!((weight::felt(pull, at, Some(&r.s), true, false) - pull).length() < 1e-12, "{}", r.id());
        // falling free: nothing
        r.s.acc = pull;
        assert!(weight::felt(pull, at, Some(&r.s), true, false).length() < 1e-12, "{}", r.id());
        // under thrust: its push the other way
        r.s.acc = pull + DVec3::new(0.0, 0.0, 5.0);
        assert!((weight::felt(pull, at, Some(&r.s), true, false) - DVec3::new(0.0, 0.0, -5.0)).length() < 1e-12, "{}", r.id());
        // (and turning: thrown out from its axis, as far out as one is)
        r.s.acc = DVec3::ZERO;
        r.s.spin = Vec3::new(0.0, 0.5, 0.0);
        let out = at + DVec3::new(4.0, 0.0, 0.0);
        let w = weight::felt(DVec3::ZERO, out, Some(&r.s), true, false);
        assert!((w - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-6, "{}: girando, a 4 m del eje pesa {w:?}", r.id());
    }
}
