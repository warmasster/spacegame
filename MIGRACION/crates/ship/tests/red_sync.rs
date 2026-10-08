//! A ship as bytes and back (`lunar_ship::sync`): what keeps the copies of a ship in several
//! games in agreement. For every ship there is:
//! - a snapshot of one copy leaves another equal (its own snapshot is then the same bytes), after
//!   controls worked, systems run, parts hurt and destroyed, cargo let go;
//! - their digests agree when they are equal and not when one of them drifts;
//! - what changed is told as a delta that leaves the other copy's parts the same;
//! - a piece come off a ship is made in a game that never had it;
//! - no bytes whatever make a reader panic.
use glam::{DVec3, Quat, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, set::Structures, state::Structure},
};
use lunar_ship::{
    Ship, ShipKind, ShipLibrary, World,
    ship::TICK,
    sync::{self, Digest, In, Shadow, SyncError},
};
use std::{path::Path, sync::Arc};

const SHIPS: [&str; 3] = ["alcotan", "abejorro", "cachalote"];

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn library() -> (Arc<Library>, ShipLibrary, BodyRegistry) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let def: BodyDef = lunar_core::defs::parse("luna", &std::fs::read_to_string(defs().join("bodies/luna.jsonc")).unwrap()).unwrap();
    (Arc::new(lib), ships, BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()]))
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

/// A game's structures with a ship of `kind` in them, floating (nothing here needs the ground).
fn game(lib: &Arc<Library>, kind: &Arc<ShipKind>) -> (Structures, Ship, u64) {
    let mut set = Structures::new(lib.clone());
    let id = set.spawn(&kind.blueprint, DVec3::new(0.0, 1_800_000.0, 0.0), Quat::IDENTITY).unwrap();
    let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut set.list[0], &w(), 0.0);
    (set, ship, id)
}

fn run(ship: &mut Ship, s: &mut Structure, secs: f64) {
    for _ in 0..(secs / TICK).round() as usize {
        ship.update(s, &w(), TICK);
    }
}

/// Everything of a ship and its structure as bytes: its own state, then its systems.
fn snapshot(ship: &Ship, s: &Structure) -> Vec<u8> {
    let mut out = Vec::new();
    sync::write_state(s, &mut out);
    sync::write_ship(ship, s, &|id| Some(id), &mut out);
    out
}

fn restore(ship: &mut Ship, s: &mut Structure, bytes: &[u8]) -> Result<Vec<u32>, SyncError> {
    let mut inp = In::new(bytes);
    let mut died = Vec::new();
    sync::read_state(s, &mut inp, &mut died)?;
    sync::read_ship(ship, s, &|id| Some(id), &mut inp)?;
    assert!(inp.is_empty(), "todo se lee");
    Ok(died)
}

/// A ship lived in for a while: controls worked, its systems run, parts hurt and destroyed.
fn lived(ship: &mut Ship, s: &mut Structure) {
    let kind = ship.kind.clone();
    // every control a press changes, pressed (covers open, switches thrown, breakers set)
    for round in 0..2 {
        for k in 0..ship.panels.controls.len() {
            if (k + round) % 3 == 0 {
                continue;
            }
            ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
            ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
        }
        run(ship, s, 4.0);
    }
    // parts hurt, parts gone, a joint given
    let n = s.parts.len();
    for (k, share) in [(n / 7, 0.5), (n / 5, 0.1), (n / 3, 0.9)] {
        s.parts[k].hp = s.parts[k].max_hp * share;
    }
    for k in [n / 11, n / 2] {
        (s.parts[k].alive, s.parts[k].working, s.parts[k].hp) = (false, false, 0.0);
    }
    if let Some(j) = s.joints.get_mut(3) {
        (j.alive, j.hp) = (false, 0.0);
    }
    // what a container holds, half gone
    if let Some(part) = s.stored.first().map(|st| st.part) {
        let half = s.stored[0].mass * 0.5;
        s.take(part, half);
    }
    s.refresh();
    run(ship, s, 6.0);
}

#[test]
fn a_snapshot_of_a_ship_leaves_another_copy_equal() {
    let (lib, ships, _) = library();
    for name in SHIPS {
        let kind = ships.get(name).unwrap();
        let (mut sa, mut a, _) = game(&lib, kind);
        let (mut sb, mut b, _) = game(&lib, kind);
        // two copies just made say the same, and that is a few bytes
        let fresh = snapshot(&a, &sa.list[0]);
        assert!(fresh == snapshot(&b, &sb.list[0]), "{name}: dos copias recién hechas no dicen lo mismo");
        let mut state = Vec::new();
        sync::write_state(&sa.list[0], &mut state);
        assert!(Digest::of(Some(&a), &sa.list[0]).agrees(&Digest::of(Some(&b), &sb.list[0])));
        lived(&mut a, &mut sa.list[0]);
        let before = Digest::of(Some(&b), &sb.list[0]);
        assert!(!Digest::of(Some(&a), &sa.list[0]).agrees(&before), "{name}: lo vivido no cambia el resumen");
        // told to the other copy: it is left the same, to the byte
        let bytes = snapshot(&a, &sa.list[0]);
        let gone = sa.list[0].parts.iter().zip(&sb.list[0].parts).filter(|(p, q)| !p.alive && q.alive).count();
        let died = restore(&mut b, &mut sb.list[0], &bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(gone >= 2 && died.len() == gone, "{name}: se fueron {gone} piezas y se dicen {}", died.len());
        assert!(snapshot(&b, &sb.list[0]) == bytes, "{name}: la copia no queda igual");
        let (da, db) = (Digest::of(Some(&a), &sa.list[0]), Digest::of(Some(&b), &sb.list[0]));
        assert_eq!(da, db, "{name}: el resumen de las dos copias");
        // (a part that goes while the systems run is weighed out the next time anything is: now)
        sa.list[0].refresh();
        let (x, y) = (&sa.list[0], &sb.list[0]);
        // (what is held is weighed again a quantum at a time: the two may be that far apart)
        let quantum: f32 = x.stored.iter().map(|st| st.capacity).sum::<f32>() * lunar_core::structure::contents::QUANTUM;
        assert!((x.mass - y.mass).abs() <= quantum + 1e-3 * x.mass && x.stored.iter().zip(&y.stored).all(|(p, q)| p.mass == q.mass), "{name}: pesan {} y {} kg", x.mass, y.mass);
        assert!(x.parts.iter().zip(&y.parts).all(|(p, q)| (p.alive, p.working, p.hp, p.left) == (q.alive, q.working, q.hp, q.left)));
        assert!(a.joints.iter().zip(&b.joints).all(|(p, q)| p.q == q.q && p.qd == q.qd));
        assert!(a.store.values() == b.store.values());
        // and both go on the same from there
        run(&mut a, &mut sa.list[0], 2.0);
        run(&mut b, &mut sb.list[0], 2.0);
        assert!(Digest::of(Some(&a), &sa.list[0]).agrees(&Digest::of(Some(&b), &sb.list[0])), "{name}: dos copias iguales no siguen igual");
        eprintln!("{name}: {} piezas, {} señales, {} máquinas, {} mandos; recién hecha su estado propio son {} bytes y todo {} bytes; vivida, {} bytes; el resumen, {} bytes", sa.list[0].parts.len(), a.store.len(), a.machines.len(), a.panels.controls.len(), state.len(), fresh.len(), bytes.len(), sync::DIGEST_BYTES);
        assert!(state.len() < 1000 && bytes.len() < 20_000, "{name}: {} y {} bytes", state.len(), bytes.len());
        // a ship of another kind is not told this way
        let other = ships.get(SHIPS.iter().find(|n| **n != name).unwrap()).unwrap();
        let (mut so, mut o, _) = game(&lib, other);
        assert!(matches!(restore(&mut o, &mut so.list[0], &bytes), Err(SyncError::Count(_))), "{name}: una nave de otro tipo lo acepta");
    }
}

#[test]
fn a_copy_that_drifts_is_caught_by_the_digest() {
    let (lib, ships, _) = library();
    for name in SHIPS {
        let kind = ships.get(name).unwrap();
        let (mut sa, mut a, _) = game(&lib, kind);
        let (mut sb, mut b, _) = game(&lib, kind);
        run(&mut a, &mut sa.list[0], 3.0);
        run(&mut b, &mut sb.list[0], 3.0);
        let truth = Digest::of(Some(&a), &sa.list[0]);
        assert!(truth.agrees(&Digest::of(Some(&b), &sb.list[0])), "{name}: dos copias que han hecho lo mismo");
        let differs = |b: &Ship, s: &Structure| !truth.agrees(&Digest::of(Some(b), s));
        // a control left elsewhere
        let k = (0..b.panels.controls.len()).find(|&k| b.panels.intent(k, &Intent::Press { elem: 0 }, &sb.list[0], kind, &b.store).changed).expect("un mando que cambie");
        assert!(differs(&b, &sb.list[0]), "{name}: un mando en otra posición");
        let keep = snapshot(&a, &sa.list[0]);
        restore(&mut b, &mut sb.list[0], &keep).unwrap();
        assert!(!differs(&b, &sb.list[0]), "{name}: mando {k} puesto como en la otra");
        // a part gone, a part that no longer works, a joint that gave
        let n = sb.list[0].parts.len();
        sb.list[0].parts[n / 4].alive = false;
        assert!(differs(&b, &sb.list[0]));
        sb.list[0].parts[n / 4].alive = true;
        sb.list[0].parts[n / 6].working = false;
        assert!(differs(&b, &sb.list[0]));
        sb.list[0].parts[n / 6].working = true;
        sb.list[0].joints[1].alive = false;
        assert!(differs(&b, &sb.list[0]));
        sb.list[0].joints[1].alive = true;
        assert!(!differs(&b, &sb.list[0]));
        // hit points: a scratch is within what need only be near; a part half gone is not
        sb.list[0].parts[n / 4].hp *= 0.98;
        assert!(!differs(&b, &sb.list[0]), "{name}: un rasguño no es una diferencia");
        sb.list[0].parts[n / 4].hp *= 0.4;
        assert!(differs(&b, &sb.list[0]), "{name}: media pieza menos sí");
        sb.list[0].parts[n / 4].hp = sb.list[0].parts[n / 4].max_hp;
        // the air of a compartment gone, a door latched or not
        if let Some(air) = b.atmos.air.first_mut() {
            (air.o2, air.n2) = (air.o2 * 0.5, air.n2 * 0.5);
            assert!(differs(&b, &sb.list[0]), "{name}: medio aire menos");
            restore(&mut b, &mut sb.list[0], &keep).unwrap();
        }
        if !b.latched.is_empty() {
            b.latched[0] = !b.latched[0];
            assert!(differs(&b, &sb.list[0]));
            b.latched[0] = !b.latched[0];
        }
        assert!(!differs(&b, &sb.list[0]));
    }
}

#[test]
fn what_changed_is_told_as_a_delta() {
    let (lib, ships, _) = library();
    let kind = ships.get("alcotan").unwrap();
    let (mut sa, _, _) = game(&lib, kind);
    let (mut sb, _, _) = game(&lib, kind);
    let (a, b) = (&mut sa.list[0], &mut sb.list[0]);
    let mut shadow = Shadow::of(a);
    let mut out = Vec::new();
    assert!(!shadow.delta(a, &mut out) && out.is_empty(), "nada cambió: nada que contar");
    // a hit: three parts hurt, one gone with its joints
    for (k, share) in [(10, 0.5), (11, 0.25), (40, 0.9)] {
        a.parts[k].hp = a.parts[k].max_hp * share;
    }
    (a.parts[12].alive, a.parts[12].working, a.parts[12].hp) = (false, false, 0.0);
    let gave: Vec<usize> = a.joints.iter().enumerate().filter(|(_, j)| j.a == 12 || j.b == 12).map(|(k, _)| k).collect();
    for &k in &gave {
        (a.joints[k].alive, a.joints[k].hp) = (false, 0.0);
    }
    a.refresh();
    assert!(shadow.delta(a, &mut out));
    eprintln!("un impacto (3 piezas dañadas, 1 destruida, {} uniones rotas): {} bytes", gave.len(), out.len());
    assert!(out.len() < 40 + gave.len() * 8);
    let mut died = Vec::new();
    sync::read_delta(b, &mut In::new(&out), &mut died).unwrap();
    assert_eq!(died, [12]);
    assert!(a.parts.iter().zip(&b.parts).all(|(p, q)| (p.alive, p.working, p.hp) == (q.alive, q.working, q.hp)));
    assert!(a.joints.iter().zip(&b.joints).all(|(p, q)| (p.alive, p.hp) == (q.alive, q.hp)));
    assert!((a.mass - b.mass).abs() < 1e-3, "{} y {} kg", a.mass, b.mass);
    // told once
    out.clear();
    assert!(!shadow.delta(a, &mut out));
    // a wear too small to tell is not told; one worth telling is
    a.parts[50].hp *= 0.999;
    assert!(!shadow.delta(a, &mut out));
    a.parts[50].hp *= 0.9;
    assert!(shadow.delta(a, &mut out));
    died.clear();
    sync::read_delta(b, &mut In::new(&out), &mut died).unwrap();
    assert!(died.is_empty() && a.parts[50].hp == b.parts[50].hp);
    // mended and put back
    let cat = &lib.catalog;
    a.mend(cat, 10, 1e9);
    assert!(a.rebuild(cat, 12, 0.2));
    out.clear();
    assert!(shadow.delta(a, &mut out));
    sync::read_delta(b, &mut In::new(&out), &mut died).unwrap();
    assert!(b.parts[12].alive && b.parts[10].hp == b.parts[10].max_hp && died.is_empty());
    assert!(a.joints.iter().zip(&b.joints).all(|(p, q)| p.alive == q.alive));
    // a delta of another structure is refused whole
    let other = ships.get("abejorro").unwrap();
    let (mut so, _, _) = game(&lib, other);
    assert_eq!(sync::read_delta(&mut so.list[0], &mut In::new(&out), &mut died), Err(SyncError::Count("piezas")));
    // a shadow that knows nothing tells everything
    let mut all = Vec::new();
    assert!(Shadow::unknown().delta(a, &mut all) && all.len() > a.parts.len());
}

#[test]
fn a_piece_come_off_a_ship_is_made_in_a_game_that_never_had_it() {
    let (lib, ships, _) = library();
    let kind = ships.get("alcotan").unwrap();
    let (mut set, mut ship, id) = game(&lib, kind);
    // every clamp let go: its cargo comes off as structures of their own
    for c in &kind.clamps {
        ship.set_signal(&format!("{}.soltar", c.id), 1.0);
    }
    ship.update(&mut set.list[0], &w(), TICK);
    let came = set.separate();
    assert!(came >= 3, "{came} bultos sueltos");
    // the ship itself is still its whole blueprint (what left is not missed), told in a few bytes
    let mut make = Vec::new();
    sync::write_make(set.get(id).unwrap(), &lib, &mut make).unwrap();
    assert_eq!(make.len(), 1 + 1 + kind.blueprint.len());
    assert!(sync::made_as(set.get(id).unwrap(), &lib, &make));
    let mut elsewhere = Structures::new(lib.clone());
    for piece in set.list.iter().skip(1) {
        let (mut make, mut state) = (Vec::new(), Vec::new());
        sync::write_make(piece, &lib, &mut make).unwrap();
        sync::write_state(piece, &mut state);
        assert!(sync::made_as(piece, &lib, &make) && !sync::made_as(set.get(id).unwrap(), &lib, &make));
        let new = elsewhere.next_id();
        let mut made = sync::make(&lib, new, piece.pos, piece.rot, &mut In::new(&make)).unwrap_or_else(|e| panic!("{}: {e:?}", piece.name));
        sync::read_state(&mut made, &mut In::new(&state), &mut Vec::new()).unwrap();
        assert_eq!((made.parts.len(), made.joints.len(), made.stored.len()), (piece.parts.len(), piece.joints.len(), piece.stored.len()), "{}", piece.label(&lib.catalog));
        assert!((made.mass - piece.mass).abs() < 1e-2 && (made.com - piece.com).length() < 1e-3 && (made.radius - piece.radius).abs() < 1e-3, "{}: {} y {} kg", piece.label(&lib.catalog), made.mass, piece.mass);
        assert!(made.parts.iter().zip(&piece.parts).all(|(p, q)| p.kind == q.kind && p.origin == q.origin && p.local == q.local && p.bone == 0));
        assert_eq!(made.cargo_card(&lib.catalog), piece.cargo_card(&lib.catalog));
        assert!(!made.anchored && made.owner.is_none() && made.model == piece.model);
        // what it says of itself is what was said of the other
        let mut again = Vec::new();
        sync::write_make(&made, &lib, &mut again).unwrap();
        assert!(again == make);
        eprintln!("{}: {} piezas, {:.0} kg: {} bytes para hacerlo y {} de estado", piece.label(&lib.catalog), piece.parts.len(), piece.mass, make.len(), state.len());
        elsewhere.list.push(made);
    }
    // the whole ship made from what is said of it, in a game that has none
    let (mut make, mut state) = (Vec::new(), Vec::new());
    let s = set.get(id).unwrap();
    sync::write_make(s, &lib, &mut make).unwrap();
    sync::write_state(s, &mut state);
    let new = elsewhere.next_id();
    let mut made = sync::make(&lib, new, s.pos, s.rot, &mut In::new(&make)).unwrap();
    let mut died = Vec::new();
    sync::read_state(&mut made, &mut In::new(&state), &mut died).unwrap();
    assert!((made.mass - s.mass).abs() < 0.5, "la nave sin su carga: {} y {} kg", made.mass, s.mass);
    assert_eq!(died.len(), s.parts.iter().filter(|p| !p.alive).count(), "lo que se fue con la carga");
    assert!(made.parts.iter().zip(&s.parts).all(|(p, q)| (p.alive, p.left) == (q.alive, q.left)));
    // what is of no blueprint cannot be told
    let mut odd = Vec::new();
    let mut nameless = sync::make(&lib, 99, DVec3::ZERO, Quat::IDENTITY, &mut In::new(&make)).unwrap();
    nameless.name = "nada".into();
    assert!(matches!(sync::write_make(&nameless, &lib, &mut odd), Err(SyncError::Unknown(_))));
    assert!(matches!(sync::make(&lib, 1, DVec3::ZERO, Quat::IDENTITY, &mut In::new(&[0, 4, b'n', b'a', b'd', b'a'])), Err(SyncError::Unknown(_))));
}

#[test]
fn bytes_cut_or_flipped_never_panic() {
    let (lib, ships, _) = library();
    let kind = ships.get("abejorro").unwrap();
    let (mut sa, mut a, _) = game(&lib, kind);
    lived(&mut a, &mut sa.list[0]);
    let whole = snapshot(&a, &sa.list[0]);
    let (mut make, mut delta) = (Vec::new(), Vec::new());
    sync::write_make(&sa.list[0], &lib, &mut make).unwrap();
    Shadow::unknown().delta(&sa.list[0], &mut delta);
    let (mut sb, mut b, _) = game(&lib, kind);
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut dice = move || {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        seed.wrapping_mul(0x2545_f491_4f6c_dd1d)
    };
    let mut died = Vec::new();
    let mut feed = |bytes: &[u8], b: &mut Ship, s: &mut Structure| {
        let _ = restore_quiet(b, s, bytes);
        let _ = sync::read_delta(s, &mut In::new(bytes), &mut died);
        let _ = sync::make(&lib, 5, DVec3::ZERO, Quat::IDENTITY, &mut In::new(bytes));
        let _ = Digest::read(&mut In::new(bytes));
    };
    for sample in [&whole, &make, &delta] {
        for cut in (0..sample.len()).step_by((sample.len() / 300).max(1)) {
            feed(&sample[..cut], &mut b, &mut sb.list[0]);
        }
        for _ in 0..400 {
            let mut flipped = sample.clone();
            for _ in 0..1 + dice() % 4 {
                let at = (dice() % flipped.len() as u64) as usize;
                flipped[at] ^= 1 << (dice() % 8);
            }
            feed(&flipped, &mut b, &mut sb.list[0]);
        }
    }
    for _ in 0..3000 {
        let n = (dice() % 200) as usize;
        let noise: Vec<u8> = (0..n).map(|_| dice() as u8).collect();
        feed(&noise, &mut b, &mut sb.list[0]);
    }
    // and after all that, told the truth, it is the truth again
    restore(&mut b, &mut sb.list[0], &whole).unwrap();
    assert!(snapshot(&b, &sb.list[0]) == whole, "dicha la verdad, vuelve a ser la verdad");
    // a digest goes and comes back
    let d = Digest::of(Some(&a), &sa.list[0]);
    let mut bytes = Vec::new();
    d.write(&mut bytes);
    assert_eq!((bytes.len(), Digest::read(&mut In::new(&bytes))), (sync::DIGEST_BYTES, Ok(d)));
}

fn restore_quiet(ship: &mut Ship, s: &mut Structure, bytes: &[u8]) -> Result<(), SyncError> {
    let mut inp = In::new(bytes);
    sync::read_state(s, &mut inp, &mut Vec::new())?;
    sync::read_ship(ship, s, &|id| Some(id), &mut inp)
}
