//! Rigid bodies on the real Moon: a loose building comes out of the ground and settles; blocks
//! land on a station's floor slab and on each other instead of falling through; a sleeper wakes
//! when something lands on it.
use glam::{Affine3A, DVec3, Quat};
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    structure::{
        Library,
        set::Structures,
        state::{Part, Structure},
    },
};
use std::{path::Path, sync::Arc};

fn world() -> (BodyRegistry, Structures) {
    let lib = Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap());
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    (BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]), Structures::new(lib))
}

/// A loose block (1.5 x 0.75 x 0.75 m) at `pos`, turned `rot`.
fn block(set: &mut Structures, pos: DVec3, rot: Quat) -> usize {
    let cat = &set.lib.catalog;
    let kind = cat.part("bloque").unwrap();
    let part = Part::new(cat, kind, Affine3A::IDENTITY, None, false);
    let id = set.next_id();
    set.list.push(Structure::assemble(id, "bloque".into(), pos, rot, false, vec![part], Vec::new()));
    set.list.len() - 1
}

fn lowest(set: &Structures, k: usize, bodies: &BodyRegistry) -> f64 {
    let s = &set.list[k];
    s.parts.iter().filter(|p| p.alive).flat_map(|p| p.shape.verts().map(|v| s.to_world(p.local.transform_point3(v)))).map(|w| bodies.get(0).altitude(w)).fold(f64::MAX, f64::min)
}

fn settle(set: &mut Structures, bodies: &BodyRegistry, secs: f32) {
    for _ in 0..(secs * 60.0) as usize {
        set.step(1.0 / 60.0, bodies);
    }
}

#[test]
fn a_loose_building_comes_out_of_the_ground_and_settles() {
    let (bodies, mut set) = world();
    set.place("torre", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    // set on a slope, its uphill side starts buried
    assert!(lowest(&set, 0, &bodies) < -0.5);
    set.list[0].anchored = false;
    set.list[0].resting = false;
    settle(&mut set, &bodies, 15.0);
    assert!(set.list[0].resting, "still moving at {:.2} m/s", set.list[0].vel.length());
    let low = lowest(&set, 0, &bodies);
    assert!(low > -0.15, "lowest corner {low:.2} m");
}

#[test]
fn blocks_land_on_a_slab_and_on_each_other() {
    let (bodies, mut set) = world();
    let b = bodies.get(0);
    set.place("estacion", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let st = &set.list[0];
    let (rot, up) = (st.rot, b.up(st.pos));
    // over a clear corner of the west slab (suelo#0, between the tanks and the antenna mast), 3 m
    // up; another over it
    let over = st.to_world(glam::Vec3::new(-8.6, 0.3, -2.4));
    let a = block(&mut set, over + up * 3.0, rot);
    let c = block(&mut set, over + up * 5.0, rot);
    settle(&mut set, &bodies, 12.0);
    let slab_top = b.altitude(over);
    let (la, lc) = (lowest(&set, a, &bodies), lowest(&set, c, &bodies));
    println!("slab top {slab_top:.2} m over the ground; blocks rest at {la:.2} and {lc:.2}");
    assert!((la - slab_top).abs() < 0.08, "the block went through the slab: {la:.2} vs {slab_top:.2}");
    assert!(lc > la + 0.6, "the top block went through the bottom one: {lc:.2}");
    assert!(set.list[a].resting && set.list[c].resting);
}

#[test]
fn a_sleeper_wakes_when_something_lands_on_it() {
    let (bodies, mut set) = world();
    let b = bodies.get(0);
    let at = b.above_ground(DVec3::Y, 0.4);
    let rot = scene_rot(&bodies, at);
    let a = block(&mut set, at, rot);
    settle(&mut set, &bodies, 3.0);
    assert!(set.list[a].resting);
    let up = b.up(at);
    let c = block(&mut set, at + up * 6.0, rot);
    let (mut woke, mut landed) = (false, f64::MAX);
    for _ in 0..240 {
        set.step(1.0 / 60.0, &bodies);
        woke |= !set.list[a].resting;
        // how close the falling one came to sitting on top (0.75 m: the lower one's height)
        landed = landed.min((lowest(&set, c, &bodies) - lowest(&set, a, &bodies) - 0.75).abs());
    }
    assert!(woke, "the block underneath never felt it");
    assert!(landed < 0.1, "it went through instead of landing on it ({landed:.2})");
}

fn scene_rot(bodies: &BodyRegistry, at: DVec3) -> Quat {
    let up = bodies.get(0).up(at);
    lunar_core::scene::basis(up, up.any_orthonormal_vector())
}

#[test]
fn the_smallest_push_counts_the_same_however_fast_it_goes() {
    // a block high over the ground pushed the way it goes with 5 mm/s² for two seconds, against
    // one left alone: standing still or at 7.8 km/s it ends a centimetre a second faster and a
    // centimetre ahead (kept as one number, a speed that big would swallow a push that small)
    for speed in [0.0, 1600.0, 7800.0] {
        let mut ends = Vec::new();
        for push in [0.0, 0.005] {
            let (bodies, mut set) = world();
            let k = block(&mut set, bodies.get(0).above_ground(DVec3::Y, 40_000.0), Quat::IDENTITY);
            set.list[k].vel = DVec3::X * speed;
            let mass = set.list[k].mass;
            for _ in 0..120 {
                set.list[k].force = glam::Vec3::X * (mass * push);
                set.step(1.0 / 60.0, &bodies);
            }
            ends.push((set.list[k].vel.x, set.list[k].pos.x));
        }
        let (gained, ahead) = (ends[1].0 - ends[0].0, ends[1].1 - ends[0].1);
        assert!((gained - 0.01).abs() < 2e-4, "at {speed} m/s it gains {gained:.5} m/s, not 0.01");
        assert!((ahead - 0.01).abs() < 5e-4, "at {speed} m/s it gets {ahead:.5} m ahead, not 0.01");
    }
}

#[test]
fn what_a_body_holds_is_that_body_to_what_runs_into_it() {
    // a block holding another beside it, and a third coming up on the held one from behind at
    // half a metre a second, all of them high up and going the same way: what happens between
    // them is the same standing still and at 1.6 km/s (the held one is part of its holder, not
    // something standing still in the world)
    let mut ends = Vec::new();
    for speed in [0.0, 1600.0] {
        let (bodies, mut set) = world();
        let at = bodies.get(0).above_ground(DVec3::Y, 40_000.0);
        let holder = block(&mut set, at, Quat::IDENTITY);
        let held = block(&mut set, at + DVec3::Y * 1.2, Quat::IDENTITY);
        let loose = block(&mut set, at + DVec3::Y * 1.2 - DVec3::X * 2.2, Quat::IDENTITY);
        let (a, b) = (set.list[holder].id, set.list[held].id);
        assert!(set.hold(b, a, 0));
        let go = DVec3::X * speed;
        for k in [holder, held] {
            set.list[k].vel = go;
        }
        set.list[loose].vel = go + DVec3::X * 0.5;
        for _ in 0..180 {
            set.step(1.0 / 60.0, &bodies);
        }
        let apart = set.list[loose].pos - set.list[held].pos;
        let rel = set.list[loose].vel - set.list[holder].vel;
        // it came up against the held block (1.5 m long: their middles no nearer than that) and
        // stayed with the two of them
        assert!(apart.x < -1.4 && apart.length() < 3.0, "at {speed} m/s the loose block ends {apart:.2?} from the held one");
        assert!(rel.length() < 0.6, "at {speed} m/s it ends going {rel:.2?} from them");
        ends.push((apart, rel));
    }
    let (still, fast) = (ends[0], ends[1]);
    assert!((still.0 - fast.0).length() < 0.15 && (still.1 - fast.1).length() < 0.15, "standing still {still:.3?}, at speed {fast:.3?}");
}

#[test]
fn bodies_that_close_fast_meet_instead_of_passing_through() {
    // two blocks (1.5 m long) closing head-on far from any body, in steps of a sixtieth of a
    // second: at 200 m/s they used to pass through each other unseen (3.3 m a step, more than
    // they measure together); now they meet whatever the speed, up to two opposite orbits
    for closing in [5.0, 50.0, 200.0, 500.0, 1000.0, 3000.0, 7800.0, 15_600.0] {
        let (bodies, mut set) = world();
        let at = bodies.get(0).above_ground(DVec3::Y, 200_000.0);
        let a = block(&mut set, at - DVec3::X * 30.0, Quat::IDENTITY);
        let b = block(&mut set, at + DVec3::X * 30.0, Quat::IDENTITY);
        set.list[a].vel = DVec3::X * (closing / 2.0);
        set.list[b].vel = -DVec3::X * (closing / 2.0);
        // (long enough to have crossed twice over had nothing stopped them)
        let steps = ((120.0 / closing + 0.5) * 60.0) as usize;
        for _ in 0..steps {
            set.step(1.0 / 60.0, &bodies);
        }
        let (pa, pb) = (set.list[a].pos.x, set.list[b].pos.x);
        assert!(pa < pb, "at {closing} m/s they went through each other: a at {:.1} m, b at {:.1} m", pa - at.x, pb - at.x);
        // and no further into each other than touching (their middles a block's length apart)
        assert!(pb - pa > 1.2, "at {closing} m/s they end {:.2} m apart", pb - pa);
    }
}

#[test]
fn what_passes_by_fast_is_not_stopped() {
    // the same two blocks going past each other 5 m apart sideways at orbital closing speed: the
    // way along is looked at, and nothing on it is met
    let (bodies, mut set) = world();
    let at = bodies.get(0).above_ground(DVec3::Y, 200_000.0);
    let a = block(&mut set, at - DVec3::X * 30.0, Quat::IDENTITY);
    let b = block(&mut set, at + DVec3::X * 30.0 + DVec3::Z * 5.0, Quat::IDENTITY);
    set.list[a].vel = DVec3::X * 3900.0;
    set.list[b].vel = -DVec3::X * 3900.0;
    for _ in 0..6 {
        set.step(1.0 / 60.0, &bodies);
    }
    assert!((set.list[a].vel.x - 3900.0).abs() < 1e-6 && (set.list[b].vel.x + 3900.0).abs() < 1e-6, "{:?} {:?}", set.list[a].vel, set.list[b].vel);
}

#[test]
fn what_comes_down_fast_stops_on_the_ground() {
    // a block dropped straight down at the Moon's ground from 2 km up at 50 to 3000 m/s (a rock
    // out of the sky): it ends on the ground, never under it
    for speed in [50.0, 300.0, 1000.0, 3000.0] {
        let (bodies, mut set) = world();
        let b = bodies.get(0);
        let dir = DVec3::new(0.3, 1.0, 0.2).normalize();
        let k = block(&mut set, b.above_ground(dir, 2000.0), Quat::IDENTITY);
        set.list[k].vel = -dir * speed;
        let steps = ((2000.0 / speed + 1.0) * 60.0) as usize;
        for _ in 0..steps {
            set.step(1.0 / 60.0, &bodies);
        }
        let low = lowest(&set, k, &bodies);
        let ground = b.altitude(b.above_ground(dir, 0.0));
        assert!(low > ground - 0.3, "at {speed} m/s it ends {:.2} m under the ground", ground - low);
        assert!(set.list[k].vel.dot(dir) > -speed * 0.5, "at {speed} m/s it is still going down at {:.1} m/s", -set.list[k].vel.dot(dir));
    }
}
