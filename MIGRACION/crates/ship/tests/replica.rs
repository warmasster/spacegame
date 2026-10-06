//! What a hand does to a control can be told to another copy of the ship as "this control is now
//! at this value" (`Intent::Set`), and that copy ends with its control where the first has it:
//! what multiplayer tells of a control worked by another player. For every control of every ship.
use glam::{DQuat, DVec3};
use lunar_controls::{Intent, Mods};
use lunar_core::{
    structure::{Library, state::Structure},
};
use lunar_ship::{Ship, ShipLibrary, World};
use std::path::Path;

fn spawn(name: &str) -> (Structure, Ship) {
    let defs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs");
    let mut lib = Library::load(&defs.join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get(name).unwrap().clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, DQuat::IDENTITY.as_quat());
    let mut ship = Ship::new(kind, 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &World::default(), 0.0);
    (s, ship)
}

fn value(ship: &Ship, k: usize) -> f64 {
    let c = &ship.panels.controls[k];
    c.mech.value(&c.st)
}

fn act(ship: &mut Ship, s: &Structure, k: usize, i: &Intent) -> bool {
    let kind = ship.kind.clone();
    ship.panels.intent(k, i, s, &kind, &ship.store).changed
}

#[test]
fn a_control_worked_on_one_copy_is_set_the_same_on_another() {
    for name in ["alcotan", "abejorro", "cachalote"] {
        let (sa, mut a) = spawn(name);
        let (sb, mut b) = spawn(name);
        let n = a.panels.controls.len();
        assert_eq!(n, b.panels.controls.len());
        let mut told = vec![0u32; n];
        // (twice round: the first opens the covers, the second works what they guard)
        for _ in 0..2 {
            for k in 0..n {
                let ways: [&[Intent]; 3] = [&[Intent::Press { elem: 0 }, Intent::Release], &[Intent::Turn { notches: 1.0, rate: 10.0, m: Mods::default() }], &[Intent::Turn { notches: -1.0, rate: 10.0, m: Mods::default() }]];
                for way in ways {
                    let before = value(&a, k);
                    for i in way {
                        act(&mut a, &sa, k, i);
                        // each change told as it happens, as the game does
                        let now = value(&a, k);
                        if (now - value(&b, k)).abs() > 1e-9 {
                            act(&mut b, &sb, k, &Intent::Set { value: now });
                            let got = value(&b, k);
                            assert!((got - now).abs() < 1e-6, "{name}, mando {}: en una copia vale {now}, dicho a la otra queda en {got}", a.panels.controls[k].id);
                            told[k] += 1;
                        }
                    }
                    if (value(&a, k) - before).abs() > 1e-9 {
                        break;
                    }
                }
            }
        }
        let worked = told.iter().filter(|t| **t > 0).count();
        eprintln!("{name}: {worked} de {n} mandos accionados y repetidos en la otra copia");
        assert!(worked * 10 >= n * 7, "{name}: solo {worked} de {n} mandos se pudieron accionar");
    }
}
