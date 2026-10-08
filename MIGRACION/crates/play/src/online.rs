//! A player's game over a server that has the game (`host`): it predicts its own body and
//! simulates everything else as the server does, and the server's word puts it right
//! (`docs/PLAN_AUTORITATIVO.md` §3.2-3.3).
//!
//! - **ahead of the server**: this game steps a little ahead of the server (half the way there
//!   and a few steps more), so that what its player asks of a step reaches the server before the
//!   server takes that step. The server says how early each came (`Snap::ahead`) and this game's
//!   clock runs a hair faster or slower until it is `TARGET` steps early;
//! - **its body, predicted**: each step goes with what was asked of it (`Cmd`, sent several times
//!   over) and what it made of the body (`Pilot::summary`); the server compares, and if they differ
//!   sends the body as it has it at a step (`Event::Correct`): put so, and stepped again with what
//!   was asked since (`Game::step_alone`); what the eye would jump is taken out over a moment
//!   (`Player::offset`);
//! - **everything else, simulated and put right**: every structure runs here as it runs there
//!   (its physics, its machines); where each was at every step is kept (`Track`), and a snapshot
//!   of the server's says how far that was off at its step: a share of that is put right now (and
//!   in what is kept, so it is not put right twice). However fast it goes, what is off is only
//!   what the two simulations differ by, not how far it went meanwhile;
//! - **what happened**: what came into being or is gone, what was struck (done here with the
//!   server's dice, so the pieces are the same: a piece made here takes the server's name when
//!   it says it, by its lineage), what was let fly, the ships' controls and systems.
use crate::{
    blasts::Seen,
    builds::Strike,
    controls,
    game::{Game, Player, STEP, Say},
    hands::Hands,
    host,
    net::{self, Act, Check, Cmd, EVENTS, Event, REPEAT, SNAP, Snap},
    pilot::Pilot,
    seats::Drive,
    told::{self, Named},
};
use glam::{Affine3A, DVec3, Quat, Vec3};
use lunar_core::{
    scenario::PlayerDef,
    structure::{
        schedule::{SimLevel, coast, coasted},
        set::Structures,
    },
};
use lunar_net::{Client, Event as NetEvent, PlayerState, Reader, Status};
use lunar_ship::sync::{self, Digest};
use std::collections::VecDeque;

/// Steps of what was asked, and of where each structure was, kept.
pub const RING: usize = 128;
/// How many steps early this game's commands should reach the server.
pub const TARGET: f64 = 3.0;
/// What this game makes of its own (pieces off what the server struck, before the server names
/// them) is named from here up.
pub const LOCAL_IDS: u64 = 1 << 40;
/// A piece made here that the server has not named after this many steps is not one it has.
const ORPHAN: u64 = 90;
/// Steps a frame takes at most (catching up).
const MOST_STEPS: u32 = 8;
/// Off by less than this (m, m/s, rad, rad/s) is off by no more than a snapshot's rounding
/// (`lunar_net::RigidState`: 1/4096 m, 1/1024 m/s, the turn's 15 bits, 1/4096 rad/s).
const AGREE_POS: f64 = 5e-4;
const AGREE_VEL: f64 = 3e-3;
const AGREE_TURN: f32 = 2.5e-4;
const AGREE_SPIN: f32 = 1e-3;
/// What is off by more than this (m, rad) is put where the server has it at once.
const SNAP_OFF: f64 = 40.0;
const SNAP_TURN: f32 = 0.6;
/// What is off is taken out with this half-life (s).
const HALF_LIFE: f64 = 0.1;
/// The eye's jump after a correction is taken out with this half-life (s); one bigger than this
/// (m) is taken at once (a correction across the map is a teleport).
const EYE_HALF_LIFE: f64 = 0.12;
const EYE_SNAP: f64 = 4.0;
/// How long before asking again for all of a structure that does not agree (steps).
const RESYNC_EVERY: u64 = 120;
/// A way to act along nearer the body's own look than this (the cosine of a tenth of a degree) is
/// the look itself.
const AIM_SAME: f64 = 0.999_998_5;

/// Where a structure was at a step, and what whoever it carries weighs by there: how it sped up,
/// how far its own gravity was on, how its moving parts were (`Structure::pose`, a counter: the
/// bones themselves are in `Online::bones`, kept when they change).
#[derive(Clone, Copy, Debug, Default)]
struct Pose {
    step: u64,
    pos: DVec3,
    vel: DVec3,
    rot: Quat,
    spin: Vec3,
    acc: DVec3,
    on: f32,
    bones: u64,
}

/// Where a structure was at each of the last steps (`RING`, each at `step % RING`), when it was
/// last put right, and which of its poses was last kept (`Online::bones`).
struct Track {
    id: u64,
    poses: Box<[Pose; RING]>,
    fixed: u64,
    kept: u64,
}

/// A structure's moving parts as they were (`Structure::bones` but the first), by its pose
/// counter, from step `step`: what a body's steps are done again against.
struct Bones {
    id: u64,
    pose: u64,
    step: u64,
    at: Vec<Affine3A>,
}

/// What a structure was before it was put as it was at a step to do a body's step again: put
/// back after (its pose counter, and the one put now).
struct Was {
    id: u64,
    rot: Quat,
    vel: DVec3,
    spin: Vec3,
    acc: DVec3,
    on: f32,
    pose: u64,
    put: u64,
    bones: Vec<Affine3A>,
}

/// Counters since this game joined.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnlineStats {
    pub steps: u64,
    /// Times the body was put right, and steps stepped again for it.
    pub corrections: u64,
    pub replayed: u64,
    /// Snapshots taken, structures put right from them (and put there at once).
    pub snaps: u64,
    pub fixed: u64,
    pub snapped: u64,
    /// Structures made and forgotten; pieces of ours named by the server; ours it never named.
    pub made: u64,
    pub gone: u64,
    pub renamed: u64,
    pub orphans: u64,
    /// Strikes done as told, structures asked for again, acts refused, messages that made no sense.
    pub strikes: u64,
    pub resyncs: u64,
    pub denied: u64,
    pub garbled: u64,
    /// We came back to the body that waited for us.
    pub back: bool,
}

pub struct Online {
    pub client: Client,
    def: PlayerDef,
    /// In (`Event::Hello` came): our id on the network.
    pub you: Option<u32>,
    pub region: u32,
    pub stats: OnlineStats,
    /// Which of the seat's keys are held now (`net::Cmd::keys`), as whoever reads the keys says;
    /// and what the others need to draw us that no step makes (`net::Cmd`).
    pub keys: u32,
    pub head: [f32; 2],
    pub tool: u8,
    pub trigger: bool,
    pub outside: bool,
    pub gesture: u8,
    /// What the player reads: what the ships said, what was refused (text, level).
    pub said: Vec<(String, u8)>,
    /// The others, as the newest snapshot has them, and the step it is of.
    pub others: Vec<(u32, PlayerState)>,
    pub others_step: u64,
    /// The key our body is ours by if we are cut off: what to come back with (`Act::Back`).
    pub key: Option<u64>,
    /// Coming back with this key (`back`): what the server answers is awaited before anything
    /// is stepped; and the key this connection was given, if there is no body to come back to.
    back: Option<u64>,
    awaiting: bool,
    given: u64,
    /// Times our body was put right (`Check::fixes`).
    fixes: u8,
    cmds: Vec<Cmd>,
    /// The clock: steps owed, how fast it runs, how early our commands come (smoothed); when each
    /// step's command went (our clock), how long there and back is (s), and when the clock was
    /// last set at a stroke (it is not again before what that did can be seen).
    due: f64,
    rate: f64,
    early: f64,
    sent: Vec<f64>,
    pub rtt: f64,
    set_at: f64,
    now: f64,
    /// The clock has been set (by the first snapshot): steps are taken, the first of them `first`.
    clock: bool,
    first: u64,
    /// The newest correction come, not applied yet: the step it is of and the body.
    pending: Option<(u64, Vec<u8>)>,
    tracks: Vec<Track>,
    /// The moving parts of what moves, as each changed over the last steps (oldest first: what is
    /// past `RING` is reused); and (reused) what is put back after a body's steps are done again.
    bones: VecDeque<Bones>,
    was: Vec<Was>,
    /// Pieces made here not named yet, and when.
    ours: Vec<(u64, u64)>,
    drive: Drive,
    /// What was last said of the seat and the hands (what changed of them is said: `Act`).
    seated: Option<(u64, usize)>,
    holding: Option<(u64, f64)>,
    /// When each structure was last asked for again, and how many times in a row it did not
    /// agree (once is a word that overtook the one that made it).
    asked: Vec<(u64, u64, u8)>,
    // (reused)
    snap: Snap,
    snap_new: bool,
    events: Vec<Event>,
    out: Vec<u8>,
    moved: Vec<(u64, usize, f64)>,
    died: Vec<u32>,
    strikes: Vec<(Named, Strike)>,
    poses: Vec<(Named, Vec<glam::Affine3A>)>,
    seen: Vec<(Seen, f32)>,
}

impl Online {
    /// A game over `client` (connecting, or in), for a player of `def`.
    pub fn new(client: Client, def: PlayerDef) -> Online {
        Online {
            client,
            def,
            you: None,
            region: 0,
            stats: OnlineStats::default(),
            keys: 0,
            head: [0.0; 2],
            tool: 0,
            trigger: false,
            outside: false,
            gesture: 0,
            said: Vec::new(),
            others: Vec::new(),
            others_step: 0,
            key: None,
            back: None,
            awaiting: false,
            given: 0,
            fixes: 0,
            cmds: vec![Cmd::default(); RING],
            due: 0.0,
            rate: 1.0,
            early: TARGET,
            sent: vec![f64::NAN; RING],
            rtt: 0.1,
            set_at: f64::MIN,
            now: 0.0,
            clock: false,
            first: 0,
            pending: None,
            tracks: Vec::new(),
            bones: VecDeque::new(),
            was: Vec::new(),
            ours: Vec::new(),
            drive: Drive::default(),
            seated: None,
            holding: None,
            asked: Vec::new(),
            snap: Snap::default(),
            snap_new: false,
            events: Vec::new(),
            out: Vec::new(),
            moved: Vec::new(),
            died: Vec::new(),
            strikes: Vec::new(),
            poses: Vec::new(),
            seen: Vec::new(),
        }
    }

    /// A game over `client` for who was cut off and comes back with `key` (`Online::key` of the
    /// game they had): their body as it waited for them, if it still does. The game they had may
    /// be kept: what is not of the scenario is forgotten on coming in, and told again.
    pub fn back(client: Client, def: PlayerDef, key: u64) -> Online {
        let mut o = Online::new(client, def);
        // (ours still: if this connection is lost too, it is the one to try again with)
        (o.back, o.key) = (Some(key), Some(key));
        o
    }

    pub fn status(&self) -> Status {
        self.client.status()
    }

    /// In, and told where the game is.
    pub fn live(&self) -> bool {
        self.you.is_some() && self.clock && self.client.connected()
    }

    /// What has come, done: call it once a frame before stepping (`now`: a clock that never goes
    /// back, `lunar_net::now()`).
    pub fn receive(&mut self, now: f64, game: &mut Game, me: &mut Player) {
        self.now = now;
        self.client.update(now);
        let events: Vec<NetEvent> = self.client.events().collect();
        for e in events {
            match e {
                NetEvent::Game { reliable: true, data } => self.reliable(&data, game, me),
                NetEvent::Game { reliable: false, data } => self.loose(&data),
                NetEvent::Disconnected { reason } => self.said.push((format!("Desconectado del servidor: {reason}"), 2)),
                NetEvent::Chat { from, text } => {
                    let who = from.and_then(|id| self.client.name(id)).unwrap_or("Servidor").to_string();
                    self.said.push((format!("{who}: {text}"), 0));
                }
                NetEvent::Joined { name, .. } if self.you.is_some() => self.said.push((format!("{name} se ha unido"), 0)),
                NetEvent::Left { name, .. } => self.said.push((format!("{name} se ha ido"), 0)),
                _ => {}
            }
        }
        if self.you.is_none() {
            return;
        }
        if let Some((step, state)) = self.pending.take() {
            self.correct(step, &state, game, me);
        }
        if std::mem::take(&mut self.snap_new) {
            self.snapshot(game);
        }
        // (pieces of ours the server never named: not pieces it has)
        let step = game.step;
        let mut k = 0;
        while k < self.ours.len() {
            let (id, born) = self.ours[k];
            if game.builds.set.index_of(id).is_none() {
                self.ours.swap_remove(k);
            } else if step > born + ORPHAN {
                remove(game, id);
                self.ours.swap_remove(k);
                self.stats.orphans += 1;
            } else {
                k += 1;
            }
        }
    }

    /// How many steps to take in a frame of `dt` s, by this game's clock.
    pub fn steps(&mut self, dt: f64) -> u32 {
        if self.you.is_none() || !self.clock {
            return 0;
        }
        self.due += dt.clamp(0.0, 0.25) * self.rate;
        let mut n = 0;
        while self.due >= STEP && n < MOST_STEPS {
            self.due -= STEP;
            n += 1;
        }
        n
    }

    /// The share of a step the picture is past the last one (for `Game::present`).
    pub fn alpha(&self) -> f64 {
        (self.due / STEP).clamp(0.0, 1.0)
    }

    /// One step: what the player asks (`me.input`, the look, the suit's switches, `keys`), as it
    /// travels, stepped and sent with what it made of the body. What the player did since the last
    /// step that the server must check is said first, whoever did it here (a key, a click, a
    /// script): sitting down and getting up, taking hold and letting go, what was let fly.
    pub fn step(&mut self, game: &mut Game, me: &mut Player) {
        let s = game.step;
        self.said_since(game, me);
        // (the way it acts along goes only if it is not the body's own look: from outside, the
        // middle of the picture)
        let own = me.pilot.view_aboard(&game.builds.set).unwrap_or_else(|| me.pilot.view()).forward;
        let aim = me.aim.map(|v| v.forward).filter(|f| f.dot(own) < AIM_SAME).map(|f| f.as_vec3());
        let mut cmd = Cmd {
            step: s,
            input: me.input,
            yaw: me.pilot.yaw,
            pitch: me.pilot.pitch,
            aim,
            pack: me.pilot.pack_on,
            steady: me.pilot.steady,
            lamps: me.pilot.lamps,
            fly: me.pilot.flying,
            keys: self.keys,
            head: self.head,
            tool: self.tool,
            trigger: self.trigger,
            outside: self.outside,
            gesture: self.gesture,
            // (floating, the mouse turns the whole body: that goes too)
            frame: me.pilot.floating().then(|| {
                let (up, fore) = me.pilot.body_frame();
                [up.as_vec3(), fore.as_vec3()]
            }),
        }
        .travelled();
        cmd.step = s;
        // (we step with what the server steps with: the command as it travels)
        host::apply(&cmd, me, true);
        self.cmds[(s % RING as u64) as usize] = cmd;
        self.moved.clear();
        self.drive.step(&me.pilot, self.keys, &mut game.ships, &game.builds.set, STEP as f32, &mut self.moved);
        game.tick(&mut [&mut *me]);
        self.stats.steps += 1;
        // (what came into being here this step — a piece let go by a clamp, one off what was
        // struck — is ours until the server names it, by its lineage)
        for &id in &game.out.made {
            if id >= LOCAL_IDS && self.ours.iter().all(|o| o.0 != id) {
                self.ours.push((id, game.step));
            }
        }
        // (where everything was at this step, to be put right by the server's word of it)
        self.track(game);
        // what was asked of the last few steps, and what we made of the body
        let check = Check { step: s, body: me.pilot.summary(), fixes: self.fixes };
        let mut list = [Cmd::default(); REPEAT];
        // (only steps we took)
        let n = (s + 1 - self.first.min(s)).min(REPEAT as u64) as usize;
        for (k, c) in list.iter_mut().enumerate().take(n) {
            *c = self.cmds[((s - k as u64) % RING as u64) as usize];
        }
        net::write_cmds(&list[..n], Some(check), &mut self.out);
        self.client.send_quick(&self.out);
        self.sent[(s % RING as u64) as usize] = self.now;
        // the drawn eye comes to where the body is
        let k = (-STEP * std::f64::consts::LN_2 / EYE_HALF_LIFE).exp();
        me.offset.0 *= k;
        me.offset.1 *= k as f32;
    }

    /// What changed since the last step that the server must be told: the seat, the hands, what
    /// was let fly here (said as an act of this step: done there before it, as here).
    fn said_since(&mut self, game: &mut Game, me: &Player) {
        let s = game.step;
        let seat = me.pilot.seat.map(|x| (x.structure, x.index));
        if seat != self.seated {
            if self.seated.is_some() && seat.is_none_or(|x| Some(x) != self.seated) {
                self.act(s, &Act::Stand);
            }
            if let Some((ship, index)) = seat {
                self.act(s, &Act::Sit { ship, seat: index as u16 });
            }
            self.seated = seat;
        }
        let held = me.hands.holding().zip(me.hands.held_at());
        match (self.holding, held) {
            (None, Some(_)) => self.act(s, &Act::Grab),
            (Some(_), None) => self.act(s, &Act::Release),
            (Some((a, d0)), Some((b, d1))) if a != b => {
                self.act(s, &Act::Release);
                self.act(s, &Act::Grab);
                let _ = (d0, d1);
            }
            (Some((_, d0)), Some((_, d1))) if (d1 - d0).abs() > 1e-6 => self.act(s, &Act::Wheel(((d1 - d0) / crate::hands::NOTCH) as f32)),
            _ => {}
        }
        self.holding = held;
        // (what was let fly here: the server lets it fly; where it ends it says)
        let mut seen = std::mem::take(&mut game.blasts.seen);
        for x in seen.drain(..) {
            if let Seen::Launch { launch, tag } = x {
                self.act(s, &Act::Launch(launch, tag));
            }
        }
        game.blasts.seen = seen;
    }

    /// The body was put where it is by whoever runs this game (a menu's «start here», a script):
    /// said, so the server puts it there too (where tests are let be: else it is put back).
    pub fn put(&mut self, game: &Game, me: &Player) {
        let mut state = Vec::with_capacity(256);
        me.pilot.write_state(&mut state);
        self.act(game.step, &Act::Body(state));
    }

    /// Something the player does that the server must check (sent; done here by whoever calls,
    /// as a prediction, before the next step).
    pub fn act(&mut self, step: u64, act: &Act) {
        net::write_act(step, act, &mut self.out);
        self.client.send_game(&self.out);
    }

    /// Said reliably: the events of a step.
    fn reliable(&mut self, data: &[u8], game: &mut Game, me: &mut Player) {
        let mut r = Reader::new(data);
        if r.u8() != Ok(EVENTS) {
            self.stats.garbled += 1;
            return;
        }
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        let at = match net::read_events(&mut r, &mut events) {
            Ok(at) => at,
            Err(e) => {
                if std::env::var("LUNAR_DEBUG").is_ok() {
                    eprintln!("events garbled: {e:?} after {} events ({} bytes): {:?}", events.len(), data.len(), events.last().map(|e| std::mem::discriminant(e)));
                }
                self.stats.garbled += 1;
                self.events = events;
                return;
            }
        };
        for e in events.drain(..) {
            self.event(e, at, game, me);
        }
        self.events = events;
    }

    /// Said loosely: a snapshot (the newest kept).
    fn loose(&mut self, data: &[u8]) {
        let mut r = Reader::new(data);
        if r.u8() != Ok(SNAP) {
            self.stats.garbled += 1;
            return;
        }
        let mut snap = Snap::default();
        std::mem::swap(&mut snap, &mut self.snap);
        let step = snap.step;
        match net::read_snap(&mut r, &mut snap) {
            Ok(()) if !self.snap_new || snap.step > step => {
                self.snap = snap;
                self.snap_new = true;
            }
            Ok(()) => {}
            Err(e) => {
                if std::env::var("LUNAR_DEBUG").is_ok() {
                    eprintln!("snapshot garbled: {e:?} ({} bytes)", data.len());
                }
                self.stats.garbled += 1;
            }
        }
    }

    fn event(&mut self, e: Event, at: u64, game: &mut Game, me: &mut Player) {
        match e {
            Event::Hello { step, you, sun, region, key } => {
                // (the clock is set by the first snapshot, which is never old: this may have come
                // again after it was lost)
                game.step = step.max(at);
                // (what this game had of its own or was told before, forgotten: what is, is told
                // again; the scenario's is as every game has it, and what of it is no more, told)
                let end = game.builds.scenario_end;
                let had: Vec<u64> = game.builds.set.list.iter().filter(|s| s.id >= end).map(|s| s.id).collect();
                for id in had {
                    remove(game, id);
                }
                self.ours.clear();
                self.asked.clear();
                (self.seated, self.holding, self.pending) = (None, None, None);
                self.given = key;
                match self.back {
                    Some(back) => {
                        self.awaiting = true;
                        net::write_act(step, &Act::Back { key: back }, &mut self.out);
                        self.client.send_game(&self.out);
                    }
                    None => self.key = Some(key),
                }
                self.clock = false;
                game.sun = sun;
                game.say = Say::Client;
                game.blasts.tell = true;
                game.builds.set.reserve(LOCAL_IDS);
                me.pilot = Pilot::new(game.bodies.clone(), &game.site, self.def);
                me.hands = Hands::new(self.def.manos);
                (self.you, self.region, self.fixes) = (Some(you), region, 0);
                self.tracks.clear();
                self.bones.clear();
                (self.due, self.rate, self.early) = (0.0, 1.0, TARGET);
            }
            Event::Back { step: _, state } => {
                // (the body as it waited: ours again, by the key we came with; or none, and we
                // start anew with the one this connection was given)
                self.awaiting = false;
                if !state.is_empty() && me.pilot.read_state(&state).is_ok() {
                    self.key = self.back.take();
                    self.stats.back = true;
                    // (seated as it waited: said already)
                    self.seated = me.pilot.seat.map(|x| (x.structure, x.index));
                } else {
                    (self.key, self.back) = (Some(self.given), None);
                    self.said.push(("Tu cuerpo ya no te esperaba: empiezas de nuevo".to_string(), 1));
                }
            }
            Event::Correct { step, state } => {
                // (counted as the server counts them, each one: what we say from now on is of a
                // body put right this many times; only the newest is put)
                self.fixes = self.fixes.wrapping_add(1);
                if self.pending.as_ref().is_none_or(|p| p.0 <= step) {
                    self.pending = Some((step, state));
                }
            }
            e @ Event::Made { .. } => self.made(game, at, &e),
            Event::Hold { id, held } => {
                let world = game.builds.set.now;
                game.builds.set.hold_as(id, held);
                if held.is_none()
                    && let Some(k) = game.builds.set.index_of(id)
                {
                    game.builds.set.list[k].clock = world;
                }
                self.forget_track(id);
            }
            Event::Gone { id } => {
                remove(game, id);
                self.stats.gone += 1;
            }
            Event::Strikes(bytes) => {
                let mut r = Reader::new(&bytes);
                if r.u8() != Ok(told::STRIKES) {
                    return;
                }
                self.strikes.clear();
                self.poses.clear();
                let Ok(seed) = told::read_strikes(&mut r, &mut self.strikes, &mut self.poses) else {
                    self.stats.garbled += 1;
                    return;
                };
                for (i, (n, s)) in self.strikes.iter().enumerate() {
                    let Named::Built(id) = *n else { continue };
                    if game.builds.set.index_of(id).is_none() {
                        continue;
                    }
                    let s = match *s {
                        Strike::Hit { hit, .. } => Strike::Hit { id, hit },
                        Strike::Blow { part, push, energy, .. } => Strike::Blow { id, part, push, energy },
                    };
                    let pose = self.poses.iter().find(|p| p.0 == *n).map_or(&[][..], |p| &p.1[..]);
                    let from = game.builds.set.next_free();
                    game.builds.strike_done(&s, seed.wrapping_add(i as u64), false, pose);
                    self.stats.strikes += 1;
                    // (what came off it here: ours until the server names it)
                    let step = game.step;
                    self.ours.extend(game.builds.set.list.iter().filter(|x| x.id >= from).map(|x| (x.id, step)));
                }
            }
            Event::Seen(bytes) => {
                let mut r = Reader::new(&bytes);
                if r.u8() != Ok(told::SEEN) {
                    return;
                }
                let now = game.time();
                let set = &game.builds.set;
                self.seen.clear();
                let find = |n: Named| match n {
                    Named::Built(id) => set.get(id).map(|s| (s.id, told::Frame { pos: s.pos, rot: s.rot, vel: s.vel, spin: s.spin })),
                    _ => None,
                };
                if told::read_seen(&mut r, now, find, &mut self.seen).is_err() {
                    self.stats.garbled += 1;
                }
                let bodies = game.bodies.clone();
                for (s, age) in self.seen.drain(..) {
                    game.blasts.show(0, &s, age, &bodies, &mut game.builds);
                }
            }
            Event::Control { ship, control, value } => {
                controls::set(&mut game.ships, &game.builds.set, ship, usize::from(control), value);
            }
            Event::Hand { ship, act } => controls::act(&mut game.ships, ship, act),
            Event::Said { about: _, text, level, .. } => self.said.push((text, level)),
            Event::Systems { ship, data } => {
                if let (Some(n), Some(k)) = (game.ships.by_structure(ship), game.builds.set.index_of(ship)) {
                    let mut inp = sync::In::new(&data);
                    let _ = sync::read_ship(&mut game.ships.list[n], &mut game.builds.set.list[k], &|x| Some(x), &mut inp);
                    catch_up(game, n, at);
                }
            }
            Event::State { id, delta } => {
                if let Some(k) = game.builds.set.index_of(id) {
                    self.died.clear();
                    let mut inp = sync::In::new(&delta);
                    let _ = sync::read_delta(&mut game.builds.set.list[k], &mut inp, &mut self.died);
                }
            }
            Event::Unheld => {
                me.hands.release(&mut game.builds);
                self.holding = None;
            }
            Event::Unfired { tag } => game.blasts.unfire(tag),
            Event::Denied(why) => {
                self.stats.denied += 1;
                self.said.push((why, 1));
            }
            Event::Rest { id, pos, rot } => {
                let world = game.builds.set.now;
                if let Some(k) = game.builds.set.index_of(id) {
                    let s = &mut game.builds.set.list[k];
                    (s.pos, s.rot, s.vel, s.spin) = (pos, rot, DVec3::ZERO, Vec3::ZERO);
                    (s.clock, s.resting) = (world, true);
                    self.forget_track(id);
                }
            }
            Event::Crater { body, crater } => {
                if usize::from(body) < game.bodies.len() {
                    let b = game.bodies.get(body);
                    b.edit(|d| d.add(crater, b.radius));
                }
            }
            Event::Ground { body, from, craters } => {
                if usize::from(body) < game.bodies.len() {
                    game.bodies.get(body).edit(|d| d.put_from(from as usize, &craters));
                }
            }
        }
    }

    /// All of a structure (`Event::Made`), as the server had it at the end of step `at`: ours
    /// already (put as it says), a piece of ours by its lineage (named so), or new here.
    fn made(&mut self, game: &mut Game, at: u64, e: &Event) {
        let Event::Made { id, lineage, .. } = *e else { return };
        self.stats.made += 1;
        // (a piece we broke off the same: the server's name for it)
        if game.builds.set.index_of(id).is_none()
            && let Some(k) = self.ours.iter().position(|o| game.builds.set.get(o.0).is_some_and(|s| s.lineage == lineage))
        {
            let (local, _) = self.ours.swap_remove(k);
            if rename(game, local, id) {
                self.stats.renamed += 1;
            }
        }
        if !put_made(game, at, e, &mut self.died) {
            self.stats.garbled += 1;
        }
        self.forget_track(id);
    }

    /// Our body as the server had it at the end of step `step`, stepped again with what we asked
    /// since; the eye's jump taken out over a moment.
    fn correct(&mut self, step: u64, state: &[u8], game: &mut Game, me: &mut Player) {
        let before = (me.pilot.position, me.pilot.ride.map(|r| (r.id, r.local)));
        // (where the player looks is theirs, not the server's: the mouse moved since the last step)
        let look = (me.pilot.yaw, me.pilot.pitch);
        if me.pilot.read_state(state).is_err() {
            self.stats.garbled += 1;
            return;
        }
        self.stats.corrections += 1;
        if std::env::var("LUNAR_DEBUG").is_ok() {
            eprintln!("correct at {step} (now {}): was {:?} -> {:?}", game.step, before, me.pilot.summary());
        }
        let now = game.step;
        if now.saturating_sub(step + 1) < RING as u64 {
            for t in step + 1..now {
                let cmd = self.cmds[(t % RING as u64) as usize];
                if cmd.step != t {
                    continue;
                }
                host::apply(&cmd, me, true);
                // (against what it stands in as it was then: how it sped up, its own gravity, its
                // moving parts; not as it is now)
                if let Some(r) = me.pilot.ride {
                    self.as_then(&mut game.builds.set, r.id, t);
                }
                game.step_alone(me);
                self.stats.replayed += 1;
            }
            self.as_now(&mut game.builds.set);
        }
        (me.pilot.yaw, me.pilot.pitch) = look;
        // (drawn where it was, and brought to where it is)
        let after = (me.pilot.position, me.pilot.ride.map(|r| (r.id, r.local)));
        match (before.1, after.1) {
            (Some((a, la)), Some((b, lb))) if a == b => {
                let off = la - lb + me.offset.1;
                me.offset.1 = if f64::from(off.length()) > EYE_SNAP { Vec3::ZERO } else { off };
            }
            _ => {
                let off = before.0 - after.0 + me.offset.0;
                me.offset = (if off.length() > EYE_SNAP { DVec3::ZERO } else { off }, Vec3::ZERO);
            }
        }
    }

    /// Where every structure that moves is at this step, kept.
    fn track(&mut self, game: &Game) {
        let set = &game.builds.set;
        let step = game.step;
        let bodies = &*game.bodies;
        // (both in the order of the ids: gone through together)
        let mut t = 0;
        let mut fresh = Vec::new();
        for s in &set.list {
            if s.anchored {
                continue;
            }
            while t < self.tracks.len() && self.tracks[t].id < s.id {
                t += 1;
            }
            // (as it is at the world's moment, if it is stepped now and then)
            let (pos, vel, rot) = coasted(s, bodies.field(s.pos).pull, set.now - s.clock);
            let pose = Pose { step, pos, vel, rot, spin: s.spin, acc: s.acc, on: s.gravity.on, bones: s.pose };
            if t < self.tracks.len() && self.tracks[t].id == s.id {
                let tr = &mut self.tracks[t];
                tr.poses[(step % RING as u64) as usize] = pose;
                // (its moving parts, kept as they change: a buffer of what is past `RING` reused)
                if s.bones.len() > 1 && tr.kept != s.pose {
                    tr.kept = s.pose;
                    let mut b = match self.bones.front() {
                        Some(old) if old.step + RING as u64 <= step => self.bones.pop_front().unwrap_or_else(|| Bones { id: 0, pose: 0, step: 0, at: Vec::new() }),
                        _ => Bones { id: 0, pose: 0, step: 0, at: Vec::new() },
                    };
                    (b.id, b.pose, b.step) = (s.id, s.pose, step);
                    b.at.clear();
                    b.at.extend_from_slice(&s.bones[1..]);
                    self.bones.push_back(b);
                }
            } else {
                fresh.push((s.id, pose));
            }
        }
        if !fresh.is_empty() {
            for (id, pose) in fresh {
                let mut poses = Box::new([Pose::default(); RING]);
                poses[(step % RING as u64) as usize] = pose;
                let at = self.tracks.partition_point(|x| x.id < id);
                self.tracks.insert(at, Track { id, poses, fixed: step, kept: u64::MAX });
            }
        }
        if self.tracks.len() > set.list.len() {
            self.tracks.retain(|tr| set.index_of(tr.id).is_some());
        }
    }

    /// Structure `id` put as it was at step `t` (as far as was kept of it), what it was before
    /// kept to be put back (`as_now`).
    fn as_then(&mut self, set: &mut Structures, id: u64, t: u64) {
        // (kept as `game.step` was after it: the step after)
        let Ok(tr) = self.tracks.binary_search_by_key(&id, |x| x.id) else { return };
        let p = self.tracks[tr].poses[((t + 1) % RING as u64) as usize];
        let Some(k) = set.index_of(id).filter(|_| p.step == t + 1) else { return };
        let s = &mut set.list[k];
        let w = match self.was.iter().position(|w| w.id == id) {
            Some(w) => w,
            None => {
                let (rot, vel, spin, acc, on, pose) = (s.rot, s.vel, s.spin, s.acc, s.gravity.on, s.pose);
                self.was.push(Was { id, rot, vel, spin, acc, on, pose, put: pose, bones: s.bones[1..].to_vec() });
                self.was.len() - 1
            }
        };
        (s.rot, s.vel, s.spin, s.acc, s.gravity.on) = (p.rot, p.vel, p.spin, p.acc, p.on);
        let w = &mut self.was[w];
        if p.bones != w.put {
            let then = if p.bones == w.pose { Some(&w.bones[..]) } else { self.bones.iter().find(|b| b.id == id && b.pose == p.bones).map(|b| &b.at[..]) };
            if let Some(then) = then.filter(|b| b.len() + 1 == s.bones.len()) {
                s.set_pose(then);
                w.put = p.bones;
            }
        }
    }

    /// What `as_then` changed, put back as it is.
    fn as_now(&mut self, set: &mut Structures) {
        for w in self.was.drain(..) {
            let Some(k) = set.index_of(w.id) else { continue };
            let s = &mut set.list[k];
            (s.rot, s.vel, s.spin, s.acc, s.gravity.on) = (w.rot, w.vel, w.spin, w.acc, w.on);
            if w.put != w.pose && w.bones.len() + 1 == s.bones.len() {
                s.set_pose(&w.bones);
            }
        }
    }

    fn forget_track(&mut self, id: u64) {
        if let Ok(t) = self.tracks.binary_search_by_key(&id, |x| x.id) {
            self.tracks.remove(t);
        }
    }

    /// The newest snapshot: how early our commands come (the clock), the others, and how far
    /// off each structure was at its step (put right by a share of it, here and in what is kept).
    fn snapshot(&mut self, game: &mut Game) {
        self.stats.snaps += 1;
        let snap = &self.snap;
        // ---- the clock, the first time: ahead of the server by half the way there and a few
        // steps (early enough to spare: the first commands must not come late, as the body starts
        // at the first the server has; the clock takes off what is too much)
        if !self.clock && !self.awaiting {
            let ping = match self.client.status() {
                Status::Connected { ping_ms, .. } => f64::from(ping_ms) / 1000.0,
                _ => 0.1,
            };
            self.rtt = ping;
            // (the snapshot is half the way old already)
            game.step = game.step.max(snap.step + (ping / STEP).round() as u64 + 3 * TARGET as u64);
            (self.clock, self.due, self.set_at, self.first) = (true, 0.0, self.now, game.step);
        }
        // ---- the clock: there and back (from when the newest command it had went), and how
        // early ours come: a little off, run a hair faster or slower; much, set at a stroke (and
        // not again until what that did has had time to come back)
        if snap.took > 0 {
            let went = self.sent[((snap.took - 1) % RING as u64) as usize];
            if went.is_finite() && self.now >= went {
                self.rtt += ((self.now - went).min(2.0) - self.rtt) * 0.1;
            }
        }
        let early = f64::from(snap.ahead);
        if snap.ahead > -1000 && snap.ahead < 1000 {
            self.early += (early - self.early) * 0.25;
            let off = self.early - TARGET;
            if (early - TARGET).abs() > 3.0 && self.now > self.set_at + self.rtt + 0.15 {
                self.due += (TARGET - early) * STEP;
                (self.early, self.set_at) = (TARGET, self.now);
            }
            self.rate = 1.0 - (off * 0.02).clamp(-0.05, 0.05);
        }
        // ---- the others
        self.others.clear();
        self.others.extend_from_slice(&snap.players);
        self.others_step = snap.step;
        // ---- what moves
        let now = game.step;
        let set = &mut game.builds.set;
        let bodies = game.bodies.clone();
        let world = set.now;
        for th in &snap.things {
            let Some(k) = set.index_of(th.id) else { continue };
            let s = &mut set.list[k];
            let want = Pose { step: snap.step, pos: th.pos, vel: th.vel.as_dvec3(), rot: th.rot, spin: th.spin, ..Pose::default() };
            let track = self.tracks.binary_search_by_key(&th.id, |x| x.id).ok();
            let had = track.map(|t| self.tracks[t].poses[(snap.step % RING as u64) as usize]).filter(|p| p.step == snap.step && now.saturating_sub(snap.step) < RING as u64);
            // (at rest there: put where it rests, at rest)
            if th.resting {
                // (ours resting there already, to what the snapshot can say: kept as it is, which
                // is to the last bit where it was made)
                if s.resting && s.pos.distance(want.pos) < 1e-3 && s.rot.angle_between(want.rot) < 1e-4 {
                    continue;
                }
                (s.pos, s.rot, s.vel, s.spin) = (want.pos, want.rot, DVec3::ZERO, Vec3::ZERO);
                (s.clock, s.resting) = (world, true);
                if let Some(t) = track {
                    self.tracks[t].fixed = now;
                }
                self.stats.snapped += 1;
                continue;
            }
            let off = had.map(|h| (want.pos - h.pos, want.vel - h.vel, want.rot * h.rot.inverse(), want.spin - h.spin));
            // (off by no more than what a snapshot can say: left as it is. Brought to the snapshot's
            // rounding instead, a ship's hull would move a millimetre this way and that, and what
            // stands on it with it)
            if off.is_some_and(|(dp, dv, dr, dw)| dp.length() < AGREE_POS && dv.length() < AGREE_VEL && dr.angle_between(Quat::IDENTITY) < AGREE_TURN && dw.length() < AGREE_SPIN) {
                continue;
            }
            // (put where the server had it at a step after this one's: this one is older news)
            if off.is_none() && track.is_some_and(|t| snap.step < self.tracks[t].fixed) {
                continue;
            }
            let far = off.is_none_or(|o| o.0.length() > SNAP_OFF || o.2.angle_between(Quat::IDENTITY) > SNAP_TURN) || s.sim != SimLevel::Active;
            if far {
                // (put where the server has it, carried on to now: what is kept of where it was is
                // of another way, and goes; snapshots older than this are left)
                (s.pos, s.rot, s.vel, s.spin) = (want.pos, want.rot, want.vel, want.spin);
                carry(s, &bodies, now.saturating_sub(snap.step) as f64 * STEP);
                (s.clock, s.resting, s.still) = (world, false, 0.0);
                if let Some(t) = track {
                    for p in self.tracks[t].poses.iter_mut() {
                        p.step = 0;
                    }
                    self.tracks[t].fixed = now;
                }
                self.stats.snapped += 1;
                continue;
            }
            let Some((dp, dv, dr, dw)) = off else { continue };
            let t = track.unwrap_or(0);
            let since = now.saturating_sub(self.tracks[t].fixed).max(1) as f64 * STEP;
            let share = 1.0 - (-since * std::f64::consts::LN_2 / HALF_LIFE).exp();
            let (dp, dv, dw) = (dp * share, dv * share, dw * share as f32);
            let dr = Quat::IDENTITY.slerp(dr, share as f32).normalize();
            (s.pos, s.vel, s.spin) = (s.pos + dp, s.vel + dv, s.spin + dw);
            s.rot = (dr * s.rot).normalize();
            (s.resting, s.still) = (false, 0.0);
            // (what is kept past the snapshot's step, put right likewise: not put right twice)
            let tr = &mut self.tracks[t];
            tr.fixed = now;
            for p in tr.poses.iter_mut().filter(|p| p.step > snap.step && p.step <= now) {
                (p.pos, p.vel, p.spin) = (p.pos + dp, p.vel + dv, p.spin + dw);
                p.rot = (dr * p.rot).normalize();
            }
            self.stats.fixed += 1;
        }
        // ---- a ship's systems: all of it again if ours does not agree
        if let Some((id, d)) = snap.check
            && let Some(s) = game.builds.set.get(id)
        {
            let ship = game.ships.by_structure(id).map(|n| &game.ships.list[n]);
            let i = match self.asked.iter().position(|a| a.0 == id) {
                Some(i) => i,
                None => {
                    self.asked.push((id, 0, 0));
                    self.asked.len() - 1
                }
            };
            if Digest::of(ship, s).agrees(&d) {
                self.asked[i].2 = 0;
            } else {
                self.asked[i].2 = self.asked[i].2.saturating_add(1);
                if self.asked[i].2 >= 2 && now > self.asked[i].1 + RESYNC_EVERY {
                    (self.asked[i].1, self.asked[i].2) = (now, 0);
                    self.stats.resyncs += 1;
                    net::write_act(now, &Act::Resync { id }, &mut self.out);
                    self.client.send_game(&self.out);
                }
            }
        }
    }
}

/// Structure `s`, as the server had it `t` s ago, carried on to now as it flies: on its way, pulled
/// as it is where it is; but what is near the ground only on its way (it lies or slides on it: it
/// does not fall into it).
fn carry(s: &mut lunar_core::structure::state::Structure, bodies: &lunar_core::body::BodyRegistry, t: f64) {
    let here = bodies.field(s.pos);
    let low = here.ground.is_some_and(|g| bodies.get(g).altitude(s.to_world(s.center)) < f64::from(s.radius) + 1.0);
    coast(s, if low { DVec3::ZERO } else { here.pull }, t);
}

/// Ship `n`'s systems, told as they were at the end of step `at`, run on to this game's step (it is
/// ahead of the server's): else they would stay that many steps behind.
fn catch_up(game: &mut Game, n: usize, at: u64) {
    let ticks = game.step.saturating_sub(at).min(RING as u64) as u32;
    let bodies = game.bodies.clone();
    game.ships.catch_up(n, ticks, &mut game.builds, &bodies, game.sun, STEP);
}

/// A structure as `Event::Made` tells it, as the server had it at the end of step `at`, put in
/// `game`: made if it has none of that id (or one not made of what it says), and put as it says,
/// carried on to the game's step (resting: where it rests), with its ship's systems run on to it.
/// False if what it says made no sense (what could be put, is).
pub(crate) fn put_made(game: &mut Game, at: u64, e: &Event, died: &mut Vec<u32>) -> bool {
    let Event::Made { id, lineage, born, ref ship, seed, pos, rot, vel, spin, resting, held, ref make, ref state, ref systems } = *e else { return false };
    let lib = game.builds.set.lib.clone();
    let mut ok = true;
    // (ours, but not of what the server says it is made of: made anew)
    if let Some(s) = game.builds.set.get(id)
        && !sync::made_as(s, &lib, make)
    {
        remove(game, id);
    }
    if game.builds.set.index_of(id).is_none() {
        let mut inp = sync::In::new(make);
        let Ok(s) = sync::make(&lib, id, pos, rot, &mut inp) else { return false };
        game.builds.set.insert(s);
        if !ship.is_empty() && game.ships.adopt(&mut game.builds, ship, id, seed).is_err() {
            ok = false;
        }
    }
    let Some(k) = game.builds.set.index_of(id) else { return false };
    died.clear();
    let s = &mut game.builds.set.list[k];
    let mut inp = sync::In::new(state);
    let _ = sync::read_state(s, &mut inp, died);
    (s.lineage, s.born) = (lineage, born);
    (s.pos, s.rot, s.vel, s.spin) = (pos, rot, vel.as_dvec3(), spin);
    if resting {
        // (at rest there: at rest here, where it rests)
        s.resting = true;
    } else {
        // (as it was then, carried on to now)
        (s.resting, s.still) = (false, 0.0);
        carry(s, &game.bodies, game.step.saturating_sub(at) as f64 * STEP);
    }
    s.clock = game.builds.set.now;
    if !systems.is_empty()
        && let Some(n) = game.ships.by_structure(id)
    {
        let mut inp = sync::In::new(systems);
        let _ = sync::read_ship(&mut game.ships.list[n], &mut game.builds.set.list[k], &|x| Some(x), &mut inp);
        catch_up(game, n, at);
    }
    game.builds.set.hold_as(id, held);
    ok
}

/// Structure `id` (and its ship) gone from this game.
pub(crate) fn remove(game: &mut Game, id: u64) {
    game.ships.list.retain(|sh| sh.structure != id);
    game.builds.set.remove(id);
}

/// Structure `old` called `new` here (the server's name for it), and so wherever this game names
/// it: its ship, the players' ride and hands.
fn rename(game: &mut Game, old: u64, new: u64) -> bool {
    if !game.builds.set.rename(old, new) {
        return false;
    }
    for sh in &mut game.ships.list {
        if sh.structure == old {
            sh.structure = new;
        }
    }
    true
}
