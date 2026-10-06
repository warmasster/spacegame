use lunar_core::{
    body::BodyDef,
    cube_sphere::*,
    defs,
    noise::*,
    surface::{Procedural, ProceduralDef, SurfaceDef},
};

/// The Moon as the data file declares it.
fn moon() -> BodyDef {
    defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap()
}

/// The TS terrain exactly: no rim noise or central peaks, every crater layer at full depth.
fn ts() -> ProceduralDef {
    let Some(SurfaceDef::Procedural(mut d)) = moon().surface else { panic!("the Moon has a procedural surface") };
    for l in &mut d.craters {
        l.depth = 1.0;
    }
    ProceduralDef { crater_detail: 0.0, crater_odds_fade: 0.0, ..d }
}

#[test]
fn natural_surface_matches_independent_ts_fixtures() {
    let data = include_str!("../../../reference/golden/surface.csv");
    let (ts, radius) = (ts(), moon().radius);
    let mut surfaces = Vec::new();
    for seed in [0, 1969, u32::MAX] {
        for level in [None, Some([0., 1., 0.])] {
            surfaces.push(Procedural::new(ts.clone(), radius, seed, level));
        }
    }
    let mut max_error = 0_f64;
    let mut count = 0;
    for line in data.lines().skip(1) {
        let v: Vec<f64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        let seed = v[1] as u32;
        let index = match seed {
            0 => 0,
            1969 => 2,
            _ => 4,
        } + v[2] as usize;
        let sample = surfaces[index].sample([v[4], v[5], v[6]], v[3]);
        let error = (sample.height - v[7]).abs();
        max_error = max_error.max(error);
        // Much tighter than the later 1 cm GPU limit: this verifies the f64 port itself.
        assert!(error < 1e-6, "case {}: height error {} m", v[0], error);
        assert!((sample.albedo - v[8]).abs() < 1e-8, "albedo case {}", v[0]);
        count += 1;
    }
    assert_eq!(count, 28800);
    println!("TS/Rust: {count} cases; max height error = {max_error:.12} m");
}

#[test]
fn noise_hashes_rng_and_craters_match_ts() {
    let mut current_seed = None;
    let mut n = Noise3::new(0);
    let mut rng = Random(0);
    let mut i = 0;
    for line in include_str!("../../../reference/golden/noise.csv").lines().skip(1) {
        let v: Vec<f64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        let seed = v[0] as u32;
        if current_seed != Some(seed) {
            current_seed = Some(seed);
            n = Noise3::new(seed);
            rng = Random(seed);
            i = 0;
        }
        let [x, y, z] = [v[1], v[2], v[3]];
        assert!((n.noise(x, y, z) - v[4]).abs() < 1e-12);
        assert_eq!(hash2i(x as i32 as u32, y as i32 as u32, seed), v[5] as u32);
        assert_eq!(hash3i(x as i32 as u32, y as i32 as u32, z as i32 as u32, seed), v[6] as u32);
        assert_eq!(rng.next_f64(), v[7]);
        assert!((crater_profile(f64::from(i) / 50., 70., f64::from(i) / 128.) - v[8]).abs() < 1e-12);
        i += 1;
    }
}

#[test]
fn cube_roundtrip_winding_and_shared_edges() {
    for (f, basis) in FACES.iter().enumerate() {
        let u = basis.u;
        let v = basis.v;
        assert_eq!([u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]], basis.n);
        for i in 0..=20 {
            for j in 0..=20 {
                let a = f64::from(i) / 10. - 1.;
                let b = f64::from(j) / 10. - 1.;
                let d = cube_dir(f, a, b);
                assert!((dot(d, d) - 1.).abs() < 1e-14);
                let uv = params_on(f, d).unwrap();
                assert!((uv[0] - a).abs() < 1e-14 && (uv[1] - b).abs() < 1e-14);
                let (other, uv) = face_of(d);
                let back = cube_dir(other, uv[0], uv[1]);
                for k in 0..3 {
                    assert!((back[k] - d[k]).abs() < 1e-14);
                }
                if i == 0 || i == 20 || j == 0 || j == 20 {
                    let neighbors = (0..6).filter(|&other| other != f).filter(|&other| params_on(other, d).is_some_and(|uv| uv[0].abs() <= 1. + 1e-14 && uv[1].abs() <= 1. + 1e-14)).count();
                    assert!(neighbors >= 1, "every boundary has an adjacent face");
                }
            }
        }
    }
}

#[test]
fn crater_support_and_reference_level() {
    for age in [0., 0.2, 0.8, 1.] {
        assert!(crater_profile(0., 70., age) < 0.);
        assert!(crater_profile(1.3, 70., age) > 0.);
        assert!(crater_profile(2.2, 70., age).abs() < 1e-12);
        assert!(crater_profile(2.2 - 1e-7, 70., age).abs() < 1e-8);
    }
    let radius = moon().radius;
    let s = Procedural::new(ts(), radius, 1969, Some([0., 1., 0.]));
    assert!(s.sample([0., 1., 0.], 0.).height.abs() < 1e-9);
    assert!(s.sample([0., -1., 0.], 0.).height.abs() > 1.);
    assert_eq!(cell_of(-1., 4), 0);
    assert_eq!(cell_of(1., 4), 15);
    assert_eq!(level_for(1e9, radius, 24), 0);
    assert_eq!(level_for(0., radius, 24), 24);
}
