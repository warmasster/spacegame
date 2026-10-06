//! Guard covers of the Alcotán, measured on their models: shut, a cover holds what it guards
//! inside it (no lever through its lid or walls); swinging open and open, it touches nothing else
//! on its panel (no other control, indicator or cover) and stays on its plate and out of the wall.
use glam::{DQuat, DVec3, Mat3, Vec3};
use lunar_controls::{Pose, mech::COVER_OPEN};
use lunar_core::{
    font::Font,
    props::Prop,
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, kind::PanelPlan, scene::control_props};
use std::path::Path;

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn ship() -> Ship {
    let defs = dir().join("defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let _ = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"))
}

fn font() -> Font {
    Font::parse(&std::fs::read_to_string(dir().join("fonts/serigrafia.json")).unwrap()).unwrap()
}

/// A prop as an oriented box: centre, axes, half extents (m).
#[derive(Clone, Copy)]
struct Obb {
    c: Vec3,
    a: Mat3,
    h: Vec3,
}

fn obb(p: &Prop) -> Obb {
    Obb { c: p.pos, a: Mat3::from_quat(p.rot), h: p.size * 0.5 }
}

impl Obb {
    fn corners(&self) -> impl Iterator<Item = Vec3> + '_ {
        (0..8).map(move |i| {
            let s = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
            self.c + self.a * (s * self.h)
        })
    }

    /// Overlap deeper than `slack` (m) along every separating axis (SAT, 15 axes).
    fn hits(&self, o: &Obb, slack: f32) -> bool {
        let axes_a = [self.a.x_axis, self.a.y_axis, self.a.z_axis];
        let axes_b = [o.a.x_axis, o.a.y_axis, o.a.z_axis];
        let mut axes = Vec::with_capacity(15);
        axes.extend(axes_a);
        axes.extend(axes_b);
        for x in axes_a {
            for y in axes_b {
                let c = x.cross(y);
                if c.length_squared() > 1e-8 {
                    axes.push(c.normalize());
                }
            }
        }
        let d = o.c - self.c;
        for n in axes {
            let ra: f32 = (0..3).map(|i| (axes_a[i].dot(n) * self.h[i]).abs()).sum();
            let rb: f32 = (0..3).map(|i| (axes_b[i].dot(n) * o.h[i]).abs()).sum();
            if d.dot(n).abs() > ra + rb - slack {
                return false;
            }
        }
        true
    }
}

/// Poses a control may stand in (levers both ways, covers shut to open).
fn poses(kind: &str) -> Vec<Pose> {
    let e = |a: f32, b: f32| Pose { e: [a, b, 0.0, 0.0], ..Default::default() };
    match kind {
        "tapa" => (0..=4).map(|k| e(COVER_OPEN * k as f32 / 4.0, 1.0)).collect(),
        "interruptor" | "selector" | "llave" | "palanca" | "rueda" => {
            vec![e(-0.6, 0.0), e(0.0, 0.0), e(0.6, 0.0)]
        }
        _ => vec![e(0.0, 0.0)],
    }
}

/// Every model of panel `plan` but `skip`, in all its poses; indicators as their box 9 mm deep.
fn obstacles(plan: &PanelPlan, skip: &[usize], font: &Font) -> Vec<(usize, Obb)> {
    let mut out = Vec::new();
    for (i, d) in plan.def.mandos.iter().enumerate() {
        if skip.contains(&i) {
            continue;
        }
        if lunar_controls::mech::KINDS.contains(&d.kind.as_str()) {
            for pose in poses(&d.kind) {
                out.extend(control_props(plan, i, &pose, font).iter().map(|p| (i, obb(p))));
            }
        } else {
            let r = plan.layout.controls[i];
            let depth = 0.009 * lunar_controls::layout::scale_of(d, plan.layout.scale);
            out.push((i, Obb { c: Vec3::new((r.x + r.w * 0.5) * 0.001, (r.y + r.h * 0.5) * 0.001, depth * 0.5), a: Mat3::IDENTITY, h: Vec3::new(r.w * 0.0005, r.h * 0.0005, depth * 0.5) }));
        }
    }
    out
}

#[test]
fn covers_hold_what_they_guard_and_swing_clear() {
    let ship = ship();
    let font = font();
    let mut bad = Vec::new();
    let mut covers = 0;
    for plan in &ship.kind.panels {
        let [w, h] = plan.layout.size.map(|x| x * 0.001);
        for (c, d) in plan.def.mandos.iter().enumerate().filter(|(_, d)| d.kind == "tapa") {
            covers += 1;
            let guarded: Vec<usize> = d.protege.iter().filter_map(|t| plan.def.mandos.iter().position(|m| &m.id == t)).collect();
            // shut: what it guards is under its lid and inside its walls
            let shut: Vec<Obb> = control_props(plan, c, &Pose { e: [0.0, 1.0, 0.0, 0.0], ..Default::default() }, &font).iter().map(obb).collect();
            let r = plan.layout.controls[c];
            let lid = lunar_controls::mech::GUARD_H * lunar_controls::layout::scale_of(d, plan.layout.scale) * 0.001;
            for &g in &guarded {
                for pose in poses(&plan.def.mandos[g].kind) {
                    for p in control_props(plan, g, &pose, &font) {
                        let o = obb(&p);
                        if shut.iter().any(|s| s.hits(&o, 2e-4)) {
                            bad.push(format!("{}: '{}' atraviesa su tapa cerrada", plan.id, plan.def.mandos[g].id));
                        }
                        for k in o.corners() {
                            let out_x = k.x < r.x * 0.001 - 1e-4 || k.x > (r.x + r.w) * 0.001 + 1e-4;
                            let out_y = k.y < r.y * 0.001 - 1e-4 || k.y > (r.y + r.h) * 0.001 + 1e-4;
                            if out_x || out_y || k.z > lid {
                                bad.push(format!("{}: '{}' asoma de su tapa cerrada ({:.1}, {:.1}, {:.1} mm)", plan.id, plan.def.mandos[g].id, k.x * 1e3, k.y * 1e3, k.z * 1e3));
                                break;
                            }
                        }
                    }
                }
            }
            // swinging open: clear of everything else, on its plate, out of the wall
            let mut skip = vec![c];
            skip.extend(&guarded);
            let others = obstacles(plan, &skip, &font);
            let mut guarded_props = Vec::new();
            for &g in &guarded {
                for pose in poses(&plan.def.mandos[g].kind) {
                    guarded_props.extend(control_props(plan, g, &pose, &font).iter().map(obb));
                }
            }
            let steps = 28;
            for k in 0..=steps {
                let a = COVER_OPEN * k as f32 / steps as f32;
                let props: Vec<Obb> = control_props(plan, c, &Pose { e: [a, 1.0, 0.0, 0.0], ..Default::default() }, &font).iter().map(obb).collect();
                for p in &props {
                    if let Some((i, _)) = others.iter().find(|(_, o)| p.hits(o, 3e-4)) {
                        bad.push(format!("{}: la tapa '{}' choca con '{}' a {:.0}°", plan.id, d.id, plan.def.mandos[*i].id, a.to_degrees()));
                        break;
                    }
                    if guarded_props.iter().any(|o| p.hits(o, 3e-4)) {
                        bad.push(format!("{}: la tapa '{}' choca con lo que guarda a {:.0}°", plan.id, d.id, a.to_degrees()));
                        break;
                    }
                    for q in p.corners() {
                        if q.z < -3e-4 || q.x < -2e-3 || q.x > w + 2e-3 || q.y < -2e-3 || q.y > h + 2e-3 {
                            bad.push(format!("{}: la tapa '{}' se sale de la placa a {:.0}° ({:.1}, {:.1}, {:.1} mm)", plan.id, d.id, a.to_degrees(), q.x * 1e3, q.y * 1e3, q.z * 1e3));
                            break;
                        }
                    }
                }
            }
            // open, it lies back: low over the panel, not standing out of it
            let open: Vec<Obb> = control_props(plan, c, &Pose { e: [COVER_OPEN, 1.0, 0.0, 0.0], ..Default::default() }, &font).iter().map(obb).collect();
            let top = open.iter().flat_map(|o| o.corners().collect::<Vec<_>>()).map(|q| q.z).fold(0.0f32, f32::max);
            let scale = lunar_controls::layout::scale_of(d, plan.layout.scale);
            eprintln!("{}: tapa '{}' abierta sobresale {:.0} mm (escala {:.1})", plan.id, d.id, top * 1e3, scale);
            if top > 0.047 * scale {
                bad.push(format!("{}: la tapa '{}' abierta sobresale {:.0} mm", plan.id, d.id, top * 1e3));
            }
        }
    }
    bad.dedup();
    assert!(covers > 0, "the Alcotán has guard covers");
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// What a hand aimed at the middle of `rect` (lifted `up` mm along the panel) finds: the id of the
/// control.
fn aimed(ship: &Ship, s: &Structure, panel: usize, rect: &lunar_controls::layout::Rect, up: f32) -> Option<String> {
    let kind = ship.kind.clone();
    let f = lunar_ship::panels::Panels::frame(&kind, s, &kind.panels[panel]);
    let at = f.transform_point3(Vec3::new((rect.x + rect.w * 0.5) / 1000.0, (rect.y + rect.h * 0.5 + up) / 1000.0, 0.0));
    let n = f.transform_vector3(Vec3::Z).normalize();
    ship.panels.pick(&kind, s, at + n * 0.4, -n, 1.0).map(|(k, _, _)| ship.panels.controls[k].id.clone())
}

#[test]
fn under_an_open_cover_the_hand_finds_what_it_guards() {
    use lunar_controls::Intent;
    let defs = dir().join("defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &lunar_ship::World::default(), 0.0);
    let covers: Vec<usize> = (0..ship.panels.controls.len()).filter(|&k| ship.panels.controls[k].mech.kind() == "tapa").collect();
    assert!(covers.len() >= 4, "{} tapas", covers.len());
    for cv in covers {
        let (panel, cover_id, cover_rect) = (ship.panels.controls[cv].panel, ship.panels.controls[cv].id.clone(), ship.panels.controls[cv].rect);
        let guarded: Vec<(String, lunar_controls::layout::Rect)> = ship.panels.controls.iter().filter(|c| c.cover == Some(cv)).map(|c| (c.id.clone(), c.rect)).collect();
        assert!(!guarded.is_empty(), "{cover_id} no protege nada");
        // shut: the cover is what the hand finds over each of them
        for (id, r) in &guarded {
            assert_eq!(aimed(&ship, &s, panel, r, 0.0).as_deref(), Some(cover_id.as_str()), "{id} bajo su tapa cerrada");
        }
        // open: each of them, and the lid lying back above them shuts it again
        let o = ship.panels.intent(cv, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
        assert!(o.changed, "{cover_id} no se abre");
        ship.panels.intent(cv, &Intent::Release, &s, &kind, &ship.store);
        for (id, r) in &guarded {
            assert_eq!(aimed(&ship, &s, panel, r, 0.0).as_deref(), Some(id.as_str()), "{id} con {cover_id} abierta");
        }
        assert_eq!(aimed(&ship, &s, panel, &cover_rect, cover_rect.h * 0.9).as_deref(), Some(cover_id.as_str()), "la tapa {cover_id} abierta no se encuentra para cerrarla");
    }
}
