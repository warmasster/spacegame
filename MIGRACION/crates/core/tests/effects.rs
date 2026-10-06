//! Explosions on the real Moon: aim, blow up, the ground changes, particles fly and settle.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    effects::{EffectDefs, Effects},
};
use std::path::Path;

fn moon() -> BodyRegistry {
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
}

fn effect_defs() -> EffectDefs {
    EffectDefs::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")).unwrap()
}

#[test]
fn aim_at_the_ground() {
    let bodies = moon();
    let b = bodies.get(0);
    let eye = b.above_ground(DVec3::Y, 10.0);
    let down = (-b.up(eye) + DVec3::X).normalize();
    let (id, hit) = bodies.raycast(eye, down, 1000.0).unwrap();
    assert_eq!(id, 0);
    assert!(b.altitude(hit).abs() < 1e-3);
    assert!((hit.distance(eye) - 10.0 * 2f64.sqrt()).abs() < 3.0, "{}", hit.distance(eye));
    // looking up: nothing
    assert!(bodies.raycast(eye, b.up(eye), 1000.0).is_none());
}

#[test]
fn every_explosion_definition_is_valid() {
    let d = effect_defs();
    assert!(d.explosions.len() >= 2);
    assert!(d.explosions.iter().all(|(_, e)| e.charge.is_some() || !e.emitters.is_empty()));
    // a shot that carries a charge gets its blast made for it
    let (_, shell) = d.shots.iter().find(|(id, _)| id == "proyectil").unwrap();
    assert_eq!(shell.impact.as_deref(), Some("disparo:proyectil"));
}

#[test]
fn a_blast_digs_flashes_throws_and_everything_settles() {
    let bodies = moon();
    let b = bodies.get(0);
    let d = effect_defs();
    let mut fx = Effects::new(&d, 20_000);
    let dir = DVec3::Y;
    let at = b.above_ground(dir, 0.0);
    let before = b.height(dir);
    // 4 kg of TNT: R = 4^(1/3) m on the Moon, 0.4 R deep, dug with the blast (under its glow)
    let damage = fx.explode("granada", &bodies, 0, at).unwrap().expect("a charge hurts structures");
    assert!(fx.explode("no_existe", &bodies, 0, at).is_err());
    let r = 4f64.cbrt();
    assert!((damage.radius as f64 - 5.0 * r).abs() < 1e-3);
    assert_eq!(fx.pending_craters(), 0);
    let thrown = fx.particles.len();
    assert!(thrown > 50, "{thrown} particles");
    assert_eq!(fx.flashes().len(), 1);
    assert!(fx.shake(at) > 0.0 && fx.shake(at + DVec3::X * 1000.0) == 0.0);
    let dug = before - b.height(dir);
    assert!((dug - 0.4 * r).abs() < 1e-3, "dug {dug}");
    let mut lowest = f64::INFINITY;
    for _ in 0..(12 * 60) {
        fx.update(1.0 / 60.0, &bodies);
        for p in &fx.particles.list {
            lowest = lowest.min(b.altitude(p.pos));
        }
    }
    assert!(lowest > -0.5, "a particle went {lowest} m into the ground");
    assert!(fx.particles.is_empty(), "{} left", fx.particles.len());
    assert!(fx.flashes().is_empty());
    assert_eq!(fx.shake(at), 0.0);
}

#[test]
fn the_heaviest_menu_charge_stays_in_bounds() {
    let bodies = moon();
    let b = bodies.get(0);
    let d = effect_defs();
    let (_, shot) = d.shots.iter().find(|(_, s)| s.menu.is_some()).expect("a shot in the menu");
    let max = shot.menu.unwrap().max_tnt;
    let mut fx = Effects::new(&d, 20_000);
    let charge = lunar_core::detonation::ChargeDef { tnt: max, casing: 0.0 };
    fx.add_explosion("max", lunar_core::effects::ExplosionDef { name: "max".into(), charge: Some(charge), ..Default::default() }).unwrap();
    let dir = DVec3::new(0.3, 1.0, 0.2).normalize();
    let at = b.above_ground(dir, 0.0);
    let before = b.height(dir);
    let damage = fx.explode("max", &bodies, 0, at).unwrap().unwrap();
    let r = f64::from(max).cbrt();
    let dug = before - b.height(dir);
    println!("{max} kg: crater {r:.0} m, dug {dug:.0} m, reach {:.0} m, {} particles, shake {:.3}", damage.radius, fx.particles.len(), fx.shake(at));
    assert!((dug - 0.4 * r).abs() < 0.05 * r, "dug {dug}");
    assert!(fx.particles.len() < 1500, "{} particles", fx.particles.len());
    assert!(fx.shake(at) <= 0.09 + 1e-6);
}

#[test]
fn an_explosion_inherits_motion_without_stretching_or_braking_against_the_world() {
    use lunar_core::structure::motion::Motion;
    let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    let moonlet: BodyDef = defs::parse("luna_menor", include_str!("../../../assets/defs/bodies/luna_menor.jsonc")).unwrap();
    let other: BodyDef = defs::parse("prueba", r#"{ "name": "Prueba", "center": [4e6, 2.5e6, -3e6], "radius": 300000, "gravity": 3.7, "reach": { "to": 90000, "band": 35000 }, "north": [0.3, 1, 0.2], "horizon_depth": 100 }"#).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &moon).unwrap(), Body::from_def("luna_menor", &moonlet).unwrap(), Body::from_def("prueba", &other).unwrap()]);
    let definitions = effect_defs();
    let mut worst = 0.0_f64;
    for (body, celestial) in bodies.iter() {
        for height in [celestial.whole_to() * 0.4, celestial.whole_to() + celestial.band * 0.5, celestial.reach * 1.6] {
            let at = celestial.center + DVec3::Y * (celestial.radius + height);
            for fps in [10.0, 30.0, 60.0, 144.0, 240.0] {
                for speed in [0.0, 30.0, 300.0, 1600.0, 7800.0] {
                    let velocity = DVec3::new(speed, speed * 0.3, -speed * 0.4);
                    let mut still = Effects::new(&definitions, 2000);
                    let mut moving = Effects::new(&definitions, 2000);
                    still.explode("disparo:cohete", &bodies, body, at).unwrap();
                    moving.explode_moving("disparo:cohete", &bodies, body, Motion { at, vel: velocity, spin: DVec3::ZERO }, 1.0, 0.0).unwrap();
                    let dt = (1.0 / fps) as f32;
                    let displacement = velocity * f64::from(dt);
                    still.update(dt, &bodies);
                    moving.update(dt, &bodies);
                    assert_eq!(still.particles.len(), moving.particles.len());
                    for (base, carried) in still.particles.list.iter().zip(&moving.particles.list) {
                        let error = (carried.pos - base.pos - displacement).length();
                        worst = worst.max(error);
                        assert!(error < 0.001, "{body}, {height} m, {fps} fps, {speed} m/s: la nube queda {error} m atras");
                        assert_eq!(carried.vel, base.vel, "la velocidad heredada deforma o frena el efecto");
                    }
                    for (base, carried) in still.flashes().iter().zip(moving.flashes()) {
                        assert!((carried.pos - base.pos - displacement).length() < 0.001);
                    }
                    assert!((still.shake(at) - moving.shake(at + displacement)).abs() < 1e-5);
                }
            }
        }
    }
    eprintln!("225 explosiones: error maximo de herencia {worst:.9} m");
}

#[test]
fn inherited_particle_motion_is_bounded_in_cost_and_storage() {
    use lunar_core::structure::motion::Motion;
    let bodies = moon();
    let mut effects = Effects::new(&effect_defs(), 20_000);
    let at = DVec3::new(4e6, 3e6, 2e6);
    effects.explode_moving("disparo:cohete", &bodies, 0, Motion { at, vel: DVec3::X * 7800.0, spin: DVec3::ZERO }, 1.0, 0.0).unwrap();
    let mut particle = effects.particles.list[0];
    particle.life = 1000.0;
    effects.particles.clear();
    for _ in 0..20_000 {
        effects.particles.spawn(particle);
    }
    effects.update(1.0 / 240.0, &bodies);
    let storage = (effects.particles.list.as_ptr(), effects.particles.list.capacity());
    let started = std::time::Instant::now();
    for _ in 0..100 {
        effects.update(1.0 / 240.0, &bodies);
    }
    let ms = started.elapsed().as_secs_f64() * 10.0;
    assert_eq!(effects.particles.len(), 20_000);
    assert_eq!((effects.particles.list.as_ptr(), effects.particles.list.capacity()), storage);
    eprintln!("20.000 particulas con herencia: {ms:.3} ms/paso; {} bytes/particula", std::mem::size_of::<lunar_core::particles::Particle>());
}
