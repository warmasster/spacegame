//! The player among structures, on the bare ground and aboard: what the pilot's own steps do,
//! and the game's frame round them (`Loop`) at any speed and frame rate.
use super::*;
use lunar_core::{
    body::{Body, BodyDef},
    defs,
    scenario::ScenarioDef,
};

pub(super) fn pilot() -> Pilot {
    let moon: BodyDef = defs::parse("luna", include_str!("../../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let sc: ScenarioDef = defs::parse("scenario", include_str!("../../../../assets/defs/scenario.jsonc")).unwrap();
    let bodies = Arc::new(BodyRegistry::new(vec![Body::from_def("luna", &moon).unwrap()]));
    let site = Site::from_def(&sc.site, &bodies).unwrap();
    Pilot::new(bodies, &site, sc.player)
}

#[test]
fn walk_speed_and_lunar_jump() {
    let mut p = pilot();
    let c = Controls::new(&p.def);
    let start = p.position;
    for _ in 0..60 {
        p.step(1. / 60., Input { forward: 1., ..Default::default() }, &c, None);
    }
    assert!((p.position.distance(start) - 1.8).abs() < 0.15);
    let mut max = 0_f64;
    for i in 0..200 {
        p.step(1. / 60., Input { jump: i == 0, ..Default::default() }, &c, None);
        max = max.max(p.altitude());
    }
    assert!((max - 1.2).abs() < 0.05);
    assert!(p.grounded);
    assert!(p.altitude().abs() < 1e-7);
}

#[test]
fn off_a_ship_under_way_you_keep_its_speed() {
    // a structure flying level at 30 m/s with us riding it, high over the ground
    let mut p = pilot();
    let c = Controls::new(&p.def);
    let lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
    let name = lib.blueprints.first().map(|b| b.0.clone()).expect("a blueprint");
    let mut set = Structures::new(Arc::new(lib));
    let up = p.bodies.get(p.body).up(p.position);
    let side = up.any_orthonormal_vector();
    let at = p.position + up * 600.0;
    let id = set.spawn(&name, at - up * 40.0, glam::Quat::IDENTITY).unwrap();
    let v = side * 30.0 + up * 2.0;
    set.list[0].vel = v;
    p.position = at;
    p.grounded = false;
    p.ride = Some(Ride { id, local: set.list[0].to_local(at), rot: set.list[0].rot });
    p.pack_on = true;
    let step = 1.0 / 60.0;
    // in one of its rooms, in the air, it carries us: we stay where we are in it
    p.cabin = Some(id);
    let before = p.ride.unwrap().local;
    set.list[0].pos += v * step;
    p.step(step, Input::default(), &c, Some(&set));
    assert!(p.ride.is_some_and(|r| r.id == id && r.local.distance(before) < 0.01), "dentro de la nave no nos lleva");
    // outside its rooms we are on our own, with the speed it had
    p.cabin = None;
    set.list[0].pos += v * step;
    p.step(step, Input::default(), &c, Some(&set));
    assert!(p.ride.is_none());
    assert!((p.drift - side * 30.0).length() < 0.05 && (p.vertical_speed() - 2.0).abs() < 0.1, "al salir no llevamos su velocidad: {:.2?} {:.2}", p.drift, p.vertical_speed());
    // (from here as the world goes: the ship on, then we)
    let go = |p: &mut Pilot, set: &mut Structures, input: Input| {
        set.list[0].pos += v * step;
        p.step(step, input, &c, Some(set));
    };
    // the pack steadies us to that speed, not to the ground: hands off, we keep going with it
    let from = p.position;
    for _ in 0..120 {
        go(&mut p, &mut set, Input::default());
    }
    let gone = (p.position - from).dot(side);
    assert!((gone - 60.0).abs() < 0.5, "en 2 s avanzamos {gone:.1} m, la nave 60");
    // (level with it too: it holds the speed up the ship had, burning what our weight asks)
    assert!((p.vertical_speed() - 2.0).abs() < 0.1, "no mantiene la velocidad vertical de la nave: {:.2}", p.vertical_speed());
    assert!(p.jet > 0.3 && p.jet < 0.5, "quietos respecto a la nave la mochila solo sostiene nuestro peso: {:.2}", p.jet);
    // pushed off it, then let go: it brakes us back to the ship's speed, no further
    for _ in 0..60 {
        go(&mut p, &mut set, Input { forward: 1.0, ..Default::default() });
    }
    assert!((p.drift - side * 30.0).length() > 1.0);
    for _ in 0..600 {
        go(&mut p, &mut set, Input::default());
    }
    assert!((p.drift - side * 30.0).length() < 0.1, "no vuelve a la velocidad de la nave: {:.2?}", p.drift);
    // with the steadying off it lets us coast
    p.toggle_steady();
    for _ in 0..30 {
        go(&mut p, &mut set, Input { forward: 1.0, ..Default::default() });
    }
    let coasting = p.drift;
    for _ in 0..120 {
        go(&mut p, &mut set, Input::default());
    }
    // (nothing brakes us. What is level of how we go changes only as the horizon turns under
    // us: at 30 m/s for 2 s, a millimetre a second)
    assert!((p.drift - coasting).length() < 5e-3 && (coasting - side * 30.0).length() > 0.5);
}

#[test]
fn shift_and_alt_multiply_flight_speed() {
    let mut p = pilot();
    p.toggle_flight();
    let c = Controls { run_factor: 10.0, boost_factor: 10.0, mouse: 1.0, invert_y: false, volume: 1.0 };
    let start = p.position;
    p.step(1.0, Input { vertical: 1.0, run: true, boost: true, ..Default::default() }, &c, None);
    assert!((p.position.distance(start) - p.speed * 100.0).abs() < 1e-6);
}

#[test]
fn the_jet_pack_lifts_carries_and_runs_dry() {
    let mut p = pilot();
    let c = Controls::new(&p.def);
    let j = p.def.mochila.expect("the scenario's suit has a jet pack");
    let step = 1. / 60.;
    // switched off there is no jet pack: Space held does nothing and burns nothing
    assert!(p.has_pack() && !p.pack_on);
    for _ in 0..60 {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
    }
    assert!(p.grounded && p.altitude().abs() < 1e-6 && p.fuel == 1.0, "apagada y sube: {:.2} m", p.altitude());
    assert_eq!(p.toggle_pack(), Some(true));
    // on, it does nothing by itself: walking burns no gas, and neither does a step off a sill
    // (off the ground for a moment), which is still walking
    for k in 0..120 {
        if k % 20 < 6 {
            p.grounded = false;
        }
        p.step(step, Input { forward: 1.0, ..Default::default() }, &c, None);
        assert_eq!(p.jet, 0.0, "la mochila empuja sola al andar (paso {k})");
    }
    assert!(p.fuel == 1.0 && p.drift == DVec3::ZERO, "andar gasta gas: {:.4}", p.fuel);
    // Space held: off the ground and up, burning gas
    for _ in 0..120 {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
    }
    let high = p.altitude();
    assert!(!p.grounded && high > 3.0, "altura {high:.1} m tras 2 s");
    assert!((p.fuel - (1.0 - 2.0 / j.autonomia)).abs() < 0.01, "queda {:.3}", p.fuel);
    // pushing sideways in the air: speed builds up; hands off: it steadies
    let at = p.position;
    for _ in 0..60 {
        p.step(step, Input { forward: 1.0, vertical: 1.0, ..Default::default() }, &c, None);
    }
    let moved = p.position.distance(at);
    assert!(moved > 0.8, "se movió {moved:.2} m");
    for _ in 0..240 {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
    }
    let (a, b) = (p.position, {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
        p.position
    });
    let up = p.bodies.get(p.body).up(a);
    let side = (b - a) - up * (b - a).dot(up);
    assert!(side.length() / step < 0.2, "sigue derivando a {:.2} m/s", side.length() / step);
    // dry: it falls and lands; with no gas Space does nothing
    p.fuel = 0.0;
    for _ in 0..3000 {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
        if p.grounded {
            break;
        }
    }
    assert!(p.grounded && p.altitude().abs() < 1e-6, "no aterriza: {:.1} m", p.altitude());
    for _ in 0..60 {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
    }
    assert!(p.altitude() < 0.01);
}

/// A floor in the sky with two steps of 0.3 m on it and a block too high to step on, and
/// the pilot standing on the floor facing them: (pilot, structures, structure id).
pub(super) fn stairs() -> (Pilot, Structures, u64) {
    let mut p = pilot();
    let mut lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
    let def: lunar_core::structure::blueprint::StructureDef = defs::parse(
        "escalera",
        r#"{ "name": "Escalera de prueba", "anchored": true, "auto_joint": "mortero", "parts": [
            { "id": "suelo", "part": "losa", "at": [0.0, -0.15, 0.0] },
            { "id": "escalon_1", "part": "losa", "at": [0.0, 0.15, 4.5] },
            { "id": "escalon_2", "part": "losa", "at": [0.0, 0.45, 9.0] },
            { "id": "bloque", "part": "bloque", "at": [0.0, 0.975, 10.0] }
        ] }"#,
    )
    .unwrap();
    let bp = lunar_core::structure::blueprint::Blueprint::new(&def, &lib.catalog).unwrap();
    lib.blueprints.push(("escalera".into(), bp));
    let mut set = Structures::new(Arc::new(lib));
    let (forward, _, up) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    let rot = glam::Quat::from_mat3(&glam::Mat3::from_cols(up.cross(ahead).as_vec3(), up.as_vec3(), ahead.as_vec3()));
    let origin = p.position + up * 80.0;
    let id = set.spawn("escalera", origin, rot).unwrap();
    p.position = origin + up * p.def.eye_height;
    p.grounded = true;
    (p, set, id)
}

/// The feet in the frame of the stairs.
pub(super) fn feet_on(p: &Pilot, set: &Structures, id: u64) -> Vec3 {
    let (feet, _) = p.feet();
    set.get(id).unwrap().to_local(feet)
}

#[test]
fn a_step_is_walked_up_and_down_and_what_is_too_high_is_not() {
    let (mut p, set, id) = stairs();
    let c = Controls::new(&p.def);
    let step = 1. / 60.;
    let walk = Input { forward: 1.0, ..Default::default() };
    // up the first step (0.3 m, its face at z 1.5) without a jump, never off our feet
    let (mut steps, mut aloft) = (Vec::new(), 0);
    for _ in 0..170 {
        p.step(step, walk, &c, Some(&set));
        if p.stepped > 0.0 {
            steps.push((p.stepped, feet_on(&p, &set, id).z));
        }
        aloft += usize::from(!p.grounded);
    }
    let f = feet_on(&p, &set, id);
    eprintln!("tras 170 pasos: pies {f:.2?}; escalones {steps:.2?}; {aloft} sin suelo");
    assert!(f.z > 4.0 && (f.y - 0.3).abs() < 0.03, "no sube el primer escalón: pies en {f:.2?}");
    assert!(!steps.is_empty() && steps.iter().all(|s| s.0 > 0.02 && s.0 < 0.34), "escalones {steps:.2?}");
    assert_eq!(aloft, 0, "pierde el suelo al subir");
    // (it did not cost speed: 170 ticks at 1.8 m/s are 5.1 m)
    assert!(f.z > 4.9, "el escalón lo frena: {:.2} m en 170 pasos", f.z);
    // the eyes come up after the feet, not at once
    assert!(p.sink.abs() < 0.3);
    // the second one, then the block on it (0.75 m): that one is a wall
    for _ in 0..400 {
        p.step(step, walk, &c, Some(&set));
    }
    let f = feet_on(&p, &set, id);
    eprintln!("contra el bloque: pies {f:.2?}");
    assert!((f.y - 0.6).abs() < 0.03 && f.z < 10.0 - 0.375 - 0.25 && f.z > 9.0, "pies en {f:.2?}");
    // back down: on our feet all the way (the legs reach down: no floating off the edge in
    // lunar gravity), and down on the floor soon after each edge
    p.yaw += std::f64::consts::PI;
    let (mut aloft, mut low) = (0, None);
    for k in 0..300 {
        p.step(step, walk, &c, Some(&set));
        aloft += usize::from(!p.grounded);
        let f = feet_on(&p, &set, id);
        if low.is_none() && f.y < 0.02 {
            low = Some((k, f.z));
        }
    }
    let f = feet_on(&p, &set, id);
    eprintln!("de vuelta: pies {f:.2?}; abajo en {low:?}; {aloft} sin suelo");
    assert!(f.y.abs() < 0.02 && f.z < 1.0, "no baja: pies en {f:.2?}");
    assert_eq!(aloft, 0, "pierde el suelo al bajar");
    assert!(low.is_some_and(|l| l.1 > 0.9), "tarda en bajar el último escalón: {low:?}");
    // a jump over the same edge is a jump: off our feet
    let (mut p, set, _) = stairs();
    p.step(step, Input { jump: true, ..Default::default() }, &c, Some(&set));
    p.step(step, Input::default(), &c, Some(&set));
    assert!(!p.grounded, "un salto no despega");
}

#[test]
fn hands_off_the_pack_brakes_you_in_under_a_second() {
    // aloft with the pack on, going sideways at a run's speed and more; keys let go
    let mut p = pilot();
    let c = Controls::new(&p.def);
    let up = p.bodies.get(p.body).up(p.position);
    let side = up.any_orthonormal_vector();
    p.position += up * 30.0;
    (p.grounded, p.aloft, p.pack_on) = (false, true, true);
    p.drift = side * 6.0;
    let step = 1.0 / 60.0;
    let mut stopped = None;
    for k in 0..180 {
        p.step(step, Input::default(), &c, None);
        if stopped.is_none() && p.drift.length() < 0.06 {
            stopped = Some(f64::from(k + 1) * step);
        }
    }
    let j = p.def.mochila.expect("the scenario's suit has a jet pack");
    let took = stopped.expect("it never stops us");
    assert!(took < 1.0 && (took - 6.0 / j.frenada).abs() < 0.1, "from 6 m/s it takes {took:.2} s to stop us");
    // (and it does not push us back the other way, nor let us fall)
    assert!(p.drift.length() < 0.06 && p.vertical_speed().abs() < 0.2);
    // with the steadying off nothing brakes us
    p.toggle_steady();
    p.drift = side * 6.0;
    for _ in 0..60 {
        p.step(step, Input::default(), &c, None);
    }
    assert!((p.drift.length() - 6.0).abs() < 0.05);
}

#[test]
fn the_ground_brakes_whoever_lands_with_speed() {
    let step = 1. / 60.;
    // on our feet with 6 m/s over the ground: stopped in a little over a second, in a few metres
    let mut p = pilot();
    let c = Controls::new(&p.def);
    let (forward, _, up) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    p.drift = ahead * 6.0;
    let from = p.position;
    let mut ticks = 0;
    while p.drift.length() > 0.01 && ticks < 600 {
        p.step(step, Input::default(), &c, None);
        ticks += 1;
    }
    let run = (p.position - from).dot(ahead);
    eprintln!("6 m/s en el suelo: parado en {:.2} s y {run:.2} m", ticks as f64 * step);
    assert!(ticks as f64 * step < 1.6 && run < 4.6 && run > 2.0, "{:.2} s, {run:.2} m", ticks as f64 * step);
    // coming down at 3 m/s with 6 m/s over the ground: the boots take a good part of it as
    // they land, and the rest is braked the same
    let mut p = pilot();
    p.position += up * 0.5;
    p.grounded = false;
    p.aloft = true;
    p.vertical_velocity = -3.0;
    p.drift = ahead * 6.0;
    let from = p.position;
    let mut landed = None;
    for k in 0..600 {
        p.step(step, Input::default(), &c, None);
        if p.landed > 0.0 {
            landed = Some((k, p.landed, p.drift.length()));
        }
        if p.grounded && p.drift.length() < 0.01 {
            break;
        }
    }
    let run = (p.position - from).dot(ahead);
    eprintln!("aterrizaje: {landed:.2?}; recorre {run:.2} m");
    let (_, hit, left) = landed.expect("no aterriza");
    assert!(hit > 3.0 && hit < 3.6, "golpe de {hit:.2} m/s");
    assert!(left < 6.0 - 0.9 * 3.0 + 0.2, "al tocar suelo sigue a {left:.2} m/s");
    assert!(run < 2.6, "se escurre {run:.2} m");
}

#[test]
fn the_pack_holds_your_height_when_you_let_go() {
    let mut p = pilot();
    let c = Controls::new(&p.def);
    let j = p.def.mochila.unwrap();
    let step = 1. / 60.;
    p.toggle_pack();
    for _ in 0..120 {
        p.step(step, Input { vertical: 1.0, ..Default::default() }, &c, None);
    }
    assert!(p.vertical_speed() > 3.0);
    // hands off: it brakes the climb and holds the height, burning what our weight asks
    for _ in 0..200 {
        p.step(step, Input::default(), &c, None);
    }
    let (high, fuel) = (p.altitude(), p.fuel);
    assert!(p.vertical_speed().abs() < 0.05, "sigue a {:.2} m/s", p.vertical_speed());
    for _ in 0..300 {
        p.step(step, Input::default(), &c, None);
    }
    let g = p.bodies.field(p.position).g();
    assert!((p.altitude() - high).abs() < 0.05, "no mantiene la altura: {:.2} -> {:.2}", high, p.altitude());
    let burnt = fuel - p.fuel;
    assert!((burnt - 5.0 * (g / j.empuje) / j.autonomia).abs() < 0.004, "gasta {burnt:.4} en 5 s");
    // Ctrl: down; let go: it stops again
    for _ in 0..60 {
        p.step(step, Input { vertical: -1.0, ..Default::default() }, &c, None);
    }
    assert!(p.vertical_speed() < -1.0);
    for _ in 0..180 {
        p.step(step, Input::default(), &c, None);
    }
    assert!(p.vertical_speed().abs() < 0.05 && !p.grounded && p.altitude() < high);
    // the steadying off: it lets you fall
    p.toggle_steady();
    for _ in 0..60 {
        p.step(step, Input::default(), &c, None);
    }
    assert!(p.vertical_speed() < -1.0 && p.jet == 0.0);
}

/// Frames as long as a game's are: no two the same, some shorter than a slice of the world,
/// some of two.
pub(super) const UNEVEN: [f64; 8] = [0.0069, 0.0111, 0.02, 0.0167, 0.0143, 0.009, 0.025, 0.0167];

/// The game's frame round a pilot, as `play` runs it: what the keys ask, then the world on, the
/// pilot among its structures.
pub(super) struct Loop {
    pub p: Pilot,
    pub set: Structures,
    pub c: Controls,
    /// How long each frame is (s), gone through again and again; the last one's length.
    pub frames: Vec<f64>,
    pub k: usize,
    pub dt: f64,
    pub now: f64,
    /// The world's own step (`Structures::simulate_with`: its physics, its gravity). Else the
    /// structures are moved by hand in the world's slices, each straight at its speed, sped up
    /// by `push` (m/s²) and, with `fall`, by its weight: nothing else moves them.
    pub world: bool,
    pub push: DVec3,
    pub fall: bool,
}

impl Loop {
    pub fn new(p: Pilot, set: Structures) -> Loop {
        let c = Controls::new(&p.def);
        Loop { p, set, c, frames: UNEVEN.to_vec(), k: 0, dt: 0.0, now: 0.0, world: false, push: DVec3::ZERO, fall: false }
    }

    /// A frame: the ship whose rooms we are in (`rooms`, as `play` asks it), what the keys ask,
    /// and the world on with us among its structures. The eye shown at the end of it.
    pub fn frame(&mut self, input: Input, rooms: &dyn Fn(&Pilot, &Structures) -> Option<u64>) -> DVec3 {
        self.frame_with(input, rooms, &mut [])
    }

    pub fn frame_with(&mut self, input: Input, rooms: &dyn Fn(&Pilot, &Structures) -> Option<u64>, others: &mut [&mut dyn Among]) -> DVec3 {
        let dt = self.frames[self.k % self.frames.len()];
        (self.k, self.dt, self.now) = (self.k + 1, dt, self.now + dt);
        self.p.cabin = rooms(&self.p, &self.set);
        self.p.begin(input, self.c);
        let bodies = self.p.bodies.clone();
        if self.world {
            let mut among: Vec<&mut dyn Among> = vec![&mut self.p];
            for other in others.iter_mut() {
                among.push(&mut **other);
            }
            self.set.simulate_with(self.now, dt, &bodies, &lunar_core::structure::schedule::Full, &mut among);
        } else {
            // the world's slices as it takes them: the structures on, then what lives among them
            let n = lunar_core::structure::physics::slices(dt as f32);
            for _ in 0..n {
                let dt = dt / n as f64;
                for other in others.iter_mut() {
                    other.before(&self.set);
                }
                for s in &mut self.set.list {
                    let weight = if self.fall { bodies.field(s.pos).pull } else { DVec3::ZERO };
                    // (how its speed changes is the structure's to say, as the world's physics
                    // says it of the ones it moves)
                    s.acc = self.push + weight;
                    s.vel += s.acc * dt;
                    // (as the world's physics moves a body: its centre of mass on, and it
                    // turning about that)
                    let com = s.to_world(s.com) + s.vel * dt;
                    s.rot = (Quat::from_scaled_axis(s.spin * dt as f32) * s.rot).normalize();
                    s.pos = com - (s.rot * s.com).as_dvec3();
                }
                Among::slice(&mut self.p, &self.set, &bodies, dt);
                for other in others.iter_mut() {
                    other.slice(&self.set, &bodies, dt);
                }
            }
        }
        self.p.view_aboard(&self.set).map_or_else(|| self.p.eye(), |v| v.eye)
    }
}

/// Aboard structure `id` where we are, as it goes: it carries us from the first step.
pub(super) fn aboard(p: &mut Pilot, set: &Structures, id: u64) {
    let s = set.get(id).unwrap();
    p.ride = Some(Ride { id, local: s.to_local(p.position), rot: s.rot });
}

/// In whatever structure's rooms we are, as `play` asks it.
pub(super) fn rooms(p: &Pilot, set: &Structures) -> Option<u64> {
    match p.ride {
        Some(r) => set.get(r.id).filter(|s| s.in_rooms(r.local)).map(|s| s.id),
        None => set.rooms_at(p.position),
    }
}

/// Structure `k` of `set` makes gravity of its own (`g` m/s², all of it there) and its inside is
/// everything within `size` m of its origin.
pub(super) fn with_gravity(set: &mut Structures, k: usize, g: f32, size: f32) {
    set.list[k].gravity = lunar_core::structure::state::OwnGravity { g, on: 1.0 };
    set.list[k].rooms = Arc::new([[Vec3::splat(-size), Vec3::splat(size)]]);
}

/// In no ship's rooms.
pub(super) fn outside(_: &Pilot, _: &Structures) -> Option<u64> {
    None
}

/// A structure high over the ground (where the Moon's pull is whole) going at `speed` level and
/// climbing a little, and us in the air 40 m over it with the pack on: (the game's frame, the
/// structure's id, the way it goes).
fn flying_by(speed: f64) -> (Loop, u64, DVec3) {
    let mut p = pilot();
    let lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
    let name = lib.blueprints.first().map(|b| b.0.clone()).expect("a blueprint");
    let mut set = Structures::new(Arc::new(lib));
    let up = p.bodies.get(p.body).up(p.position);
    let side = up.any_orthonormal_vector();
    let at = p.position + up * 12_000.0;
    let id = set.spawn(&name, at - up * 40.0, glam::Quat::IDENTITY).unwrap();
    set.list[0].vel = side * speed + up * 2.0;
    p.position = at;
    (p.grounded, p.aloft, p.pack_on) = (false, true, true);
    (Loop::new(p, set), id, side)
}

#[test]
fn a_ship_left_stays_where_it_is_at_any_speed_and_any_frame_rate() {
    let rates: [&[f64]; 7] = [&UNEVEN, &[1.0 / 30.0], &[1.0 / 60.0], &[1.0 / 144.0], &[1.0 / 240.0], &[0.05, 0.004, 0.004], &[0.1]];
    // (the ship moved by hand at a steady speed, and by the world's own physics, falling)
    for world in [false, true] {
        for frames in rates {
            for speed in [0.0, 30.0, 300.0, 1600.0, 7800.0] {
                let (mut g, id, _) = flying_by(speed);
                (g.frames, g.world) = (frames.to_vec(), world);
                aboard(&mut g.p, &g.set, id);
                // about a second in its rooms, then out of them: on our own beside it, hands off
                let (mut last, mut first, mut worst, mut left, mut t) = (None::<Vec3>, None, 0.0_f32, None, 0.0);
                while t < 7.0 {
                    let inside = t < 1.0;
                    let eye = g.frame(Input::default(), &|p, _| p.ride.filter(|_| inside).map(|r| r.id));
                    t += g.dt;
                    let eye = g.set.list[0].to_local(eye);
                    if let Some(l) = last {
                        worst = worst.max(eye.distance(l));
                    }
                    last = Some(eye);
                    first.get_or_insert(eye);
                    if left.is_none() && g.p.ride.is_none() {
                        left = Some(t);
                    }
                }
                let gone = last.unwrap().distance(first.unwrap());
                eprintln!("a {speed} m/s, fotogramas de {:.1} ms, {}: fuera a los {left:.2?} s; el mayor salto del ojo frente a la nave {worst:.4} m; al final a {gone:.2} m de donde estaba", frames[0] * 1e3, if world { "la física del mundo" } else { "nave a mano" });
                assert!(left.is_some_and(|t| t > 1.0 && t < 1.25), "sale a los {left:?} s");
                assert!(worst < 0.03, "a {speed} m/s la nave salta {worst:.2} m de un fotograma a otro");
                assert!(gone < 1.0, "a {speed} m/s la nave se nos va {gone:.2} m en 6 s");
                // (and we do go as fast as it does)
                assert!((g.p.velocity() - g.set.list[0].vel).length() < 0.5, "vamos a {:.1?}", g.p.velocity());
            }
        }
    }
}

#[test]
fn what_goes_as_we_do_is_kept_to_though_we_never_rode_it() {
    for speed in [300.0, 1600.0] {
        // beside a ship we never were aboard, going as it goes
        let (mut g, id, _) = flying_by(speed);
        let (v, up) = (g.set.list[0].vel, g.p.up);
        (g.p.vertical_velocity, g.p.drift) = (v.dot(up), v - up * v.dot(up));
        assert_eq!(g.p.hold, Hold::Still);
        // to the eye we go on as we went, and the ship stays where it is, from the first frame
        let (mut last, mut ship, mut bump, mut worst) = (g.p.eye(), None::<Vec3>, 0.0_f64, 0.0_f32);
        for _ in 0..300 {
            let eye = g.frame(Input::default(), &outside);
            bump = bump.max((eye - last - v * g.dt).length());
            last = eye;
            let eye = g.set.list[0].to_local(eye);
            if let Some(l) = ship {
                worst = worst.max(eye.distance(l));
            }
            ship = Some(eye);
        }
        eprintln!("junto a una nave a {speed} m/s en la que nunca estuvimos: el ojo se aparta {bump:.4} m de su camino; la nave salta {worst:.4} m");
        // (and it is that ship the pack keeps us beside: not the ground, far under and standing)
        assert_eq!(g.p.beside(), Some(id), "la mochila no nos tiene junto a ella");
        assert!(bump < 0.01, "el ojo salta {bump:.3} m");
        assert!(worst < 0.03, "la nave salta {worst:.3} m de un fotograma a otro");
    }
}

#[test]
fn in_the_rooms_of_a_ship_never_boarded_it_carries_us() {
    for speed in [0.0, 300.0, 1600.0] {
        // in the air in the rooms of a ship we never stood in (flown into), going as it goes; a
        // ship that makes its own gravity: aboard it one weighs that, whatever the ship does
        let (mut g, id, side) = flying_by(speed);
        with_gravity(&mut g.set, 0, 1.62, 80.0);
        let (v, up) = (g.set.list[0].vel, g.p.up);
        (g.p.vertical_velocity, g.p.drift) = (v.dot(up), v - up * v.dot(up));
        let inside = |_: &Pilot, _: &Structures| Some(id);
        let (mut ship, mut worst) = (None::<Vec3>, 0.0_f32);
        for k in 0..300 {
            // (then it speeds up at 15 m/s² for some three seconds: it takes us with it)
            g.push = if (100..300).contains(&k) { side * 15.0 } else { DVec3::ZERO };
            let eye = g.frame(Input::default(), &inside);
            let eye = g.set.list[0].to_local(eye);
            if let Some(l) = ship {
                worst = worst.max(eye.distance(l));
            }
            ship = Some(eye);
        }
        let moved = ship.unwrap().distance(Vec3::new(0.0, 40.0, 0.0));
        eprintln!("en las salas de una nave a {speed} m/s en la que nunca estuvimos: la nave salta {worst:.4} m; acabamos a {moved:.2} m de donde entramos");
        assert!(g.p.ride.is_some_and(|r| r.id == id), "la nave no nos lleva");
        assert!(worst < 0.03, "la nave salta {worst:.3} m de un fotograma a otro");
        assert!(moved < 0.5, "la nave se nos va {moved:.2} m al acelerar");
    }
}

#[test]
fn a_ship_that_speeds_up_leaves_us_without_a_jolt() {
    // (the pack steadying us: it goes after the ship as it can; not: we go on as we went)
    for steady in [true, false] {
        let (mut g, id, side) = flying_by(300.0);
        aboard(&mut g.p, &g.set, id);
        g.p.steady = steady;
        // to the eye: how far each frame it is from where it was going
        let (mut was, mut jolt, mut ours) = (None::<(DVec3, DVec3)>, 0.0_f64, DVec3::ZERO);
        for k in 0..500 {
            let inside = k < 70;
            // out of it, it speeds up at 15 m/s² for some three seconds
            g.push = if (100..300).contains(&k) { side * 15.0 } else { DVec3::ZERO };
            let eye = g.frame(Input::default(), &|p, _| p.ride.filter(|_| inside).map(|r| r.id));
            if k == 90 {
                ours = g.p.velocity();
            }
            if let Some((at, going)) = was.filter(|_| k > 90) {
                jolt = jolt.max((eye - at - going * g.dt).length());
            }
            was = Some((eye, was.map_or(DVec3::ZERO, |(at, _)| (eye - at) / g.dt)));
        }
        let apart = g.set.list[0].to_local(was.unwrap().0).length();
        let gained = (g.p.velocity() - ours).dot(side);
        eprintln!("la nave acelera y nos deja (estabilizador {steady}): el ojo se aparta {jolt:.4} m de por donde iba; acaba a {apart:.0} m; ganamos {gained:.1} m/s");
        assert!(apart > 60.0, "la nave no se va: {apart:.1} m");
        assert!(jolt < 0.012, "el ojo salta {jolt:.3} m mientras la nave se va");
        // on our own, what it does is nothing to us: not dragged along
        assert!(steady || gained.abs() < 0.05, "la nave nos arrastra: {gained:.2} m/s");
    }
}

#[test]
fn beside_a_ship_that_falls_we_fall_with_it() {
    // a ship with nothing holding it up (dropped, on a ballistic arc, in orbit) and us out of
    // it on the pack, hands off: we stay where we are to it, and the pack is not holding us up
    // (the ship falling by hand, and by the world's own physics)
    // (and with its steadying off: then nothing is done about it, and we fall as it does)
    for (world, steady) in [(false, true), (true, true), (true, false)] {
        for speed in [0.0, 300.0, 1600.0] {
            let (mut g, id, _) = flying_by(speed);
            aboard(&mut g.p, &g.set, id);
            (g.fall, g.world, g.p.steady) = (true, world, steady);
            let (mut last, mut first, mut worst, mut t, mut gas) = (None::<Vec3>, None, 0.0_f32, 0.0, 1.0);
            while t < 16.0 {
                // (a second in its rooms first, the pack holding us off its deck; with the
                // steadying off we would be falling to that deck: out of them from the start)
                let inside = steady && t < 1.0;
                let eye = g.frame(Input::default(), &|p, _| p.ride.filter(|_| inside).map(|r| r.id));
                t += g.dt;
                let eye = g.set.list[0].to_local(eye);
                if let Some(l) = last {
                    worst = worst.max(eye.distance(l));
                }
                last = Some(eye);
                if t > 3.0 {
                    first.get_or_insert(eye);
                } else {
                    gas = g.p.fuel;
                }
            }
            let gone = last.unwrap().distance(first.unwrap());
            let burnt = gas - g.p.fuel;
            eprintln!("junto a una nave que cae a {speed} m/s ({}, estabilizador {steady}): el mayor salto del ojo {worst:.4} m; en 13 s nos apartamos {gone:.2} m; gas gastado {burnt:.4}", if world { "la física del mundo" } else { "a mano" });
            assert!(g.p.ride.is_none() && g.p.beside() == Some(id));
            assert!(worst < 0.03, "a {speed} m/s la nave salta {worst:.2} m de un fotograma a otro");
            assert!(gone < 0.5, "a {speed} m/s la nave que cae se nos va {gone:.2} m");
            // (holding our weight for those 13 s would burn 13 x g / empuje / autonomia of it: 0.11)
            assert!(burnt < 0.02, "la mochila nos sostiene mientras la nave cae: gasta {burnt:.3}");
        }
    }
}

#[test]
fn down_on_the_ground_with_speed_nothing_jolts() {
    // in the air just over the ground at 40 m/s, as off a ship that is gone
    let mut p = pilot();
    let (forward, _, up) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    let lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
    p.position += up * 0.6;
    (p.grounded, p.aloft) = (false, true);
    (p.drift, p.hold) = (ahead * 40.0, Hold::Speed(ahead * 40.0));
    let mut g = Loop::new(p, Structures::new(Arc::new(lib)));
    let (mut last, mut speed, mut worst, mut down) = (g.p.eye(), 40.0, 0.0_f64, None);
    for k in 0..200 {
        let eye = g.frame(Input::default(), &outside);
        let now = (eye - last).dot(ahead) / g.dt;
        worst = worst.max((now - speed).abs());
        (last, speed) = (eye, now);
        if down.is_none() && g.p.grounded {
            down = Some((k, now));
        }
    }
    eprintln!("al suelo a 40 m/s: toca en {down:.1?}; el mayor cambio de velocidad del ojo de un fotograma a otro {worst:.2} m/s; acaba a {speed:.2} m/s");
    assert!(down.is_some() && g.p.hold == Hold::Still, "no toca el suelo");
    assert!(worst < 6.0, "el ojo da un tirón de {worst:.1} m/s al tocar el suelo");
    assert!(speed < 38.0, "el suelo no nos frena: {speed:.1} m/s");
}

#[test]
fn a_jump_on_a_deck_under_way_comes_down_where_it_went_up() {
    let (p, set, id) = stairs();
    let (forward, _, up) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    let mut g = Loop::new(p, set);
    // the deck going at 300 m/s and coming down at 3, with us on it
    g.set.list[0].vel = ahead * 300.0 - up * 3.0;
    aboard(&mut g.p, &g.set, id);
    for _ in 0..30 {
        g.frame(Input::default(), &outside);
    }
    assert!(g.p.grounded && g.p.ride.is_some_and(|r| r.id == id), "no nos lleva");
    let from = feet_on(&g.p, &g.set, id);
    g.frame(Input { jump: true, ..Default::default() }, &outside);
    let (mut high, mut hit, mut off) = (0.0_f32, None, 0);
    for k in 0..400 {
        g.frame(Input::default(), &outside);
        let f = feet_on(&g.p, &g.set, id);
        high = high.max(f.y - from.y);
        if hit.is_none() && g.p.landed > 0.0 {
            hit = Some((k, g.p.landed, f));
        } else if hit.is_some() {
            off += usize::from(!g.p.grounded);
        }
    }
    eprintln!("salto sobre una cubierta a 300 m/s: sube {high:.2} m; cae {hit:.2?} (salió de {from:.2?}); {off} fotogramas sin suelo después");
    let (_, speed, at) = hit.expect("no vuelve a la cubierta");
    assert!((high - 1.2).abs() < 0.1, "sube {high:.2} m");
    assert!(at.distance(from) < 0.05, "cae a {:.2} m de donde saltó", at.distance(from));
    // as hard as one comes down from that jump, not as the deck's own speed says; and no bounce
    let g_here = g.p.bodies.field(g.p.position).g();
    assert!((speed - (2.0 * g_here * g.p.def.jump_height).sqrt()).abs() < 0.2, "golpe de {speed:.2} m/s");
    assert_eq!(off, 0, "rebota al caer");
    assert!(g.p.ride.is_some_and(|r| r.id == id));
}

#[test]
fn steps_are_walked_the_same_on_a_deck_under_way() {
    // the stairs of `a_step_is_walked_up_and_down_and_what_is_too_high_is_not`, going at
    // 300 m/s and coming down at 3, in frames of any length
    let (p, set, id) = stairs();
    let (forward, _, up) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    let mut g = Loop::new(p, set);
    g.set.list[0].vel = ahead * 300.0 - up * 3.0;
    aboard(&mut g.p, &g.set, id);
    let walk = Input { forward: 1.0, ..Default::default() };
    // for `s` seconds: how many frames off our feet, and the hardest landing
    let go = |g: &mut Loop, s: f64| {
        let (mut t, mut aloft, mut hit) = (0.0, 0, 0.0_f64);
        while t < s {
            g.frame(walk, &outside);
            t += g.dt;
            aloft += usize::from(!g.p.grounded);
            hit = hit.max(g.p.landed);
        }
        (aloft, hit)
    };
    // up the first step without a jump, never off our feet, at a walk's pace
    let (aloft, _) = go(&mut g, 170.0 / 60.0);
    let f = feet_on(&g.p, &g.set, id);
    eprintln!("sobre una cubierta a 300 m/s, tras 2,8 s: pies {f:.2?}; {aloft} fotogramas sin suelo");
    assert!(f.z > 4.8 && (f.y - 0.3).abs() < 0.03, "no sube el primer escalón: pies en {f:.2?}");
    assert_eq!(aloft, 0, "pierde el suelo al subir");
    // the second, then the block on it: a wall
    go(&mut g, 400.0 / 60.0);
    let f = feet_on(&g.p, &g.set, id);
    assert!((f.y - 0.6).abs() < 0.03 && f.z < 10.0 - 0.375 - 0.25 && f.z > 9.0, "pies en {f:.2?}");
    // back down: on our feet all the way, no blow at any edge
    g.p.yaw += std::f64::consts::PI;
    let (aloft, hit) = go(&mut g, 300.0 / 60.0);
    let f = feet_on(&g.p, &g.set, id);
    eprintln!("de vuelta: pies {f:.2?}; {aloft} fotogramas sin suelo; el mayor golpe {hit:.2} m/s");
    assert!(f.y.abs() < 0.02 && f.z < 1.0, "no baja: pies en {f:.2?}");
    assert_eq!(aloft, 0, "pierde el suelo al bajar");
    assert!(hit < 0.5, "al bajar un escalón da un golpe de {hit:.2} m/s");
    assert!(g.p.ride.is_some_and(|r| r.id == id));
}

#[test]
fn set_on_a_deck_under_way_we_are_still_to_it() {
    // as a script's `ir` does: put there, and still to the ship
    let (mut p, set, id) = stairs();
    let (_, _, up) = p.directions();
    let (forward, _, _) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    let mut g = Loop::new(pilot(), set);
    g.set.list[0].vel = ahead * 300.0 - up * 3.0;
    for _ in 0..30 {
        g.frame(Input::default(), &outside);
    }
    // (two centimetres into the floor, a metre back along it: on it at once)
    p.put(g.set.list[0].to_world(Vec3::new(0.0, -0.02, -1.0)), up);
    p.still_to(&g.set, id);
    g.p = p;
    let mut worst = 0.0_f32;
    for _ in 0..120 {
        g.frame(Input::default(), &outside);
        worst = worst.max(feet_on(&g.p, &g.set, id).distance(Vec3::new(0.0, 0.0, -1.0)));
    }
    eprintln!("puesto sobre una cubierta a 300 m/s: los pies se apartan {worst:.3} m");
    assert!(g.p.ride.is_some_and(|r| r.id == id) && g.p.grounded, "no nos lleva");
    assert!(worst < 0.03, "puesto en la cubierta, se va {worst:.2} m");
}

#[test]
fn what_is_bumped_into_stops_you_against_itself_not_against_the_world() {
    let (p, set, id) = stairs();
    let (forward, _, up) = p.directions();
    let ahead = (forward - up * forward.dot(up)).normalize();
    let mut g = Loop::new(p, set);
    g.set.list[0].vel = ahead * 300.0;
    aboard(&mut g.p, &g.set, id);
    for _ in 0..20 {
        g.frame(Input::default(), &outside);
    }
    // off its deck on the pack: on our own beside it
    g.p.pack_on = true;
    for _ in 0..25 {
        g.frame(Input { vertical: 1.0, ..Default::default() }, &outside);
    }
    assert!(g.p.ride.is_none() && !g.p.grounded, "la mochila no nos despega");
    // set by the block on its second step, a little over the step, still to it
    let v = g.set.list[0].vel;
    g.p.position = g.set.list[0].to_world(Vec3::new(0.0, 0.75, 8.6)) + up * g.p.eye_h;
    (g.p.vertical_velocity, g.p.drift) = (v.dot(up), v - up * v.dot(up));
    // pushed at the block (ahead of us, the way everything goes): it stops us, and we go on with it
    // (looking level at it: the pack pushes the way we look)
    g.p.pitch = 0.0;
    for _ in 0..150 {
        g.frame(Input { forward: 1.0, ..Default::default() }, &outside);
    }
    let f = feet_on(&g.p, &g.set, id);
    eprintln!("contra el bloque a 300 m/s: pies {f:.2?}; vamos a {:.1} m/s", g.p.velocity().length());
    assert!(f.z > 9.2 && f.z < 9.4, "pies en {f:.2?}: el bloque está en z 9.625");
    assert!((g.p.velocity() - ahead * 300.0).length() < 3.0, "tras el golpe vamos a {:.1?}", g.p.velocity());
}

#[test]
fn in_the_air_in_a_cabin_under_way_the_ship_keeps_you() {
    // a real ship flying nose first, fast and at an orbit's speed, climbing a little; we stand
    // where its pilot gets up to
    let root = crate::root().join("assets/defs");
    let mut lib = lunar_core::structure::Library::load(&root.join("structures")).unwrap();
    let (ships, bps) = lunar_ship::ShipLibrary::load(&root, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let lib = Arc::new(lib);
    // (where the Moon's pull is whole, and past its reach: aboard it is the ship that gives the
    // weight, the same in both)
    for (speed, high) in [(300.0, 12_000.0), (1600.0, 12_000.0), (300.0, 45_000.0), (7800.0, 45_000.0)] {
        let mut p = pilot();
        let mut set = Structures::new(lib.clone());
        let up = p.bodies.get(p.body).up(p.position);
        let (forward, _, _) = p.directions();
        let ahead = (forward - up * forward.dot(up)).normalize();
        let id = set.spawn(&kind.blueprint, p.position + up * high, lunar_core::scene::basis(up, ahead)).unwrap();
        let mut ship = lunar_ship::Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
        ship.update(&mut set.list[0], &lunar_ship::World::default(), 0.0);
        // (its rooms and its gravity, as its systems give them once they run)
        set.list[0].rooms = kind.rooms.clone();
        set.list[0].gravity = lunar_core::structure::state::OwnGravity { g: kind.def.gravedad.g, on: 1.0 };
        set.list[0].vel = ahead * speed + up * 4.0;
        p.put(set.list[0].to_world(Vec3::from_array(kind.seats[0].def.salida)), up);
        aboard(&mut p, &set, id);
        let mut g = Loop::new(p, set);
        for _ in 0..40 {
            g.frame(Input::default(), &rooms);
        }
        assert!(g.p.grounded && g.p.ride.is_some_and(|r| r.id == id) && rooms(&g.p, &g.set).is_some(), "no estamos de pie en su cabina");
        // the pack on, a push up off the deck, then hands off and a nudge now and then
        g.p.pack_on = true;
        let (mut last, mut worst, mut lost, mut aloft) = (None::<Vec3>, 0.0_f32, 0, 0);
        for k in 0..300 {
            let input = Input { vertical: if k < 14 { 1.0 } else { 0.0 }, side: if k % 90 > 80 { 1.0 } else { 0.0 }, ..Default::default() };
            let eye = g.frame(input, &rooms);
            let eye = g.set.list[0].to_local(eye);
            if let Some(l) = last {
                worst = worst.max(eye.distance(l));
            }
            last = Some(eye);
            lost += usize::from(g.p.ride.is_none());
            aloft += usize::from(!g.p.grounded);
            // (where we are as the world has it is where we are in the ship: what asks by
            // one or the other is told the same)
            let apart = g.set.list[0].to_local(g.p.position).distance(g.p.ride.map_or(Vec3::ZERO, |r| r.local));
            assert!(apart < 0.02, "a {speed} m/s la posición va {apart:.2} m por detrás de la nave");
        }
        eprintln!("en el aire en la cabina a {speed} m/s: {aloft} fotogramas en el aire, {lost} sin que la nave nos lleve; el mayor salto del ojo {worst:.3} m");
        assert!(aloft > 200, "la mochila no nos tiene en el aire: {aloft} fotogramas");
        assert_eq!(lost, 0, "la nave nos suelta dentro de su cabina");
        assert!(worst < 0.05, "el ojo salta {worst:.2} m en la cabina");
    }
}
