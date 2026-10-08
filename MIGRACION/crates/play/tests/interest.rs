//! Who knows what, asked of the index (`Interest::update_in`) and asked of every structure
//! (`Interest::update`): the same, step after step, for players standing, walking and flying at
//! orbital speed among thousands of things of every size, still and fast; and what each costs.
use glam::{DVec3, Quat};
use lunar_play::{
    defs::Defs,
    game::Game,
    interest::{Index, Interest, Rule},
};

fn dice(seed: &mut u64) -> f64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn the_index_knows_what_looking_at_everything_knows_and_costs_far_less() {
    let defs = Defs::load(&lunar_play::root().join("assets/defs")).unwrap();
    let mut g = Game::new_apart(&defs, &lunar_play::root().join("assets/defs"), 64, |_| true).unwrap();
    let set = &mut g.builds.set;
    let mut seed = 0x1234_5678_9ABC_DEF1u64;
    let n = 6000;
    let centre = DVec3::new(0.0, 1.74e6, 0.0);
    for k in 0..n {
        let pos = centre + DVec3::new(dice(&mut seed) - 0.5, (dice(&mut seed) - 0.5) * 0.2, dice(&mut seed) - 0.5) * 60_000.0;
        let id = set.spawn("torre", pos, Quat::IDENTITY).unwrap();
        let i = set.index_of(id).unwrap();
        let s = &mut set.list[i];
        // (most small and still; some big; some fast: falling rocks, ships going by)
        s.radius = if k % 50 == 0 { 5.0 + dice(&mut seed) as f32 * 40.0 } else { 0.2 + dice(&mut seed) as f32 * 1.5 };
        s.vel = if k % 40 == 0 { DVec3::new(dice(&mut seed) - 0.5, dice(&mut seed) - 0.5, dice(&mut seed) - 0.5) * 4000.0 } else { DVec3::ZERO };
    }
    let rule = Rule::default();
    // (eyes: standing, walking, flying at orbital speed)
    let eyes: Vec<(DVec3, DVec3)> = (0..16)
        .map(|k| {
            let at = centre + DVec3::new(dice(&mut seed) - 0.5, 0.0, dice(&mut seed) - 0.5) * 50_000.0;
            let vel = match k % 4 {
                0 => DVec3::ZERO,
                1 => DVec3::new(3.0, 0.0, 1.0),
                2 => DVec3::new(0.0, 0.0, 120.0),
                _ => DVec3::new(7800.0, 0.0, 0.0),
            };
            (at, vel)
        })
        .collect();
    let (mut brute, mut fast): (Vec<Interest>, Vec<Interest>) = (vec![Interest::default(); eyes.len()], vec![Interest::default(); eyes.len()]);
    let mut index = Index::default();
    let (mut came, mut went, mut came2, mut went2) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut t_brute, mut t_fast, mut known) = (0.0f64, 0.0f64, 0usize);
    let dt = 1.0 / 60.0;
    for step in 0..120 {
        // (things move, eyes move)
        for s in &mut g.builds.set.list {
            s.pos += s.vel * dt;
        }
        let set = &g.builds.set;
        let began = std::time::Instant::now();
        index.build(&rule, set);
        t_fast += began.elapsed().as_secs_f64();
        for (k, (eye, vel)) in eyes.iter().enumerate() {
            let eye = *eye + *vel * (step as f64 * dt);
            let pinned: Vec<u64> = if k == 3 { vec![set.list[k * 7].id] } else { Vec::new() };
            (came.clear(), went.clear(), came2.clear(), went2.clear());
            let began = std::time::Instant::now();
            brute[k].update(&rule, set, eye, *vel, &pinned, dt as f32, &mut came, &mut went);
            t_brute += began.elapsed().as_secs_f64();
            let began = std::time::Instant::now();
            fast[k].update_in(&rule, set, &index, eye, *vel, &pinned, dt as f32, &mut came2, &mut went2);
            t_fast += began.elapsed().as_secs_f64();
            (came.sort_unstable(), came2.sort_unstable(), went.sort_unstable(), went2.sort_unstable());
            assert_eq!(came, came2, "step {step}, eye {k}: what comes");
            assert_eq!(went, went2, "step {step}, eye {k}: what goes");
            assert_eq!(brute[k].known.iter().map(|x| x.id).collect::<Vec<_>>(), fast[k].known.iter().map(|x| x.id).collect::<Vec<_>>(), "step {step}, eye {k}: what is known");
            known += fast[k].known.len();
        }
    }
    let per = |t: f64| t / 120.0 * 1000.0;
    println!(
        "{} structures, 16 players: looking at everything {:.2} ms a step, with the index {:.2} ms (built in it); {} known each on average",
        g.builds.set.list.len(),
        per(t_brute),
        per(t_fast),
        known / (120 * 16)
    );
    assert!(known > 0, "nobody knew anything: the scene is wrong");
    assert!(t_fast < t_brute * 0.6, "the index does not pay: {:.2} ms against {:.2} ms", per(t_fast), per(t_brute));
}
