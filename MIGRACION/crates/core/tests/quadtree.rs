//! Terrain LOD selection without a GPU: it settles, neighbours never differ by more than one
//! level (the morph closes one, a wall shows otherwise), it keeps up in flight, and ground changes
//! regenerate the nodes under them.
use glam::DVec3;
use lunar_core::{
    body::{Body, BodyDef},
    defs,
    quadtree::{QuadConfig, Quadtree},
};

fn moon() -> Body {
    let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    Body::from_def("luna", &def).unwrap()
}

const CFG: QuadConfig = QuadConfig { grid: 64, capacity: 1024, finest_cell: 0.4 };
const SPLIT: f64 = 1.75;
const BUDGET: usize = 12;

/// Frames until nothing is pending (None: never).
fn settle(q: &mut Quadtree, eye: DVec3, frames: usize) -> Option<usize> {
    (0..frames).find(|_| {
        q.update(eye, SPLIT, BUDGET, |_, _| true);
        q.pending == 0
    })
}

#[test]
fn settles_with_neighbours_one_level_apart() {
    let b = moon();
    for (alt, dir) in [(2.0, DVec3::Y), (300.0, DVec3::new(0.3, 1.0, -0.2)), (5000.0, DVec3::new(-1.0, 0.2, 0.9))] {
        let mut q = Quadtree::new(&b, b.surface.clone().unwrap(), CFG);
        let eye = b.above_ground(dir.normalize(), alt);
        let frames = settle(&mut q, eye, 3000).unwrap_or_else(|| panic!("never settled at {alt} m ({} pending)", q.pending));
        let step = q.worst_neighbour_step();
        let used = q.split(1.0);
        println!("{alt} m: settled in {frames} frames, {} drawn, {} cached, deepest {}, worst step {step}, split x{used:.2}", q.selected.len(), q.cached(), q.deepest);
        assert!(step <= 1, "{alt} m: neighbours {step} levels apart");
        assert!(used > 0.4, "{alt} m: detail dropped to x{used:.2} to fit the cache");
    }
}

#[test]
fn keeps_up_in_flight_without_thrashing() {
    let b = moon();
    let mut q = Quadtree::new(&b, b.surface.clone().unwrap(), CFG);
    let start = DVec3::Y;
    let east = DVec3::X;
    settle(&mut q, b.above_ground(start, 150.0), 3000).unwrap();
    let mut worst = 0;
    let mut late = 0;
    // 60 s at 300 m/s, 150 m up
    for f in 0..3600 {
        let d = (start + east * (300.0 * f64::from(f) / 60.0 / b.radius)).normalize();
        q.update(b.above_ground(d, 150.0), SPLIT, BUDGET, |_, _| true);
        if f % 10 != 0 {
            continue;
        }
        let s = q.worst_neighbour_step();
        worst = worst.max(s);
        late += usize::from(s > 1);
    }
    println!("flight: worst step {worst}, frames over one level {late}, evicted {}", q.evicted);
    assert!(late < 4, "{late} of 360 frames with a wall");
}

#[test]
fn ground_changes_regenerate_the_nodes_under_them() {
    let b = moon();
    let mut q = Quadtree::new(&b, b.surface.clone().unwrap(), CFG);
    let eye = b.above_ground(DVec3::Y, 5.0);
    settle(&mut q, eye, 3000).unwrap();
    q.update(eye, SPLIT, BUDGET, |_, _| true);
    assert_eq!(q.stale_len(), 0);
    q.invalidate(DVec3::Y, 10.0);
    let mut stale = Vec::new();
    while let Some(s) = q.next_stale() {
        stale.push(s);
    }
    assert!(stale.len() >= 6, "only {} nodes regenerated", stale.len());
    assert!(stale.iter().all(|(k, _)| k.face == 2));
}

/// Where the ground lies kilometres off the datum sphere the drawn node under a blast is still
/// regenerated (it was missed: craters dug the collision ground but never showed).
#[test]
fn ground_changes_reach_nodes_far_off_the_datum() {
    let b = moon();
    let s = b.surface.clone().unwrap();
    let dir = (0..4000)
        .map(|i| {
            let t = f64::from(i) * 0.37;
            DVec3::new(t.sin() * (t * 0.7).cos(), t.cos(), (t * 1.3).sin()).normalize()
        })
        .max_by(|a, c| s.sample(a.to_array(), 50.0).height.abs().total_cmp(&s.sample(c.to_array(), 50.0).height.abs()))
        .unwrap();
    let h = s.sample(dir.to_array(), 50.0).height;
    assert!(h.abs() > 1500.0, "no site far off the datum ({h} m)");
    let mut q = Quadtree::new(&b, s, CFG);
    let eye = b.above_ground(dir, 5.0);
    settle(&mut q, eye, 3000).unwrap();
    let under = q.selected_at(dir).unwrap().key;
    q.invalidate(dir, 8.0 * 2.2);
    let mut stale = Vec::new();
    while let Some((k, _)) = q.next_stale() {
        stale.push(k);
    }
    assert!(stale.contains(&under), "ground {h:.0} m off the datum: the drawn node (level {}) not regenerated", under.level);
}

/// Off screen the ground is refined to half the distance: the cache that frees goes to the view,
/// which keeps more of the asked detail.
#[test]
fn off_screen_ground_takes_less_of_the_cache() {
    let b = moon();
    let dir = DVec3::new(0.3, 1.0, -0.2).normalize();
    let eye = b.above_ground(dir, 2.0);
    let ahead = dir.any_orthonormal_vector();
    let mut all = Quadtree::new(&b, b.surface.clone().unwrap(), CFG);
    settle(&mut all, eye, 3000).unwrap();
    let mut seen = Quadtree::new(&b, b.surface.clone().unwrap(), CFG);
    let in_view = |rel: DVec3, r: f64| rel.dot(ahead) > -r;
    (0..3000)
        .find(|_| {
            seen.update(eye, SPLIT, BUDGET, in_view);
            seen.pending == 0
        })
        .unwrap();
    let there = (dir + ahead * (200.0 / b.radius)).normalize();
    let level = |q: &Quadtree| q.selected_at(there).unwrap().key.level;
    let (a, s) = (all.split(1.0), seen.split(1.0));
    println!("split x{a:.2} -> x{s:.2}, level ahead {} -> {}", level(&all), level(&seen));
    assert!(level(&seen) >= level(&all), "less detail where the camera looks");
    assert!(s > a * 1.15, "the view kept x{s:.2} of the detail (x{a:.2} before)");
}
