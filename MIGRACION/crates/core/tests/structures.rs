//! Structures from the real data: every build loads, holds together as one piece and stands on
//! the Moon's ground; rays find their parts.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    defs,
    structure::{Library, graph::Groups, set::Structures},
};
use std::{path::Path, sync::Arc};

fn library() -> Arc<Library> {
    Arc::new(Library::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs/structures")).unwrap())
}

fn moon() -> BodyRegistry {
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
}

#[test]
fn every_build_holds_together() {
    let lib = library();
    assert!(lib.blueprints.len() >= 3);
    for (id, bp) in &lib.blueprints {
        let mut g = Groups::default();
        g.build(bp.parts.len(), |_| true, bp.joints.iter().map(|j| (j.a, j.b)));
        let loose: Vec<_> = (0..bp.parts.len()).filter(|&i| g.label[i] != g.label[0]).map(|i| bp.ids[i].clone()).collect();
        println!("{id}: {} parts, {} joints", bp.parts.len(), bp.joints.len());
        assert_eq!(g.count, 1, "{id}: loose parts {loose:?}");
    }
}

#[test]
fn builds_stand_on_the_ground_and_rays_find_them() {
    let lib = library();
    let bodies = moon();
    let b = bodies.get(0);
    let mut set = Structures::new(lib.clone());
    let dir = DVec3::new(0.01, 1.0, -0.02).normalize();
    for (k, (id, _)) in lib.blueprints.iter().enumerate() {
        let at = (dir + DVec3::X * (k as f64 * 60.0 / b.radius)).normalize();
        set.place(id, &bodies, 0, at, 0.3, 0.0).unwrap();
    }
    for s in &set.list {
        assert!(s.mass > 1000.0, "{}: {} kg", s.name, s.mass);
        // the lowest corner sits on the ground under it or below (buried on a slope, never floating)
        let lowest = s.parts.iter().flat_map(|p| p.shape.verts().map(|v| s.to_world(p.local.transform_point3(v)))).map(|w| b.altitude(w)).fold(f64::MAX, f64::min);
        assert!(lowest < 0.05 && lowest > -4.0, "{}: lowest corner {lowest} m over the ground", s.name);
        // a ray from above at its centre hits it
        let c = s.to_world(s.center);
        let up = b.up(c);
        let (i, _, p) = set.raycast(c + up * 100.0, -up, 200.0).expect("a ray from above finds it");
        assert_eq!(set.list[i].id, s.id);
        assert!(p.distance(c) < f64::from(s.radius) + 1.0);
    }
}

#[test]
fn coarse_levels_shrink_stay_inside_and_damage_is_a_word_per_part() {
    use lunar_core::structure::look::{DecimatedLods, FlatMesher, LodBuilder, Mesher};
    let lib = library();
    let bodies = moon();
    let mut set = Structures::new(lib.clone());
    set.place("estacion", &bodies, 0, DVec3::Y, 0.0, 0.0).unwrap();
    let mut full = Vec::new();
    FlatMesher.mesh(&set.list[0], &lib.catalog, &mut full);
    let lods = DecimatedLods::default();
    let mut levels = Vec::new();
    lods.build(&full, None, None, &mut levels);
    assert_eq!(levels.len(), lods.levels.len() + 1);
    let tris: Vec<usize> = std::iter::once(full.len() / 3).chain(levels.iter().map(|l| l.len() / 3)).collect();
    println!("station levels: {tris:?} triangles");
    assert!(tris.windows(2).all(|w| w[1] <= w[0]) && tris.last() < tris.first(), "{tris:?}");
    // its far shape: a few boxes of its own (a building is never one box), far fewer triangles
    let far = *tris.last().unwrap();
    assert!(far >= 12 && far <= 24 * 12 && far * 4 < tris[0], "{tris:?}");
    let bound = |v: &[lunar_core::structure::look::Vertex]| v.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(lo, hi), x| (lo.min(x.pos.into()), hi.max(x.pos.into())));
    let (lo, hi) = bound(&full);
    for l in &levels {
        let (a, b) = bound(l);
        assert!(a.cmpge(lo - 0.01).all() && b.cmple(hi + 0.01).all());
    }
    // what changes while it lives is not in the look: a word per part says it
    use lunar_core::structure::look::{ALIVE, NO_PART, part_states};
    let n = set.list[0].parts.len();
    assert!(full.iter().all(|v| usize::from(v.part) < n && v.damage == 0));
    // the levels that only leave parts out still tell them apart; the far shape does not
    assert!(levels[0].iter().all(|v| v.part != NO_PART));
    assert!(levels.last().unwrap().iter().all(|v| v.part == NO_PART));
    let mut words = Vec::new();
    part_states(&set.list[0], n, &mut words);
    assert!(words.iter().all(|w| w & ALIVE != 0 && w & 0xff == 0));
    for p in &mut set.list[0].parts {
        p.hp = p.max_hp * 0.1;
    }
    set.list[0].parts[3].alive = false;
    part_states(&set.list[0], n, &mut words);
    assert_eq!(words[3], 0, "a part gone");
    assert!(words.iter().enumerate().all(|(i, w)| i == 3 || (w & ALIVE != 0 && (w & 0xff) > 200)), "the rest badly damaged");
    // and the look is the same whatever happened to its parts
    let mut hurt = Vec::new();
    FlatMesher.mesh(&set.list[0], &lib.catalog, &mut hurt);
    assert!(hurt == full, "the look does not change with damage");
}
