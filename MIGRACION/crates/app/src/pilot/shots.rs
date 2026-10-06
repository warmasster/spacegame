use super::{
    frames::{attitudes, pilot_in, places, worlds},
    tests::{Loop, aboard, rooms, with_gravity},
    *,
};
use lunar_core::{
    defs,
    rounds::{Flight, Impact, Rounds, round},
    structure::{
        Library,
        blueprint::{Blueprint, StructureDef},
        motion::Sweep,
    },
};

fn library() -> Arc<Library> {
    let mut lib = Library::load(&crate::root().join("assets/defs/structures")).unwrap();
    let def: StructureDef = defs::parse(
        "pared",
        r#"{
        "name": "Pared", "parts": [
            { "id": "pared", "part": "losa", "at": [0, 1.5, 12], "rot": [90, 0, 0] }
        ]
    }"#,
    )
    .unwrap();
    lib.blueprints.push(("pared".into(), Blueprint::new(&def, &lib.catalog).unwrap()));
    Arc::new(lib)
}

#[test]
fn rockets_and_the_eye_share_the_world_at_every_speed_frame_body_and_attitude() {
    let bodies = worlds();
    let lib = library();
    let mut worst_hit = 0.0f32;
    let mut worst_eye = 0.0f32;
    let mut cases = 0;
    for (place, at, up) in places(&bodies) {
        for (attitude, rot, spin) in attitudes(up) {
            for fps in [10.0, 30.0, 60.0, 144.0, 240.0] {
                for world in [false, true] {
                    let mut reference: Option<(Vec3, Vec<Vec3>)> = None;
                    for speed in [0.0, 30.0, 300.0, 1600.0, 7800.0] {
                        let mut set = Structures::new(lib.clone());
                        let id = set.spawn("pared", at, rot).unwrap();
                        with_gravity(&mut set, 0, 1.62, 30.0);
                        set.list[0].vel = up.any_orthonormal_vector() * speed;
                        set.list[0].spin = spin;
                        let mut pilot = pilot_in(&bodies);
                        pilot.position = at;
                        pilot.up = (rot * Vec3::Y).as_dvec3();
                        pilot.fore = (rot * Vec3::Z).as_dvec3();
                        pilot.grounded = false;
                        aboard(&mut pilot, &set, id);
                        let muzzle = set.list[0].to_world(Vec3::new(0.0, 1.5, 0.0));
                        let dir = (rot * Vec3::Z).as_dvec3();
                        let mut shot = round(0, muzzle, dir, 150.0, 2500.0, 0, 0, 0.16, 0.0);
                        shot.vel += pilot.motion_in(&set).velocity_at(muzzle);
                        let mut game = Loop::new(pilot, set);
                        game.world = world;
                        game.fall = true;
                        game.push = up * 4.0;
                        game.set.list[0].force = (game.push * f64::from(game.set.list[0].mass)).as_vec3();
                        game.frames = vec![1.0 / fps];
                        let mut rounds = Rounds::new(1);
                        rounds.fire(shot);
                        let mut impacts = Vec::<Impact>::with_capacity(1);
                        let mut sweep = Sweep::default();
                        let mut eyes = Vec::new();
                        while impacts.is_empty() && game.now < 0.2 {
                            let eye = game.frame_with(Input::default(), &rooms, &mut [&mut Flight { rounds: &mut rounds, impacts: &mut impacts, sweep: &mut sweep }]);
                            eyes.push(game.set.list[0].to_local(eye));
                        }
                        assert_eq!(impacts.len(), 1, "{place}, {attitude}, {fps} fps, {speed} m/s, world={world}: atraviesa la pared");
                        let hit = impacts[0].surface.unwrap();
                        assert_eq!(hit.id, id);
                        if let Some((point, base_eyes)) = &reference {
                            let error = point.distance(hit.point);
                            worst_hit = worst_hit.max(error);
                            assert!(error < 0.001, "{place}, {attitude}, {fps} fps, {speed} m/s, world={world}: el impacto cambia {error} m");
                            assert_eq!(eyes.len(), base_eyes.len());
                            for (eye, base) in eyes.iter().zip(base_eyes) {
                                worst_eye = worst_eye.max(eye.distance(*base));
                                assert!(eye.distance(*base) < 0.001, "el ojo cambia con la velocidad: {} m", eye.distance(*base));
                            }
                        } else {
                            reference = Some((hit.point, eyes));
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    eprintln!("{cases} vuelos: impacto cambia como mucho {worst_hit:.7} m; ojo {worst_eye:.7} m");
}

#[test]
fn a_release_uses_the_velocity_of_its_point_aboard_and_after_leaving() {
    let bodies = worlds();
    let lib = library();
    for speed in [0.0, 30.0, 300.0, 1600.0, 7800.0] {
        let mut set = Structures::new(lib.clone());
        let id = set.spawn("pared", DVec3::new(4e6, 3e6, 2e6), Quat::from_rotation_z(2.2)).unwrap();
        set.list[0].vel = DVec3::X * speed;
        set.list[0].spin = Vec3::new(0.2, -0.3, 0.5);
        let mut pilot = pilot_in(&bodies);
        pilot.position = set.list[0].to_world(Vec3::new(1.0, 2.0, -3.0));
        aboard(&mut pilot, &set, id);
        pilot.vel = DVec3::new(0.4, 0.2, -0.1);
        let muzzle = pilot.position + DVec3::new(0.2, 1.5, 0.8);
        let expected = pilot.vel + set.list[0].velocity_at(muzzle);
        assert!(pilot.motion_in(&set).velocity_at(muzzle).distance(expected) < 1e-5);
        let released = pilot.velocity_in(&set);
        pilot.ride = None;
        pilot.vel = released;
        assert!(pilot.motion_in(&set).velocity_at(muzzle).distance(released) < 1e-8);
    }
}
