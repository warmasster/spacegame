//! Coherence of every ship that flies, as a pilot finds it: each control and each key of its seats
//! does what it says it does, the same at any frame rate, and the ship answers each of them on
//! the axis and the way it says and on no other. What fails here is something a pilot would call
//! «no va» or «hace cosas raras». Nothing is named here: a new ship is checked the day it is
//! dropped in `assets/defs/ships`.
//!
//! - **keys held** (`seat_keys`): a key held on a control that turns (the throttle) moves it
//!   steadily out of any detent, as far in a second at 30 frames a second as at 240; `cero` takes
//!   it to its least;
//! - **wheels and levers turned**: one notch is the step the wheel's data and its help give — with
//!   Shift the coarse one, with Ctrl the fine one, which no speed of turning multiplies — and one
//!   notch takes a lever out of a detent it stands in;
//! - **the stick and the translation keys**, flown past every body's reach with the stabiliser
//!   on: each key turns or pushes the ship about one axis only, the way its help says, and the
//!   opposite key the other way; let go, the stabiliser stops it; the same key does the same on
//!   every ship;
//! - **power without a turn**: with the throttle open and the stick let go, in every position of
//!   engines that swing, the ship does not turn by itself, its thrusters fire when they must and
//!   its momentum wheels do not stay full;
//! - **the autopilot where it can do nothing**: past every body's reach, a hold or a program that
//!   needs a ground under it says so (`ap.sin_cuerpo`) instead of showing itself at work.
//!
//! Run with `--nocapture` for the table of what every key does on every ship.
mod comun;

use comun::{
    report,
    vuelo::{Flight, flown, flyers},
};
use glam::Vec3;
use lunar_controls::{Intent, Mods};
use lunar_ship::{
    ShipKind,
    seat_keys::{self, Does},
};
use lunar_signals::Q;
use std::sync::Arc;

// ---------------------------------------------------------------- helpers

/// The value of control `k` now.
fn value(f: &Flight, k: usize) -> f64 {
    let c = &f.ship.panels.controls[k];
    c.mech.value(&c.st)
}

/// The least and the most control `k` goes to.
fn range(f: &mut Flight, k: usize) -> (f64, f64) {
    let was = value(f, k);
    f.intent(k, &Intent::Set { value: -1.0e12 });
    let lo = value(f, k);
    f.intent(k, &Intent::Set { value: 1.0e12 });
    let hi = value(f, k);
    f.intent(k, &Intent::Set { value: was });
    (lo, hi)
}

fn def(f: &Flight, k: usize) -> &lunar_controls::ControlDef {
    let c = &f.ship.panels.controls[k];
    &f.kind.panels[c.panel].def.mandos[c.index]
}

/// A step as the data writes it, in SI (a difference: "0.5 °C" is half a kelvin) and as written.
fn step_of(q: &Option<Q>) -> Option<(f64, f64)> {
    match q.as_ref()? {
        Q::N(x) => Some((*x, *x)),
        Q::S(t) => {
            let written = t.trim().split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).next()?.parse().ok()?;
            lunar_signals::units::parse(t).ok().map(|(x, u)| (x - u.offset, written))
        }
    }
}

/// The number written before `tag` in `text` ("10 m por muesca": 10 before "por muesca"), or
/// after it ("Mayús 100 m": 100 after "Mayús").
fn number_near(text: &str, tag: &str, before: bool) -> Option<f64> {
    let at = text.find(tag)?;
    let s = if before { &text[..at] } else { &text[at + tag.len()..] };
    let words: Vec<&str> = s.split(|c: char| c.is_whitespace() || c == ':' || c == ';').filter(|w| !w.is_empty()).collect();
    // (a decimal comma is a point; a comma after a word is not part of it)
    let pick = |w: &&str| w.trim_end_matches([',', '.']).replace(',', ".").parse::<f64>().ok();
    if before { words.iter().rev().take(3).find_map(pick) } else { words.iter().take(2).find_map(pick) }
}

/// Its seat keys, each with the ship it is on.
fn seats(f: &Flight) -> Vec<(usize, Vec<seat_keys::SeatKey>, Vec<String>)> {
    (0..f.kind.seats.len())
        .map(|i| {
            let (k, bad) = seat_keys::keys(&f.ship, i);
            (i, k, bad)
        })
        .collect()
}

/// A ship of `kind` with its engines running and its stabiliser on: on its gear, or (`space`)
/// still, past every body's reach.
fn running(kind: &Arc<ShipKind>, space: bool) -> Flight {
    let mut f = if space { Flight::in_space(kind) } else { Flight::new(kind) };
    f.start();
    f
}

fn axis_name(k: usize) -> &'static str {
    ["x (babor)", "y (arriba)", "z (proa)"][k]
}

// ---------------------------------------------------------------- keys held

#[test]
fn a_key_held_moves_its_control_steadily_at_any_frame_rate() {
    let mut bad = Vec::new();
    for kind in flyers() {
        let mut f = running(&kind, false);
        for (seat, keys, wrong) in seats(&f) {
            bad.extend(wrong.into_iter().map(|w| format!("{}: {w}", kind.id)));
            for key in keys {
                let (k, name) = (key.control, f.ship.panels.controls[key.control].id.clone());
                let (lo, hi) = range(&mut f, k);
                let span = hi - lo;
                let sign = match key.does {
                    Does::Up => 1.0,
                    Does::Down => -1.0,
                    Does::Zero => {
                        f.intent(k, &Intent::Set { value: hi });
                        f.intent(k, &Intent::Set { value: lo });
                        if (value(&f, k) - lo).abs() > 1e-6 {
                            bad.push(format!("{} asiento {seat}: '{}' no lleva {name} a su mínimo", kind.id, key.key));
                        }
                        continue;
                    }
                    _ => continue,
                };
                // a second of the key held, from the end it moves away from, at several frame rates
                let mut moved = Vec::new();
                for fps in [30.0f32, 60.0, 144.0, 240.0] {
                    f.intent(k, &Intent::Set { value: if sign > 0.0 { lo } else { hi } });
                    let from = value(&f, k);
                    for _ in 0..fps as usize {
                        f.intent(k, &seat_keys::held(sign, 1.0 / fps));
                    }
                    moved.push((fps, (value(&f, k) - from) * sign as f64 / span));
                }
                let most = moved.iter().map(|m| m.1).fold(f64::MIN, f64::max);
                let least = moved.iter().map(|m| m.1).fold(f64::MAX, f64::min);
                if least < 0.05 {
                    bad.push(format!("{} asiento {seat}: '{}' mantenida 1 s apenas mueve {name}: {}", kind.id, key.key, moved.iter().map(|(fps, m)| format!("{:.0} % a {fps:.0} fps", m * 100.0)).collect::<Vec<_>>().join(", ")));
                } else if most - least > 0.1 * most {
                    bad.push(format!("{} asiento {seat}: '{}' mueve {name} distinto según los fotogramas: {}", kind.id, key.key, moved.iter().map(|(fps, m)| format!("{:.0} % a {fps:.0} fps", m * 100.0)).collect::<Vec<_>>().join(", ")));
                }
                f.intent(k, &Intent::Set { value: lo });
            }
        }
    }
    report("teclas mantenidas", &bad);
}

// ---------------------------------------------------------------- wheels and levers

#[test]
fn one_notch_is_the_step_a_wheel_says_and_takes_a_lever_out_of_its_detent() {
    let mut bad = Vec::new();
    for kind in flyers() {
        let mut f = running(&kind, false);
        for k in 0..f.ship.panels.controls.len() {
            let name = f.ship.panels.controls[k].id.clone();
            let mech = f.ship.panels.controls[k].mech.kind();
            let d = def(&f, k).clone();
            let (lo, hi) = range(&mut f, k);
            let turn = |f: &mut Flight, from: f64, notches: f32, rate: f32, m: Mods| -> Option<f64> {
                f.intent(k, &Intent::Set { value: from });
                let at = value(f, k);
                let o = f.intent(k, &Intent::Turn { notches, rate, m });
                (!matches!(o.event, Some(lunar_controls::Event::Blocked(_)))).then(|| value(f, k) - at)
            };
            match mech {
                "rueda" if d.curva.as_deref() != Some("log") && hi > lo => {
                    let mid = lo + (hi - lo) * 0.37;
                    let (coarse, fine) = (Mods { coarse: true, fine: false }, Mods { coarse: false, fine: true });
                    let (Some(plain), Some(big), Some(small), Some(small_fast), Some(plain_fast)) =
                        (turn(&mut f, mid, 1.0, 1.0, Mods::default()), turn(&mut f, mid, 1.0, 1.0, coarse), turn(&mut f, mid, 1.0, 1.0, fine), turn(&mut f, mid, 1.0, 12.0, fine), turn(&mut f, mid, 1.0, 12.0, Mods::default()))
                    else {
                        continue;
                    };
                    let said = |q: &Option<Q>| step_of(q).map(|s| s.0);
                    let near = |a: f64, b: f64| (a - b).abs() <= 1e-6 + 0.02 * b.abs();
                    for (what, got, want) in [("una muesca", plain, said(&d.paso)), ("con Mayús", big, said(&d.paso_grueso)), ("con Ctrl", small, said(&d.paso_fino))] {
                        if let Some(w) = want
                            && !near(got, w)
                        {
                            bad.push(format!("{} {name}: {what} mueve {got} y sus datos dicen {w}", kind.id));
                        }
                    }
                    if !near(small_fast, small) {
                        bad.push(format!("{} {name}: con Ctrl, una muesca deprisa mueve {small_fast} y despacio {small}: Ctrl debe ser siempre el paso fino", kind.id));
                    }
                    if plain_fast < plain - 1e-9 {
                        bad.push(format!("{} {name}: girada deprisa va menos que despacio ({plain_fast} y {plain})", kind.id));
                    }
                    if !(small <= plain + 1e-9 && plain <= big + 1e-9) {
                        bad.push(format!("{} {name}: los pasos no van de fino a grueso ({small}, {plain}, {big})", kind.id));
                    }
                    // what its help says of them, in its own units
                    if let Some(help) = d.ayuda.as_deref() {
                        for (tag, before, q) in [("por muesca", true, &d.paso), ("Mayús", false, &d.paso_grueso), ("Ctrl", false, &d.paso_fino)] {
                            if let (Some(n), Some((_, written))) = (number_near(help, tag, before), step_of(q))
                                && (n - written).abs() > 1e-6
                            {
                                bad.push(format!("{} {name}: su ayuda dice {n} {tag} y sus datos {written}", kind.id));
                            }
                        }
                    }
                }
                "palanca" => {
                    for r in &d.retenes {
                        let Ok(at) = r.en.si() else { continue };
                        for sign in [1.0f32, -1.0] {
                            if (sign > 0.0 && at >= hi - 1e-9) || (sign < 0.0 && at <= lo + 1e-9) {
                                continue;
                            }
                            if let Some(m) = turn(&mut f, at, sign, 1.0, Mods::default())
                                && m.abs() < 1e-9
                            {
                                bad.push(format!("{} {name}: una muesca {} no la saca del retén en {at}", kind.id, if sign > 0.0 { "arriba" } else { "abajo" }));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    report("ruedas y palancas", &bad);
}

// ---------------------------------------------------------------- the stick and the keys

/// What holding a seat's axis key did to the ship: the turn or the push it gained (ship frame).
struct Answer {
    turn: Vec3,
    push: Vec3,
}

/// What the help of a key says it moves: (turn, axis) — x pitch, y yaw, z roll; or a push along
/// x (to the side), y (up and down), z (fore and aft).
fn said(help: &str) -> Option<(bool, usize)> {
    let h = help.to_lowercase();
    [("cabeceo", (true, 0)), ("guiñada", (true, 1)), ("alabeo", (true, 2)), ("subir y bajar", (false, 1)), ("adelante y atrás", (false, 2)), ("a los lados", (false, 0))].into_iter().find(|(w, _)| h.contains(w)).map(|(_, a)| a)
}

#[test]
fn each_key_of_the_stick_moves_the_ship_about_one_axis_the_way_it_says() {
    let mut bad = Vec::new();
    // per key, on every ship: (ship, turn or push, axis, sign)
    let mut by_key: Vec<(String, String, bool, usize, f32)> = Vec::new();
    for kind in flyers() {
        let base = running(&kind, true);
        for (seat, keys, _) in seats(&base) {
            for key in &keys {
                let Does::Axis { axis, value } = key.does else { continue };
                let help = kind.seats[seat].def.mandos.iter().find(|b| b.tecla == key.key).and_then(|b| b.ayuda.clone()).unwrap_or_default();
                // a fresh ship for every key, its engines running: how it goes on its own (its
                // engines at idle push a little), and then with the key held as long. (A key
                // whose help says it works with a mode on — «con MANTENER» — with that mode.)
                let mut f = running(&kind, true);
                if help.contains("MANTENER")
                    && let Some(hold) = kind.def.vuelo.as_ref().and_then(|v| v.mantener.clone())
                {
                    f.set(&hold, 1.0);
                }
                f.fly(2.0);
                let (spin0, vel0) = (f.spin(), f.vel());
                f.fly(1.5);
                let (own_turn, own_push) = (f.spin() - spin0, f.vel() - vel0);
                let (spin0, vel0) = (f.spin(), f.vel());
                f.intent(key.control, &Intent::Axis { axis, value });
                f.fly(1.5);
                let a = Answer { turn: f.spin() - spin0 - own_turn, push: f.vel() - vel0 - own_push };
                // let go: the stabiliser stops it (as soon as the thrusters can: a big ship takes
                // longer)
                f.intent(key.control, &Intent::Axis { axis, value: 0.0 });
                let mut left = f.spin().length();
                for _ in 0..40 {
                    f.fly(0.5);
                    left = f.spin().length();
                    if left < 0.005 {
                        break;
                    }
                }
                let (t, p) = (a.turn.abs(), a.push.abs());
                let turning = t.max_element() > 0.02;
                let pushing = p.max_element() > 0.1;
                let what = format!("{} asiento {seat}: '{}' ({help})", kind.id, key.key);
                if !turning && !pushing {
                    bad.push(format!("{what}: la nave no hace nada (giro {:.3} rad/s, velocidad {:.2} m/s)", t.max_element(), p.max_element()));
                    continue;
                }
                // a turn wins over the push it makes as it turns (thrusters off centre)
                let (is_turn, v) = if turning { (true, a.turn) } else { (false, a.push) };
                let ax = v.abs().max_position();
                let main = v[ax];
                let others = (0..3).filter(|&i| i != ax).map(|i| v[i].abs()).fold(0.0f32, f32::max);
                if others > 0.25 * main.abs() {
                    bad.push(format!("{what}: {} sobre todo {} ({main:+.3}) pero también por los otros ejes ({:+.3}, {:+.3}, {:+.3})", if is_turn { "gira" } else { "empuja" }, axis_name(ax), v.x, v.y, v.z));
                }
                match said(&help) {
                    Some((turn, want)) if turn != is_turn || want != ax => {
                        bad.push(format!("{what}: su ayuda dice {} {} y la nave {} {}", if turn { "girar en" } else { "empujar por" }, axis_name(want), if is_turn { "gira en" } else { "va por" }, axis_name(ax)))
                    }
                    None => bad.push(format!("{what}: su ayuda no dice qué eje mueve")),
                    _ => {}
                }
                if is_turn && left > 0.01 {
                    bad.push(format!("{what}: soltada, el estabilizador no la para (sigue girando a {left:.3} rad/s)"));
                }
                by_key.push((key.key.clone(), kind.id.clone(), is_turn, ax, main.signum()));
            }
        }
    }
    // the same key, the same thing, on every ship
    by_key.sort_by(|a, b| a.0.cmp(&b.0));
    eprintln!("tecla: lo que hace en cada nave (giro o empuje, eje, sentido)");
    for chunk in by_key.chunk_by(|a, b| a.0 == b.0) {
        let line: Vec<String> = chunk.iter().map(|(_, ship, turn, ax, s)| format!("{ship}: {} {} {}", if *turn { "gira" } else { "empuja" }, axis_name(*ax), if *s > 0.0 { "+" } else { "−" })).collect();
        eprintln!("  {}: {}", chunk[0].0, line.join(" · "));
        if chunk.iter().any(|c| (c.2, c.3, c.4) != (chunk[0].2, chunk[0].3, chunk[0].4)) {
            bad.push(format!("la tecla '{}' no hace lo mismo en todas las naves: {}", chunk[0].0, line.join(" · ")));
        }
    }
    report("palanca y teclas del asiento", &bad);
}

// ---------------------------------------------------------------- power without a turn

/// The control that writes signal `name`, if a hand works it.
fn writer(f: &Flight, name: &str) -> Option<usize> {
    comun::vuelo::writer(&f.ship, name)
}

/// The levers a pilot swings the engines with: those whose signal the order of the actuator of
/// an engine's hinge is worked out from (straight, or through the ship's derived logic).
fn engine_levers(f: &Flight) -> Vec<usize> {
    let kind = &f.kind;
    let mut out = Vec::new();
    for &m in &flown(kind, "motores") {
        let Some(p) = kind.machines[m].part else { continue };
        for (ji, _) in kind.joints.iter().enumerate().filter(|(_, j)| j.parts.contains(&p)) {
            for a in kind.actuators.iter().filter(|a| a.joint == ji) {
                let Some(order) = a.def.control.orden.as_deref() else { continue };
                // the order, and whatever its derivation reads
                let mut names = vec![order.to_string()];
                if let Some(expr) = kind.def.derivadas.get(order) {
                    names.extend(expr.split(|c: char| !(c.is_alphanumeric() || c == '.' || c == '_')).filter(|w| w.contains('.')).map(str::to_string));
                }
                for n in names {
                    if let Some(k) = writer(f, &n).filter(|&k| f.ship.panels.controls[k].mech.kind() == "palanca")
                        && !out.contains(&k)
                    {
                        out.push(k);
                    }
                }
            }
        }
    }
    out
}

#[test]
fn with_the_throttle_open_and_the_stick_let_go_it_does_not_turn_by_itself() {
    let mut bad = Vec::new();
    for kind in flyers() {
        let Some(throttle) = kind.def.vuelo.as_ref().and_then(|v| v.acelerador.clone()) else { continue };
        let probe = running(&kind, true);
        let Some(tk) = writer(&probe, &throttle) else {
            bad.push(format!("{}: ningún mando escribe el acelerador {throttle}", kind.id));
            continue;
        };
        // every position the pilot can put the engines in (the detents of the lever that swings
        // them), or as they stand
        let levers = engine_levers(&probe);
        let mut positions: Vec<Option<(usize, f64, String)>> = vec![None];
        for &l in &levers {
            positions = def(&probe, l).retenes.iter().filter_map(|r| Some(Some((l, r.en.si().ok()?, r.nombre.clone().unwrap_or_default())))).collect();
        }
        for pos in positions {
            let mut f = running(&kind, true);
            // (wheels unloaded by a switch, as their help says: on)
            if let Some(unload) = kind.def.vuelo.as_ref().and_then(|v| v.descarga.clone())
                && comun::vuelo::writer(&f.ship, &unload).is_some()
            {
                f.set(&unload, 1.0);
            }
            let at = match &pos {
                Some((l, v, name)) => {
                    f.intent(*l, &Intent::Set { value: *v });
                    f.fly(8.0);
                    format!("góndolas en {name}")
                }
                None => "motores como están".to_string(),
            };
            f.fly(1.0);
            let rot0 = f.s().rot;
            f.intent(tk, &Intent::Set { value: 0.6 });
            let (mut fired, mut fullest, mut fastest) = (0.0f64, 0.0f64, 0.0f32);
            for _ in 0..30 {
                f.fly(1.0);
                fired = fired.max(f.rcs_thrust());
                fullest = fullest.max(f.wheels_full());
                fastest = fastest.max(f.spin().length());
            }
            let turned = (f.s().rot * rot0.inverse()).to_axis_angle().1.to_degrees();
            let turned = if turned > 180.0 { 360.0 - turned } else { turned };
            let full_now = f.wheels_full();
            let what = format!("{} ({at}, acelerador al 60 %, estabilizador puesto, 30 s)", kind.id);
            eprintln!("{what}: gira {turned:.2}°, a {fastest:.4} rad/s como mucho; giróscopos al {:.0} % (al final {:.0} %); toberas hasta {fired:.0} N; empuje {:.0} N", fullest * 100.0, full_now * 100.0, f.thrust());
            if f.thrust() < 1.0 {
                bad.push(format!("{what}: los motores no empujan"));
                continue;
            }
            if turned > 2.0 || fastest > 0.02 {
                bad.push(format!("{what}: se gira sola {turned:.1}° (hasta {fastest:.3} rad/s)"));
            }
            if full_now > 0.6 {
                bad.push(format!("{what}: los giróscopos se quedan llenos ({:.0} %)", full_now * 100.0));
            }
            if fullest > 0.5 && fired < 1.0 {
                bad.push(format!("{what}: con los giróscopos llenos las toberas no se encienden"));
            }
        }
    }
    report("potencia sin giro", &bad);
}

// ---------------------------------------------------------------- the autopilot where it can do nothing

#[test]
fn past_every_body_the_autopilot_says_what_it_cannot_do() {
    let mut bad = Vec::new();
    for kind in flyers() {
        let mut f = running(&kind, true);
        if f.ship.signal("ap.nav").is_none() || comun::vuelo::writer(&f.ship, "ap.altura").is_none() {
            continue;
        }
        let Some(_) = f.ship.signal("ap.sin_cuerpo") else {
            bad.push(format!("{}: el piloto automático no tiene manera de decir que aquí no puede (ap.sin_cuerpo)", kind.id));
            continue;
        };
        // a hold of height, a program that climbs, one that lands: none means anything here
        let cases: [(&str, &[(&str, f64)]); 3] = [("ALTURA retenida", &[("ap.altura", 1.0)]), ("DESPEG.", &[("ap.nav", comun::vuelo::program("despegar"))]), ("ATERRIZ.", &[("ap.nav", comun::vuelo::program("aterrizar"))])];
        for (what, sets) in cases {
            for (n, v) in sets {
                f.set(n, *v);
            }
            f.fly(1.0);
            let (on, none) = (f.ship.signal("ap.activo").unwrap_or(0.0), f.ship.signal("ap.sin_cuerpo").unwrap_or(0.0));
            if on > 0.0 && none < 0.5 {
                bad.push(format!("{}: {what} fuera de todo cuerpo se enciende como si volara y no dice que no puede", kind.id));
            }
            for (n, _) in sets {
                f.set(n, 0.0);
            }
            f.fly(0.5);
        }
        // and a speed held does fly there, without saying it cannot
        f.set("ap.velocidad_sel", 20.0);
        f.set("ap.velocidad", 1.0);
        f.fly(1.0);
        if f.ship.signal("ap.sin_cuerpo").unwrap_or(0.0) > 0.5 {
            bad.push(format!("{}: VELOC. fuera de todo cuerpo dice que no puede, y puede", kind.id));
        }
    }
    report("piloto automático sin cuerpo", &bad);
}
