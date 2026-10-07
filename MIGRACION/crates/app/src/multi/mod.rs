//! Other players. Every game simulates the whole world; this keeps them in agreement through
//! `lunar_net` and a server that passes things on (`servidores/`, docs/MULTIJUGADOR.md):
//!
//! - **players**: ours goes out a few times a second (where the eyes are — in the frame of the
//!   ship it rides, or beside the ship it floats by —, where the body faces, its speed, a few
//!   flags) and the others come in the same way: each is drawn with a body of its own, animated
//!   here from that little (`body`);
//! - **ships**: each is simulated by whoever owns it (who sits at its controls; else the host);
//!   the others' copies follow it smoothly however fast it goes (`follow`), before the world
//!   steps, whoever rides them with them. A copy of a ship someone else owns is `remote`: what
//!   its own machines do is theirs;
//! - **controls**: a control worked by a hand is told as "this control is now at this value"
//!   and set the same on every copy; a ship made in play is made on every copy;
//! - **fire and damage**: a ship's weapons fire in every game from its copy there, but only the
//!   owner's rounds and missiles do anything: elsewhere they are twins, seen to fly and strike
//!   (`Blasts::fire_twin`). What is done to a structure every game has (`Structure::shared`) is
//!   decided where the round struck and told (`told::STRIKES`); every game, that one too, does it
//!   when it comes back, in the order the server passed it on, with the dice it was told with:
//!   every copy ends the same. What a player fires from their hands goes as `told::SEEN`, beside
//!   what they ride.
//!
//! Call `receive` early in a frame (before the ships run and the world steps) and `send` after.
//!
//! What is not told yet: loose cargo and crates, doors and clamps worked by hand at them.
mod follow;
pub mod told;

use crate::{
    aboard::Aboard,
    blasts::Seen,
    body::{Body, Stance},
    builds::{Builds, Strike},
    pilot::Pilot,
    rig::Rig,
    ships::Ships,
};
use follow::Kin;
use glam::{DVec3, Vec3};
use lunar_core::{
    anim::BodyScene,
    body::BodyRegistry,
    structure::{schedule::SimLevel, set::Structures},
};
use lunar_net::{Client, Event, Frame, PlayerState, Reader, RigidState, Status, Writer, flag, key};
use told::Named;

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

/// How far a ship we do not own may be from where its owner has it, at rest, before our copy
/// is moved (m, and the cosine of half the angle): less than this is our own copy at rest
/// agreeing (it is left asleep).
const AGREE: (f64, f32) = (0.004, 0.999_999);
/// A ship of ours at rest is still told of this often (s).
const REFRESH: f64 = 1.0;
/// A word of a ship older than this is not followed (s): its owner is gone silent, and our copy
/// goes on by itself until the next.
const STALE: f64 = 1.0;

pub struct Multi {
    client: Client,
    /// The ship's structure by its network id (the scenario's first, in order).
    ships: Vec<Option<u64>>,
    /// When each ship of ours was last told of (s).
    told: Vec<f64>,
    /// The ship our player floats by, if not aboard one.
    floating_by: Option<u64>,
    others: Vec<Other>,
    states: Vec<(u32, PlayerState)>,
    ship: RigidState,
    /// The ship whose controls we asked for (sat at them).
    flown: Option<u64>,
    /// Ships we made and the server has not numbered yet (their structures, in order).
    made: std::collections::VecDeque<u64>,
    /// The structures the scenario set round the site have ids below this (`Builds::scenario_end`).
    built: u64,
    /// What we decided to do to shared structures (reused), and how many strikes we told so far
    /// (each one's dice follow from it).
    strikes: Vec<Strike>,
    named: Vec<(Named, Strike)>,
    struck: u64,
    buf: Vec<u8>,
    /// What the others fired or set off, for this game to show now (and how long ago it was):
    /// taken by whoever shows it (`Blasts::show`).
    pub shown: Vec<(Seen, f32)>,
    /// What to tell the player: (text, 0 plain, 1 caution, 2 warning).
    pub said: Vec<(String, u8)>,
    /// Past the catching up on joining: what comes now is news.
    live: bool,
    /// The network's clock at the last `receive` (s): the moment our world is at when the next
    /// one comes (it has stepped to it since).
    last: Option<f64>,
}

/// The way a body faces from its turn (rad from `north` toward the right) where `up` is up.
fn facing(north: DVec3, up: DVec3, yaw: f64) -> DVec3 {
    north * yaw.cos() + north.cross(up) * yaw.sin()
}

impl Multi {
    /// Asks the server at `addr` to let us in as `name` (it answers in a moment: `status`).
    pub fn connect(addr: &str, name: &str, ships: &Ships, builds: &Builds) -> Result<Multi, String> {
        let client = Client::connect(addr, name, crate::BUILD, ships.list.len() as u32)?;
        Ok(Multi::with(client, ships, builds))
    }

    /// With a client already made (a test's, over a network of its own).
    pub fn with(client: Client, ships: &Ships, builds: &Builds) -> Multi {
        let list: Vec<Option<u64>> = ships.list.iter().map(|s| Some(s.structure)).collect();
        let n = list.len();
        Multi {
            client,
            told: vec![f64::MIN; n],
            floating_by: None,
            ships: list,
            others: Vec::new(),
            states: Vec::new(),
            ship: RigidState::default(),
            flown: None,
            made: std::collections::VecDeque::new(),
            built: builds.scenario_end,
            strikes: Vec::new(),
            named: Vec::new(),
            struck: 0,
            buf: vec![0; lunar_net::MAX_TELL.min(1 << 16)],
            shown: Vec::new(),
            said: Vec::new(),
            live: false,
            last: None,
        }
    }

    fn net_id(&self, structure: u64) -> Option<u64> {
        self.ships.iter().position(|s| *s == Some(structure)).map(|k| k as u64)
    }

    /// How the network names a structure of this game, if every game has it.
    fn name_of(&self, structure: u64) -> Option<Named> {
        match self.net_id(structure) {
            Some(k) => Some(Named::Ship(k)),
            None => (structure < self.built).then_some(Named::Built(structure)),
        }
    }

    /// The structure of this game the network names so.
    fn named(&self, n: Named) -> Option<u64> {
        match n {
            Named::Ship(k) => self.ships.get(k as usize).copied().flatten(),
            Named::Built(id) => (id < self.built).then_some(id),
        }
    }

    /// In a word or two: for the HUD and the menus.
    pub fn status(&self) -> (String, u8) {
        match self.client.status() {
            Status::Connecting => ("conectando".into(), 1),
            Status::Connected { players, ping_ms, .. } => (format!("{players} jug. · {ping_ms:.0} ms"), 0),
            Status::Failed(why) => (format!("sin conexión: {why}"), 2),
        }
    }

    /// Whether we are in the game (what we fire is to be told, what we strike decided in one
    /// order with the rest).
    pub fn connected(&self) -> bool {
        self.client.connected()
    }

    /// A control of a ship left at `value` by our own hand: told to the rest.
    pub fn control(&mut self, structure: u64, control: u16, value: f64) {
        if let Some(id) = self.net_id(structure) {
            let mut buf = [0u8; 32];
            let mut w = Writer::new(&mut buf);
            told::write_control(&mut w, id, control, value);
            if let Ok(n) = w.finish() {
                self.client.tell(&buf[..n]);
            }
        }
    }

    /// A door or a clamp of a ship worked by our own hand: told to the rest.
    pub fn act(&mut self, structure: u64, act: crate::aboard::Act) {
        if let Some(id) = self.net_id(structure) {
            let mut buf = [0u8; 32];
            let mut w = Writer::new(&mut buf);
            told::write_act(&mut w, id, act);
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
        told::write_spawn(&mut w, kind, &state);
        // (back to us too: the place it takes among those everyone made is its number)
        if let Ok(n) = w.finish()
            && self.client.tell_all(&buf[..n])
        {
            self.made.push_back(structure);
        }
    }

    /// Early in a frame, before the ships run and the world steps: what came in is done to our
    /// copy of the world (controls set, ships made, strikes done in their order), what the
    /// others fired is put in `shown`, and their ships are steered to where they are told.
    pub fn receive(&mut self, now: f64, ships: &mut Ships, builds: &mut Builds) {
        // (our world is still at the last frame's moment: what is told is placed for that one,
        // and steps on with the world)
        let at = self.last.unwrap_or(now);
        let dt = (now - at).clamp(0.0, 0.25);
        self.last = Some(now);
        self.client.update(now);
        // what is done in one order everywhere, and whose each ship is
        let on = self.client.connected();
        for s in &mut builds.set.list {
            let k = self.ships.iter().position(|x| *x == Some(s.id));
            s.shared = on && (k.is_some() || s.id < self.built);
            s.remote = on && k.is_some_and(|k| !self.client.owns_thing(k as u64));
        }
        // ---- what happened
        let events: Vec<Event> = self.client.events().collect();
        for e in events {
            match e {
                Event::Joined { name, .. } if self.live => self.said.push((format!("{name} se ha unido"), 0)),
                Event::Left { name, .. } if self.live => self.said.push((format!("{name} se ha ido"), 0)),
                Event::Joined { .. } | Event::Left { .. } | Event::Owner { .. } | Event::Host { .. } | Event::Direct { .. } => {}
                Event::Synced => self.live = true,
                Event::Told { by, data } => self.told(by, &data, ships, builds),
                Event::Hinted { data, .. } => {
                    let mut r = Reader::new(&data);
                    if r.u8() == Ok(told::SEEN) {
                        let now = self.client.server_time(at).unwrap_or(0.0);
                        let set = &builds.set;
                        let mut shown = std::mem::take(&mut self.shown);
                        // (a structure behind the world, as it will be when it catches up: now)
                        let find = |n: Named| self.named(n).and_then(|id| set.get(id)).map(|s| (s.pos + s.vel * (set.now - s.clock).max(0.0), s.vel));
                        let _ = told::read_seen(&mut r, now, find, &mut shown);
                        self.shown = shown;
                    }
                }
                Event::Chat { from, text } => {
                    let who = from.and_then(|id| self.client.name(id)).unwrap_or("Servidor").to_string();
                    self.said.push((format!("{who}: {text}"), 0));
                }
                Event::Disconnected { reason } => self.said.push((format!("Desconectado del servidor: {reason}"), 2)),
            }
        }
        self.follow(at, dt, ships, builds);
    }

    /// Something a player told everyone (we too, for what is echoed).
    fn told(&mut self, by: u32, data: &[u8], ships: &mut Ships, builds: &mut Builds) {
        let mut r = Reader::new(data);
        match r.u8() {
            Ok(told::SPAWN) => {
                let (Ok(kind), Ok(state)) = (r.str(64).map(str::to_string), RigidState::decode(&mut r)) else { return };
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
            Ok(told::CONTROL) => {
                let Ok((ship, control, value)) = told::read_control(&mut r) else { return };
                if let Some(Some(structure)) = self.ships.get(ship as usize) {
                    Aboard::remote(ships, &builds.set, *structure, usize::from(control), value);
                }
            }
            Ok(told::ACT) => {
                let Ok((ship, act)) = told::read_act(&mut r) else { return };
                if let Some(Some(structure)) = self.ships.get(ship as usize) {
                    Aboard::remote_act(ships, *structure, act);
                }
            }
            Ok(told::STRIKES) => {
                self.named.clear();
                let Ok(seed) = told::read_strikes(&mut r, &mut self.named) else { return };
                for (i, (n, s)) in self.named.iter().enumerate() {
                    let Some(id) = self.named(*n) else { continue };
                    let s = match *s {
                        Strike::Hit { hit, .. } => Strike::Hit { id, hit },
                        Strike::Blow { part, push, energy, .. } => Strike::Blow { id, part, push, energy },
                    };
                    builds.strike_done(&s, seed.wrapping_add(i as u64));
                }
            }
            _ => {}
        }
    }

    /// The ships of the others: our copies brought to where their owners have them at `now` (our
    /// world's moment), over `dt` (this frame). A copy far from us that the world does not step
    /// every frame (`Structure::sim`) is put there instead, and its clock says it is at the
    /// world's moment: it has nothing to catch up with when it next steps.
    fn follow(&mut self, now: f64, dt: f64, ships: &mut Ships, builds: &mut Builds) {
        let world = builds.set.now;
        for k in 0..self.ships.len() {
            let (Some(structure), false) = (self.ships[k], self.client.owns_thing(k as u64)) else { continue };
            let Some(s) = builds.set.list.iter_mut().find(|s| s.id == structure) else { continue };
            // (as its owner has it now: their last word carried on as it was going; too old a word
            // is not followed, and our copy goes on by itself)
            match self.client.rigid_carried(k as u64, now, STALE, &mut self.ship) {
                Some(age) if f64::from(age) <= STALE && self.ship.frame == Frame::World => {}
                _ => continue,
            }
            let st = &self.ship;
            let want = Kin { pos: st.pos, vel: st.vel.as_dvec3(), rot: st.rot, spin: st.spin };
            // (a copy at rest that agrees is left asleep)
            let still = want.vel.length_squared() < 1e-8 && want.spin.length_squared() < 1e-10;
            if !(still && s.pos.distance_squared(want.pos) <= AGREE.0 * AGREE.0 && s.rot.dot(want.rot).abs() >= AGREE.1) {
                let mut cur = Kin { pos: s.pos, vel: s.vel, rot: s.rot, spin: s.spin };
                if s.sim == SimLevel::Active && s.clock == world {
                    follow::steer(&mut cur, &want, dt);
                } else {
                    (cur, s.clock) = (want, world);
                }
                (s.pos, s.vel, s.rot, s.spin) = (cur.pos, cur.vel, cur.rot, cur.spin);
                (s.resting, s.still) = (false, 0.0);
            }
            if let Some(n) = ships.by_structure(structure) {
                ships.list[n].set_joints(&st.joints);
            }
        }
    }

    /// After the world stepped: our player, our ships, what we decided to do to shared
    /// structures and what we fired (`seen`, taken) out to the rest.
    #[allow(clippy::too_many_arguments)]
    pub fn send(&mut self, now: f64, pilot: &Pilot, eye_h: f64, head: [f64; 2], tool: u8, trigger: bool, outside: bool, ships: &Ships, builds: &mut Builds, seen: &mut Vec<Seen>) {
        let set = &builds.set;
        // ---- our player
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
        // (aboard a ship: in its frame; floating by one: beside it, so that we are drawn by it
        // however fast it goes)
        let mut ride = pilot.ride.and_then(|r| Some((self.net_id(r.id)?, r.local)));
        self.floating_by = match ride {
            Some(_) => None,
            None => follow::pick(pilot.position, self.floating_by, self.ship_places(set)),
        };
        if let (None, Some(k)) = (ride, self.floating_by)
            && let Some(s) = self.ships[k as usize].and_then(|id| set.get(id))
        {
            ride = Some((k, s.to_local(pilot.position)));
            flags |= flag::BESIDE;
        }
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
        // ---- our ships: while one moves, every time; at rest, now and then
        for k in 0..self.ships.len() {
            let (Some(structure), true) = (self.ships[k], self.client.owns_thing(k as u64)) else { continue };
            let Some(s) = set.get(structure) else { continue };
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
        // ---- what we decided to do to what every game has: told, and done when it comes back
        // (here too, in its place among what everyone decided)
        self.strikes.clear();
        builds.take_strikes(&mut self.strikes);
        if !self.strikes.is_empty() {
            self.named.clear();
            for s in &self.strikes {
                let id = match *s {
                    Strike::Hit { id, .. } | Strike::Blow { id, .. } => id,
                };
                if let Some(n) = self.name_of(id) {
                    self.named.push((n, *s));
                }
            }
            let seed = (u64::from(self.client.id().unwrap_or(0)) << 40) | (self.struck & ((1 << 40) - 1));
            let mut sent = false;
            if self.client.connected() {
                let mut w = Writer::new(&mut self.buf);
                told::write_strikes(&mut w, seed, &self.named);
                if let Ok(n) = w.finish() {
                    sent = self.client.tell_all(&self.buf[..n]);
                }
            }
            // (no one to tell after all: done here at once)
            if !sent {
                for (i, s) in self.strikes.iter().enumerate() {
                    builds.strike_done(s, seed.wrapping_add(i as u64));
                }
            }
            self.struck += self.strikes.len() as u64;
        }
        // ---- what we fired, beside what we ride
        if !seen.is_empty() {
            let from = match pilot.ride.and_then(|r| Some((self.name_of(r.id)?, builds.set.get(r.id)?))) {
                Some((named, s)) => told::From::Beside { named, pos: s.pos, vel: s.vel },
                None => told::From::World,
            };
            if let Some(stamp) = self.client.server_time(now) {
                for chunk in seen.chunks(told::SEEN_EACH) {
                    let mut w = Writer::new(&mut self.buf);
                    told::write_seen(&mut w, stamp, from, chunk);
                    if let Ok(n) = w.finish() {
                        self.client.hint(&self.buf[..n]);
                    }
                }
            }
            seen.clear();
        }
        self.client.update(now);
    }

    /// Every ship's number and where it is.
    fn ship_places<'a>(&'a self, set: &'a Structures) -> impl Iterator<Item = (u64, DVec3)> + 'a {
        self.ships.iter().enumerate().filter_map(move |(k, s)| Some((k as u64, set.get((*s)?)?.pos)))
    }

    /// `receive` and `send` at once (tests: one frame of a game with no world to step).
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, now: f64, pilot: &Pilot, eye_h: f64, head: [f64; 2], tool: u8, trigger: bool, outside: bool, ships: &mut Ships, builds: &mut Builds) {
        self.receive(now, ships, builds);
        self.send(now, pilot, eye_h, head, tool, trigger, outside, ships, builds, &mut Vec::new());
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
            let by = p.ride.and_then(|k| self.ships.get(k as usize).copied().flatten()).and_then(|id| set.get(id));
            // (riding a ship, or floating by one: where it is in our copy of the ship)
            let mut eye = by.map_or(p.pos, |s| s.to_world(p.local));
            let ride = by.filter(|_| p.flags & flag::BESIDE == 0);
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
mod tests;
