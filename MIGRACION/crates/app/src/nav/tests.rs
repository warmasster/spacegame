//! The references and the compass: what must not matter does not, and nothing on the tape ever
//! jumps. Swept over the bodies there are and one made up here, on the ground, round them and
//! past their reach, across their poles and from one regime to another, turning all the way
//! round and with one's own way up any way at all.
use super::*;
use lunar_core::body::{BodyDef, BodyId};

fn worlds() -> BodyRegistry {
    let moon: BodyDef = defs::parse("luna", include_str!("../../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let moonlet: BodyDef = defs::parse("luna_menor", include_str!("../../../../assets/defs/bodies/luna_menor.jsonc")).unwrap();
    let other: BodyDef = defs::parse("prueba", r#"{ "name": "Prueba", "center": [4.0e6, 2.5e6, -3.0e6], "radius": 300000, "gravity": 3.7, "reach": { "to": 90000, "band": 35000 }, "north": [0.3, 1.0, 0.2], "horizon_depth": 100 }"#).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &moon).unwrap(), Body::from_def("luna_menor", &moonlet).unwrap(), Body::from_def("prueba", &other).unwrap()])
}

fn nav() -> Nav {
    Nav::load(&crate::root().join("assets/defs/navegacion.jsonc")).unwrap()
}

fn clear(bodies: &BodyRegistry, k: BodyId) -> DVec3 {
    let b = bodies.get(k);
    let others: DVec3 = bodies.iter().filter(|(i, _)| *i != k).map(|(_, o)| (o.center - b.center).normalize()).sum();
    (-others).normalize_or(DVec3::Y)
}

/// A ship to go by: a structure of the library at `at`, turned any way.
fn ship(at: DVec3) -> Structure {
    let lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
    let mut s = Structure::new(1, &lib.blueprints[0].1, &lib.catalog, at, glam::Quat::from_euler(glam::EulerRot::YXZ, 0.7, -0.4, 1.9));
    s.name = "Alcotán".into();
    s
}

/// `v` laid level where `up` is the way up.
fn level(v: DVec3, up: DVec3) -> DVec3 {
    (v - up * v.dot(up)).try_normalize().unwrap_or_else(|| up.any_orthonormal_vector())
}

/// What is seen of the compass from one moment to the next: the farthest anything on the tape
/// goes at a stroke (degrees of tape, weighed by how solid it is: a ghost of a mark may move),
/// and the most anything changes in how solid it is. Marks are told apart by what they say.
#[derive(Default)]
struct Seen {
    was: Vec<(String, f32, f32)>,
    was_scales: Vec<(String, f32, f32)>,
    moved: f32,
    faded: f32,
    /// The figures under the pointer changed which scale they read while they were showing.
    switched: f32,
    main: Option<String>,
}

impl Seen {
    fn look(&mut self, c: &Compass) {
        // (a mark that says how far something is is the same mark as the figures change)
        let now: Vec<(String, f32, f32)> = c.marks().iter().map(|m| (m.text.chars().filter(|ch| !ch.is_ascii_digit() && *ch != '.').collect(), m.shown().0, m.shown().1)).collect();
        let scales: Vec<(String, f32, f32)> = c.scales().iter().map(|s| (s.name.clone(), s.heading, s.alpha)).collect();
        let pair = |a: &[(String, f32, f32)], b: &[(String, f32, f32)], moved: &mut f32, faded: &mut f32, round: bool| {
            let mut left: Vec<&(String, f32, f32)> = b.iter().collect();
            for m in a {
                // (two bodies may each mark a north: each goes with the nearest of its name, and
                // of two at the same place with the one as solid as it)
                let apart = |x: f32, y: f32| if round { ((x - y + 180.0).rem_euclid(360.0) - 180.0).abs() } else { (x - y).abs() };
                let unlike = |o: &(String, f32, f32)| apart(o.1, m.1) + (o.2 - m.2).abs();
                match (0..left.len()).filter(|&i| left[i].0 == m.0).min_by(|&i, &j| unlike(left[i]).total_cmp(&unlike(left[j]))) {
                    Some(i) => {
                        let o = left.swap_remove(i);
                        *moved = moved.max(apart(o.1, m.1) * o.2.min(m.2));
                        *faded = faded.max((o.2 - m.2).abs());
                    }
                    None => *faded = faded.max(m.2),
                }
            }
            for o in left {
                *faded = faded.max(o.2);
            }
        };
        if !self.was.is_empty() || !self.was_scales.is_empty() {
            pair(&self.was, &now, &mut self.moved, &mut self.faded, false);
            pair(&self.was_scales, &scales, &mut self.moved, &mut self.faded, true);
        }
        let main = c.main().map(|s| (s.name.clone(), (s.alpha * 2.0 - 1.0).clamp(0.0, 1.0)));
        if let (Some(was), Some((name, shown))) = (&self.main, &main)
            && was != name
        {
            self.switched = self.switched.max(*shown);
        }
        self.main = main.map(|m| m.0);
        (self.was, self.was_scales) = (now, scales);
    }
}

#[test]
fn on_the_ground_the_tape_reads_from_the_bodys_own_north() {
    let (bodies, nav) = (worlds(), nav());
    let mut c = Compass::default();
    for (k, b) in bodies.iter() {
        // anywhere on it but its poles, looking any way: the figures are how far the look is
        // turned from its north, and N is that far to the left
        for dir in [clear(&bodies, k), DVec3::new(0.5, 0.3, -0.8).normalize(), DVec3::new(-0.2, -0.9, 0.1).normalize()] {
            let (north, much) = b.north_at(dir);
            if much < 0.1 {
                continue;
            }
            let east = north.cross(dir);
            for heading in [0.0_f64, 37.0, 90.0, 181.0, 300.0] {
                let h = heading.to_radians();
                let who = Who { at: b.center + dir * (b.radius + 2.0), vel: DVec3::ZERO, up: dir, ahead: north * h.cos() + east * h.sin(), ship: None, sun: DVec3::X };
                nav.compass(&bodies, &who, &mut c);
                let main = c.main().unwrap_or_else(|| panic!("{}: sin escala", b.name));
                assert!(main.name == "RUMBO" && (main.alpha - 1.0).abs() < 1e-6, "{}: {main:?}", b.name);
                assert!(((f64::from(main.heading) - heading + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-3, "{}: mirando a {heading}° la cinta dice {}", b.name, main.heading);
                let n = c.marks().iter().find(|m| m.text == "N").unwrap();
                assert!(((f64::from(n.bearing) + heading + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-3 && n.rise.abs() < 1e-3, "{}: N a {}°", b.name, n.bearing);
                let e = c.marks().iter().find(|m| m.text == "E").unwrap();
                assert!(((e.bearing - n.bearing).rem_euclid(360.0) - 90.0).abs() < 1e-3);
                assert_eq!(c.marks().len(), 8, "{}: solo los ocho vientos", b.name);
            }
        }
    }
}

#[test]
fn what_counts_as_orbit_is_the_same_round_any_body() {
    let (bodies, nav) = (worlds(), nav());
    for (k, b) in bodies.iter() {
        let radial = clear(&bodies, k);
        let side = radial.any_orthonormal_vector();
        for alt in [10.0, b.whole_to() * 0.5, b.whole_to()] {
            let round = (b.own_pull(alt) * (b.radius + alt)).sqrt();
            // standing, falling or climbing straight: not at all; at the speed of a circular
            // orbit there: wholly; and smoothly between
            assert_eq!(nav.orbiting(b, radial, alt, DVec3::ZERO), 0.0);
            assert!(nav.orbiting(b, radial, alt, radial * round * 3.0) < 1e-12);
            assert_eq!(nav.orbiting(b, radial, alt, side * round), 1.0);
            let (lo, hi) = (nav.def.orbita[0], nav.def.orbita[1]);
            assert!(nav.orbiting(b, radial, alt, side * round * lo) < 1e-12);
            let mid = nav.orbiting(b, radial, alt, side * round * (lo + hi) * 0.5);
            assert!((mid - 0.5).abs() < 1e-9, "{}: a medio camino pesa {mid}", b.name);
        }
        eprintln!("{}: órbita circular a ras de suelo a {:.0} m/s", b.name, (b.own_pull(0.0) * b.radius).sqrt());
    }
}

#[test]
fn nothing_on_the_tape_ever_jumps() {
    let (bodies, nav) = (worlds(), nav());
    let mut c = Compass::default();
    let sun = DVec3::new(0.6, 0.3, -0.7).normalize();
    let mut worst = (0.0_f32, 0.0_f32, 0.0_f32, String::new());
    let mut check = |what: &str, seen: Seen| {
        assert!(seen.moved < 1.0, "{what}: algo en la cinta salta {:.2}° de golpe", seen.moved);
        assert!(seen.faded < 0.06, "{what}: algo aparece o se va de golpe ({:.3})", seen.faded);
        assert!(seen.switched < 0.06, "{what}: las cifras cambian de escala a la vista ({:.3})", seen.switched);
        if seen.moved > worst.0 {
            worst = (seen.moved, seen.faded, seen.switched, what.to_string());
        }
    };
    for (k, b) in bodies.iter() {
        let radial = clear(&bodies, k);
        let side = radial.any_orthonormal_vector();
        let round0 = (b.own_pull(0.0) * b.radius).sqrt();
        let boat = ship(b.center + radial * (b.radius + b.reach * 1.3) + side * 900.0);
        // ---- turning all the way round, in every regime and between them, one's own way up
        // the body's or any other
        for (place, alt, speed) in [("en el suelo", 2.0, 0.0), ("en órbita", b.whole_to() * 0.5, round0), ("a medias entre suelo y órbita", 500.0, round0 * 0.62), ("en la franja", b.whole_to() + b.band * 0.6, round0 * 0.3), ("fuera", b.reach * 1.3, 40.0)] {
            for up in [radial, (radial + side * 1.3).normalize(), -radial] {
                let mut seen = Seen::default();
                let fwd = up.any_orthonormal_vector();
                for i in 0..=1440 {
                    let a = f64::from(i) * 0.25_f64.to_radians();
                    let ahead = fwd * a.cos() + fwd.cross(up) * a.sin();
                    let who = Who { at: b.center + radial * (b.radius + alt), vel: side * speed + radial * 3.0, up, ahead, ship: Some(&boat), sun };
                    nav.compass(&bodies, &who, &mut c);
                    seen.look(&c);
                }
                check(&format!("{}, {place}, girando sobre uno mismo", b.name), seen);
            }
        }
        // ---- straight up from the ground and out of its reach, speeding up to orbit on the
        // way and slowing again: ground, orbit, band, free space, with a ship to go by
        let mut seen = Seen::default();
        let top = b.reach * 1.2;
        let steps = 40_000;
        for i in 0..=steps {
            let f = f64::from(i) / f64::from(steps);
            let alt = 2.0 + top * f;
            let speed = round0 * (f * 4.0).min(1.0) * ((1.0 - f) * 3.0).min(1.0);
            let who = Who { at: b.center + radial * (b.radius + alt), vel: side * speed + radial * 20.0, up: radial, ahead: level(DVec3::new(0.3, -0.5, 0.8), radial), ship: Some(&boat), sun };
            nav.compass(&bodies, &who, &mut c);
            seen.look(&c);
        }
        check(&format!("{}, del suelo al espacio libre", b.name), seen);
        // ---- across each of its poles on foot, looking the way one walks (carried over the
        // pole as one's own frame is: the way one goes, laid level)
        for pole in [b.north, -b.north] {
            let across = pole.any_orthonormal_vector();
            let mut seen = Seen::default();
            for i in -20_000..=20_000 {
                // (two metres a step, from 40 km on one side to 40 km on the other)
                let x = f64::from(i) * 2.0;
                let up = (pole * b.radius + across * (x + 0.37)).normalize();
                let who = Who { at: b.center + up * (b.radius + 1.75), vel: DVec3::ZERO, up, ahead: level(across, up), ship: None, sun };
                nav.compass(&bodies, &who, &mut c);
                seen.look(&c);
            }
            check(&format!("{}, andando por encima de un polo", b.name), seen);
        }
    }
    // ---- from the Moon's reach into the moonlet's and down to its ground: two bodies at once
    let (moon, moonlet) = (bodies.get(0), bodies.get(1));
    let toward = (moon.center - moonlet.center).normalize();
    let boat = ship(moonlet.center + toward * (moonlet.radius + 9000.0));
    let mut seen = Seen::default();
    for i in 0..=40_000 {
        let alt = (moonlet.reach + 3000.0) * (1.0 - f64::from(i) / 40_000.0) + 2.0;
        let who = Who { at: moonlet.center + toward * (moonlet.radius + alt), vel: -toward * 30.0, up: -toward, ahead: level(DVec3::new(0.7, 0.1, 0.4), toward), ship: Some(&boat), sun };
        nav.compass(&bodies, &who, &mut c);
        seen.look(&c);
    }
    check("de la influencia de la Luna a la de la luna menor", seen);
    eprintln!("lo más que salta algo en la cinta: {:.3}° ({}); lo más que cambia de solidez de un paso a otro: {:.3}", worst.0, worst.3, worst.1);
}

#[test]
fn the_heading_it_replaces_broke_where_this_does_not() {
    // the rule before: a heading from -Z of the world laid level (from +X where that has no
    // level part), read on any body, anywhere, whatever pulled. Walking over the place where
    // -Z of the world is straight up, looking the way one walks:
    let (bodies, nav) = (worlds(), nav());
    let old = |up: DVec3, fwd: DVec3| {
        let mut north = DVec3::NEG_Z - up * DVec3::NEG_Z.dot(up);
        if north.length_squared() < 1e-8 {
            north = DVec3::X - up * DVec3::X.dot(up);
        }
        let north = north.normalize();
        fwd.dot(north.cross(up)).atan2(fwd.dot(north)).to_degrees().rem_euclid(360.0)
    };
    let mut c = Compass::default();
    for (_, b) in bodies.iter() {
        let across = DVec3::Y;
        let (mut seen, mut last, mut jump) = (Seen::default(), None::<f64>, 0.0_f64);
        for i in -6000..=6000 {
            let x = f64::from(i) * 0.05;
            let up = (DVec3::NEG_Z * b.radius + across * (x + 0.013)).normalize();
            let ahead = level(across, up);
            let h = old(up, ahead);
            if let Some(l) = last {
                jump = jump.max(((h - l + 180.0).rem_euclid(360.0) - 180.0).abs());
            }
            last = Some(h);
            nav.compass(&bodies, &Who { at: b.center + up * (b.radius + 1.75), vel: DVec3::ZERO, up, ahead, ship: None, sun: DVec3::X }, &mut c);
            seen.look(&c);
        }
        eprintln!("{}: andando 600 m sobre donde -Z del mundo es arriba, el rumbo de antes salta {jump:.1}° en un paso de 5 cm; en la cinta de ahora lo más que salta algo es {:.3}°", b.name, seen.moved);
        assert!(jump > 80.0, "{}: el rumbo de antes no saltaba ahí ({jump:.2}°)", b.name);
        assert!(seen.moved < 1.0 && seen.faded < 0.06);
    }
    // and past every reach it went on giving a heading from the nearest body, which means
    // nothing there; now there is no scale of any body, and what is marked is the ship and the sun
    let b = bodies.get(0);
    let radial = clear(&bodies, 0);
    let at = b.center + radial * (b.radius + b.reach * 2.0);
    let boat = ship(at + radial.any_orthonormal_vector() * 400.0);
    nav.compass(&bodies, &Who { at, vel: DVec3::ZERO, up: radial, ahead: radial.any_orthonormal_vector(), ship: Some(&boat), sun: DVec3::X }, &mut c);
    assert!(c.scales().iter().all(|s| s.name == "PROA") && c.marks().iter().all(|m| !["N", "E", "S", "O", "PRO"].contains(&m.text.as_str())), "fuera de toda influencia: {:?} {:?}", c.scales(), c.marks());
    assert!(c.marks().iter().any(|m| m.text.starts_with("Alcotán ") && m.text.ends_with(" m")) && c.marks().iter().any(|m| m.text == "SOL"));
    // (with no ship to go by: the sun alone, and no figures)
    nav.compass(&bodies, &Who { at, vel: DVec3::ZERO, up: radial, ahead: radial.any_orthonormal_vector(), ship: None, sun: DVec3::X }, &mut c);
    assert!(c.scales().is_empty() && c.marks().iter().all(|m| m.text == "SOL"));
}

#[test]
fn filling_the_compass_allocates_nothing_once_it_has_held_its_most() {
    let (bodies, nav) = (worlds(), nav());
    let b = bodies.get(0);
    let boat = ship(b.center + DVec3::Y * (b.radius + 500.0));
    let mut c = Compass::default();
    let who = |alt: f64| Who { at: b.center + DVec3::Y * (b.radius + alt), vel: DVec3::X * 900.0, up: DVec3::Y, ahead: DVec3::X, ship: Some(&boat), sun: DVec3::Z };
    // (the band, where every regime shows at once: the most it ever holds)
    nav.compass(&bodies, &who(b.whole_to() + b.band * 0.5), &mut c);
    let held: Vec<(*const u8, usize)> = c.marks.iter().map(|m| (m.text.as_ptr(), m.text.capacity())).collect();
    let (marks, scales) = (c.marks.capacity(), c.scales.capacity());
    for alt in [2.0, 5000.0, b.whole_to() + b.band * 0.5, b.reach * 2.0, b.whole_to() + b.band * 0.5] {
        nav.compass(&bodies, &who(alt), &mut c);
    }
    assert!(c.marks.capacity() == marks && c.scales.capacity() == scales);
    assert!(c.marks.iter().zip(&held).all(|(m, h)| (m.text.as_ptr(), m.text.capacity()) == *h), "los textos de las marcas se vuelven a reservar");
}
