//! A ship flown by its own systems: its engines, its thrusters, its wheels and its weight, with
//! the world's physics round it — over the ground of a body, or past every body's reach. What
//! the tests that fly a ship share (`piloto.rs`, `coherencia.rs`); like the rest of `comun`, it
//! names no ship: what a ship has, it reads off the ship.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::{
    body::{Body, BodyDef, BodyRegistry},
    structure::{Library, set::Structures, state::Structure},
};
use lunar_ship::{Ship, ShipKind, Sources, World, def::ShipDef, kind::resolve, ship::TICK, tactical::Contact};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

/// The pull at the ground of the body flown over (the Moon's).
pub const G: f64 = 1.62;

pub fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

/// Every ship of the library, built once, with the library that places them.
pub struct Hangar {
    pub lib: Arc<Library>,
    pub kinds: Vec<Arc<ShipKind>>,
}

pub fn hangar() -> &'static Hangar {
    static HANGAR: OnceLock<Hangar> = OnceLock::new();
    HANGAR.get_or_init(|| {
        let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
        let src = Sources::load(&defs()).unwrap_or_else(|e| panic!("{e}"));
        let ships: Vec<(String, ShipDef)> = lunar_core::defs::load_dir(&defs().join("ships")).unwrap_or_else(|e| panic!("{e}"));
        let mut kinds = Vec::new();
        for (id, d) in ships {
            let (k, bp) = src.build(&id, d, &mut lib.catalog).unwrap_or_else(|e| panic!("{e}"));
            kinds.push(Arc::new(k));
            lib.blueprints.push((id, bp));
        }
        Hangar { lib: Arc::new(lib), kinds }
    })
}

/// The ships of the library that fly (they have a flight computer: `vuelo`).
pub fn flyers() -> Vec<Arc<ShipKind>> {
    hangar().kinds.iter().filter(|k| k.def.vuelo.is_some()).cloned().collect()
}

/// A body to fly over: the first of the library's, centred on the origin, with the Moon's pull
/// at its ground, levelled where the ships are set down (`up`).
pub fn world() -> (BodyRegistry, DVec3) {
    let all: Vec<(String, BodyDef)> = lunar_core::defs::load_dir(&defs().join("bodies")).unwrap_or_else(|e| panic!("{e}"));
    let (id, mut d) = all.into_iter().next().unwrap();
    let up = DVec3::new(0.3, 0.9, -0.2).normalize();
    d.center = [0.0; 3];
    d.gravity = G;
    d.level_at = Some(up.to_array());
    (BodyRegistry::new(vec![Body::from_def(&id, &d).unwrap()]), up)
}

pub fn signal(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("{}: no hay señal {name}", ship.kind.id))
}

/// The control of its panels that writes signal `name`.
pub fn writer(ship: &Ship, name: &str) -> Option<usize> {
    let sig = ship.store.find(name)?;
    ship.panels.controls.iter().position(|c| c.sig == sig)
}

pub fn control(ship: &Ship, name: &str) -> usize {
    writer(ship, name).unwrap_or_else(|| panic!("{}: ningún mando escribe {name}", ship.kind.id))
}

/// The machines its flight computer runs as `role` (`motores`, `rcs`, `ruedas`).
pub fn flown(kind: &ShipKind, role: &str) -> Vec<usize> {
    let Some(f) = &kind.def.vuelo else { return Vec::new() };
    let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
    let pats = match role {
        "motores" => &f.motores,
        "rcs" => &f.rcs,
        _ => &f.ruedas,
    };
    pats.iter().flat_map(|p| resolve(&ids, p)).map(|m| m as usize).collect()
}

pub fn engines(kind: &ShipKind) -> Vec<usize> {
    flown(kind, "motores")
}

pub fn order(kind: &ShipKind, m: usize, role: &str) -> String {
    kind.machines[m].def.ordenes.get(role).and_then(|v| v.as_str()).map_or_else(|| format!("{}.{role}", kind.machines[m].id), str::to_string)
}

/// Where program `id` is on the selector.
pub fn program(id: &str) -> f64 {
    (lunar_ship::autopilot::NAV.iter().position(|m| m.id == id).unwrap_or_else(|| panic!("no hay programa {id}")) + 1) as f64
}

pub fn combat(id: &str) -> f64 {
    (lunar_ship::autopilot::COMBAT.iter().position(|m| m.id == id).unwrap_or_else(|| panic!("no hay modo {id}")) + 1) as f64
}

/// Something in the world that the ship's sensors are told of: where it is and how it goes.
#[derive(Clone, Copy)]
pub struct Thing {
    pub pos: DVec3,
    pub vel: DVec3,
}

/// A ship flown by its own systems.
pub struct Flight {
    pub set: Structures,
    pub bodies: BodyRegistry,
    pub ship: Ship,
    pub kind: Arc<ShipKind>,
    pub thing: Option<Thing>,
    /// Its lowest point as it stands on its gear (ship frame, y).
    pub keel: f32,
    pub t: f64,
    /// The most it has leaned from the upright since `watch` (degrees).
    pub leaned: f64,
    /// The way up where it was set down (world): in space, the way it still calls up.
    pub up: DVec3,
}

impl Flight {
    /// Standing on the ground of the body, on its gear.
    pub fn new(kind: &Arc<ShipKind>) -> Flight {
        let lib = &hangar().lib;
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
        Flight { set, bodies, ship, kind: kind.clone(), thing: None, keel, t: 0.0, leaned: 0.0, up }
    }

    /// Still, far past the reach of every body: nothing pulls it, there is no ground and no way
    /// up but its own.
    pub fn in_space(kind: &Arc<ShipKind>) -> Flight {
        let mut f = Flight::new(kind);
        let b = f.bodies.get(0);
        let far = b.radius * 10.0 + 1.0e7;
        f.set.list[0].pos += f.up * far;
        f.set.list[0].vel = DVec3::ZERO;
        f.set.list[0].spin = Vec3::ZERO;
        f.set.list[0].grounded = false;
        assert!(f.bodies.field(f.set.list[0].pos).ground.is_none(), "{}: fuera del alcance de todo cuerpo sigue habiendo suelo", kind.id);
        f
    }

    pub fn s(&self) -> &Structure {
        &self.set.list[0]
    }

    /// What the world is to it: its height, that of its lowest point (its feet) over the ground.
    pub fn world(&self) -> World {
        let s = self.s();
        World::at(s, &self.bodies, s.to_world(Vec3::new(s.com.x, self.keel, s.com.z)))
    }

    /// The ship's systems and the world's bodies, together, for `secs` at 60 frames a second;
    /// what there is to sense told to its sensors four times a second, as the game does.
    pub fn fly(&mut self, secs: f64) {
        self.fly_at(secs, 60.0);
    }

    /// The same at `fps` frames a second.
    pub fn fly_at(&mut self, secs: f64, fps: f64) {
        let dt = 1.0 / fps;
        for _ in 0..(secs * fps).round() as usize {
            if let Some(th) = &mut self.thing {
                th.pos += th.vel * dt;
                if (self.t * 4.0).floor() != ((self.t - dt) * 4.0).floor() {
                    let s = &self.set.list[0];
                    let c = Contact { id: 77, pos: s.to_local(th.pos), vel: s.rot.inverse() * (th.vel - s.vel).as_vec3(), rcs: 5.0, heat: 5.0e4, cone: -1.0, code: 4321, ..Contact::default() };
                    let tac = self.ship.tactical.as_mut().expect("táctico");
                    tac.sensed.clear();
                    tac.sensed.push(c);
                    tac.fresh = true;
                }
            }
            let world = self.world();
            for _ in 0..(dt / TICK).round().max(1.0) as usize {
                self.ship.update(&mut self.set.list[0], &world, TICK);
            }
            self.set.step(dt as f32, &self.bodies);
            self.t += dt;
            self.leaned = self.leaned.max(self.lean());
        }
    }

    pub fn set(&mut self, name: &str, value: f64) {
        let k = control(&self.ship, name);
        self.intent(k, &Intent::Set { value });
    }

    /// What a hand or a key does to control `k`.
    pub fn intent(&mut self, k: usize, i: &Intent) -> lunar_controls::Outcome {
        let kind = self.kind.clone();
        self.ship.panels.intent(k, i, &self.set.list[0], &kind, &self.ship.store)
    }

    /// A sprung lever held at `value`, as a key holds it (0: let go).
    pub fn hold(&mut self, name: &str, value: f64) {
        let k = control(&self.ship, name);
        self.intent(k, &Intent::Axis { axis: 0, value });
    }

    pub fn press(&mut self, name: &str) {
        let k = control(&self.ship, name);
        self.intent(k, &Intent::Press { elem: 0 });
        self.fly(0.2);
        self.intent(k, &Intent::Release);
        self.fly(0.1);
    }

    /// Its engines armed and started by the switches that do it, its stabiliser on.
    pub fn start(&mut self) {
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
    pub fn up_to(&mut self, height: f64) {
        self.start();
        self.set("ap.altura_sel", height);
        self.set("ap.altura", 1.0);
        self.set("ap.nav", program("despegar"));
        self.fly(8.0 + height / 12.0);
        self.set("ap.nav", 0.0);
        self.watch();
    }

    /// From now: how far it leans.
    pub fn watch(&mut self) {
        self.leaned = 0.0;
    }

    /// How far its top is from the way up (degrees): where it was set down, in space.
    pub fn lean(&self) -> f64 {
        let s = self.s();
        let up = self.bodies.field(s.pos).ground.map_or(self.up, |g| self.bodies.get(g).up(s.pos));
        (s.rot * Vec3::Y).as_dvec3().angle_between(up).to_degrees()
    }

    /// Its speed over the ground (m/s), across and up.
    pub fn speed(&self) -> (f64, f64) {
        let s = self.s();
        let up = self.bodies.get(0).up(s.pos);
        ((s.vel - up * s.vel.dot(up)).length(), s.vel.dot(up))
    }

    pub fn thrust(&self) -> f64 {
        engines(&self.kind).iter().map(|&m| signal(&self.ship, &format!("{}.empuje", self.kind.machines[m].id))).sum()
    }

    /// What its thrusters push with together now (N).
    pub fn rcs_thrust(&self) -> f64 {
        flown(&self.kind, "rcs").iter().map(|&m| self.ship.signal(&format!("{}.empuje", self.kind.machines[m].id)).unwrap_or(0.0)).sum()
    }

    /// How full its fullest momentum wheel is (0..1; none: 0).
    pub fn wheels_full(&self) -> f64 {
        flown(&self.kind, "ruedas").iter().map(|&m| self.ship.signal(&format!("{}.carga", self.kind.machines[m].id)).unwrap_or(0.0)).fold(0.0, f64::max)
    }

    /// How it turns (rad/s) and goes (m/s), in its own frame.
    pub fn spin(&self) -> Vec3 {
        let s = self.s();
        s.rot.inverse() * s.spin
    }

    pub fn vel(&self) -> Vec3 {
        let s = self.s();
        s.rot.inverse() * s.vel.as_vec3()
    }

    /// How far off its nose something at `p` (world) is (degrees).
    pub fn off_nose(&self, p: DVec3) -> f64 {
        let s = self.s();
        (s.rot * Vec3::Z).as_dvec3().angle_between(p - s.to_world(s.com)).to_degrees()
    }
}
