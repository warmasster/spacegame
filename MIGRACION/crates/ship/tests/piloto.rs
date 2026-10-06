//! The autopilot, flown: every ship of the library whose panels have its holds and its programs,
//! with its engines, its thrusters and its weight, over a ground (`docs/COMBATE.md`).
//! - MANTENER and a translation key: it goes that way with its hull level (a ship that swings
//!   its engines swings them; it does not stand on its tail);
//! - DESPEG. climbs to the height held; a height, a heading and a speed held together are all
//!   reached and kept; the speed taken down to nothing, it stops;
//! - ATERRIZ. sets it down gently where it is and lets its engines go;
//! - FRENAR stops it from speed, level all the while;
//! - a combat mode puts its nose on a track wherever it is — behind, above — turning across and
//!   then up, never rolling over;
//! - SEGUIR goes to a track that moves and stays with it at the range kept.
//!
//! Run with `--nocapture` for what each flight did.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, set::Structures, state::Structure},
};
use lunar_ship::{Ship, ShipKind, Sources, World, def::ShipDef, ship::TICK, tactical::Contact};
use std::{path::Path, sync::Arc};

/// The Moon's.
const G: f64 = 1.62;

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

/// The library and the ships a pilot can set the autopilot's holds of.
fn built() -> (Arc<Library>, Vec<Arc<ShipKind>>) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let src = Sources::load(&defs()).unwrap_or_else(|e| panic!("{e}"));
    let ships: Vec<(String, ShipDef)> = lunar_core::defs::load_dir(&defs().join("ships")).unwrap_or_else(|e| panic!("{e}"));
    let mut kinds = Vec::new();
    for (id, d) in ships {
        let (k, bp) = src.build(&id, d, &mut lib.catalog).unwrap_or_else(|e| panic!("{e}"));
        let k = Arc::new(k);
        let ship = Ship::new(k.clone(), 1, 7).unwrap_or_else(|e| panic!("{id}: {e}"));
        if ["ap.nav", "ap.altura", "ap.rumbo", "ap.velocidad"].iter().all(|n| writer(&ship, n).is_some()) {
            kinds.push(k);
        }
        lib.blueprints.push((id, bp));
    }
    assert!(!kinds.is_empty(), "ninguna nave tiene en sus paneles el piloto automático");
    (Arc::new(lib), kinds)
}

fn world() -> (BodyRegistry, DVec3) {
    let all: Vec<(String, BodyDef)> = lunar_core::defs::load_dir(&defs().join("bodies")).unwrap_or_else(|e| panic!("{e}"));
    let (id, mut d) = all.into_iter().next().unwrap();
    let up = DVec3::new(0.3, 0.9, -0.2).normalize();
    d.center = [0.0; 3];
    d.gravity = G;
    d.level_at = Some(up.to_array());
    (BodyRegistry::new(vec![Body::from_def(&id, &d).unwrap()]), up)
}

fn signal(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id))
}

/// The control of its panels that writes signal `name`.
fn writer(ship: &Ship, name: &str) -> Option<usize> {
    let sig = ship.store.find(name)?;
    ship.panels.controls.iter().position(|c| c.sig == sig)
}

fn control(ship: &Ship, name: &str) -> usize {
    writer(ship, name).unwrap_or_else(|| panic!("{}: ningún mando escribe {name}", ship.kind.id))
}

fn engines(kind: &ShipKind) -> Vec<usize> {
    let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
    kind.def.vuelo.iter().flat_map(|f| &f.motores).flat_map(|p| lunar_ship::kind::resolve(&ids, p)).map(|m| m as usize).collect()
}

fn order(kind: &ShipKind, m: usize, role: &str) -> String {
    kind.machines[m].def.ordenes.get(role).and_then(|v| v.as_str()).map_or_else(|| format!("{}.{role}", kind.machines[m].id), str::to_string)
}

/// Something in the world that the ship's sensors are told of: where it is and how it goes.
#[derive(Clone, Copy)]
struct Thing {
    pos: DVec3,
    vel: DVec3,
}

/// A ship standing on the ground of the Moon, flown by its own systems.
struct Flight {
    set: Structures,
    bodies: BodyRegistry,
    ship: Ship,
    kind: Arc<ShipKind>,
    thing: Option<Thing>,
    /// Its lowest point as it stands on its gear (ship frame, y).
    keel: f32,
    t: f64,
    /// The most it has leaned from the upright since `watch` (degrees).
    leaned: f64,
}

impl Flight {
    fn new(lib: &Arc<Library>, kind: &Arc<ShipKind>) -> Flight {
        let (bodies, up) = world();
        let mut set = Structures::new(lib.clone());
        let id = set.place(&kind.blueprint, &bodies, 0, up, 0.0, f64::from(kind.lift)).unwrap();
        let mut ship = Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{}: {e}", kind.id));
        let w = World::at(&set.list[0], &bodies, set.list[0].to_world(set.list[0].com));
        ship.update(&mut set.list[0], &w, 0.0);
        set.rest_on_ground(id, &bodies);
        let sink = ship.rest_on_legs(&mut set.list[0], G as f32);
        let up = bodies.get(0).up(set.list[0].pos);
        set.list[0].pos -= up * f64::from(sink);
        let keel = ship.keel(&set.list[0]);
        Flight { set, bodies, ship, kind: kind.clone(), thing: None, keel, t: 0.0, leaned: 0.0 }
    }

    fn s(&self) -> &Structure {
        &self.set.list[0]
    }

    /// What the world is to it: its height, that of its lowest point (its feet) over the ground.
    fn world(&self) -> World {
        let s = self.s();
        World::at(s, &self.bodies, s.to_world(Vec3::new(s.com.x, self.keel, s.com.z)))
    }

    /// The ship's systems and the world's bodies, together, for `secs`; what there is to sense
    /// told to its sensors four times a second, as the game does.
    fn fly(&mut self, secs: f64) {
        for _ in 0..(secs * 60.0).round() as usize {
            if let Some(th) = &mut self.thing {
                th.pos += th.vel / 60.0;
                if (self.t * 4.0).floor() != ((self.t - 1.0 / 60.0) * 4.0).floor() {
                    let s = &self.set.list[0];
                    let c = Contact { id: 77, pos: s.to_local(th.pos), vel: s.rot.inverse() * (th.vel - s.vel).as_vec3(), rcs: 5.0, heat: 5.0e4, cone: -1.0, code: 4321, ..Contact::default() };
                    let tac = self.ship.tactical.as_mut().expect("táctico");
                    tac.sensed.clear();
                    tac.sensed.push(c);
                    tac.fresh = true;
                }
            }
            let world = self.world();
            for _ in 0..(1.0 / 60.0 / TICK).round().max(1.0) as usize {
                self.ship.update(&mut self.set.list[0], &world, TICK);
            }
            self.set.step(1.0 / 60.0, &self.bodies);
            self.t += 1.0 / 60.0;
            self.leaned = self.leaned.max(self.lean());
        }
    }

    fn set(&mut self, name: &str, value: f64) {
        let (k, kind) = (control(&self.ship, name), self.kind.clone());
        self.ship.panels.intent(k, &Intent::Set { value }, &self.set.list[0], &kind, &self.ship.store);
    }

    /// A sprung lever held at `value`, as a key holds it (0: let go).
    fn hold(&mut self, name: &str, value: f64) {
        let (k, kind) = (control(&self.ship, name), self.kind.clone());
        self.ship.panels.intent(k, &Intent::Axis { axis: 0, value }, &self.set.list[0], &kind, &self.ship.store);
    }

    fn press(&mut self, name: &str) {
        let (k, kind) = (control(&self.ship, name), self.kind.clone());
        self.ship.panels.intent(k, &Intent::Press { elem: 0 }, &self.set.list[0], &kind, &self.ship.store);
        self.fly(0.2);
        self.ship.panels.intent(k, &Intent::Release, &self.set.list[0], &kind, &self.ship.store);
        self.fly(0.1);
    }

    /// Its engines armed and started by the switches that do it, its stabiliser on.
    fn start(&mut self) {
        let kind = self.kind.clone();
        let mut names: Vec<(String, String)> = engines(&kind).iter().map(|&m| (order(&kind, m, "armado"), order(&kind, m, "arranque"))).collect();
        names.sort();
        names.dedup();
        self.fly(3.0);
        for (arm, _) in &names {
            self.set(arm, 1.0);
        }
        self.fly(0.5);
        for (_, start) in &names {
            self.set(start, 2.0);
        }
        self.fly(0.4);
        for (_, start) in &names {
            self.set(start, 1.0);
        }
        self.fly(6.0);
        for &m in &engines(&kind) {
            assert_eq!(signal(&self.ship, &format!("{}.estado", kind.machines[m].id)), 4.0, "{}: el motor {} no arranca", kind.id, kind.machines[m].id);
        }
        let sas = kind.def.vuelo.as_ref().unwrap().estabilizador.clone().unwrap();
        self.set(&sas, 1.0);
    }

    /// Off the ground by its own program, to `height` metres, and hanging there.
    fn up_to(&mut self, height: f64) {
        self.start();
        self.set("ap.altura_sel", height);
        self.set("ap.altura", 1.0);
        self.set("ap.nav", program("despegar"));
        self.fly(8.0 + height / 12.0);
        self.set("ap.nav", 0.0);
        self.watch();
    }

    /// From now: how far it leans.
    fn watch(&mut self) {
        self.leaned = 0.0;
    }

    fn lean(&self) -> f64 {
        let s = self.s();
        (s.rot * Vec3::Y).as_dvec3().angle_between(self.bodies.get(0).up(s.pos)).to_degrees()
    }

    /// Its speed over the ground (m/s), across and up.
    fn speed(&self) -> (f64, f64) {
        let s = self.s();
        let up = self.bodies.get(0).up(s.pos);
        ((s.vel - up * s.vel.dot(up)).length(), s.vel.dot(up))
    }

    fn thrust(&self) -> f64 {
        engines(&self.kind).iter().map(|&m| signal(&self.ship, &format!("{}.empuje", self.kind.machines[m].id))).sum()
    }

    /// How far off its nose something at `p` (world) is (degrees).
    fn off_nose(&self, p: DVec3) -> f64 {
        let s = self.s();
        (s.rot * Vec3::Z).as_dvec3().angle_between(p - s.to_world(s.com)).to_degrees()
    }
}

/// Where program `id` is on the selector.
fn program(id: &str) -> f64 {
    (lunar_ship::autopilot::NAV.iter().position(|m| m.id == id).unwrap_or_else(|| panic!("no hay programa {id}")) + 1) as f64
}

fn combat(id: &str) -> f64 {
    (lunar_ship::autopilot::COMBAT.iter().position(|m| m.id == id).unwrap_or_else(|| panic!("no hay modo {id}")) + 1) as f64
}

#[test]
fn holding_a_key_takes_it_that_way_with_its_hull_level() {
    let (lib, kinds) = built();
    for kind in &kinds {
        let mut f = Flight::new(&lib, kind);
        f.up_to(40.0);
        let fd = kind.def.vuelo.clone().unwrap();
        f.set("ap.altura", 0.0);
        f.set(fd.mantener.as_deref().unwrap(), 1.0);
        f.fly(4.0);
        f.watch();
        // forward for a while
        let keys = fd.traslacion.clone().unwrap();
        f.hold(&keys[2], 1.0);
        f.fly(6.0);
        let (going, _) = f.speed();
        let ahead = (f.s().rot * Vec3::Z).as_dvec3().dot(f.s().vel);
        eprintln!("{}: MANTENER y adelante 6 s: {going:.1} m/s ({ahead:.1} hacia el morro), se inclina {:.1}° como mucho", kind.id, f.leaned);
        assert!(ahead > 2.5, "{}: con MANTENER y la tecla de avanzar va a {ahead:.2} m/s", kind.id);
        // let go: it stops, and it never stood on its tail
        f.hold(&keys[2], 0.0);
        f.fly(10.0);
        let (going, rising) = f.speed();
        assert!(going < 0.5 && rising.abs() < 0.4, "{}: soltada la tecla sigue a {going:.2} m/s y {rising:.2} m/s", kind.id);
        assert!(f.leaned < 6.0, "{}: con MANTENER se inclina {:.1}°", kind.id, f.leaned);
        assert!(f.world().altitude > 15.0, "{}: pierde altura: a {:.1} m", kind.id, f.world().altitude);
    }
}

#[test]
fn a_height_a_heading_and_a_speed_held_together_are_reached_and_kept() {
    let (lib, kinds) = built();
    for kind in &kinds {
        let mut f = Flight::new(&lib, kind);
        f.up_to(60.0);
        let w = f.world();
        eprintln!("{}: DESPEG. a 60 m: a {:.1} m y {:.2} m/s, rumbo {:.0}°, se inclina {:.1}°", kind.id, w.altitude, w.climb, w.heading, f.leaned);
        assert!((w.altitude - 60.0).abs() < 6.0 && w.climb.abs() < 0.6, "{}: DESPEG. a 60 m la deja a {:.1} m y {:.2} m/s", kind.id, w.altitude, w.climb);
        assert!(w.heading >= 0.0, "{}: no sabe su rumbo", kind.id);
        // another height, and a quarter turn to the right of where it looks
        let to = (w.heading + 90.0).rem_euclid(360.0);
        f.set("ap.rumbo_sel", to);
        f.set("ap.rumbo", 1.0);
        f.set("ap.altura_sel", 120.0);
        f.fly(20.0);
        let w = f.world();
        let off = (f64::from(w.heading) - to + 180.0).rem_euclid(360.0) - 180.0;
        eprintln!("{}: retenidos 120 m y {to:.0}°: a {:.1} m y {:.1}°", kind.id, w.altitude, w.heading);
        assert!((w.altitude - 120.0).abs() < 5.0 && w.climb.abs() < 0.6, "{}: a {:.1} m y {:.2} m/s con 120 retenidos", kind.id, w.altitude, w.climb);
        assert!(off.abs() < 2.0, "{}: a {off:.1}° del rumbo retenido", kind.id);
        // and 80 m/s that way, over whatever ground comes (the height held is over the ground:
        // past the levelled field it goes up and down with it)
        f.set("ap.velocidad_sel", 80.0);
        f.set("ap.velocidad", 1.0);
        let mut lowest = f64::MAX;
        for _ in 0..(30 * 60) {
            f.fly(1.0 / 60.0);
            lowest = lowest.min(f.world().altitude);
        }
        let (w, (going, _)) = (f.world(), f.speed());
        let off = (f64::from(w.heading) - to + 180.0).rem_euclid(360.0) - 180.0;
        let along = (f.s().rot * Vec3::Z).as_dvec3().dot(f.s().vel);
        eprintln!("{}: y 80 m/s: a {:.1}° y {going:.1} m/s ({along:.1} hacia el morro), nunca a menos de {lowest:.0} m del suelo; se inclina {:.1}° como mucho", kind.id, w.heading, f.leaned);
        assert!(lowest > 40.0, "{}: con 120 m retenidos pasa a {lowest:.0} m del suelo", kind.id);
        assert!(off.abs() < 3.0, "{}: a {off:.1}° del rumbo retenido", kind.id);
        assert!((going - 80.0).abs() < 4.0 && along > 76.0, "{}: a {going:.1} m/s ({along:.1} hacia el morro) con 80 retenidos", kind.id);
        assert!(f.leaned < 8.0, "{}: se inclina {:.1}° volando con las retenciones", kind.id, f.leaned);
        // the speed taken down to nothing: it stops, level, and keeps its height
        f.set("ap.velocidad_sel", 0.0);
        f.fly(25.0);
        let (w, (going, _)) = (f.world(), f.speed());
        eprintln!("{}: la velocidad a cero: {going:.2} m/s a {:.1} m; se inclina {:.1}° como mucho", kind.id, w.altitude, f.leaned);
        assert!(going < 1.0 && (w.altitude - 120.0).abs() < 6.0, "{}: no se para: {going:.2} m/s a {:.1} m", kind.id, w.altitude);
        assert!(f.leaned < 8.0, "{}: se inclina {:.1}° al frenar", kind.id, f.leaned);
    }
}

#[test]
fn landing_sets_it_down_gently_where_it_is_and_lets_its_engines_go() {
    let (lib, kinds) = built();
    for kind in &kinds {
        let mut f = Flight::new(&lib, kind);
        f.up_to(45.0);
        let at = f.s().pos;
        // down, by its program: gently, and its engines let go once it stands
        f.set("ap.altura", 0.0);
        f.set("ap.nav", program("aterrizar"));
        let mut hardest = 0.0f64;
        let mut landed = None;
        for k in 0..(120 * 60) {
            f.fly(1.0 / 60.0);
            if f.s().grounded {
                landed.get_or_insert(k as f64 / 60.0);
            } else if landed.is_none() {
                hardest = f.speed().1.min(0.0).abs().max(0.0);
            }
            if landed.is_some_and(|at| k as f64 / 60.0 > at + 6.0) {
                break;
            }
        }
        let (thrust, weight) = (f.thrust(), f64::from(f.s().mass) * G);
        eprintln!("{}: ATERRIZ. desde 45 m: posada a los {:.0} s, tocando a {hardest:.2} m/s; luego sus motores empujan {thrust:.0} N de {weight:.0} N de peso", kind.id, landed.unwrap_or(-1.0));
        assert!(landed.is_some(), "{}: ATERRIZ. no la posa en dos minutos (a {:.1} m)", kind.id, f.world().altitude);
        assert!(hardest < 0.8, "{}: ATERRIZ. toca el suelo a {hardest:.2} m/s", kind.id);
        let up = f.bodies.get(0).up(at);
        let moved = f.s().pos - at;
        assert!((moved - up * moved.dot(up)).length() < 2.0, "{}: ATERRIZ. la posa a {:.1} m de donde estaba", kind.id, (moved - up * moved.dot(up)).length());
        assert!(thrust < weight * 0.2, "{}: posada, sus motores siguen empujando {thrust:.0} N", kind.id);
        assert!(f.lean() < 4.0, "{}: posada queda inclinada {:.1}°", kind.id, f.lean());
    }
}

#[test]
fn braking_stops_it_from_speed_level_all_the_while() {
    let (lib, kinds) = built();
    for kind in &kinds {
        let mut f = Flight::new(&lib, kind);
        f.up_to(80.0);
        f.set("ap.velocidad_sel", 70.0);
        f.set("ap.velocidad", 1.0);
        f.fly(25.0);
        assert!(f.speed().0 > 60.0, "{}: no coge velocidad: {:.1} m/s", kind.id, f.speed().0);
        // every hold off, and FRENAR
        for hold in ["ap.velocidad", "ap.altura"] {
            f.set(hold, 0.0);
        }
        f.set("ap.nav", program("frenar"));
        f.watch();
        let mut took = None;
        for k in 0..(40 * 60) {
            f.fly(1.0 / 60.0);
            if took.is_none() && f.speed().0 < 1.0 {
                took = Some(k as f64 / 60.0);
            }
        }
        let (w, (going, rising)) = (f.world(), f.speed());
        eprintln!("{}: FRENAR desde 70 m/s: parada en {:.1} s; queda a {going:.2} m/s y {rising:.2} m/s, a {:.1} m; se inclina {:.1}° como mucho", kind.id, took.unwrap_or(-1.0), w.altitude, f.leaned);
        assert!(took.is_some_and(|t| t < 25.0), "{}: FRENAR no la para en 25 s (va a {going:.1} m/s)", kind.id);
        assert!(going < 0.5 && rising.abs() < 0.4 && w.altitude > 30.0, "{}: tras FRENAR no queda sostenida: {going:.2} m/s, {rising:.2} m/s, {:.1} m", kind.id, w.altitude);
        assert!(f.leaned < 8.0, "{}: se inclina {:.1}° frenando", kind.id, f.leaned);
    }
}

#[test]
fn a_combat_mode_puts_its_nose_on_a_track_anywhere_without_rolling_over() {
    let (lib, kinds) = built();
    for kind in kinds.iter().filter(|k| Ship::new((*k).clone(), 1, 7).is_ok_and(|s| s.tactical.is_some())) {
        // behind and high to one side; ahead and low to the other; straight behind, level
        for (k, at) in [Vec3::new(2500.0, 1800.0, -3000.0), Vec3::new(-3000.0, -40.0, 2000.0), Vec3::new(0.0, 0.0, -4000.0)].into_iter().enumerate() {
            let mut f = Flight::new(&lib, kind);
            f.up_to(150.0);
            let fd = kind.def.vuelo.clone().unwrap();
            f.set("ap.altura", 0.0);
            f.set(fd.mantener.as_deref().unwrap(), 1.0);
            let p = f.s().to_world(at);
            f.thing = Some(Thing { pos: p, vel: DVec3::ZERO });
            f.fly(1.5);
            f.press("tac.cercano");
            assert_eq!(signal(&f.ship, "tac.elegido"), 1.0, "{}: no elige la traza", kind.id);
            f.set("ap.combate", combat("apuntar"));
            f.watch();
            // how far its wings ever tip (what a pilot sees as rolling over), and how long it takes
            let (mut tipped, mut took, mut wandered) = (0.0f64, None, 0.0f64);
            let mut last = f.off_nose(p);
            for n in 0..(40 * 60) {
                f.fly(1.0 / 60.0);
                let up = f.bodies.get(0).up(f.s().pos);
                tipped = tipped.max((f.s().rot * Vec3::X).as_dvec3().dot(up).abs().asin().to_degrees());
                let off = f.off_nose(p);
                // (it must close on it all the way in, not hunt about)
                wandered = wandered.max(off - last);
                last = last.min(off);
                if took.is_none() && off < 2.0 {
                    took = Some(n as f64 / 60.0);
                }
            }
            let off = f.off_nose(p);
            eprintln!("{} traza {k}: APUNTAR la pone a {off:.2}° en {:.1} s; las alas se inclinan {tipped:.1}° como mucho; se aleja de ella {wandered:.1}° como mucho", kind.id, took.unwrap_or(-1.0));
            assert!(took.is_some_and(|t| t < 25.0) && off < 1.5, "{} traza {k}: APUNTAR no la tiene en el morro: a {off:.1}°", kind.id);
            assert!(tipped < 12.0, "{} traza {k}: apuntando alabea {tipped:.1}°", kind.id);
            assert!(wandered < 4.0, "{} traza {k}: apuntando se aleja {wandered:.1}° de ella antes de llegar", kind.id);
        }
    }
}

#[test]
fn following_goes_to_a_track_that_moves_and_stays_with_it() {
    let (lib, kinds) = built();
    for kind in kinds.iter().filter(|k| Ship::new((*k).clone(), 1, 7).is_ok_and(|s| s.tactical.is_some())) {
        let mut f = Flight::new(&lib, kind);
        f.up_to(150.0);
        // 4 km off to one side and ahead, at its height, going across at 45 m/s
        let s = f.s();
        let (p, v) = (s.to_world(Vec3::new(2500.0, 0.0, 3000.0)), (s.rot * Vec3::new(-0.6, 0.0, 0.8)).as_dvec3() * 45.0);
        f.thing = Some(Thing { pos: p, vel: v });
        f.fly(1.5);
        f.press("tac.cercano");
        f.set("ap.distancia", 600.0);
        f.set("ap.altura_sel", 150.0);
        f.set("ap.nav", program("seguir"));
        f.watch();
        f.fly(110.0);
        let th = f.thing.unwrap();
        let (range, rel) = ((th.pos - f.s().to_world(f.s().com)).length(), (th.vel - f.s().vel).length());
        let w = f.world();
        eprintln!("{}: SEGUIR a 600 m de una traza a 45 m/s: a {range:.0} m y {rel:.2} m/s de ella, a {:.0} m de altura; se inclina {:.1}° como mucho; morro a {:.1}° de ella", kind.id, w.altitude, f.leaned, f.off_nose(th.pos));
        assert!((range - 600.0).abs() < 120.0, "{}: SEGUIR la deja a {range:.0} m de la traza (600 pedidos)", kind.id);
        assert!(rel < 3.0, "{}: SEGUIR no va como la traza: {rel:.1} m/s de diferencia", kind.id);
        assert!((w.altitude - 150.0).abs() < 12.0, "{}: siguiendo con la altura retenida está a {:.0} m", kind.id, w.altitude);
        assert!(f.leaned < 8.0, "{}: siguiendo se inclina {:.1}°", kind.id, f.leaned);
    }
}
