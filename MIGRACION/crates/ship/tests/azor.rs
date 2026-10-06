//! The Azor, the one-seat fighter: its one power pack starts on its batteries and stops without
//! a trip; its sensors make tracks of what they are told is round it, each as far as it reaches;
//! any track can be locked; a decoy takes the lock; its weapons fire only armed, each as its
//! kind does; its autopilot turns it on its wheels toward what it must point at; its drum of
//! panels turns; its seat goes down through its belly and its bay doors open, with the ship
//! cold.
use glam::{DVec3, Vec3};
use lunar_controls::Intent;
use lunar_core::structure::{Library, state::Structure};
use lunar_ship::{
    Ship, ShipKind, ShipLibrary, World,
    ship::TICK,
    tactical::{BY_BEACON, BY_RADAR, BY_WARNER, Class, Contact, Iff, Load},
};
use std::{path::Path, sync::Arc};

fn defs() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
}

fn w() -> World {
    World { gravity: Vec3::new(0.0, -1.62, 0.0), altitude: 0.0, ..World::default() }
}

fn space() -> World {
    World { gravity: Vec3::ZERO, altitude: 1.0e5, ..World::default() }
}

fn run_in(s: &mut Structure, ship: &mut Ship, world: &World, secs: f64) {
    for _ in 0..(secs / TICK).round() as usize {
        ship.update(s, world, TICK);
    }
}

fn run(s: &mut Structure, ship: &mut Ship, secs: f64) {
    run_in(s, ship, &w(), secs);
}

fn ctl(ship: &Ship, id: &str) -> usize {
    ship.panels.controls.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no hay mando {id}"))
}

fn press(ship: &mut Ship, s: &mut Structure, id: &str) {
    let (k, kind) = (ctl(ship, id), ship.kind.clone());
    ship.panels.intent(k, &Intent::Press { elem: 0 }, s, &kind, &ship.store);
    run(s, ship, 0.2);
    ship.panels.intent(k, &Intent::Release, s, &kind, &ship.store);
    run(s, ship, 0.1);
}

fn set(ship: &mut Ship, s: &Structure, id: &str, value: f64) {
    let (k, kind) = (ctl(ship, id), ship.kind.clone());
    ship.panels.intent(k, &Intent::Set { value }, s, &kind, &ship.store);
}

fn get(ship: &Ship, name: &str) -> f64 {
    ship.signal(name).unwrap_or_else(|| panic!("no hay señal {name}"))
}

fn alone() -> (Structure, Ship, Arc<ShipKind>) {
    let mut lib = Library::load(&defs().join("structures")).unwrap_or_else(|e| panic!("{e}"));
    let (ships, bps) = ShipLibrary::load(&defs(), &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    lib.blueprints.extend(bps);
    let kind = ships.get("azor").expect("el Azor").clone();
    let bp = lib.blueprint(&kind.blueprint).unwrap();
    let mut s = Structure::new(1, bp, &lib.catalog, DVec3::ZERO, glam::Quat::IDENTITY);
    let mut ship = Ship::new(kind.clone(), 1, 7).unwrap_or_else(|e| panic!("{e}"));
    ship.update(&mut s, &w(), 0.0);
    (s, ship, kind)
}

/// Something `ahead` m ahead of the nose and `port` m to port, as its owner would tell it.
fn thing(id: u64, port: f32, ahead: f32) -> Contact {
    Contact { id, pos: Vec3::new(port, 0.0, ahead), rcs: 5.0, heat: 5.0e4, cone: -1.0, ..Contact::default() }
}

/// `secs` of flight with `things` told to its sensors four times a second (as the game does).
fn watch(s: &mut Structure, ship: &mut Ship, things: &[Contact], secs: f64) {
    let world = space();
    let mut next = 0.0;
    let mut t = 0.0;
    while t < secs {
        if t >= next {
            let tac = ship.tactical.as_mut().expect("táctico");
            tac.sensed.clear();
            tac.sensed.extend_from_slice(things);
            tac.fresh = true;
            next += 0.25;
        }
        ship.update(s, &world, TICK);
        t += TICK;
    }
}

/// Radar on and radiating, on scale `scale`; the passive sensors as they come (on).
fn radar_on(s: &mut Structure, ship: &mut Ship, scale: f64) {
    set(ship, s, "sensores/radar_enc", 1.0);
    set(ship, s, "sensores/radar_emitir", 1.0);
    set(ship, s, "sensores/radar_escala", scale);
    run_in(s, ship, &space(), 3.0);
    assert_eq!(get(ship, "radar.on"), 1.0, "el radar no arranca (bus {:.1} V)", get(ship, "elec.bus_a"));
}

#[test]
fn it_is_what_its_data_says() {
    let (s, ship, kind) = alone();
    let tac = ship.tactical.as_ref().expect("un caza tiene sistema táctico");
    eprintln!("azor: {:.2} t, centro de masas {:?}, radio {:.1} m, {} piezas, {} máquinas, {} mandos", s.mass / 1000.0, s.com, s.radius, s.parts.len(), kind.machines.len(), ship.panels.controls.len());
    assert!(s.mass > 4000.0 && s.mass < 16_000.0, "pesa {:.0} kg", s.mass);
    // its weapons in three groups the selector counts, its decoys apart
    assert_eq!(tac.groups, ["CAÑÓN", "DARDO", "LANZA", "SEÑUELOS"]);
    let of = |load: Load| tac.weapons.iter().filter(|w| w.load == load).count();
    assert_eq!((of(Load::Round), of(Load::Guided), of(Load::Decoy)), (2, 4, 2));
    // one power pack, one computer, two batteries
    let models = |m: &str| kind.machines.iter().filter(|x| x.def.modelo == m).count();
    assert_eq!((models("reactor_unificado"), models("ordenador"), models("bateria")), (1, 1, 2));
    // its nacelles push through its centre of mass (hovering, the thrusters carry nothing)
    let nacelle = kind.joints.iter().find(|j| j.id == "vector_izq").unwrap().pivot.z;
    assert!((nacelle - s.com.z).abs() < 0.12, "las góndolas en z = {nacelle:.2} y el centro de masas en z = {:.2}", s.com.z);
    // every mode of the autopilot is on its selector, in its order
    let positions = |panel: &str, id: &str| -> Vec<String> {
        let p = kind.panels.iter().find(|p| p.id == panel).unwrap_or_else(|| panic!("panel {panel}"));
        p.def.mandos.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{panel}/{id}")).posiciones.clone()
    };
    let names = |modes: &[lunar_ship::autopilot::Mode]| std::iter::once("OFF".to_string()).chain(modes.iter().map(|m| m.name.to_string())).collect::<Vec<_>>();
    assert_eq!(positions("nav", "nav_modo"), names(lunar_ship::autopilot::NAV));
    assert_eq!(positions("combate", "ap_modo"), names(lunar_ship::autopilot::COMBAT));
}

#[test]
fn its_power_pack_starts_on_its_batteries_and_stops_without_a_trip() {
    let (mut s, mut ship, _) = alone();
    run(&mut s, &mut ship, 4.0);
    assert!(get(&ship, "elec.ess") > 22.0, "frío, sus baterías tienen la barra esencial: {:.1} V", get(&ship, "elec.ess"));
    assert_eq!(get(&ship, "ordenador.on"), 1.0, "su ordenador arranca solo con la barra esencial");
    set(&mut ship, &s, "sistemas/rx_marcha", 1.0);
    run(&mut s, &mut ship, 8.0);
    assert_eq!(get(&ship, "reactor.estado"), 2.0, "en línea a los 8 s");
    run(&mut s, &mut ship, 20.0);
    assert!(get(&ship, "energia.generacion") > 500.0 && get(&ship, "energia.baterias") < 1.0, "genera {:.0} W, las baterías dan {:.0}", get(&ship, "energia.generacion"), get(&ship, "energia.baterias"));
    // PARO: off, no cause, nothing to rearm
    set(&mut ship, &s, "sistemas/rx_marcha", 0.0);
    run(&mut s, &mut ship, 0.5);
    assert_eq!((get(&ship, "reactor.estado"), get(&ship, "reactor.causa")), (0.0, 0.0));
    set(&mut ship, &s, "sistemas/rx_marcha", 1.0);
    run(&mut s, &mut ship, 8.0);
    assert_eq!(get(&ship, "reactor.estado"), 2.0);
    // the button trips it and says why; let go, it comes back by itself
    let k = ctl(&ship, "sistemas/rx_tapa_scram");
    let kind = ship.kind.clone();
    ship.panels.intent(k, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 0.5);
    let k = ctl(&ship, "sistemas/rx_scram");
    ship.panels.intent(k, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    run(&mut s, &mut ship, 1.0);
    assert_eq!((get(&ship, "reactor.estado"), get(&ship, "reactor.causa")), (3.0, 1.0), "la seta lo dispara y lo dice");
}

#[test]
fn its_sensors_make_tracks_each_as_far_as_it_reaches() {
    let (mut s, mut ship, _) = alone();
    run_in(&mut s, &mut ship, &space(), 4.0);
    // nothing on: nothing is asked of whoever tells it what is round it
    set(&mut ship, &s, "sensores/sens_irst", 0.0);
    set(&mut ship, &s, "sensores/sens_rwr", 0.0);
    set(&mut ship, &s, "sensores/sens_iff", 0.0);
    run_in(&mut s, &mut ship, &space(), 1.0);
    assert!(!ship.tactical.as_ref().unwrap().looking);
    set(&mut ship, &s, "sensores/sens_rwr", 1.0);
    set(&mut ship, &s, "sensores/sens_iff", 1.0);
    // radar on its 20 km scale: 12 km ahead it is found, 30 km ahead it is not, behind it is not
    radar_on(&mut s, &mut ship, 1.0);
    assert!(ship.tactical.as_ref().unwrap().looking);
    let (near, far, behind) = (thing(1, 0.0, 12_000.0), thing(2, 800.0, 30_000.0), thing(3, 0.0, -6_000.0));
    watch(&mut s, &mut ship, &[near, far, behind], 1.0);
    let seen = |ship: &Ship| -> Vec<(u64, u8)> { ship.tactical.as_ref().unwrap().tracks[..ship.tactical.as_ref().unwrap().len].iter().map(|t| (t.id, t.by)).collect() };
    assert_eq!(seen(&ship), [(1, BY_RADAR)], "a 20 km de escala");
    assert_eq!(get(&ship, "tac.contactos"), 1.0);
    // on its 80 km scale the far one too (a square metre at 80 km: this reflects five)
    set(&mut ship, &s, "sensores/radar_escala", 3.0);
    watch(&mut s, &mut ship, &[near, far, behind], 1.0);
    assert_eq!(seen(&ship).iter().map(|t| t.0).collect::<Vec<_>>(), [1, 2]);
    // what answers with a code is a track wherever it is, radar or no radar: CIVIL
    let civil = Contact { code: 2044, ..behind };
    set(&mut ship, &s, "sensores/radar_emitir", 0.0);
    watch(&mut s, &mut ship, &[near, far, civil], 4.0);
    assert_eq!(seen(&ship), [(3, BY_BEACON)], "callado, solo lo que responde");
    assert_eq!(ship.tactical.as_ref().unwrap().tracks[0].iff, Iff::Civil);
    // a radar sweeping the ship from behind is heard, by bearing; locked on it, it is hostile
    let radar = Contact { erp: 1.0, beam: Vec3::Z, cone: 0.5, ..thing(4, 0.0, -60_000.0) };
    watch(&mut s, &mut ship, &[radar], 4.0);
    assert_eq!(seen(&ship), [(4, BY_WARNER)]);
    assert_eq!((get(&ship, "rwr.amenazas"), get(&ship, "rwr.fijado")), (1.0, 0.0));
    watch(&mut s, &mut ship, &[Contact { locks: true, ..radar }], 1.0);
    assert_eq!(get(&ship, "rwr.fijado"), 1.0);
    assert_eq!(ship.tactical.as_ref().unwrap().tracks[0].iff, Iff::Hostile);
    // (looking the other way, its sidelobes are heard only from near)
    watch(&mut s, &mut ship, &[Contact { beam: Vec3::NEG_Z, ..radar }], 4.0);
    assert!(seen(&ship).is_empty(), "{:?}", seen(&ship));
}

#[test]
fn any_track_can_be_locked_and_a_decoy_takes_the_lock() {
    let (mut s, mut ship, _) = alone();
    run_in(&mut s, &mut ship, &space(), 4.0);
    radar_on(&mut s, &mut ship, 1.0);
    // a civil craft ahead: chosen with the nearest button, locked with FIJAR
    let civil = Contact { code: 2044, class: Class::Craft, ..thing(9, 200.0, 8_000.0) };
    watch(&mut s, &mut ship, &[civil], 1.0);
    press(&mut ship, &mut s, "principal/obj_cerca");
    watch(&mut s, &mut ship, &[civil], 0.5);
    assert_eq!(ship.tactical.as_ref().unwrap().chosen, Some(9));
    assert_eq!((get(&ship, "tac.fijado"), get(&ship, "tac.obj.iff")), (0.0, 2.0));
    assert!((get(&ship, "tac.obj.dist") - 8002.5).abs() < 5.0, "{}", get(&ship, "tac.obj.dist"));
    press(&mut ship, &mut s, "principal/obj_fijar");
    watch(&mut s, &mut ship, &[civil], 0.5);
    assert_eq!(get(&ship, "tac.fijado"), 1.0, "se fija lo que sea");
    assert_eq!(ship.tactical.as_ref().unwrap().emission.lock, Some(9), "y quien lo cuenta a los demás lo sabe");
    // it lets a cloud of chaff go beside itself: the lock goes to the cloud
    let cloud = Contact { id: 77, pos: civil.pos + Vec3::new(40.0, 0.0, 0.0), rcs: 90.0, class: Class::Decoy, ..Contact::default() };
    watch(&mut s, &mut ship, &[civil, cloud], 0.5);
    assert_eq!(ship.tactical.as_ref().unwrap().chosen, Some(77), "la nube se lleva el blocaje");
    // the cloud gone, the lock is lost
    watch(&mut s, &mut ship, &[civil], 3.0);
    assert_eq!(get(&ship, "tac.fijado"), 0.0);
}

#[test]
fn its_weapons_fire_only_armed_each_as_its_kind_does() {
    let (mut s, mut ship, _) = alone();
    run_in(&mut s, &mut ship, &space(), 4.0);
    let fired = |ship: &mut Ship| std::mem::take(&mut ship.tactical.as_mut().unwrap().fired);
    let trigger = ctl(&ship, "mando/gatillo");
    let kind = ship.kind.clone();
    let rounds = get(&ship, "canon_izq.municion");
    // safe: the trigger does nothing
    ship.panels.intent(trigger, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    watch(&mut s, &mut ship, &[], 1.0);
    assert!(fired(&mut ship).is_empty() && get(&ship, "canon_izq.municion") == rounds, "en SEGURO no dispara");
    ship.panels.intent(trigger, &Intent::Release, &s, &kind, &ship.store);
    // armed, cannon chosen: both cannons, at their rate, while it is held
    press(&mut ship, &mut s, "combate/armas_tapa");
    set(&mut ship, &s, "combate/armas_maestro", 1.0);
    watch(&mut s, &mut ship, &[], 0.3);
    ship.panels.intent(trigger, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
    watch(&mut s, &mut ship, &[], 1.0);
    ship.panels.intent(trigger, &Intent::Release, &s, &kind, &ship.store);
    watch(&mut s, &mut ship, &[], 0.3);
    let shots = fired(&mut ship);
    let tac = ship.tactical.as_ref().unwrap();
    let by = |load: Load| shots.iter().filter(|f| tac.weapons[usize::from(f.weapon)].load == load).map(|f| u32::from(f.count)).sum::<u32>();
    assert!((50..=80).contains(&by(Load::Round)) && by(Load::Guided) == 0, "un segundo de los dos cañones: {} proyectiles", by(Load::Round));
    assert_eq!(get(&ship, "canon_izq.municion") + get(&ship, "canon_der.municion"), 1200.0 - f64::from(by(Load::Round)));
    // the heat seekers chosen: one each pull, from one launcher and then the other
    set(&mut ship, &s, "combate/seleccion", 1.0);
    watch(&mut s, &mut ship, &[], 0.3);
    let mut from = Vec::new();
    for _ in 0..3 {
        ship.panels.intent(trigger, &Intent::Press { elem: 0 }, &s, &kind, &ship.store);
        watch(&mut s, &mut ship, &[], 0.8);
        ship.panels.intent(trigger, &Intent::Release, &s, &kind, &ship.store);
        watch(&mut s, &mut ship, &[], 0.3);
        let shots = fired(&mut ship);
        assert_eq!(shots.len(), 1, "un misil por pulsación");
        let w = &ship.tactical.as_ref().unwrap().weapons[usize::from(shots[0].weapon)];
        assert_eq!((w.load, w.ammo.as_str()), (Load::Guided, "dardo"));
        from.push(shots[0].weapon);
    }
    assert!(from[0] != from[1] && from[0] == from[2], "alternan los dos lanzadores: {from:?}");
    assert_eq!(get(&ship, "armas.municion"), 3.0);
    // the decoys by their own button, armed or not
    set(&mut ship, &s, "combate/armas_maestro", 0.0);
    let left = get(&ship, "cm.quedan");
    press(&mut ship, &mut s, "combate/cm_soltar");
    watch(&mut s, &mut ship, &[], 1.0);
    let shots = fired(&mut ship);
    assert!(shots.iter().all(|f| ship.tactical.as_ref().unwrap().weapons[usize::from(f.weapon)].load == Load::Decoy) && shots.len() == 2, "una nube y una bengala: {shots:?}");
    assert_eq!(get(&ship, "cm.quedan"), left - 2.0);
}

#[test]
fn its_autopilot_turns_it_toward_what_it_must_point_at() {
    let (mut s, mut ship, _) = alone();
    run_in(&mut s, &mut ship, &space(), 12.0);
    radar_on(&mut s, &mut ship, 1.0);
    assert!(get(&ship, "giroscopo.giro") > 0.9, "giróscopos a {:.2}", get(&ship, "giroscopo.giro"));
    // a target ahead and well to port; APUNTAR
    let target = thing(5, 3000.0, 6000.0);
    watch(&mut s, &mut ship, &[target], 1.0);
    press(&mut ship, &mut s, "principal/obj_cerca");
    set(&mut ship, &s, "combate/ap_modo", 2.0);
    watch(&mut s, &mut ship, &[target], 1.0);
    assert_eq!((get(&ship, "ap.activo"), get(&ship, "ap.modo")), (2.0, 2.0));
    // port is +X: the nose goes there by turning about +Y
    assert!(s.torque.y > 50.0, "par {:?}", s.torque);
    // the stick wins while it is held
    let (stick, kind) = (ctl(&ship, "mando/guinada"), ship.kind.clone());
    ship.panels.intent(stick, &Intent::Axis { axis: 0, value: -1.0 }, &s, &kind, &ship.store);
    watch(&mut s, &mut ship, &[target], 0.5);
    assert!(s.torque.y < -50.0, "con la palanca a estribor manda la palanca: {:?}", s.torque);
    ship.panels.intent(stick, &Intent::Axis { axis: 0, value: 0.0 }, &s, &kind, &ship.store);
    // a mode that needs a track says so when there is none
    press(&mut ship, &mut s, "principal/obj_soltar");
    watch(&mut s, &mut ship, &[], 4.0);
    assert_eq!(get(&ship, "ap.sin_objetivo"), 1.0);
    // FRENAR in free flight with its nacelles at cruise: it turns its engines against its speed
    set(&mut ship, &s, "combate/ap_modo", 0.0);
    set(&mut ship, &s, "gases/gondolas", 1.0);
    watch(&mut s, &mut ship, &[], 6.0);
    s.vel = DVec3::new(0.0, 0.0, 80.0);
    set(&mut ship, &s, "nav/nav_modo", 4.0);
    watch(&mut s, &mut ship, &[], 1.0);
    assert_eq!(get(&ship, "ap.activo"), 1.0);
    assert!(s.torque.length() > 50.0, "no gira para frenar: {:?}", s.torque);
}

#[test]
fn its_drum_turns_its_seat_goes_down_and_its_doors_open_with_the_ship_cold() {
    let (mut s, mut ship, kind) = alone();
    run(&mut s, &mut ship, 3.0);
    assert_eq!(get(&ship, "reactor.estado"), 0.0);
    // the drum: a third of a turn for each face
    for (face, at) in [(1.0, 0.5), (2.0, 1.0), (0.0, 0.0)] {
        set(&mut ship, &s, "gases/cara", face);
        run(&mut s, &mut ship, 3.0);
        assert!((get(&ship, "tambor.pos") - at).abs() < 0.02, "cara {face}: el tambor en {:.2}", get(&ship, "tambor.pos"));
    }
    // the seat: down through the belly with whoever sits on it, and up again
    let up = ship.seat_eyes(0);
    assert!((up - Vec3::from_array(kind.seats[0].def.ojos)).length() < 1e-4);
    press(&mut ship, &mut s, "exterior/plataforma");
    run(&mut s, &mut ship, 6.0);
    assert!(get(&ship, "plataforma.pos") > 0.98, "la plataforma en {:.2}", get(&ship, "plataforma.pos"));
    assert!((up.y - ship.seat_eyes(0).y - 1.06).abs() < 0.03, "los ojos bajan {:.2} m", up.y - ship.seat_eyes(0).y);
    press(&mut ship, &mut s, "sistemas/plataforma");
    run(&mut s, &mut ship, 6.0);
    assert!(get(&ship, "plataforma.pos") < 0.02);
    // the two doors of the bay on one order; the ramp's two lengths out with it
    press(&mut ship, &mut s, "exterior/compuertas");
    run(&mut s, &mut ship, 10.0);
    assert!(get(&ship, "rampa.abierta") > 0.9 && get(&ship, "visera.abierta") > 0.9, "rampa {:.2}, visera {:.2}", get(&ship, "rampa.abierta"), get(&ship, "visera.abierta"));
    assert!(get(&ship, "rampa_ext.pos") > 0.95 && get(&ship, "rampa_ext2.pos") > 0.95, "tramos {:.2} y {:.2}", get(&ship, "rampa_ext.pos"), get(&ship, "rampa_ext2.pos"));
    press(&mut ship, &mut s, "bahia/compuertas");
    run(&mut s, &mut ship, 10.0);
    assert!(get(&ship, "rampa.cerrada") > 0.5 && get(&ship, "visera.cerrada") > 0.5 && get(&ship, "rampa_ext2.pos") < 0.02);
}
