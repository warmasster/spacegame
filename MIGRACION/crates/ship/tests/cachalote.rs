//! The Cachalote, the heavy freighter: its four engines lift it loaded; its gantry crane takes a
//! pallet off its clamp with its magnet, lifts it and carries it down the hold; its dorsal cradle
//! holds an Abejorro, lets it go and takes it back; its ramp reaches the ground and no further.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, set::Structures, state::Structure},
};
use lunar_ship::{Ship, ShipKind, ShipLibrary, World, ship::TICK};
use std::{path::Path, sync::Arc};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn kinds() -> (Library, ShipLibrary) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    (lib, ships)
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn control(ship: &Ship, id: &str) -> usize {
    ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"))
}

/// The Moon, and a Cachalote standing on level ground with its systems started.
fn world() -> (Structures, BodyRegistry, Ship, u64, Arc<ShipKind>, ShipLibrary) {
    let (lib, ships) = kinds();
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs().join("bodies/luna.jsonc")).unwrap()).unwrap();
    let bodies = BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]);
    let kind = ships.get("cachalote").unwrap().clone();
    let mut set = Structures::new(Arc::new(lib));
    let id = set.place(&kind.blueprint, &bodies, 0, level(&bodies), 0.0, f64::from(kind.lift)).unwrap();
    let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut set.list[0], &w(), 0.0);
    set.rest_on_ground(id, &bodies);
    // down on its legs, as the game sets it
    let up = bodies.get(0).up(set.list[0].pos);
    let sink = ship.rest_on_legs(&mut set.list[0], 1.62);
    set.list[0].pos -= up * f64::from(sink);
    (set, bodies, ship, id, kind, ships)
}

fn level(bodies: &BodyRegistry) -> DVec3 {
    let b = bodies.get(0);
    let mut best = (f64::MAX, DVec3::Y);
    let mut seed = 4242u64;
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

fn ship_of(set: &mut Structures, id: u64) -> &mut Structure {
    set.list.iter_mut().find(|s| s.id == id).unwrap()
}

/// The ship and the world on for `secs`: its systems, what its clamps ask, the bodies.
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

#[test]
fn its_four_engines_lift_it_loaded() {
    let (mut set, bodies, mut ship, id, _, _) = world();
    live(&mut set, &bodies, &mut ship, id, 3.0);
    for what in ["pedestal/tapa_arm_izq", "pedestal/arm_izq", "pedestal/tapa_arm_der", "pedestal/arm_der", "pedestal/arr_izq", "pedestal/arr_der", "pedestal/arr_izq", "pedestal/arr_der"] {
        press(&mut set, &bodies, &mut ship, id, what);
    }
    live(&mut set, &bodies, &mut ship, id, 4.0);
    lever(&set, &mut ship, id, "pedestal/acelerador", 0.8);
    // (held down: what its engines give, not where it goes)
    for _ in 0..(3.0 / TICK) as usize {
        ship.update(ship_of(&mut set, id), &w(), TICK);
    }
    let s = set.get(id).unwrap();
    // (what pushes it, along its own up)
    let lift = s.force.dot(s.rot * Vec3::Y);
    let get = |n: &str| ship.signal(n).unwrap_or(f64::NAN);
    eprintln!(
        "motores: {:.0} {:.0} {:.0} {:.0} kN; fuerza {:.0} kN, masa {:.1} t (peso {:.0} kN)",
        get("gondola_izq.empuje") / 1e3,
        get("gondola_der.empuje") / 1e3,
        get("gondola_popa_izq.empuje") / 1e3,
        get("gondola_popa_der.empuje") / 1e3,
        lift / 1e3,
        s.mass / 1e3,
        s.mass * 1.62 / 1e3
    );
    if std::env::var("LUNA_BALANCE").is_ok() {
        // what sits off the middle, by component: mass and its moment about the centreline
        let mut by: std::collections::BTreeMap<String, (f32, f32)> = std::collections::BTreeMap::new();
        for (i, p) in s.parts.iter().enumerate().filter(|(_, p)| p.alive) {
            let name = ship.kind.parts[i].split('.').next().unwrap_or("").trim_end_matches(|c: char| c.is_ascii_digit() || c == '#').to_string();
            let e = by.entry(name).or_default();
            e.0 += p.mass;
            e.1 += p.mass * p.center.x;
        }
        let mut list: Vec<_> = by.into_iter().collect();
        list.sort_by(|a, b| b.1.1.abs().total_cmp(&a.1.1.abs()));
        for (name, (m, mx)) in list.iter().take(16) {
            eprintln!("  {name}: {m:.0} kg, momento {mx:.0} kg·m");
        }
    }
    let mut jets = Vec::new();
    ship.jets(s, &mut jets);
    eprintln!("centro de masas {:.2?}; empujan en {:?}", s.com, jets.iter().map(|j| format!("({:.1} {:.1} {:.1}) {:.0} kN", j.0.x, j.0.y, j.0.z, j.2 / 1e3)).collect::<Vec<_>>());
    for e in ship.blackbox.entries.iter().rev().take(8) {
        eprintln!("  t {:.1}: {}", e.t, e.text);
    }
    assert!(lift > s.mass * 1.62 * 1.3, "no levanta su peso con margen: {:.0} kN contra {:.0}", lift / 1e3, s.mass * 1.62 / 1e3);
    // and they push through its centre of mass, wherever its reactor and its cargo have put it:
    // the computer trims them against each other (their arm, under a hand's breadth)
    eprintln!("par {:.0} N·m: brazo {:.3} m", s.torque.length(), s.torque.length() / lift);
    assert!(s.torque.length() < lift * 0.12, "los motores la cabecean: par {:.0} N·m con {:.0} N", s.torque.length(), lift);
}

#[test]
fn its_crane_takes_a_pallet_off_its_clamp_and_carries_it_down_the_hold() {
    let (mut set, bodies, mut ship, id, kind, _) = world();
    live(&mut set, &bodies, &mut ship, id, 2.0);
    assert!(set.get(id).unwrap().resting, "no se queda quieta");
    let structures = set.list.len();
    // the pallet let go by its clamp: a body of its own on the deck
    ship.set_signal("anclaje_b1.soltar", 1.0);
    live(&mut set, &bodies, &mut ship, id, 1.0);
    assert_eq!(set.list.len(), structures + 1, "el palé no se suelta de su anclaje");
    let pallet = set.list.last().unwrap().id;
    let aboard = |set: &Structures| {
        let (sh, p) = (set.get(id).unwrap(), set.get(pallet).unwrap());
        sh.to_local(p.to_world(p.center))
    };
    let before = aboard(&set);
    // the crane over it, the magnet down on it
    let clamp = kind.clamps.iter().position(|c| c.id == "grua_iman").expect("el imán de la grúa");
    lever(&set, &mut ship, id, "grua/puente", 0.0774);
    live(&mut set, &bodies, &mut ship, id, 6.0);
    // (all the way down: it stops on the pallet)
    lever(&set, &mut ship, id, "grua/gancho", 1.0);
    live(&mut set, &bodies, &mut ship, id, 12.0);
    press(&mut set, &bodies, &mut ship, id, "grua/tapa_iman");
    press(&mut set, &bodies, &mut ship, id, "grua/agarre");
    live(&mut set, &bodies, &mut ship, id, 1.5);
    eprintln!("imán: activo {:?}, sujeta {:?}; puente {:.3}, gancho {:.3}", ship.signal("grua_iman.activo"), ship.signal("grua_iman.sujeta"), ship.signal("grua_puente.pos").unwrap(), ship.signal("grua_gancho.pos").unwrap());
    assert_eq!(ship.clamp_held[clamp], vec![pallet], "el imán no toma el palé");
    assert!(set.get(pallet).unwrap().held.is_some_and(|h| h.by == id));
    // up, and down the hold with it
    // (both levers at once: the bridge waits for the load to clear what is in its way, then
    // carries it over the pallets of the next rows)
    lever(&set, &mut ship, id, "grua/gancho", 0.0);
    lever(&set, &mut ship, id, "grua/puente", 0.5);
    live(&mut set, &bodies, &mut ship, id, 26.0);
    let after = aboard(&set);
    eprintln!("palé: de {before:.2?} a {after:.2?}");
    assert!(after.y > before.y + 0.4, "no lo levanta: {:.2} m", after.y - before.y);
    assert!(after.z > before.z + 4.5, "no lo lleva: {:.2} m", after.z - before.z);
    assert!((after.x - before.x).abs() < 0.3);
    // (while the crane works the ship does not move under it)
    assert!(set.get(id).unwrap().vel.length() < 0.2);
    // let go: it falls to the deck and stays in the hold
    press(&mut set, &bodies, &mut ship, id, "grua/agarre");
    live(&mut set, &bodies, &mut ship, id, 4.0);
    assert!(ship.clamp_held[clamp].is_empty() && set.get(pallet).unwrap().held.is_none(), "el imán no suelta");
    let down = aboard(&set);
    eprintln!("soltado: {down:.2?}");
    assert!(down.y < after.y - 0.3 && down.y > before.y - 0.3, "soltado, queda a {:.2} m (estaba a {:.2})", down.y, before.y);
    assert!((down.z - after.z).abs() < 1.0);
}

#[test]
fn its_cradle_holds_a_tug_lets_it_go_and_takes_it_back() {
    let (mut set, bodies, mut ship, id, kind, ships) = world();
    let tug = ships.get("abejorro").unwrap().clone();
    assert_eq!(kind.def.lleva.len(), 1, "lleva un Abejorro en el lomo");
    let c = kind.clamps.iter().position(|c| c.id == kind.def.lleva[0].anclaje).expect("la cuna");
    let zone = kind.clamps[c].zone.expect("la cuna toma lo que se posa en ella");
    // the tug made over the cradle, as the game does, and taken
    let (at, rot) = {
        let s = set.get(id).unwrap();
        (s.to_world(zone.centre), s.rot)
    };
    let tid = set.spawn(&tug.blueprint, at, rot).unwrap();
    {
        let k = set.index_of(tid).unwrap();
        let t = &mut set.list[k];
        t.pos = at - (t.rot * t.center).as_dvec3();
    }
    ship.work_clamp(c, false);
    lunar_ship::cargo::serve(&mut ship, &mut set);
    live(&mut set, &bodies, &mut ship, id, 1.0);
    assert_eq!(ship.clamp_held[c], vec![tid], "la cuna no toma el Abejorro");
    assert_eq!(ship.signal("cuna.sujeta"), Some(1.0));
    let alone = kind_mass(&set, id) - set.get(tid).unwrap().mass;
    assert!(alone > 0.0 && set.get(id).unwrap().mass > alone + 2000.0, "la nave no carga con el peso del Abejorro");
    // held, it goes with the ship; the ship still rests
    let on_back = {
        let (s, t) = (set.get(id).unwrap(), set.get(tid).unwrap());
        s.to_local(t.to_world(t.center))
    };
    assert!(on_back.y > 5.5 && on_back.x.abs() < 0.3, "atracado en {on_back:.2?}");
    // (its legs take the two tonnes more: it settles a little, and rests)
    live(&mut set, &bodies, &mut ship, id, 4.0);
    assert!(set.get(id).unwrap().resting);
    // LIBRE: the tug is its own again, standing on the roof
    press(&mut set, &bodies, &mut ship, id, "auxiliar/tapa_cuna");
    press(&mut set, &bodies, &mut ship, id, "auxiliar/abrir");
    live(&mut set, &bodies, &mut ship, id, 3.0);
    assert!(ship.clamp_held[c].is_empty() && set.get(tid).unwrap().held.is_none(), "la cuna no lo suelta");
    assert_eq!(ship.signal("cuna.sujeta"), Some(0.0));
    let free = {
        let (s, t) = (set.get(id).unwrap(), set.get(tid).unwrap());
        s.to_local(t.to_world(t.center))
    };
    assert!((free - on_back).length() < 0.6, "suelto, se va de la cuna: {free:.2?}");
    // ANCLADA again: taken back
    press(&mut set, &bodies, &mut ship, id, "auxiliar/abrir");
    live(&mut set, &bodies, &mut ship, id, 2.0);
    assert_eq!(ship.clamp_held[c], vec![tid], "no lo vuelve a tomar");
}

fn kind_mass(set: &Structures, id: u64) -> f32 {
    set.get(id).unwrap().mass
}

#[test]
fn its_ramp_reaches_the_ground_and_no_further() {
    let (mut set, bodies, mut ship, id, kind, _) = world();
    live(&mut set, &bodies, &mut ship, id, 4.0);
    let s = set.get(id).unwrap();
    assert_eq!(ship.signal("rampa.cerrada"), Some(0.0), "la rampa empieza abierta");
    // the lowest of the ramp against the lowest of its feet: it lies on the ground with them
    let low = |names: &dyn Fn(&str) -> bool| kind.parts.iter().enumerate().filter(|(_, n)| names(n)).flat_map(|(i, _)| s.parts[i].shape.verts().map(move |v| s.parts[i].local.transform_point3(v).y)).fold(f32::MAX, f32::min);
    let (ramp, feet) = (low(&|n: &str| n.starts_with("rampa")), low(&|n: &str| n.contains("_pie")));
    eprintln!("rampa hasta {ramp:.2} m, pies a {feet:.2} m");
    assert!(ramp < 0.0 && feet < 0.0);
    assert!(ramp > feet - 0.05, "la rampa baja {:.2} m más que los pies: la nave se apoyaría en ella", feet - ramp);
    assert!(ramp < feet + 0.25, "la rampa se queda a {:.2} m del suelo", ramp - feet);
}

#[test]
fn its_console_reads_the_bay_under_the_hook_in_the_grid_painted_on_the_deck() {
    let (mut set, bodies, mut ship, id, kind, _) = world();
    live(&mut set, &bodies, &mut ship, id, 1.0);
    let magnet = kind.parts.iter().position(|p| p.starts_with("grua_iman")).expect("el imán de la grúa");
    // the bays as the ship's data has them: a clamp each, named by its column and its row
    let bay = |name: &str| {
        let k = kind.parts.iter().position(|p| p.starts_with(&format!("anclaje_{name}"))).unwrap_or_else(|| panic!("no hay anclaje {name}"));
        kind.centers[k]
    };
    let sill = bay("b1").z - 2.6;
    for (name, col, row) in [("b1", 1.0, 1.0), ("a8", 0.0, 8.0), ("c4", 2.0, 4.0)] {
        let at = bay(name);
        lever(&set, &mut ship, id, "grua/puente", f64::from((at.z + 20.6) / 15.5));
        lever(&set, &mut ship, id, "grua/carro", f64::from(at.x / 6.0 + 0.5));
        live(&mut set, &bodies, &mut ship, id, 34.0);
        let read = |n: &str| ship.signal(n).unwrap_or_else(|| panic!("no hay señal {n}"));
        assert_eq!((read("grua.columna"), read("grua.fila")), (col, row), "sobre {name}");
        assert_eq!(read("grua.centrada"), 1.0, "sobre {name}: no dice que está centrada");
        // its figures are the deck's: metres from the sill and to port, where the magnet is
        let s = set.get(id).unwrap();
        let hook = s.bones[usize::from(s.parts[magnet].bone)].transform_point3(kind.centers[magnet]);
        assert!((read("grua.largo") - f64::from(hook.z - sill)).abs() < 0.05, "{name}: largo {:.2}, el imán a {:.2}", read("grua.largo"), hook.z - sill);
        assert!((read("grua.banda") - f64::from(hook.x)).abs() < 0.05, "{name}: banda {:.2}, el imán a {:.2}", read("grua.banda"), hook.x);
    }
    // between two bays it says so
    lever(&set, &mut ship, id, "grua/carro", f64::from((bay("c4").x + 0.5) / 6.0 + 0.5));
    live(&mut set, &bodies, &mut ship, id, 6.0);
    assert_eq!(ship.signal("grua.centrada"), Some(0.0));
    // and the grid is painted: its cells' lines, its rules, the names of its columns and rows
    let d = &kind.def;
    assert!(d.calcas.iter().filter(|c| c.imagen == "linea").count() >= 40 && d.calcas.iter().filter(|c| c.imagen == "regla").count() >= 30);
    for t in ["A", "B", "C", "1", "8", "2 m", "14 m", "BABOR", "ESTRIBOR"] {
        assert!(d.rotulos.iter().any(|l| l.texto == t && l.normal[1] > 0.9), "falta el rótulo '{t}' en el suelo");
    }
}
