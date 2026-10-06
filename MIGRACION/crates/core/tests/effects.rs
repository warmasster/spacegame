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
