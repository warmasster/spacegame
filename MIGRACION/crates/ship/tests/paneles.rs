//! The panels of the Alcotán as built: each fits the room it has at a good scale, its box is its
//! size, everything drawn on it stays on its plate and out of the wall, and the space in front of
//! it is clear for a hand (no part, no cable through it).
use glam::{DQuat, DVec3, Vec3};
use lunar_core::{
    font::Font,
    props::PropScene,
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, geom::Role, panels::Panels};
use std::path::Path;

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn spawn() -> (Structure, Ship, Library) {
    let defs = dir().join("defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    // posed as it starts (ramp down, gear down)
    ship.update(&mut s, &lunar_ship::World::default(), 0.0);
    (s, ship, lib)
}

/// Panels a hand works standing (big controls), and the scale they must reach at least.
const HAND: [(&str, f32); 6] = [("reactor", 2.0), ("rampa_bodega", 2.5), ("rampa_ext", 2.5), ("puerta_bodega_h", 2.5), ("soporte", 1.6), ("disyuntores", 1.6)];

#[test]
fn panels_fit_their_room_at_a_good_scale() {
    let (_, ship, _) = spawn();
    for p in &ship.kind.panels {
        let l = &p.layout;
        eprintln!("{:16} escala {:.1}, placa {:.0}×{:.0} mm", p.id, l.scale, l.size[0], l.size[1]);
        if let Some((_, min)) = HAND.iter().find(|(id, _)| *id == p.id) {
            assert!(l.scale >= *min - 1e-3, "{}: escala {:.1}, mínimo {min}", p.id, l.scale);
        }
        let plate = lunar_controls::layout::Rect { x: 0.0, y: 0.0, w: l.size[0], h: l.size[1] };
        for (i, r) in l.controls.iter().enumerate() {
            assert!(r.within(&plate), "{}: '{}' fuera de la placa", p.id, p.def.mandos[i].id);
        }
    }
    // the outside ramp panel has no floodlight switch: those are worked from inside
    assert!(!ship.panels.controls.iter().any(|c| c.id == "rampa_ext/focos"));
    assert!(ship.panels.controls.iter().any(|c| c.id == "rampa_bodega/focos"));
}

#[test]
fn what_a_panel_draws_stays_on_its_plate() {
    let (s, ship, lib) = spawn();
    let json = std::fs::read_to_string(dir().join("fonts/serigrafia.json")).unwrap();
    let font = Font::parse(&json).unwrap();
    for (pi, p) in ship.kind.panels.iter().enumerate() {
        let f = Panels::frame(&ship.kind, &s, p);
        let inv = f.inverse();
        // seen from 0.6 m in front: everything drawn, text too
        let eye = f.transform_point3(Vec3::new(p.layout.size[0] * 0.0005, p.layout.size[1] * 0.0005, 0.6));
        let mut scene = PropScene::default();
        let mut lamps = Vec::new();
        ship.scene(&s, &lib.catalog, &font, eye.as_dvec3(), &mut scene, &mut lamps);
        let (w, h) = (p.layout.size[0] / 1000.0, p.layout.size[1] / 1000.0);
        let others: Vec<(glam::Affine3A, f32, f32)> = ship.kind.panels.iter().enumerate().filter(|(k, _)| *k != pi).map(|(_, o)| (Panels::frame(&ship.kind, &s, o).inverse(), o.layout.size[0] / 1000.0, o.layout.size[1] / 1000.0)).collect();
        let mut bad = Vec::new();
        for pr in &scene.props {
            let l = inv.transform_point3(pr.pos);
            // only what is on this panel: just in front of its plate, near it
            if l.z < -0.02 || l.z > 0.25 || l.x < -0.3 || l.x > w + 0.3 || l.y < -0.3 || l.y > h + 0.3 {
                continue;
            }
            // (and not on a neighbour's plate: the pedestal's levers stand right under the main panel)
            if others.iter().any(|(inv, w, h)| {
                let q = inv.transform_point3(pr.pos);
                q.z > -0.01 && q.z < 0.25 && q.x > -0.03 && q.x < w + 0.03 && q.y > -0.03 && q.y < h + 0.03
            }) {
                continue;
            }
            let reach = pr.size.max_element() * 0.5;
            let off = (-l.x).max(l.x - w).max(-l.y).max(l.y - h);
            if off > reach.min(0.03) + 0.004 || l.z < -0.002 {
                bad.push(format!("{:?} a ({:.3}, {:.3}, {:.3}) tamaño {:.3}", pr.mesh, l.x, l.y, l.z, reach * 2.0));
            }
        }
        assert!(bad.is_empty(), "{}: {} cosas fuera de la placa: {:?}", p.id, bad.len(), &bad[..bad.len().min(6)]);
    }
}

#[test]
fn the_space_in_front_of_every_panel_is_clear() {
    let (s, ship, _) = spawn();
    let mut problems = Vec::new();
    for p in &ship.kind.panels {
        let f = Panels::frame(&ship.kind, &s, p);
        let (w, h) = (p.layout.size[0] / 1000.0, p.layout.size[1] / 1000.0);
        // a hand's room: the plate's face and 12 cm out of it
        // every 2 cm over the plate
        let (nx, ny) = ((w / 0.02).ceil() as usize, (h / 0.02).ceil() as usize);
        let pts: Vec<Vec3> = (0..=nx).flat_map(|i| (0..=ny).flat_map(move |j| [0.015f32, 0.06, 0.12].map(move |z| Vec3::new(w * i as f32 / nx as f32, h * j as f32 / ny as f32, z)))).map(|l| f.transform_point3(l)).collect();
        for (k, part) in s.parts.iter().enumerate() {
            if k == p.part as usize || !part.alive {
                continue;
            }
            let inv = part.local.inverse();
            let inside = pts.iter().filter(|q| part.shape.closest(inv.transform_point3(**q)).0 < -0.004).count();
            if inside > 0 {
                let role = ship.kind.roles[k];
                let ax = part.local.transform_vector3(Vec3::Y) * part.radius;
                problems.push(format!("{} ← {} ({role:?}, {inside} puntos) de {:.2?} a {:.2?}", p.id, ship.kind.parts[k], part.center - ax, part.center + ax));
                let _ = Role::Conduit;
            }
        }
    }
    for e in &problems {
        eprintln!("  {e}");
    }
    assert!(problems.is_empty(), "{} piezas delante de los paneles", problems.len());
}

#[test]
fn wires_come_out_of_the_top_of_a_panel_box() {
    let (s, ship, _) = spawn();
    let mut stubs = Vec::new();
    for (bi, id) in ship.kind.parts.iter().enumerate() {
        if !id.ends_with(".caja") {
            continue;
        }
        let bx = &s.parts[bi];
        let inv = bx.local.inverse();
        // the box's up: its panel's
        let Some(p) = ship.kind.panels.iter().find(|p| p.part as usize == bi) else {
            continue;
        };
        let up = p.frame.transform_vector3(Vec3::Y).normalize();
        let top = p.frame.transform_point3(Vec3::new(0.0, p.layout.size[1] / 1000.0, 0.0)).dot(up);
        for (k, part) in s.parts.iter().enumerate() {
            if ship.kind.roles[k] != Role::Conduit {
                continue;
            }
            let c = part.center;
            let near = bx.shape.closest(inv.transform_point3(c)).0 < 0.03;
            if near && c.dot(up) < top - 0.02 {
                stubs.push(format!("{id} ← {}", ship.kind.parts[k]));
            }
        }
    }
    for e in &stubs {
        eprintln!("  {e}");
    }
    assert!(stubs.is_empty(), "{} cables por los lados de una caja", stubs.len());
}

#[test]
fn no_panel_covers_a_decal() {
    // a poster, a sign, a stripe under a panel's box: one of them is in the wrong place
    let (s, ship, _) = spawn();
    let mut bad = Vec::new();
    for plan in &ship.kind.panels {
        let f = Panels::frame(&ship.kind, &s, plan);
        let inv = f.inverse();
        let (w, h) = (plan.layout.size[0] / 1000.0, plan.layout.size[1] / 1000.0);
        for d in &ship.kind.decals {
            let c = inv.transform_point3(d.center);
            if c.z.abs() > 0.12 {
                continue;
            }
            let (hu, hv) = (inv.transform_vector3(d.u).abs(), inv.transform_vector3(d.v).abs());
            let (hx, hy) = (hu.x + hv.x, hu.y + hv.y);
            let overlap_x = (c.x + hx).min(w) - (c.x - hx).max(0.0);
            let overlap_y = (c.y + hy).min(h) - (c.y - hy).max(0.0);
            if overlap_x > 0.01 && overlap_y > 0.01 {
                bad.push(format!("{} tapa una calca en ({:.2}, {:.2}, {:.2})", plan.id, d.center.x, d.center.y, d.center.z));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn no_panel_is_cut_by_its_wall_nor_crossed_by_a_cable() {
    // a hull wall leans and turns: a panel's box is seated on it (its face out as far as the wall
    // needs, its box back to the wall), so nothing comes through its plate; and no drop or pipe,
    // which are only seen, runs across its face
    let (s, ship, _) = spawn();
    let cut = lunar_ship::diag::panels_cut(&ship, &s);
    let crossed = lunar_ship::diag::panel_faces(&ship.kind);
    for e in cut.iter().chain(&crossed) {
        eprintln!("  {e}");
    }
    assert!(cut.is_empty(), "{} paneles cortados o en el aire", cut.len());
    assert!(crossed.is_empty(), "{} paneles con cables por delante", crossed.len());
    // (and there are drops and pipes to speak of)
    assert!(ship.kind.seen.len() > 20, "{} bajantes y tubos", ship.kind.seen.len());
}
