//! The game as the server runs it: the truth (`docs/PLAN_AUTORITATIVO.md` §3.9). Each player's
//! game sends what its player asks of each step (`net::Cmd`, several times over) and what it
//! means to do (`net::Act`); the server steps everyone's body with what they asked, the world
//! with them, and tells each what it needs: snapshots of what moves near it, what happened, and
//! its body put right when its own game went astray (`net::Event::Correct`).
//!
//! Per step (`Host::step`):
//! 1. each player's command for the step (the last one again, if it has not come: the jump left
//!    out), and the acts due, checked (in reach, in time, not too fast);
//! 2. the seats' keys on their controls (`seats::Drive`), what they moved told to the others;
//! 3. what changed of the structures by a hand (`Event::State`);
//! 4. the step (`Game::tick`, `Say::Server`: what is done to structures is done in one order and
//!    told, `Event::Strikes`);
//! 5. each body against what its game said it got (`net::Check`): a correction if they differ
//!    by more than a millimetre (`pilot::Summary::near`);
//! 6. who knows what (`interest`), what came and went (`Event::Made`, `Event::Gone`), what was
//!    let fly and where it ended (`Event::Seen`), what the ships said to whoever rides them;
//! 7. a snapshot every `snap_every` steps to each (staggered), and the events.
//!
//! What goes to many is encoded once; the work of each player (who knows what, its snapshot) is
//! spread over every thread (rayon).
use crate::{
    blasts::Seen,
    builds::Strike,
    controls, follow,
    game::{Game, Player, STEP, Say},
    interest::{Interest, Rule},
    net::{self, ACT, Act, CMDS, Check, Cmd, Event, Snap},
    pilot::Summary,
    seats::{self, Drive},
    told::{self, Named},
};
use glam::Vec3;
use lunar_core::{scenario::PlayerDef, structure::schedule::coasted, view::View};
use lunar_net::{Frame, PlayerState, Reader, RigidState, Writer, flag};
use lunar_ship::sync::{self, Digest, Shadow};
use rayon::prelude::*;
use std::collections::VecDeque;

/// Steps of each player's commands and bodies kept (a little over 2 s).
pub const RING: usize = 128;
/// A command for a step further ahead than this is not taken (a game whose clock ran away).
const AHEAD_MOST: u64 = (RING as u64) - 8;
/// Acts of one player waiting for their step, at most.
const ACTS_MOST: usize = 64;
/// How far from where the body is something it does may be (m): what it works with its hands, a
/// control, what it lets fly.
const HAND_REACH: f64 = 6.0;
/// A ship's controls are within reach this much past its size (m).
const CONTROL_REACH: f64 = 8.0;
/// How much faster than what lets it fly a launch may say it goes besides its own speed (m/s).
const LAUNCH_SLIP: f64 = 50.0;
/// Hit points a second a hand may mend, as a share of what the part has (a welder's best and some).
const MEND_RATE: f32 = 0.5;

#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Free flight and ships put anywhere (the tests and the debug keys) are let be.
    pub cheats: bool,
    /// Steps between two snapshots to one player (2: 30 a second), and the bytes of each.
    pub snap_every: u64,
    pub snap_room: usize,
    /// Who knows what.
    pub rule: Rule,
    /// Things let fly a second by one player's hand, at most (and at once).
    pub launches: f32,
    /// The region of the galaxy this game is of (`Event::Hello`).
    pub region: u32,
}

impl Default for HostConfig {
    fn default() -> HostConfig {
        HostConfig { cheats: false, snap_every: 2, snap_room: lunar_net::MAX_HINT - 8, rule: Rule::default(), launches: 30.0, region: 0 }
    }
}

/// Counters since the host began.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostStats {
    pub steps: u64,
    /// Commands guessed (the player's had not come in time), and come after their step.
    pub guessed: u64,
    pub late: u64,
    /// Bodies put right.
    pub corrections: u64,
    /// Acts refused, and messages that made no sense.
    pub denied: u64,
    pub garbled: u64,
    /// Bytes of snapshots and of events sent.
    pub snap_bytes: u64,
    pub event_bytes: u64,
}

/// One player, as the server has them besides their body.
struct Peer {
    id: u32,
    /// Their commands by step (`RING` of them, each at `step % RING`).
    cmds: Vec<Cmd>,
    /// The newest step of theirs we have (to step with), the newest heard of at all (late ones
    /// too), and the last command stepped with.
    newest: Option<u64>,
    heard: Option<u64>,
    /// Not in the world yet and catching up with it, alone: the step the body is at.
    behind: Option<u64>,
    last: Cmd,
    /// How many steps ahead of its step the command of each step came, at least, since the last
    /// snapshot (less than 0: it had not come).
    lead: i64,
    /// What their game said it got at each step, and what we got (`step + 1`: 0, none).
    claims: Vec<(u64, Summary)>,
    mine: Vec<(u64, Summary)>,
    /// Times their body was put right (`Check::fixes`).
    fixes: u8,
    acts: VecDeque<(u64, Act)>,
    drive: Drive,
    /// The ship they float by (told beside it: `PlayerState::ride` with `flag::BESIDE`).
    beside: Option<u64>,
    interest: Interest,
    came: Vec<u64>,
    went: Vec<u64>,
    /// Events for them, encoded (`net::append_event`), and how many.
    events: Vec<u8>,
    count: u32,
    /// What goes out to them this step: a snapshot, the events.
    quick: Vec<u8>,
    sure: Vec<u8>,
    /// What may be let fly and mended now (refilled each step).
    launches: f32,
    mend: f32,
    /// (reused)
    snap: Snap,
    order: Vec<usize>,
}

impl Peer {
    fn new(id: u32) -> Peer {
        Peer {
            id,
            cmds: vec![Cmd::default(); RING],
            newest: None,
            heard: None,
            behind: None,
            last: Cmd::default(),
            lead: i64::MAX,
            claims: vec![(0, Summary::default()); RING],
            mine: vec![(0, Summary::default()); RING],
            fixes: 0,
            acts: VecDeque::new(),
            drive: Drive::default(),
            beside: None,
            interest: Interest::default(),
            came: Vec::new(),
            went: Vec::new(),
            events: Vec::new(),
            count: 0,
            quick: Vec::new(),
            sure: Vec::new(),
            launches: 0.0,
            mend: 0.0,
            snap: Snap::default(),
            order: Vec::new(),
        }
    }

    fn cmd_at(&self, step: u64) -> Option<&Cmd> {
        let c = &self.cmds[(step % RING as u64) as usize];
        (c.step == step && self.newest.is_some_and(|n| n >= step)).then_some(c)
    }

    fn tell(&mut self, e: &Event) {
        net::append_event(e, &mut self.events);
        self.count += 1;
    }

    fn tell_bytes(&mut self, encoded: &[u8]) {
        self.events.extend_from_slice(encoded);
        self.count += 1;
    }
}

pub struct Host {
    pub game: Game,
    pub config: HostConfig,
    pub stats: HostStats,
    def: PlayerDef,
    players: Vec<Player>,
    peers: Vec<Peer>,
    /// What each structure was last told as (parts and joints), to tell only what changed of it by
    /// a hand (`Event::State`); by id.
    shadows: Vec<(u64, u64, Shadow)>,
    /// What every player is drawn from this step (theirs left out of their own snapshot).
    states: Vec<(u32, PlayerState)>,
    /// (reused)
    cmds_in: Vec<Cmd>,
    due: Vec<(usize, Act)>,
    moved: Vec<(u64, usize, f64)>,
    encoded: Vec<u8>,
    made: Vec<(u64, Vec<u8>)>,
    seen: Vec<(told::From, Seen)>,
    pinned: Vec<Vec<u64>>,
    /// The ship whose digest goes in this step's snapshots, in turns.
    check_turn: usize,
}

/// Why an act was refused (what the player reads).
const OUT_OF_REACH: &str = "fuera de alcance";
const NOT_ALLOWED: &str = "no permitido en este servidor";
const TOO_FAST: &str = "demasiado deprisa";

impl Host {
    /// The game `game`, run as a server (its players' games are told what it does).
    pub fn new(mut game: Game, def: PlayerDef, config: HostConfig) -> Host {
        game.say = Say::Server;
        game.blasts.tell = true;
        Host {
            game,
            config,
            stats: HostStats::default(),
            def,
            players: Vec::new(),
            peers: Vec::new(),
            shadows: Vec::new(),
            states: Vec::new(),
            cmds_in: Vec::new(),
            due: Vec::new(),
            moved: Vec::new(),
            encoded: Vec::new(),
            made: Vec::new(),
            seen: Vec::new(),
            pinned: Vec::new(),
            check_turn: 0,
        }
    }

    /// The players in the game, by id.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.peers.iter().map(|p| p.id)
    }

    /// Player `id`'s body and hands as the server has them.
    pub fn player(&self, id: u32) -> Option<&Player> {
        self.peers.iter().position(|p| p.id == id).map(|k| &self.players[k])
    }

    pub fn player_mut(&mut self, id: u32) -> Option<&mut Player> {
        self.peers.iter().position(|p| p.id == id).map(|k| &mut self.players[k])
    }

    /// The game and player `id` in it, both to be changed (a script, a test, an admin's command).
    pub fn game_and_player(&mut self, id: u32) -> Option<(&mut Game, &mut Player)> {
        let k = self.peers.iter().position(|p| p.id == id)?;
        Some((&mut self.game, &mut self.players[k]))
    }

    /// Player `id` comes in: their body at the spawn, and told where they are (`Event::Hello`), what
    /// of what their game starts with is no more (`Event::Gone`), and, as they come to know it,
    /// everything round them.
    pub fn join(&mut self, id: u32) {
        if self.peers.iter().any(|p| p.id == id) {
            return;
        }
        let mut peer = Peer::new(id);
        peer.tell(&Event::Hello { step: self.game.step, you: id, sun: self.game.sun, region: self.config.region });
        // (what the scenario set that is no more: their game has it from the start)
        let set = &self.game.builds.set;
        for gone in 1..self.game.builds.scenario_end {
            if set.index_of(gone).is_none() {
                peer.tell(&Event::Gone { id: gone });
            }
        }
        let mut p = Player::new(self.game.bodies.clone(), &self.game.site, self.def);
        p.away = true;
        self.players.push(p);
        self.peers.push(peer);
    }

    /// Player `id` is gone (their body with them).
    pub fn leave(&mut self, id: u32) {
        if let Some(k) = self.peers.iter().position(|p| p.id == id) {
            self.peers.remove(k);
            self.players.remove(k);
        }
    }

    /// Something player `from`'s game said (`net::CMDS` or `net::ACT`).
    pub fn take(&mut self, from: u32, data: &[u8]) {
        let Some(k) = self.peers.iter().position(|p| p.id == from) else { return };
        let mut r = Reader::new(data);
        let ok = match r.u8() {
            Ok(CMDS) => {
                self.cmds_in.clear();
                match net::read_cmds(&mut r, &mut self.cmds_in) {
                    Ok(check) => {
                        self.commands(k, check);
                        true
                    }
                    Err(_) => false,
                }
            }
            Ok(ACT) => match net::read_act(&mut r) {
                Ok((step, act)) => {
                    let peer = &mut self.peers[k];
                    if peer.acts.len() < ACTS_MOST {
                        peer.acts.push_back((step, act));
                    } else {
                        self.stats.denied += 1;
                        peer.tell(&Event::Denied(TOO_FAST.to_string()));
                    }
                    true
                }
                Err(_) => false,
            },
            _ => false,
        };
        if !ok {
            self.stats.garbled += 1;
        }
    }

    /// The commands just read of player `k`, and what their game said it got.
    fn commands(&mut self, k: usize, check: Option<Check>) {
        let now = self.game.step;
        let peer = &mut self.peers[k];
        // (how early the newest came: what their game's clock is set by)
        if let Some(c) = self.cmds_in.first() {
            peer.lead = peer.lead.min(c.step as i64 - now as i64);
            peer.heard = Some(peer.heard.map_or(c.step, |h| h.max(c.step)));
        }
        // (not in the world yet, and what it asks comes late: its game's clock was set a little
        // behind. The body goes on alone through what it asked of the steps gone, as its own game
        // stepped it, until it is up with the world; then it is in it)
        if self.players[k].away && self.cmds_in.last().is_some_and(|c| c.step < now) {
            let p = &mut self.players[k];
            let mut at = peer.behind.unwrap_or(self.cmds_in.last().map_or(now, |c| c.step));
            for c in self.cmds_in.iter().rev() {
                if c.step < at {
                    continue;
                }
                // (what was lost on the way: as the last)
                while at < c.step && at < now {
                    let mut g = peer.last;
                    (g.step, g.input.jump) = (at, false);
                    apply(&g, p, self.config.cheats);
                    self.game.step_alone(p);
                    at += 1;
                }
                if c.step != at || at >= now {
                    break;
                }
                let c = c.travelled();
                apply(&c, p, self.config.cheats);
                self.game.step_alone(p);
                peer.last = c;
                at += 1;
            }
            if at >= now {
                (p.away, peer.behind) = (false, None);
            } else {
                peer.behind = Some(at);
            }
        }
        for c in &self.cmds_in {
            if c.step < now {
                // (its step is gone: it was guessed; and a repeat of one we had is nothing)
                if peer.newest.is_none_or(|n| c.step > n) {
                    self.stats.late += 1;
                }
                continue;
            }
            if c.step > now + AHEAD_MOST {
                continue;
            }
            peer.cmds[(c.step % RING as u64) as usize] = c.travelled();
            if peer.newest.is_none_or(|n| c.step > n) {
                peer.newest = Some(c.step);
            }
        }
        let Some(c) = check.filter(|c| c.fixes == peer.fixes) else { return };
        let slot = (c.step % RING as u64) as usize;
        if c.step < now {
            // (said of a step we have stepped: compared now)
            if peer.mine[slot].0 == c.step + 1 && !peer.mine[slot].1.near(&c.body) {
                self.correct(k, now.saturating_sub(1));
            }
        } else if c.step <= now + AHEAD_MOST {
            peer.claims[slot] = (c.step + 1, c.body);
        }
    }

    /// Player `k`'s body put right in their game: as it is here now, after step `step`.
    fn correct(&mut self, k: usize, step: u64) {
        let mut state = Vec::with_capacity(256);
        self.players[k].pilot.write_state(&mut state);
        let peer = &mut self.peers[k];
        peer.fixes = peer.fixes.wrapping_add(1);
        peer.tell(&Event::Correct { step, state });
        self.stats.corrections += 1;
    }

    /// One step of the game, and what each player is to be told of it (`send`).
    pub fn step(&mut self) {
        let s = self.game.step;
        // ---- 1. what each asks of this step, and what each does that is due
        let cheats = self.config.cheats;
        self.due.clear();
        for (k, peer) in self.peers.iter_mut().enumerate() {
            let p = &mut self.players[k];
            let cmd = match peer.cmd_at(s) {
                Some(c) => {
                    // (its first: in the world from this step, as it is in its own game)
                    p.away = false;
                    *c
                }
                // (none yet: not in the world)
                None if p.away => continue,
                None => {
                    self.stats.guessed += 1;
                    let mut c = peer.last;
                    (c.step, c.input.jump) = (s, false);
                    c
                }
            };
            apply(&cmd, p, cheats);
            peer.last = cmd;
            (peer.launches, peer.mend) = ((peer.launches + self.config.launches * STEP as f32).min(self.config.launches), (peer.mend + MEND_RATE * STEP as f32).min(MEND_RATE));
            while peer.acts.front().is_some_and(|a| a.0 <= s) {
                if let Some((_, a)) = peer.acts.pop_front() {
                    self.due.push((k, a));
                }
            }
        }
        let mut due = std::mem::take(&mut self.due);
        for (k, a) in due.drain(..) {
            if let Err(why) = self.act(k, a) {
                self.stats.denied += 1;
                self.peers[k].tell(&Event::Denied(why.to_string()));
            }
        }
        self.due = due;
        // ---- 2. the seats' keys on their controls
        self.moved.clear();
        for (k, peer) in self.peers.iter_mut().enumerate() {
            let keys = peer.last.keys;
            peer.drive.step(&self.players[k].pilot, keys, &mut self.game.ships, &self.game.builds.set, STEP as f32, &mut self.moved);
        }
        for m in 0..self.moved.len() {
            let (ship, control, value) = self.moved[m];
            let e = Event::Control { ship, control: control as u16, value };
            let by = self.players.iter().position(|p| p.pilot.seat.is_some_and(|x| x.structure == ship));
            self.tell_knowing(ship, &e, by);
        }
        // ---- 3. what changed of the structures by a hand
        self.hand_changes();
        // ---- 4. the step
        self.game.tick(&mut self.players);
        self.stats.steps += 1;
        // ---- 5. each body against what its game got
        for k in 0..self.peers.len() {
            if self.players[k].away {
                continue;
            }
            let body = self.players[k].pilot.summary();
            let slot = (s % RING as u64) as usize;
            let peer = &mut self.peers[k];
            peer.mine[slot] = (s + 1, body);
            let claim = peer.claims[slot];
            if claim.0 == s + 1 && !claim.1.near(&body) {
                self.correct(k, s);
            }
        }
        // ---- 6. what happened
        self.strikes();
        self.know();
        self.seen();
        self.said();
        // ---- 7. what goes out
        self.snapshots();
    }

    /// What every player is to be told of the last step: `send(to, reliable, bytes)`, which says
    /// whether it was taken. Events not taken (the player's connection is not through yet) are
    /// kept for the next time.
    pub fn send(&mut self, mut send: impl FnMut(u32, bool, &[u8]) -> bool) {
        let step = self.game.step;
        for peer in &mut self.peers {
            if peer.count > 0 {
                net::events_message(step, peer.count, &peer.events, &mut peer.sure);
                if send(peer.id, true, &peer.sure) {
                    self.stats.event_bytes += peer.sure.len() as u64;
                    peer.events.clear();
                    peer.count = 0;
                }
                peer.sure.clear();
            }
            if !peer.quick.is_empty() {
                send(peer.id, false, &peer.quick);
                peer.quick.clear();
            }
        }
    }

    /// Event `e` to everyone who knows structure `id` but player `but`.
    fn tell_knowing(&mut self, id: u64, e: &Event, but: Option<usize>) {
        self.encoded.clear();
        net::append_event(e, &mut self.encoded);
        for (k, peer) in self.peers.iter_mut().enumerate() {
            if Some(k) != but && peer.interest.knows(id) {
                peer.tell_bytes(&self.encoded);
            }
        }
    }

    /// Act `a` of player `k`, checked: done, or why not.
    fn act(&mut self, k: usize, a: Act) -> Result<(), &'static str> {
        let g = &mut self.game;
        let set = &g.builds.set;
        let p = &mut self.players[k];
        let eye = p.pilot.position;
        let near = |id: u64, extra: f64| set.get(id).is_some_and(|s| p.pilot.ride.is_some_and(|r| r.id == id) || s.to_world(s.center).distance(eye) <= f64::from(s.radius) + extra);
        match a {
            Act::Control { ship, control, value } => {
                if !near(ship, CONTROL_REACH) {
                    return Err(OUT_OF_REACH);
                }
                if controls::set(&mut g.ships, &g.builds.set, ship, usize::from(control), value) {
                    self.tell_knowing(ship, &Event::Control { ship, control, value }, Some(k));
                }
            }
            Act::Hand { ship, act } => {
                if !near(ship, CONTROL_REACH) {
                    return Err(OUT_OF_REACH);
                }
                controls::act(&mut g.ships, ship, act);
                self.tell_knowing(ship, &Event::Hand { ship, act }, Some(k));
            }
            Act::Sit { ship, seat } => {
                let others: Vec<(u64, usize)> = self.players.iter().enumerate().filter(|(j, _)| *j != k).filter_map(|(_, o)| o.pilot.seat.map(|s| (s.structure, s.index))).collect();
                let p = &mut self.players[k];
                seats::sit(&mut p.pilot, &g.ships, &g.builds.set, ship, usize::from(seat), |st, i| others.contains(&(st, i)))?;
            }
            Act::Stand => seats::stand(&mut p.pilot, &g.ships, &g.builds.set),
            Act::Grab => {
                let view = p.acting_view(set);
                let on = p.pilot.ride.map(|r| r.id);
                p.hands.grab(set, &g.ships, &view, on)?;
            }
            Act::Release => p.hands.release(&mut g.builds),
            Act::Wheel(n) => {
                p.hands.wheel(f64::from(n));
            }
            Act::Launch(mut l) => {
                let peer = &mut self.peers[k];
                if peer.launches < 1.0 {
                    return Err(TOO_FAST);
                }
                // (from the hand, going as the body goes, at the speed its kind has)
                if l.from.distance(eye) > HAND_REACH {
                    return Err(OUT_OF_REACH);
                }
                let motion = p.pilot.motion_in(set);
                if l.vel.distance(motion.vel) > LAUNCH_SLIP {
                    l.vel = motion.vel;
                }
                if l.by.is_some_and(|b| set.get(b).is_none()) {
                    l.by = None;
                }
                peer.launches -= 1.0;
                let bodies = g.bodies.clone();
                if !g.blasts.launch(l, &bodies, &mut g.builds) {
                    return Err(TOO_FAST);
                }
            }
            Act::Mend { structure, part, hp } => {
                let peer = &mut self.peers[k];
                let lib = g.builds.set.lib.clone();
                let Some(i) = g.builds.set.index_of(structure) else { return Err(OUT_OF_REACH) };
                let s = &mut g.builds.set.list[i];
                let Some(pt) = s.parts.get(part as usize) else { return Err(OUT_OF_REACH) };
                if s.to_world(pt.local.translation.into()).distance(eye) > HAND_REACH + f64::from(pt.shape.sphere().1) {
                    return Err(OUT_OF_REACH);
                }
                // (no more than a welder's best mends)
                let most = pt.max_hp * peer.mend;
                let hp = hp.clamp(0.0, most);
                peer.mend -= if pt.max_hp > 0.0 { hp / pt.max_hp } else { 0.0 };
                s.mend(&lib.catalog, part as usize, hp);
                if let Some(n) = g.ships.by_structure(structure) {
                    g.ships.list[n].touch();
                }
            }
            Act::Rebuild { structure, part } => {
                let lib = g.builds.set.lib.clone();
                let Some(i) = g.builds.set.index_of(structure) else { return Err(OUT_OF_REACH) };
                let s = &mut g.builds.set.list[i];
                let Some(pt) = s.parts.get(part as usize) else { return Err(OUT_OF_REACH) };
                if s.to_world(pt.local.translation.into()).distance(eye) > HAND_REACH + f64::from(pt.shape.sphere().1) {
                    return Err(OUT_OF_REACH);
                }
                s.rebuild(&lib.catalog, part as usize, REBUILT);
            }
            Act::Spawn { kind, pos, rot } => {
                if !self.config.cheats {
                    return Err(NOT_ALLOWED);
                }
                g.ships.spawn_free(&mut g.builds, &kind, pos, rot).map_err(|_| NOT_ALLOWED)?;
            }
            Act::Resync { id } => {
                // (told anew: as if it had just come to be known)
                let peer = &mut self.peers[k];
                if peer.interest.knows(id) {
                    peer.came.push(id);
                }
            }
        }
        Ok(())
    }

    /// What a hand changed of the structures since the last step (mended, put back), told to
    /// whoever knows each; what strikes did is not told so (every game does the strike itself).
    fn hand_changes(&mut self) {
        let set = &self.game.builds.set;
        // (kept in the order of the ids, as the structures are)
        self.shadows.retain(|sh| set.index_of(sh.0).is_some());
        for s in &set.list {
            let i = match self.shadows.binary_search_by_key(&s.id, |sh| sh.0) {
                Ok(i) => i,
                Err(i) => {
                    self.shadows.insert(i, (s.id, s.version, Shadow::of(s)));
                    continue;
                }
            };
            if self.shadows[i].1 == s.version {
                continue;
            }
            self.shadows[i].1 = s.version;
            let mut delta = Vec::new();
            if self.shadows[i].2.delta(s, &mut delta) {
                let e = Event::State { id: s.id, delta };
                self.encoded.clear();
                net::append_event(&e, &mut self.encoded);
                for peer in &mut self.peers {
                    if peer.interest.knows(s.id) {
                        peer.tell_bytes(&self.encoded);
                    }
                }
            }
        }
    }

    /// The strikes of the last step, to whoever knows what each struck; what they did taken as told.
    fn strikes(&mut self) {
        let strikes = std::mem::take(&mut self.game.out.strikes);
        for (strike, seed, pose) in &strikes {
            let id = match strike {
                Strike::Hit { id, .. } | Strike::Blow { id, .. } => *id,
            };
            let mut bytes = vec![0u8; 256 + pose.len() * 32];
            let mut w = Writer::new(&mut bytes);
            let poses: [(Named, &[glam::Affine3A]); 1] = [(Named::Built(id), pose)];
            told::write_strikes(&mut w, *seed, &[(Named::Built(id), *strike)], if pose.is_empty() { &[] } else { &poses });
            let n = w.finish().unwrap_or(0);
            bytes.truncate(n);
            self.tell_knowing(id, &Event::Strikes(bytes), None);
        }
        // (what the strikes did is what every game does: not told again as changes)
        let set = &self.game.builds.set;
        for (strike, ..) in &strikes {
            let id = match strike {
                Strike::Hit { id, .. } | Strike::Blow { id, .. } => *id,
            };
            if let (Ok(i), Some(s)) = (self.shadows.binary_search_by_key(&id, |sh| sh.0), set.get(id)) {
                self.shadows[i].1 = s.version;
                self.shadows[i].2.take(s);
            }
        }
        self.game.out.strikes = strikes;
    }

    /// Who knows what now: each player's interest (spread over the threads), and what came and
    /// went told (what came, all of it: `Event::Made`).
    fn know(&mut self) {
        let set = &self.game.builds.set;
        let rule = self.config.rule;
        let dt = STEP as f32;
        self.pinned.resize_with(self.peers.len(), Vec::new);
        for (k, p) in self.players.iter().enumerate() {
            let pins = &mut self.pinned[k];
            // (what comes to carry them is told all over again: while they were not on it, each
            // game ran its systems at its own pace)
            let peer = &mut self.peers[k];
            for id in p.pilot.ride.map(|r| r.id).into_iter().chain(p.pilot.seat.map(|s| s.structure)) {
                if !pins.contains(&id) && peer.interest.knows(id) {
                    peer.came.push(id);
                }
            }
            pins.clear();
            pins.extend(p.pilot.ride.map(|r| r.id));
            pins.extend(p.pilot.seat.map(|s| s.structure));
            pins.extend(peer.beside);
        }
        let players = &self.players;
        let pinned = &self.pinned;
        let scenario_end = self.game.builds.scenario_end;
        self.peers.par_iter_mut().enumerate().with_min_len(2).for_each(|(k, peer)| {
            let p = &players[k].pilot;
            peer.went.clear();
            let from = peer.came.len();
            peer.interest.update(&rule, set, p.position, p.velocity_in(set), &pinned[k], dt, &mut peer.came, &mut peer.went);
            peer.interest.owe(set, p.position, &pinned[k], dt);
            // (what their game has as the server does, from the start, is not told: what the
            // scenario set, anchored, untouched)
            let mut i = from;
            while i < peer.came.len() {
                let id = peer.came[i];
                let untouched = id < scenario_end && set.get(id).is_some_and(|s| s.anchored && s.version == 0);
                if untouched {
                    peer.came.swap_remove(i);
                } else {
                    i += 1;
                }
            }
        });
        // (each made once, whoever comes to know it)
        self.made.clear();
        for k in 0..self.peers.len() {
            let came = std::mem::take(&mut self.peers[k].came);
            for &id in &came {
                let at = match self.made.iter().position(|m| m.0 == id) {
                    Some(at) => at,
                    None => match made(&self.game, id) {
                        Some(e) => {
                            let mut b = Vec::new();
                            net::append_event(&e, &mut b);
                            self.made.push((id, b));
                            self.made.len() - 1
                        }
                        None => continue,
                    },
                };
                let peer = &mut self.peers[k];
                peer.events.extend_from_slice(&self.made[at].1);
                peer.count += 1;
            }
            let mut came = came;
            came.clear();
            self.peers[k].came = came;
            let went = std::mem::take(&mut self.peers[k].went);
            for &id in &went {
                self.peers[k].tell(&Event::Gone { id });
            }
            self.peers[k].went = went;
        }
    }

    /// What was let fly and where it ended this step, to whoever knows what it is told beside (or
    /// is near it): beside what let it fly or what it struck, so that it is where it is in every
    /// game however fast that goes.
    fn seen(&mut self) {
        if self.game.blasts.seen.is_empty() {
            return;
        }
        let set = &self.game.builds.set;
        self.seen.clear();
        for s in self.game.blasts.seen.drain(..) {
            let beside = match s {
                Seen::Launch { launch, .. } => launch.by,
                Seen::End { on, .. } => on,
                Seen::Track { .. } => None,
            };
            let from = beside.and_then(|id| set.get(id)).map_or(told::From::World, |st| told::From::Beside { named: Named::Built(st.id), frame: told::Frame { pos: st.pos, rot: st.rot, vel: st.vel, spin: st.spin } });
            self.seen.push((from, s));
        }
        let stamp = self.game.time();
        let reach = self.config.rule.most;
        for chunk in self.seen.chunks(told::SEEN_EACH) {
            let mut bytes = vec![0u8; 64 + chunk.len() * 96];
            let mut w = Writer::new(&mut bytes);
            told::write_seen(&mut w, stamp, chunk, |id| Some(Named::Built(id)));
            let n = w.finish().unwrap_or(0);
            bytes.truncate(n);
            self.encoded.clear();
            net::append_event(&Event::Seen(bytes), &mut self.encoded);
            // (to whoever is near enough to see any of it)
            let at = |s: &Seen| match s {
                Seen::Launch { launch, .. } => launch.from,
                Seen::End { at, .. } => *at,
                Seen::Track { pos, .. } => *pos,
            };
            for (k, peer) in self.peers.iter_mut().enumerate() {
                let eye = self.players[k].pilot.position;
                if chunk.iter().any(|(_, s)| at(s).distance(eye) < reach) {
                    peer.tell_bytes(&self.encoded);
                }
            }
        }
    }

    /// What the ships said, to whoever rides each.
    fn said(&mut self) {
        let said = std::mem::take(&mut self.game.out.said);
        for (ship, about, text, level) in &said {
            for (k, peer) in self.peers.iter_mut().enumerate() {
                if self.players[k].pilot.ride.is_some_and(|r| r.id == *ship) {
                    peer.tell(&Event::Said { ship: *ship, about: about.clone(), text: text.clone(), level: *level });
                }
            }
        }
        self.game.out.said = said;
    }

    /// The snapshots of this step, to the players whose turn it is (staggered over the steps),
    /// made at once on every thread.
    fn snapshots(&mut self) {
        // (every player as the others draw them)
        self.states.clear();
        let set = &self.game.builds.set;
        for (k, peer) in self.peers.iter_mut().enumerate() {
            let p = &self.players[k];
            peer.beside = match p.pilot.ride {
                Some(_) => None,
                None => follow::pick(p.pilot.position, peer.beside, self.game.ships.list.iter().filter_map(|sh| set.get(sh.structure).map(|s| (s.id, s.pos)))),
            };
            self.states.push((peer.id, state_of(p, &peer.last, peer.beside, &self.game)));
        }
        // (one ship's digest in this step's snapshots, in turns)
        let ships = &self.game.ships.list;
        let check = (!ships.is_empty()).then(|| {
            self.check_turn = (self.check_turn + 1) % ships.len();
            let sh = &ships[self.check_turn];
            set.get(sh.structure).map(|s| (s.id, Digest::of(Some(sh), s)))
        });
        let check = check.flatten();
        let (every, room, rule, step) = (self.config.snap_every.max(1), self.config.snap_room, self.config.rule, self.game.step);
        let states = &self.states;
        let players = &self.players;
        let bodies = &*self.game.bodies;
        let reach = rule.most;
        let bytes: u64 = self
            .peers
            .par_iter_mut()
            .enumerate()
            .with_min_len(2)
            .map(|(k, peer)| {
                if (step + k as u64) % every != 0 {
                    return 0;
                }
                let me = &players[k].pilot;
                let snap = &mut peer.snap;
                snap.step = step;
                snap.took = peer.heard.map_or(0, |n| n + 1);
                snap.ahead = peer.lead.clamp(-1000, 1000) as i32;
                peer.lead = i64::MAX;
                snap.players.clear();
                snap.players.extend(states.iter().filter(|(id, st)| *id != peer.id && st.pos.distance(me.position) < reach).cloned());
                snap.check = check.filter(|c| peer.interest.knows(c.0));
                // (the things that move, as many as fit, the most owed first)
                peer.interest.take(set, &rule, &mut peer.order);
                let mut things = std::mem::take(&mut snap.things);
                while things.len() < peer.order.len() {
                    things.push(RigidState::default());
                }
                for (slot, &i) in peer.order.iter().enumerate() {
                    let Some(s) = set.get(peer.interest.known[i].id) else { continue };
                    let t = &mut things[slot];
                    // (as it is at the world's moment, if it is stepped now and then; at rest, still:
                    // it is to rest where it is in every game)
                    let (pos, vel, rot) = coasted(s, bodies.field(s.pos).pull, set.now - s.clock);
                    let (vel, spin) = if s.resting { (glam::Vec3::ZERO, glam::Vec3::ZERO) } else { (vel.as_vec3(), s.spin) };
                    (t.id, t.body, t.frame, t.pos, t.rot, t.vel, t.spin, t.resting) = (s.id, me.body as u8, Frame::World, pos, rot, vel, spin, s.resting);
                    t.joints.clear();
                }
                things.truncate(peer.order.len());
                snap.things = things;
                let sent = net::write_snap(snap, room, &mut peer.quick);
                for slot in 0..sent {
                    let i = peer.order[slot];
                    peer.interest.told(i);
                }
                peer.quick.len() as u64
            })
            .sum();
        self.stats.snap_bytes += bytes;
    }
}

/// The share of its hit points a part put back has (a welder's own).
const REBUILT: f32 = 0.2;

/// What player `p`'s command `c` asks of the step: the keys, the look, the switches of the suit
/// (free flight only with `cheats`). The same in the server and in the player's own game.
pub(crate) fn apply(c: &Cmd, p: &mut Player, cheats: bool) {
    p.input = c.input;
    (p.pilot.yaw, p.pilot.pitch) = (c.yaw, c.pitch);
    (p.pilot.pack_on, p.pilot.steady, p.pilot.lamps) = (c.pack, c.steady, c.lamps);
    if cheats && c.fly != p.pilot.flying {
        p.pilot.toggle_flight();
    }
    p.aim = c.aim.map(|dir| {
        let v = p.pilot.view();
        View { forward: dir.as_dvec3(), ..v }
    });
}

/// Player `p` as the others draw them, with what their command says of what no step makes.
fn state_of(p: &Player, c: &Cmd, beside: Option<u64>, g: &Game) -> PlayerState {
    let pilot = &p.pilot;
    let set = &g.builds.set;
    let mut flags = 0;
    for (on, bit) in [(pilot.grounded, flag::GROUNDED), (pilot.crouched(), flag::CROUCHED), (pilot.pack_on, flag::PACK), (pilot.pack_thrust() > 0.0, flag::THRUSTING), (pilot.lamps, flag::LAMP), (c.trigger, flag::TRIGGER), (c.outside, flag::THIRD_PERSON)]
    {
        if on {
            flags |= bit;
        }
    }
    let seat = pilot.seat.map(|s| (s.structure, s.index as u8));
    if let Some(s) = pilot.seat
        && g.ships.by_structure(s.structure).is_some_and(|n| g.ships.list[n].kind.seats.get(s.index).is_some_and(|x| !x.def.mandos.is_empty()))
    {
        flags |= flag::AT_CONTROLS;
    }
    let mut ride = pilot.ride.map(|r| (r.id, r.local));
    if let (None, Some(id)) = (ride, beside)
        && let Some(s) = set.get(id)
    {
        ride = Some((id, s.to_local(pilot.position)));
        flags |= flag::BESIDE;
    }
    PlayerState {
        body: pilot.body as u8,
        pos: pilot.position,
        ride: ride.map(|r| r.0),
        local: ride.map_or(Vec3::ZERO, |r| r.1),
        yaw: if pilot.seat.is_some() { pilot.yaw } else { pilot.told_heading() } as f32,
        pitch: pilot.pitch as f32,
        head: c.head,
        vel: pilot.velocity().as_vec3(),
        flags,
        seat,
        tool: c.tool,
        eye_h: pilot.eye_over_feet() as f32,
        push: pilot.push.as_vec3(),
        work: None,
        gesture: c.gesture,
    }
}

/// All of structure `id` as a player's game that does not have it needs it (`Event::Made`).
pub fn made(g: &Game, id: u64) -> Option<Event> {
    let set = &g.builds.set;
    let s = set.get(id)?;
    let mut make = Vec::new();
    sync::write_make(s, &set.lib, &mut make).ok()?;
    let mut state = Vec::new();
    sync::write_state(s, &mut state);
    let (ship, seed, systems) = match g.ships.by_structure(id) {
        Some(n) => {
            let sh = &g.ships.list[n];
            let mut systems = Vec::new();
            sync::write_ship(sh, &|x| Some(x), &mut systems);
            (sh.kind.id.clone(), sh.seed, systems)
        }
        None => (String::new(), 0, Vec::new()),
    };
    let (pos, vel, rot) = coasted(s, g.bodies.field(s.pos).pull, set.now - s.clock);
    // (at rest, still: it is to rest where it is in every game)
    let (vel, spin) = if s.resting { (Vec3::ZERO, Vec3::ZERO) } else { (vel.as_vec3(), s.spin) };
    Some(Event::Made { id, lineage: s.lineage, born: s.born, ship, seed, pos, rot, vel, spin, resting: s.resting, make, state, systems })
}
