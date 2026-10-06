//! The multi-function displays of the Alcotán: every page has something on it, every bezel button
//! brings up the page written by it (pressed as a hand would, aimed at by a ray), and the
//! instruments take their zone's colour (an empty hold red on the plan of the air).
use glam::{DQuat, DVec3, Vec3};
use lunar_controls::{IndState, Intent, indicator::IndKind, mfd::slot_pos};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World, panels::Panels};
use std::path::Path;

fn spawn() -> (Structure, Ship) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("alcotan").unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship)
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    let w = World { gravity: Vec3::new(0.0, -1.62, 0.0), ..World::default() };
    for _ in 0..(secs / 0.05) as usize {
        ship.update(s, &w, 0.05);
    }
}

fn mfd_state<'a>(ship: &'a Ship, id: &str) -> (&'a lunar_controls::mfd::Mfd, &'a IndState) {
    let (k, plan) = ship.kind.panels.iter().enumerate().find_map(|(k, p)| p.def.mandos.iter().position(|m| m.id == id).map(|i| (k, i))).map(|(k, i)| (k, (k, i))).unwrap();
    let ind = ship.panels.indicators.iter().find(|x| x.panel == plan.0 && x.index == plan.1).unwrap_or_else(|| panic!("no {id}"));
    let _ = k;
    let IndKind::Mfd(m) = &ind.ind.kind else { panic!("{id} is no mfd") };
    (m, &ind.st)
}

#[test]
fn every_bezel_button_brings_up_its_page() {
    let (mut s, mut ship) = spawn();
    // power up: batteries and buses (the panels need theirs)
    for c in ["techo/bat1", "techo/bat2", "techo/bus_a", "techo/bus_b", "techo/ess"] {
        let k = ship.panels.controls.iter().position(|x| x.id == c).unwrap();
        let kind = ship.kind.clone();
        ship.panels.intent(k, &Intent::Set { value: 1.0 }, &s, &kind, &ship.store);
    }
    run(&mut s, &mut ship, 2.0);
    for screen in ["mfd1", "mfd2"] {
        let bezel = format!("principal/{screen}_bisel");
        let k = ship.panels.controls.iter().position(|c| c.id == bezel).unwrap_or_else(|| panic!("no {bezel}"));
        let pages = mfd_state(&ship, screen).0.pages.len();
        for page in (0..pages).rev() {
            let kind = ship.kind.clone();
            let o = ship.panels.intent(k, &Intent::Press { elem: page as u8 }, &s, &kind, &ship.store);
            run(&mut s, &mut ship, 0.2);
            ship.panels.intent(k, &Intent::Release, &s, &kind, &ship.store);
            run(&mut s, &mut ship, 0.3);
            let (m, st) = mfd_state(&ship, screen);
            assert_eq!(st.mfd.page, page, "{screen}: button {page} → page {} ({o:?})", st.mfd.page);
            assert!(!m.pages[page].widgets.is_empty(), "{screen}: page '{}' is empty", m.pages[page].title);
            assert!(st.lit > 0.0, "{screen} powered");
            eprintln!("{screen} · {}: {} instrumentos", m.pages[page].title, m.pages[page].widgets.len());
        }
    }
}

#[test]
fn a_ray_at_a_bezel_button_presses_that_button() {
    let (s, ship) = spawn();
    let kind = ship.kind.clone();
    let k = ship.panels.controls.iter().position(|c| c.id == "principal/mfd2_bisel").unwrap();
    let c = &ship.panels.controls[k];
    let plan = &kind.panels[c.panel];
    let d = &plan.def.mandos[c.index];
    let scale = lunar_controls::layout::scale_of(d, plan.layout.scale);
    let strip = lunar_controls::mfd::STRIP * scale;
    let r = c.rect;
    let per = d.botones.unwrap_or(lunar_controls::mfd::PER_SIDE);
    let f = Panels::frame(&kind, &s, plan);
    for button in [0u32, 3, per, per + 2, 2 * per + 1, 3 * per + 4] {
        let (_, [bx, by]) = slot_pos(button, per, r.w - 2.0 * strip, r.h - 2.0 * strip, strip);
        let at = Vec3::new((r.x + strip + bx) / 1000.0, (r.y + strip + by) / 1000.0, 0.0);
        let from = f.transform_point3(at + Vec3::Z * 0.4);
        let dir = f.transform_vector3(-Vec3::Z).normalize();
        let (hit, elem, _) = ship.panels.pick(&kind, &s, from, dir, 1.0).expect("the bezel is hit");
        assert_eq!(hit, k);
        assert_eq!(u32::from(elem), button, "aimed at button {button}");
    }
}

#[test]
fn instruments_take_their_zone_colour() {
    let (mut s, mut ship) = spawn();
    for c in ["techo/bat1", "techo/bat2", "techo/bus_a", "techo/bus_b", "techo/ess"] {
        let k = ship.panels.controls.iter().position(|x| x.id == c).unwrap();
        let kind = ship.kind.clone();
        ship.panels.intent(k, &Intent::Set { value: 1.0 }, &s, &kind, &ship.store);
    }
    run(&mut s, &mut ship, 2.0);
    // the systems screen on its air page; the hold is open to vacuum (the ramp starts down)
    let k = ship.panels.controls.iter().position(|c| c.id == "principal/mfd2_bisel").unwrap();
    let air = mfd_state(&ship, "mfd2").0.pages.iter().position(|p| p.title == "AIRE").unwrap();
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Press { elem: air as u8 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 3.0);
    let (m, st) = mfd_state(&ship, "mfd2");
    let page = &m.pages[st.mfd.page];
    let (wi, w) = page.widgets.iter().enumerate().find(|(_, w)| matches!(w.kind, lunar_controls::mfd::WKind::Map(..))).expect("a plan of the air");
    let lunar_controls::mfd::WKind::Map(areas, _) = &w.kind else { unreachable!() };
    let ws = &st.mfd.widgets[wi];
    for ((name, _), (color, text)) in areas.iter().zip(&ws.areas) {
        eprintln!("{name}: {text} kPa {color:?}");
        let want = if name == "BODEGA" { [255, 40, 30] } else { [60, 255, 90] };
        assert_eq!(*color, want, "{name}");
    }
}
