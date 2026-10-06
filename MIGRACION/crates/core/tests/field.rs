//! What holds at a point of space (`BodyRegistry::field`): a body's pull ends where its data says
//! and fades to nothing without a jolt; where two reaches meet one gives way to the other; and a
//! structure is ruled by where it is now — not by where it was made, how fast it goes or what
//! happened before. Swept over the bodies there are and one made up here, inside, across and
//! past each reach, from standing still to orbital speed.
use glam::{Affine3A, DVec3, Quat};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    structure::{
        Library,
        schedule::{DistancePolicy, coast},
        set::Structures,
        state::{Part, Structure},
    },
};
use std::{path::Path, sync::Arc};

/// The bodies of the game's data and one made up: another size, another gravity, another reach,
/// its axis another way, far from the others.
fn bodies() -> BodyRegistry {
    let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let moonlet: BodyDef = defs::parse("luna_menor", include_str!("../../../assets/defs/bodies/luna_menor.jsonc")).unwrap();
    let other: BodyDef = defs::parse("prueba", r#"{ "name": "Prueba", "center": [4.0e6, 2.5e6, -3.0e6], "radius": 300000, "gravity": 3.7, "reach": { "to": 90000, "band": 35000 }, "north": [0.3, 1.0, 0.2], "horizon_depth": 100 }"#).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &moon).unwrap(), Body::from_def("luna_menor", &moonlet).unwrap(), Body::from_def("prueba", &other).unwrap()])
}

/// A way out from the centre of body `k` along which no other body's reach is met (away from
/// the others).
fn clear_of_the_rest(bodies: &BodyRegistry, k: u16) -> DVec3 {
    let b = bodies.get(k);
    let others: DVec3 = bodies.iter().filter(|(i, _)| *i != k).map(|(_, o)| (o.center - b.center).normalize()).sum();
    (-others).normalize_or(DVec3::Y)
}

fn structures() -> Structures {
    Structures::new(Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap()))
}

fn block(set: &mut Structures, pos: DVec3) -> usize {
    let cat = &set.lib.catalog;
    let part = Part::new(cat, cat.part("bloque").unwrap(), Affine3A::IDENTITY, None, false);
    let id = set.next_id();
    set.list.push(Structure::assemble(id, "bloque".into(), pos, Quat::IDENTITY, false, vec![part], Vec::new()));
    set.list.len() - 1
}

#[test]
fn a_bodys_pull_ends_where_its_data_says_and_fades_without_a_jolt() {
    let bodies = bodies();
    for (k, b) in bodies.iter() {
        let dir = clear_of_the_rest(&bodies, k);
        let at = |alt: f64| b.center + dir * (b.radius + alt);
        // whole on the ground and as far up as its data says: the square of the distance
        for alt in [0.0, b.whole_to() * 0.5, b.whole_to()] {
            let f = bodies.field(at(alt));
            let law = b.gravity * (b.radius / (b.radius + alt)).powi(2);
            assert!((f.g() - law).abs() < 1e-9 * law, "{}: a {alt:.0} m tira {:.6} y la ley dice {law:.6}", b.name, f.g());
            assert!((f.pull.normalize() + dir).length() < 1e-9 && f.ground == Some(k) && (f.hold - 1.0).abs() < 1e-12);
            assert!((f.up().unwrap() - dir).length() < 1e-9);
        }
        // nothing from its reach on: no pull, no ground, no way up
        for alt in [b.reach, b.reach + 1.0, b.reach * 3.0] {
            let f = bodies.field(at(alt));
            assert!(f.pull == DVec3::ZERO && f.ground.is_none() && f.up().is_none() && f.hold == 0.0, "{}: a {alt:.0} m aún rige: {f:?}", b.name);
        }
        // (the previous rule: the square of the distance for ever. At the edge of its reach it
        // still pulled this much, and at three times that)
        let before = |alt: f64| b.gravity * (b.radius / (b.radius + alt)).powi(2);
        assert!(before(b.reach) > 0.5 * b.gravity * 0.5 && before(b.reach * 3.0) > 0.02, "{}", b.name);
        // across the band: down all the way, and smooth — how it changes per metre, and how
        // that changes, never jump (from nothing at both ends)
        let step = b.band / 4000.0;
        let g = |i: i64| bodies.field(at(b.whole_to() + step * i as f64)).g();
        let (mut worst_slope, mut worst_bend) = (0.0_f64, 0.0_f64);
        for i in -20..4020 {
            let (a, c, d) = (g(i), g(i + 1), g(i + 2));
            assert!(c <= a + 1e-15, "{}: sube en la franja", b.name);
            worst_slope = worst_slope.max((c - a).abs() / step);
            worst_bend = worst_bend.max((d - 2.0 * c + a).abs() / (step * step));
        }
        // (a fall of g over the band has a slope of the order of g / band, and bends of the
        // order of g / band²: nothing sharper than a few times that)
        let (slope, bend) = (b.gravity / b.band, b.gravity / (b.band * b.band));
        eprintln!("{}: en la franja la pendiente mayor es {:.2} veces g/franja y la curvatura mayor {:.2} veces g/franja²", b.name, worst_slope / slope, worst_bend / bend);
        assert!(worst_slope < 2.5 * slope && worst_bend < 8.0 * bend);
        // and it leaves and comes in from nothing: at each end of the band, no slope
        let end = |alt: f64| (bodies.field(at(alt + 0.5)).g() - bodies.field(at(alt - 0.5)).g()).abs();
        let law_slope = 2.0 * b.gravity / b.radius;
        assert!(end(b.reach) < 1e-9 && end(b.whole_to()) < 1.5 * law_slope, "{}: salto de pendiente en los extremos: {:.2e} y {:.2e}", b.name, end(b.reach), end(b.whole_to()));
    }
}

#[test]
fn where_two_reaches_meet_one_gives_way_to_the_other() {
    let bodies = bodies();
    let (moon, moonlet) = (bodies.get(0), bodies.get(1));
    // from the Moon's centre through the moonlet's: up from the Moon's ground, across the edge of
    // its reach, into the moonlet's from under it, through it and out the far side
    let dir = (moonlet.center - moon.center).normalize();
    let far = (moonlet.center - moon.center).length() + moonlet.radius + moonlet.reach + 5000.0;
    let (mut last, mut worst, mut t) = (bodies.field(moon.center + dir * moon.radius), 0.0_f64, moon.radius);
    let (mut both, mut only_moonlet, mut none) = (0, 0, 0);
    while t < far {
        t += 2.0;
        let p = moon.center + dir * t;
        // (not inside the moonlet itself)
        if (p - moonlet.center).length() < moonlet.radius {
            last = bodies.field(moon.center + dir * (t + 2.0));
            continue;
        }
        let f = bodies.field(p);
        worst = worst.max((f.pull - last.pull).length() / 2.0);
        let (near_moon, near_moonlet) = ((p - moon.center).length() - moon.radius < moon.reach, (p - moonlet.center).length() - moonlet.radius < moonlet.reach);
        match (near_moon, near_moonlet) {
            (true, true) => both += 1,
            (false, true) => only_moonlet += 1,
            (false, false) => none += 1,
            _ => {}
        }
        // where the moonlet's pull is whole it is the moonlet that holds, all of it, however
        // much of the Moon's reach is there too
        if (p - moonlet.center).length() - moonlet.radius <= moonlet.whole_to() {
            let own = (moonlet.center - p).normalize() * moonlet.own_pull((p - moonlet.center).length() - moonlet.radius);
            assert!((f.pull - own).length() < 1e-12, "junto a la luna menor tira algo más: {:?} y lo suyo es {own:?}", f.pull);
            assert_eq!(f.ground, Some(1));
        }
        assert!(f.hold <= 1.0 + 1e-12);
        last = f;
    }
    eprintln!("de la Luna a través de la luna menor: {both} puntos donde llegan las dos, {only_moonlet} solo de la menor, {none} de ninguna; el mayor cambio del tirón, {worst:.2e} m/s² por metro");
    assert!(both > 100 && only_moonlet > 100 && none > 100, "el camino no pasa por todo");
    // (the sharpest stretch is the moonlet's band on the side that looks at the Moon: there the
    // Moon's pull gives way to the moonlet's, the other way, over that band. A pull fading over
    // a band changes at most 1.875 times what it is per band; nothing changes faster than that)
    assert!(worst < 2.5 * (moon.gravity + moonlet.gravity) / moonlet.band, "el tirón salta: {worst:.2e} m/s² por metro");
}

#[test]
fn a_structure_is_ruled_by_where_it_is_now_not_by_where_it_was_made() {
    let bodies = bodies();
    // the same loose block, made over the Moon, then carried to each of the other bodies and
    // back: it falls toward whatever is under it there, as hard as that pulls
    let mut set = structures();
    let made = bodies.get(0).center + DVec3::Y * (bodies.get(0).radius + 3000.0);
    let k = block(&mut set, made);
    for body in [1u16, 2, 0, 1] {
        let b = bodies.get(body);
        let dir = clear_of_the_rest(&bodies, body);
        let from = b.center + dir * (b.radius + b.whole_to() * 0.4);
        let s = &mut set.list[k];
        (s.pos, s.vel, s.spin, s.resting) = (from, DVec3::ZERO, glam::Vec3::ZERO, false);
        for _ in 0..120 {
            set.step(1.0 / 60.0, &bodies);
        }
        let s = &set.list[k];
        let (fell, g) = ((from - s.pos).dot(dir), bodies.field(from).g());
        eprintln!("hecho sobre la Luna, puesto sobre {}: en 2 s cae {fell:.3} m hacia él (g = {g:.3}: {:.3} m)", b.name, 0.5 * g * 4.0);
        assert!((fell - 0.5 * g * 4.0).abs() < 0.02 * g + 1e-3, "sobre {} cae {fell:.3} m", b.name);
        assert!((s.acc + dir * g).length() < 1e-3 * g, "su aceleración no es la de ahí: {:?}", s.acc);
        // (and not sideways: with the body it was made at ruling it, it went toward the Moon)
        let aside = (s.pos - from + dir * fell).length();
        assert!(aside < 1e-3, "se va {aside:.3} m de lado");
        // the previous rule (the pull of the body it was made at, for ever): where it would be
        let moon = bodies.get(0);
        let old = (moon.center - from).normalize() * moon.gravity * (moon.radius / (from - moon.center).length()).powi(2);
        if body != 0 {
            assert!(((from + old * 2.0) - s.pos).length() > 0.05, "con la regla anterior quedaría donde mismo");
        }
    }
}

#[test]
fn past_every_reach_a_structure_keeps_its_speed_and_meets_no_ground() {
    let bodies = bodies();
    for body in 0..3u16 {
        let b = bodies.get(body);
        let dir = clear_of_the_rest(&bodies, body);
        let side = dir.any_orthonormal_vector();
        for speed in [0.0, 30.0, 1600.0, 7800.0] {
            // (made on the ground of the body, then found out there: nothing of that is kept)
            let mut set = structures();
            let k = block(&mut set, b.center + dir * (b.radius + 2.0));
            set.step(1.0 / 60.0, &bodies);
            let from = b.center + dir * (b.radius + b.reach * 1.2);
            let v = side * speed + dir * 3.0;
            let s = &mut set.list[k];
            (s.pos, s.vel, s.spin, s.resting) = (from, v, glam::Vec3::new(0.1, 0.0, 0.2), false);
            for _ in 0..300 {
                set.step(1.0 / 60.0, &bodies);
            }
            let s = &set.list[k];
            assert!((s.vel - v).length() < 1e-9 * (1.0 + speed), "{} a {speed} m/s: su velocidad cambia {:.3e} m/s", b.name, (s.vel - v).length());
            assert!((s.pos - (from + v * 5.0)).length() < 1e-6 * (1.0 + speed), "no va recto");
            assert!(!s.grounded && !s.resting && s.acc == DVec3::ZERO);
            // (the previous rule pulled it from here still: in those 5 s it would have gained)
            let before = b.gravity * (b.radius / (b.radius + b.reach * 1.2)).powi(2) * 5.0;
            assert!(before > 0.5, "{}: la regla anterior le habría dado {before:.2} m/s", b.name);
        }
    }
}

#[test]
fn a_sleeper_catches_up_as_it_is_pulled_where_it_is() {
    let bodies = bodies();
    for body in 0..3u16 {
        let b = bodies.get(body);
        let dir = clear_of_the_rest(&bodies, body);
        for (alt, pulled) in [(b.whole_to() * 0.5, true), (b.reach * 1.5, false)] {
            let mut set = structures();
            let k = block(&mut set, b.center + dir * (b.radius + alt));
            let s = &mut set.list[k];
            s.vel = dir.any_orthonormal_vector() * 100.0;
            let (from, v) = (s.pos, s.vel);
            coast(s, bodies.field(from).pull, 2.0);
            let fell = (from + v * 2.0 - s.pos).dot(dir);
            let want = if pulled { 0.5 * bodies.field(from).g() * 4.0 } else { 0.0 };
            assert!((fell - want).abs() < 1e-9 + 1e-9 * want, "{} a {alt:.0} m: cae {fell:.4} m, serían {want:.4}", b.name);
        }
    }
    // and through the world's own clock: far from every watcher it sleeps, and woken it is where
    // free flight under the pull of where it was would have taken it
    let b = bodies.get(1);
    let dir = clear_of_the_rest(&bodies, 1);
    let mut set = structures();
    let k = block(&mut set, b.center + dir * (b.radius + b.whole_to() * 0.5));
    let from = set.list[k].pos;
    let far = [from + dir * 1.0e6];
    let mut now = 0.0;
    for _ in 0..4 {
        now += 0.5;
        set.simulate(now, 0.5, &bodies, &DistancePolicy::default(), &far);
    }
    assert!(set.list[k].pos == from, "dormida y se mueve");
    now += 1.0 / 60.0;
    set.simulate(now, 1.0 / 60.0, &bodies, &DistancePolicy::default(), &[from]);
    let fell = (from - set.list[k].pos).dot(dir);
    let g = bodies.field(from).g();
    eprintln!("dormida 2 s junto a la luna menor y despertada: ha caído {fell:.3} m (g = {g:.3})");
    assert!((fell - 0.5 * g * now * now).abs() < 0.05 * g * now * now, "al despertar ha caído {fell:.3} m");
}

#[test]
fn north_is_the_bodys_own_and_is_none_at_its_poles() {
    let bodies = bodies();
    for (_, b) in bodies.iter() {
        // level everywhere, toward the pole, and less and less of it as the pole is neared
        let side = b.north.any_orthonormal_vector();
        let mut last = 1.0;
        for deg in [90.0_f64, 45.0, 10.0, 1.0, 0.01] {
            let up = (b.north * deg.to_radians().cos() + side * deg.to_radians().sin()).normalize();
            let (n, much) = b.north_at(up);
            assert!(n.dot(up).abs() < 1e-9 && (n.length() - 1.0).abs() < 1e-9 && n.dot(b.north) > 0.0);
            assert!((much - deg.to_radians().sin()).abs() < 1e-9 && much <= last);
            last = much;
        }
        let (n, much) = b.north_at(b.north);
        assert!(n == DVec3::ZERO && much == 0.0);
        // (what only needs a level way to count a turn from gets one even there)
        let any = b.turn_from(b.north);
        assert!(any.dot(b.north).abs() < 1e-9 && (any.length() - 1.0).abs() < 1e-9);
    }
}

#[test]
fn asking_what_holds_somewhere_costs_next_to_nothing() {
    // (what every body, round and particle asks every slice: a square root and a few products
    // per body of the system; nothing allocated)
    let bodies = bodies();
    let moon = bodies.get(0);
    let n = 2_000_000;
    let mut sum = DVec3::ZERO;
    let t = std::time::Instant::now();
    for i in 0..n {
        let k = f64::from(i);
        // (on the ground, in the band, past every reach, by the moonlet: all of it)
        let p = moon.center + DVec3::new((k * 0.37).sin(), 1.0, (k * 0.11).cos()).normalize() * (moon.radius + (k * 0.731) % 80_000.0);
        sum += bodies.field(p).pull;
    }
    let each = t.elapsed().as_secs_f64() / f64::from(n) * 1e9;
    eprintln!("field(): {each:.0} ns por consulta con {} cuerpos ({sum:.1?})", bodies.len());
    // (far over what it takes: it only fails if someone makes it walk something)
    assert!(each < 2000.0, "{each:.0} ns por consulta");
}
