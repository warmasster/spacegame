//! The generated ground has no cliffs: walking 1 m never climbs a wall. Searched where cliffs would
//! hide (the edges of the maria, where crater odds change) and anywhere else.
use glam::DVec3;
use lunar_core::{body::Body, defs, surface::Surface};

fn moon() -> Body {
    let def = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
    Body::from_def("luna", &def).unwrap()
}

/// Deterministic directions spread over the sphere.
fn directions(n: usize) -> impl Iterator<Item = DVec3> {
    (0..n).map(move |i| {
        let t = (i as f64 + 0.5) / n as f64;
        let z = 1.0 - 2.0 * t;
        let a = i as f64 * 2.399_963;
        let s = (1.0 - z * z).sqrt();
        DVec3::new(s * a.cos(), z, s * a.sin())
    })
}

/// The biggest jump along three straight walks through `at`: steps over 1 m tall in `step` are
/// looked at again 1 cm at a time, and only a jump that stays sharp counts (steep is fine).
fn worst_step(s: &dyn Surface, radius: f64, at: DVec3, length: f64, step: f64) -> (f64, DVec3) {
    let east = DVec3::Y.cross(at).normalize_or(DVec3::X);
    let north = at.cross(east);
    let h = |t: DVec3, x: f64| s.sample((at + t * (x / radius)).normalize().to_array(), 2.0).height;
    let mut worst = (0.0, at);
    for t in [east, north, (east + north).normalize()] {
        let n = (length / step) as i32;
        let mut last = h(t, f64::from(-n / 2) * step);
        for k in -n / 2 + 1..n / 2 {
            let x = f64::from(k) * step;
            let now = h(t, x);
            if f64::abs(now - last) > 1.0 {
                let mut prev = last;
                for c in 1..=100 {
                    let y = x - step + step * f64::from(c) / 100.0;
                    let v = h(t, y);
                    if f64::abs(v - prev) > worst.0 {
                        worst = (f64::abs(v - prev), (at + t * (y / radius)).normalize());
                    }
                    prev = v;
                }
            }
            last = now;
        }
    }
    worst
}

#[test]
fn no_cliffs_where_crater_odds_change() {
    let body = moon();
    let s = body.surface.clone().unwrap();
    let r = body.radius;
    // the edges of the maria: albedo between highland and mare
    let edges: Vec<DVec3> = directions(4000)
        .filter(|d| {
            let a = s.sample(d.to_array(), 3000.0).albedo;
            (0.74..0.94).contains(&a)
        })
        .take(64)
        .collect();
    assert!(edges.len() >= 8, "found {} mare edges", edges.len());
    let mut worst = (0.0, DVec3::ZERO);
    for d in edges.iter().copied().chain(directions(12)) {
        let w = worst_step(s.as_ref(), r, d, 3000.0, 1.0);
        if w.0 > worst.0 {
            worst = w;
        }
    }
    println!("worst 1 cm jump: {:.3} m at {:?}", worst.0, worst.1);
    assert!(worst.0 < 0.25, "a {:.1} m cliff at {:?}", worst.0, worst.1);
}
