//! A ship weighs what it carries (`docs/CARGA.md` §6), every ship of the library the same way:
//! - full, it weighs what it weighs dry and what its stores hold; each store holds what its
//!   machine says, takes what fits in its volume at its substance's density and no more, and
//!   one told to take more is refused when the ship is put together;
//! - what its engines burn makes it lighter by that much, and moves its centre of mass where
//!   a sum by hand says; a holed bottle lightens it by its gas;
//! - its tanks drained unevenly, the flight computer trims its engines through where the
//!   centre of mass went: it hovers without turning;
//! - its legs carry it full and empty within their travel, and its engines lift it full and
//!   set it down empty under the gravity of every body there is;
//! - what it weighs is told by signals every ship has (`nave.masa` and the rest), shown by the
//!   "masa" section of its panels;
//! - and all of it costs a ship with nothing flowing nothing: no weighing, a comparison a store.
//!
//! Run with `--nocapture` for the tables (masses, thrust to weight, centre of mass, costs).
use glam::{DVec3, Quat, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, contents::QUANTUM, set::Structures, state::Structure},
};
use lunar_ship::{Ship, ShipKind, Sources, World, contents::ContentsDef, def::ShipDef, diag, kind::Holder, ship::TICK};
use lunar_signals::Q;
use std::{path::Path, sync::Arc, time::Instant};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

/// The structures' library and the ships, as the data has them once `tweak` has had its way
/// with the component kinds and the ships' definitions (a test's own data).
fn try_built(tweak: impl Fn(&mut Sources, &mut Vec<(String, ShipDef)>)) -> Result<(Library, Vec<Arc<ShipKind>>, Sources), String> {
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
    Ok((lib, kinds, src))
}

fn built() -> (Library, Vec<Arc<ShipKind>>, Sources) {
    try_built(|_, _| {}).unwrap_or_else(|e| panic!("{e}"))
}

/// Every body of the registry (`assets/defs/bodies`): nothing here is the Moon's alone.
fn bodies() -> BodyRegistry {
    let all: Vec<(String, BodyDef)> = lunar_core::defs::load_dir(&defs().join("bodies")).unwrap_or_else(|e| panic!("{e}"));
    BodyRegistry::new(all.iter().map(|(id, d)| Body::from_def(id, d).unwrap()).collect())
}

fn w(g: f32) -> World {
    World { gravity: Vec3::new(0.0, -g, 0.0), altitude: 0.0, ..World::default() }
}

/// What the world is to a ship where it is: gravity as its own frame has it, how high it is.
fn world_at(s: &Structure, bodies: &BodyRegistry) -> World {
    World::at(s, bodies, s.to_world(s.com))
}

/// A ship of `kind` on a structure of its own, nothing under it, its systems on a tick.
fn bare(lib: &Library, kind: &Arc<ShipKind>) -> (Structure, Ship) {
    let mut s = Structure::new(1, lib.blueprint(&kind.blueprint).unwrap(), &lib.catalog, DVec3::ZERO, Quat::IDENTITY);
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{}: {e}", kind.id));
    ship.update(&mut s, &w(1.62), TICK);
    (s, ship)
}

fn run(ship: &mut Ship, s: &mut Structure, g: f32, secs: f64) {
    for _ in 0..(secs / TICK).round() as usize {
        ship.update(s, &w(g), TICK);
    }
}

fn signal(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id))
}

/// The control of `ship` that writes signal `name`, set to `value` (whatever cover is over it).
fn set(ship: &mut Ship, s: &Structure, name: &str, value: f64) {
    let sig = ship.store.find(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id));
    let k = ship.panels.controls.iter().position(|c| c.sig == sig).unwrap_or_else(|| panic!("{}: ningún mando escribe {name}", ship.kind.id));
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Set { value }, s, &kind, &ship.store);
}

/// The sprung lever of `ship` that writes signal `name` held at `value`, as a key holds it (at
/// its centre: let go).
fn hold(ship: &mut Ship, s: &Structure, name: &str, value: f64) {
    let sig = ship.store.find(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id));
    let k = ship.panels.controls.iter().position(|c| c.sig == sig).unwrap_or_else(|| panic!("{}: ningún mando escribe {name}", ship.kind.id));
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Axis { axis: 0, value }, s, &kind, &ship.store);
}

/// The machines that are `ship`'s engines (`vuelo.motores`).
fn engines(kind: &ShipKind) -> Vec<usize> {
    let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
    kind.def.vuelo.iter().flat_map(|f| &f.motores).flat_map(|p| lunar_ship::kind::resolve(&ids, p)).map(|m| m as usize).collect()
}

/// What an engine's order `role` is called (its data's name for it, else `<id>.<role>`).
fn order(kind: &ShipKind, m: usize, role: &str) -> String {
    kind.machines[m].def.ordenes.get(role).and_then(|v| v.as_str()).map_or_else(|| format!("{}.{role}", kind.machines[m].id), str::to_string)
}

/// Its engines armed and started by the switches that do it, whatever they are called: every
/// signal its engines take as `armado` set, every `arranque` taken to its start position and
/// let back to run. `tick`: the ship run that long (s).
fn start_engines(ship: &mut Ship, s: &mut Structure, tick: &mut dyn FnMut(&mut Ship, &mut Structure, f64)) {
    let kind = ship.kind.clone();
    let mut names: Vec<(String, String)> = engines(&kind).iter().map(|&m| (order(&kind, m, "armado"), order(&kind, m, "arranque"))).collect();
    names.sort();
    names.dedup();
    tick(ship, s, 3.0);
    for (arm, _) in &names {
        set(ship, s, arm, 1.0);
    }
    tick(ship, s, 0.5);
    for (_, start) in &names {
        set(ship, s, start, 2.0);
    }
    tick(ship, s, 0.4);
    for (_, start) in &names {
        set(ship, s, start, 1.0);
    }
    tick(ship, s, 5.0);
    for &m in &engines(&kind) {
        let state = signal(ship, &format!("{}.estado", kind.machines[m].id));
        assert_eq!(state, 4.0, "{}: el motor {} no arranca (estado {state})", kind.id, kind.machines[m].id);
    }
}

/// The parts of `kind` that hold what holder `h` holds, with what each holds now (kg).
fn held(s: &Structure, h: &Holder) -> f32 {
    h.holds.iter().filter_map(|&p| s.contents(p)).map(|c| c.mass).sum()
}

/// The holders of `kind` whose machine feeds a propellant network: its tanks.
fn tanks(kind: &ShipKind) -> Vec<&Holder> {
    kind.holders.iter().filter(|h| h.level.is_some() && h.substance == "propelente").collect()
}

/// The machine that counts what holder `h` holds.
fn machine_of(kind: &ShipKind, h: &Holder) -> usize {
    kind.machines.iter().position(|m| m.id == h.id).unwrap_or_else(|| panic!("{}: {} no tiene máquina", kind.id, h.id))
}

// ------------------------------------------------------------------ full = dry + what it holds

#[test]
fn every_ship_weighs_what_it_weighs_dry_and_what_its_stores_hold() {
    let (lib, kinds, _) = built();
    eprintln!("{:<10} {:>10} {:>12} {:>10} {:>10} {:>10} {:>10}", "nave", "piezas kg", "depósitos kg", "así kg", "seca kg", "llena kg", "propel. kg");
    for kind in &kinds {
        let (mut s, mut ship) = bare(&lib, kind);
        run(&mut ship, &mut s, 1.62, 3.0);
        // its parts alone, and what its parts hold: nothing else weighs
        let shells: f32 = s.parts.iter().filter(|p| p.alive && !p.ghost).map(|p| p.mass).sum();
        let stored: f32 = s.stored.iter().map(|st| st.mass).sum();
        assert!((s.mass - shells - stored).abs() < 1.0, "{}: pesa {:.1} kg con {shells:.1} kg de piezas y {stored:.1} kg dentro", kind.id, s.mass);
        // every store that a machine counts holds what the machine says, in the parts it lies in
        let mut counted = 0.0;
        for h in kind.holders.iter().filter(|h| h.level.is_some()) {
            let says = signal(&ship, h.level.as_deref().unwrap()) as f32;
            assert!((held(&s, h) - says).abs() < 1e-2, "{}: {} lleva {:.2} kg y su máquina dice {says:.2}", kind.id, h.id, held(&s, h));
            assert!((says - h.built).abs() < 0.5, "{}: {} se construye con {:.1} kg y su máquina empieza con {says:.1}", kind.id, h.id, h.built);
            counted += says;
        }
        // nothing a machine counts weighs nothing, and none is free to say more than fits
        let found = diag::stores(&ship);
        assert!(found.is_empty(), "{}: {found:?}", kind.id);
        // what the ship says of itself: the same figures
        let f = ship.mass.figures();
        let propellant: f32 = tanks(kind).iter().map(|h| held(&s, h)).sum();
        assert!(!tanks(kind).is_empty() && propellant > 100.0, "{}: sin depósitos de propelente que pesen", kind.id);
        assert!((signal(&ship, "nave.masa") - f64::from(s.mass)).abs() < 0.5 && (f.mass - f64::from(s.mass)).abs() < 0.5, "{}: dice {:.1} kg y pesa {:.1}", kind.id, signal(&ship, "nave.masa"), s.mass);
        assert!((signal(&ship, "nave.propelente") - f64::from(propellant)).abs() < 0.01 && (signal(&ship, "nave.masa_seca") - f64::from(s.mass - propellant)).abs() < 0.5);
        let capacity: f32 = tanks(kind).iter().map(|h| h.capacity).sum();
        assert!((signal(&ship, "nave.propelente_nivel") - f64::from(propellant / capacity)).abs() < 1e-3);
        // full: every tank at what it takes; dry: none
        let l = diag::lift(&ship, &s);
        assert!((l.full - (f64::from(s.mass - propellant + capacity))).abs() < 1.0 && (l.dry - f64::from(s.mass - propellant)).abs() < 1.0);
        eprintln!("{:<10} {:>10.0} {:>12.0} {:>10.0} {:>10.0} {:>10.0} {:>10.0}", kind.id, shells, counted, s.mass, l.dry, l.full, propellant);
    }
}

/// The room of the pieces of component `h` that hold (m³), from the shapes its data gives them
/// less their walls: worked out here by hand from the data, not asked of the ship.
fn room_by_hand(kind: &ShipKind, src: &Sources, h: &Holder) -> (f32, f32) {
    use lunar_ship::components::mirror_name;
    let placed = kind.def.componentes.iter().find(|c| c.id == h.id || mirror_name(&c.id) == h.id || h.id.strip_prefix(c.id.as_str()).is_some_and(|r| r.starts_with('#'))).unwrap_or_else(|| panic!("{}: no hay componente {}", kind.id, h.id));
    let ck = placed.tipo.as_ref().and_then(|t| src.components.get(t)).unwrap_or_else(|| panic!("{}: {} sin tipo", kind.id, h.id));
    let def = ck.contenido.as_ref().unwrap();
    let names: Vec<String> = def.piezas.clone().unwrap_or_else(|| vec![ck.piezas[0].id.clone()]);
    let room: f32 = names
        .iter()
        .map(|n| {
            let p = ck.piezas.iter().find(|p| &p.id == n).unwrap();
            let (volume, area) = p.forma.measure();
            volume - area * p.hueco.unwrap()
        })
        .sum();
    let substance = placed.contenido.as_ref().and_then(|c| c.sustancia.clone()).or(def.sustancia.clone()).unwrap();
    assert_eq!(substance, h.substance, "{}: {}", kind.id, h.id);
    (room, f32::from(u8::from(def.lleno == Some(0.0))))
}

#[test]
fn every_tank_takes_what_fits_in_its_volume_at_the_density_of_what_it_holds() {
    let (lib, kinds, src) = built();
    eprintln!("{:<10} {:<24} {:<12} {:>9} {:>9} {:>9} {:>9}", "nave", "depósito", "sustancia", "dentro L", "caben kg", "lleva kg", "trae kg");
    for kind in &kinds {
        let (s, ship) = bare(&lib, kind);
        for h in kind.holders.iter().filter(|h| h.level.is_some()) {
            let (room, _) = room_by_hand(kind, &src, h);
            let density = lib.catalog.substances.iter().find(|(id, _)| *id == h.substance).unwrap().1.densidad;
            eprintln!("{:<10} {:<24} {:<12} {:>9.1} {:>9.1} {:>9.1} {:>9.1}", kind.id, h.id, h.substance, room * 1000.0, room * density, h.capacity, h.built);
            // what fits is its volume by the density; it takes that or less, and is built with no more
            assert!((h.brim - room * density).abs() < h.brim * 2e-3, "{}: {}: caben {:.1} kg y {:.1} L a {density} kg/m³ son {:.1}", kind.id, h.id, h.brim, room * 1000.0, room * density);
            assert!(h.capacity <= h.brim * 1.001 && h.built <= h.capacity * 1.001 && h.tied, "{}: {}: lleva hasta {:.1} kg, caben {:.1}", kind.id, h.id, h.capacity, h.brim);
            // its machine was told the same: what it says it takes at most is what the tank takes
            // (a tank and a bottle say it in kg, `capacidad`; a vessel of gas by its volume)
            let params = &kind.machines[machine_of(kind, h)].def.params;
            let said = params.get("capacidad").and_then(|v| v.as_f64()).map(|kg| kg as f32).or(params.get("volumen").and_then(|v| v.as_f64()).map(|m3| m3 as f32 * density));
            let said = said.unwrap_or_else(|| panic!("{}: la máquina de {} no dice cuánto le cabe", kind.id, h.id));
            assert!((said - h.capacity).abs() < h.capacity * 1e-3, "{}: {}: su máquina dice {said:.1} kg y el depósito {:.1}", kind.id, h.id, h.capacity);
            // and the structure holds it so: each part its share
            let cap: f32 = h.holds.iter().filter_map(|&p| s.contents(p)).map(|c| c.capacity).sum();
            assert!((cap - h.capacity).abs() < h.capacity * 1e-3);
        }
        let _ = ship;
    }
}

#[test]
fn a_machine_told_to_hold_more_than_fits_in_its_tank_is_refused_and_one_that_is_not_tied_is_found() {
    // the Cachalote as it was: a wing tank told to hold nine tonnes
    let e = try_built(|_, ships| {
        let tank = ships.iter_mut().find(|s| s.0 == "cachalote").unwrap().1.componentes.iter_mut().find(|c| c.id == "deposito_izq").unwrap();
        tank.maquina.as_mut().unwrap().params.insert("capacidad".into(), serde_json::json!("9000 kg"));
    })
    .err()
    .expect("9 t en un depósito de 1,6 m³");
    eprintln!("{e}");
    assert!(e.contains("cachalote") && e.contains("deposito_izq") && e.contains("de propelente no caben") && e.contains("dentro hay sitio para 16") && e.contains("lo dice su máquina, 'capacidad'"), "{e}");
    // a bottle told to hold sixty kilos of nitrogen
    let e = try_built(|_, ships| {
        let b = ships.iter_mut().find(|s| s.0 == "alcotan").unwrap().1.componentes.iter_mut().find(|c| c.id == "botella_n2").unwrap();
        b.maquina.as_mut().unwrap().params.insert("capacidad".into(), serde_json::json!("60 kg"));
    })
    .err()
    .expect("60 kg de nitrógeno en una botella de 48 L");
    assert!(e.contains("60 kg de nitrógeno no caben") && e.contains("dentro hay sitio para 10"), "{e}");
    // started with more than it takes; a vessel whose machine says it is bigger inside than it is
    let e = try_built(|src, _| {
        src.components.get_mut("deposito_tug").unwrap().maquina.as_mut().unwrap().params.insert("masa_inicial".into(), serde_json::json!("300 kg"));
    })
    .err()
    .expect("300 kg en un depósito de 240");
    assert!(e.contains("empieza con") && e.contains("no lleva más de"), "{e}");
    let e = try_built(|src, _| {
        src.components.get_mut("deposito_aire").unwrap().maquina.as_mut().unwrap().params.insert("volumen".into(), serde_json::json!("0.42 m3"));
    })
    .err()
    .expect("0,42 m³ en un depósito de 0,38");
    assert!(e.contains("420 L dentro") && e.contains("hay sitio para 383 L"), "{e}");
    // what its placing says goes over what its kind's machine says; the machine is given it
    let (lib, kinds, _) = try_built(|_, ships| {
        ships.retain(|s| s.0 == "abejorro");
        ships[0].1.componentes.iter_mut().find(|c| c.id == "deposito_izq").unwrap().contenido = Some(ContentsDef { capacidad: Some(Q::S("300 kg".into())), lleno: Some(0.5), ..ContentsDef::default() });
    })
    .unwrap();
    let (s, ship) = bare(&lib, &kinds[0]);
    assert!((signal(&ship, "deposito_izq.masa") - 150.0).abs() < 0.1 && (signal(&ship, "deposito_der.masa") - 150.0).abs() < 0.1 && (signal(&ship, "deposito_izq.nivel") - 0.5).abs() < 1e-3, "{} kg", signal(&ship, "deposito_izq.masa"));
    assert!((f64::from(held(&s, &kinds[0].holders[0])) - 150.0).abs() < 0.1);
    // a tank whose data does not tie it to its machine, and one that holds nothing: found
    let (lib, kinds, _) = try_built(|src, ships| {
        ships.retain(|s| s.0 == "abejorro");
        let c = src.components.get_mut("deposito_tug").unwrap().contenido.as_mut().unwrap();
        (c.cabida, c.inicial) = (None, None);
    })
    .unwrap();
    let found = diag::stores(&bare(&lib, &kinds[0]).1);
    assert!(found.len() == 2 && found[0].contains("deposito_izq") && found[0].contains("falta 'cabida'"), "{found:?}");
    let (lib, kinds, _) = try_built(|src, ships| {
        ships.retain(|s| s.0 == "abejorro");
        src.components.get_mut("deposito_tug").unwrap().contenido = None;
    })
    .unwrap();
    let found = diag::stores(&bare(&lib, &kinds[0]).1);
    assert!(found.len() == 2 && found[1].contains("deposito_der.masa") && found[1].contains("no pesa"), "{found:?}");
}

// ------------------------------------------------------------------ lighter as it burns

#[test]
fn what_its_engines_burn_makes_it_lighter_by_that_much_and_moves_its_centre_of_mass() {
    let (lib, kinds, _) = built();
    for kind in &kinds {
        let (mut s, mut ship) = bare(&lib, kind);
        start_engines(&mut ship, &mut s, &mut |ship, s, secs| run(ship, s, 1.62, secs));
        let throttle = kind.def.vuelo.as_ref().unwrap().acelerador.clone().unwrap();
        // before: what each tank holds, where the ship's centre of mass is
        let fuel = |ship: &Ship| -> Vec<f64> { tanks(kind).iter().map(|h| signal(ship, h.level.as_deref().unwrap())).collect() };
        let (m0, c0, before, w0) = (f64::from(s.mass), s.com.as_dvec3(), fuel(&ship), s.weighings);
        let lay0: Vec<(f32, Vec3)> = tanks(kind).iter().flat_map(|h| h.holds.iter()).map(|&p| s.contents(p).map(|st| (st.weighed, s.parts[p as usize].local.transform_point3(st.at))).unwrap()).collect();
        set(&mut ship, &s, &throttle, 1.0);
        // (minutes of it: an hour's worth of hanging still is in its tanks)
        let secs = 400.0;
        run(&mut ship, &mut s, 1.62, secs);
        let after = fuel(&ship);
        let burnt: f64 = before.iter().zip(&after).map(|(a, b)| a - b).sum();
        // (each part that holds is a quantum behind what its machine says, at most)
        let lag: f32 = tanks(kind).iter().map(|h| h.capacity * QUANTUM).sum();
        let lighter = m0 - f64::from(s.mass);
        eprintln!("{}: {secs} s a todo empuje: {burnt:.1} kg gastados, {lighter:.1} kg más ligera (hasta {lag:.1} kg de retraso), pesada {} veces en {} tics", kind.id, s.weighings - w0, (secs / TICK) as u64);
        assert!(burnt > 20.0, "{}: sus motores no gastan ({burnt:.2} kg)", kind.id);
        assert!((lighter - burnt).abs() <= f64::from(lag) + 0.05, "{}: gasta {burnt:.2} kg y pesa {lighter:.2} kg menos", kind.id);
        assert!((signal(&ship, "nave.masa") - (m0 - burnt)).abs() < f64::from(lag) + 0.5 && (signal(&ship, "nave.propelente") - after.iter().sum::<f64>()).abs() < 1e-6);
        // the centre of mass, by hand: the same ship with what left each part taken from where
        // it lay and what is left put where it lies now (first moments about the origin)
        let lay1: Vec<(f32, Vec3)> = tanks(kind).iter().flat_map(|h| h.holds.iter()).map(|&p| s.contents(p).map(|st| (st.weighed, s.parts[p as usize].local.transform_point3(st.at))).unwrap()).collect();
        let mut first = c0 * m0;
        let mut mass = m0;
        for ((w_was, at_was), (w_now, at_now)) in lay0.iter().zip(&lay1) {
            first += at_now.as_dvec3() * f64::from(*w_now) - at_was.as_dvec3() * f64::from(*w_was);
            mass += f64::from(*w_now) - f64::from(*w_was);
        }
        let by_hand = first / mass;
        assert!((mass - f64::from(s.mass)).abs() < 0.05 && (by_hand - s.com.as_dvec3()).length() < 2e-3, "{}: centro de masas en {:.4?}, a mano {by_hand:.4?}", kind.id, s.com);
        // ... which, with tanks side by side that empty alike, moves along the ship and down
        // (the liquid lies lower in them), not across it
        let moved = s.com.as_dvec3() - c0;
        eprintln!("   centro de masas: de {c0:.3?} a {:.3?} ({:.1} mm)", s.com, moved.length() * 1e3);
        // and all of it gone over again gives the same: nothing drifted
        let (mass, com, inertia) = (s.mass, s.com, s.inertia);
        s.refresh();
        assert!((s.mass - mass).abs() < 0.05 && (s.com - com).length() < 1e-4, "{}: pesada entera {:.2} kg en {:?}, llevaba {mass:.2} en {com:?}", kind.id, s.mass, s.com);
        for k in 0..3 {
            assert!((s.inertia.col(k) - inertia.col(k)).length() < s.inertia.col(k).length() * 1e-4, "{}: inercia {:?}, llevaba {inertia:?}", kind.id, s.inertia);
        }
    }
}

#[test]
fn a_holed_bottle_lightens_the_ship_by_its_gas_and_a_tank_filled_makes_it_heavier() {
    let (lib, kinds, _) = built();
    let kind = kinds.iter().find(|k| k.id == "alcotan").unwrap();
    let (mut s, mut ship) = bare(&lib, kind);
    run(&mut ship, &mut s, 1.62, 2.0);
    let bottle = kind.holders.iter().find(|h| h.id == "botella_n2").unwrap();
    let (m0, gas) = (s.mass, signal(&ship, "botella_n2.masa"));
    assert!((9.0..12.0).contains(&gas) && bottle.substance == "nitrogeno", "una botella de 48 L a 20 MPa lleva {gas:.1} kg de nitrógeno");
    // holed (hurt past working, still there): its gas goes; the bottle stays
    let part = bottle.holds[0] as usize;
    s.parts[part].hp = s.parts[part].max_hp * 0.2;
    s.parts[part].working = false;
    s.version += 1;
    run(&mut ship, &mut s, 1.62, 30.0);
    assert!(signal(&ship, "botella_n2.masa") < 0.01 && s.parts[part].alive);
    assert!((f64::from(m0 - s.mass) - gas).abs() < 0.1, "la nave pesa {:.2} kg menos sin {gas:.2} kg de nitrógeno", m0 - s.mass);
    // the recovered-air tank is empty as built and weighs what is put in it
    let tank = kind.holders.iter().find(|h| h.id == "deposito_aire").unwrap();
    assert!(tank.built == 0.0 && signal(&ship, "deposito_aire.masa") == 0.0 && (50.0..60.0).contains(&tank.capacity), "cabe {:.1} kg de aire", tank.capacity);
    let k = machine_of(kind, tank);
    let v = ship.machines[k].m.vessel().unwrap();
    let n = 6.0e6 * v.volume / (lunar_ship::atmos::R * v.t);
    v.gas = lunar_machines::gas::Mix { o2: n * 0.3, n2: n * 0.7, co2: 0.0 };
    let m1 = s.mass;
    run(&mut ship, &mut s, 1.62, 1.0);
    let air = signal(&ship, "deposito_aire.masa");
    assert!((26.0..29.0).contains(&air) && (f64::from(s.mass - m1) - air).abs() < 0.3, "a 6 MPa lleva {air:.1} kg de aire y la nave pesa {:.1} kg más", s.mass - m1);
    // a propellant tank refilled from the ground: heavier by what went in, up to what it takes
    let wing = tanks(kind)[0];
    let m2 = s.mass;
    ship.set_signal(&format!("{}.repostar", wing.id), 1.0);
    run(&mut ship, &mut s, 1.62, 40.0);
    let level = signal(&ship, wing.level.as_deref().unwrap());
    assert!((level - f64::from(wing.capacity)).abs() < 1e-3 && (s.mass - m2 - (wing.capacity - wing.built)).abs() < 0.5, "repostado lleva {level:.1} kg y la nave pesa {:.1} kg más", s.mass - m2);
}

#[test]
fn a_scanner_reads_what_is_left_in_a_tank_and_what_it_weighs_with_it() {
    let (lib, kinds, _) = built();
    let kind = kinds.iter().find(|k| k.id == "alcotan").unwrap();
    let (mut s, mut ship) = bare(&lib, kind);
    run(&mut ship, &mut s, 1.62, 1.0);
    let part = |id: &str| kind.parts.iter().position(|p| p == id).unwrap_or_else(|| panic!("no hay pieza {id}"));
    let read = |s: &Structure, id: &str| lunar_ship::cargo::scan(kind, s, &lib.catalog, part(id));
    let (title, told) = read(&s, "deposito_izq.cuerpo");
    eprintln!("{title}: {told}");
    assert_eq!(title, "Depósito de propelente");
    assert!(told.starts_with("1314 L de propelente · 88 % · ") && told.ends_with(" kg"), "{told}");
    assert_eq!(read(&s, "deposito_izq.proa").1, told, "cualquier pieza del depósito lo dice");
    assert!(read(&s, "botella_o2_1.cuerpo").1.starts_with("13 kg de oxígeno · 100 % · "), "{}", read(&s, "botella_o2_1.cuerpo").1);
    assert!(read(&s, "bucle_2").1.starts_with("35 L de refrigerante · 50 % · "), "{}", read(&s, "bucle_2").1);
    // half of it gone: it reads so, and lighter
    let k = machine_of(kind, kind.holders.iter().find(|h| h.id == "deposito_izq").unwrap());
    ship.machines[k].m.load(&[650.0]);
    run(&mut ship, &mut s, 1.62, 0.1);
    assert!(read(&s, "deposito_izq.cuerpo").1.starts_with("743 L de propelente · 50 % · "), "{}", read(&s, "deposito_izq.cuerpo").1);
}

// ------------------------------------------------------------------ flight

/// A level place to stand a ship on (the search the other ship tests use).
fn level(bodies: &BodyRegistry) -> DVec3 {
    let b = bodies.get(0);
    let mut best = (f64::MAX, DVec3::Y);
    let mut seed = 12345u64;
    for _ in 0..400 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = |k: u32| ((seed >> (11 + 17 * k)) & 0xffff) as f64 / 65535.0 * 2.0 - 1.0;
        let dir = DVec3::new(r(0), r(1), r(2)).normalize_or(DVec3::Y);
        let side = dir.any_orthonormal_vector();
        let fwd = dir.cross(side);
        let at = |x: f64, z: f64| b.height((dir * b.radius + side * x + fwd * z).normalize());
        let hs = [at(0.0, 0.0), at(24.0, 0.0), at(-24.0, 0.0), at(0.0, 24.0), at(0.0, -24.0), at(16.0, 16.0), at(-16.0, -16.0), at(16.0, -16.0), at(-16.0, 16.0)];
        let spread = hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min);
        if spread < best.0 {
            best = (spread, dir);
        }
    }
    best.1
}

#[test]
fn its_tanks_drained_unevenly_it_hovers_without_turning() {
    let bodies = bodies();
    let (lib, kinds, _) = built();
    let lib = Arc::new(lib);
    for kind in &kinds {
        // standing on the ground of the first body, its engines started
        let mut set_ = Structures::new(lib.clone());
        let id = set_.place(&kind.blueprint, &bodies, 0, level(&bodies), 0.0, f64::from(kind.lift)).unwrap();
        let mut ship = Ship::new(kind.clone(), id, 7).unwrap();
        let g = bodies.get(0).gravity as f32;
        ship.update(&mut set_.list[0], &w(g), 0.0);
        set_.rest_on_ground(id, &bodies);
        let sink = ship.rest_on_legs(&mut set_.list[0], g);
        let up = bodies.get(0).up(set_.list[0].pos);
        set_.list[0].pos -= up * f64::from(sink);
        let fly = |ship: &mut Ship, set_: &mut Structures, secs: f64| {
            for _ in 0..(secs * 60.0) as usize {
                let world = world_at(&set_.list[0], &bodies);
                for _ in 0..(1.0 / 60.0 / TICK).round().max(1.0) as usize {
                    ship.update(&mut set_.list[0], &world, TICK);
                }
                set_.step(1.0 / 60.0, &bodies);
            }
        };
        {
            let s = &mut set_.list[0];
            start_engines(&mut ship, s, &mut |ship, s, secs| run(ship, s, g, secs));
            // (and what turns it without propellant up to speed, where it has it: momentum wheels)
            run(&mut ship, s, g, 30.0);
        }
        // the tanks to port of its middle left with two fifths of what they take; the rest full
        let flight = kind.def.vuelo.clone().unwrap();
        let s = &mut set_.list[0];
        let middle = s.com.x;
        for h in tanks(kind) {
            let port = kind.centers[h.holds[0] as usize].x > middle;
            ship.machines[machine_of(kind, h)].m.load(&[f64::from(h.capacity) * if port { 0.4 } else { 1.0 }]);
        }
        run(&mut ship, s, g, 0.1);
        let off = (s.com.x - middle).abs();
        // MANTENER, and up: it leaves the ground and hangs there
        set(&mut ship, s, flight.estabilizador.as_deref().unwrap(), 1.0);
        set(&mut ship, s, flight.mantener.as_deref().unwrap(), 1.0);
        hold(&mut ship, s, &flight.traslacion.as_ref().unwrap()[1], 1.0);
        fly(&mut ship, &mut set_, 8.0);
        hold(&mut ship, &set_.list[0], &flight.traslacion.as_ref().unwrap()[1], 0.0);
        fly(&mut ship, &mut set_, 12.0);
        // ... for half a minute: how much it turns, how far it leans, what its engines and
        // its thrusters do about it
        let (mut spin, mut lean, mut jets, mut arm, mut left) = (0.0f32, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let lean0 = {
            let s = &set_.list[0];
            (s.rot * Vec3::Y).as_dvec3().angle_between(bodies.get(0).up(s.pos)).to_degrees()
        };
        let rcs: Vec<String> = {
            let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
            flight.rcs.iter().flat_map(|p| lunar_ship::kind::resolve(&ids, p)).map(|m| format!("{}.empuje", ids[m as usize])).collect()
        };
        let frames = 30 * 60;
        for _ in 0..frames {
            fly(&mut ship, &mut set_, 1.0 / 60.0);
            let s = &set_.list[0];
            let up = bodies.get(0).up(s.pos);
            spin = spin.max(s.spin.length());
            lean = lean.max((s.rot * Vec3::Y).as_dvec3().angle_between(up).to_degrees());
            jets += rcs.iter().map(|n| signal(&ship, n)).sum::<f64>() / f64::from(frames);
            arm += signal(&ship, "nave.centrado") / f64::from(frames);
            // what its engines' push, as they are trimmed, leaves across it about the centre of mass
            let (mut force, mut torque) = (Vec3::ZERO, Vec3::ZERO);
            for &m in &engines(kind) {
                let part = &s.parts[kind.machines[m].part.unwrap() as usize];
                let f = part.local.transform_vector3(kind.machines[m].thrust).normalize_or_zero() * signal(&ship, &format!("{}.empuje", kind.machines[m].id)) as f32;
                force += f;
                torque += (part.center - s.com).cross(f);
            }
            left += f64::from(torque.length() / force.length().max(1.0)) / f64::from(frames);
        }
        let s = &set_.list[0];
        let world = world_at(s, &bodies);
        let thrust: Vec<f64> = engines(kind).iter().map(|&m| signal(&ship, &format!("{}.empuje", kind.machines[m].id))).collect();
        let weight = f64::from(s.mass) * f64::from(world.gravity.length());
        eprintln!(
            "{}: {:.0} kg descentrada {:.0} mm de través (brazo de sus motores sin compensar {:.0} mm, compensados {:.1} mm): gira a {:.4} rad/s como mucho, inclinada {lean:.2}° (al empezar {lean0:.2}°), a {:.0} m y {:.2} m/s; motores {:?} kN, toberas {:.0} N de media",
            kind.id,
            s.mass,
            off * 1e3,
            arm * 1e3,
            left * 1e3,
            spin,
            world.altitude,
            world.speed,
            thrust.iter().map(|t| (t / 100.0).round() / 10.0).collect::<Vec<_>>(),
            jets
        );
        assert!(off > 0.004, "{}: vaciar los depósitos de un lado no mueve su centro de masas ({:.1} mm)", kind.id, off * 1e3);
        assert!(world.altitude > 5.0 && world.speed < 1.0, "{}: no se sostiene: a {:.1} m y {:.2} m/s", kind.id, world.altitude, world.speed);
        // (the computer trims its engines through where the centre of mass is: what is left of
        // their arm is a small part of it; and it does not turn: it ends as upright as it began)
        assert!(left < (arm * 0.25).max(0.004), "{}: sus motores, compensados, empujan a {:.1} mm del centro de masas (sin compensar, {:.0} mm)", kind.id, left * 1e3, arm * 1e3);
        // (it rolls a little as it comes upright after leaving the ground)
        assert!(spin < 0.03 && lean < lean0 + 4.0, "{}: descentrada, gira a {spin:.4} rad/s y se inclina {lean:.2}° (al empezar {lean0:.2}°)", kind.id);
        // (its engines hold it up between them, and it is they that take the off-centre load:
        // the thrusters barely fire)
        assert!((thrust.iter().sum::<f64>() - weight).abs() < weight * 0.06, "{}: sus motores empujan {:.0} N y pesa {weight:.0} N", kind.id, thrust.iter().sum::<f64>());
        assert!(jets < weight * 0.02, "{}: son las toberas las que la sostienen derecha ({jets:.0} N de media)", kind.id);
    }
}

#[test]
fn every_ship_lifts_off_full_and_sets_down_empty_under_the_gravity_of_every_body() {
    let bodies = bodies();
    let (lib, kinds, _) = built();
    eprintln!("{:<10} {:>9} {:>9} {:>10} {:>10} {:>8} {:>8} {:>9}", "nave", "llena kg", "seca kg", "empuje kN", "mínimo kN", "toberas", "Δv m/s", "suspen. s");
    for kind in &kinds {
        let (mut s, mut ship) = bare(&lib, kind);
        run(&mut ship, &mut s, 1.62, 1.0);
        let l = diag::lift(&ship, &s);
        // (hanging still: the weight is spent as exhaust, g·t = Δv)
        eprintln!("{:<10} {:>9.0} {:>9.0} {:>10.1} {:>10.1} {:>8.1} {:>8.0}", kind.id, l.full, l.dry, l.thrust / 1e3, l.least / 1e3, l.jets_up / 1e3, l.dv);
        assert!(l.thrust > 0.0 && l.least < l.thrust && l.dv > 50.0 && l.ve > 2000.0, "{}: {l:?}", kind.id);
        // Δv by hand: the rocket equation with what its engines' data says
        let ve = {
            let (mut f, mut flow) = (0.0, 0.0);
            for &m in &engines(kind) {
                let p = &kind.machines[m].def.params;
                let thrust = serde_json::from_value::<Q>(p["empuje_vacio"].clone()).unwrap().si().unwrap();
                let isp = serde_json::from_value::<Q>(p["isp_vacio"].clone()).unwrap().si().unwrap();
                f += thrust;
                flow += thrust / (isp * 9.806_65);
            }
            f / flow
        };
        assert!((l.dv - ve * (l.full / l.dry).ln()).abs() < 0.5 && (l.ve - ve).abs() < 0.01, "{}: Δv {:.1} m/s, a mano {:.1}", kind.id, l.dv, ve * (l.full / l.dry).ln());
        for (_, b) in bodies.iter() {
            let (full, dry) = l.ratios(b.gravity);
            eprintln!("   {:<12} g = {:.2} m/s²: empuje/peso {:.2} llena, {:.2} seca; suspendida {:.0} s", b.name, b.gravity, full, dry, l.dv / b.gravity);
            let faults = l.faults(b.gravity);
            assert!(faults.is_empty(), "{} en {}: {faults:?}", kind.id, b.name);
        }
        // what it says of itself where it is: thrust over weight as it stands, under this gravity
        let (g, m) = (signal(&ship, "nave.g"), signal(&ship, "nave.masa"));
        assert!((signal(&ship, "nave.empuje_peso") - l.thrust / (m * g)).abs() < 1e-3 && (signal(&ship, "nave.aceleracion") - l.thrust / m).abs() < 1e-3);
        // with all it is rated to take under its magnets and a ship of each kind it carries on its
        // back, tanks full: it still lifts off the first body
        let extra: f64 = kind.clamps.iter().filter_map(|c| c.zone).filter(|z| z.mode == lunar_ship::cargo::Mode::Grip).map(|z| f64::from(z.rated.min(z.max))).sum::<f64>()
            + kind.def.lleva.iter().map(|c| kinds.iter().find(|k| k.id == c.nave).map_or(0.0, |k| diag::lift(&bare(&lib, k).1, &bare(&lib, k).0).full)).sum::<f64>();
        let g0 = bodies.get(0).gravity;
        let loaded = l.thrust / ((l.full + extra) * g0);
        eprintln!("   con {extra:.0} kg más (imanes a su carga nominal, lo que lleva atracado): empuje/peso {loaded:.2}");
        assert!(loaded >= diag::LIFT_MARGIN, "{}: con {extra:.0} kg más no despega (empuje/peso {loaded:.2})", kind.id);
    }
    // and a ship that cannot is told so: the tug with engines of a fifth of the push, on the first body
    let (lib, kinds, _) = try_built(|src, ships| {
        ships.retain(|s| s.0 == "abejorro");
        src.components.get_mut("motor_elevacion").unwrap().maquina.as_mut().unwrap().params.insert("empuje_vacio".into(), serde_json::json!("1 kN"));
    })
    .unwrap();
    let (mut s, mut ship) = bare(&lib, &kinds[0]);
    run(&mut ship, &mut s, 1.62, 1.0);
    let faults = diag::lift(&ship, &s).faults(bodies.get(0).gravity);
    assert!(faults.len() == 1 && faults[0].contains("no despega"), "{faults:?}");
    assert!(signal(&ship, "nave.empuje_peso") < 1.0);
}

#[test]
fn its_engines_push_what_the_figures_say_full_and_at_their_least() {
    let bodies = bodies();
    let g = bodies.get(0).gravity;
    let (lib, kinds, _) = built();
    for kind in &kinds {
        let (mut s, mut ship) = bare(&lib, kind);
        let l = diag::lift(&ship, &s);
        start_engines(&mut ship, &mut s, &mut |ship, s, secs| run(ship, s, g as f32, secs));
        let throttle = kind.def.vuelo.as_ref().unwrap().acelerador.clone().unwrap();
        // at full: what really pushes it (trimmed through its centre of mass, fed by its tanks)
        // is the figure near enough, and beats its weight full with the margin
        set(&mut ship, &s, &throttle, 1.0);
        run(&mut ship, &mut s, g as f32, 4.0);
        let full = f64::from(s.force.dot(s.rot * Vec3::Y));
        set(&mut ship, &s, &throttle, 0.0);
        run(&mut ship, &mut s, g as f32, 4.0);
        let least = f64::from(s.force.dot(s.rot * Vec3::Y));
        eprintln!("{}: a tope {:.1} kN (nominal {:.1}), al mínimo {:.1} kN ({:.1}); pesa {:.1} kN llena y {:.1} kN seca: empuje/peso {:.2} y {:.2}", kind.id, full / 1e3, l.thrust / 1e3, least / 1e3, l.least / 1e3, l.full * g / 1e3, l.dry * g / 1e3, full / (l.full * g), full / (l.dry * g));
        assert!(full > l.thrust * 0.9 && full < l.thrust * 1.06, "{}: a tope empuja {full:.0} N de {:.0}: no le llega el propelente", kind.id, l.thrust);
        assert!(full > l.full * g * diag::LIFT_MARGIN, "{}: llena no despega: {full:.0} N contra {:.0}", kind.id, l.full * g);
        assert!(least < l.dry * g * diag::LAND_MARGIN && (least - l.least).abs() < l.least * 0.12, "{}: al mínimo empuja {least:.0} N (dice {:.0}) y seca pesa {:.0}", kind.id, l.least, l.dry * g);
    }
}

#[test]
fn its_legs_carry_it_full_and_empty_within_their_travel() {
    let bodies = bodies();
    let (lib, kinds, _) = built();
    for kind in &kinds {
        for (_, b) in bodies.iter() {
            let g = b.gravity as f32;
            let mut s = Structure::new(1, lib.blueprint(&kind.blueprint).unwrap(), &lib.catalog, DVec3::ZERO, Quat::IDENTITY);
            let mut ship = Ship::new(kind.clone(), 1, 7).unwrap();
            ship.update(&mut s, &w(g), 0.0);
            // as it is built, set down: its legs are made for this
            let sink = ship.rest_on_legs(&mut s, g);
            let rest = |s: &Structure| -> Vec<f32> { s.springs.iter().map(|sp| sp.x / sp.stroke).collect() };
            let (built, as_built) = (rest(&s), s.mass);
            // every tank full; then empty: where the same legs rest
            let mut sag = Vec::new();
            for share in [1.0f32, 0.0] {
                for h in tanks(kind) {
                    for (&p, &part) in h.holds.iter().zip(&h.shares) {
                        s.fill(p, h.capacity * part * share);
                    }
                }
                let sink = ship.rest_on_legs(&mut s, g);
                sag.push((s.mass, sink, rest(&s)));
            }
            eprintln!("{} en {} (g = {:.2}): así {as_built:.0} kg se asienta {sink:.3} m, patas a {built:.2?}; llena {:.0} kg {:.3} m {:.2?}; vacía {:.0} kg {:.3} m {:.2?}", kind.id, b.name, b.gravity, sag[0].0, sag[0].1, sag[0].2, sag[1].0, sag[1].1, sag[1].2);
            assert!(!s.springs.is_empty(), "{}: sin patas con muelle", kind.id);
            for (mass, sink, legs) in &sag {
                assert!(legs.iter().all(|x| (0.45..0.92).contains(x)), "{} en {} con {mass:.0} kg: patas a {legs:.2?} de su carrera", kind.id, b.name);
                assert!(*sink > 0.0);
            }
            // lighter, it stands higher; and between full and empty its legs move a few centimetres
            assert!(sag[1].1 < sag[0].1 && sag[0].1 - sag[1].1 < 0.12, "{}: de llena a vacía sus patas se mueven {:.3} m", kind.id, sag[0].1 - sag[1].1);
        }
    }
}

// ------------------------------------------------------------------ what a panel shows

#[test]
fn every_ship_says_what_it_weighs_and_shows_it_where_its_pilot_looks() {
    let (lib, kinds, _) = built();
    for kind in &kinds {
        let (mut s, mut ship) = bare(&lib, kind);
        run(&mut ship, &mut s, 1.62, 1.0);
        for name in ["nave.masa", "nave.masa_seca", "nave.propelente", "nave.propelente_nivel", "nave.dv", "nave.empuje_peso", "nave.aceleracion", "nave.centrado"] {
            assert!(signal(&ship, name).is_finite() && (signal(&ship, name) > 0.0 || name == "nave.centrado"), "{}: {name} = {}", kind.id, signal(&ship, name));
        }
        // the "masa" section on one of its panels, each of its instruments reading its signal
        let shown: Vec<(String, String)> = kind.panels.iter().flat_map(|p| p.def.mandos.iter().filter(|m| m.senal.as_deref().is_some_and(|s| s.starts_with("nave.masa") || s.starts_with("nave.prop") || s == "nave.dv" || s == "nave.empuje_peso")).map(|m| (format!("{}/{}", p.id, m.id), m.senal.clone().unwrap()))).collect();
        eprintln!("{}: {shown:?}", kind.id);
        for p in kind.panels.iter().filter(|p| shown.iter().any(|(id, _)| id.starts_with(&format!("{}/", p.id))) || kind.panels.len() == 1) {
            eprintln!("   panel {}: escala {:.1}, placa {:.0}×{:.0} mm", p.id, p.layout.scale, p.layout.size[0], p.layout.size[1]);
        }
        for sig in ["nave.masa", "nave.propelente", "nave.dv", "nave.empuje_peso"] {
            assert!(shown.iter().any(|(_, s)| s == sig), "{}: ningún panel enseña {sig}", kind.id);
        }
        // (and how full its tanks are: the section's bar, or an instrument of its own on a tank)
        assert!(kind.panels.iter().any(|p| p.def.mandos.iter().any(|m| m.senal.as_deref().is_some_and(|s| s == "nave.propelente_nivel" || tanks(kind).iter().any(|h| s == format!("{}.nivel", h.id))))), "{}: ningún panel enseña el nivel de propelente", kind.id);
        // read from a seat that works that panel (or it is the ship's only one)
        let panel = shown[0].0.split('/').next().unwrap().to_string();
        assert!(kind.seats.iter().any(|seat| seat.def.paneles.contains(&panel)), "{}: ningún asiento trabaja el panel {panel}", kind.id);
        // what its display writes: tonnes to two decimals
        let k = ship.panels.indicators.iter().position(|i| kind.panels[i.panel].def.mandos[i.index].senal.as_deref() == Some("nave.masa")).unwrap();
        let card = ship.panels.card_indicator(&kind.clone(), &ship.store, k);
        eprintln!("   {}: {} — {}", card.title, card.value, card.lines.first().cloned().unwrap_or_default());
        assert_eq!(card.value, format!("{:.2} t", s.mass / 1000.0).replace('.', ","), "{}: su indicador de masa, con {:.0} kg", kind.id, s.mass);
        // its multi-function displays, where it has them, have the page
        for p in &kind.panels {
            for d in p.def.mandos.iter().filter(|d| d.kind == "mfd" && d.paginas.iter().any(|pg| pg.auto.as_deref() == Some("masa"))) {
                let page = d.paginas.iter().find(|pg| pg.auto.as_deref() == Some("masa")).unwrap();
                assert!(page.elementos.len() >= 7 && page.elementos.iter().any(|e| e.senal.as_deref() == Some("nave.dv")), "{}: {}: la página MASA tiene {} instrumentos", kind.id, d.id, page.elementos.len());
            }
        }
    }
    // (the ships with multi-function displays have the page on one)
    for kind in kinds.iter().filter(|k| k.panels.iter().any(|p| p.def.mandos.iter().any(|d| d.kind == "mfd"))) {
        assert!(kind.panels.iter().any(|p| p.def.mandos.iter().any(|d| d.paginas.iter().any(|pg| pg.auto.as_deref() == Some("masa")))), "{}: tiene pantallas multifunción y ninguna con la página MASA", kind.id);
    }
}

// ------------------------------------------------------------------ what it costs

#[test]
fn a_ship_with_nothing_flowing_is_never_weighed_and_a_tank_that_drains_costs_a_handful_of_sums() {
    let (lib, kinds, _) = built();
    for kind in &kinds {
        let (mut s, mut ship) = bare(&lib, kind);
        run(&mut ship, &mut s, 1.62, 5.0);
        // its systems on and nothing burning: not weighed once in a minute, its figures untouched
        let (w0, v0) = (s.weighings, ship.store.version(ship.store.find("nave.masa").unwrap()));
        let t = Instant::now();
        let ticks = (60.0 / TICK) as u32;
        run(&mut ship, &mut s, 1.62, 60.0);
        let idle = t.elapsed().as_secs_f64() * 1e6 / f64::from(ticks);
        assert_eq!(s.weighings, w0, "{}: en reposo se vuelve a pesar", kind.id);
        assert_eq!(ship.store.version(ship.store.find("nave.masa").unwrap()), v0, "{}: en reposo vuelve a escribir lo que pesa", kind.id);
        // burning at full: weighed a quantum of a tank at a time, never every tick
        start_engines(&mut ship, &mut s, &mut |ship, s, secs| run(ship, s, 1.62, secs));
        let throttle = kind.def.vuelo.as_ref().unwrap().acelerador.clone().unwrap();
        set(&mut ship, &s, &throttle, 1.0);
        run(&mut ship, &mut s, 1.62, 3.0);
        let (w1, fuel) = (s.weighings, signal(&ship, "nave.propelente"));
        let t = Instant::now();
        let ticks = (400.0 / TICK) as u32;
        run(&mut ship, &mut s, 1.62, 400.0);
        let burning = t.elapsed().as_secs_f64() * 1e6 / f64::from(ticks);
        let (weighed, burnt) = (s.weighings - w1, fuel - signal(&ship, "nave.propelente"));
        // (each piece of a tank is weighed once for every quantum of its own that leaves it:
        // half a hundredth of what the tank takes, whatever the piece's share of it)
        let pieces: usize = tanks(kind).iter().map(|h| h.holds.len()).sum();
        let capacity: f64 = tanks(kind).iter().map(|h| f64::from(h.capacity)).sum();
        let quanta = burnt / capacity / f64::from(QUANTUM) * pieces as f64;
        eprintln!("{}: tic en reposo {idle:.1} µs, quemando {burning:.1} µs; {burnt:.0} kg en 400 s: pesada {weighed} veces en {ticks} tics ({pieces} piezas con propelente: {quanta:.0} cuantos)", kind.id);
        assert!(weighed >= 3 && (weighed as f64) < quanta + pieces as f64 + 2.0, "{}: pesada {weighed} veces quemando {burnt:.0} kg ({quanta:.0} cuantos)", kind.id);
        assert!(burning < 500.0 && burning < idle * 1.5 + 5.0, "{}: {burning:.0} µs por tic quemando, {idle:.0} en reposo", kind.id);
    }
    // what one weighing costs: the block of a tank alone against going over every part
    let kind = kinds.iter().max_by_key(|k| k.parts.len()).unwrap();
    let (mut s, _) = bare(&lib, kind);
    let h = tanks(kind)[0];
    let (part, cap) = (h.holds[0], s.contents(h.holds[0]).unwrap().capacity);
    let n = 20_000u32;
    let w0 = s.weighings;
    let t = Instant::now();
    for k in 0..n {
        // (each a quantum and a bit from the last: every one is weighed)
        s.fill(part, cap * (0.2 + 0.006 * (k % 100) as f32));
    }
    let one = t.elapsed().as_secs_f64() * 1e9 / f64::from(n);
    assert_eq!(s.weighings - w0, u64::from(n));
    let t = Instant::now();
    for _ in 0..n * 10 {
        // (asked for what it holds: nothing to do)
        s.fill(part, cap * 0.2);
    }
    let none = t.elapsed().as_secs_f64() * 1e9 / f64::from(n * 10);
    let t = Instant::now();
    for _ in 0..200 {
        s.refresh();
    }
    let all = t.elapsed().as_secs_f64() * 1e9 / 200.0;
    eprintln!("{} ({} piezas): pesar el bloque de un depósito {one:.0} ns; pedirle lo que ya lleva {none:.1} ns; pesar la nave entera {:.1} µs ({:.0} veces más)", kind.id, s.parts.len(), all / 1e3, all / one);
    assert!(one < 2_000.0 && one * 20.0 < all, "pesar un bloque cuesta {one:.0} ns y la nave entera {all:.0} ns");
    assert!(none < 100.0, "pedir lo que ya lleva cuesta {none:.1} ns");
}
