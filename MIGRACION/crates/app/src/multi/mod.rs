//! Other players. Every game simulates the whole world; this keeps them in agreement through
//! `lunar_net` and a server that passes things on (`servidores/`):
//! - our own player goes out a few times a second (where the eyes are — in the ship's frame when
//!   riding one —, where the body faces, its speed, a few flags) and the others come in the same
//!   way: each is drawn with a body of its own, animated here from that little (`body`);
//! - a ship is simulated by whoever owns it (who sits at its controls; else the first player) and
//!   its place, speed and joints go to the rest, who put their copy there;
//! - a control worked by a hand is told as "this control is now at this value", and set the same
//!   on every copy; a ship made in play is made on every copy.
//!
//! What is not told yet: loose cargo, damage, shots, doors worked by hand at the door.
use crate::{
    aboard::Aboard,
    body::{Body, Stance},
    builds::Builds,
    pilot::Pilot,
    rig::Rig,
    ships::Ships,
};
use glam::{DVec3, Vec3};
use lunar_core::{
    anim::BodyScene,
    body::BodyRegistry,
    structure::set::Structures,
};
use lunar_net::{Client, Event, Frame, PlayerState, Reader, RigidState, Status, Writer, flag, key};

/// What a body is made from: one for each of the others.
#[derive(Clone)]
pub struct BodySource {
    /// Its rig, as measured once (each body a copy).
    pub rig: Rig,
    /// Its mesh in the renderer, whole (seen from outside).
    pub whole: Option<u16>,
}

/// Another player, as drawn here.
struct Other {
    id: u32,
    body: Body,
    stance: Stance,
    /// Riding a ship: where they were in its frame last frame (their speed over its deck is
    /// taken from how that changes: what is told of their speed is not fresh while they ride).
    local: Option<Vec3>,
    /// Told of this frame (the ones not told of are gone).
    here: bool,
}

/// How far a ship we do not own may be from where its owner has it before our copy is put there
/// (m, and the cosine of half the angle): less than this is our own copy at rest agreeing.
const AGREE: (f64, f32) = (0.004, 0.999_999);
/// A ship of ours at rest is still told of this often (s).
const REFRESH: f64 = 1.0;
/// What is told to everyone: a control set, a ship made in play.
const TOLD_CONTROL: u8 = 1;
const TOLD_SPAWN: u8 = 2;

pub struct Multi {
    client: Client,
    /// The ship's structure by its network id (the scenario's first, in order).
    ships: Vec<Option<u64>>,
    /// When each ship of ours was last told of (s).
    told: Vec<f64>,
    others: Vec<Other>,
    states: Vec<(u32, PlayerState)>,
    ship: RigidState,
    /// The ship whose controls we asked for (sat at them).
    flown: Option<u64>,
    /// Ships we made and the server has not numbered yet (their structures, in order).
    made: std::collections::VecDeque<u64>,
    /// What to tell the player: (text, 0 plain, 1 caution, 2 warning).
    pub said: Vec<(String, u8)>,
    /// Past the catching up on joining: what comes now is news.
    live: bool,
}

/// The way a body faces from its turn (rad from `north` toward the right) where `up` is up.
fn facing(north: DVec3, up: DVec3, yaw: f64) -> DVec3 {
    north * yaw.cos() + north.cross(up) * yaw.sin()
}

impl Multi {
    /// Asks the server at `addr` to let us in as `name` (it answers in a moment: `status`).
    pub fn connect(addr: &str, name: &str, ships: &Ships) -> Result<Multi, String> {
        let client = Client::connect(addr, name, crate::BUILD, ships.list.len() as u32)?;
        Ok(Multi::with(client, ships))
    }

    /// With a client already made (a test's, over a network of its own).
    pub fn with(client: Client, ships: &Ships) -> Multi {
        let list: Vec<Option<u64>> = ships.list.iter().map(|s| Some(s.structure)).collect();
        Multi { client, told: vec![f64::MIN; list.len()], ships: list, others: Vec::new(), states: Vec::new(), ship: RigidState::default(), flown: None, made: std::collections::VecDeque::new(), said: Vec::new(), live: false }
    }

    fn net_id(&self, structure: u64) -> Option<u64> {
        self.ships.iter().position(|s| *s == Some(structure)).map(|k| k as u64)
    }

    /// In a word or two: for the HUD and the menus.
    pub fn status(&self) -> (String, u8) {
        match self.client.status() {
            Status::Connecting => ("conectando".into(), 1),
            Status::Connected { players, ping_ms, .. } => (format!("{players} jug. · {ping_ms:.0} ms"), 0),
            Status::Failed(why) => (format!("sin conexión: {why}"), 2),
        }
    }

    /// A control of a ship left at `value` by our own hand: told to the rest.
    pub fn control(&mut self, structure: u64, control: u16, value: f64) {
        if let Some(id) = self.net_id(structure) {
            let mut buf = [0u8; 32];
            let mut w = Writer::new(&mut buf);
            w.u8(TOLD_CONTROL);
            w.var(id);
            w.var(u64::from(control));
            w.f64(value);
            if let Ok(n) = w.finish() {
                self.client.tell(&buf[..n]);
            }
        }
    }

    /// A ship we made in play (`structure`, of kind `kind`): told to the rest, who make theirs.
    pub fn made(&mut self, kind: &str, structure: u64, set: &Structures, ships: &Ships) {
        let Some(s) = set.get(structure) else { return };
        let joints = ships.by_structure(structure).map(|n| ships.list[n].joints.iter().map(|j| j.q as f32).collect()).unwrap_or_default();
        // (the wire still has a byte for the body a thing is at: nothing is ruled by it any more)
        let state = RigidState { id: 0, body: 0, frame: Frame::World, pos: s.pos, rot: s.rot, vel: s.vel.as_vec3(), spin: s.spin, joints };
        let mut buf = [0u8; 1200];
        let mut w = Writer::new(&mut buf);
        w.u8(TOLD_SPAWN);
        w.str(kind);
        state.encode(&mut w);
        // (back to us too: the place it takes among those everyone made is its number)
        if let Ok(n) = w.finish()
            && self.client.tell_all(&buf[..n])
        {
            self.made.push_back(structure);
        }
    }

    /// A frame of it: our player and our ships out, everything else in and done to our copy of
    /// the world. `now`: the network's clock (`lunar_net::now`); `head`: the free look's yaw and
    /// pitch; `tool`: what is in hand (0: nothing).
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, now: f64, pilot: &Pilot, eye_h: f64, head: [f64; 2], tool: u8, trigger: bool, outside: bool, ships: &mut Ships, builds: &mut Builds) {
        // ---- ours, out
        let mut flags = 0;
        for (on, bit) in [(pilot.grounded, flag::GROUNDED), (pilot.crouched(), flag::CROUCHED), (pilot.pack_on, flag::PACK), (pilot.pack_thrust() > 0.0, flag::THRUSTING), (pilot.lamps, flag::LAMP), (trigger, flag::TRIGGER), (outside, flag::THIRD_PERSON)] {
            if on {
                flags |= bit;
            }
        }
        let seat = pilot.seat.and_then(|s| Some((self.net_id(s.structure)?, s.index as u8)));
        if let Some(s) = pilot.seat
            && ships.by_structure(s.structure).is_some_and(|n| ships.list[n].kind.seats.get(s.index).is_some_and(|x| !x.def.mandos.is_empty()))
        {
            flags |= flag::AT_CONTROLS;
        }
        let ride = pilot.ride.and_then(|r| Some((self.net_id(r.id)?, r.local)));
        // (who sits at a ship's controls asks for it: it is theirs to simulate)
        let flown = seat.filter(|_| flags & flag::AT_CONTROLS != 0).map(|s| s.0);
        if flown != self.flown {
            if let Some(was) = self.flown {
                self.client.release(key::thing(was));
            }
            if let Some(now) = flown {
                self.client.claim(key::thing(now));
            }
            self.flown = flown;
        }
        self.client.set_player(&PlayerState {
            body: pilot.body as u8,
            pos: pilot.position,
            ride: ride.map(|r| r.0),
            local: ride.map_or(Vec3::ZERO, |r| r.1),
            // (seated: the head's turn in the seat; on foot: from the reference every game works
            // out the same)
            yaw: if pilot.seat.is_some() { pilot.yaw } else { pilot.told_heading() } as f32,
            pitch: pilot.pitch as f32,
            head: [head[0] as f32, head[1] as f32],
            vel: pilot.velocity().as_vec3(),
            flags,
            seat,
            tool,
            eye_h: eye_h as f32,
            ..PlayerState::default()
        });
        // (a ship of ours: while it moves, every time; at rest, now and then)
        for k in 0..self.ships.len() {
            let (Some(structure), true) = (self.ships[k], self.client.owns_thing(k as u64)) else { continue };
            let Some(s) = builds.set.get(structure) else { continue };
            if s.resting && now - self.told[k] < REFRESH {
                continue;
            }
            self.told[k] = now;
            let st = &mut self.ship;
            (st.id, st.body, st.frame, st.pos, st.rot, st.vel, st.spin) = (k as u64, 0, Frame::World, s.pos, s.rot, s.vel.as_vec3(), s.spin);
            st.joints.clear();
            if let Some(n) = ships.by_structure(structure) {
                st.joints.extend(ships.list[n].joints.iter().map(|j| j.q as f32));
            }
            self.client.set_rigid(&self.ship);
        }
        self.client.update(now);
        // ---- what happened
        let events: Vec<Event> = self.client.events().collect();
        for e in events {
            match e {
                Event::Joined { name, .. } if self.live => self.said.push((format!("{name} se ha unido"), 0)),
                Event::Left { name, .. } if self.live => self.said.push((format!("{name} se ha ido"), 0)),
                Event::Joined { .. } | Event::Left { .. } | Event::Owner { .. } | Event::Host { .. } | Event::Hinted { .. } | Event::Direct { .. } => {}
                Event::Synced => self.live = true,
                Event::Told { by, data } => {
                    let mut r = Reader::new(&data);
                    match r.u8() {
                        Ok(TOLD_SPAWN) => {
                            let (Ok(kind), Ok(state)) = (r.str(64).map(str::to_string), RigidState::decode(&mut r)) else { continue };
                            // (its number: its place among the ships everyone made, the same for all)
                            let k = self.ships.len();
                            self.ships.push(None);
                            self.told.push(f64::MIN);
                            // ours: the one we made already; anyone else's: made here now
                            let ours = Some(by) == self.client.id();
                            self.ships[k] = match ours.then(|| self.made.pop_front()).flatten() {
                                Some(structure) => Some(structure),
                                None => ships.spawn_free(builds, &kind, state.pos, state.rot).ok(),
                            };
                            if let (false, Some(name)) = (ours || !self.live, self.client.name(by)) {
                                self.said.push((format!("{name} ha puesto una nave ({kind})"), 0));
                            }
                        }
                        Ok(TOLD_CONTROL) => {
                            let (Ok(ship), Ok(control), Ok(value)) = (r.var(), r.var16(), r.f64()) else { continue };
                            if let Some(Some(structure)) = self.ships.get(ship as usize) {
                                Aboard::remote(ships, &builds.set, *structure, usize::from(control), value);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Chat { from, text } => {
                    let who = from.and_then(|id| self.client.name(id)).unwrap_or("Servidor").to_string();
                    self.said.push((format!("{who}: {text}"), 0));
                }
                Event::Disconnected { reason } => self.said.push((format!("Desconectado del servidor: {reason}"), 2)),
            }
        }
        // ---- the ships of the others: our copies where their owners have them
        for k in 0..self.ships.len() {
            let (Some(structure), false) = (self.ships[k], self.client.owns_thing(k as u64)) else { continue };
            // (as its owner has it now: their last word carried on with its speed)
            if self.client.rigid_now(k as u64, now, &mut self.ship).is_none() {
                continue;
            }
            let st = &self.ship;
            let Some(s) = builds.set.list.iter_mut().find(|s| s.id == structure) else { continue };
            if s.pos.distance_squared(st.pos) > AGREE.0 * AGREE.0 || s.rot.dot(st.rot).abs() < AGREE.1 {
                (s.pos, s.rot, s.vel, s.spin) = (st.pos, st.rot, st.vel.as_dvec3(), st.spin);
                (s.resting, s.still) = (false, 0.0);
            }
            if let Some(n) = ships.by_structure(structure) {
                ships.list[n].set_joints(&st.joints);
            }
        }
    }

    /// The others, as bodies: each made the first time it is told of, moved from what is told,
    /// and drawn. `g`: how much things weigh where each is.
    pub fn bodies(&mut self, now: f64, dt: f64, source: &BodySource, set: &Structures, bodies: &BodyRegistry, ships: &Ships, out: &mut BodyScene) {
        self.states.clear();
        self.client.players(now, &mut self.states);
        for o in &mut self.others {
            o.here = false;
        }
        for (id, p) in &self.states {
            let ride = p.ride.and_then(|k| self.ships.get(k as usize).copied().flatten()).and_then(|id| set.get(id));
            // (riding a ship: where it is in our copy of the ship)
            let mut eye = ride.map_or(p.pos, |s| s.to_world(p.local));
            let body = bodies.get(usize::from(p.body).min(bodies.len().saturating_sub(1)) as lunar_core::body::BodyId);
            let mut up = body.up(eye);
            let mut ahead = facing(body.turn_from(up), up, f64::from(p.yaw));
            // seated: the seat's eyes and the way it faces, up the ship's up
            let seat = p.seat.and_then(|(k, i)| {
                let structure = self.ships.get(k as usize).copied().flatten()?;
                let (s, n) = (set.get(structure)?, ships.by_structure(structure)?);
                let d = &ships.list[n].kind.seats.get(usize::from(i))?.def;
                let h = d.rumbo.to_radians();
                Some((s.to_world(Vec3::from_array(d.ojos)), (s.rot * Vec3::Y).as_dvec3(), (s.rot * Vec3::new(h.sin(), 0.0, h.cos())).as_dvec3()))
            });
            if let Some((at, u, a)) = seat {
                (eye, up, ahead) = (at, u, a);
            }
            let known = self.others.iter().position(|o| o.id == *id);
            // (over a ship's deck: how fast they go is how fast their place in it changes)
            let vel = match (ride, known.and_then(|k| self.others[k].local)) {
                (Some(s), Some(was)) if dt > 1e-6 => (s.rot * ((p.local - was) / dt as f32)).as_dvec3().clamp_length_max(12.0),
                (Some(_), None) => DVec3::ZERO,
                _ => p.vel.as_dvec3(),
            };
            let stance = Stance {
                eye,
                up,
                ahead: (ahead - up * ahead.dot(up)).normalize_or(up.any_orthonormal_vector()),
                eye_h: if seat.is_some() { 1.2 } else { f64::from(p.eye_h) },
                vel,
                grounded: p.flags & flag::GROUNDED != 0,
                g: lunar_core::structure::weight::felt(bodies.field(eye).pull, eye, ride, true, ride.is_some_and(|s| s.in_rooms(s.to_local(eye)))).length(),
                ride: ride.map(|s| s.id),
                seated: seat.is_some(),
                inside: false,
                own_eyes: false,
            };
            let k = match known {
                Some(k) => k,
                None => {
                    self.others.push(Other { id: *id, body: Body::new(source.rig.clone(), source.whole, source.whole), stance, local: None, here: true });
                    self.others.len() - 1
                }
            };
            let o = &mut self.others[k];
            (o.stance, o.here, o.local) = (stance, true, ride.map(|_| p.local));
            o.body.update(dt, &o.stance, set, bodies, [None, None]);
            o.body.show(out, &o.stance);
        }
        self.others.retain(|o| o.here);
    }

    /// How many others are drawn.
    #[cfg(test)]
    pub fn others(&self) -> usize {
        self.others.len()
    }

    /// Whether the ship on `structure` is ours to simulate (and tell the rest of).
    #[cfg(test)]
    pub fn owns(&self, structure: u64) -> bool {
        self.net_id(structure).is_some_and(|k| self.client.owns_thing(k))
    }

    /// Where each of the others is (their eyes) and what they are called: for whoever writes
    /// their names.
    pub fn names(&self) -> impl Iterator<Item = (DVec3, &str)> {
        self.others.iter().filter_map(|o| Some((o.stance.eye, self.client.name(o.id)?)))
    }

}

impl Drop for Multi {
    /// Leaving: the server is told, so the rest see us go at once.
    fn drop(&mut self) {
        self.client.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content::Defs, pilot::Seat};
    use lunar_core::scene::Site;
    use lunar_net::{MemoryNet, Server, ServerConfig};
    use std::sync::Arc;

    /// A game without its picture: the scenario's ships standing, a player, its end of the network.
    struct Game {
        ships: Ships,
        builds: Builds,
        pilot: Pilot,
        multi: Multi,
    }

    fn game(defs: &Defs, client: Client) -> (Game, Arc<BodyRegistry>) {
        let bodies = defs.system.bodies.clone();
        let sc = &defs.scenario;
        let site = Site::from_def(&sc.site, &bodies).unwrap();
        let effects: Vec<&str> = defs.effects.explosions.iter().map(|(id, _)| id.as_str()).collect();
        let mut builds = Builds::new(defs.structures.clone(), sc, &site, &bodies, &effects).unwrap();
        let mut ships = Ships::new(defs.ships.clone(), defs.font.0.clone());
        for a in &sc.ships {
            ships.spawn(&mut builds, &bodies, &a.ship, site.body, site.at(a.east, a.north), a.yaw.to_radians()).unwrap();
        }
        let multi = Multi::with(client, &ships);
        (Game { ships, builds, pilot: Pilot::new(bodies.clone(), &site, sc.player), multi }, bodies)
    }

    #[test]
    fn two_games_through_a_server_agree_on_players_controls_and_ships() {
        let root = crate::root();
        let defs = Defs::load(&root.join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let rig: crate::rig::RigDef = lunar_core::defs::load(&root.join("assets/defs/rigs/astronauta.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let Ok(model) = crate::rig::Rigged::load(&root.join("assets/models").join(format!("{}.glb", rig.modelo))) else { return };
        let source = BodySource { rig: Rig::new(rig, &model).unwrap(), whole: None };
        // a server and two games, on a network of their own
        let net = MemoryNet::new(11);
        let mut link = net.endpoint();
        let addr = link.addr();
        let mut server = Server::new(ServerConfig::default());
        let client = |name: &str, ships: u32| Client::with_transport(Box::new(net.endpoint()), addr, name, crate::BUILD, ships);
        let (mut a, bodies) = game(&defs, client("nadie", 0));
        let (mut b, _) = game(&defs, client("nadie", 0));
        // (the scenario's ships and whatever they carry docked: the same on both)
        let count = a.ships.list.len() as u32;
        assert!(count >= defs.scenario.ships.len() as u32 && count == b.ships.list.len() as u32);
        a.multi = Multi::with(client("Ana", count), &a.ships);
        b.multi = Multi::with(client("Berto", count), &b.ships);
        let mut now = 50.0;
        let dt = 1.0 / 60.0;
        let mut scene = BodyScene::default();
        let mut run = |a: &mut Game, b: &mut Game, secs: f64, now: &mut f64, scene: &mut BodyScene| {
            for _ in 0..((secs / dt).round() as usize).max(1) {
                *now += dt;
                net.set_time(*now);
                server.update(*now, &mut link);
                let _ = server.events().count();
                for g in [&mut *a, &mut *b] {
                    g.multi.frame(*now, &g.pilot, 1.75, [0.0, 0.0], 0, false, false, &mut g.ships, &mut g.builds);
                    scene.clear();
                    g.multi.bodies(*now, dt, &source, &g.builds.set, &bodies, &g.ships, scene);
                }
            }
        };
        // ---- each sees the other where they are, by their name
        let body = bodies.get(a.pilot.body);
        let up = body.up(a.pilot.position);
        let east = up.any_orthonormal_vector();
        let feet = body.above_ground(body.up(a.pilot.position + east * 7.0), 0.0);
        a.pilot.put(feet, body.up(feet));
        run(&mut a, &mut b, 2.0, &mut now, &mut scene);
        assert_eq!((a.multi.others(), b.multi.others()), (1, 1), "{:?} / {:?}", a.multi.status(), b.multi.status());
        let (eye, name) = b.multi.names().next().map(|(e, n)| (e, n.to_string())).unwrap();
        assert_eq!(name, "Ana");
        assert!(eye.distance(a.pilot.position) < 0.02, "Berto has Ana {:.3} m from where she is", eye.distance(a.pilot.position));
        assert!(!scene.bodies.is_empty(), "the other is drawn");
        // ---- a control worked on one copy is set the same on the other
        let structure = a.ships.list[0].structure;
        let (k, value) = {
            let (sh, s) = (&mut a.ships.list[0], a.builds.set.get(structure).unwrap());
            let kind = sh.kind.clone();
            let k = (0..sh.panels.controls.len()).find(|&k| sh.panels.intent(k, &lunar_controls::Intent::Press { elem: 0 }, s, &kind, &sh.store).changed).expect("a control that a press changes");
            let c = &sh.panels.controls[k];
            (k, c.mech.value(&c.st))
        };
        a.multi.control(structure, k as u16, value);
        run(&mut a, &mut b, 1.0, &mut now, &mut scene);
        let theirs = {
            let c = &b.ships.list[0].panels.controls[k];
            c.mech.value(&c.st)
        };
        assert!((theirs - value).abs() < 1e-9, "control {k}: {value} on one copy, {theirs} on the other");
        // ---- a ship made in one game is made in the other, once in each
        let at = a.pilot.position + up * 40.0;
        let made = a.ships.spawn_free(&mut a.builds, "abejorro", at, glam::Quat::IDENTITY).unwrap();
        a.multi.made("abejorro", made, &a.builds.set, &a.ships);
        run(&mut a, &mut b, 1.0, &mut now, &mut scene);
        assert_eq!((a.ships.list.len() as u32, b.ships.list.len() as u32), (count + 1, count + 1));
        // ---- who sits at the controls of a ship owns it, and where they take it the other copy goes
        let seat = b.ships.list[0].kind.seats.iter().position(|s| !s.def.mandos.is_empty()).expect("a seat with controls");
        let d = b.ships.list[0].kind.seats[seat].def.clone();
        let flown = b.ships.list[0].structure;
        b.pilot.sit(&b.builds.set, Seat { structure: flown, index: seat, eyes: Vec3::from_array(d.ojos), heading: d.rumbo.to_radians(), exit: Vec3::from_array(d.salida) });
        run(&mut a, &mut b, 1.0, &mut now, &mut scene);
        assert!(b.multi.owns(flown) && !a.multi.owns(structure), "whoever sits at the controls owns the ship");
        let lift = up * 6.0;
        let before = a.builds.set.get(structure).unwrap().pos;
        for _ in 0..90 {
            // (held there by its owner: that game is what says where it is)
            let s = b.builds.set.list.iter_mut().find(|s| s.id == flown).unwrap();
            (s.pos, s.vel, s.resting) = (before + lift, DVec3::ZERO, false);
            run(&mut a, &mut b, dt, &mut now, &mut scene);
        }
        let ours = a.builds.set.get(structure).unwrap().pos;
        assert!((ours - before - lift).length() < 0.6, "the owner has it {:.2} m up, the other copy is {:.2} m from there", lift.length(), (ours - before - lift).length());
        // (and the seated player is seen, sat in it)
        assert_eq!(a.multi.others(), 1);
    }

    #[test]
    fn a_body_faces_where_its_player_does() {
        // (as the player's own: with north -z where up is +y, east is +x)
        let up = DVec3::Y;
        assert!((facing(DVec3::NEG_Z, up, 0.0) - DVec3::NEG_Z).length() < 1e-12);
        assert!((facing(DVec3::NEG_Z, up, std::f64::consts::FRAC_PI_2) - DVec3::X).length() < 1e-12);
        // level wherever up is
        let up = DVec3::new(0.4, 0.5, -0.3).normalize();
        let north = up.any_orthonormal_vector();
        for yaw in [0.0, 1.0, 2.5, -2.0] {
            let f = facing(north, up, yaw);
            assert!(f.dot(up).abs() < 1e-12 && (f.length() - 1.0).abs() < 1e-12);
        }
    }
}
