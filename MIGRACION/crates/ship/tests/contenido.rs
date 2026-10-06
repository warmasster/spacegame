//! What cargo holds and what it weighs (`lunar_core::structure::contents`, `lunar_ship::contents`),
//! and the magnet that takes everything under its face (`lunar_ship::cargo`):
//! - a drum weighs what it weighs empty plus what it holds; half full, half of that; what is
//!   taken out makes it lighter, and the ship that carries it, and what the magnet that holds
//!   it reads; its centre of mass goes down as it empties; nothing is weighed again while
//!   nothing changes;
//! - what holds goes with what it is in: off the ship, onto a clamp, under a magnet;
//! - what is written on a container and what a scanner reads of it follow its data;
//! - every kind of cargo weighs what a real one does (the table of `docs/CARGA.md`);
//! - a magnet (the tug's, the crane's: one kind) takes the two drums under its face and lets
//!   both go, leaves the third beside them, leaves what would take it past its rated load and
//!   says so, and never takes what is still lashed.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{
        Library,
        breakup::{Event, Rules},
        contents::figure,
        set::Structures,
        state::Structure,
    },
};
use lunar_ship::{Ship, ShipKind, Sources, World, contents::ContentsDef, def::ShipDef, ship::TICK};
use lunar_signals::Q;
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

/// The structures' library and the ships, as the data has them once `tweak` has had its way
/// with the component kinds and the ships' definitions (a test's own data).
fn try_built(tweak: impl Fn(&mut Sources, &mut Vec<(String, ShipDef)>)) -> Result<(Library, Vec<Arc<ShipKind>>), String> {
    let mut lib = Library::load(&defs().join("structures")).map_err(|e| e.to_string())?;
    let mut src = Sources::load(&defs()).map_err(|e| e.to_string())?;
    let mut ships: Vec<(String, ShipDef)> = lunar_core::defs::load_dir(&defs().join("ships")).map_err(|e| e.to_string())?;
    tweak(&mut src, &mut ships);
    let mut kinds = Vec::new();
    for (id, d) in ships {
        let (k, bp) = src.build(&id, d, &mut lib.catalog)?;
        kinds.push(Arc::new(k));
        lib.blueprints.push((id, bp));
    }
    Ok((lib, kinds))
}

fn built(tweak: impl Fn(&mut Sources, &mut Vec<(String, ShipDef)>)) -> (Library, Vec<Arc<ShipKind>>) {
    try_built(tweak).unwrap_or_else(|e| panic!("{e}"))
}

fn only(ships: &mut Vec<(String, ShipDef)>, ids: &[&str]) {
    ships.retain(|s| ids.contains(&s.0.as_str()));
}

fn kind_of(kinds: &[Arc<ShipKind>], id: &str) -> Arc<ShipKind> {
    kinds.iter().find(|k| k.id == id).unwrap_or_else(|| panic!("no hay nave {id}")).clone()
}

fn moon() -> BodyRegistry {
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs().join("bodies/luna.jsonc")).unwrap()).unwrap();
    BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
}

/// A level place: of a few hundred spots, the one whose ground changes least over `reach` m
/// (and a little way to one side, where a second ship stands).
fn level(bodies: &BodyRegistry, reach: f64) -> DVec3 {
    let b = bodies.get(0);
    let mut best = (f64::MAX, DVec3::Y);
    let mut seed = 2718u64;
    for _ in 0..400 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = |k: u32| ((seed >> (11 + 17 * k)) & 0xffff) as f64 / 65535.0 * 2.0 - 1.0;
        let dir = DVec3::new(r(0), r(1), r(2)).normalize_or(DVec3::Y);
        let side = dir.any_orthonormal_vector();
        let fwd = dir.cross(side);
        let at = |x: f64, z: f64| b.height((dir * b.radius + side * x + fwd * z).normalize());
        let (h, d) = (reach, reach * 0.7);
        let hs = [at(0.0, 0.0), at(h, 0.0), at(-h, 0.0), at(0.0, h), at(0.0, -h), at(d, d), at(-d, -d), at(d, -d), at(-d, d), at(80.0, 0.0), at(90.0, 8.0), at(72.0, -8.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn ship_of(set: &mut Structures, id: u64) -> &mut Structure {
    set.list.iter_mut().find(|s| s.id == id).unwrap()
}

/// The ship and the world on for `secs`: its systems, what its clamps ask, what is in the way
/// of what moves, the bodies.
fn live(set: &mut Structures, bodies: &BodyRegistry, ship: &mut Ship, id: u64, secs: f64) {
    let per = (1.0 / 60.0 / TICK).round().max(1.0) as usize;
    for _ in 0..(secs * 60.0) as usize {
        for _ in 0..per {
            ship.update(ship_of(set, id), &w(), TICK);
        }
        set.separate();
        lunar_ship::cargo::serve(ship, set);
        ship.stop_at_obstacles(set, bodies, &[]);
        set.step(1.0 / 60.0, bodies);
    }
}

fn control(ship: &Ship, id: &str) -> usize {
    ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"))
}

fn press(set: &mut Structures, bodies: &BodyRegistry, ship: &mut Ship, id: u64, what: &str) {
    let (k, kind) = (control(ship, what), ship.kind.clone());
    ship.panels.intent(k, &Intent::Press { elem: 0 }, set.get(id).unwrap(), &kind, &ship.store);
    live(set, bodies, ship, id, 0.25);
    ship.panels.intent(k, &Intent::Release, set.get(id).unwrap(), &kind, &ship.store);
    live(set, bodies, ship, id, 0.1);
}

fn lever(set: &Structures, ship: &mut Ship, id: u64, what: &str, value: f64) {
    let (k, kind) = (control(ship, what), ship.kind.clone());
    ship.panels.intent(k, &Intent::Set { value }, set.get(id).unwrap(), &kind, &ship.store);
}

fn clamp(kind: &ShipKind, id: &str) -> usize {
    kind.clamps.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay anclaje {id}"))
}

fn part(kind: &ShipKind, id: &str) -> u32 {
    kind.parts.iter().position(|p| p == id).unwrap_or_else(|| panic!("no hay pieza {id}")) as u32
}

/// What the parts of the component placed as `id` weigh without what they hold (kg), and what
/// they hold (kg).
fn weighs(kind: &ShipKind, s: &Structure, id: &str) -> (f32, f32) {
    let mine = |n: &str| n == id || n.strip_prefix(id).is_some_and(|rest| rest.starts_with('.'));
    let parts: Vec<u32> = kind.parts.iter().enumerate().filter(|(_, n)| mine(n)).map(|(i, _)| i as u32).collect();
    assert!(!parts.is_empty(), "no hay componente {id}");
    (parts.iter().map(|&p| s.parts[p as usize].mass).sum(), parts.iter().filter_map(|&p| s.contents(p)).map(|c| c.mass).sum())
}

fn signal(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("no hay señal {name}"))
}

/// A ship of `kind` standing on level ground on the Moon, its systems started, down on its legs.
fn standing(lib: Library, kind: &Arc<ShipKind>) -> (Structures, BodyRegistry, Ship, u64) {
    let bodies = moon();
    let mut set = Structures::new(Arc::new(lib));
    let id = set.place(&kind.blueprint, &bodies, 0, level(&bodies, 24.0), 0.0, f64::from(kind.lift)).unwrap();
    let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut set.list[0], &w(), 0.0);
    set.rest_on_ground(id, &bodies);
    let up = bodies.get(0).up(set.list[0].pos);
    let sink = ship.rest_on_legs(&mut set.list[0], 1.62);
    set.list[0].pos -= up * f64::from(sink);
    (set, bodies, ship, id)
}

// ------------------------------------------------------------------ what is held weighs

#[test]
fn a_drum_weighs_what_it_holds_and_less_as_it_empties() {
    let (lib, kinds) = built(|_, ships| only(ships, &["alcotan"]));
    let kind = kind_of(&kinds, "alcotan");
    let (mut set, bodies, mut ship, id) = standing(lib, &kind);
    live(&mut set, &bodies, &mut ship, id, 4.0);
    let drum = part(&kind, "carga_agua_1");
    // full as built: what it weighs empty, and its capacity in water
    let s = set.get(id).unwrap();
    let st = *s.contents(drum).expect("el bidón de agua no lleva nada");
    let (shell, held) = weighs(&kind, s, "carga_agua_1");
    eprintln!("bidón de agua: {shell:.1} kg vacío, {held:.1} kg de agua ({:.0} L de cabida)", st.capacity);
    assert!((st.capacity - 170.0).abs() < 0.01 && st.mass == st.capacity && (held - 170.0).abs() < 0.01, "lleva {} de {} kg", st.mass, st.capacity);
    assert!((9.0..13.0).contains(&shell), "vacío pesa {shell:.1} kg");
    assert!(s.contents(0).is_none(), "una pieza cualquiera no lleva nada");
    // nothing changes: nothing is weighed again, however long its systems run
    let (m0, w0) = (s.mass, s.weighings);
    live(&mut set, &bodies, &mut ship, id, 3.0);
    assert_eq!(set.get(id).unwrap().weighings, w0, "la nave en reposo se vuelve a pesar sin que cambie nada");
    assert!(set.get(id).unwrap().resting, "la nave no reposa");
    // half its water taken out: the drum holds half, the ship weighs that much less; weighed once
    let s = ship_of(&mut set, id);
    assert!((s.take(drum, 85.0) - 85.0).abs() < 1e-3);
    assert!((s.contents(drum).unwrap().mass - 85.0).abs() < 1e-3 && (s.contents(drum).unwrap().share() - 0.5).abs() < 1e-4);
    assert!((s.mass - (m0 - 85.0)).abs() < 0.5, "la nave pesa {:.1} kg menos", m0 - s.mass);
    assert_eq!(s.weighings, w0 + 1);
    // a sip (under the quantum): counted, not weighed; asked for what it holds: nothing done
    assert!((s.take(drum, 0.5) - 0.5).abs() < 1e-3);
    let now = s.contents(drum).unwrap().mass;
    assert_eq!(s.fill(drum, now), now);
    assert!((s.contents(drum).unwrap().mass - 84.5).abs() < 1e-3 && s.weighings == w0 + 1 && (s.mass - (m0 - 85.0)).abs() < 0.5);
    // sip by sip it is weighed again each quantum (0.5 % of its capacity), not each sip
    for _ in 0..40 {
        s.take(drum, 0.1);
    }
    assert!((s.contents(drum).unwrap().mass - 80.5).abs() < 1e-2);
    assert!((3..=5).contains(&(s.weighings - w0 - 1)), "40 sorbos de 0,1 kg: pesada {} veces", s.weighings - w0 - 1);
    // more than there is: what there is; then nothing; and only what fits goes back in
    assert!((s.take(drum, 500.0) - 80.5).abs() < 1e-2);
    assert!(s.contents(drum).unwrap().mass == 0.0 && (s.mass - (m0 - 170.0)).abs() < 0.5);
    assert_eq!(s.take(drum, 10.0), 0.0);
    assert!((s.put(drum, 1000.0) - 170.0).abs() < 1e-3 && (s.mass - m0).abs() < 0.5);
    assert_eq!((s.take(0, 5.0), s.put(0, 5.0)), (0.0, 0.0), "lo que no es un recipiente ni da ni toma");
    assert!(s.inv_inertia.is_finite() && s.inertia.determinant() > 0.0);
}

#[test]
fn what_a_drum_holds_goes_with_it_and_its_centre_of_mass_goes_down_as_it_empties() {
    let (lib, kinds) = built(|_, ships| only(ships, &["alcotan"]));
    let kind = kind_of(&kinds, "alcotan");
    let (mut set, bodies, mut ship, id) = standing(lib, &kind);
    live(&mut set, &bodies, &mut ship, id, 2.0);
    let (drum, mass) = (part(&kind, "carga_agua_1"), set.get(id).unwrap().mass);
    // one of the two drums half empty, then both let go: each a body of its own with its water
    ship_of(&mut set, id).take(drum, 85.0);
    ship.set_signal("anclaje_agua.soltar", 1.0);
    let before = set.list.len();
    live(&mut set, &bodies, &mut ship, id, 2.0);
    let loose: Vec<u64> = set.list[before..].iter().map(|s| s.id).collect();
    assert_eq!(loose.len(), 2, "dos bidones sueltos");
    let held = |set: &Structures, d: u64| set.get(d).unwrap().stored.iter().map(|s| s.mass).sum::<f32>();
    let (half, full) = if held(&set, loose[0]) < 100.0 { (loose[0], loose[1]) } else { (loose[1], loose[0]) };
    assert!((held(&set, half) - 85.0).abs() < 1e-3 && (held(&set, full) - 170.0).abs() < 1e-3, "sueltos llevan {} y {} kg", held(&set, half), held(&set, full));
    let (mh, mf) = (set.get(half).unwrap().mass, set.get(full).unwrap().mass);
    eprintln!("sueltos: el lleno {mf:.1} kg, el medio {mh:.1} kg; la nave {:.0} kg menos", mass - set.get(id).unwrap().mass);
    assert!((mf - mh - 85.0).abs() < 0.1 && (175.0..190.0).contains(&mf), "lleno {mf:.1} kg, a medias {mh:.1} kg");
    assert!(set.get(id).unwrap().stored.iter().all(|s| s.part != drum), "la nave sigue contando el agua que se fue");
    assert!((mass - 85.0 - set.get(id).unwrap().mass - mf - mh).abs() < 1.0, "la masa de los bidones no sale de la nave");
    // its centre of mass (how high over its base) goes down as it empties, until so little is
    // left that the drum itself counts for more
    let k = set.index_of(full).unwrap();
    let d = &mut set.list[k];
    let (p, base) = (d.stored[0].part, d.parts.iter().flat_map(|p| p.shape.verts().map(|v| p.local.transform_point3(v).y)).fold(f32::MAX, f32::min));
    let mut heights = Vec::new();
    for kg in [170.0, 127.5, 85.0, 42.5, 0.0] {
        d.fill(p, kg);
        heights.push(d.com.y - base);
    }
    eprintln!("centro de masas sobre la base, de lleno a vacío: {heights:.3?} m");
    assert!((0.38..0.47).contains(&heights[0]), "lleno: {:.3} m", heights[0]);
    assert!(heights[0] > heights[1] + 0.05 && heights[1] > heights[2] + 0.05 && heights[2] > heights[3] + 0.03, "no baja al vaciarse: {heights:.3?}");
    assert!(heights[4] > heights[3] + 0.15, "vacío, es el del bidón: {heights:.3?}");
    d.fill(p, 170.0);
    // the clamp shut again takes both back as they are: the ship carries their water again
    let c = clamp(&kind, "anclaje_agua");
    live(&mut set, &bodies, &mut ship, id, 2.0);
    ship.work_clamp(c, false);
    live(&mut set, &bodies, &mut ship, id, 0.5);
    assert_eq!(ship.clamp_held[c].len(), 2, "el anclaje no vuelve a tomar los bidones");
    assert!((held(&set, half) - 85.0).abs() < 1e-3 && (held(&set, full) - 170.0).abs() < 1e-3, "anclados de nuevo llevan otra cosa");
    assert!((set.get(id).unwrap().mass - (mass - 85.0)).abs() < 1.0, "la nave no carga con lo que llevan");
    assert!((signal(&ship, "anclaje_agua.masa") - f64::from(mf + mh)).abs() < 1.0, "el anclaje dice que sujeta {:.1} kg", signal(&ship, "anclaje_agua.masa"));
    // water drunk from a drum the clamp holds: the clamp and the ship carry that much less
    let k = set.index_of(full).unwrap();
    let p = set.list[k].stored[0].part;
    set.list[k].take(p, 60.0);
    live(&mut set, &bodies, &mut ship, id, 0.5);
    assert!((set.get(id).unwrap().mass - (mass - 145.0)).abs() < 1.0, "la nave pesa lo mismo con 60 kg menos de agua");
    assert!((signal(&ship, "anclaje_agua.masa") - f64::from(mf + mh - 60.0)).abs() < 1.0);
}

#[test]
fn propellant_bursts_by_what_is_held_and_an_empty_drum_does_not() {
    // one Alcotán as its data has it, and one whose drum of propellant is placed half full
    let half = |_: &mut Sources, ships: &mut Vec<(String, ShipDef)>| {
        only(ships, &["alcotan"]);
        ships[0].1.componentes.iter_mut().find(|c| c.id == "carga_prop").unwrap().contenido = Some(ContentsDef { lleno: Some(0.5), ..ContentsDef::default() });
    };
    let mut bursts = Vec::new();
    for tweak in [&(|_: &mut Sources, ships: &mut Vec<(String, ShipDef)>| only(ships, &["alcotan"])) as &dyn Fn(&mut Sources, &mut Vec<(String, ShipDef)>), &half] {
        let (lib, kinds) = built(tweak);
        let kind = kind_of(&kinds, "alcotan");
        let b = lib.catalog.parts[usize::from(lib.blueprint("alcotan").unwrap().parts[part(&kind, "carga_prop") as usize].kind)].def.burst.clone().expect("el bidón de propelente no estalla");
        bursts.push(b);
    }
    eprintln!("estalla: lleno {:.0} kJ a {:.1} m; a medias {:.0} kJ a {:.1} m", bursts[0].energy / 1e3, bursts[0].radius, bursts[1].energy / 1e3, bursts[1].radius);
    assert!((bursts[0].energy - 8.0e5).abs() < 2.0e4 && (bursts[0].radius - 6.0).abs() < 0.1 && bursts[0].effect == "granada");
    assert!((bursts[1].energy - bursts[0].energy * 0.5).abs() < 1.0 && bursts[1].radius < bursts[0].radius * 0.82);
    // emptied, it is destroyed without a blast; the same drum with something left bursts
    let (lib, kinds) = built(|_, ships| only(ships, &["alcotan"]));
    let kind = kind_of(&kinds, "alcotan");
    let (mut set, _, _, id) = standing(lib, &kind);
    let rules = Rules::standard(&set.lib.catalog).unwrap();
    let drum = part(&kind, "carga_prop");
    let all = ship_of(&mut set, id).take(drum, 1.0e6);
    assert!((all - 148.75).abs() < 0.01, "llevaba {all} kg");
    let mut events = Vec::new();
    set.blow_out(id, drum, Vec3::Y, 1.0e3, &rules, &mut events, 1);
    assert!(!events.iter().any(|e| matches!(e, Event::Burst { .. })), "un bidón vacío estalla");
    assert!(set.get(id).unwrap().contents(drum).is_none(), "destruido, sigue llevando algo");
}

// ------------------------------------------------------------------ what is written and read

fn labels(lib: &Library, kind: &ShipKind, id: &str) -> Vec<String> {
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    lib.catalog.parts[usize::from(bp.parts[part(kind, id) as usize].kind)].def.labels.iter().map(|l| l.text.clone()).collect()
}

#[test]
fn what_is_written_on_a_container_and_what_a_scanner_reads_follow_its_data() {
    let (lib, kinds) = built(|_, ships| only(ships, &["alcotan"]));
    let kind = kind_of(&kinds, "alcotan");
    // as the data has it: nothing of this is typed in the drum
    let written = labels(&lib, &kind, "carga_agua_1");
    assert!(written.iter().any(|t| t == "AGUA") && written.iter().any(|t| t == "POTABLE · 170 L"), "{written:?}");
    let written = labels(&lib, &kind, "carga_prop");
    assert!(written.iter().any(|t| t == "PROPELENTE") && written.iter().any(|t| t == "INFLAMABLE · 170 L"), "{written:?}");
    assert!(labels(&lib, &kind, "carga_caja_1").iter().any(|t| t == "REPUESTOS"));
    let read = |k: &ShipKind, id: &str| k.describe(part(k, id) as usize);
    assert_eq!(read(&kind, "carga_agua_1"), ("Bidón de agua".to_string(), "170 L de agua".to_string()));
    assert_eq!(read(&kind, "carga_agua_1.tapon").1, "170 L de agua", "cualquier pieza del bidón lo dice");
    assert_eq!(read(&kind, "carga_prop").1, "170 L de propelente");
    assert_eq!(read(&kind, "carga_pale").1, "600 kg de regolito en sacos");
    assert_eq!(read(&kind, "carga_caja_1").1, "40 kg de repuestos");
    // a scanner reads what is left now and what it weighs with it; loose, the drum says the same
    let (mut set, bodies, mut ship, id) = standing(lib, &kind);
    let cat = set.lib.clone();
    let drum = part(&kind, "carga_agua_1");
    ship_of(&mut set, id).take(drum, 85.0);
    let scan = |set: &Structures, p: u32| lunar_ship::cargo::scan(&kind, set.get(id).unwrap(), &cat.catalog, p as usize);
    assert_eq!(scan(&set, drum), ("Bidón de agua".to_string(), "85 L de agua · 50 % · 97 kg".to_string()));
    assert_eq!(scan(&set, part(&kind, "carga_agua_1.aro")).1, "85 L de agua · 50 % · 97 kg");
    assert_eq!(scan(&set, part(&kind, "carga_agua_2")).1, "170 L de agua · 100 % · 182 kg");
    let (shell, held) = weighs(&kind, set.get(id).unwrap(), "carga_pale");
    assert_eq!(scan(&set, part(&kind, "carga_pale.sacos_2")).1, format!("600 kg de regolito en sacos · 100 % · {} kg", figure(shell + held)));
    assert_eq!(scan(&set, 0), kind.describe(0), "lo que no lleva nada se lee como siempre");
    ship.set_signal("anclaje_agua.soltar", 1.0);
    let before = set.list.len();
    live(&mut set, &bodies, &mut ship, id, 1.0);
    let cards: Vec<String> = set.list[before..].iter().map(|s| s.cargo_card(&cat.catalog).expect("un bidón suelto no dice qué lleva")).collect();
    assert!(cards.contains(&"85 L de agua · 50 % · 97 kg".to_string()) && cards.contains(&"170 L de agua · 100 % · 182 kg".to_string()), "{cards:?}");
    assert_eq!(set.list[before].label(&cat.catalog), "Bidón de agua");

    // the capacity changed in the data, and one drum placed with something else in it, half
    // full: what is written, what is read and what it weighs all change, with no other change
    let (lib, kinds) = built(|src, ships| {
        only(ships, &["alcotan"]);
        src.components.get_mut("bidon_agua").unwrap().contenido.as_mut().unwrap().capacidad = Some(Q::S("120 L".into()));
        ships[0].1.componentes.iter_mut().find(|c| c.id == "carga_agua_2").unwrap().contenido = Some(ContentsDef { sustancia: Some("propelente".into()), capacidad: Some(Q::S("140 kg".into())), lleno: Some(0.5), ..ContentsDef::default() });
    });
    let kind = kind_of(&kinds, "alcotan");
    let written = labels(&lib, &kind, "carga_agua_1");
    assert!(written.iter().any(|t| t == "POTABLE · 120 L") && !written.iter().any(|t| t.contains("170")), "{written:?}");
    assert_eq!(read(&kind, "carga_agua_1").1, "120 L de agua");
    let written = labels(&lib, &kind, "carga_agua_2");
    assert!(written.iter().any(|t| t == "PROPELENTE") && written.iter().any(|t| t == "POTABLE · 160 L"), "{written:?}");
    assert_eq!(read(&kind, "carga_agua_2").1, "80 L de propelente");
    let s = Structure::new(1, lib.blueprint("alcotan").unwrap(), &lib.catalog, DVec3::ZERO, glam::Quat::IDENTITY);
    let (one, two) = (weighs(&kind, &s, "carga_agua_1"), weighs(&kind, &s, "carga_agua_2"));
    assert!((one.1 - 120.0).abs() < 0.01 && (two.1 - 70.0).abs() < 0.01 && (one.0 - two.0).abs() < 1e-3, "llevan {} y {} kg", one.1, two.1);
    // (and what the substance does goes with it: the water drum with propellant in it bursts)
    assert!(lib.catalog.parts[usize::from(lib.blueprint("alcotan").unwrap().parts[part(&kind, "carga_agua_2") as usize].kind)].def.burst.is_some());

    // what does not fit, or is of nothing known, is refused when the ship is put together
    let e = try_built(|src, ships| {
        only(ships, &["alcotan"]);
        src.components.get_mut("bidon_agua").unwrap().contenido.as_mut().unwrap().capacidad = Some(Q::S("200 L".into()));
    })
    .err()
    .expect("200 L en un bidón de 175");
    assert!(e.contains("200 L de agua no caben") && e.contains("175 L"), "{e}");
    let e = try_built(|src, ships| {
        only(ships, &["alcotan"]);
        src.components.get_mut("bidon_agua").unwrap().contenido.as_mut().unwrap().sustancia = Some("vino".into());
    })
    .err()
    .expect("una sustancia que no está en el registro");
    assert!(e.contains("sustancia desconocida 'vino'"), "{e}");
}

/// The kinds of cargo: what each weighed when what it held weighed nothing (kg), and what a
/// real one weighs empty and loaded (kg; where from: `docs/CARGA.md`).
const CARGO: [(&str, f32, [f32; 2], [f32; 2]); 4] = [
    // a plastic drum of 170 L: 9 to 13 kg empty (a 208 L one, 9 to 12.7), its water on top
    ("bidon_agua", 41.9, [9.0, 13.0], [175.0, 190.0]),
    // a steel drum: 17 to 25 kg empty, 170 L of a fuel of 875 kg/m³
    ("bidon_combustible", 114.8, [17.0, 25.0], [160.0, 178.0]),
    // a transport case of 118 L: 8 to 16 kg empty, 40 kg of spares
    ("caja_repuestos", 68.1, [8.0, 16.0], [45.0, 60.0]),
    // a plastic pallet (15 to 25 kg), sacks and straps, and its 600 kg of regolith
    ("pale_regolito", 962.4, [28.0, 48.0], [625.0, 650.0]),
];

#[test]
fn every_kind_of_cargo_weighs_what_a_real_one_does() {
    let (lib, kinds) = built(|_, _| {});
    // every kind of component that holds something and is no machine is in the table
    let src = Sources::load(&defs()).unwrap();
    for (id, k) in &src.components {
        if k.contenido.is_some() && k.maquina.is_none() {
            assert!(CARGO.iter().any(|c| c.0 == id), "falta '{id}' en la tabla de masas");
        }
    }
    let mut seen: BTreeMap<&str, (f32, f32)> = BTreeMap::new();
    for kind in &kinds {
        let s = Structure::new(1, lib.blueprint(&kind.blueprint).unwrap(), &lib.catalog, DVec3::ZERO, glam::Quat::IDENTITY);
        let (mut empty, mut held) = (0.0, 0.0);
        for h in kind.holders.iter().filter(|h| h.level.is_none()) {
            let tipo = kind.def.componentes.iter().find(|d| d.id == h.id).and_then(|d| d.tipo.clone()).unwrap_or_default();
            let Some(row) = CARGO.iter().find(|c| c.0 == tipo) else { continue };
            let (shell, inside) = weighs(kind, &s, &h.id);
            assert!((row.2[0]..=row.2[1]).contains(&shell), "{} ({tipo}): vacío pesa {shell:.1} kg, uno de verdad de {} a {}", h.id, row.2[0], row.2[1]);
            assert!((row.3[0]..=row.3[1]).contains(&(shell + inside)), "{} ({tipo}): cargado pesa {:.1} kg, uno de verdad de {} a {}", h.id, shell + inside, row.3[0], row.3[1]);
            seen.insert(row.0, (shell, inside));
            empty += shell;
            held += inside;
        }
        eprintln!("{}: {:.0} kg; su carga {:.0} kg ({:.0} de envases y {:.0} de contenido)", kind.id, s.mass, empty + held, empty, held);
    }
    eprintln!("{:<20} {:>10} {:>10} {:>10} {:>10}", "tipo", "antes kg", "vacío kg", "lleva kg", "ahora kg");
    for row in &CARGO {
        let (shell, inside) = *seen.get(row.0).unwrap_or_else(|| panic!("ninguna nave lleva '{}'", row.0));
        eprintln!("{:<20} {:>10.1} {:>10.1} {:>10.1} {:>10.1}", row.0, row.1, shell, inside, shell + inside);
    }
}

// ------------------------------------------------------------------ the magnet

/// A tug standing on level ground and an Alcotán well away whose cargo was let go: its two
/// drums of water, its drum of propellant and its pallet, loose, to set under the tug.
struct Yard {
    set: Structures,
    bodies: BodyRegistry,
    ship: Ship,
    tug: u64,
    water: [u64; 2],
    fuel: u64,
    pallet: u64,
}

fn yard(lib: Library, kinds: &[Arc<ShipKind>]) -> Yard {
    let bodies = moon();
    let (tug, alc) = (kind_of(kinds, "abejorro"), kind_of(kinds, "alcotan"));
    let mut set = Structures::new(Arc::new(lib));
    let dir = level(&bodies, 6.0);
    let id = set.place(&tug.blueprint, &bodies, 0, dir, 0.0, 0.0).unwrap();
    let mut ship = Ship::new(tug, id, 7).unwrap();
    ship.update(ship_of(&mut set, id), &w(), 0.0);
    set.rest_on_ground(id, &bodies);
    let far = (dir * bodies.get(0).radius + dir.any_orthonormal_vector() * 80.0).normalize();
    let other = set.place(&alc.blueprint, &bodies, 0, far, 0.0, f64::from(alc.lift)).unwrap();
    let mut alcotan = Ship::new(alc, other, 3).unwrap();
    for c in ["anclaje_agua", "anclaje_prop", "anclaje_pale"] {
        alcotan.set_signal(&format!("{c}.soltar"), 1.0);
    }
    alcotan.update(ship_of(&mut set, other), &w(), TICK);
    assert_eq!(set.separate(), 4);
    let cat = set.lib.clone();
    let named = |what: &str| -> Vec<u64> { set.list.iter().filter(|s| s.id != id && s.id != other && s.label(&cat.catalog) == what).map(|s| s.id).collect() };
    let (water, fuel, pallet) = (named("Bidón de agua"), named("Bidón de propelente"), named("Palé de regolito"));
    assert_eq!((water.len(), fuel.len(), pallet.len()), (2, 1, 1));
    Yard { water: [water[0], water[1]], fuel: fuel[0], pallet: pallet[0], set, bodies, ship, tug: id }
}

impl Yard {
    /// Loose body `id` stood on the ground under the tug, its middle at (`x`, `z`) of the
    /// tug's frame (out of the way: far to one side).
    fn stand(&mut self, id: u64, x: f32, z: f32) {
        let t = self.set.get(self.tug).unwrap();
        // (the feet of the tug are on the ground)
        let ground = t.parts.iter().filter(|p| p.alive).flat_map(|p| p.shape.verts().map(|v| p.local.transform_point3(v).y)).fold(f32::MAX, f32::min);
        let rot = t.rot;
        let k = self.set.index_of(id).unwrap();
        let low = self.set.list[k].parts.iter().flat_map(|p| p.shape.verts().map(|v| p.local.transform_point3(v).y)).fold(f32::MAX, f32::min);
        let middle = self.set.list[k].center;
        let t = self.set.get(self.tug).unwrap();
        let pos = t.to_world(Vec3::new(x, ground + 0.03 + (middle.y - low), z));
        let c = &mut self.set.list[k];
        c.rot = rot;
        c.pos = pos - (rot * c.center).as_dvec3();
        (c.vel, c.spin, c.resting) = (DVec3::ZERO, Vec3::ZERO, false);
    }

    fn live(&mut self, secs: f64) {
        live(&mut self.set, &self.bodies, &mut self.ship, self.tug, secs);
    }

    fn press(&mut self, what: &str) {
        press(&mut self.set, &self.bodies, &mut self.ship, self.tug, what);
    }

    fn held(&self, id: u64) -> bool {
        self.set.get(id).unwrap().held.is_some_and(|h| h.by == self.tug)
    }

    fn mass(&self, id: u64) -> f32 {
        self.set.get(id).unwrap().mass
    }
}

#[test]
fn a_tugs_magnet_takes_the_two_drums_under_its_face_lets_both_go_and_leaves_the_third() {
    let (lib, kinds) = built(|_, ships| only(ships, &["abejorro", "alcotan"]));
    let mut y = yard(lib, &kinds);
    let c = clamp(&y.ship.kind, "iman");
    // two drums of water side by side under the pad; the drum of propellant in line with
    // them, out from under it; the pallet away
    let [a, b] = y.water;
    y.stand(a, -0.28, 0.05);
    y.stand(b, 0.28, 0.05);
    y.stand(y.fuel, 0.86, 0.05);
    y.live(3.0);
    let (tug, drums) = (y.mass(y.tug), y.mass(a) + y.mass(b));
    assert_eq!((signal(&y.ship, "iman.sujeta"), signal(&y.ship, "iman.masa")), (0.0, 0.0));
    let face = y.ship.clamp_zone(y.set.get(y.tug).unwrap(), c).map(|z| z.centre.y + z.half.y).unwrap();
    eprintln!("cara del imán a y = {face:.2}; los bidones llegan a {:.2} y {:.2}", y.set.top_in(a, y.tug).unwrap(), y.set.top_in(b, y.tug).unwrap());
    // AGARRA: both come up to the face; the third stays where it stands
    y.press("consola/tapa_iman");
    y.press("consola/agarre");
    y.live(1.0);
    let mut took = y.ship.clamp_held[c].clone();
    took.sort_unstable();
    assert_eq!(
        took,
        {
            let mut w = vec![a, b];
            w.sort_unstable();
            w
        },
        "el imán no toma los dos bidones que tiene debajo"
    );
    assert!(y.held(a) && y.held(b) && !y.held(y.fuel), "sujetos: {} {} {}", y.held(a), y.held(b), y.held(y.fuel));
    for d in [a, b] {
        let top = y.set.top_in(d, y.tug).unwrap();
        assert!((top - face).abs() < 0.02, "un bidón no queda contra la cara del imán: llega a y = {top:.2}");
    }
    assert_eq!(signal(&y.ship, "iman.sujeta"), 1.0);
    assert_eq!(signal(&y.ship, "iman.sobrecarga"), 0.0, "dos bidones no pasan de su carga nominal");
    eprintln!("imán: sujeta {:.1} kg (los dos bidones pesan {drums:.1})", signal(&y.ship, "iman.masa"));
    assert!((signal(&y.ship, "iman.masa") - f64::from(drums)).abs() < 1.0);
    assert!((y.mass(y.tug) - tug - drums).abs() < 1.0, "el remolcador no carga con los dos");
    // water out of one of them while it hangs: the drum, what the magnet reads and the tug, lighter
    let k = y.set.index_of(a).unwrap();
    let p = y.set.list[k].stored[0].part;
    assert!((y.set.list[k].take(p, 100.0) - 100.0).abs() < 1e-3);
    y.live(0.3);
    assert!((signal(&y.ship, "iman.masa") - f64::from(drums - 100.0)).abs() < 1.0, "el imán lee {:.1} kg con 100 kg menos de agua", signal(&y.ship, "iman.masa"));
    assert!((y.mass(y.tug) - tug - drums + 100.0).abs() < 1.0);
    // SUELTA: both go, and it holds nothing
    y.press("consola/agarre");
    y.live(1.0);
    assert!(y.ship.clamp_held[c].is_empty() && !y.held(a) && !y.held(b), "el imán no suelta los dos");
    assert_eq!((signal(&y.ship, "iman.sujeta"), signal(&y.ship, "iman.masa")), (0.0, 0.0));
    assert!((y.mass(y.tug) - tug).abs() < 1.0);
}

#[test]
fn a_magnet_leaves_what_would_take_it_past_its_rated_load_and_says_so() {
    // the same magnet rated for 300 kg (a test's own data): two drums of water are 365
    let (lib, kinds) = built(|src, ships| {
        only(ships, &["abejorro", "alcotan"]);
        src.components.get_mut("electroiman_carga").unwrap().anclaje.as_mut().unwrap().carga = Some(300.0);
    });
    let mut y = yard(lib, &kinds);
    let c = clamp(&y.ship.kind, "iman");
    let [a, b] = y.water;
    // one nearer the middle of the pad than the other
    y.stand(a, -0.2, 0.05);
    y.stand(b, 0.36, 0.05);
    y.live(3.0);
    y.press("consola/tapa_iman");
    y.press("consola/agarre");
    y.live(1.0);
    // the nearest one it takes; with the other it would hold more than it is rated for
    assert_eq!(y.ship.clamp_held[c], vec![a], "toma {:?}", y.ship.clamp_held[c]);
    assert!(y.held(a) && !y.held(b));
    assert_eq!(signal(&y.ship, "iman.sobrecarga"), 1.0, "no dice que deja algo");
    let said: Vec<String> = y.ship.said.iter().filter(|s| s.0 == "iman").map(|s| s.1.clone()).collect();
    eprintln!("dice: {said:?}");
    assert_eq!(said.len(), 1, "lo dice una vez: {said:?}");
    assert!(said[0].contains("deja Bidón de agua (182 kg)") && said[0].contains("carga nominal (300 kg)"), "{said:?}");
    assert!((signal(&y.ship, "iman.masa") - f64::from(y.mass(a))).abs() < 1.0);
    // SUELTA: it says so no more
    y.press("consola/agarre");
    y.live(1.0);
    assert_eq!((signal(&y.ship, "iman.sujeta"), signal(&y.ship, "iman.sobrecarga")), (0.0, 0.0));
    // over the pallet alone (640 kg): it leaves it where it is, says so once, and holds nothing
    y.stand(a, 2.3, 0.05);
    y.stand(b, -2.3, 0.05);
    y.stand(y.pallet, 0.0, 0.05);
    y.live(3.0);
    y.ship.said.clear();
    y.press("consola/agarre");
    y.live(3.0);
    assert!(y.ship.clamp_held[c].is_empty() && !y.held(y.pallet), "se lleva un palé de {:.0} kg con un imán de 300", y.mass(y.pallet));
    assert_eq!((signal(&y.ship, "iman.activo"), signal(&y.ship, "iman.sujeta"), signal(&y.ship, "iman.sobrecarga")), (1.0, 0.0, 1.0));
    let said: Vec<String> = y.ship.said.iter().filter(|s| s.0 == "iman").map(|s| s.1.clone()).collect();
    eprintln!("dice: {said:?}");
    assert!(said.len() == 1 && said[0].contains(&format!("deja Palé de regolito ({} kg)", figure(y.mass(y.pallet)))), "{said:?}");
    assert!(y.ship.blackbox.entries.iter().any(|e| e.text.contains("Palé de regolito")), "no queda en la caja negra");
    // the pallet lightened to what it can hold (sacks emptied): the magnet, still on, takes it
    let k = y.set.index_of(y.pallet).unwrap();
    for p in y.set.list[k].stored.iter().map(|s| s.part).collect::<Vec<_>>() {
        y.set.list[k].take(p, 130.0);
    }
    assert!(y.mass(y.pallet) < 300.0, "el palé pesa {:.0} kg", y.mass(y.pallet));
    y.live(1.0);
    assert_eq!(y.ship.clamp_held[c], vec![y.pallet], "aligerado, no lo toma");
    assert_eq!(signal(&y.ship, "iman.sobrecarga"), 0.0);
}

#[test]
fn a_cranes_magnet_leaves_drums_still_lashed_and_takes_both_once_they_are_let_go() {
    let (lib, kinds) = built(|_, ships| only(ships, &["cachalote"]));
    let kind = kind_of(&kinds, "cachalote");
    let (mut set, bodies, mut ship, id) = standing(lib, &kind);
    live(&mut set, &bodies, &mut ship, id, 2.0);
    let c = clamp(&kind, "grua_iman");
    let mass = set.get(id).unwrap().mass;
    // over bay C1 (two drums of water, lashed), all the way down, the magnet on
    let bay = kind.centers[part(&kind, "anclaje_c1") as usize];
    lever(&set, &mut ship, id, "grua/puente", f64::from((bay.z + 20.6) / 15.5));
    lever(&set, &mut ship, id, "grua/carro", f64::from(bay.x / 6.0 + 0.5));
    live(&mut set, &bodies, &mut ship, id, 10.0);
    lever(&set, &mut ship, id, "grua/gancho", 1.0);
    live(&mut set, &bodies, &mut ship, id, 12.0);
    press(&mut set, &bodies, &mut ship, id, "grua/tapa_iman");
    press(&mut set, &bodies, &mut ship, id, "grua/agarre");
    live(&mut set, &bodies, &mut ship, id, 1.5);
    assert_eq!(signal(&ship, "grua_iman.activo"), 1.0);
    assert!(ship.clamp_held[c].is_empty() && signal(&ship, "grua_iman.sujeta") == 0.0, "se lleva bidones que siguen anclados");
    assert_eq!(signal(&ship, "grua_iman.anclada"), 1.0);
    // their clamp let go: the magnet, still on, takes both
    let structures = set.list.len();
    ship.set_signal("anclaje_c1.soltar", 1.0);
    live(&mut set, &bodies, &mut ship, id, 2.0);
    assert_eq!(set.list.len(), structures + 2, "los dos bidones no se sueltan de su anclaje");
    let drums: Vec<u64> = set.list[structures..].iter().map(|s| s.id).collect();
    let mut took = ship.clamp_held[c].clone();
    took.sort_unstable();
    assert_eq!(took, drums, "el imán de la grúa no toma los dos bidones");
    let weight: f32 = drums.iter().map(|d| set.get(*d).unwrap().mass).sum();
    eprintln!("imán de la grúa: sujeta {:.1} kg (los dos bidones pesan {weight:.1})", signal(&ship, "grua_iman.masa"));
    assert!((signal(&ship, "grua_iman.masa") - f64::from(weight)).abs() < 1.0 && (360.0..370.0).contains(&weight));
    assert_eq!((signal(&ship, "grua_iman.anclada"), signal(&ship, "grua_iman.sobrecarga")), (0.0, 0.0));
    assert!((set.get(id).unwrap().mass - mass).abs() < 1.0, "la nave pesa otra cosa con sus bidones colgados del imán");
    // up with both
    let aboard = |set: &Structures, d: u64| set.get(id).unwrap().to_local(set.get(d).unwrap().to_world(set.get(d).unwrap().center));
    let before: Vec<Vec3> = drums.iter().map(|d| aboard(&set, *d)).collect();
    lever(&set, &mut ship, id, "grua/gancho", 0.2);
    live(&mut set, &bodies, &mut ship, id, 10.0);
    for (d, was) in drums.iter().zip(&before) {
        let now = aboard(&set, *d);
        assert!(now.y > was.y + 0.4 && (now.x - was.x).abs() < 0.05 && (now.z - was.z).abs() < 0.05, "un bidón no sube con el imán: de {was:.2?} a {now:.2?}");
    }
    // SUELTA: both fall to the deck
    press(&mut set, &bodies, &mut ship, id, "grua/agarre");
    live(&mut set, &bodies, &mut ship, id, 4.0);
    assert!(ship.clamp_held[c].is_empty() && drums.iter().all(|d| set.get(*d).unwrap().held.is_none()), "el imán no suelta los dos");
    assert_eq!(signal(&ship, "grua_iman.masa"), 0.0);
    for (d, was) in drums.iter().zip(&before) {
        let now = aboard(&set, *d);
        assert!((now.y - was.y).abs() < 0.3, "soltado, un bidón queda a {:.2} m de la cubierta", now.y - was.y);
    }
}

// ------------------------------------------------------------------ what a machine counts

#[test]
fn a_tank_weighs_what_its_machine_says_it_holds() {
    // (the tug's tanks as its data has them: containers whose level is their machine's. Every
    // ship's stores, the same way: tests/masa.rs)
    let (lib, kinds) = built(|_, ships| only(ships, &["abejorro"]));
    let kind = kind_of(&kinds, "abejorro");
    let mut s = Structure::new(1, lib.blueprint("abejorro").unwrap(), &lib.catalog, DVec3::ZERO, glam::Quat::IDENTITY);
    // (dry: its parts alone, what none of them holds)
    let dry: f32 = s.parts.iter().filter(|p| p.alive && !p.ghost).map(|p| p.mass).sum();
    assert!((2300.0..2500.0).contains(&dry) && (s.mass - dry - 440.0).abs() < 0.5, "se construye con {:.0} kg sobre {dry:.0} en seco", s.mass);
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
    let run = |ship: &mut Ship, s: &mut Structure, secs: f64| {
        for _ in 0..(secs / TICK).round() as usize {
            ship.update(s, &w(), TICK);
        }
    };
    let press = |ship: &mut Ship, s: &mut Structure, id: &str| {
        let (k, kind) = (control(ship, id), ship.kind.clone());
        ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
        run(ship, s, 0.25);
        ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
        run(ship, s, 0.1);
    };
    let fuel = |ship: &Ship| signal(ship, "deposito_izq.masa") + signal(ship, "deposito_der.masa");
    // its systems on: it weighs what it weighs dry and what its two tanks say they hold
    run(&mut ship, &mut s, 5.0);
    let full = fuel(&ship);
    eprintln!("abejorro: {dry:.0} kg en seco, {full:.0} kg de propelente, {:.0} kg en total", s.mass);
    assert!((full - 440.0).abs() < 1.0, "lleva {full:.1} kg");
    assert!((f64::from(s.mass - dry) - full).abs() < 0.5, "pesa {:.1} kg más que en seco con {full:.1} kg de propelente", s.mass - dry);
    // nothing burns: nothing is weighed
    let w0 = s.weighings;
    run(&mut ship, &mut s, 4.0);
    assert_eq!(s.weighings, w0, "se vuelve a pesar sin gastar nada");
    // its engines at full for a while: lighter by what they burnt, weighed a quantum at a time
    for id in ["consola/tapa_arm", "consola/arm", "consola/arr", "consola/arr"] {
        press(&mut ship, &mut s, id);
    }
    run(&mut ship, &mut s, 3.0);
    let (k, kd) = (control(&ship, "consola/acelerador"), ship.kind.clone());
    ship.panels.intent(k, &Intent::Set { value: 1.0 }, &s, &kd, &ship.store);
    let (before, w1) = (fuel(&ship), s.weighings);
    // (four minutes: its engines burn a fifth of a kilo a second between them)
    run(&mut ship, &mut s, 240.0);
    let burnt = before - fuel(&ship);
    let (ticks, weighed) = ((240.0 / TICK) as u64, s.weighings - w1);
    eprintln!("240 s a todo empuje: {burnt:.1} kg gastados, pesada {weighed} veces en {ticks} tics");
    assert!(burnt > 20.0, "los motores no gastan: {burnt:.1} kg");
    // (each tank is a quantum behind at most: 0.5 % of the 240 kg it takes)
    assert!((f64::from(s.mass - dry) - fuel(&ship)).abs() < 2.5, "pesa {:.1} kg más que en seco con {:.1} kg de propelente", s.mass - dry, fuel(&ship));
    assert!(weighed >= 4 && weighed <= (burnt / 1.2) as u64 + 4 && weighed < ticks / 5, "pesada {weighed} veces");
}
