//! Which way is up, what one weighs and how one faces, swept over what must not matter: the
//! bodies there are and one made up here (another size, another gravity, another reach, its axis
//! another way); inside, across and past each reach; from standing still to orbital speed; frames
//! from 10 to 240 a second; the ship upright, lying on its side, upside down and tumbling.
//! `Loop` runs the game's frame, and what is measured is what would be seen: the eye from one
//! frame to the next, and how far the picture turns.
use super::tests::{Loop, UNEVEN, aboard, feet_on, outside, rooms, with_gravity};
use super::*;
use lunar_core::{
    body::{Body, BodyDef},
    defs,
    scenario::ScenarioDef,
    structure::blueprint::{Blueprint, StructureDef},
};

/// The bodies of the game's data and one made up, far from the others.
fn worlds() -> Arc<BodyRegistry> {
    let moon: BodyDef = defs::parse("luna", include_str!("../../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let moonlet: BodyDef = defs::parse("luna_menor", include_str!("../../../../assets/defs/bodies/luna_menor.jsonc")).unwrap();
    let other: BodyDef = defs::parse("prueba", r#"{ "name": "Prueba", "center": [4.0e6, 2.5e6, -3.0e6], "radius": 300000, "gravity": 3.7, "reach": { "to": 90000, "band": 35000 }, "north": [0.3, 1.0, 0.2], "horizon_depth": 100 }"#).unwrap();
    Arc::new(BodyRegistry::new(vec![Body::from_def("luna", &moon).unwrap(), Body::from_def("luna_menor", &moonlet).unwrap(), Body::from_def("prueba", &other).unwrap()]))
}

fn pilot_in(bodies: &Arc<BodyRegistry>) -> Pilot {
    let sc: ScenarioDef = defs::parse("scenario", include_str!("../../../../assets/defs/scenario.jsonc")).unwrap();
    let site = Site::from_def(&sc.site, bodies).unwrap();
    Pilot::new(bodies.clone(), &site, sc.player)
}

/// A way out from the centre of body `k` along which no other body's reach is met.
fn clear(bodies: &BodyRegistry, k: BodyId) -> DVec3 {
    let b = bodies.get(k);
    let others: DVec3 = bodies.iter().filter(|(i, _)| *i != k).map(|(_, o)| (o.center - b.center).normalize()).sum();
    (-others).normalize_or(DVec3::Y)
}

/// Places to be at: by each body where its pull is whole, in its band and past its reach, and
/// where the Moon's reach and the moonlet's meet. (What it is, the point, the way out from the
/// body it is by.)
fn places(bodies: &BodyRegistry) -> Vec<(String, DVec3, DVec3)> {
    let mut out = Vec::new();
    for (k, b) in bodies.iter() {
        let dir = clear(bodies, k);
        for (what, alt) in [("dentro", b.whole_to() * 0.4), ("en la franja", b.whole_to() + b.band * 0.5), ("fuera", b.reach * 1.6)] {
            out.push((format!("{} ({what})", b.name), b.center + dir * (b.radius + alt), dir));
        }
    }
    let (moon, moonlet) = (bodies.get(0), bodies.get(1));
    let toward = (moon.center - moonlet.center).normalize();
    out.push(("entre la Luna y la luna menor".into(), moonlet.center + toward * (moonlet.radius + moonlet.whole_to() + moonlet.band * 0.5), -toward));
    out
}

/// How many frames of lengths `frames` (gone through again and again) `secs` s take.
fn frames_of(secs: f64, frames: &[f64]) -> usize {
    let (mut t, mut n) = (0.0, 0);
    while t < secs {
        t += frames[n % frames.len()];
        n += 1;
    }
    n
}

/// A deck 6 m wide and 12 long (its top at y 0 of its frame, from z -3 to 9) with a block on
/// it to bump into, loose at `at` turned `rot`: its structure id.
fn deck(set: &mut Structures, at: DVec3, rot: Quat) -> u64 {
    if set.lib.blueprint("cubierta").is_none() {
        let mut lib = (*set.lib).clone();
        let def: StructureDef = defs::parse(
            "cubierta",
            r#"{ "name": "Cubierta de prueba", "anchored": true, "auto_joint": "mortero", "parts": [
                { "id": "suelo", "part": "losa", "at": [0.0, -0.15, 0.0] },
                { "id": "suelo_2", "part": "losa", "at": [0.0, -0.15, 6.0] },
                { "id": "bloque", "part": "bloque", "at": [0.0, 0.375, 8.0] }
            ] }"#,
        )
        .unwrap();
        let bp = Blueprint::new(&def, &lib.catalog).unwrap();
        lib.blueprints.push(("cubierta".into(), bp));
        let list = std::mem::take(&mut set.list);
        *set = Structures::new(Arc::new(lib));
        set.list = list;
    }
    set.spawn("cubierta", at, rot).unwrap()
}

fn library() -> Arc<lunar_core::structure::Library> {
    Arc::new(lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap())
}

/// The ways a ship may lie: upright (its deck up as `up`), rolled on its side, upside down,
/// pitched up; and how it tumbles (rad/s, world), if it does.
fn attitudes(up: DVec3) -> Vec<(&'static str, Quat, Vec3)> {
    let level = lunar_core::scene::basis(up, up.any_orthonormal_vector());
    let fwd = level * Vec3::Z;
    vec![
        ("derecha", level, Vec3::ZERO),
        ("de lado", Quat::from_axis_angle(fwd, 1.5708) * level, Vec3::ZERO),
        ("boca abajo", Quat::from_axis_angle(fwd, 3.1416) * level, Vec3::ZERO),
        ("encabritada 50°", Quat::from_axis_angle(level * Vec3::X, -0.87) * level, Vec3::ZERO),
        ("dando tumbos", level, Vec3::new(0.21, 0.13, -0.17)),
    ]
}

#[test]
fn aboard_a_ship_with_gravity_of_its_own_the_deck_is_the_floor_wherever_and_however_it_lies() {
    let bodies = worlds();
    let mut runs = 0;
    for (place, at, out) in places(&bodies) {
        for (lies, rot, spin) in attitudes(out) {
            for (speed, frames) in [(0.0, &UNEVEN[..]), (300.0, &[1.0 / 30.0][..]), (7800.0, &[1.0 / 144.0][..]), (1600.0, &[0.1][..]), (30.0, &[1.0 / 240.0][..])] {
                let mut set = Structures::new(library());
                let id = deck(&mut set, at, rot);
                with_gravity(&mut set, 0, 1.62, 20.0);
                (set.list[0].vel, set.list[0].spin) = (out.any_orthonormal_vector() * speed + out * 2.0, spin);
                let mut p = pilot_in(&bodies);
                p.put_on(&set, id, Vec3::new(0.0, 0.0, -2.0));
                // (facing along the deck: the way it is long)
                p.look_at(set.list[0].to_world(Vec3::new(0.0, 1.75, 30.0)));
                let mut g = Loop::new(p, set);
                g.frames = frames.to_vec();
                let ship_eye = |g: &Loop, eye: DVec3| g.set.list[0].to_local(eye);
                // standing: on our feet, where we were put, and the picture still in the ship
                let (mut last, mut worst, mut aloft, mut t) = (None::<Vec3>, 0.0_f32, 0, 0.0);
                while t < 1.5 {
                    let eye = g.frame(Input::default(), &rooms);
                    t += g.dt;
                    let eye = ship_eye(&g, eye);
                    if let Some(l) = last {
                        worst = worst.max(eye.distance(l));
                    }
                    last = Some(eye);
                    aloft += usize::from(!g.p.grounded);
                }
                let what = format!("{place}, nave {lies}, a {speed} m/s, fotogramas de {:.1} ms", frames[0] * 1e3);
                let deck_up = (g.set.list[0].rot * Vec3::Y).as_dvec3().normalize();
                assert_eq!(aloft, 0, "{what}: no se tiene en pie en la cubierta");
                assert!(g.p.ride.is_some_and(|r| r.id == id), "{what}: la nave no nos lleva");
                assert!(worst < 2e-3, "{what}: de pie, el ojo salta {:.1} mm frente a la nave", worst * 1e3);
                assert!(feet_on(&g.p, &g.set, id).distance(Vec3::new(0.0, 0.0, -2.0)) < 0.02, "{what}: los pies se van a {:.2?}", feet_on(&g.p, &g.set, id));
                assert!(g.p.feet().1.dot(deck_up) > 1.0 - 1e-9, "{what}: su arriba no es el de la cubierta");
                assert!((g.p.weighs() - 1.62).abs() < 1e-3, "{what}: pesa {:.3} m/s²", g.p.weighs());
                // walking: along the deck at a walk's pace, never off our feet
                let (from, mut aloft, mut t) = (feet_on(&g.p, &g.set, id), 0, 0.0);
                while t < 2.0 {
                    g.frame(Input { forward: 1.0, ..Default::default() }, &rooms);
                    t += g.dt;
                    aloft += usize::from(!g.p.grounded);
                }
                let walked = feet_on(&g.p, &g.set, id) - from;
                assert_eq!(aloft, 0, "{what}: pierde la cubierta al andar");
                assert!((walked.z - 1.8 * t as f32).abs() < 0.2 && walked.x.abs() < 0.15 && walked.y.abs() < 0.02, "{what}: anda {walked:.2?} en {t:.2} s");
                // a jump: up by the ship's gravity and down again where it went up
                let from = feet_on(&g.p, &g.set, id);
                g.frame(Input { jump: true, ..Default::default() }, &rooms);
                let (mut high, mut down) = (0.0_f32, None);
                for _ in 0..(3.0 / frames[0].min(0.02)) as usize {
                    g.frame(Input::default(), &rooms);
                    let f = feet_on(&g.p, &g.set, id);
                    high = high.max(f.y - from.y);
                    if g.p.landed > 0.0 {
                        down = Some(f);
                        break;
                    }
                }
                let down = down.unwrap_or_else(|| panic!("{what}: salta y no vuelve a la cubierta (subió {high:.2} m)"));
                assert!((high - 1.2).abs() < 0.12 && down.distance(from) < 0.06, "{what}: salta {high:.2} m y cae a {:.2} m de donde saltó", down.distance(from));
                runs += 1;
            }
        }
    }
    eprintln!("a bordo de una nave con gravedad propia: {runs} casos (sitios × posturas × velocidades y fotogramas), en todos se anda por la cubierta");
}

#[test]
fn aboard_what_makes_no_gravity_one_weighs_what_is_left_of_the_pull() {
    let bodies = worlds();
    for (place, at, out) in places(&bodies) {
        let pull = bodies.field(at).pull;
        for (speed, frames) in [(0.0, &UNEVEN[..]), (1600.0, &[1.0 / 30.0][..]), (7800.0, &[1.0 / 144.0][..])] {
            let go = |fall: bool, push: DVec3| {
                let mut set = Structures::new(library());
                // (its deck up against what it will weigh: one stands on it)
                let weight = pull * f64::from(u8::from(!fall)) - push;
                let up = (-weight).try_normalize().unwrap_or(out);
                let id = deck(&mut set, at, lunar_core::scene::basis(up, up.any_orthonormal_vector()));
                set.list[0].vel = out.any_orthonormal_vector() * speed;
                let mut p = pilot_in(&bodies);
                p.put_on(&set, id, Vec3::new(0.0, 0.0, -2.0));
                let mut g = Loop::new(p, set);
                (g.frames, g.fall, g.push) = (frames.to_vec(), fall, push);
                let (began, eye_began) = (g.p.position, g.set.list[0].to_local(g.p.position));
                let (mut worst, mut last, mut aloft, mut t) = (0.0_f32, None::<Vec3>, 0, 0.0);
                while t < 4.0 {
                    let eye = g.frame(Input::default(), &outside);
                    t += g.dt;
                    let eye = g.set.list[0].to_local(eye);
                    if let Some(l) = last {
                        worst = worst.max(eye.distance(l));
                    }
                    last = Some(eye);
                    aloft += usize::from(!g.p.grounded);
                }
                // (what it should weigh where it has got to: going straight at orbital speed it
                // has climbed, and the way down has turned under it by so much: a body upright
                // to that has its feet that much aside of where they were on the deck)
                let want = bodies.field(g.p.position).pull * f64::from(u8::from(!fall)) - push;
                let leant = if fall { 0.0 } else { bodies.field(began).pull.angle_between(bodies.field(g.p.position).pull) };
                let gone = if fall {
                    // (floating, it is the eye that stays: the body may right itself about it)
                    f64::from(g.set.list[0].to_local(g.p.position).distance(eye_began))
                } else {
                    (f64::from(feet_on(&g.p, &g.set, id).distance(Vec3::new(0.0, 0.0, -2.0))) - leant * g.p.def.eye_height).max(0.0)
                };
                (g.p.weight().length(), aloft, worst, gone, want.length())
            };
            let what = format!("{place}, a {speed} m/s, fotogramas de {:.1} ms", frames[0] * 1e3);
            // held up where it is (hovering, or standing on something): all of the pull
            let (weighs, aloft, worst, gone, want) = go(false, DVec3::ZERO);
            // (the pull where the eye is, a couple of metres from where the deck was put; going
            // fast across a band, where it was a slice ago: tens of metres back)
            let slack = if speed == 0.0 { 1e-4 } else { 0.06 };
            assert!((weighs - want).abs() < 1e-6 + slack * want, "{what}: sostenida, pesa {weighs:.6} y el tirón es {want:.6}");
            if want > 0.06 {
                assert!(aloft == 0 && worst < 2e-3 && gone < 0.02, "{what}: sostenida, no se tiene en pie ({aloft} fotogramas en el aire, el ojo salta {:.1} mm)", worst * 1e3);
            }
            // falling free: nothing — one floats (not a frame on one's feet), and stays where
            // one was to it. (Before, one stood on it weighing the whole pull.)
            let (_, aloft, worst, gone, _) = go(true, DVec3::ZERO);
            assert_eq!(aloft, frames_of(4.0, frames), "{what}: en caída libre se tiene en pie sobre ella (el tirón es {:.3})", pull.length());
            assert!(worst < 2e-3 && gone < 0.03, "{what}: en caída libre el ojo salta {:.1} mm y se va {gone:.3} m", worst * 1e3);
            // falling and pushed by its engines at 4 m/s² along its deck's way up: that much
            let (weighs, aloft, worst, gone, _) = go(true, (-pull).try_normalize().unwrap_or(out) * 4.0);
            // (give or take how the pull differs between the deck and the eye over it: where
            // one body gives way to another, a few thousandths)
            assert!((weighs - 4.0).abs() < 0.01, "{what}: empujada a 4 m/s² pesa {weighs:.3}");
            assert!(aloft == 0 && worst < 2e-3 && gone < 0.02, "{what}: empujada, no se tiene en pie ({aloft} en el aire, {:.1} mm)", worst * 1e3);
        }
    }
    // and with the world's own physics moving it: a deck dropped where the Moon pulls, us on it
    for speed in [0.0, 1600.0] {
        let mut set = Structures::new(library());
        let moon = bodies.get(0);
        let up = DVec3::new(0.2, 0.9, -0.3).normalize();
        let at = moon.center + up * (moon.radius + 9000.0);
        let id = deck(&mut set, at, lunar_core::scene::basis(up, up.any_orthonormal_vector()));
        set.list[0].vel = up.any_orthonormal_vector() * speed;
        let mut p = pilot_in(&bodies);
        p.put_on(&set, id, Vec3::new(0.0, 0.0, -2.0));
        let mut g = Loop::new(p, set);
        g.world = true;
        // (what it weighs on the deck while the deck carries it; and never a frame on its feet)
        let (mut worst, mut last, mut heaviest, mut afoot) = (0.0_f32, None::<Vec3>, 0.0_f64, 0);
        for _ in 0..400 {
            let carried = g.p.ride.is_some();
            let eye = g.frame(Input::default(), &outside);
            let eye = g.set.list[0].to_local(eye);
            if let Some(l) = last {
                worst = worst.max(eye.distance(l));
            }
            last = Some(eye);
            afoot += usize::from(g.p.grounded);
            // (carried all through the frame: what it weighed was worked out on the deck)
            if carried && g.p.ride.is_some() {
                heaviest = heaviest.max(g.p.weight().length());
            }
        }
        let gone = feet_on(&g.p, &g.set, id).distance(Vec3::new(0.0, 0.0, -2.0));
        eprintln!("sobre una cubierta que cae (la física del mundo) a {speed} m/s: {afoot} fotogramas de pie; llevado por ella pesa como mucho {heaviest:.5} m/s²; el ojo salta {:.2} mm; se aparta {gone:.3} m en 5 s", worst * 1e3);
        assert!(afoot == 0 && heaviest < 1e-3 && worst < 2e-3 && gone < 0.03);
    }
}

#[test]
fn past_every_reach_nothing_pulls_and_nothing_turns_us() {
    let bodies = worlds();
    for (place, at, out) in places(&bodies).into_iter().filter(|p| p.0.contains("fuera")) {
        for speed in [0.0, 30.0, 1600.0, 7800.0] {
            for frames in [&UNEVEN[..], &[1.0 / 30.0], &[1.0 / 240.0], &[0.1]] {
                let mut p = pilot_in(&bodies);
                // as we came: our own way up any way but the body's, going any way
                let up = (out * 0.2 + out.any_orthonormal_vector()).normalize();
                p.put(at, up);
                (p.grounded, p.aloft) = (false, true);
                let v = (out.any_orthonormal_vector() * 0.8 + out * 0.6) * speed;
                (p.vertical_velocity, p.drift) = (v.dot(up), v - up * v.dot(up));
                let mut g = Loop::new(p, Structures::new(library()));
                g.frames = frames.to_vec();
                let (from, ahead) = (g.p.position, g.p.heading());
                let (mut t, mut worst) = (0.0, 0.0_f64);
                while t < 8.0 {
                    let was = g.p.eye();
                    let eye = g.frame(Input::default(), &outside);
                    t += g.dt;
                    worst = worst.max((eye - was - v * g.dt).length());
                }
                let what = format!("{place}, a {speed} m/s, fotogramas de {:.1} ms", frames[0] * 1e3);
                assert!(g.p.ground.is_none() && g.p.weight() == DVec3::ZERO && !g.p.grounded, "{what}: algo rige");
                assert!(g.p.feet().1 == up && g.p.heading().dot(ahead) > 1.0 - 1e-12, "{what}: gira solo: su arriba se ha movido {:.2e} rad", g.p.feet().1.angle_between(up));
                assert!((g.p.position - (from + v * t)).length() < 1e-6 * (1.0 + speed * t) && worst < 1e-6 * (1.0 + speed), "{what}: no va recto (se aparta {:.2e} m por fotograma)", worst);
                // the pack on, hands off: nothing to keep to, nothing burnt, nothing changed
                g.p.pack_on = true;
                for _ in 0..120 {
                    g.frame(Input::default(), &outside);
                }
                assert!(g.p.hold == Hold::Free && g.p.fuel == 1.0, "{what}: la mochila hace algo sin nada a lo que sujetarnos: {:?}, gas {:.6}", g.p.hold, g.p.fuel);
                // (how fast we go, read off where we are from one slice to the next: as fine as
                // a place millions of metres from the world's origin is told)
                assert!((g.p.velocity() - v).length() < 1e-6 * (1.0 + speed), "{what}: con la mochila encendida y las manos quietas la velocidad cambia {:.3e} m/s", (g.p.velocity() - v).length());
                // its push: along our own way up, as hard as it pushes
                let (mut t, before) = (0.0, g.p.velocity());
                while t < 1.0 {
                    g.frame(Input { vertical: 1.0, ..Default::default() }, &outside);
                    t += g.dt;
                }
                let gained = g.p.velocity() - before;
                let j = g.p.def.mochila.unwrap();
                assert!((gained - up * j.empuje * t).length() < 0.02 * j.empuje * t, "{what}: empuja {gained:.2?} en {t:.2} s");
            }
        }
    }
}

#[test]
fn across_the_edge_of_a_reach_nothing_jumps() {
    let bodies = worlds();
    for k in 0..3 {
        let b = bodies.get(k);
        let out = clear(&bodies, k);
        let side = out.any_orthonormal_vector();
        for (down, along) in [(60.0, 0.0), (400.0, 300.0), (2500.0, 0.0)] {
            for frames in [&UNEVEN[..], &[1.0 / 30.0], &[1.0 / 144.0], &[0.1]] {
                let mut p = pilot_in(&bodies);
                // from just past its reach, going in, our own way up nothing like the body's
                let up = (side - out * 0.3).normalize();
                p.put(b.center + out * (b.radius + b.reach + 300.0), up);
                (p.grounded, p.aloft) = (false, true);
                let v = side * along - out * down;
                (p.vertical_velocity, p.drift) = (v.dot(up), v - up * v.dot(up));
                let mut g = Loop::new(p, Structures::new(library()));
                g.frames = frames.to_vec();
                let e = g.p.def.cuerpo.enderezar;
                let (mut was, mut going, mut look) = (g.p.eye(), v, g.p.directions().0);
                let (mut bump, mut turn, mut first_turn, mut kept, mut last_dt) = (0.0_f64, 0.0_f64, None::<f64>, true, 0.0);
                let until = b.radius + b.whole_to() * 0.9;
                let mut frames_in = 0;
                while (g.p.position - b.center).length() > until && frames_in < 200_000 {
                    let eye = g.frame(Input::default(), &outside);
                    let alt = (g.p.position - b.center).length() - b.radius;
                    frames_in += 1;
                    // the eye: only what pulls there moves it off its way (and that never at a
                    // stroke: no more in a frame than the pull of the place over that frame and
                    // the one before, whose mean speed `going` is)
                    let pull = bodies.field(was).g().max(bodies.field(eye).g());
                    let off = (eye - was - going * g.dt).length();
                    bump = bump.max(off - pull * g.dt * (g.dt + last_dt));
                    going = (eye - was) / g.dt;
                    last_dt = g.dt;
                    was = eye;
                    // the picture: it turns no faster than the body rights itself
                    let now = g.p.directions().0;
                    let turned = now.angle_between(look);
                    turn = turn.max(turned / g.dt);
                    look = now;
                    if alt >= b.reach {
                        kept &= turned == 0.0;
                    } else if first_turn.is_none() {
                        first_turn = Some(turned / g.dt);
                    }
                }
                let what = format!("{}, entrando a {down} m/s (y {along} de lado), fotogramas de {:.1} ms", b.name, frames[0] * 1e3);
                assert!(kept, "{what}: gira antes de entrar en su influencia");
                assert!(bump < 1e-6, "{what}: el ojo salta {:.3} mm de por donde iba", bump * 1e3);
                assert!(turn <= e.giro * 1.0001, "{what}: la imagen gira a {turn:.2} rad/s (lo más, {})", e.giro);
                // (as it comes in hardly anything pulls yet: hardly anything turns)
                let first = first_turn.unwrap_or(0.0);
                assert!(first < 0.02 * e.giro * (down / 60.0).max(1.0), "{what}: nada más entrar gira a {first:.4} rad/s");
                // well inside, given the time its pace asks: upright to the body
                let mut t = 0.0;
                while t < 8.0 {
                    (g.p.vertical_velocity, g.p.drift) = (0.0, DVec3::ZERO);
                    g.frame(Input::default(), &outside);
                    t += g.dt;
                }
                let aligned = g.p.feet().1.angle_between(b.up(g.p.position));
                assert!(aligned < 0.02, "{what}: dentro, a {:.1}° de la vertical", aligned.to_degrees());
            }
        }
    }
}

#[test]
fn how_one_faces_has_no_place_where_it_flips() {
    let bodies = worlds();
    let c = Controls::new(&pilot_in(&bodies).def);
    // on the ground of each body, walking straight across the places where a frame taken from a
    // fixed axis of the world breaks (where -Z of the world is the way up, and the other way),
    // and across the body's own poles
    for k in 0..3 {
        let b = bodies.get(k);
        for (what, pole) in [("donde arriba es -Z del mundo", DVec3::NEG_Z), ("donde arriba es +Z del mundo", DVec3::Z), ("su polo norte", b.north), ("su polo sur", -b.north)] {
            // (not along +X of the world: the previous rule took its north from there within a
            // ten-thousandth of the radius of those two places, and jumped at the rim of that)
            let aside = pole.cross(DVec3::X).try_normalize().unwrap_or(DVec3::Y);
            let rim = if pole.abs_diff_eq(DVec3::NEG_Z, 1e-9) || pole.abs_diff_eq(DVec3::Z, 1e-9) { 1e-4 * b.radius } else { 0.0 };
            let steps = ((rim + 55.0) / 0.03) as usize;
            let dir = (pole * b.radius - aside * (rim + 25.0)).normalize();
            let mut p = pilot_in(&bodies);
            p.put(b.above_ground(dir, 0.0), dir);
            p.look_at(b.above_ground((pole * b.radius + aside * 25.0).normalize(), 1.75));
            p.pitch = 0.0;
            // (the previous rule, as it was written: north and east from -Z of the world laid
            // level, or from +X where that has no level part)
            let old = |up: DVec3, yaw: f64| {
                let mut north = DVec3::NEG_Z - up * DVec3::NEG_Z.dot(up);
                if north.length_squared() < 1e-8 {
                    north = DVec3::X - up * DVec3::X.dot(up);
                }
                let north = north.normalize();
                north * yaw.cos() + north.cross(up) * yaw.sin()
            };
            let yaw = {
                let (up, f) = (p.feet().1, p.heading());
                let north = (DVec3::NEG_Z - up * DVec3::NEG_Z.dot(up)).normalize();
                f.dot(north.cross(up)).atan2(f.dot(north))
            };
            let (mut look, mut was_old) = (p.heading(), old(p.feet().1, yaw));
            let (mut worst, mut worst_old, mut nearest) = (0.0_f64, 0.0_f64, f64::MAX);
            for _ in 0..steps {
                p.step(1.0 / 60.0, Input { forward: 1.0, ..Default::default() }, &c, None);
                let now = p.heading();
                worst = worst.max(now.angle_between(look));
                look = now;
                let now_old = old(p.feet().1, yaw);
                worst_old = worst_old.max(now_old.angle_between(was_old));
                was_old = now_old;
                nearest = nearest.min((b.up(p.position) - pole).length() * b.radius);
            }
            eprintln!("{}, andando sobre {what} (pasa a {nearest:.2} m): la mirada gira como mucho {:.2e} rad en un paso; con el norte de un eje fijo del mundo, {worst_old:.2} rad", b.name, worst);
            assert!(nearest < 3.0, "{}: no pasa por {what}", b.name);
            // (the ground curves 3 cm of walk over its radius per step; slopes tilt nothing)
            assert!(worst < 1e-5, "{}, sobre {what}: la mirada salta {worst:.2e} rad", b.name);
            if rim > 0.0 {
                assert!(worst_old > 1.0, "la regla anterior no saltaba aquí ({worst_old:.3} rad): la prueba no la distingue");
            }
        }
    }
    // and all the way round a small body, through both of those places: back where we began,
    // looking the way we looked
    let b = bodies.get(2);
    let mut p = pilot_in(&bodies);
    let start = DVec3::new(0.0, 1.0, 0.0);
    p.put(b.center + start * b.radius, start);
    p.look_at(b.center + (start * b.radius + DVec3::NEG_Z * 100.0).normalize() * (b.radius + 1.75));
    p.pitch = 0.0;
    let ahead = p.heading();
    // (carried round by its own frame, not walked: a great circle in steps of a hundredth of a degree)
    for i in 1..=36_000 {
        let a = f64::from(i) * 1e-2_f64.to_radians();
        p.set_up(start * a.cos() + DVec3::NEG_Z * a.sin());
    }
    assert!(p.heading().dot(ahead) > 1.0 - 1e-9 && p.feet().1.dot(start) > 1.0 - 1e-12, "tras dar la vuelta mira a {:.2e} rad de como miraba", p.heading().angle_between(ahead));
}

#[test]
fn from_one_way_up_to_another_the_body_rights_itself_at_its_own_pace() {
    let bodies = worlds();
    let moon = bodies.get(0);
    let out = DVec3::new(0.1, 0.95, 0.2).normalize();
    // standing on a deck that makes its own gravity, as we came on to it from ground that
    // sloped otherwise: our way up so many degrees off the deck's
    for tilt in [3.0_f64, 25.0, 80.0, 170.0] {
        for frames in [&UNEVEN[..], &[1.0 / 30.0], &[1.0 / 240.0], &[0.1]] {
            let mut set = Structures::new(library());
            let rot = lunar_core::scene::basis(out, out.any_orthonormal_vector());
            let id = deck(&mut set, moon.center + out * (moon.radius + 5000.0), rot);
            with_gravity(&mut set, 0, 1.62, 20.0);
            let mut p = pilot_in(&bodies);
            p.put_on(&set, id, Vec3::new(0.0, 0.0, -2.0));
            let axis = (rot * Vec3::Z).as_dvec3();
            let eye = p.position;
            p.set_up(DQuat::from_axis_angle(axis, tilt.to_radians()) * out);
            p.position = eye;
            aboard(&mut p, &set, id);
            let mut g = Loop::new(p, set);
            g.frames = frames.to_vec();
            let e = g.p.def.cuerpo.enderezar;
            let (mut look, mut was, mut fastest, mut farthest, mut t, mut upright) = (g.p.directions().2, g.p.eye(), 0.0_f64, 0.0_f64, 0.0, None);
            let mut left = tilt.to_radians();
            while t < 4.0 {
                let eye = g.frame(Input::default(), &rooms);
                t += g.dt;
                let now = g.p.directions().2;
                fastest = fastest.max(now.angle_between(look) / g.dt);
                look = now;
                farthest = farthest.max(eye.distance(was) / g.dt);
                was = eye;
                // (always toward the deck's way up, never past it nor back)
                let to_go = now.angle_between(out);
                assert!(to_go <= left + 1e-9, "inclinado {tilt}°: se aleja de la vertical de la cubierta");
                left = to_go;
                if upright.is_none() && to_go < 1e-6 {
                    upright = Some(t);
                }
            }
            let what = format!("inclinado {tilt}° respecto a la cubierta, fotogramas de {:.1} ms", frames[0] * 1e3);
            let took = upright.unwrap_or_else(|| panic!("{what}: no se endereza (le quedan {:.2}°)", left.to_degrees()));
            assert!(fastest <= e.giro * 1.0001, "{what}: gira a {fastest:.2} rad/s (lo más, {})", e.giro);
            // (as long as its pace asks, give or take: the turn at its fastest, then the last of it easing in)
            assert!(took < tilt.to_radians() / e.giro + 25.0 / e.ritmo + 0.3, "{what}: tarda {took:.2} s en enderezarse");
            // the eye does not jump while it does: it goes no faster than a body turning about
            // its feet at that pace would take it
            assert!(farthest < e.giro * g.p.def.eye_height * 1.3, "{what}: el ojo va a {farthest:.2} m/s mientras se endereza");
            assert!(g.p.grounded && g.p.ride.is_some_and(|r| r.id == id), "{what}: acaba sin estar de pie en la cubierta");
        }
    }
}
