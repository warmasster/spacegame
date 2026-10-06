//! The models (`assets/models`, built in Blender by `tools/modelos`) and what goes with them:
//! every kind of component, every plain part and every style a ship asks for has its own, each
//! keeps to the shape it is the look of, a mirrored one is its reflection; a window's frame is
//! solid and its pane is not; what is written on a part is read from close by, goes round a drum,
//! and says what its component is marked as.
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    font::Font,
    props::PropScene,
    structure::{
        Library,
        catalog::{BODY, ShapeDef},
        labels,
        look::{FlatMesher, GLASS_ROUGH, Mesher, opaque_len},
        state::Structure,
    },
};
use lunar_ship::{
    ShipKind, ShipLibrary, Sources,
    components::{model_name, style_model},
    def::ShipDef,
};
use std::{path::Path, sync::Arc};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn library() -> (Library, Vec<Arc<ShipKind>>) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships.kinds.clone())
}

/// Half the box a shape takes up.
fn half(shape: &ShapeDef) -> Vec3 {
    match *shape {
        ShapeDef::Box { size } | ShapeDef::Wedge { size } => Vec3::from_array(size) * 0.5,
        ShapeDef::Cylinder { radius, height, taper, .. } => Vec3::new(radius * taper.max(1.0), height * 0.5, radius * taper.max(1.0)),
        ShapeDef::Hull { ref points } => points.iter().fold(Vec3::ZERO, |m, p| m.max(Vec3::from_array(*p).abs())),
    }
}

#[test]
fn every_kind_every_part_and_every_style_has_its_model() {
    let lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let models = &lib.catalog.models;
    assert!(!models.is_empty(), "no hay modelos en assets/models");
    let src = Sources::load(&defs()).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let mut missing = Vec::new();
    for (id, kind) in &src.components {
        for p in &kind.piezas {
            let name = p.modelo.clone().unwrap_or_else(|| model_name(id, &p.id));
            if models.get(&name).is_none() {
                missing.push(name);
            }
        }
    }
    for part in &lib.catalog.parts {
        let name = format!("parte_{}/{BODY}", part.id);
        if models.get(&name).is_none() {
            missing.push(name);
        }
    }
    let ships: Vec<(String, ShipDef)> = lunar_core::defs::load_dir(&defs().join("ships")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let mut styled = 0;
    for (ship, def) in &ships {
        for c in &def.componentes {
            let (Some(style), Some(shape)) = (&c.estilo, &c.forma) else { continue };
            let shape: ShapeDef = serde_json::from_value(shape.clone()).unwrap_or_else(|e| panic!("{ship}/{}: {e}", c.id));
            let name = style_model(style, &shape).unwrap_or_else(|| panic!("{ship}/{}: un estilo necesita una caja, un cilindro o una cuña", c.id));
            styled += 1;
            if models.get(&name).is_none() {
                missing.push(format!("{name} ({ship}/{})", c.id));
            }
        }
    }
    missing.sort();
    missing.dedup();
    eprintln!("{} modelos; {styled} piezas con estilo", models.len());
    assert!(missing.is_empty(), "faltan modelos (blender -b -P tools/modelos/hacer.py --): {missing:?}");
    assert!(styled >= 30, "solo {styled} piezas con estilo");
}

#[test]
fn a_model_keeps_to_the_shape_it_is_the_look_of() {
    let lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let models = &lib.catalog.models;
    let src = Sources::load(&defs()).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let mut shapes: Vec<(String, ShapeDef)> = Vec::new();
    for (id, kind) in &src.components {
        shapes.extend(kind.piezas.iter().map(|p| (model_name(id, &p.id), p.forma.clone())));
    }
    shapes.extend(lib.catalog.parts.iter().map(|p| (format!("parte_{}/{BODY}", p.id), p.def.shape.clone())));
    let ships: Vec<(String, ShipDef)> = lunar_core::defs::load_dir(&defs().join("ships")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    for (_, def) in &ships {
        for c in &def.componentes {
            if let (Some(style), Some(shape)) = (&c.estilo, &c.forma) {
                let shape: ShapeDef = serde_json::from_value(shape.clone()).unwrap();
                shapes.push((style_model(style, &shape).unwrap(), shape));
            }
        }
    }
    let mut bad = Vec::new();
    let mut checked = 0;
    for (name, shape) in &shapes {
        let Some(model) = models.get(name) else { continue };
        checked += 1;
        let h = half(shape);
        let (lo, hi) = model.bounds();
        // it may round its shape off and stand proud of it (a handle, a hose, a rail, a nose
        // fairing), never be another size or sit elsewhere
        let slack = (0.32 * h.max_element()).max(0.06) + 0.15;
        let out = (-h - lo).max(hi - h).max_element();
        let fill = ((hi - lo) / (h * 2.0).max(Vec3::splat(1e-6))).max_element();
        if out > slack || fill < 0.5 {
            bad.push(format!("{name}: sale {:.0} cm (margen {:.0}), llena {:.0} %", out * 100.0, slack * 100.0, fill * 100.0));
        }
        assert!(!model.mesh.idx.is_empty() && model.mesh.pos.len() == model.mesh.nrm.len() && model.mesh.pos.len() == model.tint.len(), "{name}");
    }
    assert!(checked > 100, "solo {checked} modelos comprobados");
    assert!(bad.is_empty(), "{} modelos fuera de su forma: {bad:#?}", bad.len());
}

#[test]
fn a_mirrored_model_is_its_reflection_and_still_faces_out() {
    let lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let name = lib.catalog.models.names().find(|n| n.starts_with("estilo_ala/")).expect("el ala").to_string();
    let model = lib.catalog.models.get(&name).unwrap();
    let mirrored = model.mirrored();
    let ((lo, hi), (mlo, mhi)) = (model.bounds(), mirrored.bounds());
    assert!((mlo.x + hi.x).abs() < 1e-5 && (mhi.x + lo.x).abs() < 1e-5 && (mlo.y - lo.y).abs() < 1e-6 && (mhi.z - hi.z).abs() < 1e-6);
    // every triangle's face agrees with the normals at its corners, before and after
    let facing = |m: &lunar_core::mesh::Mesh| {
        let (mut agree, mut all) = (0, 0);
        for t in m.idx.chunks_exact(3) {
            let p = |k: usize| Vec3::from_array(m.pos[t[k] as usize]);
            let face = (p(1) - p(0)).cross(p(2) - p(0));
            if face.length_squared() < 1e-14 {
                continue;
            }
            all += 1;
            agree += usize::from(face.dot(Vec3::from_array(m.nrm[t[0] as usize])) > 0.0);
        }
        agree as f32 / all.max(1) as f32
    };
    let (a, b) = (facing(&model.mesh), facing(&mirrored.mesh));
    assert!(a > 0.98 && b > 0.98, "caras de frente: {a:.3} tal cual, {b:.3} reflejada");
}

#[test]
fn a_windows_frame_is_solid_and_its_pane_is_not() {
    let (lib, kinds) = library();
    let cat = &lib.catalog;
    let kind = kinds.iter().find(|k| k.id == "alcotan").unwrap();
    // the fine looks of its windows: a pane (smooth as glass) and a frame (not)
    let windows: Vec<&lunar_core::mesh::Mesh> = cat.looks.iter().filter(|(n, m)| n.starts_with("alcotan/casco.") && n.ends_with('+') && m.mat.iter().any(|x| x.rough < GLASS_ROUGH)).map(|(_, m)| m).collect();
    assert!(windows.len() >= 12, "{} ventanas con marco", windows.len());
    for m in &windows {
        let solid = m.mat.iter().filter(|x| x.rough >= GLASS_ROUGH).count();
        assert!(solid > 40 && solid < m.mat.len(), "marco {solid} de {} vértices", m.mat.len());
    }
    // meshed: the frames with everything solid, the panes in the tail that is drawn blended
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, cat, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut out = Vec::new();
    FlatMesher.mesh(&s, cat, &mut out);
    let solid = opaque_len(&out);
    assert!(solid > 0 && solid < out.len());
    let glass_part = |v: &lunar_core::structure::look::Vertex| cat.material(s.parts[usize::from(v.part)].kind).glass;
    let frames = out[..solid].iter().filter(|v| glass_part(v)).count();
    assert!(frames > 1000, "{frames} vértices de marco entre lo sólido");
    assert!(out[..solid].iter().all(|v| v.glass == 0) && out[solid..].iter().all(|v| v.glass != 0 && glass_part(v) && v.rough < GLASS_ROUGH), "lo traslúcido va al final, y solo el cristal");
    // from a little way off (the plain look) a window is all pane but its ground edge
    let mut basic = Vec::new();
    FlatMesher.basic(&s, cat, &mut basic);
    assert!(opaque_len(&basic) < basic.len());
}

#[test]
fn what_is_written_on_a_part_is_read_from_close_by_and_goes_with_it() {
    let (lib, kinds) = library();
    let cat = &lib.catalog;
    let kind = kinds.iter().find(|k| k.id == "alcotan").unwrap();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let s = Structure::new(1, bp, cat, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let texts: Vec<&str> = s.parts.iter().flat_map(|p| cat.parts[usize::from(p.kind)].def.labels.iter().map(|l| l.text.as_str())).collect();
    // its kinds' labels, what its bottles are marked as, its hull's stencils, a serial number
    for want in ["AGUA", "PROPELENTE", "O2", "N2", "RX-2", "▲ RESCATE ▲", "PELIGRO · CHORRO"] {
        assert!(texts.contains(&want), "falta el rótulo '{want}' (hay {})", texts.len());
    }
    assert!(!texts.iter().any(|t| t.contains('{')), "marcadores sin resolver: {:?}", texts.iter().filter(|t| t.contains('{')).collect::<Vec<_>>());
    let serials: Vec<&&str> = texts.iter().filter(|t| t.starts_with("N/S ")).collect();
    assert!(serials.len() >= 4 && serials.iter().any(|a| a != &serials[0]), "números de serie: {serials:?}");
    assert!(labels::any(&s, cat));
    let font = Font::parse(&std::fs::read_to_string(defs().join("../fonts/serigrafia.json")).unwrap()).unwrap();
    let (mut scene, mut scratch) = (PropScene::default(), Vec::new());
    // from the hold: read; from half a kilometre: nothing but the hull's biggest, if that
    let near = labels::show(&s, cat, &font, Vec3::new(0.5, 1.5, -5.5), |_| 0, &mut scratch, &mut scene);
    assert!(near > 60, "{near} letras desde la bodega");
    scene.glyphs.clear();
    let far = labels::show(&s, cat, &font, Vec3::new(0.0, 100.0, -500.0), |_| 0, &mut scratch, &mut scene);
    assert!(far == 0, "{far} letras desde 500 m");
    // what goes round a drum lies on its skin: every letter as far from its axis as its radius
    let (k, part) = s.parts.iter().enumerate().find(|(_, p)| cat.parts[usize::from(p.kind)].def.labels.iter().any(|l| l.text == "AGUA")).expect("el bidón de agua");
    let label = cat.parts[usize::from(part.kind)].def.labels.iter().find(|l| l.text == "AGUA").unwrap();
    assert!(label.radius > 0.2);
    scene.glyphs.clear();
    let eye = part.local.transform_point3(Vec3::new(0.0, 0.1, 1.5));
    labels::show(&s, cat, &font, eye, |_| 0, &mut scratch, &mut scene);
    // (its own letters: those by its label's anchor)
    let anchor = part.local.transform_point3(Vec3::from_array(label.at));
    scene.glyphs.retain(|g| g.center.distance(anchor) < 0.16);
    assert!(scene.glyphs.len() >= 4, "pieza {k}: {} letras", scene.glyphs.len());
    let to_part = part.local.inverse();
    for g in &scene.glyphs {
        let p = to_part.transform_point3(g.center);
        let r = (p.x * p.x + p.z * p.z).sqrt();
        assert!((r - label.radius).abs() < 0.006, "una letra a {r:.3} m del eje (radio {:.3})", label.radius);
    }
}
