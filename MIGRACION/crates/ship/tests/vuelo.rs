//! Flying, every ship of the library the same way and under more than one gravity (nothing here
//! is the Moon's alone):
//! - its engines started and its throttle not touched, it stays where it stands: at idle they
//!   give next to nothing, and what they give goes through its centre of mass (it neither
//!   creeps forward nor rocks on its legs); the throttle part of the way up, short of its
//!   weight, the same;
//! - MANTENER lifts it off level and hangs it still: it stands on its engines (the computer
//!   turns it upright against gravity and trims them through the centre of mass), so the
//!   thrusters barely fire;
//! - cargo held to one side moves its centre of mass by what a sum by hand says, and it still
//!   hangs level.
//!
//! Run with `--nocapture` for the table.
use glam::{Affine3A, DVec3, Quat, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, convex::Convex, set::Structures, state::{Part, Structure}},
};
use lunar_ship::{Ship, ShipKind, Sources, World, def::ShipDef, ship::TICK};
use std::{path::Path, sync::Arc};

/// The gravities it is flown under (m/s²): a small moon, between, the Moon, and heavier (where a
/// ship's engines do not lift it with a margin it is left out and said so).
const GRAVITIES: [f64; 4] = [0.21, 0.8, 1.62, 2.2];
/// A ship is flown where its engines at full give this many times its weight.
const MARGIN: f64 = 1.2;

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn built() -> (Arc<Library>, Vec<Arc<ShipKind>>) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let src = Sources::load(&defs()).unwrap_or_else(|e| panic!("{e}"));
    let ships: Vec<(String, ShipDef)> = lunar_core::defs::load_dir(&defs().join("ships")).unwrap_or_else(|e| panic!("{e}"));
    let mut kinds = Vec::new();
    for (id, d) in ships {
        let (k, bp) = src.build(&id, d, &mut lib.catalog).unwrap_or_else(|e| panic!("{e}"));
        kinds.push(Arc::new(k));
        lib.blueprints.push((id, bp));
    }
    (Arc::new(lib), kinds)
}

/// A world of gravity `g`: the first body of the registry with that gravity, its ground levelled
/// where the ship stands (`up`).
fn world(g: f64) -> (BodyRegistry, DVec3) {
    let all: Vec<(String, BodyDef)> = lunar_core::defs::load_dir(&defs().join("bodies")).unwrap_or_else(|e| panic!("{e}"));
    let (id, mut d) = all.into_iter().next().unwrap();
    let up = DVec3::new(0.3, 0.9, -0.2).normalize();
    d.center = [0.0; 3];
    d.gravity = g;
    d.level_at = Some(up.to_array());
    (BodyRegistry::new(vec![Body::from_def(&id, &d).unwrap()]), up)
}

fn world_at(s: &Structure, bodies: &BodyRegistry) -> World {
    World::at(s, bodies, s.to_world(s.com))
}

fn signal(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id))
}

fn control(ship: &Ship, name: &str) -> usize {
    let sig = ship.store.find(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id));
    ship.panels.controls.iter().position(|c| c.sig == sig).unwrap_or_else(|| panic!("{}: ningún mando escribe {name}", ship.kind.id))
}

fn set(ship: &mut Ship, s: &Structure, name: &str, value: f64) {
    let (k, kind) = (control(ship, name), ship.kind.clone());
    ship.panels.intent(k, &Intent::Set { value }, s, &kind, &ship.store);
}

/// A sprung lever held at `value`, as a key holds it (0: let go).
fn hold(ship: &mut Ship, s: &Structure, name: &str, value: f64) {
    let (k, kind) = (control(ship, name), ship.kind.clone());
    ship.panels.intent(k, &Intent::Axis { axis: 0, value }, s, &kind, &ship.store);
}

fn engines(kind: &ShipKind) -> Vec<usize> {
    let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
    kind.def.vuelo.iter().flat_map(|f| &f.motores).flat_map(|p| lunar_ship::kind::resolve(&ids, p)).map(|m| m as usize).collect()
}

fn order(kind: &ShipKind, m: usize, role: &str) -> String {
    kind.machines[m].def.ordenes.get(role).and_then(|v| v.as_str()).map_or_else(|| format!("{}.{role}", kind.machines[m].id), str::to_string)
}

/// A ship standing on the ground of a world of gravity `g`, with whatever else is in `set`.
struct Flight {
    set: Structures,
    bodies: BodyRegistry,
    ship: Ship,
    kind: Arc<ShipKind>,
}

impl Flight {
    fn new(lib: &Arc<Library>, kind: &Arc<ShipKind>, g: f64) -> Flight {
        let (bodies, up) = world(g);
        let mut set = Structures::new(lib.clone());
        let id = set.place(&kind.blueprint, &bodies, 0, up, 0.0, f64::from(kind.lift)).unwrap();
        let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{}: {e}", kind.id));
        let w = world_at(&set.list[0], &bodies);
        ship.update(&mut set.list[0], &w, 0.0);
        set.rest_on_ground(id, &bodies);
        let sink = ship.rest_on_legs(&mut set.list[0], g as f32);
        let up = bodies.get(0).up(set.list[0].pos);
        set.list[0].pos -= up * f64::from(sink);
        Flight { set, bodies, ship, kind: kind.clone() }
    }

    fn s(&self) -> &Structure {
        &self.set.list[0]
    }

    /// The ship's systems and the world's bodies, together, for `secs`.
    fn fly(&mut self, secs: f64) {
        for _ in 0..(secs * 60.0).round() as usize {
            let world = world_at(&self.set.list[0], &self.bodies);
            for _ in 0..(1.0 / 60.0 / TICK).round().max(1.0) as usize {
                self.ship.update(&mut self.set.list[0], &world, TICK);
            }
            self.set.step(1.0 / 60.0, &self.bodies);
        }
    }

    /// Its engines armed and started by the switches that do it; its throttle is not touched.
    fn start(&mut self) {
        let kind = self.kind.clone();
        let mut names: Vec<(String, String)> = engines(&kind).iter().map(|&m| (order(&kind, m, "armado"), order(&kind, m, "arranque"))).collect();
        names.sort();
        names.dedup();
        self.fly(3.0);
        for (arm, _) in &names {
            set(&mut self.ship, &self.set.list[0], arm, 1.0);
        }
        self.fly(0.5);
        for (_, start) in &names {
            set(&mut self.ship, &self.set.list[0], start, 2.0);
        }
        self.fly(0.4);
        for (_, start) in &names {
            set(&mut self.ship, &self.set.list[0], start, 1.0);
        }
        self.fly(6.0);
        for &m in &engines(&kind) {
            let state = signal(&self.ship, &format!("{}.estado", kind.machines[m].id));
            assert_eq!(state, 4.0, "{}: el motor {} no arranca (estado {state})", kind.id, kind.machines[m].id);
        }
    }

    fn thrust(&self) -> f64 {
        engines(&self.kind).iter().map(|&m| signal(&self.ship, &format!("{}.empuje", self.kind.machines[m].id))).sum()
    }

    fn jets(&self) -> f64 {
        let ids: Vec<String> = self.kind.machines.iter().map(|m| m.id.clone()).collect();
        self.kind.def.vuelo.iter().flat_map(|f| &f.rcs).flat_map(|p| lunar_ship::kind::resolve(&ids, p)).map(|m| signal(&self.ship, &format!("{}.empuje", ids[m as usize]))).sum()
    }

    fn weight(&self) -> f64 {
        f64::from(self.s().mass) * self.bodies.get(0).gravity
    }

    /// How far it leans from the upright (degrees), and how fast it goes over the ground (m/s).
    fn lean(&self) -> f64 {
        let s = self.s();
        (s.rot * Vec3::Y).as_dvec3().angle_between(self.bodies.get(0).up(s.pos)).to_degrees()
    }

    /// How far it has gone over the ground from `at` (m): not what it rises on its legs as they
    /// are unloaded.
    fn crept(&self, at: DVec3) -> f64 {
        let up = self.bodies.get(0).up(at);
        let d = self.s().pos - at;
        (d - up * d.dot(up)).length()
    }

    fn drift(&self) -> f64 {
        let s = self.s();
        let up = self.bodies.get(0).up(s.pos);
        (s.vel - up * s.vel.dot(up)).length()
    }

    /// MANTENER and the stabiliser on, up for a while, and let go: it hangs.
    fn lift_off(&mut self, up_for: f64, settle: f64) {
        let f = self.kind.def.vuelo.clone().unwrap();
        set(&mut self.ship, &self.set.list[0], f.estabilizador.as_deref().unwrap(), 1.0);
        set(&mut self.ship, &self.set.list[0], f.mantener.as_deref().unwrap(), 1.0);
        hold(&mut self.ship, &self.set.list[0], &f.traslacion.as_ref().unwrap()[1], 1.0);
        self.fly(up_for);
        hold(&mut self.ship, &self.set.list[0], &f.traslacion.as_ref().unwrap()[1], 0.0);
        self.fly(settle);
    }

    /// Hanging for `secs`: the most it leans, turns and drifts, and what its thrusters give on
    /// the average (N).
    fn hang(&mut self, secs: f64) -> (f64, f32, f64, f64) {
        let frames = (secs * 60.0) as usize;
        let (mut lean, mut spin, mut drift, mut jets) = (0.0f64, 0.0f32, 0.0f64, 0.0f64);
        for _ in 0..frames {
            self.fly(1.0 / 60.0);
            lean = lean.max(self.lean());
            spin = spin.max(self.s().spin.length());
            drift = drift.max(self.drift());
            jets += self.jets() / frames as f64;
        }
        (lean, spin, drift, jets)
    }
}

/// What its engines give at full, by its data (N).
fn rated(_lib: &Arc<Library>, kind: &Arc<ShipKind>) -> f64 {
    engines(kind).iter().map(|&m| kind.machines[m].jet.as_ref().map_or(0.0, |j| f64::from(j.rated))).sum()
}

#[test]
fn its_engines_started_and_its_throttle_let_be_it_stays_where_it_stands() {
    let (lib, kinds) = built();
    for kind in &kinds {
        for g in [GRAVITIES[0], GRAVITIES[2]] {
            let mut f = Flight::new(&lib, kind, g);
            f.start();
            let (at, lean) = (f.s().pos, f.lean());
            f.fly(20.0);
            let (thrust, weight) = (f.thrust(), f.weight());
            eprintln!("{} a {g} m/s², motores al ralentí: empujan {:.0} N de {:.0} N de peso; se mueve {:.3} m y se inclina {:.2}° más", kind.id, thrust, weight, f.crept(at), f.lean() - lean);
            // (on the smallest moon what idles is a good part of what little it weighs: it must
            // still not lift it)
            assert!(thrust < weight * 0.9 && thrust < rated(&lib, kind) * 0.05, "{} a {g} m/s²: al ralentí sus motores empujan {thrust:.0} N y pesa {weight:.0} N", kind.id);
            // (a ship still settling on its sprung legs leans a little either way as it does)
            assert!(f.crept(at) < 0.05 && (f.lean() - lean).abs() < 0.8, "{} a {g} m/s²: con los motores al ralentí se mueve {:.3} m y se inclina {:.2}°", kind.id, f.crept(at), f.lean() - lean);
        }
        // the throttle part of the way up, well short of its weight: it does not creep or rock
        let mut f = Flight::new(&lib, kind, GRAVITIES[2]);
        f.start();
        let (at, lean) = (f.s().pos, f.lean());
        let throttle = kind.def.vuelo.as_ref().unwrap().acelerador.clone().unwrap();
        let part = (0.5 * f.weight() / rated(&lib, kind)).min(0.5);
        set(&mut f.ship, &f.set.list[0], &throttle, part);
        f.fly(15.0);
        eprintln!("{} con el acelerador al {:.0} %: empujan {:.0} N de {:.0} N de peso; se mueve {:.3} m y se inclina {:.2}° más", kind.id, part * 100.0, f.thrust(), f.weight(), f.crept(at), f.lean() - lean);
        assert!(f.thrust() > f.weight() * 0.3 && f.thrust() < f.weight() * 0.7, "{}: al {:.0} % empujan {:.0} N", kind.id, part * 100.0, f.thrust());
        // (half its weight off its legs, they stretch: it rises and settles a little to one side)
        assert!(f.crept(at) < 0.1 && (f.lean() - lean).abs() < 2.0, "{}: con medio peso de empuje se mueve {:.3} m y se inclina {:.2}°", kind.id, f.crept(at), f.lean() - lean);
    }
}

#[test]
fn holding_it_lifts_off_level_and_hangs_still_under_every_gravity_it_can_fly_in() {
    let (lib, kinds) = built();
    eprintln!("nave        g m/s²  empuje/peso  inclin. °  giro rad/s  deriva m/s  toberas N  altura m");
    for kind in &kinds {
        let mut flown = 0;
        for g in GRAVITIES {
            let mut f = Flight::new(&lib, kind, g);
            let ratio = rated(&lib, kind) / f.weight();
            if ratio < MARGIN {
                eprintln!("{:<10} {g:>6.2}  {ratio:>10.2}   (sus motores no la levantan con margen: no se vuela)", kind.id);
                continue;
            }
            flown += 1;
            f.start();
            // (the less it weighs the longer it is given to stop: its thrusters are the same)
            f.lift_off(6.0, 24.0);
            let (lean, spin, drift, jets) = f.hang(20.0);
            let w = world_at(f.s(), &f.bodies);
            eprintln!("{:<10} {g:>6.2}  {ratio:>10.2}  {lean:>9.2}  {spin:>10.4}  {drift:>10.3}  {jets:>9.0}  {:>8.1}", kind.id, w.altitude);
            assert!(w.altitude > 3.0 && w.climb.abs() < 0.3, "{} a {g} m/s²: no se sostiene: a {:.1} m y {:.2} m/s", kind.id, w.altitude, w.climb);
            assert!(lean < 1.5 && spin < 0.02, "{} a {g} m/s²: suspendida se inclina {lean:.2}° y gira a {spin:.4} rad/s", kind.id);
            assert!(drift < 0.35, "{} a {g} m/s²: suspendida deriva a {drift:.2} m/s", kind.id);
            // it is its engines that carry it: the thrusters give a small part of its weight
            assert!((f.thrust() - f.weight()).abs() < f.weight() * 0.08, "{} a {g} m/s²: sus motores empujan {:.0} N y pesa {:.0} N", kind.id, f.thrust(), f.weight());
            assert!(jets < (f.weight() * 0.03).max(120.0), "{} a {g} m/s²: son las toberas las que la sostienen ({jets:.0} N de media, pesa {:.0} N)", kind.id, f.weight());
        }
        assert!(flown >= 2, "{}: solo se ha podido volar bajo {flown} gravedades", kind.id);
    }
}

#[test]
fn cargo_held_to_one_side_moves_its_centre_of_mass_and_it_still_hangs_level() {
    let (lib, kinds) = built();
    for kind in &kinds {
        let mut f = Flight::new(&lib, kind, GRAVITIES[2]);
        let before = (f.s().mass, f.s().com);
        // a block of a twelfth of its mass, held a good way to port and aft of its middle
        let (id, m) = (f.set.next_id(), before.0 / 12.0);
        let at = before.1 + Vec3::new(0.35 * f.s().radius.min(4.0), 0.0, -0.2 * f.s().radius.min(6.0));
        let part = Part::new(&lib.catalog, 0, Affine3A::IDENTITY, Some(Convex::cuboid(Vec3::splat(0.3))), false);
        let mut block = Structure::assemble(id, "lastre".into(), f.s().to_world(at), f.s().rot, false, vec![part], Vec::new());
        block.mass = m;
        f.set.list.push(block);
        let ship = f.s().id;
        assert!(f.set.hold_at(id, ship, 0, at, Quat::IDENTITY), "{}: no sujeta el lastre", kind.id);
        f.fly(0.2);
        let after = (f.s().mass, f.s().com);
        // (where it is held: where it was put, near enough — a thing held is set down on what is under it)
        let held = f.s().loads.iter().find(|l| l.id == id).copied().unwrap_or_else(|| panic!("{}: el lastre no cuenta en lo que lleva", kind.id));
        assert!((held.at - at).length() < 0.4 && (held.mass - m).abs() < 1.0, "{}: sujeto en {:?} con {:.0} kg", kind.id, held.at, held.mass);
        let by_hand = (before.1 * before.0 + held.at * m) / (before.0 + m);
        eprintln!("{}: {:.0} kg con {:.0} kg más a ({:.2}, {:.2}, {:.2}): su centro de masas pasa de ({:.3}, {:.3}, {:.3}) a ({:.3}, {:.3}, {:.3})", kind.id, before.0, m, at.x, at.y, at.z, before.1.x, before.1.y, before.1.z, after.1.x, after.1.y, after.1.z);
        assert!((after.0 - before.0 - m).abs() < 1.0, "{}: con {m:.0} kg sujetos pesa {:.0} kg más", kind.id, after.0 - before.0);
        // (across and along it; up and down its legs give under the new weight, and they weigh too)
        let off = after.1 - by_hand;
        assert!(off.x.abs() < 0.005 && off.z.abs() < 0.005 && off.y.abs() < 0.05 && (after.1 - before.1).length() > 0.02, "{}: su centro de masas está en {:?} y la suma a mano da {by_hand:?}", kind.id, after.1);
        // and it flies with it: level, on its engines
        if rated(&lib, kind) / f.weight() < MARGIN {
            eprintln!("   (con ese lastre sus motores no la levantan con margen: no se vuela)");
            continue;
        }
        f.start();
        f.lift_off(6.0, 24.0);
        let (lean, spin, drift, jets) = f.hang(15.0);
        let w = world_at(f.s(), &f.bodies);
        eprintln!("   suspendida con él: inclinada {lean:.2}°, gira a {spin:.4} rad/s, deriva {drift:.2} m/s, toberas {jets:.0} N, a {:.1} m", w.altitude);
        assert!(w.altitude > 3.0 && w.climb.abs() < 0.3, "{}: con el lastre no se sostiene: a {:.1} m y {:.2} m/s", kind.id, w.altitude, w.climb);
        assert!(lean < 2.0 && spin < 0.03 && drift < 0.4, "{}: con el lastre se inclina {lean:.2}°, gira a {spin:.4} rad/s y deriva a {drift:.2} m/s", kind.id);
    }
}
