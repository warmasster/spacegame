//! The Alcotán's looks from afar (`DetailLods`): six steps, each much lighter than the last; the
//! inside and every cable gone by the second; the last a far shape (never a box) as big as the
//! ship, wings and nacelles included, in its own colours (light hull, dark windows and tops).
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    structure::{
        Library,
        look::{DetailLods, FlatMesher, LodBuilder, Mesher, Vertex},
        state::Structure,
    },
};
use lunar_ship::{ShipLibrary, geom::Role};
use std::path::Path;

fn alcotan() -> (Library, Structure, std::sync::Arc<lunar_ship::ShipKind>) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap();
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    (lib, s, kind)
}

fn bounds(v: &[Vertex]) -> (Vec3, Vec3) {
    v.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), x| (lo.min(x.pos.into()), hi.max(x.pos.into())))
}

#[test]
fn seven_steps_down_to_a_coloured_far_shape() {
    let (lib, s, kind) = alcotan();
    // with its models (and every plain piece with its edges broken) up close; without them
    // from a little way off, and everything coarser made from that
    let (mut fine, mut full) = (Vec::new(), Vec::new());
    FlatMesher.mesh(&s, &lib.catalog, &mut fine);
    FlatMesher.basic(&s, &lib.catalog, &mut full);
    assert!(lunar_core::structure::look::detailed(&s, &lib.catalog), "the Alcotán has a finer look for close by");
    let lods = DetailLods::default();
    let far = lib.catalog.far_of(&s.name[..]).expect("the Alcotán has a far shape");
    let t = std::time::Instant::now();
    let mut levels = Vec::new();
    lods.build(&fine, Some(&full), Some(&far), &mut levels);
    eprintln!("de cerca {} triángulos; sin modelos {}", fine.len() / 3, full.len() / 3);
    assert!(fine.len() > full.len() && levels[0] == full, "the first level is the look without its models, as it is");
    // (the rest of this is about the levels from there on, as before there were models)
    let levels = levels.split_off(1);
    let tris: Vec<usize> = std::iter::once(full.len() / 3).chain(levels.iter().map(|l| l.len() / 3)).collect();
    eprintln!("niveles: {tris:?} triángulos (hechos en {:.0} ms)", t.elapsed().as_secs_f64() * 1e3);
    let d = lods.distances(s.radius, true);
    eprintln!("distancias: {d:?} m (radio {:.1} m)", s.radius);
    // its models to 40 m; as it is up to 200 m, then 600, 1200, 2400 and its far shape from
    // about 4.8 km
    assert_eq!(d.len(), 6);
    for (got, want) in d.iter().zip([40.0, 200.0, 600.0, 1200.0, 2400.0, 4800.0]) {
        assert!((got / want - 1.0).abs() < 0.03, "{d:?}");
    }
    assert_eq!(lods.distances(s.radius, false).len(), 5, "a structure without models has no such step");
    assert_eq!(lods.distances(3.0, true)[0], 40.0, "a small thing keeps its models to 40 m all the same");
    // what each level is made of
    for (k, l) in std::iter::once(&full).chain(levels.iter()).enumerate() {
        let mut by = std::collections::BTreeMap::new();
        for t in l.chunks_exact(3) {
            *by.entry((t[0].inner, t[0].size.min(30) / 5 * 5)).or_insert(0usize) += 1;
        }
        eprintln!("  nivel {k}: (dentro, talla cm) → triángulos {by:?}");
    }
    let mut roles = std::collections::BTreeMap::new();
    for (i, p) in s.parts.iter().enumerate() {
        let k = &lib.catalog.parts[usize::from(p.kind)];
        let e = roles.entry(format!("{:?}", kind.roles[i])).or_insert((0, 0, 0.0f32));
        e.0 += 1;
        e.1 += usize::from(k.def.interior);
        e.2 = e.2.max(k.size);
    }
    eprintln!("  por papel (partes, interiores, talla máx): {roles:?}");
    assert_eq!(tris.len(), 6, "six steps");
    assert!(tris.windows(2).all(|w| w[1] < w[0]), "each lighter: {tris:?}");
    assert!(tris[1] * 2 < tris[0] && levels[0].iter().all(|v| v.inner & 2 == 0), "no conduits from the second: {tris:?}");
    assert!(tris[5] <= 600 && tris[5] >= 60, "far shape {} triangles", tris[5]);
    // the outside only from level 2 on: no inside part, nothing thin
    for l in &levels[1..] {
        assert!(l.iter().all(|v| v.inner == 0 && v.size >= 8), "inside parts or cables left");
    }
    // as big as the ship (wings and nacelles in)
    let outside: Vec<Vertex> = full.iter().copied().filter(|v| v.inner == 0 && v.size >= 8).collect();
    let (a, b) = bounds(&outside);
    let (fa, fb) = bounds(&levels[4]);
    let (span, fspan) = (b - a, fb - fa);
    eprintln!("caja real {span:?}, lejana {fspan:?}");
    // as long and as wide (wings in); the thin legs under it it may leave out
    let k = fspan / span;
    assert!(k.x > 0.85 && k.x < 1.15 && k.z > 0.85 && k.z < 1.15 && k.y > 0.6 && k.y < 1.15, "far shape {fspan:?} vs ship {span:?}");
    // its own colours: light hull, and dark faces (windows, tops)
    let mut colors: Vec<[u8; 3]> = levels[4].iter().map(|v| v.albedo).collect();
    colors.sort_unstable();
    colors.dedup();
    let luma = |c: [u8; 3]| 0.3 * f32::from(c[0]) + 0.59 * f32::from(c[1]) + 0.11 * f32::from(c[2]);
    let (dark, light) = (colors.iter().filter(|c| luma(**c) < 70.0).count(), colors.iter().filter(|c| luma(**c) > 150.0).count());
    eprintln!("{} colores ({dark} oscuros, {light} claros)", colors.len());
    assert!(colors.len() >= 6 && dark >= 1 && light >= 1);
    // interior flags: cables in the cabin are inside, hull plates never
    let inner = |i: usize| lib.catalog.parts[usize::from(s.parts[i].kind)].def.interior;
    let n_in = (0..s.parts.len()).filter(|&i| inner(i)).count();
    eprintln!("{n_in} de {} partes son interiores", s.parts.len());
    assert!((0..s.parts.len()).all(|i| !(inner(i) && matches!(kind.roles[i], Role::Hull | Role::Glass))));
    assert!(n_in > s.parts.len() / 4, "{n_in} interiores");
}
